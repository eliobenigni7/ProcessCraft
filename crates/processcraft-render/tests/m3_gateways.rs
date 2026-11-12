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

fn scene_for(kind: NodeKind, marker: Option<bool>) -> BpmnScene {
    let mut d = fixture();
    d.processes[0].nodes[0].kind = kind;
    d.processes[0].nodes[0].name = None;
    d.diagrams[0].shapes[0].is_marker_visible = marker;
    build(&d, Appearance::Light)
}

#[test]
fn inclusive_gateway_draws_circle_marker_distinct_from_parallel_plus() {
    let none = scene_for(NodeKind::ExclusiveGateway, Some(false));
    let inclusive = scene_for(NodeKind::InclusiveGateway, None);
    let parallel = scene_for(NodeKind::ParallelGateway, None);
    let base = none.scene.paths().count();
    assert!(inclusive.scene.paths().count() > base, "inclusive circle");
    assert!(parallel.scene.paths().count() > base, "parallel plus");
    assert_ne!(inclusive.scene.to_svg(), parallel.scene.to_svg());
    assert_ne!(inclusive.scene.to_svg(), none.scene.to_svg());
    assert_ne!(parallel.scene.to_svg(), none.scene.to_svg());
}

#[test]
fn inclusive_and_parallel_gateways_hit_test_to_their_element_keys() {
    for kind in [NodeKind::InclusiveGateway, NodeKind::ParallelGateway] {
        let s = scene_for(kind, None);
        let hit = s.hit_test(35.0, 50.0, 2.0).unwrap();
        assert_eq!(
            hit.key.xml_id,
            id::<processcraft_core::ids::ElementSelector>("task")
        );
    }
}
