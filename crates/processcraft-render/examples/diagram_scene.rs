use processcraft_core::model::{
    Bounds, BpmnDocument, Diagram, Edge, Flow, FlowKind, Node, NodeKind, Point, Process, Shape,
};
use processcraft_render::{Appearance, BpmnScene, TextEngine};
use std::{error::Error, fs, path::Path};
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut d = BpmnDocument::new("Order".into(), id("defs"));
    let mut nodes = Vec::new();
    let mut shapes = Vec::new();
    for (i, (key, name, kind, x, y, w, h)) in [
        ("start", "", NodeKind::StartEvent, 20., 100., 36., 36.),
        (
            "review",
            "Review order",
            NodeKind::UserTask,
            100.,
            80.,
            120.,
            75.,
        ),
        (
            "decision",
            "",
            NodeKind::ExclusiveGateway,
            260.,
            95.,
            50.,
            50.,
        ),
        (
            "pack",
            "Pack order",
            NodeKind::ManualTask,
            350.,
            80.,
            120.,
            75.,
        ),
        ("end", "", NodeKind::EndEvent, 515., 100., 36., 36.),
        (
            "note",
            "Check address",
            NodeKind::TextAnnotation,
            110.,
            190.,
            125.,
            50.,
        ),
        (
            "data",
            "",
            NodeKind::DataObjectReference,
            370.,
            190.,
            40.,
            50.,
        ),
        (
            "parallel",
            "",
            NodeKind::ParallelGateway,
            575.,
            95.,
            50.,
            50.,
        ),
        ("task", "Notify", NodeKind::Task, 665., 80., 100., 75.),
    ]
    .into_iter()
    .enumerate()
    {
        nodes.push(Node {
            flow_metadata: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: id(key),
            name: Some(name.into()),
            documentation: String::new().into(),
            base_extras: None,
            kind,
            owner: String::new(),
            contact: String::new(),
        });
        shapes.push(Shape {
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id(&format!("shape_{i}")),
            element: id(key),
            bounds: Bounds {
                x,
                y,
                width: w,
                height: h,
            },
            label: None,
            style: None,
        });
    }
    let specs = [
        (
            "f1",
            "start",
            "review",
            FlowKind::Sequence,
            vec![(56., 118.), (100., 118.)],
        ),
        (
            "f2",
            "review",
            "decision",
            FlowKind::Sequence,
            vec![(220., 118.), (260., 120.)],
        ),
        (
            "f3",
            "decision",
            "pack",
            FlowKind::Sequence,
            vec![(310., 120.), (350., 118.)],
        ),
        (
            "f4",
            "pack",
            "end",
            FlowKind::Sequence,
            vec![(470., 118.), (515., 118.)],
        ),
        (
            "f5",
            "review",
            "note",
            FlowKind::Association,
            vec![(160., 155.), (160., 190.)],
        ),
        (
            "f6",
            "pack",
            "data",
            FlowKind::Message,
            vec![(410., 155.), (410., 190.)],
        ),
    ];
    let mut flows = Vec::new();
    let mut edges = Vec::new();
    for (i, (key, source, target, kind, pts)) in specs.into_iter().enumerate() {
        flows.push(Flow {
            flow_metadata: None,
            association_direction: None,
            id: id(key),
            kind,
            source: id(source),
            target: id(target),
            name: Some(String::new()),
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        });
        edges.push(Edge {
            id: id(&format!("edge_{i}")),
            element: id(key),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: pts.into_iter().map(|(x, y)| Point { x, y }).collect(),
            label: None,
            style: None,
        });
    }
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
        name: Some("Order".into()),
        documentation: String::new().into(),
        base_extras: None,
        nodes,
        lanes: vec![],
        flows,
    });
    d.diagrams.push(Diagram {
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes,
        edges,
    });
    let out = Path::new("target/m1-diagram");
    fs::create_dir_all(out)?;
    for (name, appearance) in [("light", Appearance::Light), ("dark", Appearance::Dark)] {
        let scene = BpmnScene::build(
            &d,
            &id("diagram"),
            appearance,
            &mut TextEngine::new(),
            "Noto Sans",
        )?;
        fs::write(out.join(format!("{name}.svg")), scene.scene.to_svg())?;
        scene
            .scene
            .rasterize(1.0)?
            .save_png(out.join(format!("{name}.png")))?;
    }
    Ok(())
}
