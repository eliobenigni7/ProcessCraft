use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyLoopType,
    },
    model::{Bounds, BpmnDocument, Diagram, Participant, ParticipantBandKind, Shape},
};
use processcraft_render::{Appearance, BpmnScene, TextEngine};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(id(s))
}
fn shape(name: &str, element: &str, y: f64, h: f64) -> Shape {
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
            x: 0.0,
            y,
            width: 260.0,
            height: h,
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
/// One choreography task with explicit top/bottom participant bands.
fn document(
    loop_type: Option<ChoreographyLoopType>,
    details: Option<ChoreographyActivityDetails>,
) -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
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
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            loop_type,
            details,
            flow_metadata: None,
            correlation_keys: vec![],
            base: None,
            id: id("exchange"),
            name: Some("Exchange".into()),
            participant_refs: vec![local("buyer"), local("seller")],
            initiating_participant: Some(local("buyer")),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![],
    });
    let mut top = shape("buyer_band", "buyer", 0.0, 24.0);
    top.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    top.choreography_activity_shape = Some(processcraft_core::model::DiReference::Local(id(
        "exchange_shape",
    )));
    let mut bottom = shape("seller_band", "seller", 116.0, 24.0);
    bottom.participant_band_kind = Some(ParticipantBandKind::BottomNonInitiating);
    bottom.choreography_activity_shape = top.choreography_activity_shape.clone();
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("choreo"),
        shapes: vec![shape("exchange_shape", "exchange", 0.0, 140.0), top, bottom],
        edges: vec![],
    });
    d
}
fn render(d: &BpmnDocument) -> (String, usize, Vec<String>) {
    let s = BpmnScene::build(
        d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    (s.scene.to_svg(), s.scene.paths().count(), s.diagnostics)
}

#[test]
fn choreography_loop_type_marker_is_drawn_per_value() {
    let (none_svg, none_paths, diagnostics) = render(&document(None, None));
    assert!(!diagnostics.iter().any(|v| v.contains("too small")));
    let (explicit_none_svg, ..) = render(&document(Some(ChoreographyLoopType::None), None));
    assert_eq!(explicit_none_svg, none_svg, "loopType None draws no marker");
    let mut svgs = vec![none_svg];
    for loop_type in [
        ChoreographyLoopType::Standard,
        ChoreographyLoopType::MultiInstanceSequential,
        ChoreographyLoopType::MultiInstanceParallel,
    ] {
        let (svg, paths, diagnostics) = render(&document(Some(loop_type), None));
        assert!(!diagnostics.iter().any(|v| v.contains("too small")));
        assert!(paths > none_paths, "{loop_type:?} adds marker geometry");
        assert!(!svgs.contains(&svg), "{loop_type:?} marker is distinct");
        svgs.push(svg);
    }
}

#[test]
fn call_choreography_uses_the_thick_border_notation() {
    let (task_svg, ..) = render(&document(None, None));
    let call = ChoreographyActivityDetails::Call {
        participant_associations: vec![],
        called_choreography_ref: None,
    };
    let (call_svg, _, diagnostics) = render(&document(None, Some(call)));
    assert_ne!(call_svg, task_svg, "call border differs from a plain task");
    assert!(call_svg.contains("Exchange"));
    assert!(!diagnostics.iter().any(|v| v.contains("Unknown")));
}

#[test]
fn sub_choreography_collapsed_shows_plus_marker_and_expanded_does_not() {
    use processcraft_core::bpmn_collaboration::ChoreographyGraph;
    let sub = || ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph::default()),
        artifacts: vec![],
    };
    let build = |expanded: Option<bool>| {
        let mut d = document(None, Some(sub()));
        d.diagrams[0].shapes[0].is_expanded = expanded;
        d
    };
    let (_, task_paths, _) = render(&document(None, None));
    let (collapsed_svg, collapsed_paths, collapsed_diag) = render(&build(Some(false)));
    let (expanded_svg, expanded_paths, _) = render(&build(Some(true)));
    // Collapsed draws the "+" sub-process marker; expanded does not.
    assert!(
        collapsed_paths > expanded_paths,
        "collapsed {collapsed_paths} vs expanded {expanded_paths}"
    );
    assert_ne!(collapsed_svg, expanded_svg);
    // The sub-choreography keeps the task geometry otherwise (same outline paths).
    assert_eq!(expanded_paths, task_paths);
    assert!(collapsed_svg.contains("Exchange"));
    assert!(!collapsed_diag.iter().any(|v| v.contains("Unknown")));
    // Hit-testing finds the activity inside its bounds.
    let scene = BpmnScene::build(
        &build(Some(false)),
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert_eq!(
        scene.hit_test(130.0, 70.0, 0.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("exchange")
    );
}
