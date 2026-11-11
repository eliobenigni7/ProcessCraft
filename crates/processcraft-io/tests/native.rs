use processcraft_core::{
    ids::XmlId,
    model::{ProjectData, Resource},
};
use processcraft_io::native::{load_bytes, write_bytes};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn project() -> ProjectData {
    let mut p = ProjectData::new("Native test".into());
    p.resources.push(Resource::new(
        "asset".parse::<XmlId>().unwrap(),
        "text/plain".into(),
        Arc::from(&b"hello world"[..]),
    ));
    p
}

#[test]
fn immutable_pre_identity_migration_archive_preserves_identity() {
    use sha2::{Digest, Sha256};
    let bytes =
        include_bytes!("../../../tests/fixtures/native/identity-baseline-2f0f770.processcraft");
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&Sha256::digest(bytes)),
        "ad1881afb88c8b077f16b095bbd57f0fcfc338b36f06cc80f49e3b30c0e6968b"
    );
    let loaded = load_bytes(bytes).unwrap();
    let expected = "58b43fc92f74014e1683d0f5b91c0a8493a5bf9b5512179a80224f65f712ab97";
    assert_eq!(hex(&loaded.source_content_identity), expected);
    assert_eq!(hex(&loaded.project.content_identity().unwrap()), expected);
    assert_eq!(
        loaded.project.checkpoints[0].content.name,
        "Synthetic compatibility baseline"
    );
    let doc = &loaded.project.documents[0];
    assert!(processcraft_core::queries::find_node(doc, &"task".parse().unwrap()).is_some());
    assert_eq!(hex(&loaded.project.content_identity().unwrap()), expected);
    let saved = write_bytes(&loaded.project).unwrap();
    assert_eq!(
        hex(&load_bytes(&saved)
            .unwrap()
            .project
            .content_identity()
            .unwrap()),
        expected
    );
}
fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let n = f.name().to_owned();
            let mut b = vec![];
            f.read_to_end(&mut b).unwrap();
            (n, b)
        })
        .collect()
}
fn archive(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for (n, b) in entries {
        w.start_file(n, SimpleFileOptions::default()).unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}
fn edit_json(
    entries: &mut [(String, Vec<u8>)],
    name: &str,
    f: impl FnOnce(&mut serde_json::Value),
) {
    let b = &mut entries.iter_mut().find(|(n, _)| n == name).unwrap().1;
    let mut v = serde_json::from_slice(b).unwrap();
    f(&mut v);
    *b = serde_json::to_vec(&v).unwrap();
}
#[test]
fn native_round_trip_preserves_content() {
    let p = project();
    let bytes = write_bytes(&p).unwrap();
    let loaded = load_bytes(&bytes).unwrap();
    assert_eq!(
        p.content_identity().unwrap(),
        loaded.project.content_identity().unwrap()
    );
    assert!(!loaded.requires_save_as_or_backup);
    let list = entries(&bytes);
    let json = &list.iter().find(|(n, _)| n == "project.json").unwrap().1;
    assert!(!String::from_utf8_lossy(json).contains("bytes"));
}

#[test]
fn expanded_data_capability_covers_live_and_checkpoint_documents() {
    use processcraft_core::{
        bpmn_data::{DataState, DataStore, ItemAwareMetadata},
        bpmn_model::QNameRef,
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, FrozenProject},
    };
    let mut p = project();
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.data_stores.push(DataStore {
        id: Some("store".parse().unwrap()),
        name: Some("Store".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: None,
    });
    p.documents.push(Arc::new(doc));
    let legacy_identity = p.content_identity().unwrap();
    let legacy = write_bytes(&p).unwrap();
    let legacy_entries = entries(&legacy);
    let manifest: serde_json::Value = serde_json::from_slice(
        &legacy_entries
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        manifest["required_capabilities"],
        serde_json::json!(["comments", "checkpoints"])
    );
    assert_eq!(
        load_bytes(&legacy)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        legacy_identity
    );
    Arc::make_mut(&mut p.documents[0]).data_stores[0].item_aware = Some(ItemAwareMetadata {
        item_subject_ref: Some(QNameRef {
            namespace: "urn:external".into(),
            local: "Item".parse().unwrap(),
        }),
        data_state: Some(DataState {
            id: Some("state".parse().unwrap()),
            name: Some("Ready".into()),
        }),
    });
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-expanded-data")
    );
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].data_stores,
        p.documents[0].data_stores
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded data requires capability")
    );
    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    Arc::make_mut(&mut p.documents[0]).data_stores[0].item_aware = None;
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .data_stores,
        p.checkpoints[0].content.documents[0].data_stores
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded data requires capability")
    );
    p.checkpoints.clear();
    Arc::make_mut(&mut p.documents[0]).data_stores[0].id = None;
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].data_stores[0].id,
        None
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded data requires capability")
    );
    let store = &mut Arc::make_mut(&mut p.documents[0]).data_stores[0];
    store.id = Some("store".parse().unwrap());
    store.name = None;
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].data_stores[0].name,
        None
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded data requires capability")
    );
}

