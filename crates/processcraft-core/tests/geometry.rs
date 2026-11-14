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
/// An explicit edge label after a reroute: shifted by the route midpoint's delta.
fn followed(label: &Bounds, old: &[Point], new: &[Point]) -> Bounds {
    let (a, b) = (midpoint(old).unwrap(), midpoint(new).unwrap());
    rect(
        label.x + b.x - a.x,
        label.y + b.y - a.y,
        label.width,
        label.height,
    )
}
#[test]
fn connection_route_faces_shapes_and_avoids_obstacles() {
    let a = rect(0.0, 0.0, 100.0, 80.0);
    let right = connection_route(&a, &rect(200.0, 0.0, 100.0, 80.0), &[]);
    assert_eq!(coords(&right), vec![(100.0, 40.0), (200.0, 40.0)]);
    let below_right = connection_route(&a, &rect(300.0, 100.0, 100.0, 80.0), &[]);
    assert_eq!(below_right.len(), 4);
    assert_eq!(coords(&below_right)[0], (100.0, 40.0));
    assert_eq!(coords(&below_right)[3], (300.0, 140.0));
    let above = connection_route(&a, &rect(10.0, -300.0, 100.0, 80.0), &[]);
    assert_eq!((above[0].x, above[0].y), (50.0, 0.0));
    let left = connection_route(&a, &rect(-300.0, 0.0, 100.0, 80.0), &[]);
    assert_eq!((left[0].x, left[1].x), (0.0, -200.0));
    // A shape between the endpoints is routed around; an enclosing pool is not an obstacle.
    let target = rect(400.0, 0.0, 100.0, 80.0);
    let blocker = rect(200.0, 0.0, 100.0, 80.0);
    let pool = rect(-50.0, -50.0, 600.0, 300.0);
    let around = connection_route(
        &a,
        &target,
        &[a.clone(), blocker.clone(), pool, target.clone()],
    );
    assert!(
        around
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
    );
    assert!(
        around
            .iter()
            .all(|q| !(q.x > 200.0 && q.x < 300.0 && q.y > 0.0 && q.y < 80.0))
    );
    assert_ne!(around.len(), 2);
}
#[test]
fn preferred_layouts_shape_diagonal_connections() {
    let a = rect(0.0, 0.0, 50.0, 50.0);
    let b = rect(200.0, 200.0, 100.0, 80.0);
    // Out of a gateway: down, then right into the target (v:h).
    assert_eq!(
        coords(&layout_route(&a, &b, &[], RouteLayout::FromGateway)),
        vec![(25.0, 50.0), (25.0, 240.0), (200.0, 240.0)]
    );
    // Into a gateway: right, then down onto it (h:v).
    assert_eq!(
        coords(&layout_route(
            &b,
            &rect(400.0, 400.0, 50.0, 50.0),
            &[],
            RouteLayout::IntoGateway
        )),
        vec![(300.0, 240.0), (425.0, 240.0), (425.0, 400.0)]
    );
    // Sequence flows prefer h:h even when the vertical offset dominates.
    let tall = rect(100.0, 400.0, 100.0, 80.0);
    assert_eq!(
        layout_route(&a, &tall, &[], RouteLayout::Sequence)[0],
        p(50.0, 25.0)
    );
    assert_eq!(connection_route(&a, &tall, &[])[0], p(25.0, 50.0));
    // Message flows: v:v when diagonal, straight when overlapping horizontally.
    assert_eq!(
        coords(&layout_route(&a, &b, &[], RouteLayout::Message)),
        vec![(25.0, 50.0), (25.0, 125.0), (250.0, 125.0), (250.0, 200.0)]
    );
    assert_eq!(
        coords(&layout_route(
            &a,
            &rect(20.0, 200.0, 100.0, 80.0),
            &[],
            RouteLayout::Message
        )),
        vec![(35.0, 50.0), (35.0, 200.0)]
    );
    // A bottom boundary event leaves downwards even towards a target on its right.
    let boundary = rect(0.0, 0.0, 36.0, 36.0);
    assert_eq!(
        layout_route(&boundary, &b, &[], RouteLayout::FromBoundary(Side::Bottom))[0],
        p(18.0, 36.0)
    );
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        plane_metadata: None,
        plane_order: None,
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
            di_metadata: None,
            message_visible_kind: None,
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
    // The repaired route doubled back over the kept bends; normalization drops the spike.
    assert_eq!(
        coords(&diagram.edges[0].waypoints),
        vec![(12.0, 8.0), (30.0, 8.0)]
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
    // The kept bend column x = 20 stays; the spike back to y = 5 is normalized away.
    assert_eq!(
        path,
        vec![(12.0, 8.0), (20.0, 8.0), (20.0, 11.0), (32.0, 11.0)]
    );
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
            lane_set_name: None,
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        di_metadata: None,
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
fn reroute_tidies_the_route_and_its_label_follows() {
    use processcraft_core::commands::{Mutation, Request};
    // Endpoints stay; near-axis segments straighten from both ends; collinear points drop.
    assert_eq!(
        coords(&tidy(vec![
            p(0.0, 0.0),
            p(50.0, 1.0),
            p(50.0, 40.0),
            p(51.0, 80.0)
        ])),
        vec![(0.0, 0.0), (51.0, 0.0), (51.0, 80.0)]
    );
    let mut session = nested_session();
    let document_id = session.snapshot().project.documents[0].id;
    apply_session(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::Reroute {
                document_id,
                diagram_id: "diagram".parse().unwrap(),
                edge_id: "edge".parse().unwrap(),
                waypoints: vec![
                    p(10.0, 5.0),
                    p(14.0, 5.0),
                    p(26.0, 6.0),
                    p(26.0, 8.0),
                    p(30.0, 8.0),
                ],
            }],
        },
    );
    let after = session.snapshot();
    let edge = &after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap()
        .edges[0];
    assert_eq!(
        coords(&edge.waypoints),
        vec![(10.0, 5.0), (26.0, 5.0), (26.0, 8.0), (30.0, 8.0)]
    );
    // The midpoint moved from (20, 6.5) to (21.5, 5): the explicit label moved with it.
    let label = edge.label.as_ref().unwrap().bounds.as_ref().unwrap();
    assert_eq!((label.x, label.y), (15.5, 10.5));
}
#[test]
fn reroute_docks_ends_on_the_true_outline() {
    use processcraft_core::commands::{Mutation, Request};
    let b = rect(0.0, 0.0, 40.0, 20.0);
    let toward = p(40.0, 30.0);
    assert_eq!(outline_point(None, &b, &toward), p(30.0, 20.0));
    assert_eq!(
        outline_point(Some(NodeKind::ExclusiveGateway), &b, &toward),
        p(80.0 / 3.0, 50.0 / 3.0)
    );
    let on_circle = outline_point(Some(NodeKind::EndEvent), &b, &toward);
    assert!((((on_circle.x - 20.0) / 20.0).hypot((on_circle.y - 10.0) / 10.0) - 1.0).abs() < 1e-12);
    let (mut session, document_id) = session_fixture();
    // The start event is the circle in (0, 0, 10, 10): an off-centre vertical end at its
    // bounding-box bottom moves up onto the circle; the task end stays on its rectangle.
    apply_session(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::Reroute {
                document_id,
                diagram_id: "diagram".parse().unwrap(),
                edge_id: "edge".parse().unwrap(),
                waypoints: vec![p(3.0, 10.0), p(3.0, 20.0), p(35.0, 20.0), p(35.0, 10.0)],
            }],
        },
    );
    let after = session.snapshot();
    let route = &after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id.to_string() == "diagram")
        .unwrap()
        .edges[0]
        .waypoints;
    assert_eq!(route[0].x, 3.0);
    assert!(((route[0].x - 5.0).hypot(route[0].y - 5.0) - 5.0).abs() < 1e-12);
    assert!(route[0].y > 9.5 && route[0].y < 10.0);
    assert_eq!(
        coords(&route[1..]),
        vec![(3.0, 20.0), (35.0, 20.0), (35.0, 10.0)]
    );
}
#[test]
fn moved_end_redocks_on_the_side_facing_its_neighbour() {
    use processcraft_core::commands::ShapeBounds;
    // A kept end stays when its neighbour is still beyond its side.
    let b = rect(0.0, 0.0, 100.0, 80.0);
    assert_eq!(
        redock_side(&p(100.0, 20.0), &b, &p(150.0, 20.0)),
        p(100.0, 20.0)
    );
    // Otherwise it moves to the middle of the side facing the neighbour.
    assert_eq!(
        redock_side(&p(100.0, 20.0), &b, &p(40.0, 200.0)),
        p(50.0, 80.0)
    );
    assert_eq!(
        redock_side(&p(100.0, 20.0), &b, &p(-90.0, 30.0)),
        p(0.0, 40.0)
    );
    let (mut session, _) = session_fixture();
    let before = session.snapshot();
    let diagram_id: processcraft_core::ids::ElementSelector = "diagram".parse().unwrap();
    // The task moves above the bend column: its left-side end re-docks on its bottom.
    let prepared = prepare_move(
        &before.project.documents[0],
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "shape_review".parse().unwrap(),
            bounds: rect(15.0, -60.0, 10.0, 10.0),
        }],
    )
    .unwrap();
    apply_session(&mut session, prepared.request);
    let after = session.snapshot();
    let route = &after.project.documents[0]
        .diagrams
        .iter()
        .find(|d| d.id == diagram_id)
        .unwrap()
        .edges[0]
        .waypoints;
    assert_eq!(route.last(), Some(&p(20.0, -50.0)));
    assert!(route.contains(&p(20.0, 5.0)));
    assert!(
        route
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
    );
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
    let old_route = edge.waypoints.clone();
    let old_points = coords(&edge.waypoints);
    let old_label = edge.label.clone();
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
    let expected_label = old_label.map(|mut label| {
        label.bounds = label
            .bounds
            .map(|b| followed(&b, &old_route, &edge.waypoints));
        label
    });
    assert_eq!(
        serde_json::to_value(&edge.label).unwrap(),
        serde_json::to_value(&expected_label).unwrap()
    );
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
        flow_metadata: None,
        base: None,
        id: Some("data_object".parse().unwrap()),
        name: Some("Order".into()),
        is_collection: None,
        item_aware: None,
    });
    process.nodes.push(Node {
        flow_metadata: None,
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
            di_metadata: None,
            message_visible_kind: None,
            id: id.parse().unwrap(),
            element: element.parse().unwrap(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: points,
            label: Some(Label {
                di_metadata: None,
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
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: "collaboration".parse().unwrap(),
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            base: None,
            id: "buyer".parse().unwrap(),
            name: Some("Buyer".into()),
            process: None,
        }],
        flows: vec![],
        conversation_nodes: vec![ConversationNode {
            message_flow_refs: vec![],
            call: None,
            correlation_keys: vec![],
            base: None,
            id: "conversation".parse().unwrap(),
            name: Some("Order".into()),
            kind: ConversationKind::Conversation,
            participant_refs: vec![processcraft_core::bpmn_activity::ModelReference::Local(
                "buyer".parse().unwrap(),
            )],
            parent: None,
        }],
        conversation_links: vec![ConversationLink {
            base: None,
            id: "link".parse().unwrap(),
            name: Some(String::new()),
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "buyer".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "conversation".parse().unwrap(),
            )),
        }],
    });
    let shape = |id: &str, element: &str, x| Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        plane_metadata: None,
        plane_order: None,
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
            di_metadata: None,
            message_visible_kind: None,
            id: "link_edge".parse().unwrap(),
            element: "link".parse().unwrap(),
            source_element: None,
            label_visibility: None,
            target_element: None,
            waypoints: vec![p(10.0, 5.0), p(20.0, 5.0), p(20.0, 8.0), p(30.0, 8.0)],
            label: Some(Label {
                di_metadata: None,
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        if matches!(&flow.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if id.to_string() == "start")
        {
            flow.source = Some(processcraft_core::bpmn_activity::ModelReference::Local(
                node.clone(),
            ));
        }
        if matches!(&flow.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if id.to_string() == "start")
        {
            flow.target = Some(processcraft_core::bpmn_activity::ModelReference::Local(
                node.clone(),
            ));
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
        di_metadata: None,
        message_visible_kind: None,
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
        message_ref: None,
        flow_metadata: None,
        id: "association".parse().unwrap(),
        kind: FlowKind::Association,
        source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
            "note".parse().unwrap(),
        )),
        target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
            "review".parse().unwrap(),
        )),
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        di_metadata: None,
        message_visible_kind: None,
        id: "association_edge".parse().unwrap(),
        element: "association".parse().unwrap(),
        source_element: Some("note_shape".parse().unwrap()),
        label_visibility: None,
        target_element: Some("edge".parse().unwrap()),
        waypoints: vec![p(20.0, 40.0), p(20.0, 5.0)],
        label: Some(Label {
            di_metadata: None,
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
        let old_link = before.project.documents[0]
            .diagrams
            .iter()
            .find(|d| d.id == diagram_id)
            .unwrap()
            .edges
            .iter()
            .find(|e| e.id.to_string() == "association_edge")
            .unwrap();
        assert_eq!(
            link.label.as_ref().unwrap().bounds,
            Some(followed(
                &rect(22.0, 20.0, 10.0, 5.0),
                &old_link.waypoints,
                &link.waypoints
            ))
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
        message_ref: None,
        flow_metadata: None,
        id: "association".parse().unwrap(),
        kind: FlowKind::Association,
        source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
            "note".parse().unwrap(),
        )),
        target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
            "review".parse().unwrap(),
        )),
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
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
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
        di_metadata: None,
        message_visible_kind: None,
        id: "association_edge".parse().unwrap(),
        element: "association".parse().unwrap(),
        source_element: Some("note_shape".parse().unwrap()),
        label_visibility: None,
        target_element: Some("edge".parse().unwrap()),
        waypoints: vec![p(20.0, 20.0), p(20.0, 5.0)],
        label: Some(Label {
            di_metadata: None,
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

#[test]
fn participant_band_follows_only_its_own_di_representation() {
    use processcraft_core::{
        commands::{Mutation, Request, ShapeBounds},
        model::{DiReference, ParticipantBandKind},
    };
    let (session, _) = session_fixture();
    let snapshot = session.snapshot();
    let mut doc = (*snapshot.project.documents[0]).clone();
    let diagram = &mut doc.diagrams[0];
    diagram.shapes[0].bounds = rect(0.0, 0.0, 10.0, 10.0);
    let owner = diagram.shapes[0].id.selector();
    let mut band = diagram.shapes[0].clone();
    band.id = "band".parse().unwrap();
    band.element = "band_participant".parse().unwrap();
    band.bounds = rect(0.0, 0.0, 10.0, 2.0);
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(owner.clone()));
    let mut external = band.clone();
    external.id = "external_band".parse().unwrap();
    external.choreography_activity_shape = Some(DiReference::Qualified(
        processcraft_core::bpmn_model::QNameRef {
            namespace: "urn:external".into(),
            local: diagram.shapes[0].id.xml_id().unwrap().clone(),
        },
    ));
    diagram.shapes.push(band);
    diagram.shapes.push(external);
    let diagram_id = diagram.id.selector();
    let request = prepare_move(
        &doc,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: owner,
            bounds: rect(3.0, 4.0, 20.0, 20.0),
        }],
    )
    .unwrap()
    .request;
    let Request::Batch { mutations } = request else {
        panic!("expected batch")
    };
    assert!(mutations.iter().any(|m| matches!(m, Mutation::MoveShape { shape_id, bounds, .. } if shape_id == &"band".parse::<processcraft_core::ids::ElementSelector>().unwrap() && *bounds == rect(3.0, 4.0, 20.0, 4.0))));
    assert!(!mutations.iter().any(|m| matches!(m, Mutation::MoveShape { shape_id, .. } if shape_id == &"external_band".parse::<processcraft_core::ids::ElementSelector>().unwrap())));
}

