use processcraft_core::model::{
    Bounds, BpmnDocument, Capability, Diagram, Node, NodeKind, Process, Shape, Style,
};
use processcraft_render::{Appearance, BpmnScene, TextEngine};
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn fixture() -> BpmnDocument {
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
        name: Some(String::new()),
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![Node {
            flow_metadata: None,
            item_aware: None,
            activity: None,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            id: id("task"),
            name: Some("Review".into()),
            documentation: String::new().into(),
            base_extras: None,
            kind: NodeKind::UserTask,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id("shape"),
            element: id("task"),
            bounds: Bounds {
                x: -25.0,
                y: 10.0,
                width: 120.0,
                height: 80.0,
            },
            label: None,
            style: Some(Style {
                fill: Some("#123456".into()),
                stroke: Some("#aabbcc".into()),
                font: Some("#102030".into()),
            }),
        }],
        edges: vec![],
    });
    d
}
fn build(d: &BpmnDocument, a: Appearance) -> BpmnScene {
    BpmnScene::build(d, &id("diagram"), a, &mut TextEngine::new(), "Noto Sans").unwrap()
}
#[test]
fn scene_uses_semantic_element_keys() {
    let d = fixture();
    let s = build(&d, Appearance::Light);
    let hit = s.hit_test(0.0, 20.0, 2.0).unwrap();
    assert_eq!(hit.key.document_id, d.id);
    assert_eq!(
        hit.key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("task")
    );
    assert_eq!(
        hit.diagram_element_id,
        id::<processcraft_core::ids::ElementSelector>("shape")
    );
    assert_eq!(s.origin, (-41.0, -6.0));
}
#[test]
fn selection_is_excluded_from_document_scene() {
    let s = build(&fixture(), Appearance::Light);
    let before = s.scene.to_svg();
    let _ = s.hit_test(0.0, 20.0, 2.0);
    assert_eq!(before, s.scene.to_svg());
}
#[test]
fn theme_keeps_document_identity() {
    let d = fixture();
    let before = serde_json::to_vec(&d).unwrap();
    let light = build(&d, Appearance::Light);
    let dark = build(&d, Appearance::Dark);
    assert_ne!(light.scene.to_svg(), dark.scene.to_svg());
    assert_eq!(before, serde_json::to_vec(&d).unwrap());
    assert_eq!(light.hits[0].key, dark.hits[0].key);
}
#[test]
fn explicit_imported_colors_are_preserved() {
    let d = fixture();
    for a in [Appearance::Light, Appearance::Dark] {
        let svg = build(&d, a).scene.to_svg();
        for c in ["#123456", "#aabbcc", "#102030"] {
            assert!(svg.contains(c), "{c}");
        }
    }
}
#[test]
fn semantic_default_fills_follow_appearance_without_overriding_source() {
    for (kind, light, dark) in [
        (NodeKind::Task, "#fffcf0", "#36382d"),
        (NodeKind::StartEvent, "#f0f9f2", "#284435"),
        (NodeKind::EndEvent, "#fff3f0", "#4a302d"),
    ] {
        let mut d = fixture();
        d.processes[0].nodes[0].kind = kind;
        d.diagrams[0].shapes[0].style = None;
        assert!(build(&d, Appearance::Light).scene.to_svg().contains(light));
        assert!(build(&d, Appearance::Dark).scene.to_svg().contains(dark));
        d.diagrams[0].shapes[0].style = Some(Style {
            fill: Some("#123456".into()),
            stroke: None,
            font: None,
        });
        for appearance in [Appearance::Light, Appearance::Dark] {
            assert!(build(&d, appearance).scene.to_svg().contains("#123456"));
        }
    }
}
#[test]
fn empty_multiple_and_missing_font() {
    let mut d = fixture();
    d.diagrams[0].shapes.clear();
    let s = build(&d, Appearance::Light);
    assert!(s.hits.is_empty());
    let mut second = d.diagrams[0].clone();
    second.id = id("other");
    d.diagrams.push(second);
    assert!(
        BpmnScene::build(
            &d,
            &id("missing"),
            Appearance::Light,
            &mut TextEngine::new(),
            "Noto Sans"
        )
        .is_err()
    );
    d = fixture();
    let s = BpmnScene::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Missing Font",
    )
    .unwrap();
    assert!(s.diagnostics.iter().any(|d| d.contains("Missing font")));
}
#[test]
fn preserved_source_is_not_presented_as_semantic_preview() {
    let mut d = fixture();
    d.capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    let s = build(&d, Appearance::Light);
    assert!(s.hits.is_empty());
    assert!(!s.diagnostics.is_empty());
}
#[test]
fn named_flow_without_di_label_is_visible() {
    let mut d = fixture();
    d.processes[0].flows.push(processcraft_core::model::Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("flow"),
        kind: processcraft_core::model::FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some("Approved".into()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(processcraft_core::model::Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("edge"),
        element: id("flow"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![
            processcraft_core::model::Point { x: 10., y: 100. },
            processcraft_core::model::Point { x: 90., y: 100. },
        ],
        label: None,
        style: None,
    });
    let s = build(&d, Appearance::Light);
    assert!(s.scene.to_svg().contains("Approved"));
}
#[test]
fn large_diagram_renders_viewport_without_full_allocation() {
    let mut d = fixture();
    let mut shape = d.diagrams[0].shapes[0].clone();
    shape.id = id("far_shape");
    shape.bounds.x = 900_000.;
    d.diagrams[0].shapes.push(shape);
    let s = build(&d, Appearance::Light);
    assert!(s.scene.rasterize(1.0).is_err());
    let x = (900_000. - s.origin.0) as f32;
    assert!(s.scene.rasterize_viewport(x, 0., 200., 200., 1.).is_ok());
}
#[test]
fn small_label_bounds_report_omission() {
    let mut d = fixture();
    d.diagrams[0].shapes[0].label = Some(processcraft_core::model::Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 0.,
            y: 0.,
            width: 20.,
            height: 5.,
        }),
        style: None,
    });
    let s = build(&d, Appearance::Light);
    assert!(
        s.diagnostics
            .iter()
            .any(|d| d.contains("exceeds its bounds"))
    );
    assert!(s.scene.to_svg().contains("Review"));
}
#[test]
fn many_dashed_edges_fit_path_budget() {
    let mut d = fixture();
    for i in 0..250 {
        let key: processcraft_core::ids::ElementIdentity = id(&format!("flow{i}"));
        d.processes[0].flows.push(processcraft_core::model::Flow {
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            id: key.clone(),
            kind: processcraft_core::model::FlowKind::Association,
            source: id("task"),
            target: id("task"),
            name: Some(String::new()),
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        });
        d.diagrams[0].edges.push(processcraft_core::model::Edge {
            di_metadata: None,
            message_visible_kind: None,
            id: id(&format!("edge{i}")),
            element: key.selector(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: vec![
                processcraft_core::model::Point { x: 0., y: i as f64 },
                processcraft_core::model::Point {
                    x: 600.,
                    y: i as f64,
                },
            ],
            label: None,
            style: None,
        });
    }
    assert_eq!(build(&d, Appearance::Light).hits.len(), 251);
}
#[test]
fn viewport_rebases_far_coordinates_before_float_conversion() {
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let mut far = d.diagrams[0].shapes[0].clone();
    far.id = id("far");
    far.bounds.x = 999_999_000.;
    far.bounds.y = 999_999_000.;
    d.diagrams[0].shapes.push(far);
    let v = Viewport {
        x: 999_998_990.,
        y: 999_998_990.,
        width: 150.,
        height: 120.,
        zoom: 1.0,
    };
    let scene = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        v,
    )
    .unwrap();
    assert_eq!((scene.pixels.width(), scene.pixels.height()), (150, 120));
    let hit = scene.hit_test(999_999_005., 999_999_005., 0.).unwrap();
    assert_eq!(
        hit.diagram_element_id,
        id::<processcraft_core::ids::ElementSelector>("far")
    );
    assert!(
        scene
            .pixels
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .any(|(i, p)| {
                let x = i % 150;
                let y = i / 150;
                (8..=13).contains(&x) && (8..=13).contains(&y) && p[0] < 180
            })
    );
}
#[test]
fn viewport_clips_polyline_without_intermediate_arrows_or_duplicate_label() {
    use processcraft_core::model::{Edge, Flow, FlowKind, Label, Point};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes.clear();
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("f"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some("A very long label".into()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("e"),
        element: id("f"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![
            Point { x: 10., y: 50. },
            Point { x: 50., y: 50. },
            Point { x: 50., y: 90. },
            Point { x: 200., y: 90. },
        ],
        label: Some(Label {
            di_metadata: None,
            id: None,
            style_ref: None,
            bounds: Some(Bounds {
                x: 10.,
                y: 5.,
                width: 20.,
                height: 5.,
            }),
            style: None,
        }),
        style: None,
    });
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 100.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert_eq!(s.hits.len(), 2);
    assert_eq!(
        s.diagnostics
            .iter()
            .filter(|d| d.contains("Label clipped"))
            .count(),
        1
    );
    let pix = s.pixels.data();
    let i = (46 * 100 + 45) * 4;
    assert!(pix[i] > 230, "unexpected intermediate arrow");
}
#[test]
fn viewport_omits_giant_shape_but_keeps_local_node() {
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let mut giant = d.diagrams[0].shapes[0].clone();
    giant.id = id("giant");
    giant.bounds = Bounds {
        x: -900_000_000.,
        y: -900_000_000.,
        width: 1_800_000_000.,
        height: 1_800_000_000.,
    };
    d.diagrams[0].shapes.push(giant);
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.,
            y: 0.,
            width: 150.,
            height: 120.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert!(s.hits.iter().all(|h| h.diagram_element_id != id::<processcraft_core::ids::ElementSelector>("giant")));
    assert!(
        s.diagnostics
            .iter()
            .any(|d| d.contains("giant") && d.contains("omitted"))
    );
}
#[test]
fn viewport_displays_explicit_shape_label_when_shape_is_offscreen() {
    use processcraft_core::model::Label;
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes[0].bounds.x = 500.;
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 10.,
            y: 10.,
            width: 100.,
            height: 40.,
        }),
        style: None,
    });
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Missing Font",
        Viewport {
            x: 0.,
            y: 0.,
            width: 150.,
            height: 100.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert!(s.diagnostics.iter().any(|d| d.contains("Missing font")));
    assert_eq!(
        s.hit_test(15., 15., 0.).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("task")
    );
}
#[test]
fn viewport_does_not_fallback_when_explicit_shape_label_is_offscreen() {
    use processcraft_core::model::Label;
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 500.,
            y: 500.,
            width: 100.,
            height: 40.,
        }),
        style: None,
    });
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Missing Font",
        Viewport {
            x: 0.,
            y: 0.,
            width: 150.,
            height: 100.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert!(!s.diagnostics.iter().any(|d| d.contains("Missing font")));
}
#[test]
fn viewport_displays_explicit_edge_label_when_edge_is_offscreen() {
    use processcraft_core::model::{Edge, Flow, FlowKind, Label, Point};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes.clear();
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("f"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some("Visible".into()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("e"),
        element: id("f"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 500., y: 500. }, Point { x: 600., y: 500. }],
        label: Some(Label {
            di_metadata: None,
            id: None,
            style_ref: None,
            bounds: Some(Bounds {
                x: 10.,
                y: 10.,
                width: 100.,
                height: 40.,
            }),
            style: None,
        }),
        style: None,
    });
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Missing Font",
        Viewport {
            x: 0.,
            y: 0.,
            width: 150.,
            height: 100.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert!(s.diagnostics.iter().any(|d| d.contains("Missing font")));
    assert_eq!(
        s.hit_test(15., 15., 0.).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("f")
    );
}
#[test]
fn viewport_clicks_external_label_of_visible_shape() {
    use processcraft_core::model::Label;
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 110.,
            y: 110.,
            width: 70.,
            height: 30.,
        }),
        style: None,
    });
    let s = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: -30.,
            y: 0.,
            width: 220.,
            height: 160.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert_eq!(
        s.hit_test(120., 120., 0.).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("task")
    );
}
#[test]
fn typed_timer_event_has_circle_hits_and_shared_vector_marker() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails, TimerKind};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::IntermediateCatchEvent;
    d.processes[0].nodes[0].details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Timer {
            base: None,
            id: None,
            kind: Some(TimerKind::Duration),
            expression: Some("PT5M".into()),
        }],
        attached_to: None,
        cancel_activity: None,
    });
    d.processes[0].nodes[0].name = Some(String::new());
    let timer = build(&d, Appearance::Light);
    let bounds = &d.diagrams[0].shapes[0].bounds;
    assert!(
        timer
            .hit_test(
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height / 2.0,
                0.0
            )
            .is_some()
    );
    assert!(
        timer
            .hit_test(bounds.x + 1.0, bounds.y + 1.0, 0.0)
            .is_none()
    );
    let svg = timer.scene.to_svg();
    assert!(svg.contains("#123456"));
    assert!(svg.contains("#aabbcc"));
    d.processes[0].nodes[0].details = None;
    assert_ne!(svg, build(&d, Appearance::Light).scene.to_svg());
}
#[test]
fn unsupported_event_definition_produces_export_diagnostic() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::IntermediateCatchEvent;
    d.processes[0].nodes[0].details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Terminate {
            base: None,
            id: None,
        }],
        attached_to: None,
        cancel_activity: None,
    });
    assert!(
        build(&d, Appearance::Light)
            .diagnostics
            .iter()
            .any(|s| s.contains("cannot be rendered"))
    );
}
#[test]
fn gateway_hit_is_diamond_and_task_markers_differ() {
    let mut d = fixture();
    d.processes[0].nodes[0].name = Some(String::new());
    d.processes[0].nodes[0].kind = NodeKind::InclusiveGateway;
    let b = &d.diagrams[0].shapes[0].bounds;
    let scene = build(&d, Appearance::Light);
    assert!(scene.hit_test(b.x + 1.0, b.y + 1.0, 0.0).is_none());
    assert!(
        scene
            .hit_test(b.x + b.width / 2.0, b.y + b.height / 2.0, 0.0)
            .is_some()
    );
    d.processes[0].nodes[0].kind = NodeKind::SendTask;
    let send = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].kind = NodeKind::ReceiveTask;
    let receive = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(send, receive);
}