#[test]
fn historical_unresolved_activity_bindings_round_trip_without_new_capabilities() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_model::QNameRef,
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, FrozenProject, Node, NodeKind, Process},
    };
    let mut p = project();
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    let binding = |local: &str| {
        Some(QNameRef {
            namespace: "urn:old namespace".into(),
            local: local.parse().unwrap(),
        })
    };
    doc.processes.push(Process {
        is_executable: Some(false),
        io_specification: None,
        id: "process".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new(),
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            item_aware: None,
            id: "task".parse().unwrap(),
            name: Some(String::new()),
            documentation: String::new(),
            kind: NodeKind::SendTask,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: Some(ActivityProperties {
                message_ref: binding("missingMessage"),
                operation_ref: binding("missingOperation"),
                ..Default::default()
            }),
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    p.documents.push(Arc::new(doc));
    let identity = p.content_identity().unwrap();
    let bytes = write_bytes(&p).unwrap();
    let archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        manifest["required_capabilities"],
        serde_json::json!(["comments", "checkpoints"])
    );
    let raw: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "project.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(raw["documents"][0].get("catalog").is_none());
    let loaded = load_bytes(&bytes).unwrap().project;
    assert_eq!(loaded.content_identity().unwrap(), identity);
    assert_eq!(
        loaded.documents[0].processes[0].nodes[0].activity,
        p.documents[0].processes[0].nodes[0].activity
    );
    let activity = Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0]
        .activity
        .as_mut()
        .unwrap();
    activity.message_ref.as_mut().unwrap().namespace.clear();
    activity.operation_ref.as_mut().unwrap().namespace.clear();
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-empty-namespace-activity-bindings")
    );
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].processes[0].nodes[0].activity,
        p.documents[0].processes[0].nodes[0].activity
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("empty namespace activity bindings require capability")
    );
    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    let activity = Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0]
        .activity
        .as_mut()
        .unwrap();
    activity.message_ref.as_mut().unwrap().namespace = "urn:old namespace".into();
    activity.operation_ref.as_mut().unwrap().namespace = "urn:old namespace".into();
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .processes[0]
            .nodes[0]
            .activity,
        p.checkpoints[0].content.documents[0].processes[0].nodes[0].activity
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("empty namespace activity bindings require capability")
    );
}

#[test]
fn supporting_catalog_capability_covers_live_and_checkpoint_documents() {
    use processcraft_core::{
        bpmn_catalog::{
            BpmnResource, Interface, ItemDefinition, ItemKind, Operation, ResourceParameter,
        },
        bpmn_model::QNameRef,
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, FrozenProject},
    };
    let reference = |local: &str| QNameRef {
        namespace: "urn:external".into(),
        local: local.parse().unwrap(),
    };
    let mut p = project();
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.catalog.item_definitions.push(ItemDefinition {
        id: Some("item".parse().unwrap()),
        structure_ref: Some(reference("Type")),
        is_collection: Some(true),
        item_kind: Some(ItemKind::Information),
    });
    doc.catalog.interfaces.push(Interface {
        id: Some("interface".parse().unwrap()),
        name: "Service".into(),
        implementation_ref: Some(reference("Service")),
        operations: vec![Operation {
            id: Some("operation".parse().unwrap()),
            name: "Execute".into(),
            implementation_ref: None,
            in_message_ref: reference("Input"),
            out_message_ref: Some(reference("Output")),
            error_refs: vec![reference("Error")],
        }],
    });
    doc.catalog.resources.push(BpmnResource {
        id: Some("resource".parse().unwrap()),
        name: "Team".into(),
        parameters: vec![ResourceParameter {
            id: Some("parameter".parse().unwrap()),
            name: Some("Role".into()),
            type_ref: Some(reference("Role")),
            is_required: Some(true),
        }],
    });
    p.documents.push(Arc::new(doc));
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-supporting-catalog")
    );
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].catalog,
        p.documents[0].catalog
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("supporting catalog requires capability")
    );
    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    Arc::make_mut(&mut p.documents[0]).catalog = Default::default();
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .catalog,
        p.checkpoints[0].content.documents[0].catalog
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("supporting catalog requires capability")
    );
}

#[test]
fn definitions_metadata_requires_capability_in_live_and_checkpoint_documents() {
    use processcraft_core::{
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, DefinitionsImport, DefinitionsMetadata, FrozenProject},
    };
    let mut p = project();
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:example".into(),
        name: Some("Definitions".into()),
        expression_language: Some("urn:expressions".into()),
        type_language: None,
        exporter: Some("ProcessCraft".into()),
        exporter_version: Some("1".into()),
        imports: vec![
            DefinitionsImport {
                namespace: "urn:first".into(),
                location: "first.bpmn".into(),
                import_type: "urn:bpmn".into(),
            },
            DefinitionsImport {
                namespace: "urn:second".into(),
                location: "second.xsd".into(),
                import_type: "urn:xsd".into(),
            },
        ],
    });
    p.documents.push(Arc::new(doc));
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-definitions-metadata")
    );
    let loaded = load_bytes(&bytes).unwrap().project;
    assert_eq!(loaded.documents[0].definitions, p.documents[0].definitions);
    assert_eq!(
        loaded.content_identity().unwrap(),
        p.content_identity().unwrap()
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("definitions metadata requires capability")
    );

    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    Arc::make_mut(&mut p.documents[0]).definitions = None;
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .definitions,
        p.checkpoints[0].content.documents[0].definitions
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("definitions metadata requires capability")
    );
}

