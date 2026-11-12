use processcraft_core::{
    geometry::*,
    model::{Bounds, NodeKind, Point},
};
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn rect(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}
fn coords(points: &[Point]) -> Vec<(f64, f64)> {
    points.iter().map(|p| (p.x, p.y)).collect()
}
#[test]
fn routing_is_deterministic() {
    let obstacle = rect(4.0, -1.0, 2.0, 2.0);
    let a = route(
        p(0.0, 0.0),
        p(10.0, 0.0),
        std::slice::from_ref(&obstacle),
        1.0,
        100,
    )
    .unwrap();
    let b = route(p(0.0, 0.0), p(10.0, 0.0), &[obstacle], 1.0, 100).unwrap();
    assert_eq!(coords(&a.waypoints.unwrap()), coords(&b.waypoints.unwrap()));
}
#[test]
fn routing_budget_returns_warning() {
    let r = route(p(0.0, 0.0), p(1.0, 1.0), &[], 0.0, 0).unwrap();
    assert!(r.waypoints.is_none() && r.warning.is_some());
}
#[test]
fn bad_coordinates_are_rejected() {
    assert!(route(p(f64::NAN, 0.0), p(1.0, 1.0), &[], 0.0, 10).is_err());
    assert!(!valid_bounds(&rect(1e9, 0.0, 1.0, 1.0)));
    assert!(valid_point(&p(-1e9, 1e9)));
}
#[test]
fn overlapping_obstacle_refuses_route() {
    let r = route(
        p(0.0, 0.0),
        p(10.0, 0.0),
        &[rect(-1.0, -1.0, 2.0, 2.0)],
        0.0,
        100,
    )
    .unwrap();
    assert!(r.waypoints.is_none() && r.warning.is_some());
}
#[test]
fn manual_route_survives_unrelated_edit() {
    let points = vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 5.0), p(10.0, 5.0)];
    let repaired = repair_endpoints(&points, None, None, &[]).unwrap();
    assert_eq!(coords(&repaired.waypoints.unwrap()), coords(&points));
}
#[test]
fn moved_endpoint_preserves_manual_interior_and_warns() {
    let points = vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 5.0), p(10.0, 5.0)];
    let r = repair_endpoints(
        &points,
        Some(p(0.0, 2.0)),
        None,
        &[rect(3.0, -1.0, 2.0, 4.0)],
    )
    .unwrap();
    let path = r.waypoints.unwrap();
    assert!(coords(&path).contains(&(4.0, 0.0)));
    assert!(coords(&path).contains(&(4.0, 5.0)));
    assert!(r.warning.is_some());
}
#[test]
fn semantic_hits_and_zoom() {
    let b = rect(0.0, 0.0, 10.0, 10.0);
    assert!(!hit_shape(NodeKind::ExclusiveGateway, &b, &p(0.0, 0.0), 0.0, 1.0).unwrap());
    assert!(hit_shape(NodeKind::Task, &b, &p(11.0, 5.0), 2.0, 2.0).unwrap());
    assert!(hit_shape(NodeKind::Task, &b, &p(0.0, 0.0), 0.0, 0.0).is_err());
}
#[test]
fn zero_distance_and_extreme_finite() {
    let r = route(p(1e9, 1e9), p(1e9, 1e9), &[], 0.0, 10).unwrap();
    assert!(r.waypoints.is_some());
}

