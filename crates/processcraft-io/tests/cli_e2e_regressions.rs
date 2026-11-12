//! Regressions from the CLI end-to-end run: every case is built through the command
//! pipeline (as `processcraft-cli run` does), exported to BPMN and re-imported.
use processcraft_core::{
    bpmn_model::{LoopCharacteristics, NodeDetails},
    commands::{Envelope, Request, Session},
    model::{BpmnDocument, FlowKind, ProjectData},
};
use processcraft_io::interchange::{export_bpmn, prepare_import};
use serde_json::{Value, json};

const DOC: &str = "22222222-2222-4222-8222-222222222222";

fn element(id: &str, kind: &str, details: Value) -> Value {
    let mut element = json!({
        "id": id, "name": id, "documentation": "", "kind": kind, "owner": "", "contact": ""
    });
    if !details.is_null() {
        element["details"] = details;
    }
    element
}
fn create_element(process: &str, element: Value) -> Value {
    json!({"type": "createElement", "document_id": DOC, "process_id": process, "element": element})
}
fn create_process(id: &str) -> Value {
    json!({"type": "createProcess", "document_id": DOC, "process": {
        "id": id, "name": id, "documentation": "", "nodes": [], "lanes": [], "flows": []}})
}
fn sequence_flow(source: &str, target: &str, condition: Value) -> Value {
    let mut flow = json!({"id": "f", "kind": "Sequence", "documentation": "",
        "source": source, "target": target, "name": null});
    if !condition.is_null() {
        flow["condition"] = condition;
    }
    json!({"type": "createFlow", "document_id": DOC, "process_id": "p", "flow": flow})
}
fn session_with(mutations: Vec<Value>) -> Result<Session, String> {
    let mut all = vec![json!({"type": "createDocument", "document": {
        "id": DOC, "name": "T", "folder": null, "definitions_id": "defs", "processes": [],
        "collaboration": null, "diagrams": [], "provenance": null, "capability": "Editable"}})];
    all.extend(mutations);
    let request: Request =
        serde_json::from_value(json!({"type": "batch", "mutations": all})).unwrap();
    let mut session = Session::new(ProjectData::new("T".into())).unwrap();
    let envelope = Envelope {
        request_id: "r".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session
        .apply(envelope, request)
        .map_err(|error| format!("{error:?}"))?;
    Ok(session)
}
/// Apply the commands, export, re-import and return the re-imported document.
fn round_trip(mutations: Vec<Value>) -> BpmnDocument {
    let session = session_with(mutations).expect("commands accepted");
    let project = session.snapshot().project;
    let exported = export_bpmn(&project, DOC.parse().unwrap(), false)
        .unwrap_or_else(|failure| panic!("export blocked: {:?}", failure.report.blocked));
    let again = prepare_import(&exported.bytes, "again").unwrap();
    assert!(again.report.editable);
    again.document
}
fn node<'a>(document: &'a BpmnDocument, id: &str) -> &'a processcraft_core::model::Node {
    document
        .processes
        .iter()
        .flat_map(|p| &p.nodes)
        .find(|n| n.id.to_string() == id)
        .unwrap_or_else(|| panic!("node {id}"))
}

