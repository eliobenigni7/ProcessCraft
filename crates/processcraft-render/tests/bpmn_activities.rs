use processcraft_core::model::{
    Bounds, BpmnDocument, Diagram, Node, NodeKind, Process, Shape, Style,
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
            kind: NodeKind::Task,
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

fn svg_and_paths(kind: NodeKind) -> (String, usize) {
    let mut d = fixture();
    d.processes[0].nodes[0].kind = kind;
    let s = build(&d, Appearance::Light);
    (s.scene.to_svg(), s.scene.paths().count())
}

#[test]
fn task_kinds_render_their_own_markers_and_hit_test() {
    let (plain_svg, plain_paths) = svg_and_paths(NodeKind::Task);
    let kinds = [
        NodeKind::UserTask,
        NodeKind::ServiceTask,
        NodeKind::SendTask,
        NodeKind::ReceiveTask,
    ];
    let rendered: Vec<(String, usize)> = kinds.iter().map(|k| svg_and_paths(*k)).collect();
    for ((svg, paths), kind) in rendered.iter().zip(kinds) {
        // The marker is drawn on top of the plain rounded-rectangle outline.
        assert!(*paths > plain_paths, "{kind:?} adds marker geometry");
        assert_ne!(*svg, plain_svg, "{kind:?} differs from a plain task");
    }
    for i in 0..rendered.len() {
        for j in i + 1..rendered.len() {
            assert_ne!(
                rendered[i].0, rendered[j].0,
                "{:?} vs {:?}",
                kinds[i], kinds[j]
            );
        }
    }
    for kind in kinds {
        let mut d = fixture();
        d.processes[0].nodes[0].kind = kind;
        let b = &d.diagrams[0].shapes[0].bounds;
        let scene = build(&d, Appearance::Light);
        let hit = scene
            .hit_test(b.x + b.width / 2.0, b.y + b.height / 2.0, 0.0)
            .unwrap();
        assert_eq!(
            hit.key.xml_id,
            id::<processcraft_core::ids::ElementSelector>("task")
        );
    }
}