#[test]
fn structured_expression_capability_covers_live_and_checkpoint_documents() {
    use processcraft_core::{
        bpmn_model::{Expression, ExpressionData, ExpressionKind},
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, Flow, FlowKind, FrozenProject, Node, NodeKind, Process},
    };
    let mut p = project();
    let node = |id: &str| Node {
        item_aware: None,
        id: id.parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new(),
        kind: NodeKind::Task,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.processes.push(Process {
        is_executable: Some(false),
        io_specification: None,
        id: "process".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new(),
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![node("a"), node("b")],
        lanes: vec![],
        flows: vec![Flow {
            id: "flow".parse().unwrap(),
            kind: FlowKind::Sequence,
            source: "a".parse().unwrap(),
            target: "b".parse().unwrap(),
            name: Some(String::new()),
            documentation: String::new(),
            association_direction: None,
            condition: Some(Expression::Legacy("x".into())),
        }],
    });
    p.documents.push(Arc::new(doc));
    let legacy = write_bytes(&p).unwrap();
    let legacy_entries = entries(&legacy);
    let manifest: serde_json::Value = serde_json::from_slice(
        &legacy_entries
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        !manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-structured-expressions")
    );
    let json: serde_json::Value = serde_json::from_slice(
        &legacy_entries
            .iter()
            .find(|(n, _)| n == "project.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        json["documents"][0]["processes"][0]["flows"][0]["condition"],
        "x"
    );
    let expression = Expression::Structured(ExpressionData {
        id: Some("expr".parse().unwrap()),
        kind: ExpressionKind::Base {
            explicit_type: true,
        },
        text: "x".into(),
        namespaces: vec![
            (
                "b".into(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            ),
            ("ex".into(), "urn:example".into()),
            (
                "xsi".into(),
                "http://www.w3.org/2001/XMLSchema-instance".into(),
            ),
        ],
    });
    Arc::make_mut(&mut p.documents[0]).processes[0].flows[0].condition = Some(expression.clone());
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-structured-expressions")
    );
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].processes[0].flows[0].condition,
        Some(expression)
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("structured expressions require capability")
    );
    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    Arc::make_mut(&mut p.documents[0]).processes[0].flows[0].condition =
        Some(Expression::Legacy("x".into()));
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .processes[0]
            .flows[0]
            .condition,
        p.checkpoints[0].content.documents[0].processes[0].flows[0].condition
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("structured expressions require capability")
    );
}