#[test]
fn non_interrupting_boundary_and_compensation_have_distinct_shared_vectors() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut d = fixture();
    let mut activity = d.processes[0].nodes[0].clone();
    activity.id = id("activity");
    activity.name = Some(String::new());
    d.processes[0].nodes.push(activity);
    let node = &mut d.processes[0].nodes[0];
    node.name = Some(String::new());
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Compensate {
            base: None,
            id: None,
            wait_for_completion: None,
            activity_ref: None,
        }],
        attached_to: Some(id("activity")),
        cancel_activity: Some(false),
    });
    let non_interrupting = build(&d, Appearance::Light);
    assert!(non_interrupting.diagnostics.is_empty());
    let svg = non_interrupting.scene.to_svg();
    if let Some(NodeDetails::Event {
        cancel_activity, ..
    }) = &mut d.processes[0].nodes[0].details
    {
        *cancel_activity = Some(true);
    }
    let interrupting = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(svg, interrupting);
    assert!(svg.contains("#123456") && svg.contains("#aabbcc"));
}
#[test]
fn multiple_event_uses_one_marker_and_unsupported_kind_is_diagnosed() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::IntermediateCatchEvent;
    node.name = Some(String::new());
    node.details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![
            EventDefinition::Signal {
                base: None,
                id: None,
                signal_ref: None,
            },
            EventDefinition::Message {
                base: None,
                id: None,
                message_ref: None,
                operation_ref: None,
            },
        ],
        attached_to: None,
        cancel_activity: None,
    });
    let multiple = build(&d, Appearance::Light);
    assert!(multiple.diagnostics.is_empty());
    let svg = multiple.scene.to_svg();
    if let Some(NodeDetails::Event { definitions, .. }) = &mut d.processes[0].nodes[0].details {
        definitions.pop();
    }
    assert_ne!(svg, build(&d, Appearance::Light).scene.to_svg());
}
#[test]
fn nested_diagram_shows_children_and_collapsed_parent_hides_them() {
    use processcraft_core::{bpmn_model::NodeDetails, model::FlowContainer};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let mut child = d.processes[0].nodes[0].clone();
    child.id = id("child");
    child.name = Some("Nested task".into());
    let parent = &mut d.processes[0].nodes[0];
    parent.id = id("sub");
    parent.kind = NodeKind::SubProcess;
    parent.name = Some(String::new());
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![child],
            lanes: vec![],
            flows: vec![],
        }),
    });
    let parent_shape = &mut d.diagrams[0].shapes[0];
    parent_shape.element = id("sub");
    parent_shape.is_expanded = Some(true);
    parent_shape.bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 250.0,
    };
    let mut child_shape = parent_shape.clone();
    child_shape.id = id("child_shape");
    child_shape.element = id("child");
    child_shape.is_expanded = None;
    child_shape.bounds = Bounds {
        x: 80.0,
        y: 60.0,
        width: 120.0,
        height: 80.0,
    };
    d.diagrams[0].shapes.push(child_shape.clone());
    let expanded = build(&d, Appearance::Light);
    assert_eq!(
        expanded.hit_test(100.0, 100.0, 0.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("child")
    );
    assert!(expanded.scene.to_svg().contains("Nested task"));
    let expanded_svg = expanded.scene.to_svg();
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    let collapsed = build(&d, Appearance::Light);
    assert_eq!(
        collapsed.hit_test(100.0, 100.0, 0.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("sub")
    );
    assert!(!collapsed.scene.to_svg().contains("Nested task"));
    assert_ne!(expanded_svg, collapsed.scene.to_svg());
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 50.0,
            y: 40.0,
            width: 200.0,
            height: 160.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(
        !viewport
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("child"))
    );
    let mut child_diagram = d.diagrams[0].clone();
    child_diagram.id = id("child_diagram");
    child_diagram.plane = id("sub");
    child_diagram.shapes = vec![child_shape];
    d.diagrams.push(child_diagram);
    let sub = processcraft_render::BpmnScene::build(
        &d,
        &id("child_diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert_eq!(
        sub.hit_test(100.0, 100.0, 0.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("child")
    );
}
#[test]
fn nested_flow_and_called_activity_use_semantic_ids() {
    use processcraft_core::{
        bpmn_model::{NodeDetails, QNameRef},
        model::{Edge, Flow, FlowContainer, FlowKind, Point},
    };
    let mut d = fixture();
    let mut child = d.processes[0].nodes[0].clone();
    child.id = id("nested_a");
    child.name = Some(String::new());
    let mut called = child.clone();
    called.id = id("nested_b");
    called.kind = NodeKind::CallActivity;
    called.details = Some(NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: "urn:example".into(),
            local: id("called"),
        }),
    });
    let flow = Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("inner_flow"),
        kind: FlowKind::Sequence,
        source: id("nested_a"),
        target: id("nested_b"),
        name: Some("Inner".into()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    };
    let parent = &mut d.processes[0].nodes[0];
    parent.kind = NodeKind::SubProcess;
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![child, called],
            lanes: vec![],
            flows: vec![flow],
        }),
    });
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    let mut a = d.diagrams[0].shapes[0].clone();
    a.id = id("a_di");
    a.element = id("nested_a");
    a.is_expanded = None;
    a.bounds.x = 130.0;
    let mut b = a.clone();
    b.id = id("b_di");
    b.element = id("nested_b");
    b.is_expanded = Some(false);
    b.bounds.x = 300.0;
    d.diagrams[0].shapes.extend([a, b]);
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("inner_di"),
        element: id("inner_flow"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 250.0, y: 50.0 }, Point { x: 300.0, y: 50.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| hit.key.xml_id == id::<processcraft_core::ids::ElementSelector>("nested_b"))
    );
    assert!(
        scene.hits.iter().any(
            |hit| hit.key.xml_id == id::<processcraft_core::ids::ElementSelector>("inner_flow")
        )
    );
    assert!(scene.scene.to_svg().contains("Inner"));
    assert!(!scene.diagnostics.iter().any(|d| d.contains("Unknown")));
}
#[test]
fn loop_and_multi_instance_markers_use_distinct_shared_vectors() {
    use processcraft_core::bpmn_model::LoopCharacteristics;
    let mut d = fixture();
    d.processes[0].nodes[0].name = Some(String::new());
    let base = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::Standard {
        id: None,
        base: None,
        test_before: Some(false),
        condition: None,
        maximum: None,
    });
    let standard = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(base, standard);
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        id: None,
        base: None,
        is_sequential: Some(true),
        cardinality: None,
        completion_condition: None,
    });
    let sequential = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        id: None,
        base: None,
        is_sequential: Some(false),
        cardinality: None,
        completion_condition: None,
    });
    let parallel = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(standard, sequential);
    assert_ne!(sequential, parallel);
    assert!(parallel.contains("#aabbcc"));
}
#[test]
fn conversation_hexagons_and_links_render_with_semantic_hits() {
    use processcraft_core::{
        bpmn_collaboration::{ConversationKind, ConversationLink, ConversationNode},
        model::{Collaboration, Edge, Participant, Point},
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.collaboration = Some(Collaboration {
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        participants: vec![Participant {
            multiplicity: None,
            base: None,
            id: id("participant"),
            name: Some("Buyer".into()),
            process: Some(id("process")),
        }],
        flows: vec![],
        conversation_nodes: vec![
            ConversationNode {
                correlation_keys: vec![],
                base: None,
                id: id("conversation"),
                name: Some("Order".into()),
                kind: ConversationKind::SubConversation,
                participant_refs: vec![id("participant")],
                parent: None,
            },
            ConversationNode {
                correlation_keys: vec![],
                base: None,
                id: id("inner"),
                name: Some("Inner".into()),
                kind: ConversationKind::Conversation,
                participant_refs: vec![id("participant")],
                parent: Some(id("conversation")),
            },
        ],
        conversation_links: vec![ConversationLink {
            base: None,
            id: id("link"),
            name: Some("Coordination".into()),
            source: id("participant"),
            target: id("conversation"),
        }],
    });
    let diagram = &mut d.diagrams[0];
    diagram.plane = id("collab");
    diagram.shapes.clear();
    diagram.shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        id: id("conversation_di"),
        element: id("conversation"),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: Some(false),
        bounds: Bounds {
            x: 50.0,
            y: 50.0,
            width: 180.0,
            height: 100.0,
        },
        label: None,
        style: None,
    });
    diagram.shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        id: id("inner_di"),
        element: id("inner"),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        bounds: Bounds {
            x: 100.0,
            y: 80.0,
            width: 60.0,
            height: 40.0,
        },
        label: None,
        style: None,
    });
    diagram.edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("link_di"),
        element: id("link"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 0.0, y: 100.0 }, Point { x: 50.0, y: 100.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    assert!(!scene.diagnostics.iter().any(|v| v.contains("Unknown")));
    assert!(scene.scene.to_svg().contains("Coordination"));
    assert!(scene.hit_test(50.0, 50.0, 0.0).is_none());
    assert_eq!(
        scene.hit_test(140.0, 100.0, 0.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("conversation")
    );
    assert!(
        scene
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("link"))
    );
    assert!(
        !scene
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("inner"))
    );
    let v = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(
        v.hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("link"))
    );
    assert!(
        !v.hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("inner"))
    );
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    let expanded = build(&d, Appearance::Light);
    assert!(
        expanded
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("inner"))
    );
}
#[test]
fn data_store_and_association_are_drawn_in_scene_and_viewport() {
    use processcraft_core::{
        bpmn_data::{DataAssociation, DataAssociationKind},
        model::{Edge, Point},
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::DataStoreReference;
    d.processes[0].nodes[0].name = Some("Archive".into());
    d.processes[0].data_associations.push(DataAssociation {
        base: None,
        id: Some(id("assoc")),
        transformation: None,
        assignments: vec![],
        owner: id("task"),
        kind: DataAssociationKind::Input,
        sources: vec![id("task")],
        target: id("task"),
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("assoc_di"),
        element: id("assoc"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 0.0, y: 110.0 }, Point { x: 100.0, y: 110.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    assert!(!scene.diagnostics.iter().any(|v| v.contains("Unknown")));
    assert!(
        scene
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("assoc"))
    );
    let v = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: -50.0,
            y: 0.0,
            width: 200.0,
            height: 150.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(
        v.hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("assoc"))
    );
}
#[test]
fn choreography_task_shows_participant_bands_and_sequence_flow() {
    use processcraft_core::{
        bpmn_collaboration::{Choreography, ChoreographyActivity},
        model::{Edge, Flow, FlowKind, Participant, Point},
    };
    let mut d = fixture();
    d.choreographies.push(Choreography {
        base: None,
        id: id("choreo"),
        name: Some("Choreography".into()),
        participants: vec![
            Participant {
                multiplicity: None,
                base: None,
                id: id("buyer"),
                name: Some("Buyer".into()),
                process: None,
            },
            Participant {
                multiplicity: None,
                base: None,
                id: id("seller"),
                name: Some("Seller".into()),
                process: None,
            },
        ],
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            flow_metadata: None,
            correlation_keys: vec![],
            base: None,
            id: id("exchange"),
            name: Some("Exchange".into()),
            participant_refs: vec![id("buyer"), id("seller")],
            initiating_participant: id("buyer"),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![Flow {
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            id: id("seq"),
            kind: FlowKind::Sequence,
            source: id("exchange"),
            target: id("exchange"),
            name: Some(String::new()),
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        }],
    });
    let diag = &mut d.diagrams[0];
    diag.plane = id("choreo");
    diag.shapes[0].element = id("exchange");
    diag.shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 260.0,
        height: 140.0,
    };
    diag.shapes[0].label = None;
    diag.edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("seq_di"),
        element: id("seq"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 260.0, y: 70.0 }, Point { x: 300.0, y: 70.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    let svg = scene.scene.to_svg();
    for name in ["Buyer", "Seller", "Exchange"] {
        assert!(svg.contains(name), "{name}");
    }
    assert!(
        scene
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("seq"))
    );
    assert!(!scene.diagnostics.iter().any(|v| v.contains("Unknown")));
    // Explicit DI bands have independent representation and label identities.
    let diag = &mut d.diagrams[0];
    let owner = diag.shapes[0].id.selector();
    for (band_id, participant, kind, y) in [
        (
            "buyer_band",
            "buyer",
            processcraft_core::model::ParticipantBandKind::TopInitiating,
            0.0,
        ),
        (
            "seller_band",
            "seller",
            processcraft_core::model::ParticipantBandKind::BottomNonInitiating,
            116.0,
        ),
    ] {
        let mut band = diag.shapes[0].clone();
        band.id = id(band_id);
        band.element = id(participant);
        band.style = None;
        band.bounds = Bounds {
            x: 0.0,
            y,
            width: 260.0,
            height: 24.0,
        };
        band.participant_band_kind = Some(kind);
        band.choreography_activity_shape =
            Some(processcraft_core::model::DiReference::Local(owner.clone()));
        diag.shapes.push(band);
    }
    let explicit = build(&d, Appearance::Light);
    assert!(explicit.hits.iter().any(|h| h.diagram_element_id
        == id::<processcraft_core::ids::ElementSelector>("seller_band")
        && h.contains(10.0, 120.0, 0.0)));
    assert!(explicit.scene.paths().any(|p| p.color == [225, 225, 225]));
    assert_eq!(explicit.scene.to_svg().matches("Buyer").count(), 1);
    assert_eq!(explicit.scene.to_svg().matches("Seller").count(), 1);
    let buyer = d.diagrams[0]
        .shapes
        .iter_mut()
        .find(|s| s.id == id::<processcraft_core::ids::ElementSelector>("buyer_band"))
        .unwrap();
    buyer.label_visibility = Some(processcraft_core::model::DiLabelVisibility::Explicit);
    buyer.label = Some(processcraft_core::model::Label {
        di_metadata: None,
        id: Some(id("buyer_label")),
        bounds: Some(Bounds {
            x: 10.0,
            y: 2.0,
            width: 120.0,
            height: 20.0,
        }),
        style_ref: None,
        style: None,
    });
    d.diagrams[0].plane_order = Some(vec![
        id("shape"),
        id("seq_di"),
        id("buyer_band"),
        id("seller_band"),
    ]);
    let source_mode = build(&d, Appearance::Light);
    let label = source_mode
        .scene
        .labels()
        .iter()
        .find(|l| l.layout.text() == "Buyer")
        .unwrap();
    assert!(
        label
            .layout
            .glyphs()
            .iter()
            .filter(|g| g.outline.is_some())
            .count()
            >= 5
    );
    assert!(
        source_mode
            .hits
            .iter()
            .any(|h| h.label_id.as_ref() == Some(&id("buyer_label")))
    );
    assert!(
        !source_mode
            .diagnostics
            .iter()
            .any(|d| d.contains("Label clipped") && d.contains("Buyer"))
    );
    // Native creation uses a 25-pixel band and an explicit label with no
    // bounds; descenders must remain visible in both scene and viewport.
    d.choreographies[0].participants[0].name = Some("Buyer Gyp".into());
    let buyer = d.diagrams[0]
        .shapes
        .iter_mut()
        .find(|s| s.id == id::<processcraft_core::ids::ElementSelector>("buyer_band"))
        .unwrap();
    buyer.bounds.height = 25.0;
    buyer.label.as_mut().unwrap().bounds = None;
    let native_band = build(&d, Appearance::Light);
    let native_label = native_band
        .scene
        .labels()
        .iter()
        .find(|l| l.layout.text() == "Buyer Gyp")
        .unwrap();
    assert_eq!(
        native_label
            .layout
            .glyphs()
            .iter()
            .filter(|g| g.outline.is_some())
            .count(),
        8
    );
    assert!(
        !native_band
            .diagnostics
            .iter()
            .any(|d| d.contains("Label clipped") && d.contains("Buyer Gyp"))
    );
    let viewport = processcraft_render::BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        processcraft_render::Viewport {
            x: -16.0,
            y: -16.0,
            width: 320.0,
            height: 220.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    let full = native_band.scene.rasterize(1.0).unwrap();
    for y in 3..23 {
        for x in 95..165 {
            let a = full
                .pixel(
                    (x as f64 - native_band.origin.0) as u32,
                    (y as f64 - native_band.origin.1) as u32,
                )
                .unwrap();
            let b = viewport
                .pixels
                .pixel((x + 16) as u32, (y + 16) as u32)
                .unwrap();
            assert_eq!(a, b, "viewport glyph at {x},{y}");
        }
    }
    d.choreographies[0].participants[0].name = Some("Buyer".into());

    let mut response = d.choreographies[0].sequence_flows[0].clone();
    response.id = id("response");
    response.kind = FlowKind::Message;
    response.source = id("seller");
    response.target = id("buyer");
    d.choreographies[0].message_flows.push(response);
    d.choreographies[0].activities[0]
        .message_flow_refs
        .push(id("response"));
    d.diagrams[0]
        .shapes
        .iter_mut()
        .find(|s| s.id == id::<processcraft_core::ids::ElementSelector>("seller_band"))
        .unwrap()
        .is_message_visible = Some(true);
    let seller = d.diagrams[0]
        .shapes
        .iter_mut()
        .find(|s| s.id == id::<processcraft_core::ids::ElementSelector>("seller_band"))
        .unwrap();
    seller.style = None;
    seller.label_visibility = Some(processcraft_core::model::DiLabelVisibility::Explicit);
    seller.label = Some(processcraft_core::model::Label {
        di_metadata: None,
        id: Some(id("seller_label")),
        bounds: Some(Bounds {
            x: 10.0,
            y: 118.0,
            width: 120.0,
            height: 20.0,
        }),
        style_ref: None,
        style: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: Some(processcraft_core::model::MessageVisibleKind::NonInitiating),
        id: id("response_edge"),
        element: id("response"),
        source_element: None,
        target_element: None,
        waypoints: vec![Point { x: 0.0, y: 200.0 }, Point { x: 100.0, y: 200.0 }],
        label: None,
        label_visibility: None,
        style: None,
    });
    d.diagrams[0]
        .plane_order
        .as_mut()
        .unwrap()
        .push(id("response_edge"));
    let with_response = build(&d, Appearance::Light);
    assert!(
        with_response
            .scene
            .paths()
            .any(|p| p.color == [225, 225, 225]
                && (p.path.bounds().width() - 20.0).abs() < 0.01
                && (p.path.bounds().height() - 12.0).abs() < 0.01)
    );
    for appearance in [Appearance::Light, Appearance::Dark] {
        let scene = build(&d, appearance);
        let seller = scene
            .scene
            .labels()
            .iter()
            .find(|label| label.layout.text() == "Seller")
            .unwrap();
        assert_eq!(seller.color, [26, 37, 52]);
        assert!(
            seller
                .layout
                .glyphs()
                .iter()
                .filter(|g| g.outline.is_some())
                .count()
                >= 6
        );
        assert!(scene.scene.paths().any(|path| path.color == [26, 37, 52]
            && path.path.bounds().width() >= 20.0
            && path.path.bounds().height() >= 12.0));
        let raster = scene.scene.rasterize(1.0).unwrap();
        let dark_pixels = (120..136)
            .flat_map(|y| (12..128).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let pixel = raster
                    .pixel(
                        (*x as f64 - scene.origin.0) as u32,
                        (*y as f64 - scene.origin.1) as u32,
                    )
                    .unwrap();
                pixel.red() < 100 && pixel.green() < 100 && pixel.blue() < 100
            })
            .count();
        assert!(
            dark_pixels > 10,
            "participant text remains dark over the light band"
        );
        let x = (50.0 - scene.origin.0) as u32;
        let y = (200.0 - scene.origin.1) as u32;
        let contrasting = (y - 5..y + 5)
            .flat_map(|py| (x - 9..x + 9).map(move |px| (px, py)))
            .filter(|(px, py)| {
                let p = raster.pixel(*px, *py).unwrap();
                p.red() < 160 && p.green() < 160 && p.blue() < 160
            })
            .count();
        assert!(
            contrasting > 8,
            "return envelope fold must contrast with its light fill"
        );
    }
    d.diagrams[0].edges.pop();
    d.diagrams[0].plane_order.as_mut().unwrap().pop();
    d.diagrams[0]
        .shapes
        .iter_mut()
        .find(|s| s.id == id::<processcraft_core::ids::ElementSelector>("seller_band"))
        .unwrap()
        .style = Some(Style {
        fill: None,
        stroke: Some("#ffffff".into()),
        font: Some("#ffffff".into()),
    });
    let explicit_white = build(&d, Appearance::Dark);
    assert!(
        explicit_white
            .diagnostics
            .iter()
            .any(|d| d.contains("explicit stroke has low contrast"))
    );
    assert!(
        explicit_white
            .diagnostics
            .iter()
            .any(|d| d.contains("explicit label color has low contrast"))
    );
    assert_eq!(
        explicit_white
            .scene
            .labels()
            .iter()
            .find(|label| label.layout.text() == "Seller")
            .unwrap()
            .color,
        [255, 255, 255]
    );
    d.choreographies[0].message_flows[0].source = id("buyer");
    let wrong_sender = build(&d, Appearance::Light);
    assert!(
        wrong_sender
            .diagnostics
            .iter()
            .any(|v| v.contains("no resolved outgoing choreography message"))
    );
    assert!(
        !wrong_sender
            .scene
            .paths()
            .any(|p| p.color == [225, 225, 225]
                && (p.path.bounds().width() - 20.0).abs() < 0.01
                && (p.path.bounds().height() - 12.0).abs() < 0.01)
    );
    d.diagrams[0].shapes.truncate(1);
    d.diagrams[0].plane_order = Some(vec![id("shape"), id("seq_di")]);
    let missing = build(&d, Appearance::Light);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|v| v.contains("no explicit participant band representations"))
    );
    assert!(!missing.scene.to_svg().contains("Buyer"));
}
#[test]
fn data_object_collection_marker_comes_from_referenced_definition() {
    use processcraft_core::{bpmn_data::DataObject, bpmn_model::NodeDetails};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::DataObjectReference;
    d.processes[0].nodes[0].details = Some(NodeDetails::DataObjectReference {
        data_object_ref: Some(id("data")),
    });
    d.processes[0].data_objects.push(DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("data")),
        name: Some(String::new()),
        item_aware: None,
        is_collection: Some(false),
    });
    let single = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].data_objects[0].is_collection = Some(true);
    let collection = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(single, collection);
}
#[test]
fn gateway_default_flow_marker_uses_semantic_owner_and_shared_exports() {
    use processcraft_core::{
        bpmn_model::NodeDetails,
        model::{Edge, Flow, FlowKind, Point},
    };
    use processcraft_render::{ExportFormat, ExportOptions, export_diagram};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::ExclusiveGateway;
    d.processes[0].nodes[0].name = Some(String::new());
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("default"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some(String::new()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("edge_di"),
        element: id("default"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 95.0, y: 50.0 }, Point { x: 180.0, y: 50.0 }],
        label: None,
        style: None,
    });
    let export = |document: &BpmnDocument, format| {
        export_diagram(
            document,
            &id("diagram"),
            ExportOptions {
                format,
                appearance: Appearance::Light,
                scale: 1.0,
                acknowledge_diagnostics: true,
            },
            &mut TextEngine::new(),
            "Noto Sans",
        )
        .unwrap()
        .bytes
    };
    let before = [
        export(&d, ExportFormat::Svg),
        export(&d, ExportFormat::Png),
        export(&d, ExportFormat::Pdf),
    ];
    d.processes[0].nodes[0].details = Some(NodeDetails::Gateway {
        default_flow: Some(id("default")),
        direction: None,
        instantiate: None,
        event_gateway_type: None,
        activation_condition: None,
    });
    let after = [
        export(&d, ExportFormat::Svg),
        export(&d, ExportFormat::Png),
        export(&d, ExportFormat::Pdf),
    ];
    for (a, b) in before.iter().zip(after.iter()) {
        assert_ne!(a, b);
    }
    let render_view = |document: &BpmnDocument| {
        processcraft_render::BpmnViewport::build(
            document,
            &id("diagram"),
            Appearance::Light,
            &mut TextEngine::new(),
            "Noto Sans",
            processcraft_render::Viewport {
                x: -50.0,
                y: 0.0,
                width: 260.0,
                height: 120.0,
                zoom: 1.0,
            },
        )
        .unwrap()
        .pixels
        .data()
        .to_vec()
    };
    let with_marker = render_view(&d);

    let mut wrong = d.clone();
    if let Some(NodeDetails::Gateway { default_flow, .. }) =
        &mut wrong.processes[0].nodes[0].details
    {
        *default_flow = Some(id("edge_di"));
    }
    assert_eq!(before[0], export(&wrong, ExportFormat::Svg));
    assert_ne!(with_marker, render_view(&wrong));
}
#[test]
fn event_based_gateway_modes_render_distinct_symbols() {
    use processcraft_core::bpmn_model::{EventGatewayType, NodeDetails};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::EventBasedGateway;
    d.processes[0].nodes[0].name = Some(String::new());
    let marker = |instantiate, event_gateway_type| {
        Some(NodeDetails::Gateway {
            default_flow: None,
            direction: None,
            instantiate: Some(instantiate),
            event_gateway_type: Some(event_gateway_type),
            activation_condition: None,
        })
    };
    d.processes[0].nodes[0].details = marker(false, EventGatewayType::Exclusive);
    let double = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].details = marker(true, EventGatewayType::Exclusive);
    let single = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].details = marker(true, EventGatewayType::Parallel);
    let parallel = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(double, single);
    assert_ne!(single, parallel);
}

