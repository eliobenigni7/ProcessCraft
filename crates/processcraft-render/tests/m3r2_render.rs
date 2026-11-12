//! M3 round 2 render cells: sequence flow, default flow, lane, plane element, group
//! category label, choreography initiating participant, pool/message-flow hit testing.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{Choreography, ChoreographyActivity},
    bpmn_model::{Category, CategoryValue, Group, NodeDetails},
    ids::ElementSelector,
    model::{
        Bounds, BpmnDocument, Collaboration, Diagram, Edge, Flow, FlowKind, Lane, Node, NodeKind,
        Participant, Point, Process, Shape,
    },
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
fn shape(sid: &str, element: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: Some(true),
        is_marker_visible: None,
        is_expanded: None,
        id: id(sid),
        element: id(element),
        bounds: Bounds {
            x,
            y,
            width: w,
            height: h,
        },
        label: None,
        style: None,
    }
}
fn node(nid: &str, name: &str, kind: NodeKind) -> Node {
    Node {
        flow_metadata: None,
        item_aware: None,
        activity: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        id: id(nid),
        name: Some(name.into()),
        documentation: String::new().into(),
        base_extras: None,
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
fn flow(fid: &str, kind: FlowKind, src: &str, dst: &str, name: Option<&str>) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id(fid),
        kind,
        source: Some(local(src)),
        target: Some(local(dst)),
        name: name.map(Into::into),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    }
}
fn edge(eid: &str, element: &str, pts: &[(f64, f64)]) -> Edge {
    Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id(eid),
        element: id(element),
        source_element: None,
        target_element: None,
        waypoints: pts.iter().map(|&(x, y)| Point { x, y }).collect(),
        label: None,
        label_visibility: None,
        style: None,
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
fn process(nodes: Vec<Node>, lanes: Vec<Lane>, flows: Vec<Flow>) -> Process {
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
        lanes,
        flows,
    }
}
fn build(d: &BpmnDocument) -> BpmnScene {
    BpmnScene::build(
        d,
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap()
}
/// gateway -> task over a sequence flow named "Approved".
fn flow_doc(with_edge: bool) -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(process(
        vec![
            node("gw", "", NodeKind::InclusiveGateway),
            node("task", "Ship", NodeKind::Task),
        ],
        vec![],
        vec![flow(
            "sf",
            FlowKind::Sequence,
            "gw",
            "task",
            Some("Approved"),
        )],
    ));
    let edges = if with_edge {
        vec![edge("sf_edge", "sf", &[(50.0, 50.0), (150.0, 50.0)])]
    } else {
        vec![]
    };
    d.diagrams.push(diagram(
        "process",
        vec![
            shape("gw_s", "gw", 10.0, 30.0, 40.0, 40.0),
            shape("task_s", "task", 150.0, 10.0, 100.0, 80.0),
        ],
        edges,
    ));
    d
}

// Sequence Flow (section 7.2.2 notation): a line with arrowhead and its name label.
#[test]
fn sequence_flow_draws_line_label_and_hits() {
    let without = build(&flow_doc(false));
    let with = build(&flow_doc(true));
    assert!(with.scene.paths().count() > without.scene.paths().count());
    assert!(with.scene.to_svg().contains("Approved"));
    assert!(!without.scene.to_svg().contains("Approved"));
    let hit = with.hit_test(100.0, 50.0, 3.0).unwrap();
    assert_eq!(hit.key.xml_id, id::<ElementSelector>("sf"));
}

// Inclusive Gateway `default`: the default flow carries a slash marker (section 10.5.3).
#[test]
fn inclusive_gateway_default_flow_draws_slash() {
    let plain = build(&flow_doc(true));
    let mut d = flow_doc(true);
    d.processes[0].nodes[0].details = Some(NodeDetails::Gateway {
        default_flow: Some(id("sf")),
        direction: None,
        instantiate: None,
        event_gateway_type: None,
        activation_condition: None,
    });
    let marked = build(&d);
    assert!(marked.scene.paths().count() > plain.scene.paths().count());
    assert_ne!(marked.scene.to_svg(), plain.scene.to_svg());
    // pointing `default` at an unrelated id leaves the flow unmarked
    d.processes[0].nodes[0].details = Some(NodeDetails::Gateway {
        default_flow: Some(id("other")),
        direction: None,
        instantiate: None,
        event_gateway_type: None,
        activation_condition: None,
    });
    assert_eq!(build(&d).scene.to_svg(), plain.scene.to_svg());
}