#[test]
fn message_flows_connect_throwing_events_to_catching_events() {
    let message = json!({"type": "setEventResources", "document_id": DOC,
        "resources": [{"type": "message", "id": "m", "name": "m"}]});
    let definitions =
        json!({"type": "event", "definitions": [{"type": "message", "message_ref": "m"}]});
    let build = |source: (&str, &str), target: (&str, &str)| {
        vec![
            create_process("p1"),
            create_process("p2"),
            message.clone(),
            json!({"type": "createCollaboration", "document_id": DOC, "collaboration": {
                "id": "c", "flows": [],
                "participants": [{"id": "a", "process": "p1"}, {"id": "b", "process": "p2"}]}}),
            create_element("p1", element(source.0, source.1, definitions.clone())),
            create_element("p2", element(target.0, target.1, definitions.clone())),
            json!({"type": "createFlow", "document_id": DOC, "process_id": null, "flow": {
                "id": "mf", "kind": "Message", "documentation": "", "name": null,
                "source": source.0, "target": target.0, "message_ref": "m"}}),
        ]
    };
    for (source, target) in [
        (
            ("thr", "IntermediateThrowEvent"),
            ("cat", "IntermediateCatchEvent"),
        ),
        (("end", "EndEvent"), ("start", "StartEvent")),
    ] {
        let again = round_trip(build(source, target));
        let flow = &again.collaboration.as_ref().unwrap().flows[0];
        assert_eq!(flow.kind, FlowKind::Message);
        assert!(format!("{:?}", flow.source).contains(source.0));
        assert!(format!("{:?}", flow.target).contains(target.0));
    }
    // Wrong direction or non-message event kinds stay rejected.
    for (source, target) in [
        (
            ("cat", "IntermediateCatchEvent"),
            ("cat2", "IntermediateCatchEvent"),
        ),
        (
            ("thr", "IntermediateThrowEvent"),
            ("thr2", "IntermediateThrowEvent"),
        ),
        (("start", "StartEvent"), ("cat2", "IntermediateCatchEvent")),
        (("thr", "IntermediateThrowEvent"), ("end2", "EndEvent")),
    ] {
        let error = session_with(build(source, target)).err().expect("rejected");
        assert!(error.contains("invalid message flow endpoints"), "{error}");
    }
}

#[test]
fn timer_boundary_events_export_for_every_timer_kind() {
    for (kind, expression) in [
        ("Duration", "PT1H"),
        ("Date", "2030-01-01T00:00:00Z"),
        ("Cycle", "R3/PT10M"),
    ] {
        let again = round_trip(vec![
            create_process("p"),
            create_element("p", element("t", "Task", Value::Null)),
            create_element(
                "p",
                element(
                    "b",
                    "BoundaryEvent",
                    json!({"type": "event", "attached_to": "t", "cancel_activity": true,
                        "definitions": [{"type": "timer", "kind": kind, "expression": expression}]}),
                ),
            ),
        ]);
        let Some(NodeDetails::Event { definitions, .. }) = &node(&again, "b").details else {
            panic!("boundary details")
        };
        let rendered = format!("{definitions:?}");
        assert!(
            rendered.contains(expression) && rendered.contains(kind),
            "{rendered}"
        );
    }
}

#[test]
fn nested_lanes_created_by_commands_export_and_keep_their_parent() {
    let lane = |id: &str, parent: Value| {
        json!({"type": "createLane", "document_id": DOC, "process_id": "p",
            "lane": {"id": id, "name": id, "members": [], "parent": parent}})
    };
    let again = round_trip(vec![
        create_process("p"),
        lane("l1", Value::Null),
        lane("l2", json!("l1")),
        lane("l3", json!("l1")),
    ]);
    let lanes = &again.processes[0].lanes;
    assert_eq!(lanes.len(), 3);
    let parent_of = |id: &str| {
        lanes
            .iter()
            .find(|l| l.id.to_string() == id)
            .and_then(|l| l.parent.as_ref().map(ToString::to_string))
    };
    assert_eq!(parent_of("l1"), None);
    assert_eq!(parent_of("l2").as_deref(), Some("l1"));
    assert_eq!(parent_of("l3").as_deref(), Some("l1"));
}

#[test]
fn plain_string_expressions_export_and_reimport_with_their_text() {
    let mut task = element("t", "Task", Value::Null);
    task["loop_characteristics"] = json!({"type": "multiInstance", "is_sequential": null,
        "cardinality": "3", "completion_condition": "done"});
    let mut looped = element("l", "Task", Value::Null);
    looped["loop_characteristics"] =
        json!({"type": "standard", "test_before": null, "condition": "x<3", "maximum": null});
    let again = round_trip(vec![
        create_process("p"),
        create_element("p", element("g", "ExclusiveGateway", Value::Null)),
        create_element("p", task),
        create_element("p", looped),
        sequence_flow("g", "t", json!("x > 1")),
    ]);
    let flow = &again.processes[0].flows[0];
    assert_eq!(
        flow.condition.as_ref().and_then(|c| c.plain_text()),
        Some("x > 1")
    );
    let Some(LoopCharacteristics::MultiInstance {
        cardinality,
        completion_condition,
        ..
    }) = &node(&again, "t").loop_characteristics
    else {
        panic!("multi instance")
    };
    assert_eq!(cardinality.as_ref().and_then(|c| c.plain_text()), Some("3"));
    assert_eq!(
        completion_condition.as_ref().and_then(|c| c.plain_text()),
        Some("done")
    );
    let Some(LoopCharacteristics::Standard { condition, .. }) =
        &node(&again, "l").loop_characteristics
    else {
        panic!("standard loop")
    };
    assert_eq!(condition.as_ref().and_then(|c| c.plain_text()), Some("x<3"));
}