#[test]
fn group_is_drawn_and_keeps_contained_node_selectable() {
    use processcraft_core::bpmn_model::Group;
    let mut d = fixture();
    d.processes[0].groups.push(Group {
        base: None,
        id: id("group"),
        category_value_ref: None,
    });
    d.diagrams[0].shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id("group_shape"),
        element: id("group"),
        bounds: Bounds {
            x: -40.0,
            y: 0.0,
            width: 180.0,
            height: 120.0,
        },
        label: None,
        style: Some(Style {
            fill: None,
            stroke: Some("#aabbcc".into()),
            font: None,
        }),
    });
    let s = build(&d, Appearance::Light);
    assert!(
        !s.diagnostics
            .iter()
            .any(|d| d.contains("Unknown shape element group"))
    );
    assert!(s.scene.to_svg().contains("#aabbcc"));
    assert!(
        s.hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("group"))
    );
    assert_eq!(
        s.hit_test(0.0, 20.0, 2.0).unwrap().key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("task")
    );
}

#[test]
fn association_directions_change_shared_vector_markers() {
    use processcraft_core::{
        bpmn_model::AssociationDirection,
        model::{Edge, Flow, FlowKind, Point},
    };
    let mut d = fixture();
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("assoc"),
        kind: FlowKind::Association,
        source: id("task"),
        target: id("task"),
        name: Some(String::new()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("assoc_edge"),
        element: id("assoc"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 0., y: 100. }, Point { x: 90., y: 100. }],
        label: None,
        style: None,
    });
    let none = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].flows[0].association_direction = Some(AssociationDirection::One);
    let one = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].flows[0].association_direction = Some(AssociationDirection::Both);
    let both = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(none, one);
    assert_ne!(one, both);
}

