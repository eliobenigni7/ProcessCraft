//! Pools, lanes, and containers through the shared command protocol: lane add/split/resize/
//! delete, auto-resize, wrapping content in a new pool, collapse/expand, and the space tool.
use processcraft_core::{
    commands::{CommandError, Envelope, Mutation, Request, Session, ShapeBounds},
    containers::{Axis, LaneLocation},
    ids::{DocumentId, ElementSelector},
    model::*,
};
use serde_json::json;

fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn diagram() -> ElementSelector {
    sel("procurement_diagram")
}
fn send(s: &mut Session, request: Request) -> Result<(), CommandError> {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request).map(|_| ())
}
fn batch(s: &mut Session, mutations: Vec<Mutation>) -> Result<(), CommandError> {
    send(s, Request::Batch { mutations })
}
/// The procurement fixture: a process diagram with two lanes (request 40..220, procurement
/// 220..400, x 40..890), start/review/decision in the first and order/end in the second.
fn session() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let setup: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    send(&mut s, setup).unwrap();
    s
}
fn document(s: &Session) -> BpmnDocument {
    (*s.snapshot().project.documents[0]).clone()
}
fn bounds(s: &Session, element: &str) -> Bounds {
    document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(element))
        .unwrap()
        .bounds
        .clone()
}
fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}
fn lanes(s: &Session) -> Vec<Lane> {
    document(s).processes[0].lanes.clone()
}
fn lane_of(s: &Session, node: &str) -> Option<String> {
    lanes(s)
        .into_iter()
        .find(|l| l.members.contains(&sel(node)))
        .map(|l| l.id.to_string())
}
/// Lanes sorted top to bottom with their bounds.
fn lane_bounds(s: &Session) -> Vec<(String, Bounds)> {
    let mut out: Vec<_> = lanes(s)
        .into_iter()
        .map(|l| (l.id.to_string(), bounds(s, &l.id.to_string())))
        .collect();
    out.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    out
}
fn pool(s: &Session) -> (String, Bounds) {
    let doc = document(s);
    let p = &doc.collaboration.as_ref().unwrap().participants[0];
    (p.id.to_string(), bounds(s, &p.id.to_string()))
}
fn create_pool(s: &mut Session) -> (String, Bounds) {
    batch(
        s,
        vec![Mutation::CreatePool {
            document_id: docid(),
            diagram_id: diagram(),
            name: Some("Buyer".into()),
            center: None,
        }],
    )
    .unwrap();
    pool(s)
}
fn add_lane(element: &str, location: LaneLocation) -> Mutation {
    Mutation::AddLane {
        document_id: docid(),
        diagram_id: diagram(),
        element_id: sel(element),
        location,
    }
}
fn split(element: &str, count: u8) -> Mutation {
    Mutation::SplitLane {
        document_id: docid(),
        diagram_id: diagram(),
        element_id: sel(element),
        count,
    }
}
fn resize(element: &str, s: &Session, bounds: Bounds) -> Mutation {
    let shape = document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(element))
        .unwrap()
        .id
        .selector();
    Mutation::ResizeShape {
        document_id: docid(),
        diagram_id: diagram(),
        shape_id: shape,
        bounds,
    }
}
fn move_to(element: &str, s: &Session, bounds: Bounds) -> Mutation {
    let shape = document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(element))
        .unwrap()
        .id
        .selector();
    Mutation::MoveElements {
        document_id: docid(),
        diagram_id: diagram(),
        shapes: vec![ShapeBounds {
            diagram_id: diagram(),
            shape_id: shape,
            bounds,
        }],
        flow_id: None,
        edge_id: None,
    }
}
/// Apply one mutation built from the current state.
fn run(s: &mut Session, build: impl FnOnce(&Session) -> Mutation) -> Result<(), CommandError> {
    let m = build(s);
    batch(s, vec![m])
}
fn undo(s: &mut Session) {
    send(s, Request::Undo).unwrap();
}

