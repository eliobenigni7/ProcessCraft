//! M3 process/collaboration family: pool and message-flow names reach the scene.
use processcraft_core::{
    bpmn_activity::ModelReference,
    model::{
        Bounds, BpmnDocument, Collaboration, Edge, Flow, FlowKind, Participant, Point, Process,
        Shape,
    },
};
use processcraft_render::{Appearance, BpmnScene, TextEngine};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn pool(name: &str, y: f64) -> (Participant, Shape) {
    let participant = Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(&format!("{name}_participant")),
        name: Some(format!("{name} Pool")),
        process: None,
    };
    let shape = Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: Some(true),
        is_marker_visible: None,
        is_expanded: None,
        id: id(&format!("{name}_shape")),
        element: id(&format!("{name}_participant")),
        bounds: Bounds {
            x: 0.0,
            y,
            width: 400.0,
            height: 120.0,
        },
        label: None,
        style: None,
    };
    (participant, shape)
}
fn document(flow_name: Option<&str>) -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let (buyer, buyer_shape) = pool("Buyer", 0.0);
    let (seller, seller_shape) = pool("Seller", 220.0);
    d.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        participants: vec![buyer, seller],
        flows: vec![Flow {
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            id: id("order"),
            kind: FlowKind::Message,
            source: Some(ModelReference::Local(id("Buyer_participant"))),
            target: Some(ModelReference::Local(id("Seller_participant"))),
            name: flow_name.map(Into::into),
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        }],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    d.diagrams.push(processcraft_core::model::Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("collab"),
        shapes: vec![buyer_shape, seller_shape],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: None,
            id: id("order_edge"),
            element: id("order"),
            source_element: Some(id("Buyer_participant")),
            target_element: Some(id("Seller_participant")),
            waypoints: vec![Point { x: 200.0, y: 120.0 }, Point { x: 200.0, y: 220.0 }],
            label: None,
            label_visibility: None,
            style: None,
        }],
    });
    d
}
fn svg(d: &BpmnDocument) -> String {
    BpmnScene::build(
        d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap()
    .scene
    .to_svg()
}

#[test]
fn participant_name_renders_as_pool_label() {
    let svg = svg(&document(None));
    assert!(svg.contains("Buyer Pool"), "{svg}");
    assert!(svg.contains("Seller Pool"));
    let mut renamed = document(None);
    renamed.collaboration.as_mut().unwrap().participants[0].name = Some("Customer".into());
    let svg = self::svg(&renamed);
    assert!(svg.contains("Customer") && !svg.contains("Buyer Pool"));
}

#[test]
fn message_flow_name_renders_as_edge_label() {
    assert!(!svg(&document(None)).contains("Order request"));
    let svg = svg(&document(Some("Order request")));
    assert!(svg.contains("Order request"), "{svg}");
}