#[test]
fn compensation_event_capability_covers_live_and_checkpoint_documents() {
    use processcraft_core::{
        bpmn_model::{EventDefinition, NodeDetails},
        ids::CheckpointId,
        model::{BpmnDocument, Checkpoint, FrozenProject, Node, NodeKind, Process},
    };
    let mut p = project();
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    let details = NodeDetails::Event {
        definitions: vec![EventDefinition::Compensate {
            id: None,
            wait_for_completion: None,
            activity_ref: None,
        }],
        attached_to: None,
        cancel_activity: None,
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    };
    doc.processes.push(Process {
        is_executable: Some(false),
        io_specification: None,
        id: "process".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new(),
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            item_aware: None,
            id: "event".parse().unwrap(),
            name: Some(String::new()),
            documentation: String::new(),
            kind: NodeKind::IntermediateThrowEvent,
            details: Some(details.clone()),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    p.documents.push(Arc::new(doc));
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "bpmn-link-cancel-compensation")
    );
    assert_eq!(
        load_bytes(&bytes).unwrap().project.documents[0].processes[0].nodes[0].details,
        Some(details)
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("events require capability")
    );
    p.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "Before".into(),
        created_unix_ms: 0,
        content: FrozenProject::capture(&p),
    });
    Arc::make_mut(&mut p.documents[0]).processes[0]
        .nodes
        .clear();
    let bytes = write_bytes(&p).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes).unwrap().project.checkpoints[0]
            .content
            .documents[0]
            .processes[0]
            .nodes[0]
            .details,
        p.checkpoints[0].content.documents[0].processes[0].nodes[0].details
    );
    edit_json(&mut archived, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("events require capability")
    );
}
#[test]
fn unknown_required_capability_refuses_write() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["future"])
    });
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn resource_digest_mismatch_fails() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.iter_mut()
        .find(|(n, _)| n.starts_with("resources/"))
        .unwrap()
        .1
        .push(1);
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn writer_cannot_exceed_reader_limits() {
    let mut p = project();
    p.resources[0] = Resource::new(
        "asset".parse().unwrap(),
        "x".into(),
        Arc::from(vec![0; 32 * 1024 * 1024 + 1]),
    );
    assert!(write_bytes(&p).is_err());
}
#[test]
fn unsafe_archive_is_rejected() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.push(("../escape".into(), vec![]));
    assert!(load_bytes(&archive(&e)).is_err());
    e.pop();
    e.push(("mimetypX".into(), vec![]));
    let mut bytes = archive(&e);
    for i in 0..bytes.len() - 8 {
        if &bytes[i..i + 8] == b"mimetypX" {
            bytes[i + 7] = b'e';
        }
    }
    assert!(load_bytes(&bytes).is_err());
}
#[test]
fn malformed_and_duplicate_json_fail() {
    let original = entries(&write_bytes(&project()).unwrap());
    for replacement in [b"{\"id\":1,\"id\":2}".to_vec(), vec![0xff], b"{".to_vec()] {
        let mut e = original.clone();
        e.iter_mut().find(|(n, _)| n == "project.json").unwrap().1 = replacement;
        assert!(load_bytes(&archive(&e)).is_err());
    }
    let mut e = original.clone();
    e.retain(|(n, _)| n != "project.json");
    assert!(load_bytes(&archive(&e)).is_err());
    let bytes = archive(&original);
    assert!(load_bytes(&bytes[..bytes.len() / 2]).is_err());
}
#[test]
fn synthetic_v0_migrates_without_mutating_input() {
    let p = project();
    let mut e = entries(&write_bytes(&p).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!([]);
        v["schema"] = 0.into();
        v["min_reader"] = 0.into();
        v["content_identity"] = serde_json::to_value(p.legacy_content_identity().unwrap()).unwrap();
    });
    edit_json(&mut e, "project.json", |v| {
        v.as_object_mut().unwrap().remove("description");
        v.as_object_mut().unwrap().remove("dictionary_entries");
        v.as_object_mut().unwrap().remove("dictionary_links");
    });
    let bytes = archive(&e);
    let before = bytes.clone();
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert_eq!(bytes, before);
    assert_eq!(
        loaded.project.content_identity().unwrap(),
        p.content_identity().unwrap()
    );
}
#[test]
fn expanded_entry_limit_is_enforced() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.iter_mut()
        .find(|(n, _)| n.starts_with("resources/"))
        .unwrap()
        .1 = vec![0; 32 * 1024 * 1024 + 1];
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn unknown_project_fields_and_future_schema_are_rejected() {
    let original = entries(&write_bytes(&project()).unwrap());
    let mut e = original.clone();
    edit_json(&mut e, "project.json", |v| {
        v["resources"][0]["future"] = true.into();
    });
    assert!(load_bytes(&archive(&e)).is_err());
    let mut e = original;
    edit_json(&mut e, "manifest.json", |v| {
        v["schema"] = 4.into();
    });
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn shared_resources_reuse_hydrated_bytes() {
    let mut p = project();
    let mut alias = p.resources[0].clone();
    alias.id = "alias".parse().unwrap();
    p.resources.push(alias);
    let before = p.content_identity().unwrap();
    let loaded = load_bytes(&write_bytes(&p).unwrap()).unwrap().project;
    assert_eq!(before, loaded.content_identity().unwrap());
    assert!(Arc::ptr_eq(
        &loaded.resources[0].bytes,
        &loaded.resources[1].bytes
    ));
}
#[test]
fn logical_resource_size_budget_is_checked_before_hydration() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    edit_json(&mut e, "project.json", |v| {
        let original = v["resources"][0].clone();
        let resources = v["resources"].as_array_mut().unwrap();
        for i in 0..9 {
            let mut alias = original.clone();
            alias["id"] = format!("alias{i}").into();
            alias["size"] = (32_u64 * 1024 * 1024).into();
            resources.push(alias);
        }
    });
    assert!(
        load_bytes(&archive(&e))
            .err()
            .unwrap()
            .to_string()
            .contains("logical resources exceed limit")
    );
}

#[test]
fn schema_one_is_verified_then_migrated_without_touching_source() {
    let p = project();
    let mut e = entries(&write_bytes(&p).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!([]);
        v["schema"] = 1.into();
        v["min_reader"] = 1.into();
        v["content_identity"] = serde_json::to_value(p.legacy_content_identity().unwrap()).unwrap();
    });
    edit_json(&mut e, "project.json", |v| {
        v.as_object_mut().unwrap().remove("dictionary_entries");
        v.as_object_mut().unwrap().remove("dictionary_links");
    });
    let bytes = archive(&e);
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert!(loaded.project.dictionary_entries.is_empty());
    edit_json(&mut e, "project.json", |v| v["name"] = "tampered".into());
    assert!(load_bytes(&archive(&e)).is_err());
    assert!(
        !load_bytes(&write_bytes(&loaded.project).unwrap())
            .unwrap()
            .requires_save_as_or_backup
    );
}

