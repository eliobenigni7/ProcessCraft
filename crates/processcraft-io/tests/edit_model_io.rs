//! M3 round 2 (edit-model-io): typed import and XSD-checked export of
//! `tests/fixtures/bpmn/edit-model-io/lane-partitions.bpmn` (laneSet/childLaneSet
//! `name`, lane `partitionElement` and `partitionElementRef`, and the surrounding
//! process, collaboration, participant, message flow, message and sequence flow
//! elements).
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_model::EventResource,
    ids::ElementSelector,
    model::{BpmnDocument, FlowKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/edit-model-io/lane-partitions.bpmn");

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn local(s: &str) -> Option<ModelReference> {
    Some(ModelReference::Local(sel(s)))
}
fn import_bytes(bytes: &[u8]) -> PreparedImport {
    prepare_import(bytes, "fixture").unwrap()
}
fn import() -> PreparedImport {
    let b = import_bytes(FIXTURE);
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
    std::fs::write(dir.join("m3r2-edit-model-io-export.bpmn"), &bytes).unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
fn doc(b: &PreparedImport) -> &BpmnDocument {
    &b.document
}

#[test]
fn lane_set_names_and_lane_partitions_import_typed() {
    let b = import();
    let main = &doc(&b).processes[0];
    assert_eq!(main.lane_set_id.as_ref().unwrap().selector(), sel("Lanes"));
    assert_eq!(main.lane_set_name.as_deref(), Some("By role"));
    let a = main
        .lanes
        .iter()
        .find(|l| l.id.selector() == sel("LaneA"))
        .unwrap();
    assert_eq!(a.child_lane_set_name.as_deref(), Some("By skill"));
    let part = a.partition_element.as_ref().unwrap();
    assert_eq!(part.id.selector(), sel("Part"));
    assert_eq!(part.value.as_deref(), Some("Office"));
    assert_eq!(a.partition_element_ref, None);
    let b_lane = main
        .lanes
        .iter()
        .find(|l| l.id.selector() == sel("LaneB"))
        .unwrap();
    assert_eq!(b_lane.partition_element_ref, local("Buyer"));
    assert!(b_lane.partition_element.is_none());
    let c = main
        .lanes
        .iter()
        .find(|l| l.id.selector() == sel("LaneC"))
        .unwrap();
    assert_eq!(c.parent, Some(sel("LaneA")));
}

#[test]
fn process_collaboration_message_and_flows_import_typed() {
    let b = import();
    let d = doc(&b);
    let c = d.collaboration.as_ref().unwrap();
    assert_eq!(c.name.as_deref(), Some("Order handling"));
    assert_eq!(c.participants.len(), 2);
    assert_eq!(c.participants[0].process, local("Main"));
    let flow = &c.flows[0];
    assert_eq!(flow.kind, FlowKind::Message);
    assert_eq!(flow.message_ref, local("Msg"));
    assert!(
        matches!(&d.event_resources[..], [EventResource::Message { name, .. }] if name.as_deref() == Some("Order message"))
    );
    assert_eq!(d.processes[0].is_executable, Some(false));
    assert_eq!(d.processes[0].flows.len(), 2);
    assert_eq!(d.processes[0].flows[0].kind, FlowKind::Sequence);
}

#[test]
fn export_writes_lane_set_names_and_partitions_and_reimports_equal() {
    let b = import();
    let xml = export_xml(&b);
    for needle in [
        "name=\"By role\"",
        "name=\"By skill\"",
        "partitionElementRef=",
        "<partitionElement",
        "tCategoryValue",
        "value=\"Office\"",
    ] {
        assert!(xml.contains(needle), "export lacks {needle}\n{xml}");
    }
    let again = import_bytes(xml.as_bytes());
    assert!(again.report.editable);
    let (a, o) = (doc(&again), doc(&b));
    for (x, y) in [
        (
            serde_json::to_value(&a.processes).unwrap(),
            serde_json::to_value(&o.processes).unwrap(),
        ),
        (
            serde_json::to_value(&a.collaboration).unwrap(),
            serde_json::to_value(&o.collaboration).unwrap(),
        ),
        (
            serde_json::to_value(&a.event_resources).unwrap(),
            serde_json::to_value(&o.event_resources).unwrap(),
        ),
    ] {
        assert_eq!(x, y);
    }
}

/// An abstract tBaseElement `partitionElement` without a concrete xsi:type is invalid
/// against the pinned XSD and an unmodelled concrete type has content we do not type:
/// both must be refused as typed content (preserved source), never reinterpreted.
#[test]
fn unmodelled_partition_element_types_are_not_imported_as_typed() {
    let text = std::str::from_utf8(FIXTURE).unwrap();
    let no_type = text.replace(
        " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"tCategoryValue\"",
        "",
    );
    assert_ne!(no_type, text);
    assert!(!import_bytes(no_type.as_bytes()).report.editable);
    let other = text.replace("xsi:type=\"tCategoryValue\"", "xsi:type=\"tTask\"");
    assert!(!import_bytes(other.as_bytes()).report.editable);
}
