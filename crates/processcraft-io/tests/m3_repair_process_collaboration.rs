//! Repair tests for the process/collaboration family: typed import values and
//! value-asserting, XSD-checked export of
//! `tests/fixtures/bpmn/m3-repair-process-collaboration/`.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_model::EventResource,
    ids::ElementSelector,
    model::{BpmnDocument, FlowKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!(
    "../../../tests/fixtures/bpmn/m3-repair-process-collaboration/repair-process-collaboration.bpmn"
);

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn local(s: &str) -> Option<ModelReference> {
    Some(ModelReference::Local(sel(s)))
}
fn import(bytes: &[u8]) -> PreparedImport {
    let b = prepare_import(bytes, "fixture").unwrap();
    assert!(b.report.editable, "must import as typed model");
    b
}
fn export_xml(b: &PreparedImport) -> String {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    let bytes = export_bpmn(&p, b.document.id, false).unwrap().bytes;
    // Dumped for `python scripts/check_bpmn20_xsd.py` (see the fixture manifest).
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(
        dir.join("m3-repair-process-collaboration-export.bpmn"),
        &bytes,
    )
    .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Asserts every typed value of the family; used on the import and on the re-import.
fn check(d: &BpmnDocument) {
    let c = d.collaboration.as_ref().unwrap();
    assert_eq!(c.name.as_deref(), Some("Invoice exchange"));
    assert_eq!(c.participants[0].process, local("Sell"));
    let f = &c.flows[0];
    assert_eq!(f.kind, FlowKind::Message);
    assert_eq!(f.source, local("Seller"));
    assert_eq!(f.target, local("Buyer"));
    assert_eq!(f.message_ref, local("Msg"));
    match &d.event_resources[..] {
        [
            EventResource::Message {
                id, name, item_ref, ..
            },
        ] => {
            assert_eq!(id.selector(), sel("Msg"));
            assert_eq!(name.as_deref(), Some("Invoice"));
            let q = item_ref.as_ref().unwrap();
            assert_eq!(
                (q.namespace.as_str(), q.local.to_string()),
                ("urn:pc-repair-collab", "Item".into())
            );
        }
        other => panic!("{other:?}"),
    }
    let p = &d.processes[0];
    assert_eq!(p.lane_set_id.as_ref().unwrap().selector(), sel("TopSet"));
    assert_eq!(p.lane_set_name.as_deref(), Some("Top set"));
    let parent = p
        .lanes
        .iter()
        .find(|l| l.id.selector() == sel("Parent"))
        .unwrap();
    let inner = p
        .lanes
        .iter()
        .find(|l| l.id.selector() == sel("Inner"))
        .unwrap();
    assert_eq!(
        parent.child_lane_set_id.as_ref().unwrap().selector(),
        sel("ChildSet")
    );
    assert_eq!(parent.child_lane_set_name.as_deref(), Some("Child set"));
    assert_eq!(parent.parent, None);
    assert_eq!(parent.members, vec![sel("Start")]);
    assert_eq!(inner.parent, Some(sel("Parent")));
    assert_eq!(inner.name.as_deref(), Some("Review"));
    assert_eq!(inner.members, vec![sel("Work"), sel("End")]);
    let f1 = &p.flows[0];
    assert_eq!(f1.kind, FlowKind::Sequence);
    assert_eq!(f1.source, local("Start"));
    assert_eq!(f1.target, local("Work"));
    assert_eq!(
        f1.condition.as_ref().unwrap().plain_text(),
        Some("amount > 10")
    );
}

#[test]
fn typed_values_import_from_the_manifested_fixture() {
    check(&import(FIXTURE).document);
}

#[test]
fn export_writes_the_values_and_reimport_asserts_them_again() {
    let b = import(FIXTURE);
    let xml = export_xml(&b);
    for needle in [
        "<collaboration ",
        "name=\"Invoice exchange\"",
        "<participant ",
        "processRef=\"",
        "<messageFlow ",
        "sourceRef=\"",
        "targetRef=\"",
        "messageRef=\"",
        "<message ",
        "name=\"Invoice\"",
        "itemRef=\"",
        "<laneSet id=\"TopSet\" name=\"Top set\"",
        "<childLaneSet id=\"ChildSet\" name=\"Child set\"",
        "<lane id=\"Inner\" name=\"Review\"",
        "<flowNodeRef>Work</flowNodeRef>",
        "amount &gt; 10</conditionExpression>",
    ] {
        assert!(xml.contains(needle), "export lacks {needle}\n{xml}");
    }
    let again = import(xml.as_bytes());
    check(&again.document);
}