fn session_fixture() -> (
    processcraft_core::commands::Session,
    processcraft_core::ids::DocumentId,
) {
    use processcraft_core::{
        commands::{Envelope, Mutation, Request, Session},
        model::*,
    };
    let mut session = Session::new(ProjectData::new("project".into())).unwrap();
    let document_id = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let fixture: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    let env = |s: &Session| Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    session.apply(env(&session), fixture).unwrap();
    let shape = |id: &str, element: &str, x| Shape {
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        bounds: rect(x, 0.0, 10.0, 10.0),
        label: None,
        style: None,
    };
    let diagram = Diagram {
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: "diagram".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "procurement".parse().unwrap(),
        shapes: vec![
            shape("shape_start", "start", 0.0),
            shape("shape_review", "review", 30.0),
        ],
        edges: vec![Edge {
            id: "edge".parse().unwrap(),
            element: "flow_start_review".parse().unwrap(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: vec![p(10.0, 5.0), p(20.0, 5.0), p(20.0, 8.0), p(30.0, 8.0)],
            label: None,
            style: None,
        }],
    };
    session
        .apply(
            env(&session),
            Request::Batch {
                mutations: vec![Mutation::CreateDiagram {
                    document_id,
                    diagram,
                }],
            },
        )
        .unwrap();
    (session, document_id)
}
fn apply_session(
    s: &mut processcraft_core::commands::Session,
    request: processcraft_core::commands::Request,
) {
    let envelope = processcraft_core::commands::Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request).unwrap();
}
#[test]
fn drag_commits_once() {
    use processcraft_core::commands::{Request, ShapeBounds};
    let (mut s, doc) = session_fixture();
    let before = s.snapshot();
    let bounds = vec![
        ShapeBounds {
            diagram_id: "diagram".parse().unwrap(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(1.0, 2.0, 10.0, 10.0),
        },
        ShapeBounds {
            diagram_id: "diagram".parse().unwrap(),
            shape_id: "shape_review".parse().unwrap(),
            bounds: rect(31.0, 2.0, 10.0, 10.0),
        },
    ];
    let routes = vec![(
        "diagram".parse().unwrap(),
        "edge".parse().unwrap(),
        vec![p(11.0, 7.0), p(31.0, 7.0)],
    )];
    apply_session(&mut s, move_multiple(doc, bounds, routes).unwrap());
    let after = s.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    assert_ne!(after.content_id, before.content_id);
    let diagram = &after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    assert_eq!(diagram.shapes[0].bounds.x, 1.0);
    assert_eq!(diagram.shapes[1].bounds.x, 31.0);
    assert_eq!(
        coords(&diagram.edges[0].waypoints),
        vec![(11.0, 7.0), (31.0, 7.0)]
    );
    apply_session(&mut s, Request::Undo);
    assert_eq!(s.snapshot().content_id, before.content_id);
}
#[test]
fn session_rename_preserves_manual_route() {
    use processcraft_core::commands::{Mutation, Request};
    let (mut s, doc) = session_fixture();
    let before = coords(
        &s.snapshot().project.documents[0]
            .diagrams
            .iter()
            .find(|d| d.id.to_string() == "diagram")
            .unwrap()
            .edges[0]
            .waypoints,
    );
    apply_session(
        &mut s,
        Request::Batch {
            mutations: vec![Mutation::Rename {
                document_id: doc,
                element_id: "review".parse().unwrap(),
                name: Some("Renamed".into()),
            }],
        },
    );
    assert_eq!(
        coords(
            &s.snapshot().project.documents[0]
                .diagrams
                .iter()
                .find(|d| d.id.to_string() == "diagram")
                .unwrap()
                .edges[0]
                .waypoints
        ),
        before
    );
}
#[test]
fn diagonal_manual_route_warns_without_obstacles() {
    let result =
        repair_endpoints(&[p(0.0, 0.0), p(2.0, 2.0), p(4.0, 2.0)], None, None, &[]).unwrap();
    assert!(result.warning.is_some());
}
#[test]
fn prepared_move_repairs_connected_edge_in_one_undo() {
    use processcraft_core::commands::{Request, ShapeBounds};
    let (mut s, _) = session_fixture();
    let before = s.snapshot();
    let document = &before.project.documents[0];
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let moved = vec![ShapeBounds {
        diagram_id: diagram_id.clone(),
        shape_id: "shape_start".parse().unwrap(),
        bounds: rect(2.0, 3.0, 10.0, 10.0),
    }];
    let prepared = prepare_move(document, &diagram_id, moved).unwrap();
    apply_session(&mut s, prepared.request);
    let after = s.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(
        coords(&diagram.edges[0].waypoints),
        vec![
            (12.0, 8.0),
            (20.0, 8.0),
            (20.0, 5.0),
            (20.0, 8.0),
            (30.0, 8.0)
        ]
    );
    assert_eq!(diagram.shapes[1].bounds.x, 30.0);
    assert_eq!(
        after.project.documents[0].processes[0].nodes[1].name,
        before.project.documents[0].processes[0].nodes[1].name
    );
    apply_session(&mut s, Request::Undo);
    assert_eq!(s.snapshot().content_id, before.content_id);
}
#[test]
fn prepared_group_move_keeps_manual_bends_and_undoes_together() {
    use processcraft_core::commands::{Request, ShapeBounds};
    let (mut s, _) = session_fixture();
    let before = s.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let moved = vec![
        ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        },
        ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_review".parse().unwrap(),
            bounds: rect(32.0, 3.0, 10.0, 10.0),
        },
    ];
    let prepared = prepare_move(&before.project.documents[0], &diagram_id, moved).unwrap();
    apply_session(&mut s, prepared.request);
    let after = s.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    let path = coords(&diagram.edges[0].waypoints);
    assert_eq!(path.first(), Some(&(12.0, 8.0)));
    assert_eq!(path.last(), Some(&(32.0, 11.0)));
    assert!(path.contains(&(20.0, 5.0)) && path.contains(&(20.0, 8.0)));
    assert_eq!(after.undo_count, before.undo_count + 1);
    apply_session(&mut s, Request::Undo);
    assert_eq!(s.snapshot().content_id, before.content_id);
}
fn nested_session() -> processcraft_core::commands::Session {
    use processcraft_core::{
        bpmn_model::NodeDetails,
        commands::Session,
        model::{FlowContainer, Label, Shape, Style},
    };
    let (source, _) = session_fixture();
    let mut project = (*source.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let process = &mut doc.processes[0];
    let mut parent = process.nodes[0].clone();
    parent.id = "subprocess".parse().unwrap();
    parent.kind = NodeKind::SubProcess;
    parent.name = Some(String::new());
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            nodes: std::mem::take(&mut process.nodes),
            lanes: std::mem::take(&mut process.lanes),
            flows: std::mem::take(&mut process.flows),
            data_objects: std::mem::take(&mut process.data_objects),
            data_associations: std::mem::take(&mut process.data_associations),
        }),
    });
    process.nodes.push(parent);
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.shapes.push(Shape {
        id: "sub_di".parse().unwrap(),
        element: "subprocess".parse().unwrap(),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: Some(true),
        bounds: rect(-10.0, -10.0, 80.0, 50.0),
        label: None,
        style: None,
    });
    diagram.edges[0].label = Some(Label {
        id: None,
        style_ref: None,
        bounds: Some(rect(14.0, 12.0, 20.0, 8.0)),
        style: Some(Style {
            fill: None,
            stroke: None,
            font: Some("#123456".into()),
        }),
    });
    Session::new(project).unwrap()
}
#[test]
fn nested_parent_move_translates_children_internal_route_and_label_in_one_undo() {
    use processcraft_core::commands::{Request, ShapeBounds};
    let mut session = nested_session();
    let before = session.snapshot();
    let doc = &before.project.documents[0];
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let prepared = prepare_move(
        doc,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "sub_di".parse().unwrap(),
            bounds: rect(90.0, 40.0, 80.0, 50.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    let child = diagram
        .shapes
        .iter()
        .find(|s| s.id.to_string() == "shape_start")
        .unwrap();
    assert_eq!((child.bounds.x, child.bounds.y), (100.0, 50.0));
    assert_eq!(
        coords(&diagram.edges[0].waypoints),
        vec![(110.0, 55.0), (120.0, 55.0), (120.0, 58.0), (130.0, 58.0)]
    );
    let label = diagram.edges[0].label.as_ref().unwrap();
    assert_eq!(
        (
            label.bounds.as_ref().unwrap().x,
            label.bounds.as_ref().unwrap().y
        ),
        (114.0, 62.0)
    );
    assert_eq!(
        label.style.as_ref().unwrap().font.as_deref(),
        Some("#123456")
    );
    assert_eq!(after.undo_count, before.undo_count + 1);
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}
#[test]
fn conflicting_nested_moves_are_refused_without_mutation() {
    use processcraft_core::commands::ShapeBounds;
    let session = nested_session();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let moves = vec![
        ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "sub_di".parse().unwrap(),
            bounds: rect(90.0, 40.0, 80.0, 50.0),
        },
        ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(5.0, 5.0, 10.0, 10.0),
        },
    ];
    assert!(prepare_move(&before.project.documents[0], &diagram_id, moves).is_err());
    assert_eq!(before.content_id, session.snapshot().content_id);
}
#[test]
fn moving_nested_child_repairs_nested_flow_without_moving_container() {
    use processcraft_core::commands::ShapeBounds;
    let mut session = nested_session();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let request = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    assert!(request.warnings.is_empty());
    apply_session(&mut session, request.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(
        coords(&diagram.edges[0].waypoints).first(),
        Some(&(12.0, 8.0))
    );
    let parent = diagram
        .shapes
        .iter()
        .find(|s| s.id.to_string() == "sub_di")
        .unwrap();
    assert_eq!((parent.bounds.x, parent.bounds.y), (-10.0, -10.0));
}

fn assert_geometry_history(
    mut session: processcraft_core::commands::Session,
    shape_id: &str,
    new_bounds: Bounds,
    edge_id: &str,
    expected_first: bool,
) {
    use processcraft_core::commands::{Request, ShapeBounds};
    let before = session.snapshot();
    before.project.validate().unwrap();
    let document = &before.project.documents[0];
    let diagram_id = document.diagrams.last().unwrap().id.selector();
    let edge = document
        .diagrams
        .last()
        .unwrap()
        .edges
        .iter()
        .find(|e| e.id.to_string() == edge_id)
        .unwrap();
    let old_points = coords(&edge.waypoints);
    let old_label = serde_json::to_value(&edge.label).unwrap();
    let shape = document
        .diagrams
        .last()
        .unwrap()
        .shapes
        .iter()
        .find(|s| s.id.to_string() == shape_id)
        .unwrap();
    let dx = new_bounds.x - shape.bounds.x;
    let dy = new_bounds.y - shape.bounds.y;
    let prepared = prepare_move(
        document,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: shape.id.selector(),
            bounds: new_bounds,
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    after.project.validate().unwrap();
    let edge = after.project.documents[0]
        .diagrams
        .last()
        .unwrap()
        .edges
        .iter()
        .find(|e| e.id.to_string() == edge_id)
        .unwrap();
    let points = coords(&edge.waypoints);
    let i = if expected_first { 0 } else { points.len() - 1 };
    let old_i = if expected_first {
        0
    } else {
        old_points.len() - 1
    };
    assert_eq!(
        points[i],
        (old_points[old_i].0 + dx, old_points[old_i].1 + dy)
    );
    assert_eq!(serde_json::to_value(&edge.label).unwrap(), old_label);
    assert_eq!(after.undo_count, before.undo_count + 1);
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}

fn data_geometry_session() -> processcraft_core::commands::Session {
    use processcraft_core::{
        bpmn_data::{
            DataAssociation, DataAssociationKind, DataObject, DataPort, IoSet, IoSpecification,
        },
        bpmn_model::NodeDetails,
        commands::Session,
        model::{Edge, Label, Node, NodeKind, Shape},
    };
    let (source, _) = session_fixture();
    let mut project = (*source.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let process = &mut doc.processes[0];
    process.data_objects.push(DataObject {
        base: None,
        id: Some("data_object".parse().unwrap()),
        name: Some("Order".into()),
        is_collection: None,
        item_aware: None,
    });
    process.nodes.push(Node {
        item_aware: None,
        id: "data_ref".parse().unwrap(),
        name: Some("Order".into()),
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::DataObjectReference,
        details: Some(NodeDetails::DataObjectReference {
            data_object_ref: Some("data_object".parse().unwrap()),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    });
    process
        .nodes
        .iter_mut()
        .find(|n| n.id.to_string() == "review")
        .unwrap()
        .io_specification = Some(IoSpecification {
        base: None,
        id: None,
        inputs: vec![DataPort {
            base: None,
            id: Some("input_port".parse().unwrap()),
            name: Some(String::new()),
            item_aware: None,
            is_collection: None,
        }],
        outputs: vec![DataPort {
            base: None,
            id: Some("output_port".parse().unwrap()),
            name: Some(String::new()),
            item_aware: None,
            is_collection: None,
        }],
        input_sets: vec![IoSet {
            data_refs: vec!["input_port".parse().unwrap()],
            ..Default::default()
        }],
        output_sets: vec![IoSet {
            data_refs: vec!["output_port".parse().unwrap()],
            ..Default::default()
        }],
    });
    for (id, kind, source, target) in [
        (
            "input_assoc",
            DataAssociationKind::Input,
            "data_ref",
            "input_port",
        ),
        (
            "output_assoc",
            DataAssociationKind::Output,
            "output_port",
            "data_ref",
        ),
    ] {
        process.data_associations.push(DataAssociation {
            base: None,
            id: Some(id.parse().unwrap()),
            transformation: None,
            assignments: vec![],
            owner: "review".parse().unwrap(),
            kind,
            sources: vec![source.parse().unwrap()],
            target: target.parse().unwrap(),
        });
    }
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.shapes.push(Shape {
        id: "shape_data".parse().unwrap(),
        element: "data_ref".parse().unwrap(),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        bounds: rect(60.0, 0.0, 10.0, 10.0),
        label: None,
        style: None,
    });
    for (id, element, points) in [
        (
            "input_edge",
            "input_assoc",
            vec![p(60.0, 5.0), p(40.0, 5.0)],
        ),
        (
            "output_edge",
            "output_assoc",
            vec![p(40.0, 8.0), p(60.0, 8.0)],
        ),
    ] {
        diagram.edges.push(Edge {
            id: id.parse().unwrap(),
            element: element.parse().unwrap(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: points,
            label: Some(Label {
                id: None,
                style_ref: None,
                bounds: Some(rect(45.0, 10.0, 8.0, 4.0)),
                style: None,
            }),
            style: None,
        });
    }
    project.validate().unwrap();
    Session::new(project).unwrap()
}

#[test]
fn valid_data_association_endpoints_move_undo_and_redo() {
    for (shape, bounds, edge, first) in [
        (
            "shape_data",
            rect(62.0, 3.0, 10.0, 10.0),
            "input_edge",
            true,
        ),
        (
            "shape_review",
            rect(32.0, 3.0, 10.0, 10.0),
            "input_edge",
            false,
        ),
        (
            "shape_review",
            rect(32.0, 3.0, 10.0, 10.0),
            "output_edge",
            true,
        ),
        (
            "shape_data",
            rect(62.0, 3.0, 10.0, 10.0),
            "output_edge",
            false,
        ),
    ] {
        assert_geometry_history(data_geometry_session(), shape, bounds, edge, first);
    }
}

fn conversation_geometry_session() -> processcraft_core::commands::Session {
    use processcraft_core::{
        bpmn_collaboration::{ConversationKind, ConversationLink, ConversationNode},
        commands::Session,
        model::{Collaboration, Diagram, Edge, Label, Participant, Shape},
    };
    let (source, _) = session_fixture();
    let mut project = (*source.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    doc.collaboration = Some(Collaboration {
        base: None,
        name: None,
        id: "collaboration".parse().unwrap(),
        participants: vec![Participant {
            base: None,
            id: "buyer".parse().unwrap(),
            name: Some("Buyer".into()),
            process: None,
        }],
        flows: vec![],
        conversation_nodes: vec![ConversationNode {
            base: None,
            id: "conversation".parse().unwrap(),
            name: Some("Order".into()),
            kind: ConversationKind::Conversation,
            participant_refs: vec!["buyer".parse().unwrap()],
            parent: None,
        }],
        conversation_links: vec![ConversationLink {
            base: None,
            id: "link".parse().unwrap(),
            name: Some(String::new()),
            source: "buyer".parse().unwrap(),
            target: "conversation".parse().unwrap(),
        }],
    });
    let shape = |id: &str, element: &str, x| Shape {
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        bounds: rect(x, 0.0, 10.0, 10.0),
        label: None,
        style: None,
    };
    doc.diagrams.push(Diagram {
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: "conversation_diagram".parse().unwrap(),
        plane_id: "conversation_plane".parse().unwrap(),
        plane: "collaboration".parse().unwrap(),
        shapes: vec![
            shape("buyer_shape", "buyer", 0.0),
            shape("conversation_shape", "conversation", 30.0),
        ],
        edges: vec![Edge {
            id: "link_edge".parse().unwrap(),
            element: "link".parse().unwrap(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: vec![p(10.0, 5.0), p(20.0, 5.0), p(20.0, 8.0), p(30.0, 8.0)],
            label: Some(Label {
                id: None,
                style_ref: None,
                bounds: Some(rect(15.0, 10.0, 8.0, 4.0)),
                style: None,
            }),
            style: None,
        }],
    });
    project.validate().unwrap();
    Session::new(project).unwrap()
}

#[test]
fn valid_conversation_link_endpoints_move_undo_and_redo() {
    assert_geometry_history(
        conversation_geometry_session(),
        "buyer_shape",
        rect(2.0, 3.0, 10.0, 10.0),
        "link_edge",
        true,
    );
    assert_geometry_history(
        conversation_geometry_session(),
        "conversation_shape",
        rect(32.0, 3.0, 10.0, 10.0),
        "link_edge",
        false,
    );
}

#[test]
fn semantic_hits_cover_all_event_and_gateway_shapes() {
    let b = rect(0.0, 0.0, 10.0, 10.0);
    for kind in [
        NodeKind::StartEvent,
        NodeKind::EndEvent,
        NodeKind::IntermediateCatchEvent,
        NodeKind::IntermediateThrowEvent,
        NodeKind::BoundaryEvent,
        NodeKind::ExclusiveGateway,
        NodeKind::ParallelGateway,
        NodeKind::InclusiveGateway,
        NodeKind::ComplexGateway,
        NodeKind::EventBasedGateway,
    ] {
        assert!(!hit_shape(kind, &b, &p(0.0, 0.0), 0.0, 1.0).unwrap());
        assert!(hit_shape(kind, &b, &p(5.0, 5.0), 0.0, 1.0).unwrap());
        assert!(hit_shape(kind, &b, &p(11.0, 5.0), 2.0, 2.0).unwrap());
    }
}

#[test]
fn data_artifact_hits_follow_fold_and_cylinder() {
    let object = rect(0.0, 0.0, 40.0, 50.0);
    assert!(
        !hit_shape(
            NodeKind::DataObjectReference,
            &object,
            &p(39.0, 1.0),
            0.0,
            1.0
        )
        .unwrap()
    );
    assert!(
        hit_shape(
            NodeKind::DataObjectReference,
            &object,
            &p(30.0, 5.0),
            0.0,
            1.0
        )
        .unwrap()
    );
    assert!(
        hit_shape(
            NodeKind::DataObjectReference,
            &object,
            &p(39.0, 1.0),
            8.0,
            1.0
        )
        .unwrap()
    );
    let store = rect(0.0, 0.0, 40.0, 50.0);
    assert!(!hit_shape(NodeKind::DataStoreReference, &store, &p(0.0, 0.0), 0.0, 1.0).unwrap());
    assert!(
        hit_shape(
            NodeKind::DataStoreReference,
            &store,
            &p(20.0, 25.0),
            0.0,
            1.0
        )
        .unwrap()
    );
}

#[test]
fn moving_activity_translates_visible_port_and_repairs_association() {
    use processcraft_core::{
        commands::{Request, Session, ShapeBounds},
        model::Shape,
    };
    let initial = data_geometry_session();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.shapes.push(Shape {
        id: "input_shape".parse().unwrap(),
        element: "input_port".parse().unwrap(),
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        bounds: rect(42.0, 0.0, 10.0, 10.0),
        label: None,
        style: None,
    });
    diagram
        .edges
        .iter_mut()
        .find(|e| e.id.to_string() == "input_edge")
        .unwrap()
        .waypoints = vec![p(60.0, 5.0), p(52.0, 5.0)];
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_review".parse().unwrap(),
            bounds: rect(32.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(
        diagram
            .shapes
            .iter()
            .find(|s| s.id.to_string() == "input_shape")
            .unwrap()
            .bounds
            .x,
        44.0
    );
    assert_eq!(
        coords(
            &diagram
                .edges
                .iter()
                .find(|e| e.id.to_string() == "input_edge")
                .unwrap()
                .waypoints
        )
        .last(),
        Some(&(54.0, 8.0))
    );
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}

#[test]
fn global_data_store_association_moves_and_restores_route() {
    use processcraft_core::{bpmn_data::DataStore, commands::Session};
    let initial = data_geometry_session();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    doc.data_stores.push(DataStore {
        base: None,
        id: Some("global_store".parse().unwrap()),
        name: Some("Archive".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: None,
    });
    doc.processes[0].data_associations[0].sources[0] = "global_store".parse().unwrap();
    doc.processes[0].data_associations[1].target = "global_store".parse().unwrap();
    doc.diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap()
        .shapes
        .iter_mut()
        .find(|s| s.id.to_string() == "shape_data")
        .unwrap()
        .element = "global_store".parse().unwrap();
    project.validate().unwrap();
    assert_geometry_history(
        Session::new(project).unwrap(),
        "shape_data",
        rect(62.0, 3.0, 10.0, 10.0),
        "input_edge",
        true,
    );
}

#[test]
fn native_anonymous_shape_moves_connected_route_and_undoes() {
    use processcraft_core::{
        commands::{Request, Session, ShapeBounds},
        ids::{ElementIdentity, ElementSelector, NativeElementId},
    };
    let (original, _) = session_fixture();
    let mut project = (*original.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let node_native = NativeElementId::new();
    let shape_native = NativeElementId::new();
    let node = ElementSelector::Native {
        native_id: node_native,
    };
    let shape = ElementSelector::Native {
        native_id: shape_native,
    };
    let process = &mut doc.processes[0];
    process
        .nodes
        .iter_mut()
        .find(|n| n.id.to_string() == "start")
        .unwrap()
        .id = ElementIdentity::Native {
        native_id: node_native,
        xml_id: None,
    };
    for lane in &mut process.lanes {
        for member in &mut lane.members {
            if member.to_string() == "start" {
                *member = node.clone();
            }
        }
    }
    for flow in &mut process.flows {
        if flow.source.to_string() == "start" {
            flow.source = node.clone();
        }
        if flow.target.to_string() == "start" {
            flow.target = node.clone();
        }
    }
    for diagram in &mut doc.diagrams {
        for di in &mut diagram.shapes {
            if di.element.to_string() == "start" {
                di.element = node.clone();
            }
        }
    }
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    let di = diagram
        .shapes
        .iter_mut()
        .find(|s| s.id.to_string() == "shape_start")
        .unwrap();
    di.id = ElementIdentity::Native {
        native_id: shape_native,
        xml_id: None,
    };
    di.element = node;
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: ElementSelector = "diagram".parse().unwrap();
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: shape.clone(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(
        diagram
            .shapes
            .iter()
            .find(|s| s.id == shape)
            .unwrap()
            .bounds
            .x,
        2.0
    );
    assert_eq!(
        coords(&diagram.edges[0].waypoints).first(),
        Some(&(12.0, 8.0))
    );
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}

#[test]
fn explicit_di_endpoints_track_only_the_named_shape_and_undo() {
    use processcraft_core::commands::{Request, Session, ShapeBounds};
    let (initial, _) = session_fixture();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    let mut alternate = diagram
        .shapes
        .iter()
        .find(|s| s.id.to_string() == "shape_start")
        .unwrap()
        .clone();
    alternate.id = "alternate_start".parse().unwrap();
    alternate.bounds.x = 100.0;
    diagram.shapes.push(alternate);
    diagram.edges[0].source_element = Some("shape_start".parse().unwrap());
    diagram.edges[0].target_element = Some("shape_review".parse().unwrap());
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let unrelated = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "alternate_start".parse().unwrap(),
            bounds: rect(102.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, unrelated.request);
    let after_unrelated = session.snapshot();
    let d = after_unrelated.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(
        coords(&d.edges[0].waypoints),
        vec![(10.0, 5.0), (20.0, 5.0), (20.0, 8.0), (30.0, 8.0)]
    );
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let d = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    assert_eq!(coords(&d.edges[0].waypoints).first(), Some(&(12.0, 8.0)));
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}

#[test]
fn edge_representation_endpoint_move_is_refused_atomically() {
    use processcraft_core::{
        commands::{Session, ShapeBounds},
        model::Edge,
    };
    let (initial, _) = session_fixture();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let mut note = doc.processes[0].nodes[0].clone();
    note.id = "unrelated_note".parse().unwrap();
    note.kind = NodeKind::TextAnnotation;
    note.name = None;
    note.details = None;
    doc.processes[0].nodes.push(note);
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    let other = Edge {
        id: "anchor_edge".parse().unwrap(),
        ..diagram.edges[0].clone()
    };
    diagram.edges.push(other);
    diagram.edges[0].source_element = Some("anchor_edge".parse().unwrap());
    diagram.edges[1].target_element = Some("edge".parse().unwrap());
    let mut note_shape = diagram.shapes[0].clone();
    note_shape.id = "unrelated_shape".parse().unwrap();
    note_shape.element = "unrelated_note".parse().unwrap();
    note_shape.bounds = rect(200.0, 100.0, 20.0, 20.0);
    diagram.shapes.push(note_shape);
    project.validate().unwrap();
    let session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let result = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }],
    );
    assert!(matches!(
        result,
        Err("cyclic edge representation endpoints are unsupported")
    ));
    assert_eq!(session.snapshot().content_id, before.content_id);
    let unrelated = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "unrelated_shape".parse().unwrap(),
            bounds: rect(202.0, 103.0, 20.0, 20.0),
        }],
    );
    assert!(unrelated.is_ok());
}

#[test]
fn association_edge_attachment_follows_rerouted_sequence_with_undo() {
    use processcraft_core::{
        commands::{Request, Session, ShapeBounds},
        model::{Edge, Flow, FlowKind, Label, NodeKind, Shape},
    };
    let (initial, _) = session_fixture();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let process = &mut doc.processes[0];
    let mut annotation = process.nodes[0].clone();
    annotation.id = "note".parse().unwrap();
    annotation.name = Some("Note".into());
    annotation.kind = NodeKind::TextAnnotation;
    annotation.details = None;
    process.nodes.push(annotation);
    process.flows.push(Flow {
        id: "association".parse().unwrap(),
        kind: FlowKind::Association,
        source: "note".parse().unwrap(),
        target: "review".parse().unwrap(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
        association_direction: None,
    });
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.shapes.push(Shape {
        id: "note_shape".parse().unwrap(),
        element: "note".parse().unwrap(),
        is_expanded: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        bounds: rect(10.0, 40.0, 20.0, 10.0),
        label: None,
        style: None,
    });
    diagram.edges.push(Edge {
        id: "association_edge".parse().unwrap(),
        element: "association".parse().unwrap(),
        source_element: Some("note_shape".parse().unwrap()),
        label_visibility: None,
        target_element: Some("edge".parse().unwrap()),
        waypoints: vec![p(20.0, 40.0), p(20.0, 5.0)],
        label: Some(Label {
            id: None,
            style_ref: None,
            bounds: Some(rect(22.0, 20.0, 10.0, 5.0)),
            style: None,
        }),
        style: None,
    });
    for (source_attachment, move_note) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut project = project.clone();
        if source_attachment {
            let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
            let flow = doc.processes[0]
                .flows
                .iter_mut()
                .find(|f| f.id.to_string() == "association")
                .unwrap();
            std::mem::swap(&mut flow.source, &mut flow.target);
            let diagram = doc
                .diagrams
                .iter_mut()
                .find(|d| d.id.to_string() == "diagram")
                .unwrap();
            let edge = diagram
                .edges
                .iter_mut()
                .find(|e| e.id.to_string() == "association_edge")
                .unwrap();
            std::mem::swap(&mut edge.source_element, &mut edge.target_element);
            edge.waypoints.reverse();
        }
        project.validate().unwrap();
        let mut session = Session::new(project).unwrap();
        let before = session.snapshot();
        let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
        let mut moves = vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_start".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }];
        if move_note {
            moves.push(ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: "note_shape".parse().unwrap(),
                bounds: rect(12.0, 43.0, 20.0, 10.0),
            });
        }
        let prepared = prepare_move(&before.project.documents[0], &diagram_id, moves).unwrap();
        apply_session(&mut session, prepared.request);
        let after = session.snapshot();
        let diagram = after.project.documents[0]
            .diagrams
            .iter()
            .find(|d| d.id == diagram_id)
            .unwrap();
        let link = diagram
            .edges
            .iter()
            .find(|e| e.id.to_string() == "association_edge")
            .unwrap();
        let points = coords(&link.waypoints);
        let (fixed, attachment) = if source_attachment {
            (points.last(), points.first())
        } else {
            (points.first(), points.last())
        };
        assert_eq!(
            fixed,
            Some(&if move_note {
                (22.0, 43.0)
            } else {
                (20.0, 40.0)
            })
        );
        assert_ne!(attachment, Some(&(20.0, 5.0)));
        assert_eq!(
            link.label.as_ref().unwrap().bounds,
            Some(rect(22.0, 20.0, 10.0, 5.0))
        );
        apply_session(&mut session, Request::Undo);
        assert_eq!(session.snapshot().content_id, before.content_id);
        apply_session(&mut session, Request::Redo);
        assert_eq!(session.snapshot().content_id, after.content_id);
    }
}

#[test]
fn nested_move_translates_edge_attachment_and_label_as_one_gesture() {
    use processcraft_core::{
        bpmn_model::NodeDetails,
        commands::{Request, Session, ShapeBounds},
        model::{Edge, Flow, FlowKind, Label, NodeKind, Shape},
    };
    let initial = nested_session();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0].nodes[0].details else {
        panic!("subprocess")
    };
    let mut note = content.nodes[0].clone();
    note.id = "note".parse().unwrap();
    note.name = None;
    note.kind = NodeKind::TextAnnotation;
    note.details = None;
    content.nodes.push(note);
    content.flows.push(Flow {
        id: "association".parse().unwrap(),
        kind: FlowKind::Association,
        source: "note".parse().unwrap(),
        target: "review".parse().unwrap(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
        association_direction: None,
    });
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.shapes.push(Shape {
        id: "note_shape".parse().unwrap(),
        element: "note".parse().unwrap(),
        is_expanded: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        bounds: rect(10.0, 20.0, 20.0, 10.0),
        label: None,
        style: None,
    });
    diagram.edges.push(Edge {
        id: "association_edge".parse().unwrap(),
        element: "association".parse().unwrap(),
        source_element: Some("note_shape".parse().unwrap()),
        label_visibility: None,
        target_element: Some("edge".parse().unwrap()),
        waypoints: vec![p(20.0, 20.0), p(20.0, 5.0)],
        label: Some(Label {
            id: None,
            style_ref: None,
            bounds: Some(rect(22.0, 10.0, 10.0, 5.0)),
            style: None,
        }),
        style: None,
    });
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "sub_di".parse().unwrap(),
            bounds: rect(90.0, 40.0, 80.0, 50.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    let association = diagram
        .edges
        .iter()
        .find(|e| e.id.to_string() == "association_edge")
        .unwrap();
    assert_eq!(
        coords(&association.waypoints),
        vec![(120.0, 70.0), (120.0, 55.0)]
    );
    assert_eq!(
        association.label.as_ref().unwrap().bounds,
        Some(rect(122.0, 60.0, 10.0, 5.0))
    );
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}

#[test]
fn nested_move_keeps_unbounded_label_unmaterialized_and_undoable() {
    use processcraft_core::{
        commands::{Request, Session, ShapeBounds},
        model::DiLabelVisibility,
    };
    let initial = nested_session();
    let mut project = (*initial.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let diagram = doc
        .diagrams
        .iter_mut()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap();
    diagram.edges[0].label_visibility = Some(DiLabelVisibility::Explicit);
    let label = diagram.edges[0].label.as_mut().unwrap();
    label.id = Some("edge_label".parse().unwrap());
    label.bounds = None;
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "sub_di".parse().unwrap(),
            bounds: rect(90.0, 40.0, 80.0, 50.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let diagram = after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap();
    let label = diagram.edges[0].label.as_ref().unwrap();
    assert!(label.bounds.is_none());
    assert_eq!(label.id.as_ref().unwrap().to_string(), "edge_label");
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}