#[test]
fn creating_a_pool_wraps_the_process_content() {
    let mut s = session();
    let (id, pool) = create_pool(&mut s);
    // Content 40..890 x 40..400, plus 20 padding and the 30 header.
    assert_eq!(pool, b(-10.0, 20.0, 920.0, 400.0));
    let doc = document(&s);
    let collaboration = doc.collaboration.as_ref().unwrap();
    assert_eq!(doc.diagrams[0].plane, collaboration.id);
    let participant = &collaboration.participants[0];
    assert_eq!(participant.name.as_deref(), Some("Buyer"));
    assert!(matches!(&participant.process,
        Some(processcraft_core::bpmn_activity::ModelReference::Local(p)) if *p == sel("procurement")));
    // A second pool in the collaboration gets its own empty process below the first.
    batch(
        &mut s,
        vec![Mutation::CreatePool {
            document_id: docid(),
            diagram_id: diagram(),
            name: None,
            center: None,
        }],
    )
    .unwrap();
    let doc = document(&s);
    assert_eq!(doc.processes.len(), 2);
    let second = &doc.collaboration.as_ref().unwrap().participants[1];
    let second_bounds = bounds(&s, &second.id.to_string());
    assert_eq!((second_bounds.width, second_bounds.height), (600.0, 250.0));
    assert!(second_bounds.y >= pool.y + pool.height);
    undo(&mut s);
    undo(&mut s);
    assert!(document(&s).collaboration.is_none());
    assert!(!lanes(&s).is_empty() && id.starts_with("Participant_"));
}

#[test]
fn adding_lanes_makes_room_and_keeps_membership() {
    let mut s = session();
    let order = bounds(&s, "order");
    batch(&mut s, vec![add_lane("lane_request", LaneLocation::Below)]).unwrap();
    let all = lane_bounds(&s);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0], ("lane_request".into(), b(40.0, 40.0, 850.0, 180.0)));
    assert_eq!(all[1].1, b(40.0, 220.0, 850.0, 120.0));
    assert_eq!(
        all[2],
        ("lane_procurement".into(), b(40.0, 340.0, 850.0, 180.0))
    );
    // Shapes below the new lane moved with their lane; edges into them were re-docked.
    assert_eq!(bounds(&s, "order").y, order.y + 120.0);
    assert_eq!(lane_of(&s, "order").as_deref(), Some("lane_procurement"));
    let doc = document(&s);
    let edge = doc.diagrams[0]
        .edges
        .iter()
        .find(|e| e.element == sel("flow_approved"))
        .unwrap();
    assert_eq!(edge.waypoints.last().unwrap().y, 430.0);
    undo(&mut s);
    assert_eq!(lanes(&s).len(), 2);

    // Above a pool: the pool alone grows upward and the lane sits at its indentation.
    let (pool_id, pool) = create_pool(&mut s);
    batch(&mut s, vec![add_lane(&pool_id, LaneLocation::Above)]).unwrap();
    assert_eq!(
        self::pool(&s).1,
        b(pool.x, pool.y - 120.0, pool.width, pool.height + 120.0)
    );
    let all = lane_bounds(&s);
    assert_eq!(
        all[0].1,
        b(pool.x + 30.0, pool.y - 120.0, pool.width - 30.0, 120.0)
    );
    assert_eq!(all[1].1, b(40.0, 40.0, 850.0, 180.0));
}

#[test]
fn adding_a_lane_to_a_pool_without_lanes_creates_the_first_lane_too() {
    let mut s = session();
    create_pool(&mut s);
    batch(
        &mut s,
        vec![Mutation::CreatePool {
            document_id: docid(),
            diagram_id: diagram(),
            name: None,
            center: Some(Point { x: 400.0, y: 700.0 }),
        }],
    )
    .unwrap();
    let doc = document(&s);
    let second = doc.collaboration.as_ref().unwrap().participants[1]
        .id
        .to_string();
    assert_eq!(bounds(&s, &second), b(100.0, 575.0, 600.0, 250.0));
    batch(&mut s, vec![add_lane(&second, LaneLocation::Below)]).unwrap();
    assert_eq!(bounds(&s, &second), b(100.0, 575.0, 600.0, 370.0));
    let doc = document(&s);
    let new_lanes: Vec<Bounds> = doc.processes[1]
        .lanes
        .iter()
        .map(|l| bounds(&s, &l.id.to_string()))
        .collect();
    assert_eq!(
        new_lanes,
        vec![b(130.0, 575.0, 570.0, 250.0), b(130.0, 825.0, 570.0, 120.0)]
    );
}

