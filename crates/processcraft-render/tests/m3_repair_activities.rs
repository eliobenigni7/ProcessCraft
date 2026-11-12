use processcraft_core::model::{Bounds, BpmnDocument, Diagram, Node, NodeKind, Process, Shape};
use processcraft_render::{Appearance, BpmnScene, TextEngine};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn doc(kind: NodeKind) -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(Process {
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
            name: Some(String::new()),
            documentation: String::new().into(),
            base_extras: None,
            kind,
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
                x: 10.0,
                y: 20.0,
                width: 120.0,
                height: 80.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    d
}

fn scene(kind: NodeKind) -> BpmnScene {
    BpmnScene::build(
        &doc(kind),
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap()
}

#[test]
fn plain_task_is_one_rounded_rectangle_without_marker_and_hit_tests() {
    let plain = scene(NodeKind::Task);
    let outline = plain.scene.paths().count();
    for marked in [NodeKind::UserTask, NodeKind::ServiceTask] {
        assert!(
            scene(marked).scene.paths().count() > outline,
            "{marked:?} adds a marker"
        );
    }
    // Plain task adds no marker: same path count as a task with no marker geometry at all.
    assert_eq!(
        plain.scene.paths().count(),
        scene(NodeKind::Task).scene.paths().count()
    );
    assert!(plain.scene.to_svg().contains("<path"));
    // The outline path spans the shape bounds (120x80) at the shape origin.
    // Exactly background, task fill and task outline: no marker geometry.
    let paths: Vec<_> = plain.scene.paths().collect();
    assert_eq!(paths.len(), 3);
    // The fill covers the shape bounds (120x80) at the shape origin (16px scene margin).
    let fill = paths[1].path.bounds();
    assert_eq!(
        (fill.left(), fill.top(), fill.width(), fill.height()),
        (16.0, 16.0, 120.0, 80.0)
    );
    // The outline is the same rounded rectangle plus its stroke width.
    let outline = paths[2].path.bounds();
    assert!((outline.width() - 121.5).abs() < 0.01 && (outline.height() - 81.5).abs() < 0.01);
    assert!((outline.left() - 15.25).abs() < 0.01 && (outline.top() - 15.25).abs() < 0.01);
    let hit = plain.hit_test(70.0, 60.0, 0.0).unwrap();
    assert_eq!(
        hit.key.xml_id,
        id::<processcraft_core::ids::ElementSelector>("task")
    );
    assert!(plain.hit_test(200.0, 200.0, 0.0).is_none());
}