#[test]
fn schema_two_round_trips_dictionary_entries() {
    use processcraft_core::{
        ids::{DictionaryEntryId, ElementKey},
        model::{
            BpmnDocument, DictionaryEntry, DictionaryKind, DictionaryLink, DictionaryTarget, Lane,
            Process,
        },
    };
    let mut p = project();
    p.dictionary_entries.push(DictionaryEntry {
        id: DictionaryEntryId::new(),
        kind: DictionaryKind::Role,
        name: "Manager".into(),
        description: "Approves".into(),
    });
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.processes.push(Process {
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: "process".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new(),
        nodes: vec![],
        lanes: vec![Lane {
            child_lane_set_id: None,
            id: "lane".parse().unwrap(),
            name: Some(String::new()),
            parent: None,
            members: vec![],
        }],
        flows: vec![],
    });
    p.dictionary_links.push(DictionaryLink {
        source: ElementKey {
            document_id: doc.id,
            xml_id: "lane".parse().unwrap(),
        },
        kind: DictionaryKind::Role,
        target: DictionaryTarget::Entry(p.dictionary_entries[0].id),
    });
    p.documents.push(Arc::new(doc));
    let data = write_bytes(&p).unwrap();
    let loaded = load_bytes(&data).unwrap();
    assert_eq!(
        loaded.project.dictionary_links[0].target,
        DictionaryTarget::Entry(p.dictionary_entries[0].id)
    );
    assert!(!loaded.requires_save_as_or_backup);
    assert_eq!(
        loaded.project.content_identity().unwrap(),
        p.content_identity().unwrap()
    );
    assert_eq!(loaded.project.dictionary_entries[0].name, "Manager");
}

#[test]
fn checkpoint_payload_round_trips_with_shared_resources_and_detects_tampering() {
    use processcraft_core::{
        ids::CheckpointId,
        model::{Checkpoint, FrozenProject},
    };
    let mut p = project();
    let id = CheckpointId::new();
    p.checkpoints.push(Checkpoint {
        id,
        name: "Reference".into(),
        created_unix_ms: 42,
        content: FrozenProject::capture(&p),
    });
    p.reference_checkpoint = Some(id);
    p.name = "Current".into();
    let data = write_bytes(&p).unwrap();
    let mut archive_entries = entries(&data);
    let path = format!("checkpoints/{id}.json");
    assert!(archive_entries.iter().any(|(n, _)| n == &path));
    assert_eq!(
        archive_entries
            .iter()
            .filter(|(n, _)| n.starts_with("resources/"))
            .count(),
        1
    );
    let loaded = load_bytes(&data).unwrap().project;
    assert_eq!(loaded.checkpoints[0].content.name, "Native test");
    assert_eq!(loaded.reference_checkpoint, Some(id));
    assert!(Arc::ptr_eq(
        &loaded.resources[0].bytes,
        &loaded.checkpoints[0].content.resources[0].bytes
    ));
    edit_json(&mut archive_entries, &path, |v| {
        v["name"] = "tampered".into()
    });
    assert!(load_bytes(&archive(&archive_entries)).is_err());
}

#[test]
fn native_comments_round_trip_including_orphan() {
    use processcraft_core::{
        ids::{CommentId, DocumentId, ElementKey},
        model::Comment,
        queries::comment_is_orphan,
    };
    let mut p = project();
    p.comments.push(Comment {
        id: CommentId::new(),
        target: ElementKey {
            document_id: DocumentId::new(),
            xml_id: "deleted".parse().unwrap(),
        },
        author: "Analyst".into(),
        text: "Keep this context".into(),
    });
    let loaded = load_bytes(&write_bytes(&p).unwrap()).unwrap().project;
    assert_eq!(loaded.comments[0].text, "Keep this context");
    assert!(comment_is_orphan(&loaded, &loaded.comments[0]));
}

#[test]
fn schema_two_digest_verified_before_migration_to_three() {
    let p = project();
    let mut old = entries(&write_bytes(&p).unwrap());
    edit_json(&mut old, "manifest.json", |v| {
        v["schema"] = 2.into();
        v["min_reader"] = 2.into();
        v["required_capabilities"] = serde_json::json!([]);
    });
    let bytes = archive(&old);
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert_eq!(
        loaded.source_content_identity,
        p.content_identity().unwrap()
    );
    edit_json(&mut old, "project.json", |v| v["name"] = "tampered".into());
    assert!(load_bytes(&archive(&old)).is_err());
}

#[test]
fn twenty_checkpoints_share_large_resource_and_round_trip_with_bounded_archive() {
    use processcraft_core::{
        ids::CheckpointId,
        model::{Checkpoint, FrozenProject},
    };
    let mut p = ProjectData::new("Bounded history".into());
    p.resources.push(Resource::new(
        "shared".parse().unwrap(),
        "application/octet-stream".into(),
        Arc::from(vec![42u8; 1024 * 1024]),
    ));
    for index in 0..20 {
        p.checkpoints.push(Checkpoint {
            id: CheckpointId::new(),
            name: format!("Checkpoint {index}"),
            created_unix_ms: index,
            content: FrozenProject::capture(&p),
        });
    }
    let bytes = write_bytes(&p).unwrap();
    let archived = entries(&bytes);
    assert_eq!(
        archived
            .iter()
            .filter(|(name, _)| name.starts_with("checkpoints/"))
            .count(),
        20
    );
    assert_eq!(
        archived
            .iter()
            .filter(|(name, _)| name.starts_with("resources/"))
            .count(),
        1
    );
    let loaded = load_bytes(&bytes).unwrap().project;
    assert_eq!(loaded.checkpoints.len(), 20);
    for checkpoint in &loaded.checkpoints {
        assert!(Arc::ptr_eq(
            &loaded.resources[0].bytes,
            &checkpoint.content.resources[0].bytes
        ));
    }
    let extra = Checkpoint {
        id: CheckpointId::new(),
        name: "Over limit".into(),
        created_unix_ms: 21,
        content: FrozenProject::capture(&p),
    };
    p.checkpoints.push(extra);
    assert!(write_bytes(&p).is_err());
}