#[test]
fn splitting_reuses_lanes_and_moves_members_into_the_new_lanes() {
    let mut s = session();
    batch(&mut s, vec![split("lane_request", 3)]).unwrap();
    let doc = document(&s);
    let children: Vec<&Lane> = doc.processes[0]
        .lanes
        .iter()
        .filter(|l| l.parent == Some(sel("lane_request")))
        .collect();
    assert_eq!(children.len(), 3);
    let mut rows: Vec<Bounds> = children
        .iter()
        .map(|l| bounds(&s, &l.id.to_string()))
        .collect();
    rows.sort_by(|a, b| a.y.total_cmp(&b.y));
    assert_eq!(
        rows,
        vec![
            b(70.0, 40.0, 820.0, 60.0),
            b(70.0, 100.0, 820.0, 60.0),
            b(70.0, 160.0, 820.0, 60.0)
        ]
    );
    // start, review and decision (centre y 128) now belong to the middle lane.
    let middle = lane_of(&s, "review").unwrap();
    assert_eq!(bounds(&s, &middle), rows[1]);
    assert_eq!(lane_of(&s, "start"), Some(middle.clone()));
    assert_eq!(lane_of(&s, "order").as_deref(), Some("lane_procurement"));
    // More lanes than requested is refused.
    let refused = batch(&mut s, vec![split("lane_request", 2)]);
    assert!(matches!(refused, Err(CommandError::InvalidParams(r)) if r.contains("more than 2")));
    undo(&mut s);

    // A pool split in two reuses its two lanes, filling its body.
    let (pool_id, pool) = create_pool(&mut s);
    batch(&mut s, vec![split(&pool_id, 2)]).unwrap();
    assert_eq!(
        lane_bounds(&s),
        vec![
            (
                "lane_request".into(),
                b(pool.x + 30.0, pool.y, pool.width - 30.0, 200.0)
            ),
            (
                "lane_procurement".into(),
                b(pool.x + 30.0, pool.y + 200.0, pool.width - 30.0, 200.0)
            ),
        ]
    );
}

#[test]
fn resizing_a_lane_moves_the_shared_edge_and_a_pool_resizes_its_lanes() {
    let mut s = session();
    run(&mut s, |s| {
        resize("lane_request", s, b(40.0, 40.0, 850.0, 290.0))
    })
    .unwrap();
    assert_eq!(bounds(&s, "lane_procurement"), b(40.0, 330.0, 850.0, 70.0));
    // order's and end's centres (y 310) are now in the request lane; start stays put.
    assert_eq!(lane_of(&s, "order").as_deref(), Some("lane_request"));
    assert_eq!(lane_of(&s, "end").as_deref(), Some("lane_request"));
    assert_eq!(lane_of(&s, "start").as_deref(), Some("lane_request"));
    // Squeezing the neighbour below its 60 minimum is refused.
    let refused = run(&mut s, |s| {
        resize("lane_request", s, b(40.0, 40.0, 850.0, 320.0))
    });
    assert!(matches!(refused, Err(CommandError::InvalidParams(r)) if r.contains("smaller")));
    undo(&mut s);

    let (pool_id, pool) = create_pool(&mut s);
    batch(&mut s, vec![split(&pool_id, 2)]).unwrap();
    let wider = Bounds {
        width: pool.width + 100.0,
        height: pool.height + 50.0,
        ..pool.clone()
    };
    run(&mut s, |s| resize(&pool_id, s, wider)).unwrap();
    let all = lane_bounds(&s);
    assert_eq!(all[0].1, b(20.0, 20.0, 990.0, 200.0));
    assert_eq!(all[1].1, b(20.0, 220.0, 990.0, 250.0));
}

#[test]
fn deleting_a_lane_lets_its_siblings_take_its_space() {
    let mut s = session();
    batch(
        &mut s,
        vec![Mutation::Delete {
            document_id: docid(),
            element_id: sel("lane_request"),
        }],
    )
    .unwrap();
    assert_eq!(bounds(&s, "lane_procurement"), b(40.0, 40.0, 850.0, 360.0));
    assert_eq!(lane_of(&s, "review").as_deref(), Some("lane_procurement"));
    undo(&mut s);

    // A lane between two others: each neighbour takes half.
    batch(&mut s, vec![add_lane("lane_request", LaneLocation::Below)]).unwrap();
    let middle = lane_bounds(&s)[1].0.clone();
    batch(
        &mut s,
        vec![Mutation::Delete {
            document_id: docid(),
            element_id: sel(&middle),
        }],
    )
    .unwrap();
    assert_eq!(bounds(&s, "lane_request"), b(40.0, 40.0, 850.0, 240.0));
    assert_eq!(bounds(&s, "lane_procurement"), b(40.0, 280.0, 850.0, 240.0));
}