#[test]
fn group_category_label_uses_di_bounds_style_and_viewport_hit() {
    use processcraft_core::{
        bpmn_model::{Category, CategoryValue, Group},
        model::Label,
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.categories.push(Category {
        base: None,
        id: id("category"),
        name: None,
        values: vec![CategoryValue {
            base: None,
            id: id("value"),
            value: Some("Compliance".into()),
        }],
    });
    d.processes[0].groups.push(Group {
        base: None,
        id: id("group"),
        category_value_ref: Some(id("value")),
    });
    d.diagrams[0].shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id("group_shape"),
        element: id("group"),
        bounds: Bounds {
            x: 500.,
            y: 500.,
            width: 180.,
            height: 120.,
        },
        label: Some(Label {
            di_metadata: None,
            id: None,
            style_ref: None,
            bounds: Some(Bounds {
                x: 10.,
                y: 110.,
                width: 140.,
                height: 30.,
            }),
            style: Some(Style {
                fill: None,
                stroke: None,
                font: Some("#771122".into()),
            }),
        }),
        style: None,
    });
    let full = build(&d, Appearance::Light);
    let svg = full.scene.to_svg();
    assert!(svg.contains("Compliance"));
    assert!(svg.contains("#771122"));
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.,
            y: 100.,
            width: 200.,
            height: 100.,
            zoom: 1.,
        },
    )
    .unwrap();
    assert!(
        viewport
            .hits
            .iter()
            .any(|hit| hit.key.xml_id == id::<processcraft_core::ids::ElementSelector>("group"))
    );
    d.diagrams[0]
        .shapes
        .last_mut()
        .unwrap()
        .label
        .as_mut()
        .unwrap()
        .bounds
        .as_mut()
        .unwrap()
        .width = 15.0;
    assert!(
        build(&d, Appearance::Light)
            .diagnostics
            .iter()
            .any(|d| d.contains("Label clipped") && d.contains("Compliance"))
    );
    d.diagrams[0]
        .shapes
        .last_mut()
        .unwrap()
        .label
        .as_mut()
        .unwrap()
        .bounds = Some(Bounds {
        x: 0.,
        y: 20.,
        width: 70.,
        height: 25.,
    });
    assert!(
        build(&d, Appearance::Light)
            .diagnostics
            .iter()
            .any(|d| d.contains("label overlaps another shape"))
    );
    d.categories[0].values[0].value = None;
    assert!(
        !build(&d, Appearance::Light)
            .scene
            .to_svg()
            .contains("Compliance")
    );
}