#[test]
fn valid_choreography_band_moves_with_only_its_owner_and_undoes() {
    use processcraft_core::{
        bpmn_activity::ModelReference,
        bpmn_collaboration::{Choreography, ChoreographyActivity},
        commands::{Request, Session, ShapeBounds},
        model::{DiReference, Flow, FlowKind, Label, Participant, ParticipantBandKind},
    };
    let (source, _) = session_fixture();
    let mut project = (*source.snapshot().project).clone();
    let document = std::sync::Arc::make_mut(&mut project.documents[0]);
    let participant = |id: &str| Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        id: id.parse().unwrap(),
        name: Some(id.into()),
        process: None,
        base: None,
        multiplicity: None,
    };
    document.choreographies.push(Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        id: "choreo".parse().unwrap(),
        name: None,
        base: None,
        participants: vec![participant("buyer"), participant("seller")],
        message_flows: vec![Flow {
            id: "message".parse().unwrap(),
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "buyer".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "seller".parse().unwrap(),
            )),
            kind: FlowKind::Message,
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
            flow_metadata: None,
            association_direction: None,
            message_ref: None,
        }],
        activities: vec![ChoreographyActivity {
            loop_type: None,
            details: None,
            id: "exchange".parse().unwrap(),
            name: Some("Exchange".into()),
            participant_refs: vec![
                ModelReference::Local("buyer".parse().unwrap()),
                ModelReference::Local("seller".parse().unwrap()),
            ],
            initiating_participant: Some(ModelReference::Local("buyer".parse().unwrap())),
            message_flow_refs: vec![ModelReference::Local("message".parse().unwrap())],
            base: None,
            correlation_keys: vec![],
            flow_metadata: None,
        }],
        sequence_flows: vec![],
    });
    let mut diagram = document.diagrams.last().unwrap().clone();
    diagram.id = "choreo_diagram".parse().unwrap();
    diagram.plane_id = "choreo_plane".parse().unwrap();
    diagram.plane = "choreo".parse().unwrap();
    diagram.edges.clear();
    diagram.plane_order = None;
    let mut owner = diagram.shapes[0].clone();
    owner.id = "owner1".parse().unwrap();
    owner.element = "exchange".parse().unwrap();
    owner.bounds = rect(0.0, 0.0, 120.0, 100.0);
    owner.label = None;
    let mut owner2 = owner.clone();
    owner2.id = "owner2".parse().unwrap();
    owner2.bounds.x = 300.0;
    let mut band = owner.clone();
    band.id = "band1".parse().unwrap();
    band.element = "buyer".parse().unwrap();
    band.bounds = rect(0.0, 0.0, 120.0, 20.0);
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(owner.id.selector()));
    band.label = Some(Label {
        id: Some("band_label".parse().unwrap()),
        bounds: Some(rect(5.0, 2.0, 100.0, 16.0)),
        style: None,
        style_ref: None,
        di_metadata: None,
    });
    let mut band2 = band.clone();
    band2.id = "band2".parse().unwrap();
    band2.bounds.x = 300.0;
    band2.label = None;
    band2.choreography_activity_shape = Some(DiReference::Local(owner2.id.selector()));
    diagram.shapes = vec![owner, band, owner2, band2];
    document.diagrams.push(diagram);
    project.validate().unwrap();
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let document = &before.project.documents[0];
    let diagram_id = document.diagrams.last().unwrap().id.selector();
    let request = prepare_move(
        document,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "owner1".parse().unwrap(),
            bounds: rect(10.0, 15.0, 120.0, 100.0),
        }],
    )
    .unwrap()
    .request;
    apply_session(&mut session, request);
    let after = session.snapshot();
    after.project.validate().unwrap();
    let shapes = &after.project.documents[0].diagrams.last().unwrap().shapes;
    assert_eq!(shapes[1].bounds, rect(10.0, 15.0, 120.0, 20.0));
    assert_eq!(
        shapes[1].label.as_ref().unwrap().bounds,
        Some(rect(15.0, 17.0, 100.0, 16.0))
    );
    assert_eq!(shapes[3].bounds, rect(300.0, 0.0, 120.0, 20.0));
    apply_session(&mut session, Request::Undo);
    assert_eq!(session.snapshot().content_id, before.content_id);
    apply_session(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);

    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, ChoreographyGraph,
    };
    use processcraft_core::commands::Mutation;
    let mut document = (*before.project.documents[0]).clone();
    let mut first = document.choreographies[0].activities[0].clone();
    first.id = "nested1".parse().unwrap();
    first.message_flow_refs.clear();
    let mut second = first.clone();
    second.id = "nested2".parse().unwrap();
    let mut grandchild = first.clone();
    grandchild.id = "grandchild".parse().unwrap();
    second.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(grandchild))],
        }),
        artifacts: vec![],
    });
    let mut flow = document.choreographies[0].message_flows[0].clone();
    flow.id = "nested_flow".parse().unwrap();
    flow.kind = FlowKind::Sequence;
    flow.source = Some(ModelReference::Local("nested1".parse().unwrap()));
    flow.target = Some(ModelReference::Local("nested2".parse().unwrap()));
    document.choreographies[0].activities[0].details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![
                ChoreographyFlowElement::Activity(Box::new(first)),
                ChoreographyFlowElement::Activity(Box::new(second)),
                ChoreographyFlowElement::SequenceFlow(Box::new(flow)),
            ],
        }),
        artifacts: vec![],
    });
    document.choreographies[0].activities[0]
        .message_flow_refs
        .clear();
    let parent = document.choreographies[0].activities.remove(0);
    document.choreographies[0].graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(parent))],
    });
    let edge_template = document.diagrams[0].edges[0].clone();
    let diagram = document.diagrams.last_mut().unwrap();
    let mut child1 = diagram.shapes[0].clone();
    child1.id = "child1".parse().unwrap();
    child1.element = "nested1".parse().unwrap();
    child1.bounds = rect(15.0, 30.0, 20.0, 20.0);
    let mut child2 = child1.clone();
    child2.id = "child2".parse().unwrap();
    child2.element = "nested2".parse().unwrap();
    child2.bounds.x = 75.0;
    let mut child3 = child1.clone();
    child3.id = "child3".parse().unwrap();
    child3.bounds.x += 300.0;
    let mut child4 = child2.clone();
    child4.id = "child4".parse().unwrap();
    child4.bounds.x += 300.0;
    let mut grandchild1 = child1.clone();
    grandchild1.id = "grandchild1".parse().unwrap();
    grandchild1.element = "grandchild".parse().unwrap();
    grandchild1.bounds = rect(80.0, 35.0, 8.0, 8.0);
    let mut grandchild2 = grandchild1.clone();
    grandchild2.id = "grandchild2".parse().unwrap();
    grandchild2.bounds.x += 300.0;
    diagram
        .shapes
        .extend([child1, child2, child3, child4, grandchild1, grandchild2]);
    let mut edge = edge_template;
    edge.id = "nested_edge1".parse().unwrap();
    edge.element = "nested_flow".parse().unwrap();
    edge.source_element = None;
    edge.target_element = None;
    edge.waypoints = vec![p(35.0, 40.0), p(55.0, 45.0), p(75.0, 40.0)];
    edge.label = Some(Label {
        id: None,
        bounds: Some(rect(45.0, 50.0, 20.0, 10.0)),
        style: None,
        style_ref: None,
        di_metadata: None,
    });
    let mut edge2 = edge.clone();
    edge2.id = "nested_edge2".parse().unwrap();
    for point in &mut edge2.waypoints {
        point.x += 300.0;
    }
    diagram.edges.extend([edge, edge2]);
    let request = prepare_move(
        &document,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "owner1".parse().unwrap(),
            bounds: rect(10.0, 15.0, 120.0, 100.0),
        }],
    )
    .unwrap()
    .request;
    let mut graph_project = (*before.project).clone();
    graph_project.documents[0] = std::sync::Arc::new(document.clone());
    let mut graph_session = Session::new(graph_project).unwrap();
    let graph_before = graph_session.snapshot().content_id;
    apply_session(&mut graph_session, request.clone());
    let graph_after = graph_session.snapshot().content_id;
    apply_session(&mut graph_session, Request::Undo);
    assert_eq!(graph_session.snapshot().content_id, graph_before);
    apply_session(&mut graph_session, Request::Redo);
    assert_eq!(graph_session.snapshot().content_id, graph_after);
    let Request::Batch { mutations } = request else {
        panic!("expected batch")
    };
    assert!(mutations.iter().any(|m| matches!(m, Mutation::MoveShape { shape_id, bounds, .. } if shape_id == &"child1".parse::<processcraft_core::ids::ElementSelector>().unwrap() && *bounds == rect(25.0, 45.0, 20.0, 20.0))));
    assert!(mutations.iter().any(|m| matches!(m, Mutation::MoveShape { shape_id, bounds, .. } if shape_id == &"grandchild1".parse::<processcraft_core::ids::ElementSelector>().unwrap() && *bounds == rect(90.0, 50.0, 8.0, 8.0))));
    assert!(!mutations.iter().any(|m| matches!(m, Mutation::MoveShape { shape_id, .. } if shape_id == &"child3".parse::<processcraft_core::ids::ElementSelector>().unwrap())));
    assert!(mutations.iter().any(|m| matches!(m, Mutation::Reroute { edge_id, waypoints, .. } if edge_id == &"nested_edge1".parse::<processcraft_core::ids::ElementSelector>().unwrap() && waypoints == &vec![p(45.0, 55.0), p(65.0, 60.0), p(85.0, 55.0)])));
    assert!(!mutations.iter().any(|m| matches!(m, Mutation::Reroute { edge_id, .. } if edge_id == &"nested_edge2".parse::<processcraft_core::ids::ElementSelector>().unwrap())));
    assert!(mutations.iter().any(|m| matches!(m, Mutation::SetLabelBounds { representation_id, bounds, .. } if representation_id == &"nested_edge1".parse::<processcraft_core::ids::ElementSelector>().unwrap() && *bounds == rect(55.0, 65.0, 20.0, 10.0))));
    let mut orphaned = document.clone();
    orphaned.diagrams.last_mut().unwrap().shapes[6].bounds.x = 600.0;
    assert!(
        prepare_move(
            &orphaned,
            &diagram_id,
            vec![ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: "owner1".parse().unwrap(),
                bounds: rect(10.0, 15.0, 120.0, 100.0)
            }]
        )
        .is_err()
    );
    let mut detached = document.clone();
    detached.diagrams.last_mut().unwrap().edges[0].waypoints[0].x = 50.0;
    assert!(
        prepare_move(
            &detached,
            &diagram_id,
            vec![ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: "owner1".parse().unwrap(),
                bounds: rect(10.0, 15.0, 120.0, 100.0)
            }]
        )
        .is_err()
    );
    document.diagrams.last_mut().unwrap().shapes[2].bounds.x = 0.0;
    assert!(
        prepare_move(
            &document,
            &diagram_id,
            vec![ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: "owner1".parse().unwrap(),
                bounds: rect(10.0, 15.0, 120.0, 100.0)
            }]
        )
        .is_err()
    );
}

