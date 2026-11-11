use processcraft_core::{
    ids::XmlId,
    model::{Bounds, BpmnDocument, Capability, Diagram, Node, NodeKind, Process, Shape, Style},
};
use processcraft_render::{Appearance, BpmnScene, TextEngine};
fn id(s: &str) -> XmlId {
    s.parse().unwrap()
}
fn fixture() -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: String::new(),
        documentation: String::new(),
        nodes: vec![Node {
            activity: None,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            id: id("task"),
            name: "Review".into(),
            documentation: String::new(),
            kind: NodeKind::UserTask,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![Shape {
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
    assert_eq!(hit.key.xml_id, id("task"));
    assert_eq!(hit.diagram_element_id, id("shape"));
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
        association_direction: None,
        id: id("flow"),
        kind: processcraft_core::model::FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: "Approved".into(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(processcraft_core::model::Edge {
        id: id("edge"),
        element: id("flow"),
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
        bounds: Bounds {
            x: 0.,
            y: 0.,
            width: 20.,
            height: 5.,
        },
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
        let key = id(&format!("flow{i}"));
        d.processes[0].flows.push(processcraft_core::model::Flow {
            association_direction: None,
            id: key.clone(),
            kind: processcraft_core::model::FlowKind::Association,
            source: id("task"),
            target: id("task"),
            name: String::new(),
            documentation: String::new(),
            condition: None,
        });
        d.diagrams[0].edges.push(processcraft_core::model::Edge {
            id: id(&format!("edge{i}")),
            element: key,
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
    assert_eq!(hit.diagram_element_id, id("far"));
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
        association_direction: None,
        id: id("f"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: "A very long label".into(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        id: id("e"),
        element: id("f"),
        waypoints: vec![
            Point { x: 10., y: 50. },
            Point { x: 50., y: 50. },
            Point { x: 50., y: 90. },
            Point { x: 200., y: 90. },
        ],
        label: Some(Label {
            bounds: Bounds {
                x: 10.,
                y: 5.,
                width: 20.,
                height: 5.,
            },
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
    assert!(s.hits.iter().all(|h| h.diagram_element_id != id("giant")));
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
        bounds: Bounds {
            x: 10.,
            y: 10.,
            width: 100.,
            height: 40.,
        },
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
    assert_eq!(s.hit_test(15., 15., 0.).unwrap().key.xml_id, id("task"));
}
#[test]
fn viewport_does_not_fallback_when_explicit_shape_label_is_offscreen() {
    use processcraft_core::model::Label;
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes[0].label = Some(Label {
        bounds: Bounds {
            x: 500.,
            y: 500.,
            width: 100.,
            height: 40.,
        },
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
        association_direction: None,
        id: id("f"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: "Visible".into(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        id: id("e"),
        element: id("f"),
        waypoints: vec![Point { x: 500., y: 500. }, Point { x: 600., y: 500. }],
        label: Some(Label {
            bounds: Bounds {
                x: 10.,
                y: 10.,
                width: 100.,
                height: 40.,
            },
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
    assert_eq!(s.hit_test(15., 15., 0.).unwrap().key.xml_id, id("f"));
}
#[test]
fn viewport_clicks_external_label_of_visible_shape() {
    use processcraft_core::model::Label;
    use processcraft_render::{BpmnViewport, Viewport};
    let mut d = fixture();
    d.diagrams[0].shapes[0].label = Some(Label {
        bounds: Bounds {
            x: 110.,
            y: 110.,
            width: 70.,
            height: 30.,
        },
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
    assert_eq!(s.hit_test(120., 120., 0.).unwrap().key.xml_id, id("task"));
}
#[test]
fn typed_timer_event_has_circle_hits_and_shared_vector_marker() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails, TimerKind};
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::IntermediateCatchEvent;
    d.processes[0].nodes[0].details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Timer {
            id: None,
            kind: TimerKind::Duration,
            expression: "PT5M".into(),
        }],
        attached_to: None,
        cancel_activity: None,
    });
    d.processes[0].nodes[0].name.clear();
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
        definitions: vec![EventDefinition::Terminate { id: None }],
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
    d.processes[0].nodes[0].name.clear();
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
    activity.name.clear();
    d.processes[0].nodes.push(activity);
    let node = &mut d.processes[0].nodes[0];
    node.name.clear();
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Compensate {
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
    node.name.clear();
    node.details = Some(NodeDetails::Event {
        definitions: vec![
            EventDefinition::Signal {
                id: None,
                signal_ref: None,
            },
            EventDefinition::Message {
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
    child.name = "Nested task".into();
    let parent = &mut d.processes[0].nodes[0];
    parent.id = id("sub");
    parent.kind = NodeKind::SubProcess;
    parent.name.clear();
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
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
        id("child")
    );
    assert!(expanded.scene.to_svg().contains("Nested task"));
    let expanded_svg = expanded.scene.to_svg();
    d.diagrams[0].shapes[0].is_expanded = Some(false);
    let collapsed = build(&d, Appearance::Light);
    assert_eq!(
        collapsed.hit_test(100.0, 100.0, 0.0).unwrap().key.xml_id,
        id("sub")
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
    assert!(!viewport.hits.iter().any(|h| h.key.xml_id == id("child")));
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
        id("child")
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
    child.name.clear();
    let mut called = child.clone();
    called.id = id("nested_b");
    called.kind = NodeKind::CallActivity;
    called.details = Some(NodeDetails::CallActivity {
        called_element: QNameRef {
            namespace: "urn:example".into(),
            local: id("called"),
        },
    });
    let flow = Flow {
        association_direction: None,
        id: id("inner_flow"),
        kind: FlowKind::Sequence,
        source: id("nested_a"),
        target: id("nested_b"),
        name: "Inner".into(),
        documentation: String::new(),
        condition: None,
    };
    let parent = &mut d.processes[0].nodes[0];
    parent.kind = NodeKind::SubProcess;
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
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
        id: id("inner_di"),
        element: id("inner_flow"),
        waypoints: vec![Point { x: 250.0, y: 50.0 }, Point { x: 300.0, y: 50.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| hit.key.xml_id == id("nested_b"))
    );
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| hit.key.xml_id == id("inner_flow"))
    );
    assert!(scene.scene.to_svg().contains("Inner"));
    assert!(!scene.diagnostics.iter().any(|d| d.contains("Unknown")));
}
#[test]
fn loop_and_multi_instance_markers_use_distinct_shared_vectors() {
    use processcraft_core::bpmn_model::LoopCharacteristics;
    let mut d = fixture();
    d.processes[0].nodes[0].name.clear();
    let base = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::Standard {
        test_before: false,
        condition: None,
        maximum: None,
    });
    let standard = build(&d, Appearance::Light).scene.to_svg();
    assert_ne!(base, standard);
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        is_sequential: true,
        cardinality: None,
        completion_condition: None,
    });
    let sequential = build(&d, Appearance::Light).scene.to_svg();
    d.processes[0].nodes[0].loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        is_sequential: false,
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
        id: id("collab"),
        participants: vec![Participant {
            id: id("participant"),
            name: "Buyer".into(),
            process: Some(id("process")),
        }],
        flows: vec![],
        conversation_nodes: vec![
            ConversationNode {
                id: id("conversation"),
                name: "Order".into(),
                kind: ConversationKind::SubConversation,
                participant_refs: vec![id("participant")],
                parent: None,
            },
            ConversationNode {
                id: id("inner"),
                name: "Inner".into(),
                kind: ConversationKind::Conversation,
                participant_refs: vec![id("participant")],
                parent: Some(id("conversation")),
            },
        ],
        conversation_links: vec![ConversationLink {
            id: id("link"),
            name: "Coordination".into(),
            source: id("participant"),
            target: id("conversation"),
        }],
    });
    let diagram = &mut d.diagrams[0];
    diagram.plane = id("collab");
    diagram.shapes.clear();
    diagram.shapes.push(Shape {
        id: id("conversation_di"),
        element: id("conversation"),
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
        id: id("inner_di"),
        element: id("inner"),
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
        id: id("link_di"),
        element: id("link"),
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
        id("conversation")
    );
    assert!(scene.hits.iter().any(|h| h.key.xml_id == id("link")));
    assert!(!scene.hits.iter().any(|h| h.key.xml_id == id("inner")));
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
    assert!(v.hits.iter().any(|h| h.key.xml_id == id("link")));
    assert!(!v.hits.iter().any(|h| h.key.xml_id == id("inner")));
    d.diagrams[0].shapes[0].is_expanded = Some(true);
    let expanded = build(&d, Appearance::Light);
    assert!(expanded.hits.iter().any(|h| h.key.xml_id == id("inner")));
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
    d.processes[0].nodes[0].name = "Archive".into();
    d.processes[0].data_associations.push(DataAssociation {
        id: id("assoc"),
        owner: id("task"),
        kind: DataAssociationKind::Input,
        sources: vec![id("task")],
        target: id("task"),
    });
    d.diagrams[0].edges.push(Edge {
        id: id("assoc_di"),
        element: id("assoc"),
        waypoints: vec![Point { x: 0.0, y: 110.0 }, Point { x: 100.0, y: 110.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    assert!(!scene.diagnostics.iter().any(|v| v.contains("Unknown")));
    assert!(scene.hits.iter().any(|h| h.key.xml_id == id("assoc")));
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
    assert!(v.hits.iter().any(|h| h.key.xml_id == id("assoc")));
}
#[test]
fn choreography_task_shows_participant_bands_and_sequence_flow() {
    use processcraft_core::{
        bpmn_collaboration::{Choreography, ChoreographyActivity},
        model::{Edge, Flow, FlowKind, Participant, Point},
    };
    let mut d = fixture();
    d.choreographies.push(Choreography {
        id: id("choreo"),
        name: "Choreography".into(),
        participants: vec![
            Participant {
                id: id("buyer"),
                name: "Buyer".into(),
                process: None,
            },
            Participant {
                id: id("seller"),
                name: "Seller".into(),
                process: None,
            },
        ],
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            id: id("exchange"),
            name: "Exchange".into(),
            participant_refs: vec![id("buyer"), id("seller")],
            initiating_participant: id("buyer"),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![Flow {
            association_direction: None,
            id: id("seq"),
            kind: FlowKind::Sequence,
            source: id("exchange"),
            target: id("exchange"),
            name: String::new(),
            documentation: String::new(),
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
        id: id("seq_di"),
        element: id("seq"),
        waypoints: vec![Point { x: 260.0, y: 70.0 }, Point { x: 300.0, y: 70.0 }],
        label: None,
        style: None,
    });
    let scene = build(&d, Appearance::Light);
    let svg = scene.scene.to_svg();
    for name in ["Buyer", "Seller", "Exchange"] {
        assert!(svg.contains(name), "{name}");
    }
    assert!(scene.hits.iter().any(|h| h.key.xml_id == id("seq")));
    assert!(!scene.diagnostics.iter().any(|v| v.contains("Unknown")));
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
        id: id("data"),
        name: String::new(),
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
    d.processes[0].nodes[0].name.clear();
    d.processes[0].flows.push(Flow {
        association_direction: None,
        id: id("default"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: String::new(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        id: id("edge_di"),
        element: id("default"),
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
    d.processes[0].nodes[0].name.clear();
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
        id: id("group"),
        category_value_ref: None,
    });
    d.diagrams[0].shapes.push(Shape {
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
    assert!(s.hits.iter().any(|h| h.key.xml_id == id("group")));
    assert_eq!(s.hit_test(0.0, 20.0, 2.0).unwrap().key.xml_id, id("task"));
}

#[test]
fn association_directions_change_shared_vector_markers() {
    use processcraft_core::{
        bpmn_model::AssociationDirection,
        model::{Edge, Flow, FlowKind, Point},
    };
    let mut d = fixture();
    d.processes[0].flows.push(Flow {
        association_direction: None,
        id: id("assoc"),
        kind: FlowKind::Association,
        source: id("task"),
        target: id("task"),
        name: String::new(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        id: id("assoc_edge"),
        element: id("assoc"),
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
        id: id("category"),
        name: None,
        values: vec![CategoryValue {
            id: id("value"),
            value: Some("Compliance".into()),
        }],
    });
    d.processes[0].groups.push(Group {
        id: id("group"),
        category_value_ref: Some(id("value")),
    });
    d.diagrams[0].shapes.push(Shape {
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
            bounds: Bounds {
                x: 10.,
                y: 110.,
                width: 140.,
                height: 30.,
            },
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
            .any(|hit| hit.key.xml_id == id("group"))
    );
    d.diagrams[0]
        .shapes
        .last_mut()
        .unwrap()
        .label
        .as_mut()
        .unwrap()
        .bounds
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
        .bounds = Bounds {
        x: 0.,
        y: 20.,
        width: 70.,
        height: 25.,
    };
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
        association_direction: None,
        id: id("default_flow"),
        kind: FlowKind::Sequence,
        source: id("task"),
        target: id("task"),
        name: String::new(),
        documentation: String::new(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        id: id("edge"),
        element: id("default_flow"),
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
    d.processes[0].nodes[0].name.clear();
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
            definitions: vec![EventDefinition::Link {
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
    transaction.name.clear();
    transaction.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
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
    node.name.clear();
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Cancel { id: None }],
        attached_to: Some(id("transaction")),
        cancel_activity: Some(true),
    });
    assert_valid_event_document(&d);
    let boundary = build(&d, Appearance::Light).scene.to_svg();
    let mut end = d.processes[0].nodes.remove(0);
    end.kind = NodeKind::EndEvent;
    end.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Cancel { id: None }],
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
    activity.name.clear();
    d.processes[0].nodes.push(activity);
    let node = &mut d.processes[0].nodes[0];
    node.name.clear();
    node.kind = NodeKind::BoundaryEvent;
    node.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Compensate {
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
        definitions: vec![EventDefinition::Compensate {
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