#[test]
fn activity_properties_change_shared_vectors_and_viewport() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        model::{Edge, Flow, FlowKind, Point},
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::SubProcess;
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("default_flow"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some(String::new()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("edge"),
        element: id("default_flow"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 0., y: 100. }, Point { x: 90., y: 100. }],
        label: None,
        style: None,
    });
    let plain = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].activity = Some(ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    });
    let event = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(plain, event);
    d.processes[0].nodes[0]
        .activity
        .as_mut()
        .unwrap()
        .is_for_compensation = Some(true);
    let compensation = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(event, compensation);
    d.processes[0].nodes[0]
        .activity
        .as_mut()
        .unwrap()
        .default_flow = Some(id("default_flow"));
    let default = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(compensation, default);
    let view = Viewport {
        x: -50.,
        y: 0.,
        width: 200.,
        height: 150.,
        zoom: 1.,
    };
    let with = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        view,
    )
    .unwrap();
    d.processes[0].nodes[0].activity = None;
    let without = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        view,
    )
    .unwrap();
    assert_ne!(with.pixels.data(), without.pixels.data());
}

fn assert_valid_event_document(d: &BpmnDocument) {
    let mut project = processcraft_core::model::ProjectData::new("events".into());
    project.documents.push(std::sync::Arc::new(d.clone()));
    project.validate().unwrap();
}

#[test]
fn link_markers_distinguish_catch_and_throw_in_shared_scene() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut d = fixture();
    d.processes[0].nodes[0].name = Some(String::new());
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 40.0,
        height: 40.0,
    };
    let mut render = |kind| {
        let node = &mut d.processes[0].nodes[0];
        node.kind = kind;
        node.details = Some(NodeDetails::Event {
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
            definitions: vec![EventDefinition::Link {
                base: None,
                id: None,
                name: "link".into(),
                sources: vec![],
                target: None,
            }],
            attached_to: None,
            cancel_activity: None,
        });
        assert_valid_event_document(&d);
        build(&d, Appearance::Light)
    };
    let catch = render(NodeKind::IntermediateCatchEvent);
    let throwing = render(NodeKind::IntermediateThrowEvent);
    assert_ne!(catch.scene.to_svg(), throwing.scene.to_svg());
    assert!(catch.hit_test(20.0, 20.0, 0.0).is_some());
    assert!(catch.hit_test(0.0, 0.0, 0.0).is_none());
}

#[test]
fn cancel_boundary_and_end_use_hollow_and_filled_crosses() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    use processcraft_core::model::FlowContainer;
    let mut d = fixture();
    let mut transaction = d.processes[0].nodes[0].clone();
    transaction.id = id("transaction");
    transaction.kind = NodeKind::Transaction;
    transaction.name = Some(String::new());
    transaction.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![],
            lanes: vec![],
            flows: vec![],
        }),
    });
    d.processes[0].nodes.push(transaction);
    let node = &mut d.processes[0].nodes[0];
    node.name = Some(String::new());
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Cancel {
            base: None,
            id: None,
        }],
        attached_to: Some(id("transaction")),
        cancel_activity: Some(true),
    });
    assert_valid_event_document(&d);
    let boundary = build(&d, Appearance::Light).scene.to_svg();
    let mut end = d.processes[0].nodes.remove(0);
    end.kind = NodeKind::EndEvent;
    end.details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Cancel {
            base: None,
            id: None,
        }],
        attached_to: None,
        cancel_activity: None,
    });
    let Some(NodeDetails::SubProcess { content }) = &mut d.processes[0].nodes[0].details else {
        panic!("transaction content")
    };
    content.nodes.push(end);
    d.diagrams[0].plane = id("transaction");
    assert_valid_event_document(&d);
    let end = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(boundary, end);
}

#[test]
fn compensation_boundary_and_throw_render_distinct_markers() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut d = fixture();
    let mut activity = d.processes[0].nodes[0].clone();
    activity.id = id("activity");
    activity.name = Some(String::new());
    d.processes[0].nodes.push(activity);
    let node = &mut d.processes[0].nodes[0];
    node.name = Some(String::new());
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Compensate {
            base: None,
            id: None,
            wait_for_completion: None,
            activity_ref: None,
        }],
        attached_to: Some(id("activity")),
        cancel_activity: Some(true),
    });
    assert_valid_event_document(&d);
    let boundary = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].kind = NodeKind::IntermediateThrowEvent;
    d.processes[0].nodes[0].details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Compensate {
            base: None,
            id: None,
            wait_for_completion: None,
            activity_ref: None,
        }],
        attached_to: None,
        cancel_activity: None,
    });
    assert_valid_event_document(&d);
    let throwing = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(boundary, throwing);
}

#[test]
fn data_reference_state_and_collection_share_scene_and_viewport() {
    use processcraft_core::{
        bpmn_data::{DataObject, DataState, ItemAwareMetadata},
        bpmn_model::NodeDetails,
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::DataObjectReference;
    node.name = Some(String::new());
    node.details = Some(NodeDetails::DataObjectReference {
        data_object_ref: Some(id("data")),
    });
    node.item_aware = Some(ItemAwareMetadata {
        item_subject_ref: None,
        data_state: Some(DataState {
            base: None,
            id: Some(id("state")),
            name: Some("Approved".into()),
        }),
    });
    d.processes[0].data_objects.push(DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("data")),
        name: Some("Order".into()),
        is_collection: Some(true),
        item_aware: None,
    });
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 80.0,
    };
    assert_valid_event_document(&d);
    let scene = build(&d, Appearance::Light);
    let svg = scene.scene.to_svg();
    assert!(svg.contains("Order") && svg.contains("Approved"));
    assert!(scene.hit_test(99.0, 1.0, 0.0).is_none());
    assert!(scene.hit_test(50.0, 40.0, 0.0).is_some());
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(viewport.hit_test(99.0, 1.0, 0.0).is_none());
    assert!(viewport.hit_test(50.0, 40.0, 0.0).is_some());
    d.processes[0].nodes[0].item_aware = None;
    let no_state = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert_ne!(viewport.pixels.data(), no_state.pixels.data());
}

#[test]
fn process_data_ports_render_direction_collection_state_and_hits() {
    use processcraft_core::bpmn_data::{
        DataPort, DataState, IoSet, IoSpecification, ItemAwareMetadata,
    };
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let port = |name: &str| DataPort {
        base: None,
        id: Some(id(name)),
        name: Some(name.into()),
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: None,
            data_state: Some(DataState {
                base: None,
                id: None,
                name: Some("Ready".into()),
            }),
        }),
        is_collection: Some(true),
    };
    d.processes[0].io_specification = Some(IoSpecification {
        base: None,
        id: None,
        inputs: vec![port("input")],
        outputs: vec![port("output")],
        input_sets: vec![IoSet {
            data_refs: vec![id("input")],
            ..Default::default()
        }],
        output_sets: vec![IoSet {
            data_refs: vec![id("output")],
            ..Default::default()
        }],
    });
    d.diagrams[0].shapes[0].element = id("input");
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 80.0,
    };
    assert_valid_event_document(&d);
    let input = build(&d, Appearance::Light);
    let svg = input.scene.to_svg();
    assert!(svg.contains("input") && svg.contains("Ready"));
    assert!(input.hit_test(99.0, 1.0, 0.0).is_none());
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(viewport.hit_test(50.0, 40.0, 0.0).is_some());
    assert!(viewport.hit_test(99.0, 1.0, 0.0).is_none());
    d.processes[0].io_specification.as_mut().unwrap().outputs[0].name = Some("input".into());
    d.diagrams[0].shapes[0].element = id("output");
    assert_valid_event_document(&d);
    let output = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(svg, output);
}

#[test]
fn global_data_store_shape_uses_cylinder_state_and_viewport_hits() {
    use processcraft_core::bpmn_data::{DataState, DataStore, ItemAwareMetadata};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.data_stores.push(DataStore {
        base: None,
        id: Some(id("store")),
        name: Some("Archive".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: None,
            data_state: Some(DataState {
                base: None,
                id: None,
                name: Some("Ready".into()),
            }),
        }),
    });
    d.diagrams[0].shapes[0].element = id("store");
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 80.0,
    };
    assert_valid_event_document(&d);
    let scene = build(&d, Appearance::Light);
    assert!(scene.diagnostics.iter().all(|d| !d.contains("Unknown")));
    let svg = scene.scene.to_svg();
    assert!(svg.contains("Archive") && svg.contains("Ready"));
    assert!(scene.hit_test(0.0, 0.0, 0.0).is_none());
    assert!(scene.hit_test(50.0, 40.0, 0.0).is_some());
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(viewport.hit_test(0.0, 0.0, 0.0).is_none());
    assert!(viewport.hit_test(50.0, 40.0, 0.0).is_some());
}

#[test]
fn nested_reference_resolves_ancestor_data_object_and_hides_when_collapsed() {
    use processcraft_core::{bpmn_data::DataObject, bpmn_model::NodeDetails, model::FlowContainer};
    let mut d = fixture();
    d.processes[0].data_objects.push(DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("ancestor_data")),
        name: Some("Ancestor Order".into()),
        is_collection: Some(true),
        item_aware: None,
    });
    let mut child = d.processes[0].nodes[0].clone();
    child.id = id("nested_ref");
    child.name = Some(String::new());
    child.kind = NodeKind::DataObjectReference;
    child.details = Some(NodeDetails::DataObjectReference {
        data_object_ref: Some(id("ancestor_data")),
    });
    let parent = &mut d.processes[0].nodes[0];
    parent.id = id("sub");
    parent.name = Some(String::new());
    parent.kind = NodeKind::SubProcess;
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![child],
            lanes: vec![],
            flows: vec![],
        }),
    });
    let shape = &mut d.diagrams[0].shapes[0];
    shape.element = id("sub");
    shape.is_expanded = Some(true);
    shape.bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 300.0,
        height: 200.0,
    };
    let mut child_shape = shape.clone();
    child_shape.id = id("nested_shape");
    child_shape.element = id("nested_ref");
    child_shape.is_expanded = None;
    child_shape.bounds = Bounds {
        x: 40.0,
        y: 40.0,
        width: 130.0,
        height: 80.0,
    };
    d.diagrams[0].shapes.push(child_shape);
    assert_valid_event_document(&d);
    let expanded = build(&d, Appearance::Light);
    assert!(expanded.scene.to_svg().contains("Ancestor Order"));
    assert!(
        expanded
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("nested_ref"))
    );
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    let collapsed = build(&d, Appearance::Light);
    assert!(!collapsed.scene.to_svg().contains("Ancestor Order"));
    assert!(
        !collapsed
            .hits
            .iter()
            .any(|h| h.key.xml_id == id::<processcraft_core::ids::ElementSelector>("nested_ref"))
    );
}