#[test]
fn unresolved_qualified_endpoint_is_never_retargeted_by_local_name() {
    use processcraft_core::{
        bpmn_activity::ModelReference,
        bpmn_model::QNameRef,
        commands::{Mutation, Request, ShapeBounds},
    };
    let session = conversation_geometry_session();
    let snapshot = session.snapshot();
    let mut document = (*snapshot.project.documents[0]).clone();
    let link = &mut document.collaboration.as_mut().unwrap().conversation_links[0];
    link.source = Some(ModelReference::Qualified(QNameRef {
        namespace: "urn:external".into(),
        local: "buyer".parse().unwrap(),
    }));
    let diagram_id = "conversation_diagram".parse().unwrap();
    let request = prepare_move(
        &document,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "buyer_shape".parse().unwrap(),
            bounds: rect(2.0, 3.0, 10.0, 10.0),
        }],
    )
    .unwrap()
    .request;
    let Request::Batch { mutations } = request else {
        panic!("batch")
    };
    assert!(
        !mutations
            .iter()
            .any(|m| matches!(m, Mutation::Reroute { .. }))
    );
    let result = prepare_move(
        &document,
        &diagram_id,
        vec![ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id: "conversation_shape".parse().unwrap(),
            bounds: rect(32.0, 3.0, 10.0, 10.0),
        }],
    );
    assert!(
        result.is_err(),
        "moving the known endpoint of an unresolved connection must be refused"
    );
}