#[test]
fn moving_past_a_pool_border_grows_the_pool_and_its_lanes_without_cutting_children() {
    let mut s = session();
    let (pool_id, pool) = create_pool(&mut s);
    batch(&mut s, vec![split(&pool_id, 2)]).unwrap();
    let end = bounds(&s, "end");
    run(&mut s, |s| {
        move_to("end", s, b(880.0, end.y, end.width, end.height))
    })
    .unwrap();
    // end's right edge (916) came within 15 of the pool's (910): the pool grows to 916 + 100.
    assert_eq!(self::pool(&s).1, b(pool.x, pool.y, 1026.0, pool.height));
    for (_, lane) in lane_bounds(&s) {
        assert_eq!(lane.x + lane.width, 1016.0);
    }
    // Below the bottom: the pool and the bottom lane grow by 60 past the shape, in one step.
    run(&mut s, |s| move_to("end", s, b(880.0, 400.0, 36.0, 36.0))).unwrap();
    assert_eq!(self::pool(&s).1.height, 436.0 + 60.0 - pool.y);
    assert_eq!(lane_bounds(&s)[1].1.height, 496.0 - 220.0);
    assert_eq!(lane_of(&s, "end").as_deref(), Some("lane_procurement"));
    undo(&mut s);
    assert_eq!(self::pool(&s).1.height, pool.height);
    // Created elements near the border grow it too.
    batch(
        &mut s,
        vec![Mutation::CreateAt {
            document_id: docid(),
            diagram_id: diagram(),
            kind: NodeKind::Task,
            name: None,
            center: Point { x: 400.0, y: 400.0 },
            node_id: "edge_task".parse().unwrap(),
            shape_id: "edge_task_di".parse().unwrap(),
            flow_id: None,
            edge_id: None,
        }],
    )
    .unwrap();
    assert_eq!(self::pool(&s).1.height, 440.0 + 60.0 - pool.y);
    // A resize never cuts children off.
    let cut = run(&mut s, |s| {
        resize(&pool_id, s, b(pool.x, pool.y, 500.0, 500.0))
    });
    assert!(matches!(cut, Err(CommandError::InvalidParams(r)) if r.contains("cut off")));
}

#[test]
fn resizing_a_task_past_its_lane_edge_grows_the_lane_and_pool() {
    let mut s = session();
    let (pool_id, pool) = create_pool(&mut s);
    batch(&mut s, vec![split(&pool_id, 2)]).unwrap();
    let order = bounds(&s, "order");
    let bottom = pool.y + pool.height + 10.0;
    run(&mut s, |s| {
        resize(
            "order",
            s,
            b(order.x, order.y, order.width, bottom - order.y),
        )
    })
    .unwrap();
    assert_eq!(bounds(&s, "order").y + bounds(&s, "order").height, bottom);
    let grown = self::pool(&s).1;
    assert_eq!(grown.y + grown.height, bottom + 60.0);
    let last = lane_bounds(&s).last().unwrap().1.clone();
    assert_eq!(last.y + last.height, bottom + 60.0);
    undo(&mut s);
    assert_eq!(self::pool(&s).1, pool);
}

