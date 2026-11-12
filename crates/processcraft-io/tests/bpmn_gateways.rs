use processcraft_core::{
    bpmn_model::{EventGatewayType, GatewayDirection, NodeDetails},
    model::{NodeKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/gateways/gateways-all.bpmn");

fn export(b: &PreparedImport) -> Vec<u8> {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap().bytes
}

/// (kind, default, direction, instantiate, eventGatewayType) of node `name`.
fn gateway(
    b: &PreparedImport,
    name: &str,
) -> (
    NodeKind,
    Option<String>,
    Option<GatewayDirection>,
    Option<bool>,
    Option<EventGatewayType>,
) {
    let node = b.document.processes[0]
        .nodes
        .iter()
        .find(|n| n.id.to_string() == name)
        .unwrap();
    let Some(NodeDetails::Gateway {
        default_flow,
        direction,
        instantiate,
        event_gateway_type,
        ..
    }) = &node.details
    else {
        panic!("{name} has no gateway details")
    };
    (
        node.kind,
        default_flow.as_ref().map(|d| d.to_string()),
        *direction,
        *instantiate,
        *event_gateway_type,
    )
}

fn assert_values(b: &PreparedImport) {
    assert!(b.report.editable);
    assert_eq!(
        gateway(b, "xor"),
        (
            NodeKind::ExclusiveGateway,
            Some("xf_default".into()),
            Some(GatewayDirection::Diverging),
            None,
            None
        )
    );
    assert_eq!(
        gateway(b, "inc"),
        (
            NodeKind::InclusiveGateway,
            Some("of_default".into()),
            Some(GatewayDirection::Diverging),
            None,
            None
        )
    );
    assert_eq!(
        gateway(b, "par"),
        (
            NodeKind::ParallelGateway,
            None,
            Some(GatewayDirection::Converging),
            None,
            None
        )
    );
    assert_eq!(
        gateway(b, "evx"),
        (
            NodeKind::EventBasedGateway,
            None,
            Some(GatewayDirection::Unspecified),
            Some(false),
            Some(EventGatewayType::Exclusive)
        )
    );
    assert_eq!(
        gateway(b, "evp"),
        (
            NodeKind::EventBasedGateway,
            None,
            Some(GatewayDirection::Mixed),
            Some(true),
            Some(EventGatewayType::Parallel)
        )
    );
}

#[test]
fn gateway_fixture_imports_typed_values() {
    assert_values(&prepare_import(FIXTURE, "gateways").unwrap());
}

#[test]
fn gateway_fixture_export_reimports_equal_and_keeps_attributes() {
    let first = prepare_import(FIXTURE, "gateways").unwrap();
    let bytes = export(&first);
    // Output for `python scripts/check_bpmn20_xsd.py target/m3-gateways-export.bpmn`.
    let _ = std::fs::write(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/m3-gateways-export.bpmn"
        ),
        &bytes,
    );
    let text = String::from_utf8(bytes.clone()).unwrap();
    for needle in [
        "gatewayDirection=\"Diverging\"",
        "gatewayDirection=\"Converging\"",
        "gatewayDirection=\"Unspecified\"",
        "gatewayDirection=\"Mixed\"",
        "default=\"xf_default\"",
        "default=\"of_default\"",
        "instantiate=\"true\"",
        "eventGatewayType=\"Parallel\"",
        "eventGatewayType=\"Exclusive\"",
        "<inclusiveGateway",
        "<parallelGateway",
    ] {
        assert!(text.contains(needle), "{needle} missing from export");
    }
    let again = prepare_import(&bytes, "again").unwrap();
    assert_values(&again);
    assert_eq!(
        first.document.processes[0].nodes,
        again.document.processes[0].nodes
    );
}

#[test]
fn dangling_gateway_defaults_are_refused() {
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap();
    for bad in [
        text.replace("default=\"of_default\"", "default=\"missing\""),
        text.replace("default=\"xf_default\"", "default=\"missing\""),
    ] {
        let err = prepare_import(bad.as_bytes(), "bad").unwrap_err();
        assert!(err.detail.contains("missing"), "{err:?}");
    }
}