#[test]
fn moving_event_or_gateway_keeps_route_outside_endpoint_owners_and_undoes() {
    use processcraft_core::commands::{Request, Session, ShapeBounds};
    let cases = [
        (
            rect(100.0, 100.0, 36.0, 36.0),
            rect(280.0, 100.0, 50.0, 50.0),
            vec![
                p(136.0, 118.0),
                p(148.0, 118.0),
                p(148.0, 125.0),
                p(280.0, 125.0),
            ],
            "shape_start",
            rect(140.0, 172.0, 36.0, 36.0),
        ),
        (
            rect(100.0, 100.0, 36.0, 36.0),
            rect(280.0, 100.0, 50.0, 50.0),
            vec![
                p(136.0, 118.0),
                p(148.0, 118.0),
                p(148.0, 125.0),
                p(280.0, 125.0),
            ],
            "shape_review",
            rect(240.0, 172.0, 50.0, 50.0),
        ),
        (
            rect(100.0, 100.0, 36.0, 36.0),
            rect(100.0, 280.0, 50.0, 50.0),
            vec![
                p(118.0, 136.0),
                p(118.0, 148.0),
                p(125.0, 148.0),
                p(125.0, 280.0),
            ],
            "shape_start",
            rect(172.0, 140.0, 36.0, 36.0),
        ),
        (
            rect(100.1, 100.1, 36.0, 36.0),
            rect(280.1, 100.1, 50.0, 50.0),
            vec![
                p(136.1, 118.1),
                p(148.1, 118.1),
                p(148.1, 125.1),
                p(280.1, 125.1),
            ],
            "shape_start",
            rect(140.3, 172.4, 36.0, 36.0),
        ),
    ];
    for (source_bounds, target_bounds, waypoints, moved_id, moved_bounds) in cases {
        let (initial, _) = session_fixture();
        let mut project = (*initial.snapshot().project).clone();
        let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
        doc.processes[0]
            .nodes
            .iter_mut()
            .find(|n| n.id.to_string() == "review")
            .unwrap()
            .kind = NodeKind::ExclusiveGateway;
        let diagram = doc
            .diagrams
            .iter_mut()
            .find(|d| d.id.to_string() == "diagram")
            .unwrap();
        diagram.shapes[0].bounds = source_bounds;
        diagram.shapes[1].bounds = target_bounds;
        diagram.edges[0].waypoints = waypoints.clone();
        let mut session = Session::new(project).unwrap();
        let before = session.snapshot();
        let diagram_id = "diagram".parse().unwrap();
        let mut ambiguous = (*before.project.documents[0]).clone();
        let diagram = ambiguous
            .diagrams
            .iter_mut()
            .find(|d| d.id == diagram_id)
            .unwrap();
        let mut duplicate = diagram.shapes[0].clone();
        duplicate.id = "duplicate_event_representation".parse().unwrap();
        duplicate.bounds.x += 500.0;
        diagram.shapes.push(duplicate);
        assert!(
            prepare_move(
                &ambiguous,
                &diagram_id,
                vec![ShapeBounds {
                    diagram_id: diagram_id.clone(),
                    shape_id: moved_id.parse().unwrap(),
                    bounds: moved_bounds.clone()
                }]
            )
            .is_err()
        );
        let mut unproven = (*before.project.documents[0]).clone();
        let diagram = unproven
            .diagrams
            .iter_mut()
            .find(|d| d.id == diagram_id)
            .unwrap();
        // Off the border band (an endpoint 1 unit off the border is now re-docked instead).
        diagram.edges[0].waypoints[0].x -= 40.0;
        diagram.edges[0].waypoints[0].y -= 40.0;
        assert!(
            prepare_move(
                &unproven,
                &diagram_id,
                vec![ShapeBounds {
                    diagram_id: diagram_id.clone(),
                    shape_id: moved_id.parse().unwrap(),
                    bounds: moved_bounds.clone()
                }]
            )
            .is_err()
        );
        let request = prepare_move(
            &before.project.documents[0],
            &diagram_id,
            vec![ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: moved_id.parse().unwrap(),
                bounds: moved_bounds,
            }],
        )
        .unwrap()
        .request;
        apply_session(&mut session, request);
        let after = session.snapshot();
        let diagram = after.project.documents[0]
            .diagrams
            .iter()
            .find(|d| d.id == diagram_id)
            .unwrap();
        let repaired = &diagram.edges[0].waypoints;
        // The bend next to the unmoved end stays on the route (as a DI point unless left
        // collinear); the moved end re-docks on the side facing its neighbour, which may drop
        // a bend the new route no longer doubles back to.
        let kept = if moved_id == "shape_start" {
            &waypoints[waypoints.len() - 2]
        } else {
            &waypoints[1]
        };
        assert_eq!(distance_to_path(kept, repaired), 0.0);
        for pair in repaired.windows(2) {
            assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
            for shape in &diagram.shapes {
                let b = &shape.bounds;
                let crossing = if pair[0].x == pair[1].x {
                    pair[0].x > b.x
                        && pair[0].x < b.x + b.width
                        && pair[0].y.max(pair[1].y) > b.y
                        && pair[0].y.min(pair[1].y) < b.y + b.height
                } else {
                    pair[0].y > b.y
                        && pair[0].y < b.y + b.height
                        && pair[0].x.max(pair[1].x) > b.x
                        && pair[0].x.min(pair[1].x) < b.x + b.width
                };
                assert!(!crossing, "route {repaired:?} crosses {:?}", b);
            }
        }
        let after_id = after.content_id;
        apply_session(&mut session, Request::Undo);
        assert_eq!(session.snapshot().content_id, before.content_id);
        apply_session(&mut session, Request::Redo);
        assert_eq!(session.snapshot().content_id, after_id);
    }
}