#[test]
fn native_identity_capability_covers_live_and_checkpoint_documents() {
    use processcraft_core::{
        commands::{Envelope, Mutation, Request, Session},
        ids::ElementSelector,
        model::FrozenProject,
    };
    let bytes =
        include_bytes!("../../../tests/fixtures/native/identity-baseline-2f0f770.processcraft");
    let original = load_bytes(bytes).unwrap().project;
    let mut session = Session::new(original.clone()).unwrap();
    let document_id = original.documents[0].id;
    let result = session
        .apply(
            Envelope {
                request_id: "identity".into(),
                session_id: session.id(),
                expected_generation: Some(0),
            },
            Request::Batch {
                mutations: vec![Mutation::SetElementXmlId {
                    document_id,
                    element_id: "task".parse().unwrap(),
                    xml_id: None,
                }],
            },
        )
        .unwrap();
    assert!(matches!(
        result.selector_mappings[0].to,
        ElementSelector::Native { .. }
    ));
    let mut project = (*session.snapshot().project).clone();
    let live = write_bytes(&project).unwrap();
    let mut archived = entries(&live);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "bpmn-native-element-identity")
    );
    assert_eq!(
        load_bytes(&live)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        project.content_identity().unwrap()
    );
    edit_json(&mut archived, "manifest.json", |value| {
        value["required_capabilities"] =
            serde_json::json!(["comments", "checkpoints", "bpmn-supporting-catalog"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("native element identities require capability")
    );
    project.checkpoints[0].content = FrozenProject::capture(&project);
    project.documents = original.documents;
    project.links = original.links;
    project.dictionary_links = original.dictionary_links;
    project.comments = original.comments;
    let frozen = write_bytes(&project).unwrap();
    let mut archived = entries(&frozen);
    assert_eq!(
        load_bytes(&frozen)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        project.content_identity().unwrap()
    );
    edit_json(&mut archived, "manifest.json", |value| {
        value["required_capabilities"] =
            serde_json::json!(["comments", "checkpoints", "bpmn-supporting-catalog"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("native element identities require capability")
    );
}

#[test]
fn identity_capability_covers_independent_new_fields() {
    use processcraft_core::{
        bpmn_model::{EventResource, NodeDetails},
        ids::{CommentId, ElementKey, ElementSelector, NativeElementId},
        model::{
            BpmnDocument, Checkpoint, Collaboration, Comment, Diagram, FrozenProject, Node,
            NodeKind, Process,
        },
    };
    let mut base = ProjectData::new("features".into());
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.event_resources.push(EventResource::Message {
        id: "message".parse().unwrap(),
        name: None,
        item_ref: None,
    });
    doc.processes.push(Process {
        id: "process".parse().unwrap(),
        name: Some("Process".into()),
        documentation: String::new(),
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    base.documents.push(Arc::new(doc));
    let caps = |bytes: &[u8]| -> serde_json::Value {
        let e = entries(bytes);
        serde_json::from_slice(&e.iter().find(|(n, _)| n == "manifest.json").unwrap().1).unwrap()
    };
    assert!(
        !caps(&write_bytes(&base).unwrap())["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "bpmn-native-element-identity")
    );
    for case in 0..6 {
        let mut p = base.clone();
        match case {
            0 => Arc::make_mut(&mut p.documents[0]).processes[0].is_executable = None,
            1 => Arc::make_mut(&mut p.documents[0]).processes[0].is_executable = Some(true),
            2 => Arc::make_mut(&mut p.documents[0]).processes[0]
                .nodes
                .push(Node {
                    id: "annotation".parse().unwrap(),
                    name: Some(String::new()),
                    documentation: String::new(),
                    kind: NodeKind::TextAnnotation,
                    details: Some(NodeDetails::TextAnnotation {
                        text_format: None,
                        text: Some("body".into()),
                    }),
                    item_aware: None,
                    loop_characteristics: None,
                    io_specification: None,
                    activity: None,
                    owner: String::new(),
                    contact: String::new(),
                }),
            3 => p.comments.push(Comment {
                id: CommentId::new(),
                target: ElementKey {
                    document_id: p.documents[0].id,
                    xml_id: ElementSelector::Native {
                        native_id: NativeElementId::new(),
                    },
                },
                author: String::new(),
                text: "orphan".into(),
            }),
            4 => {
                Arc::make_mut(&mut p.documents[0]).collaboration = Some(Collaboration {
                    id: "collaboration".parse().unwrap(),
                    name: Some(String::new()),
                    participants: vec![],
                    flows: vec![],
                    conversation_nodes: vec![],
                    conversation_links: vec![],
                })
            }
            _ => Arc::make_mut(&mut p.documents[0]).diagrams.push(Diagram {
                id: "diagram".parse().unwrap(),
                name: Some(String::new()),
                plane_id: "plane".parse().unwrap(),
                plane: "process".parse().unwrap(),
                shapes: vec![],
                edges: vec![],
            }),
        }
        let live = write_bytes(&p).unwrap();
        assert!(
            caps(&live)["required_capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "bpmn-native-element-identity"),
            "case {case}"
        );
        let mut archived = entries(&live);
        edit_json(&mut archived, "manifest.json", |v| {
            v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
        });
        assert!(
            load_bytes(&archive(&archived))
                .err()
                .unwrap()
                .to_string()
                .contains("native element identities require capability"),
            "case {case}"
        );
        let snapshot = FrozenProject::capture(&p);
        p.documents = base.documents.clone();
        p.comments.clear();
        p.checkpoints.push(Checkpoint {
            id: processcraft_core::ids::CheckpointId::new(),
            name: "snapshot".into(),
            created_unix_ms: 0,
            content: snapshot,
        });
        let frozen = write_bytes(&p).unwrap();
        assert!(
            caps(&frozen)["required_capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "bpmn-native-element-identity"),
            "checkpoint case {case}"
        );
        let mut archived = entries(&frozen);
        edit_json(&mut archived, "manifest.json", |v| {
            v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
        });
        assert!(
            load_bytes(&archive(&archived))
                .err()
                .unwrap()
                .to_string()
                .contains("native element identities require capability"),
            "checkpoint case {case}"
        );
    }
}

#[test]
fn expanded_events_and_di_capability_round_trips_live_and_checkpoint() {
    use processcraft_core::{
        bpmn_model::{EventDefinition, EventReference, NodeDetails},
        ids::CheckpointId,
        model::{
            Bounds, BpmnDocument, Checkpoint, Diagram, FrozenProject, Node, NodeKind, Process,
            Shape,
        },
    };
    let mut project = ProjectData::new("event capability".into());
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.root_event_definitions.push(EventDefinition::Timer {
        id: Some("timer".parse().unwrap()),
        kind: None,
        expression: None,
    });
    doc.processes.push(Process {
        id: "process".parse().unwrap(),
        name: Some("Process".into()),
        documentation: String::new(),
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            id: "event".parse().unwrap(),
            name: Some("Start".into()),
            documentation: String::new(),
            kind: NodeKind::StartEvent,
            item_aware: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
            details: Some(NodeDetails::Event {
                definitions: vec![],
                attached_to: None,
                cancel_activity: None,
                definition_refs: vec![EventReference::Local("timer".parse().unwrap())],
                parallel_multiple: Some(false),
                is_interrupting: Some(true),
                properties: vec![],
                io: None,
            }),
        }],
        lanes: vec![],
        flows: vec![],
    });
    doc.diagrams.push(Diagram {
        id: "diagram".parse().unwrap(),
        name: None,
        plane_id: "plane".parse().unwrap(),
        plane: "process".parse().unwrap(),
        shapes: vec![Shape {
            id: "shape".parse().unwrap(),
            element: "event".parse().unwrap(),
            is_expanded: None,
            is_horizontal: Some(false),
            is_marker_visible: Some(true),
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 36.0,
                height: 36.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    project.documents.push(Arc::new(doc));
    let bytes = write_bytes(&project).unwrap();
    let mut archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(
        manifest["required_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "bpmn-expanded-events-di")
    );
    let loaded = load_bytes(&bytes).unwrap().project;
    assert_eq!(
        loaded.content_identity().unwrap(),
        project.content_identity().unwrap()
    );
    assert!(matches!(
        &loaded.documents[0].root_event_definitions[0],
        EventDefinition::Timer {
            kind: None,
            expression: None,
            ..
        }
    ));
    edit_json(&mut archived, "manifest.json", |value| {
        value["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded events and diagram metadata require capability")
    );
    let snapshot = FrozenProject::capture(&project);
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.root_event_definitions.clear();
    doc.processes[0].nodes.clear();
    doc.diagrams.clear();
    project.checkpoints.push(Checkpoint {
        id: CheckpointId::new(),
        name: "snapshot".into(),
        created_unix_ms: 0,
        content: snapshot,
    });
    let bytes = write_bytes(&project).unwrap();
    let mut archived = entries(&bytes);
    assert_eq!(
        load_bytes(&bytes)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        project.content_identity().unwrap()
    );
    edit_json(&mut archived, "manifest.json", |value| {
        value["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
    });
    assert!(
        load_bytes(&archive(&archived))
            .err()
            .unwrap()
            .to_string()
            .contains("expanded events and diagram metadata require capability")
    );
}

#[test]
fn expanded_events_and_di_capability_detects_independent_features() {
    use processcraft_core::{
        bpmn_data::{DataPort, ItemAwareMetadata},
        bpmn_model::{
            EventDefinition, EventIo, EventProperty, EventReference, EventResource, NodeDetails,
            QNameRef,
        },
        model::{BpmnDocument, Node, NodeKind, Process},
    };
    let q = || QNameRef {
        namespace: "urn:external".into(),
        local: "item".parse().unwrap(),
    };
    for case in 0..5 {
        let mut project = ProjectData::new("feature".into());
        let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
        let mut details = NodeDetails::Event {
            definitions: vec![],
            definition_refs: vec![],
            attached_to: None,
            cancel_activity: None,
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        };
        if let NodeDetails::Event {
            definition_refs,
            properties,
            io,
            ..
        } = &mut details
        {
            match case {
                0 => doc.root_event_definitions.push(EventDefinition::Timer {
                    id: None,
                    kind: None,
                    expression: None,
                }),
                1 => definition_refs.push(EventReference::Qualified(q())),
                2 => properties.push(EventProperty {
                    id: None,
                    name: Some("state".into()),
                    item_aware: Some(ItemAwareMetadata {
                        item_subject_ref: Some(q()),
                        data_state: None,
                    }),
                }),
                3 => {
                    *io = Some(EventIo::Catch {
                        outputs: vec![DataPort {
                            id: None,
                            name: Some("output".into()),
                            item_aware: None,
                            is_collection: None,
                        }],
                        output_set: None,
                    })
                }
                _ => doc.event_resources.push(EventResource::Message {
                    id: "message".parse().unwrap(),
                    name: None,
                    item_ref: Some(q()),
                }),
            }
        }
        doc.processes.push(Process {
            id: "process".parse().unwrap(),
            name: Some("Process".into()),
            documentation: String::new(),
            is_executable: Some(false),
            io_specification: None,
            lane_set_id: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![Node {
                id: "start".parse().unwrap(),
                name: Some("Start".into()),
                documentation: String::new(),
                kind: NodeKind::StartEvent,
                details: Some(details),
                item_aware: None,
                loop_characteristics: None,
                io_specification: None,
                activity: None,
                owner: String::new(),
                contact: String::new(),
            }],
            lanes: vec![],
            flows: vec![],
        });
        project.documents.push(Arc::new(doc));
        let bytes = write_bytes(&project).unwrap_or_else(|e| panic!("case {case}: {e}"));
        let mut archived = entries(&bytes);
        let manifest: serde_json::Value = serde_json::from_slice(
            &archived
                .iter()
                .find(|(name, _)| name == "manifest.json")
                .unwrap()
                .1,
        )
        .unwrap();
        assert!(
            manifest["required_capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "bpmn-expanded-events-di"),
            "case {case}"
        );
        assert_eq!(
            load_bytes(&bytes)
                .unwrap()
                .project
                .content_identity()
                .unwrap(),
            project.content_identity().unwrap()
        );
        edit_json(&mut archived, "manifest.json", |v| {
            v["required_capabilities"] = serde_json::json!(["comments", "checkpoints"])
        });
        assert!(
            load_bytes(&archive(&archived))
                .err()
                .unwrap()
                .to_string()
                .contains("expanded events and diagram metadata require capability"),
            "case {case}"
        );
    }
}

#[test]
fn legacy_timer_choice_and_local_reference_keep_old_json_shape() {
    use processcraft_core::{
        bpmn_model::{EventDefinition, EventReference, Expression, NodeDetails, TimerKind},
        model::{BpmnDocument, Node, NodeKind, Process},
    };
    let mut p = ProjectData::new("legacy timer".into());
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.processes.push(Process {
        id: "process".parse().unwrap(),
        name: Some("process".into()),
        documentation: String::new(),
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            id: "start".parse().unwrap(),
            name: Some("start".into()),
            documentation: String::new(),
            kind: NodeKind::StartEvent,
            item_aware: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
            details: Some(NodeDetails::Event {
                definitions: vec![EventDefinition::Timer {
                    id: None,
                    kind: Some(TimerKind::Duration),
                    expression: Some(Expression::Legacy("PT1H".into())),
                }],
                attached_to: None,
                cancel_activity: None,
                definition_refs: vec![],
                parallel_multiple: None,
                is_interrupting: None,
                properties: vec![],
                io: None,
            }),
        }],
        lanes: vec![],
        flows: vec![],
    });
    p.documents.push(Arc::new(doc));
    let bytes = write_bytes(&p).unwrap();
    let archived = entries(&bytes);
    let manifest: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        manifest["required_capabilities"],
        serde_json::json!(["comments", "checkpoints"])
    );
    let raw: serde_json::Value = serde_json::from_slice(
        &archived
            .iter()
            .find(|(name, _)| name == "project.json")
            .unwrap()
            .1,
    )
    .unwrap();
    let timer = &raw["documents"][0]["processes"][0]["nodes"][0]["details"]["definitions"][0];
    assert_eq!(timer["kind"], "Duration");
    assert_eq!(timer["expression"], "PT1H");
    assert_eq!(
        serde_json::to_value(EventReference::Local("legacy".parse().unwrap())).unwrap(),
        serde_json::json!("legacy")
    );
    assert_eq!(
        load_bytes(&bytes)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        p.content_identity().unwrap()
    );
}
