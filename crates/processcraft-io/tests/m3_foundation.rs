//! M3 foundation family: import/export value assertions on pinned fixtures.
use processcraft_core::bpmn_activity::{GlobalTaskKind, ProcessType};
use processcraft_core::bpmn_model::{
    AssociationDirection, EventGatewayType, GatewayDirection, NodeDetails,
};
use processcraft_core::model::{FlowKind, ProjectData};
use processcraft_io::interchange::{ExportedBpmn, PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const GATEWAYS: &[u8] = include_bytes!("../../../tests/fixtures/bpmn20-schema/gateways.bpmn");
const ARTIFACTS: &[u8] = include_bytes!("../../../tests/fixtures/bpmn20-schema/artifacts.bpmn");
const CORRELATION: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/activity-correlation.bpmn");
const CALLABLE: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/m3-foundation/callable-name-implementation.bpmn");

fn export(b: &PreparedImport) -> ExportedBpmn {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap()
}

/// Import, export, re-import; also drop the export under target/ for the XSD cross-check script.
fn round_trip(bytes: &[u8], name: &str) -> (PreparedImport, PreparedImport) {
    let first = prepare_import(bytes, name).unwrap();
    assert!(first.report.editable, "{:?}", first.report.limitations);
    let out = export(&first);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/m3-foundation");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.bpmn")), &out.bytes).unwrap();
    let again = prepare_import(&out.bytes, name).unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    (first, again)
}

fn gateway(p: &PreparedImport, id: &str) -> (Option<GatewayDirection>, Option<EventGatewayType>) {
    let node = p.document.processes[0]
        .nodes
        .iter()
        .find(|n| n.id.to_string() == id)
        .unwrap();
    match &node.details {
        Some(NodeDetails::Gateway {
            direction,
            event_gateway_type,
            ..
        }) => (*direction, *event_gateway_type),
        other => panic!("gateway details expected, got {other:?}"),
    }
}

#[test]
fn gateway_direction_and_event_gateway_type_values_survive_round_trip() {
    let (first, again) = round_trip(GATEWAYS, "gateways");
    for p in [&first, &again] {
        assert_eq!(gateway(p, "choose").0, Some(GatewayDirection::Diverging));
        assert_eq!(gateway(p, "complex").0, Some(GatewayDirection::Mixed));
        assert_eq!(gateway(p, "event").1, Some(EventGatewayType::Parallel));
    }
    let xml = String::from_utf8(export(&first).bytes).unwrap();
    assert!(xml.contains("gatewayDirection=\"Diverging\""));
    assert!(xml.contains("gatewayDirection=\"Mixed\""));
    assert!(xml.contains("eventGatewayType=\"Parallel\""));
}

#[test]
fn association_direction_value_survives_round_trip() {
    let (first, again) = round_trip(ARTIFACTS, "artifacts");
    for p in [&first, &again] {
        let flow = p.document.processes[0]
            .flows
            .iter()
            .find(|f| f.kind == FlowKind::Association)
            .unwrap();
        assert_eq!(flow.association_direction, Some(AssociationDirection::One));
    }
    let xml = String::from_utf8(export(&first).bytes).unwrap();
    assert!(xml.contains("associationDirection=\"One\""));
}

#[test]
fn process_type_and_category_value_ref_survive_round_trip() {
    let (first, again) = round_trip(CORRELATION, "activity-correlation");
    for p in [&first, &again] {
        let process = &p.document.processes[0];
        assert_eq!(
            process.activity_metadata.as_ref().unwrap().process_type,
            Some(ProcessType::Private)
        );
        let task = process
            .nodes
            .iter()
            .find(|n| n.id.to_string() == "task")
            .unwrap();
        let refs = &task.flow_metadata.as_ref().unwrap().category_value_refs;
        assert_eq!(refs.len(), 1);
        assert!(format!("{:?}", refs[0]).contains("categoryValue"));
    }
    let xml = String::from_utf8(export(&first).bytes).unwrap();
    assert!(xml.contains("processType=\"Private\""));
    assert!(xml.contains("categoryValueRef>"));
}

#[test]
fn callable_process_name_and_global_task_implementation_survive_round_trip() {
    let (first, again) = round_trip(CALLABLE, "callable-name-implementation");
    for p in [&first, &again] {
        assert_eq!(
            p.document.processes[0].name.as_deref(),
            Some("Callable process")
        );
        assert_eq!(
            p.document.processes[0]
                .activity_metadata
                .as_ref()
                .unwrap()
                .process_type,
            Some(ProcessType::Public)
        );
        let implementations: Vec<_> = p
            .document
            .global_tasks
            .iter()
            .filter_map(|t| match &t.kind {
                GlobalTaskKind::BusinessRule { implementation }
                | GlobalTaskKind::User { implementation, .. } => implementation.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(implementations, vec!["##unspecified", "##WebService"]);
    }
    let xml = String::from_utf8(export(&first).bytes).unwrap();
    assert!(xml.contains("name=\"Callable process\""));
    assert!(xml.contains("implementation=\"##unspecified\""));
    assert!(xml.contains("implementation=\"##WebService\""));
}