#[test]
fn text_annotation_created_before_flow_nodes_exports() {
    let mut annotation = element(
        "n",
        "TextAnnotation",
        json!({"type": "textAnnotation", "text": "hi", "text_format": "text/plain"}),
    );
    annotation["name"] = Value::Null;
    let again = round_trip(vec![
        create_process("p"),
        create_element("p", annotation),
        create_element("p", element("t", "Task", Value::Null)),
    ]);
    assert!(matches!(
        node(&again, "n").details,
        Some(NodeDetails::TextAnnotation { .. })
    ));
    node(&again, "t");
}

#[test]
fn vacuous_event_and_gateway_details_export_as_absent_details() {
    let again = round_trip(vec![
        create_process("p"),
        create_element(
            "p",
            element(
                "s",
                "StartEvent",
                json!({"type": "event", "definitions": []}),
            ),
        ),
        create_element(
            "p",
            element("g", "ParallelGateway", json!({"type": "gateway"})),
        ),
    ]);
    assert!(node(&again, "s").details.is_none());
    assert!(node(&again, "g").details.is_none());
}

#[test]
fn local_data_store_selector_exports_as_a_target_namespace_reference() {
    let again = round_trip(vec![
        create_process("p"),
        json!({"type": "createDataStore", "document_id": DOC, "data_store": {"id": "ds", "name": "S"}}),
        create_element(
            "p",
            element(
                "r",
                "DataStoreReference",
                json!({"type": "dataStoreReference", "data_store_ref": "ds"}),
            ),
        ),
    ]);
    let Some(NodeDetails::DataStoreReference {
        data_store_ref: Some(reference),
    }) = &node(&again, "r").details
    else {
        panic!("data store reference")
    };
    assert!(format!("{reference:?}").contains("\"ds\""), "{reference:?}");
}

#[test]
fn call_activity_to_missing_local_callable_is_rejected_by_commands() {
    let call = element(
        "c",
        "CallActivity",
        json!({"type": "callActivity", "called_element":
            {"namespace": "urn:processcraft:generated", "local": "nothing"}}),
    );
    let error = session_with(vec![create_process("p"), create_element("p", call)])
        .err()
        .expect("rejected");
    assert!(
        error.contains("unresolved local activity reference"),
        "{error}"
    );
}

#[test]
fn plain_string_condition_with_a_colon_exports_and_reimports_equal() {
    for text in ["status:approved", "a:b", "${amount > 1000}"] {
        let again = round_trip(vec![
            create_process("p"),
            create_element("p", element("g", "ExclusiveGateway", Value::Null)),
            create_element("p", element("t", "Task", Value::Null)),
            sequence_flow("g", "t", json!(text)),
        ]);
        assert_eq!(
            again.processes[0].flows[0]
                .condition
                .as_ref()
                .and_then(|c| c.plain_text()),
            Some(text)
        );
    }
}

#[test]
fn data_store_reference_to_missing_local_store_is_rejected_atomically() {
    let reference = element(
        "r",
        "DataStoreReference",
        json!({"type": "dataStoreReference", "data_store_ref": "nothing"}),
    );
    let error = session_with(vec![create_process("p"), create_element("p", reference)])
        .err()
        .expect("rejected");
    assert!(
        error.contains("unresolved local activity reference"),
        "{error}"
    );
}