#[test]
fn data_store_reference_displays_its_own_name_including_empty() {
    use processcraft_core::{
        bpmn_data::{DataStore, DataStoreRef},
        bpmn_model::NodeDetails,
    };
    let mut d = fixture();
    d.data_stores.push(DataStore {
        base: None,
        id: Some(id("store")),
        name: Some("Declaration Name".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: None,
    });
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::DataStoreReference;
    node.name = Some("Reference Name".into());
    node.details = Some(NodeDetails::DataStoreReference {
        data_store_ref: Some(DataStoreRef::Legacy(id("store"))),
    });
    assert_valid_event_document(&d);
    let svg = build(&d, Appearance::Light).scene.to_svg();
    assert!(svg.contains("Reference Name"));
    assert!(!svg.contains("Declaration Name"));
    d.processes[0].nodes[0].name = Some(String::new());
    assert_valid_event_document(&d);
    let svg = build(&d, Appearance::Light).scene.to_svg();
    assert!(!svg.contains("Reference Name") && !svg.contains("Declaration Name"));
}

#[test]
fn anonymous_node_and_representation_have_native_hits() {
    use processcraft_core::ids::{ElementIdentity, ElementSelector, NativeElementId};
    let mut d = fixture();
    let node_native = NativeElementId::new();
    let shape_native = NativeElementId::new();
    d.processes[0].nodes[0].id = ElementIdentity::Native {
        native_id: node_native,
        xml_id: None,
    };
    d.processes[0].nodes[0].name = None;
    d.diagrams[0].shapes[0].id = ElementIdentity::Native {
        native_id: shape_native,
        xml_id: None,
    };
    d.diagrams[0].shapes[0].element = ElementSelector::Native {
        native_id: node_native,
    };
    assert_valid_event_document(&d);
    let scene = build(&d, Appearance::Light);
    let hit = scene.hit_test(0.0, 20.0, 0.0).unwrap();
    assert_eq!(
        hit.key.xml_id,
        ElementSelector::Native {
            native_id: node_native
        }
    );
    assert_eq!(
        hit.diagram_element_id,
        ElementSelector::Native {
            native_id: shape_native
        }
    );
    assert!(d.processes[0].nodes[0].id.xml_id().is_none());
    assert!(d.processes[0].nodes[0].name.is_none());
}

#[test]
fn annotation_text_uses_dedicated_payload_with_legacy_fallback() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::TextAnnotation;
    node.name = Some("Legacy annotation".into());
    node.details = Some(NodeDetails::TextAnnotation {
        text_format: None,
        text: None,
    });
    assert_valid_event_document(&d);
    let legacy = build(&d, Appearance::Light).scene.to_svg();
    assert!(legacy.contains("Legacy annotation"));
    d.processes[0].nodes[0].details = Some(NodeDetails::TextAnnotation {
        text_format: None,
        text: Some("New annotation".into()),
    });
    assert_valid_event_document(&d);
    let updated = build(&d, Appearance::Light).scene.to_svg();
    assert!(updated.contains("New annotation"));
    assert!(!updated.contains("Legacy annotation"));
    assert_eq!(
        d.processes[0].nodes[0].name.as_deref(),
        Some("Legacy annotation")
    );
}

#[test]
fn activity_port_shape_is_diagnosed_without_scene_or_viewport_hit() {
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.processes[0].nodes[0].io_specification = Some(IoSpecification {
        base: None,
        id: None,
        inputs: vec![DataPort {
            base: None,
            id: Some(id("activity_input")),
            name: Some("Local port".into()),
            item_aware: None,
            is_collection: None,
        }],
        outputs: vec![],
        input_sets: vec![IoSet {
            data_refs: vec![id("activity_input")],
            ..Default::default()
        }],
        output_sets: vec![IoSet::default()],
    });
    d.diagrams[0].shapes[0].element = id("activity_input");
    d.diagrams[0].shapes[0].label = Some(processcraft_core::model::Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        }),
        style: None,
    });
    assert_valid_event_document(&d);
    let original = serde_json::to_vec(&d).unwrap();
    let scene = build(&d, Appearance::Light);
    assert!(scene.diagnostics.iter().any(|d| d.contains("data port")));
    assert!(scene.hits.is_empty());
    assert!(!scene.scene.to_svg().contains("Local port"));
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: -40.0,
            y: 0.0,
            width: 160.0,
            height: 120.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(viewport.diagnostics.iter().any(|d| d.contains("data port")));
    assert!(viewport.hits.is_empty());
    assert_eq!(serde_json::to_vec(&d).unwrap(), original);
}

#[test]
fn conditional_source_marker_only_appears_for_nongateway_source() {
    use processcraft_core::{
        bpmn_model::Expression,
        model::{Edge, Flow, FlowKind, Point},
    };
    let mut d = fixture();
    d.processes[0].nodes[0].name = None;
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        id: id("conditional"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
        association_direction: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("conditional_di"),
        element: id("conditional"),
        source_element: None,
        label_visibility: None,
        target_element: None,
        waypoints: vec![Point { x: 95.0, y: 50.0 }, Point { x: 150.0, y: 50.0 }],
        label: None,
        style: None,
    });
    assert_valid_event_document(&d);
    let plain = build(&d, Appearance::Light);
    let count = plain.scene.paths().count();
    d.processes[0].flows[0].condition = Some(Expression::Legacy("x".into()));
    assert_valid_event_document(&d);
    let conditional = build(&d, Appearance::Light);
    assert_eq!(conditional.scene.paths().count(), count + 2);
    let source = (95.0 - conditional.origin.0) as f32;
    assert!(conditional.scene.paths().any(|path| {
        let b = path.path.bounds();
        (b.left() - source).abs() < 2.0 && (b.right() - (source + 12.0)).abs() < 2.0
    }));
    d.processes[0].nodes[0].kind = NodeKind::ExclusiveGateway;
    let gateway_conditional = build(&d, Appearance::Light).scene.paths().count();
    d.processes[0].flows[0].condition = None;
    assert_eq!(
        gateway_conditional,
        build(&d, Appearance::Light).scene.paths().count()
    );
}

#[test]
fn effective_multiple_event_resolves_root_and_parallel_marker() {
    use processcraft_core::bpmn_model::{EventDefinition, EventReference, NodeDetails, QNameRef};
    let mut d = fixture();
    d.root_event_definitions.push(EventDefinition::Signal {
        base: None,
        id: Some(id("root_signal")),
        signal_ref: None,
    });
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::IntermediateCatchEvent;
    node.name = None;
    node.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Message {
            base: None,
            id: None,
            message_ref: None,
            operation_ref: None,
        }],
        definition_refs: vec![EventReference::Local(id("root_signal"))],
        parallel_multiple: Some(false),
        is_interrupting: None,
        properties: vec![],
        io: None,
        attached_to: None,
        cancel_activity: None,
    });
    assert_valid_event_document(&d);
    let multiple = build(&d, Appearance::Light);
    let multiple_count = multiple.scene.paths().count();
    if let Some(NodeDetails::Event {
        parallel_multiple, ..
    }) = &mut d.processes[0].nodes[0].details
    {
        *parallel_multiple = Some(true);
    }
    assert_valid_event_document(&d);
    let parallel = build(&d, Appearance::Light);
    assert_eq!(parallel.scene.paths().count(), multiple_count + 1);
    if let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[0].details
    {
        *definition_refs = vec![EventReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: id("root_signal"),
        })];
    }
    let unresolved = build(&d, Appearance::Light);
    assert!(
        unresolved
            .diagnostics
            .iter()
            .any(|message| message.contains("unresolved event definition"))
    );
}

#[test]
fn exclusive_marker_visibility_uses_di_default_and_explicit_true() {
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::ExclusiveGateway;
    d.processes[0].nodes[0].name = None;
    d.diagrams[0].shapes[0].is_marker_visible = None;
    assert_valid_event_document(&d);
    let absent_count = build(&d, Appearance::Light).scene.paths().count();
    d.diagrams[0].shapes[0].is_marker_visible = Some(false);
    assert_eq!(
        build(&d, Appearance::Light).scene.paths().count(),
        absent_count
    );
    d.diagrams[0].shapes[0].is_marker_visible = Some(true);
    assert_eq!(
        build(&d, Appearance::Light).scene.paths().count(),
        absent_count + 2
    );
}

#[test]
fn omitted_subprocess_expansion_draws_collapsed_marker() {
    use processcraft_core::{bpmn_model::NodeDetails, model::FlowContainer};
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::SubProcess;
    node.name = None;
    node.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer::default()),
    });
    d.diagrams[0].shapes[0].is_expanded = None;
    assert_valid_event_document(&d);
    let absent = build(&d, Appearance::Light);
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    let collapsed = build(&d, Appearance::Light);
    assert_eq!(
        absent.scene.paths().count(),
        collapsed.scene.paths().count()
    );
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    assert_eq!(
        collapsed.scene.paths().count(),
        build(&d, Appearance::Light).scene.paths().count() + 3
    );
}

#[test]
fn lane_orientation_places_divider_on_corresponding_axis() {
    use processcraft_core::model::Lane;
    let mut d = fixture();
    d.processes[0].lanes.push(Lane {
        base: None,
        child_lane_set_id: None,
        child_lane_set_base: None,
        id: id("lane"),
        name: Some("Pool".into()),
        parent: None,
        members: vec![],
    });
    d.diagrams[0].shapes[0].element = id("lane");
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 80.0,
    };
    d.diagrams[0].shapes[0].is_horizontal = None;
    assert_valid_event_document(&d);
    let horizontal = build(&d, Appearance::Light);
    let pool_label = horizontal
        .scene
        .labels()
        .iter()
        .find(|label| label.layout.text() == "Pool")
        .unwrap();
    assert_eq!(pool_label.layout.line_count(), 1);
    let first = pool_label
        .layout
        .glyphs()
        .iter()
        .find_map(|glyph| glyph.outline.as_ref())
        .unwrap()
        .bounds();
    let last = pool_label
        .layout
        .glyphs()
        .iter()
        .rev()
        .find_map(|glyph| glyph.outline.as_ref())
        .unwrap()
        .bounds();
    assert!(
        last.top() < first.top(),
        "glyphs advance upward in the rotated title band"
    );
    let x = (24.0 - horizontal.origin.0) as f32;
    assert!(horizontal.scene.paths().any(|path| {
        let b = path.path.bounds();
        (b.left() - x).abs() < 2.0 && b.height() > 75.0 && b.width() < 3.0
    }));
    d.diagrams[0].shapes[0].is_horizontal = Some(false);
    let vertical = build(&d, Appearance::Light);
    let y = (24.0 - vertical.origin.1) as f32;
    assert!(vertical.scene.paths().any(|path| {
        let b = path.path.bounds();
        (b.top() - y).abs() < 2.0 && b.width() > 95.0 && b.height() < 3.0
    }));
}