#[test]
fn collapsing_moves_content_to_the_subprocess_diagram_and_expanding_brings_it_back() {
    let mut s = session();
    let element = |id: &str, kind: &str, details: serde_json::Value| {
        let mut value = json!({"id": id, "name": null, "documentation": "", "kind": kind, "owner": "", "contact": ""});
        if !details.is_null() {
            value["details"] = details;
        }
        value
    };
    let inner = element("inner", "Task", json!(null));
    let sub = element(
        "sub",
        "SubProcess",
        json!({"type": "subProcess", "content": {"nodes": [inner], "lanes": [], "flows": []}}),
    );
    let shape = |id: &str, bounds: [f64; 4], expanded: bool| {
        json!({"type": "createShape", "document_id": docid(), "diagram_id": "procurement_diagram",
               "shape": {"id": format!("{id}_di"), "element": id, "is_expanded": expanded,
                         "bounds": {"x": bounds[0], "y": bounds[1], "width": bounds[2], "height": bounds[3]},
                         "label": null, "style": null}})
    };
    let setup: Vec<Mutation> = serde_json::from_value(json!([
        {"type": "createElement", "document_id": docid(), "process_id": "procurement", "element": sub},
        shape("sub", [100.0, 450.0, 350.0, 200.0], true),
        shape("inner", [150.0, 500.0, 100.0, 80.0], false),
    ]))
    .unwrap();
    batch(&mut s, setup).unwrap();
    let options = processcraft_core::replace::replace_options(&document(&s), &sel("sub"));
    let collapse = options
        .iter()
        .find(|t| t.label() == "Collapsed subprocess")
        .unwrap()
        .clone();
    batch(
        &mut s,
        vec![Mutation::ReplaceElement {
            document_id: docid(),
            element_id: sel("sub"),
            target: collapse,
        }],
    )
    .unwrap();
    let doc = document(&s);
    let main = &doc.diagrams[0];
    let sub_shape = main
        .shapes
        .iter()
        .find(|x| x.element == sel("sub"))
        .unwrap();
    assert_eq!(sub_shape.bounds, b(225.0, 510.0, 100.0, 80.0));
    assert_eq!(sub_shape.is_expanded, Some(false));
    assert!(!main.shapes.iter().any(|x| x.element == sel("inner")));
    let plane = doc.diagrams.iter().find(|d| d.plane == sel("sub")).unwrap();
    assert_eq!(plane.shapes[0].bounds, b(180.0, 160.0, 100.0, 80.0));
    // Expanding brings the content back centred on the subprocess, at least 350 x 200.
    batch(
        &mut s,
        vec![Mutation::ToggleCollapse {
            document_id: docid(),
            diagram_id: diagram(),
            element_id: sel("sub"),
        }],
    )
    .unwrap();
    let doc = document(&s);
    assert_eq!(doc.diagrams.len(), 1);
    assert_eq!(bounds(&s, "inner"), b(225.0, 510.0, 100.0, 80.0));
    assert_eq!(bounds(&s, "sub"), b(100.0, 450.0, 350.0, 200.0));
    undo(&mut s);
    assert_eq!(document(&s).diagrams.len(), 2);
    undo(&mut s);
    assert_eq!(bounds(&s, "inner"), b(150.0, 500.0, 100.0, 80.0));
}

#[test]
fn space_tool_moves_shapes_past_the_line_and_stretches_containers() {
    let mut s = session();
    let (review, order, end) = (bounds(&s, "review"), bounds(&s, "order"), bounds(&s, "end"));
    let space = |start: f64, delta: f64| Mutation::MakeSpace {
        document_id: docid(),
        diagram_id: diagram(),
        axis: Axis::X,
        start,
        delta,
        before: false,
        at: None,
    };
    batch(&mut s, vec![space(500.0, 100.0)]).unwrap();
    assert_eq!(bounds(&s, "review"), review);
    assert_eq!(bounds(&s, "order").x, order.x + 100.0);
    assert_eq!(bounds(&s, "end").x, end.x + 100.0);
    assert_eq!(bounds(&s, "lane_request"), b(40.0, 40.0, 950.0, 180.0));
    let doc = document(&s);
    let edge = |flow: &str| {
        doc.diagrams[0]
            .edges
            .iter()
            .find(|e| e.element == sel(flow))
            .unwrap()
            .waypoints
            .iter()
            .map(|p| (p.x, p.y))
            .collect::<Vec<_>>()
    };
    // Both ends moved: the points past the line shift.
    assert_eq!(edge("flow_order_end"), vec![(755.0, 310.0), (880.0, 310.0)]);
    // Only the target moved: its end is re-docked.
    assert_eq!(edge("flow_approved").last(), Some(&(615.0, 310.0)));
    // Removing more space than a lane can lose is refused; one undo restores everything.
    let refused = batch(&mut s, vec![space(100.0, -700.0)]);
    assert!(matches!(refused, Err(CommandError::InvalidParams(r)) if r.contains("smaller")));
    undo(&mut s);
    assert_eq!(bounds(&s, "order"), order);
    assert_eq!(bounds(&s, "lane_request"), b(40.0, 40.0, 850.0, 180.0));
}