// BPMNPlane.bpmnElement: the plane owner scopes which nested children are visible. A child of a
// collapsed Sub-Process is hidden in the parent plane and shown in a plane owned by the Sub-Process.
#[test]
fn plane_bpmn_element_scopes_nested_children() {
    use processcraft_core::model::FlowContainer;
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    let mut sub = node("sub", "", NodeKind::SubProcess);
    sub.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            lane_set_name: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![node("child", "Nested task", NodeKind::Task)],
            lanes: vec![],
            flows: vec![],
        }),
    });
    d.processes.push(process(vec![sub], vec![], vec![]));
    let mut sub_shape = shape("sub_s", "sub", 0.0, 0.0, 400.0, 250.0);
    sub_shape.is_expanded = Some(false);
    let child_shape = shape("child_s", "child", 80.0, 60.0, 120.0, 80.0);
    d.diagrams.push(diagram(
        "process",
        vec![sub_shape, child_shape.clone()],
        vec![],
    ));
    let parent_plane = build(&d);
    assert!(!parent_plane.scene.to_svg().contains("Nested task"));
    let mut child_plane = diagram("sub", vec![child_shape], vec![]);
    child_plane.id = id("child_diagram");
    d.diagrams.push(child_plane);
    let scene = BpmnScene::build(
        &d,
        &id("child_diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert!(scene.scene.to_svg().contains("Nested task"));
    let hit = scene.hit_test(100.0, 100.0, 0.0).unwrap();
    assert_eq!(hit.key.xml_id, id::<ElementSelector>("child"));
}

// Lane (section 10.7.2 / Table 12.25): lane shape with its name label, hit-tested to the lane.
#[test]
fn lane_renders_name_and_hit_tests() {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(process(
        vec![node("task", "Ship", NodeKind::Task)],
        vec![Lane {
            base: None,
            child_lane_set_id: None,
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            id: id("lane"),
            name: Some("Warehouse".into()),
            parent: None,
            members: vec![id("task")],
        }],
        vec![],
    ));
    d.diagrams.push(diagram(
        "process",
        vec![
            shape("lane_s", "lane", 0.0, 0.0, 400.0, 200.0),
            shape("task_s", "task", 100.0, 60.0, 100.0, 80.0),
        ],
        vec![],
    ));
    let s = build(&d);
    assert!(s.scene.to_svg().contains("Warehouse"));
    let hit = s.hit_test(350.0, 150.0, 2.0).unwrap();
    assert_eq!(hit.key.xml_id, id::<ElementSelector>("lane"));
    let task = s.hit_test(150.0, 100.0, 2.0).unwrap();
    assert_eq!(task.key.xml_id, id::<ElementSelector>("task"));
    d.processes[0].lanes[0].name = Some("Dock".into());
    let svg = build(&d).scene.to_svg();
    assert!(svg.contains("Dock") && !svg.contains("Warehouse"));
}

// Group (Table 8.21, Category section; ":" chosen over "." of Table 8.21): label is the CategoryValue value prepended by the
// Category name and a delineator.
#[test]
fn group_label_is_category_name_and_value() {
    let mut d = flow_doc(false);
    d.categories.push(Category {
        base: None,
        id: id("cat"),
        name: Some("Region".into()),
        values: vec![CategoryValue {
            base: None,
            id: id("val"),
            value: Some("North".into()),
        }],
    });
    d.processes[0].groups.push(Group {
        base: None,
        id: id("grp"),
        category_value_ref: Some(local("val")),
    });
    d.diagrams[0]
        .shapes
        .push(shape("grp_s", "grp", 300.0, 0.0, 160.0, 120.0));
    let svg = build(&d).scene.to_svg();
    assert!(svg.contains("Region: North"), "{svg}");
    d.categories[0].name = Some("Area".into());
    let svg = build(&d).scene.to_svg();
    assert!(svg.contains("Area: North") && !svg.contains("Region"));
    d.categories[0].name = None;
    let svg = build(&d).scene.to_svg();
    assert!(svg.contains("North") && !svg.contains("Area"));
}

fn participant(pid: &str, name: &str) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(pid),
        name: Some(name.into()),
        process: None,
    }
}
// Choreography Task (Choreography Task notation, section 11.5.1): the initiating Participant band is on top.
// Without explicit band shapes the renderer derives band order from initiatingParticipantRef.
#[test]
fn choreography_initiating_participant_selects_top_band() {
    let doc = |initiating: &str| {
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
            participants: vec![participant("p1", "Alpha"), participant("p2", "Omega")],
            message_flows: vec![],
            activities: vec![ChoreographyActivity {
                loop_type: None,
                details: None,
                flow_metadata: None,
                correlation_keys: vec![],
                base: None,
                id: id("task"),
                name: Some("Exchange".into()),
                participant_refs: vec![local("p1"), local("p2")],
                initiating_participant: Some(local(initiating)),
                message_flow_refs: vec![],
            }],
            sequence_flows: vec![],
        });
        d.diagrams.push(diagram(
            "choreo",
            vec![shape("task_s", "task", 0.0, 0.0, 260.0, 140.0)],
            vec![],
        ));
        d
    };
    let order = |svg: &str| (svg.find("Alpha").unwrap(), svg.find("Omega").unwrap());
    let a = build(&doc("p1")).scene.to_svg();
    let (alpha, omega) = order(&a);
    assert!(alpha < omega, "initiator Alpha is the top band");
    let b = build(&doc("p2")).scene.to_svg();
    let (alpha, omega) = order(&b);
    assert!(omega < alpha, "initiator Omega is the top band");
}

// Pool (Participant) and Message Flow: both are drawn and hit-test to their own elements.
#[test]
fn participant_pool_and_message_flow_hit_test() {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(process(vec![], vec![], vec![]));
    d.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        participants: vec![
            participant("buyer", "Buyer"),
            participant("seller", "Seller"),
        ],
        flows: vec![flow(
            "mf",
            FlowKind::Message,
            "buyer",
            "seller",
            Some("Order"),
        )],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    d.diagrams.push(diagram(
        "collab",
        vec![
            shape("buyer_s", "buyer", 0.0, 0.0, 400.0, 100.0),
            shape("seller_s", "seller", 0.0, 220.0, 400.0, 100.0),
        ],
        vec![edge("mf_edge", "mf", &[(200.0, 100.0), (200.0, 220.0)])],
    ));
    let s = build(&d);
    let svg = s.scene.to_svg();
    assert!(svg.contains("Buyer") && svg.contains("Seller") && svg.contains("Order"));
    let key = |x, y| s.hit_test(x, y, 2.0).unwrap().key.xml_id.clone();
    assert_eq!(key(20.0, 20.0), id::<ElementSelector>("buyer"));
    assert_eq!(key(20.0, 260.0), id::<ElementSelector>("seller"));
    assert_eq!(key(200.0, 160.0), id::<ElementSelector>("mf"));
}