#[test]
fn event_subprocess_start_interrupting_flag_controls_outer_ring() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_model::{EventDefinition, NodeDetails},
        model::FlowContainer,
    };
    let mut d = fixture();
    let mut start = d.processes[0].nodes[0].clone();
    start.id = id("start_event");
    start.name = None;
    start.kind = NodeKind::StartEvent;
    start.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Message {
            base: None,
            id: None,
            message_ref: None,
            operation_ref: None,
        }],
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: Some(true),
        properties: vec![],
        io: None,
        attached_to: None,
        cancel_activity: None,
    });
    let parent = &mut d.processes[0].nodes[0];
    parent.id = id("event_subprocess");
    parent.name = None;
    parent.kind = NodeKind::SubProcess;
    parent.activity = Some(ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    });
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            nodes: vec![start],
            ..Default::default()
        }),
    });
    let shape = &mut d.diagrams[0].shapes[0];
    shape.element = id("event_subprocess");
    shape.is_expanded = Some(true);
    shape.bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 300.0,
        height: 200.0,
    };
    let mut child = shape.clone();
    child.id = id("start_shape");
    child.element = id("start_event");
    child.is_expanded = None;
    child.bounds = Bounds {
        x: 60.0,
        y: 60.0,
        width: 40.0,
        height: 40.0,
    };
    d.diagrams[0].shapes.push(child);
    assert_valid_event_document(&d);
    let solid_count = build(&d, Appearance::Light).scene.paths().count();
    let Some(NodeDetails::SubProcess { content }) = &mut d.processes[0].nodes[0].details else {
        panic!("subprocess")
    };
    let Some(NodeDetails::Event {
        is_interrupting, ..
    }) = &mut content.nodes[0].details
    else {
        panic!("event")
    };
    *is_interrupting = Some(false);
    assert_valid_event_document(&d);
    assert_eq!(
        build(&d, Appearance::Light).scene.paths().count(),
        solid_count + 15
    );
}

#[test]
fn implicit_task_label_centers_each_line_in_shared_scene() {
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::Task;
    d.processes[0].nodes[0].name = Some("Approve\nOrder".into());
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 80.0,
    };
    let before = serde_json::to_vec(&d).unwrap();
    let scene = build(&d, Appearance::Light);
    let label = &scene.scene.labels()[0];
    for line in [0, 1] {
        let glyphs: Vec<_> = label
            .layout
            .glyphs()
            .iter()
            .filter(|glyph| glyph.source_line == line)
            .collect();
        assert!(!glyphs.is_empty());
        let left = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min) + label.x;
        let right = glyphs
            .iter()
            .map(|g| g.x + g.advance)
            .fold(f32::NEG_INFINITY, f32::max)
            + label.x;
        let expected = (60.0 - scene.origin.0) as f32;
        assert!(((left + right) / 2.0 - expected).abs() < 2.0);
    }
    assert!(label.y > (scene.origin.1.abs() as f32) + 10.0);
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
}

#[test]
fn explicit_label_presence_and_shared_di_font_are_rendered_without_mutation() {
    use processcraft_core::model::{DiFont, DiLabelVisibility, DiStyleRef, Label, LabelStyle};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::Task;
    d.processes[0].nodes[0].name = Some("Styled label".into());
    d.diagrams[0].label_styles.push(LabelStyle {
        id: Some(id("style")),
        font: Some(DiFont {
            name: Some("Noto Sans".into()),
            size: Some(16.0),
            is_bold: Some(true),
            is_italic: Some(true),
            is_underline: Some(true),
            is_strike_through: Some(true),
        }),
    });
    let shape = &mut d.diagrams[0].shapes[0];
    shape.label_visibility = Some(DiLabelVisibility::Explicit);
    shape.label = None;
    let hidden = build(&d, Appearance::Light);
    assert!(hidden.scene.labels().is_empty());
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: Some(id("label")),
        bounds: None,
        style_ref: Some(DiStyleRef::Local(id("style"))),
        style: None,
    });
    let mut second = d.diagrams[0].shapes[0].clone();
    second.id = id("second_shape");
    second.bounds.y = 110.0;
    second.label.as_mut().unwrap().id = Some(id("second_label"));
    d.diagrams[0].shapes.push(second);
    assert_valid_event_document(&d);
    let before = serde_json::to_vec(&d).unwrap();
    let styled = build(&d, Appearance::Light);
    assert_eq!(styled.scene.labels().len(), 2);
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
    assert!(
        !styled
            .diagnostics
            .iter()
            .any(|message| message.contains("unavailable") || message.contains("unresolved"))
    );
    let styled_count = styled.scene.paths().count();
    let styled_svg = styled.scene.to_svg();
    let font = d.diagrams[0].label_styles[0].font.as_mut().unwrap();
    font.is_bold = Some(false);
    font.is_italic = Some(false);
    font.is_underline = Some(false);
    font.is_strike_through = Some(false);
    let regular = build(&d, Appearance::Light);
    assert!(styled_count >= regular.scene.paths().count() + 4);
    assert_ne!(styled_svg, regular.scene.to_svg());
    assert_ne!(serde_json::to_vec(&d).unwrap(), before);
}

#[test]
fn mixed_annotation_projects_inert_text_without_markup_or_execution() {
    use processcraft_core::{
        bpmn_model::NodeDetails,
        bpmn_xml::{MixedXml, XmlContent, XmlElement, XmlNode},
    };
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::TextAnnotation;
    node.name = None;
    node.details = Some(NodeDetails::TextAnnotation {
        text_format: None,
        text: Some(MixedXml::Structured(XmlContent {
            namespaces: vec![],
            nodes: vec![
                XmlNode::Text {
                    text: "Before ".into(),
                },
                XmlNode::Element {
                    element: XmlElement {
                        namespace: String::new(),
                        local: id("emphasis"),
                        prefix: None,
                        namespaces: vec![],
                        attributes: vec![],
                        children: vec![XmlNode::Text {
                            text: "inside".into(),
                        }],
                    },
                },
                XmlNode::Comment {
                    text: "hidden comment".into(),
                },
                XmlNode::ProcessingInstruction {
                    target: id("pi"),
                    data: Some("hidden instruction".into()),
                },
            ],
        })),
    });
    assert_valid_event_document(&d);
    let before = serde_json::to_vec(&d).unwrap();
    let scene = build(&d, Appearance::Light);
    let text = scene.scene.labels()[0].layout.text();
    assert_eq!(text, "Before inside");
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
}

#[test]
fn explicit_external_label_hit_keeps_independent_identity_in_scene_and_viewport() {
    use processcraft_core::model::{DiLabelVisibility, Label};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    let shape = &mut d.diagrams[0].shapes[0];
    shape.label_visibility = Some(DiLabelVisibility::Explicit);
    shape.label = Some(Label {
        di_metadata: None,
        id: Some(id("label")),
        bounds: Some(Bounds {
            x: 150.0,
            y: 20.0,
            width: 120.0,
            height: 40.0,
        }),
        style_ref: None,
        style: None,
    });
    assert_valid_event_document(&d);
    let scene = build(&d, Appearance::Light);
    let hit = scene.hit_test(170.0, 35.0, 0.0).unwrap();
    assert!(hit.is_label);
    assert_eq!(hit.label_id, Some(id("label")));
    assert_eq!(
        hit.diagram_element_id,
        id::<processcraft_core::ids::ElementSelector>("shape")
    );
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 130.0,
            y: 0.0,
            width: 160.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert_eq!(
        viewport.hit_test(170.0, 35.0, 0.0).unwrap().label_id,
        Some(id("label"))
    );
    assert!(viewport.hit_test(170.0, 35.0, 0.0).unwrap().is_label);
    d.diagrams[0].shapes[0].label.as_mut().unwrap().id = None;
    let scene = build(&d, Appearance::Light);
    let anonymous = scene.hit_test(170.0, 35.0, 0.0).unwrap();
    assert!(anonymous.is_label);
    assert!(anonymous.label_id.is_none());
    assert!(!scene.hit_test(0.0, 35.0, 0.0).unwrap().is_label);
}

#[test]
fn di_font_absence_and_unsupported_values_have_explicit_fallbacks() {
    use processcraft_core::{
        bpmn_model::QNameRef,
        model::{DefinitionsMetadata, DiFont, DiLabelVisibility, DiStyleRef, Label, LabelStyle},
    };
    let mut d = fixture();
    d.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:local".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
        namespaces: vec![],
        foreign_attributes: vec![],
    });
    d.processes[0].nodes[0].kind = NodeKind::Task;
    d.processes[0].nodes[0].name = Some("Style".into());
    d.diagrams[0].label_styles.push(LabelStyle {
        id: Some(id("style")),
        font: Some(DiFont::default()),
    });
    d.diagrams[0].shapes[0].label_visibility = Some(DiLabelVisibility::Explicit);
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        bounds: None,
        style_ref: Some(DiStyleRef::Local(id("style"))),
        style: None,
    });
    assert_valid_event_document(&d);
    let absent = build(&d, Appearance::Light).scene.to_svg();
    let font = d.diagrams[0].label_styles[0].font.as_mut().unwrap();
    font.is_bold = Some(false);
    font.is_italic = Some(false);
    font.is_underline = Some(false);
    font.is_strike_through = Some(false);
    assert_eq!(absent, build(&d, Appearance::Light).scene.to_svg());
    let font = d.diagrams[0].label_styles[0].font.as_mut().unwrap();
    font.name = Some(String::new());
    font.size = Some(0.0);
    let before = serde_json::to_vec(&d).unwrap();
    let fallback = build(&d, Appearance::Light);
    assert!(
        fallback
            .diagnostics
            .iter()
            .any(|s| s.contains("font size") && s.contains("default"))
    );
    assert!(
        fallback
            .diagnostics
            .iter()
            .any(|s| s.contains("Missing font"))
    );
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
    d.diagrams[0].label_styles[0].font = None;
    assert!(
        build(&d, Appearance::Light)
            .diagnostics
            .iter()
            .any(|s| s.contains("no font"))
    );
    d.diagrams[0].shapes[0].label.as_mut().unwrap().style_ref =
        Some(DiStyleRef::Qualified(QNameRef {
            namespace: "urn:foreign".into(),
            local: id("style"),
        }));
    let external = build(&d, Appearance::Light);
    assert!(
        external
            .diagnostics
            .iter()
            .any(|s| s.contains("unresolved"))
    );
    assert!(!external.diagnostics.iter().any(|s| s.contains("no font")));
    d.diagrams[0].label_styles[0].font = Some(DiFont::default());
    d.diagrams[0].shapes[0].label.as_mut().unwrap().style_ref =
        Some(DiStyleRef::Qualified(QNameRef {
            namespace: "urn:local".into(),
            local: id("style"),
        }));
    assert!(
        !build(&d, Appearance::Light)
            .diagnostics
            .iter()
            .any(|s| s.contains("unresolved"))
    );
}

