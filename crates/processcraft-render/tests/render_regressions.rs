//! M3 round 3 render cells: tMessage/@name (envelope label) and tLinkEventDefinition/@name.
use processcraft_core::bpmn_activity::ModelReference;
use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
use processcraft_core::bpmn_model::{EventDefinition, EventReference, EventResource, NodeDetails};
use processcraft_core::model::*;
use processcraft_render::{Appearance, BpmnScene, BpmnViewport, Label, TextEngine, Viewport};

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
fn message_flow() -> Flow {
    Flow {
        message_ref: Some(ModelReference::Local(id("msg"))),
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id("mf"),
        kind: FlowKind::Message,
        source: Some(ModelReference::Local(id("buyer"))),
        target: Some(ModelReference::Local(id("seller"))),
        name: None,
        documentation: String::new().into(),
        condition: None,
    }
}
fn diagram(plane: &str, shapes: Vec<Shape>, edges: Vec<Edge>) -> Diagram {
    Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id(plane),
        shapes,
        edges,
    }
}
fn process(nodes: Vec<Node>) -> Process {
    Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes,
        lanes: vec![],
        flows: vec![],
    }
}
fn message(name: &str) -> EventResource {
    EventResource::Message {
        id: id("msg"),
        base: None,
        name: Some(name.into()),
        item_ref: None,
    }
}
fn scene(d: &BpmnDocument, appearance: Appearance) -> BpmnScene {
    BpmnScene::build(
        d,
        &id("diagram"),
        appearance,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap()
}
fn label_at<'a>(s: &'a BpmnScene, text: &str) -> &'a Label {
    let found: Vec<_> = s
        .scene
        .labels()
        .iter()
        .filter(|l| l.layout.text() == text)
        .collect();
    assert_eq!(found.len(), 1, "expected exactly one label {text:?}");
    found[0]
}
/// Asserts the label starts to the right of the envelope centre (cx, cy), vertically aligned.
fn assert_beside(s: &BpmnScene, text: &str, cx: f64, cy: f64) {
    let l = label_at(s, text);
    let (cx, cy) = ((cx - s.origin.0) as f32, (cy - s.origin.1) as f32);
    assert!(l.x > cx + 10.0 && l.x < cx + 30.0, "x {} vs {cx}", l.x);
    assert!((l.y - cy).abs() < 16.0, "y {} vs {cy}", l.y);
}

fn choreography_doc() -> BpmnDocument {
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
        message_flows: vec![message_flow()],
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
            message_flow_refs: vec![local("mf")],
        }],
        sequence_flows: vec![],
    });
    let mut task = shape("taskShape", "task", 100.0);
    task.bounds.height = 100.0;
    let mut band = shape("buyerBand", "buyer", 100.0);
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(id("taskShape")));
    band.is_message_visible = Some(true);
    let mut other = shape("sellerBand", "seller", 170.0);
    other.participant_band_kind = Some(ParticipantBandKind::BottomNonInitiating);
    other.choreography_activity_shape = Some(DiReference::Local(id("taskShape")));
    other.is_message_visible = Some(false);
    d.diagrams
        .push(diagram("choreo", vec![task, band, other], vec![]));
    d
}

// tMessage/@name: the Message name labels the envelope decorator of a participant band
// (BPMN 2.0.2 section 12.3.2, Choreography Participant Bands), beside the envelope.
#[test]
fn message_name_labels_band_envelope_in_both_themes() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let mut d = choreography_doc();
        assert!(
            !scene(&d, appearance).scene.to_svg().contains("Invoice"),
            "no Message resource"
        );
        d.event_resources.push(message("Invoice"));
        let named = scene(&d, appearance);
        assert!(named.scene.to_svg().contains("Invoice"));
        // top band spans y 100..130; the envelope centre is 13 above it, at the band's mid x
        assert_beside(&named, "Invoice", 180.0, 87.0);
        d.event_resources[0] = message("");
        assert!(!scene(&d, appearance).scene.to_svg().contains("Invoice"));
    }
}

fn collaboration_doc(visible: Option<MessageVisibleKind>, name: Option<&str>) -> BpmnDocument {
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.processes.push(process(vec![]));
    d.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        participants: vec![participant("buyer"), participant("seller")],
        flows: vec![message_flow()],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    if let Some(name) = name {
        d.event_resources.push(message(name));
    }
    let mut a = shape("a", "buyer", 0.0);
    a.bounds.height = 100.0;
    let mut b = shape("b", "seller", 220.0);
    b.bounds.height = 100.0;
    d.diagrams.push(diagram(
        "collab",
        vec![a, b],
        vec![Edge {
            di_metadata: None,
            message_visible_kind: visible,
            id: id("mfEdge"),
            element: id("mf"),
            source_element: None,
            target_element: None,
            waypoints: vec![Point { x: 180.0, y: 100.0 }, Point { x: 180.0, y: 220.0 }],
            label: None,
            label_visibility: None,
            style: None,
        }],
    ));
    d
}

