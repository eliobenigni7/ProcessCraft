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

fn ports_doc(collection: bool, out_name: &str, shape_of: &str) -> BpmnDocument {
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};
    let mut d = fixture();
    let port = |p: &str, name: &str| DataPort {
        base: None,
        id: Some(id(p)),
        name: Some(name.into()),
        item_aware: None,
        is_collection: Some(collection),
    };
    d.processes[0].io_specification = Some(IoSpecification {
        base: None,
        id: None,
        inputs: vec![port("input", "InName")],
        outputs: vec![port("output", out_name)],
        input_sets: vec![IoSet {
            data_refs: vec![id("input")],
            ..Default::default()
        }],
        output_sets: vec![IoSet {
            data_refs: vec![id("output")],
            ..Default::default()
        }],
    });
    d.diagrams[0].shapes[0].element = id(shape_of);
    d.diagrams[0].shapes[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 80.0,
    };
    let mut project = processcraft_core::model::ProjectData::new("ports".into());
    project.documents.push(std::sync::Arc::new(d.clone()));
    project.validate().unwrap();
    d
}

#[test]
fn data_input_port_draws_name_and_collection_marker() {
    let svg = |c| {
        build(&ports_doc(c, "OutName", "input"), Appearance::Light)
            .scene
            .to_svg()
    };
    let (plain, many) = (svg(false), svg(true));
    assert!(plain.contains("InName") && many.contains("InName"));
    assert_ne!(plain, many, "isCollection must change the drawn notation");
}

#[test]
fn data_output_port_draws_its_own_name_and_collection_marker() {
    let svg = |c| {
        build(&ports_doc(c, "OutName", "output"), Appearance::Light)
            .scene
            .to_svg()
    };
    let (plain, many) = (svg(false), svg(true));
    assert!(plain.contains("OutName") && many.contains("OutName"));
    assert!(
        !plain.contains("InName"),
        "output shape must not draw the input name"
    );
    assert_ne!(plain, many, "isCollection must change the drawn notation");
}