#[test]
fn explicit_absent_flow_label_is_hidden_but_legacy_implicit_is_visible() {
    use processcraft_core::model::{DiLabelVisibility, Edge, Flow, FlowKind, Point};
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.processes[0].nodes[0].name = None;
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        id: id("flow"),
        base_extras: None,
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: Some("Flow label".into()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id("edge"),
        element: id("flow"),
        source_element: None,
        target_element: None,
        waypoints: vec![Point { x: 95.0, y: 40.0 }, Point { x: 160.0, y: 40.0 }],
        label: None,
        label_visibility: None,
        style: None,
    });
    assert!(
        build(&d, Appearance::Light)
            .scene
            .to_svg()
            .contains("Flow label")
    );
    d.diagrams[0].edges[0].label_visibility = Some(DiLabelVisibility::Explicit);
    assert_valid_event_document(&d);
    let hidden = build(&d, Appearance::Light);
    assert!(!hidden.scene.to_svg().contains("Flow label"));
    let viewport = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: 100.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert!(!viewport.hits.iter().any(|hit| hit.label_id.is_some()));
}

#[test]
fn instantiating_receive_task_adds_message_start_ring() {
    use processcraft_core::bpmn_activity::ActivityProperties;
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::ReceiveTask;
    d.processes[0].nodes[0].name = None;
    d.processes[0].nodes[0].activity = Some(ActivityProperties {
        instantiate: Some(false),
        ..Default::default()
    });
    assert_valid_event_document(&d);
    let regular = build(&d, Appearance::Light);
    d.processes[0].nodes[0]
        .activity
        .as_mut()
        .unwrap()
        .instantiate = Some(true);
    assert_valid_event_document(&d);
    let initiating = build(&d, Appearance::Light);
    assert_eq!(
        initiating.scene.paths().count(),
        regular.scene.paths().count() + 1
    );
    let x = (d.diagrams[0].shapes[0].bounds.x - initiating.origin.0) as f32;
    let y = (d.diagrams[0].shapes[0].bounds.y - initiating.origin.1) as f32;
    assert!(initiating.scene.paths().any(|path| {
        let b = path.path.bounds();
        (b.left() - (x + 3.0)).abs() < 2.0
            && (b.top() - (y + 2.0)).abs() < 2.0
            && (b.width() - 22.0).abs() < 3.0
    }));
}

#[test]
fn collapsed_activity_marker_group_keeps_loop_and_compensation_left_of_plus() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_model::{LoopCharacteristics, NodeDetails},
        model::FlowContainer,
    };
    let mut d = fixture();
    let node = &mut d.processes[0].nodes[0];
    node.kind = NodeKind::SubProcess;
    node.name = None;
    node.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer::default()),
    });
    node.activity = Some(ActivityProperties {
        is_for_compensation: Some(true),
        ..Default::default()
    });
    node.loop_characteristics = Some(LoopCharacteristics::Standard {
        id: None,
        base: None,
        test_before: Some(false),
        condition: None,
        maximum: None,
    });
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 80.0,
    };
    assert_valid_event_document(&d);
    let scene = build(&d, Appearance::Light);
    let x = -scene.origin.0 as f32;
    let y = -scene.origin.1 as f32;
    let near = |center: f32| {
        scene
            .scene
            .paths()
            .filter(|path| {
                let b = path.path.bounds();
                b.top() > y + 62.0
                    && b.bottom() < y + 78.0
                    && ((b.left() + b.right()) / 2.0 - (x + center)).abs() < 5.0
            })
            .count()
    };
    assert!(near(42.0) >= 1, "loop marker");
    assert!(near(60.0) >= 2, "compensation marker");
    assert!(near(78.0) >= 3, "collapsed marker");
}

#[test]
fn local_global_call_task_uses_its_typed_marker_without_fabricating_an_id() {
    use processcraft_core::{
        bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind},
        bpmn_model::{NodeDetails, QNameRef},
        model::DefinitionsMetadata,
    };
    let mut d = fixture();
    d.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:local".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
        namespaces: vec![],
        foreign_attributes: vec![],
    });
    d.processes[0].nodes[0].kind = NodeKind::CallActivity;
    d.processes[0].nodes[0].name = None;
    d.processes[0].nodes[0].details = Some(NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: "urn:local".into(),
            local: id("global"),
        }),
    });
    d.global_tasks.push(GlobalTask {
        id: Some(id("global")),
        base: None,
        name: None,
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::Task,
    });
    assert_valid_event_document(&d);
    let generic = build(&d, Appearance::Light);
    assert!(
        !generic
            .diagnostics
            .iter()
            .any(|m| m.contains("cannot be resolved"))
    );
    let generic_count = generic.scene.paths().count();
    for kind in [
        GlobalTaskKind::Manual,
        GlobalTaskKind::User {
            implementation: None,
            renderings: vec![],
        },
        GlobalTaskKind::BusinessRule {
            implementation: None,
        },
        GlobalTaskKind::Script {
            script_language: None,
            script: None,
        },
    ] {
        d.global_tasks[0].kind = kind;
        assert_valid_event_document(&d);
        let scene = build(&d, Appearance::Light);
        assert!(scene.scene.paths().count() > generic_count);
        assert!(
            !scene
                .diagnostics
                .iter()
                .any(|m| m.contains("cannot be resolved"))
        );
    }
    let Some(NodeDetails::CallActivity { called_element }) = &mut d.processes[0].nodes[0].details
    else {
        panic!("call activity")
    };
    *called_element = Some(QNameRef {
        namespace: "urn:foreign".into(),
        local: id("global"),
    });
    let external = build(&d, Appearance::Light);
    assert!(
        external
            .diagnostics
            .iter()
            .any(|m| m.contains("cannot be resolved locally"))
    );
    assert_eq!(external.scene.paths().count(), generic_count);
}

#[test]
fn multi_instance_omitted_sequential_uses_parallel_marker_without_storing_default() {
    use processcraft_core::bpmn_model::LoopCharacteristics;
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::Task;
    d.processes[0].nodes[0].name = None;
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        id: None,
        base: None,
        is_sequential: None,
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        cardinality: None,
        completion_condition: None,
    });
    assert_valid_event_document(&d);
    let original = serde_json::to_vec(&d).unwrap();
    let omitted = build(&d, Appearance::Light);
    assert_eq!(serde_json::to_vec(&d).unwrap(), original);
    let Some(LoopCharacteristics::MultiInstance { is_sequential, .. }) =
        &mut d.processes[0].nodes[0].loop_characteristics
    else {
        panic!("loop")
    };
    *is_sequential = Some(false);
    assert_eq!(
        omitted.scene.to_svg(),
        build(&d, Appearance::Light).scene.to_svg()
    );
    let Some(LoopCharacteristics::MultiInstance { is_sequential, .. }) =
        &mut d.processes[0].nodes[0].loop_characteristics
    else {
        panic!("loop")
    };
    *is_sequential = Some(true);
    let sequential = build(&d, Appearance::Light);
    let bottom = (d.diagrams[0].shapes[0].bounds.y + d.diagrams[0].shapes[0].bounds.height
        - sequential.origin.1) as f32;
    let horizontal = sequential
        .scene
        .paths()
        .filter(|path| {
            let b = path.path.bounds();
            b.top() > bottom - 20.0 && b.width() > 9.0 && b.height() < 3.0
        })
        .count();
    let vertical = omitted
        .scene
        .paths()
        .filter(|path| {
            let b = path.path.bounds();
            b.top() > bottom - 20.0 && b.height() > 9.0 && b.width() < 3.0
        })
        .count();
    assert_eq!(horizontal, 3);
    assert_eq!(vertical, 3);
}

#[test]
fn local_process_call_defaults_to_collapsed_without_writing_di() {
    use processcraft_core::{
        bpmn_model::{NodeDetails, QNameRef},
        model::DefinitionsMetadata,
    };
    let mut d = fixture();
    d.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:local".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
        namespaces: vec![],
        foreign_attributes: vec![],
    });
    d.processes[0].nodes[0].kind = NodeKind::CallActivity;
    d.processes[0].nodes[0].name = None;
    d.processes[0].nodes[0].details = Some(NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: "urn:local".into(),
            local: id("process"),
        }),
    });
    d.diagrams[0].shapes[0].is_expanded = None;
    assert_valid_event_document(&d);
    let before = serde_json::to_vec(&d).unwrap();
    let omitted = build(&d, Appearance::Light);
    assert!(
        !omitted
            .diagnostics
            .iter()
            .any(|m| m.contains("expansion state"))
    );
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    assert_eq!(
        omitted.scene.paths().count(),
        build(&d, Appearance::Light).scene.paths().count()
    );
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    assert_eq!(
        omitted.scene.paths().count(),
        build(&d, Appearance::Light).scene.paths().count() + 3
    );
}

#[test]
fn authored_plane_order_controls_overlapping_shapes_and_viewport_hits() {
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.processes[0].nodes[0].name = None;
    let diagram = &mut d.diagrams[0];
    diagram.shapes[0].style.as_mut().unwrap().fill = Some("#ff0000".into());
    let mut second = diagram.shapes[0].clone();
    second.id = id("second");
    second.style.as_mut().unwrap().fill = Some("#0000ff".into());
    diagram.shapes.push(second);
    diagram.plane_order = Some(vec![id("second"), id("shape")]);
    let full = build(&d, Appearance::Light);
    assert_eq!(
        full.hit_test(20.0, 50.0, 0.0).unwrap().diagram_element_id,
        id::<processcraft_core::ids::ElementSelector>("shape")
    );
    let pix = full.scene.rasterize(1.0).unwrap();
    let pixel = pix
        .pixel((20.0 - full.origin.0) as u32, (50.0 - full.origin.1) as u32)
        .unwrap();
    assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (255, 0, 0));
    let view = BpmnViewport::build(
        &d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
        Viewport {
            x: -40.0,
            y: 0.0,
            width: 150.0,
            height: 110.0,
            zoom: 1.0,
        },
    )
    .unwrap();
    assert_eq!(
        view.hit_test(20.0, 50.0, 0.0).unwrap().diagram_element_id,
        id::<processcraft_core::ids::ElementSelector>("shape")
    );
    let pixel = view.pixels.pixel(60, 50).unwrap();
    assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (255, 0, 0));
    let svg = full.scene.to_svg();
    assert!(svg.find("#0000ff").unwrap() < svg.find("#ff0000").unwrap());
}