// tMessage/@name on a message-flow envelope: static scene and viewport raster both draw it.
#[test]
fn message_name_labels_flow_envelope_static_and_viewport() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let visible = Some(MessageVisibleKind::Initiating);
        let named = scene(&collaboration_doc(visible, Some("Invoice")), appearance);
        assert_beside(&named, "Invoice", 180.0, 160.0);
        // no envelope, no label; no message name, no label
        let no_envelope = scene(&collaboration_doc(None, Some("Invoice")), appearance);
        assert!(!no_envelope.scene.to_svg().contains("Invoice"));
        let no_name = scene(&collaboration_doc(visible, None), appearance);
        assert!(!no_name.scene.to_svg().contains("Invoice"));
        // viewport path: the labelled view has more ink than the unlabelled one
        let ink = |d: &BpmnDocument| {
            let view = Viewport {
                x: 100.0,
                y: 130.0,
                width: 200.0,
                height: 60.0,
                zoom: 1.0,
            };
            let v = BpmnViewport::build(
                d,
                &id("diagram"),
                appearance,
                &mut TextEngine::new(),
                "Noto Sans",
                view,
            )
            .unwrap();
            let background = v.pixels.pixels()[0];
            v.pixels
                .pixels()
                .iter()
                .filter(|p| **p != background)
                .count()
        };
        assert!(
            ink(&collaboration_doc(visible, Some("Invoice")))
                > ink(&collaboration_doc(visible, None))
        );
    }
}

fn link_doc(name: Option<&str>, link: &str) -> BpmnDocument {
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    let event = Node {
        flow_metadata: None,
        item_aware: None,
        activity: None,
        details: Some(NodeDetails::Event {
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
            definitions: vec![EventDefinition::Link {
                id: None,
                base: None,
                name: link.into(),
                sources: vec![],
                target: None,
            }],
            attached_to: None,
            cancel_activity: None,
        }),
        loop_characteristics: None,
        io_specification: None,
        id: id("evt"),
        name: name.map(Into::into),
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::IntermediateCatchEvent,
        owner: String::new(),
        contact: String::new(),
    };
    d.processes.push(process(vec![event]));
    let mut s = shape("evtShape", "evt", 100.0);
    s.bounds = Bounds {
        x: 100.0,
        y: 100.0,
        width: 36.0,
        height: 36.0,
    };
    d.diagrams.push(diagram("process", vec![s], vec![]));
    d
}

// tLinkEventDefinition/@name: BPMN 2.0.2 section 10.5.5 (Link Event Definition, Figures
// 10.83/10.84) - the shared link name labels an otherwise unnamed Link event; an own name wins.
#[test]
fn link_definition_name_labels_unnamed_event_in_both_themes() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for unnamed in [None, Some("")] {
            let s = scene(&link_doc(unnamed, "GoToCheckout"), appearance);
            let l = label_at(&s, "GoToCheckout");
            // same placement as the event's own name label: anchored at the event shape
            let named = scene(&link_doc(Some("GoToCheckout"), "Other"), appearance);
            let n = label_at(&named, "GoToCheckout");
            assert_eq!((l.x, l.y), (n.x, n.y));
            assert!(s.hits.iter().any(|h| h.is_label
                && h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("evt")));
        }
        let own = scene(&link_doc(Some("Own name"), "GoToCheckout"), appearance);
        let svg = own.scene.to_svg();
        assert!(svg.contains("Own name") && !svg.contains("GoToCheckout"));
    }
}

#[test]
fn overlay_label_ink_contrasts_with_every_ramp_colour_by_at_least_4_5() {
    use processcraft_core::overlay::OverlayScale;
    use processcraft_render::{contrast_ratio, overlay_fill, overlay_ink};
    let mut worst = f64::MAX;
    for appearance in [
        Appearance::Light,
        Appearance::Dark,
        Appearance::CanvasLight,
        Appearance::CanvasDark,
    ] {
        let ink = overlay_ink(appearance);
        for (scale, count) in [
            (OverlayScale::Sequential, 5),
            (OverlayScale::Categorical, 8),
        ] {
            for bucket in 0..count {
                let ratio = contrast_ratio(ink, overlay_fill(appearance, scale, bucket));
                worst = worst.min(ratio);
                assert!(
                    ratio >= 4.5,
                    "{appearance:?} {scale:?} bucket {bucket}: {ratio:.2}"
                );
            }
        }
    }
    eprintln!("worst overlay label contrast: {worst:.2}");
}

#[test]
fn overlay_sequential_ramps_are_monotone_in_luminance() {
    use processcraft_core::overlay::OverlayScale;
    use processcraft_render::{contrast_ratio, overlay_fill};
    // Contrast against black moves one way along the ramp, so steps read as "more".
    for appearance in [Appearance::Light, Appearance::Dark] {
        let ratios: Vec<f64> = (0..5)
            .map(|b| {
                contrast_ratio(
                    overlay_fill(appearance, OverlayScale::Sequential, b),
                    [0, 0, 0],
                )
            })
            .collect();
        assert!(
            ratios.windows(2).all(|w| w[0] > w[1]) || ratios.windows(2).all(|w| w[0] < w[1]),
            "{appearance:?}: {ratios:?}"
        );
    }
}
