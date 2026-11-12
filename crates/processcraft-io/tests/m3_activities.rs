use processcraft_core::bpmn_model::NodeDetails;
use processcraft_core::model::{NodeKind, ProjectData};
use processcraft_io::interchange::{ExportedBpmn, PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../../tests/fixtures/bpmn/m3-activities/activities.bpmn");

fn export(b: &PreparedImport) -> ExportedBpmn {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap()
}

fn node<'a>(b: &'a PreparedImport, id: &str) -> &'a processcraft_core::model::Node {
    b.document.processes[0]
        .nodes
        .iter()
        .find(|n| n.id.to_string() == id)
        .unwrap_or_else(|| panic!("node {id}"))
}

fn check(b: &PreparedImport) {
    assert!(b.report.editable, "{:?}", b.report.limitations);
    let a = |id| node(b, id).activity.clone().unwrap_or_default();
    let plain = node(b, "plain");
    assert_eq!(plain.kind, NodeKind::Task);
    let p = a("plain");
    assert_eq!(p.is_for_compensation, Some(true));
    assert_eq!(p.start_quantity.as_deref(), Some("3"));
    assert_eq!(p.completion_quantity.as_deref(), Some("2"));
    assert_eq!(p.default_flow.map(|f| f.to_string()), Some("toUser".into()));
    assert_eq!(node(b, "user").kind, NodeKind::UserTask);
    assert_eq!(a("user").implementation.as_deref(), Some("##unspecified"));
    assert_eq!(node(b, "service1").kind, NodeKind::ServiceTask);
    assert_eq!(
        a("service1").implementation.as_deref(),
        Some("##WebService")
    );
    assert_eq!(
        a("service1").operation_ref.unwrap().local.to_string(),
        "lookup"
    );
    assert_eq!(node(b, "send").kind, NodeKind::SendTask);
    assert_eq!(a("send").implementation.as_deref(), Some("##WebService"));
    assert_eq!(a("send").message_ref.unwrap().local.to_string(), "request");
    assert_eq!(a("send").operation_ref.unwrap().local.to_string(), "lookup");
    assert_eq!(node(b, "receive").kind, NodeKind::ReceiveTask);
    let r = a("receive");
    assert_eq!(r.instantiate, Some(true));
    assert_eq!(r.implementation.as_deref(), Some("##WebService"));
    assert_eq!(r.message_ref.unwrap().local.to_string(), "request");
    assert_eq!(r.operation_ref.unwrap().local.to_string(), "lookup");
    assert_eq!(node(b, "call").kind, NodeKind::CallActivity);
    let Some(NodeDetails::CallActivity { called_element }) = &node(b, "call").details else {
        panic!("call details")
    };
    assert_eq!(
        called_element.as_ref().unwrap().local.to_string(),
        "process"
    );
    assert_eq!(node(b, "event").kind, NodeKind::SubProcess);
    assert_eq!(a("event").triggered_by_event, Some(true));
}

#[test]
fn activity_family_imports_typed_values_and_reimports_equal_after_export() {
    let first = prepare_import(FIXTURE.as_bytes(), "m3").unwrap();
    check(&first);
    let out = export(&first);
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    for needle in [
        "isForCompensation=\"true\"",
        "startQuantity=\"3\"",
        "completionQuantity=\"2\"",
        "default=\"toUser\"",
        "triggeredByEvent=\"true\"",
        "instantiate=\"true\"",
        "<userTask",
        "<serviceTask",
        "<sendTask",
        "<receiveTask",
        "<callActivity",
        "<task ",
    ] {
        assert!(xml.contains(needle), "missing {needle} in export");
    }
    // Exported bytes are XSD-checked by scripts/check_bpmn20_xsd.py (see manifest).
    let _ = std::fs::write(
        concat!(env!("CARGO_TARGET_TMPDIR"), "/m3-activities-export.bpmn"),
        &out.bytes,
    );
    let again = prepare_import(&out.bytes, "again").unwrap();
    check(&again);
    for id in [
        "plain", "user", "service1", "send", "receive", "call", "event",
    ] {
        assert_eq!(node(&first, id).activity, node(&again, id).activity, "{id}");
        assert_eq!(node(&first, id).kind, node(&again, id).kind, "{id}");
    }
}

#[test]
fn non_numeric_quantity_is_preserved_not_typed() {
    for (from, to) in [
        ("startQuantity=\"3\"", "startQuantity=\"abc\""),
        ("completionQuantity=\"2\"", "completionQuantity=\"x1\""),
    ] {
        let input = FIXTURE.replace(from, to);
        let b = prepare_import(input.as_bytes(), "bad").unwrap();
        assert!(!b.report.editable, "{to}");
        assert_eq!(export(&b).bytes, input.as_bytes());
    }
}

#[test]
fn dangling_default_is_refused_with_reference_error() {
    let input = FIXTURE.replace("default=\"toUser\"", "default=\"nowhere\"");
    let e = prepare_import(input.as_bytes(), "bad").unwrap_err();
    assert!(format!("{e:?}").contains("Reference"), "{e:?}");
}
