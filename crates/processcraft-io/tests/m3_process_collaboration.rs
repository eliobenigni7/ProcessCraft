//! M3 process/collaboration family: typed import and XSD-checked export of the
//! synthetic fixture `tests/fixtures/bpmn/m3-process-collaboration/`.
use processcraft_core::{
    bpmn_activity::{ModelReference, ProcessType},
    bpmn_model::EventResource,
    ids::ElementSelector,
    model::{BpmnDocument, FlowKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!(
    "../../../tests/fixtures/bpmn/m3-process-collaboration/process-collaboration.bpmn"
);

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn local(s: &str) -> Option<ModelReference> {
    Some(ModelReference::Local(sel(s)))
}
fn import() -> PreparedImport {
    let b = prepare_import(FIXTURE, "fixture").unwrap();
    assert!(b.report.editable, "fixture must import as typed model");
    b
}
fn export_xml(b: &PreparedImport) -> String {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    let bytes = export_bpmn(&p, b.document.id, false).unwrap().bytes;
    // Dumped for `python scripts/check_bpmn20_xsd.py` (see the fixture manifest).
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join("m3-process-collaboration-export.bpmn"), &bytes).unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
fn doc(b: &PreparedImport) -> &BpmnDocument {
    &b.document
}

#[test]
fn collaboration_attributes_and_choreography_ref_import() {
    let b = import();
    let c = doc(&b).collaboration.as_ref().unwrap();
    assert_eq!(c.name.as_deref(), Some("Order handling"));
    let extras = c.extras.as_ref().unwrap();
    assert_eq!(extras.is_closed, Some(true));
    assert_eq!(
        extras.choreography_refs,
        vec![ModelReference::Local(sel("Chor"))]
    );
}

#[test]
fn participant_name_interface_and_endpoint_refs_import() {
    let b = import();
    let p = &doc(&b).collaboration.as_ref().unwrap().participants;
    assert_eq!(p[0].name.as_deref(), Some("Buyer"));
    assert_eq!(p[0].interface_refs, vec![local("Iface").unwrap()]);
    assert_eq!(p[0].endpoint_refs, vec![local("Endpoint").unwrap()]);
    assert_eq!(p[0].process, local("Main"));
    assert_eq!(p[1].name.as_deref(), Some("Seller"));
    assert!(p[1].interface_refs.is_empty() && p[1].endpoint_refs.is_empty());
}

#[test]
fn message_flow_name_and_ends_import() {
    let b = import();
    let f = &doc(&b).collaboration.as_ref().unwrap().flows[0];
    assert_eq!(f.kind, FlowKind::Message);
    assert_eq!(f.name.as_deref(), Some("Order"));
    assert_eq!(f.source, local("Buyer"));
    assert_eq!(f.target, local("Seller"));
    assert_eq!(f.message_ref, local("Msg"));
}

#[test]
fn root_message_name_and_item_ref_import() {
    let b = import();
    match &doc(&b).event_resources[..] {
        [
            EventResource::Message {
                id, name, item_ref, ..
            },
        ] => {
            assert_eq!(id.selector(), sel("Msg"));
            assert_eq!(name.as_deref(), Some("Order message"));
            let q = item_ref.as_ref().unwrap();
            assert_eq!(
                (q.namespace.as_str(), q.local.to_string()),
                ("urn:pc-collab", "Item".into())
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn process_attributes_supports_and_collaboration_ref_import() {
    let b = import();
    let main = &doc(&b).processes[1];
    assert_eq!(main.id.selector(), sel("Main"));
    assert_eq!(main.is_executable, Some(false));
    let m = main.activity_metadata.as_ref().unwrap();
    assert_eq!(m.process_type, Some(ProcessType::Public));
    assert_eq!(m.is_closed, Some(true));
    assert_eq!(m.definitional_collaboration_ref, local("Collab"));
    assert_eq!(m.supports, vec![local("Support").unwrap()]);
}

#[test]
fn lane_names_and_flow_node_refs_import() {
    let b = import();
    let main = &doc(&b).processes[1];
    assert_eq!(main.lane_set_id.as_ref().unwrap().selector(), sel("Lanes"));
    let names: Vec<_> = main.lanes.iter().map(|l| l.name.as_deref()).collect();
    assert_eq!(names, [Some("Clerk"), Some("Manager")]);
    assert_eq!(main.lanes[0].members, vec![sel("Start"), sel("Work")]);
    assert_eq!(main.lanes[1].members, vec![sel("End")]);
}

#[test]
fn sequence_flow_is_immediate_import() {
    let b = import();
    let flows = &doc(&b).processes[1].flows;
    assert_eq!(
        flows[0].flow_metadata.as_ref().unwrap().is_immediate,
        Some(true)
    );
    assert_eq!(flows[1].flow_metadata, None);
}

#[test]
fn export_writes_all_values_and_reimport_is_equal() {
    let b = import();
    let xml = export_xml(&b);
    for needle in [
        "name=\"Order handling\"",
        "isClosed=\"true\"",
        ":choreographyRef",
        "name=\"Buyer\"",
        ":interfaceRef",
        ":endPointRef",
        "name=\"Order\"",
        "name=\"Order message\"",
        "itemRef=",
        "processType=\"Public\"",
        "definitionalCollaborationRef=",
        ":supports",
        "name=\"Clerk\"",
        "<flowNodeRef>",
        "isImmediate=\"true\"",
    ] {
        assert!(xml.contains(needle), "export lacks {needle}\n{xml}");
    }
    let again = prepare_import(xml.as_bytes(), "again").unwrap();
    assert!(again.report.editable);
    let (a, o) = (doc(&again), doc(&b));
    assert_eq!(
        serde_json::to_value(&a.collaboration).unwrap(),
        serde_json::to_value(&o.collaboration).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&a.processes).unwrap(),
        serde_json::to_value(&o.processes).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&a.event_resources).unwrap(),
        serde_json::to_value(&o.event_resources).unwrap()
    );
}
