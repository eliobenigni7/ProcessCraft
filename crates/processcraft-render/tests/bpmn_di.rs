use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
use processcraft_core::bpmn_model::EventReference;
use processcraft_core::model::*;
use processcraft_render::{Appearance, BpmnScene, TextEngine};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn local(s: &str) -> EventReference {
    EventReference::Local(id(s))
}
fn shape(name: &str, element: &str, y: f64) -> Shape {
    Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        id: id(name),
        element: id(element),
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        bounds: Bounds {
            x: 100.0,
            y,
            width: 160.0,
            height: 30.0,
        },
        label: None,
        label_visibility: None,
        style: None,
    }
}
fn participant(name: &str) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(name),
        name: Some(name.into()),
        process: None,
    }
}
fn flow(name: &str, kind: FlowKind) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind,
        source: Some(local("buyer")),
        target: Some(local("seller")),
        name: None,
        documentation: String::new().into(),
        condition: None,
    }
}
fn doc() -> BpmnDocument {
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.choreographies.push(Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: vec![participant("buyer"), participant("seller")],
        message_flows: vec![flow("request", FlowKind::Message)],
        activities: vec![ChoreographyActivity {
            loop_type: None,
            details: None,
            flow_metadata: None,
            correlation_keys: vec![],
            base: None,
            id: id("task"),
            name: Some("Order".into()),
            participant_refs: vec![local("buyer"), local("seller")],
            initiating_participant: Some(local("buyer")),
            message_flow_refs: vec![local("request")],
        }],
        sequence_flows: vec![],
    });
    let mut task = shape("taskShape", "task", 100.0);
    task.bounds.height = 100.0;
    let mut band = shape("buyerBand", "buyer", 100.0);
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(id("taskShape")));
    band.is_message_visible = Some(false);
    let mut other = shape("sellerBand", "seller", 170.0);
    other.participant_band_kind = Some(ParticipantBandKind::BottomNonInitiating);
    other.choreography_activity_shape = Some(DiReference::Local(id("taskShape")));
    other.is_message_visible = Some(false);
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        name: Some("Choreo".into()),
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("choreo"),
        shapes: vec![task, band, other],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: None,
            id: id("requestEdge"),
            element: id("request"),
            source_element: None,
            target_element: None,
            waypoints: vec![Point { x: 100.0, y: 130.0 }, Point { x: 260.0, y: 170.0 }],
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
fn diagram_renders_with_plane_and_shapes() {
    let d = doc();
    let s = BpmnScene::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    // BPMNDiagram/BPMNPlane: plane element resolves to the choreography, shapes hit-test to model keys
    let hit = s.hit_test(180.0, 115.0, 2.0).unwrap();
    assert_eq!(hit.key.document_id, d.id);
    assert!(svg(&d).contains("Order"));
}

#[test]
fn is_message_visible_draws_envelope_on_band() {
    let base = svg(&doc());
    let mut d = doc();
    d.diagrams[0].shapes[1].is_message_visible = Some(true);
    let with = svg(&d);
    assert_ne!(base, with, "visible message adds an envelope");
    d.diagrams[0].shapes[1].is_message_visible = Some(false);
    assert_eq!(svg(&d), base);
}

#[test]
fn participant_band_kind_changes_band_notation() {
    let base = svg(&doc());
    let mut d = doc();
    d.diagrams[0].shapes[1].participant_band_kind = Some(ParticipantBandKind::TopNonInitiating);
    assert_ne!(
        svg(&d),
        base,
        "initiating vs non-initiating band fill differs"
    );
}

#[test]
fn choreography_activity_shape_attaches_band_to_owner() {
    let base = svg(&doc());
    let mut d = doc();
    d.diagrams[0].shapes[1].choreography_activity_shape = None;
    assert_ne!(
        svg(&d),
        base,
        "a band without its owner shape is not drawn as a band"
    );
}

#[test]
fn message_visible_kind_draws_message_flow_envelope() {
    let base = svg(&doc());
    let mut d = doc();
    d.diagrams[0].edges[0].message_visible_kind = Some(MessageVisibleKind::Initiating);
    let initiating = svg(&d);
    assert_ne!(initiating, base);
    d.diagrams[0].edges[0].message_visible_kind = Some(MessageVisibleKind::NonInitiating);
    assert_ne!(svg(&d), initiating, "filled vs unfilled envelope");
}
