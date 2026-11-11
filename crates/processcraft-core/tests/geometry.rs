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
        is_expanded: None,
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        bounds: rect(x, 0.0, 10.0, 10.0),
        label: None,
        style: None,
    };
    let diagram = Diagram {
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
                name: "Renamed".into(),
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
    let diagram_id: processcraft_core::ids::XmlId = "diagram".parse().unwrap();
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
    let diagram_id: processcraft_core::ids::XmlId = "diagram".parse().unwrap();
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
    parent.name.clear();
    parent.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
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
        is_expanded: Some(true),
        bounds: rect(-10.0, -10.0, 80.0, 50.0),
        label: None,
        style: None,
    });
    diagram.edges[0].label = Some(Label {
        bounds: rect(14.0, 12.0, 20.0, 8.0),
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
    let diagram_id: processcraft_core::ids::XmlId = "diagram".parse().unwrap();
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
    assert_eq!((label.bounds.x, label.bounds.y), (114.0, 62.0));
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
    let diagram_id: processcraft_core::ids::XmlId = "diagram".parse().unwrap();
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
    let diagram_id: processcraft_core::ids::XmlId = "diagram".parse().unwrap();
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
