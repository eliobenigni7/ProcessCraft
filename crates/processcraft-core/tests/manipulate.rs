//! Direct manipulation through the shared command protocol: boundary events following their
//! host, resize rules, snapping, attach/detach, drop on flow, reconnect, lanes, and scopes.
use processcraft_core::{
    bpmn_activity::ModelReference,
    commands::{CommandError, Envelope, Mutation, Request, Session, ShapeBounds},
    geometry,
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
fn mutations(value: serde_json::Value) -> Vec<Mutation> {
    serde_json::from_value(value).unwrap()
}
/// The procurement fixture: start -> review -> decision -> order -> end, two lanes.
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
fn node(s: &Session, id: &str) -> Node {
    processcraft_core::queries::find_node(&document(s), &sel(id))
        .unwrap()
        .clone()
}
fn shape(s: &Session, id: &str) -> Shape {
    document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(id))
        .unwrap()
        .clone()
}
fn edge(s: &Session, flow: &str) -> Edge {
    document(s).diagrams[0]
        .edges
        .iter()
        .find(|x| x.element == sel(flow))
        .unwrap()
        .clone()
}
fn flow(s: &Session, id: &str) -> Option<Flow> {
    processcraft_core::queries::find_flow(&document(s), &sel(id)).cloned()
}
fn ends(f: &Flow) -> (String, String) {
    let id = |r: &Option<ModelReference>| match r {
        Some(ModelReference::Local(id)) => id.to_string(),
        _ => String::new(),
    };
    (id(&f.source), id(&f.target))
}
fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}
fn create(element: serde_json::Value, bounds: [f64; 4], scope: &str) -> Vec<Mutation> {
    let id = element["id"].as_str().unwrap().to_owned();
    mutations(json!([
        {"type": "createElement", "document_id": docid(), "process_id": scope, "element": element},
        {"type": "createShape", "document_id": docid(), "diagram_id": "procurement_diagram",
         "shape": {"id": format!("{id}_di"), "element": id, "is_expanded": true,
                   "bounds": {"x": bounds[0], "y": bounds[1], "width": bounds[2], "height": bounds[3]},
                   "label": {"bounds": {"x": bounds[0] - 27.0, "y": bounds[1] + bounds[3] + 10.0, "width": 90.0, "height": 20.0}},
                   "style": null}}
    ]))
}
fn element(id: &str, kind: &str, details: serde_json::Value) -> serde_json::Value {
    let mut value = json!({"id": id, "name": null, "documentation": "", "kind": kind, "owner": "", "contact": ""});
    if !details.is_null() {
        value["details"] = details;
    }
    value
}
fn timer() -> serde_json::Value {
    json!([{"type": "timer", "id": null, "kind": null, "expression": null}])
}
fn move_elements(id: &str, bounds: Bounds) -> Mutation {
    Mutation::MoveElements {
        document_id: docid(),
        diagram_id: sel("procurement_diagram"),
        shapes: vec![ShapeBounds {
            diagram_id: sel("procurement_diagram"),
            shape_id: sel(&format!("{id}_di")).to_owned(),
            bounds,
        }],
        flow_id: Some("split_flow".parse().unwrap()),
        edge_id: Some("split_edge".parse().unwrap()),
    }
}
/// Move the fixture's own shapes (named `pc_shape_<id>`).
fn move_fixture(id: &str, bounds: Bounds) -> Mutation {
    let Mutation::MoveElements {
        document_id,
        diagram_id,
        mut shapes,
        flow_id,
        edge_id,
    } = move_elements(id, bounds)
    else {
        unreachable!()
    };
    shapes[0].shape_id = sel(&format!("pc_shape_{id}"));
    Mutation::MoveElements {
        document_id,
        diagram_id,
        shapes,
        flow_id,
        edge_id,
    }
}
/// A timer boundary event on `order`'s bottom edge with a flow to `end`.
fn with_boundary(s: &mut Session) {
    let mut setup = create(
        element(
            "timeout",
            "BoundaryEvent",
            json!({"type": "event", "definitions": timer(), "attached_to": "order", "cancel_activity": true}),
        ),
        [602.0, 332.0, 36.0, 36.0],
        "procurement",
    );
    setup.extend(mutations(json!([
        {"type": "createFlow", "document_id": docid(), "process_id": "procurement",
         "flow": {"id": "flow_timeout", "kind": "Sequence", "source": "timeout", "target": "end", "name": "", "documentation": "", "condition": null}},
        {"type": "createEdge", "document_id": docid(), "diagram_id": "procurement_diagram",
         "edge": {"id": "flow_timeout_di", "element": "flow_timeout",
                  "waypoints": [{"x": 620, "y": 368}, {"x": 620, "y": 390}, {"x": 798, "y": 390}, {"x": 798, "y": 328}],
                  "label": null, "style": null}}
    ])));
    batch(s, setup).unwrap();
}

#[test]
fn boundary_events_follow_their_host_and_only_slide_along_its_border() {
    let mut s = session();
    with_boundary(&mut s);
    let (host, event) = (shape(&s, "order"), shape(&s, "timeout"));
    let label = event.label.clone().unwrap().bounds.unwrap();
    let moved = Bounds {
        y: host.bounds.y - 40.0,
        ..host.bounds.clone()
    };
    batch(&mut s, vec![move_fixture("order", moved)]).unwrap();
    let after = shape(&s, "timeout");
    assert_eq!(after.bounds.y, event.bounds.y - 40.0);
    assert_eq!(after.bounds.x, event.bounds.x);
    assert_eq!(after.label.unwrap().bounds.unwrap().y, label.y - 40.0);
    // Its outgoing flow is re-docked at the moved event.
    let start = edge(&s, "flow_timeout").waypoints[0].clone();
    assert_eq!((start.x, start.y), (620.0, 328.0));
    // A keyboard-style move (plain prepare_move) of the event alone: along the border is fine,
    // off the border is refused.
    let doc = document(&s);
    let at = |dx: f64, dy: f64| {
        geometry::prepare_move(
            &doc,
            &sel("procurement_diagram"),
            vec![ShapeBounds {
                diagram_id: sel("procurement_diagram"),
                shape_id: sel("timeout_di"),
                bounds: Bounds {
                    x: after.bounds.x + dx,
                    y: after.bounds.y + dy,
                    ..after.bounds.clone()
                },
            }],
        )
    };
    assert!(at(-30.0, 0.0).is_ok());
    assert!(at(0.0, 60.0).is_err_and(|e| e.contains("border")));
}

#[test]
fn resize_follows_the_rules_and_never_cuts_off_children() {
    let mut s = session();
    let resize = |id: &str, bounds: Bounds| Mutation::ResizeShape {
        document_id: docid(),
        diagram_id: sel("procurement_diagram"),
        shape_id: sel(id),
        bounds,
    };
    // Tasks keep their size.
    let err = batch(
        &mut s,
        vec![resize("pc_shape_order", b(515.0, 270.0, 200.0, 80.0))],
    );
    assert!(matches!(err, Err(CommandError::InvalidParams(r)) if r.contains("keeps its size")));
    let sub = element(
        "sub",
        "SubProcess",
        json!({"type": "subProcess", "content": {"nodes": [element("inner", "Task", json!(null))], "lanes": [], "flows": []}}),
    );
    let mut setup = create(sub, [100.0, 450.0, 350.0, 200.0], "procurement");
    setup.extend(
        create(
            element("inner", "Task", json!(null)),
            [150.0, 500.0, 100.0, 80.0],
            "sub",
        )
        .into_iter()
        .skip(1),
    );
    batch(&mut s, setup).unwrap();
    let below_min = batch(
        &mut s,
        vec![resize("sub_di", b(100.0, 450.0, 130.0, 200.0))],
    );
    assert!(matches!(below_min, Err(CommandError::InvalidParams(r)) if r.contains("minimum")));
    let cut = batch(
        &mut s,
        vec![resize("sub_di", b(100.0, 450.0, 160.0, 200.0))],
    );
    assert!(matches!(cut, Err(CommandError::InvalidParams(r)) if r.contains("cut off")));
    // Shrinking from the top-left keeps the child where it is.
    batch(
        &mut s,
        vec![resize("sub_di", b(120.0, 470.0, 330.0, 180.0))],
    )
    .unwrap();
    assert_eq!(shape(&s, "sub").bounds, b(120.0, 470.0, 330.0, 180.0));
    assert_eq!(shape(&s, "inner").bounds, b(150.0, 500.0, 100.0, 80.0));
}

#[test]
fn resize_redocks_data_associations() {
    let mut s = session();
    let sub = element(
        "sub",
        "SubProcess",
        json!({"type": "subProcess", "content": {"nodes": [], "lanes": [], "flows": []}}),
    );
    batch(
        &mut s,
        create(sub, [100.0, 450.0, 350.0, 200.0], "procurement"),
    )
    .unwrap();
    batch(
        &mut s,
        mutations(json!([
            {"type": "createAt", "document_id": docid(), "diagram_id": "procurement_diagram",
             "kind": "DataObjectReference", "name": null, "center": {"x": 560, "y": 550},
             "node_id": "doc_ref", "shape_id": "doc_ref_di", "flow_id": null, "edge_id": null},
            {"type": "connect", "document_id": docid(), "diagram_id": "procurement_diagram",
             "source_id": "doc_ref", "target_id": "sub", "flow_id": "doc_in", "edge_id": "doc_in_di"}
        ])),
    )
    .unwrap();
    let before = edge(&s, "doc_in").waypoints;
    assert_eq!(before.last().unwrap().x, 450.0);
    batch(
        &mut s,
        vec![Mutation::ResizeShape {
            document_id: docid(),
            diagram_id: sel("procurement_diagram"),
            shape_id: sel("sub_di"),
            bounds: b(100.0, 450.0, 300.0, 200.0),
        }],
    )
    .unwrap();
    // The association's end follows the subprocess's right side.
    let after = edge(&s, "doc_in").waypoints;
    assert_eq!(after.last().unwrap().x, 400.0);
    assert_eq!(after[0], before[0]);
    assert!(
        after
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
    );
}

#[test]
fn snapping_matches_mid_and_corners_within_tolerance() {
    let moved = b(103.0, 50.0, 100.0, 80.0);
    let sibling = b(100.0, 300.0, 100.0, 80.0);
    let snap = geometry::snap(&moved, std::slice::from_ref(&sibling), &[], 7.0);
    assert_eq!((snap.dx, snap.dy), (-3.0, 0.0));
    assert_eq!((snap.guide_x, snap.guide_y), (Some(150.0), None));
    // A connected element's mid snaps vertically; too far away does not snap.
    let connected = b(400.0, 76.0, 36.0, 36.0);
    let snap = geometry::snap(&moved, &[], &[connected], 7.0);
    assert_eq!((snap.dx, snap.dy, snap.guide_y), (0.0, 4.0, Some(94.0)));
    let far = geometry::snap(&moved, &[b(500.0, 500.0, 10.0, 10.0)], &[], 7.0);
    assert_eq!(far, geometry::Snap::default());
}

#[test]
fn intermediate_events_attach_to_a_border_and_detach_from_it() {
    let mut s = session();
    batch(
        &mut s,
        create(
            element("wait", "IntermediateCatchEvent", json!({"type": "event", "definitions": timer(), "attached_to": null, "cancel_activity": null})),
            [300.0, 420.0, 36.0, 36.0],
            "procurement",
        ),
    )
    .unwrap();
    // order: x 515..655, y 270..350; drop centred on its bottom edge.
    batch(
        &mut s,
        vec![move_elements("wait", b(560.0, 332.0, 36.0, 36.0))],
    )
    .unwrap();
    let attached = node(&s, "wait");
    assert_eq!(attached.kind, NodeKind::BoundaryEvent);
    assert_eq!(
        processcraft_core::rules::attached_to(&attached),
        Some(&sel("order"))
    );
    assert_eq!(s.snapshot().undo_count, 3);
    // Moving the host now carries the event.
    batch(
        &mut s,
        vec![move_fixture("order", b(515.0, 260.0, 140.0, 80.0))],
    )
    .unwrap();
    assert_eq!(shape(&s, "wait").bounds.y, 322.0);
    // Dragged off the host it becomes an intermediate catch event with its timer.
    batch(
        &mut s,
        vec![move_elements("wait", b(560.0, 420.0, 36.0, 36.0))],
    )
    .unwrap();
    let detached = node(&s, "wait");
    assert_eq!(detached.kind, NodeKind::IntermediateCatchEvent);
    assert_eq!(processcraft_core::rules::attached_to(&detached), None);
    assert!(matches!(
        &detached.details,
        Some(processcraft_core::bpmn_model::NodeDetails::Event { definitions, .. }) if definitions.len() == 1
    ));
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(node(&s, "wait").kind, NodeKind::BoundaryEvent);
}

#[test]
fn dropping_a_node_on_a_flow_splits_it_in_one_undo_step() {
    let mut s = session();
    batch(
        &mut s,
        create(
            element("check", "Task", json!(null)),
            [300.0, 450.0, 100.0, 80.0],
            "procurement",
        ),
    )
    .unwrap();
    let undo = s.snapshot().undo_count;
    // flow_rejected runs decision (440,128) -> (798,128) -> end (798,292).
    batch(
        &mut s,
        vec![move_elements("check", b(550.0, 88.0, 100.0, 80.0))],
    )
    .unwrap();
    assert_eq!(
        ends(&flow(&s, "flow_rejected").unwrap()),
        ("decision".into(), "check".into())
    );
    assert_eq!(
        ends(&flow(&s, "split_flow").unwrap()),
        ("check".into(), "end".into())
    );
    let into = edge(&s, "flow_rejected").waypoints;
    assert_eq!(into.last().map(|p| (p.x, p.y)), Some((550.0, 128.0)));
    let out = edge(&s, "split_flow").waypoints;
    assert_eq!(out.first().map(|p| p.x), Some(650.0));
    assert_eq!(s.snapshot().undo_count, undo + 1);
    // The rejected condition stays on the original flow.
    assert!(flow(&s, "flow_rejected").unwrap().condition.is_some());
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(
        ends(&flow(&s, "flow_rejected").unwrap()),
        ("decision".into(), "end".into())
    );
    assert!(flow(&s, "split_flow").is_none());

    // Creating on a flow inserts too.
    batch(
        &mut s,
        vec![Mutation::CreateAt {
            document_id: docid(),
            diagram_id: sel("procurement_diagram"),
            kind: NodeKind::IntermediateThrowEvent,
            name: None,
            center: Point { x: 720.0, y: 310.0 },
            node_id: "signal".parse().unwrap(),
            shape_id: "signal_di".parse().unwrap(),
            flow_id: Some("created_flow".parse().unwrap()),
            edge_id: Some("created_edge".parse().unwrap()),
        }],
    )
    .unwrap();
    assert_eq!(
        ends(&flow(&s, "flow_order_end").unwrap()),
        ("order".into(), "signal".into())
    );
    assert_eq!(
        ends(&flow(&s, "created_flow").unwrap()),
        ("signal".into(), "end".into())
    );
    // It joined the lane under the drop point.
    let doc = document(&s);
    assert!(doc.processes[0].lanes[1].members.contains(&sel("signal")));
}

#[test]
fn reconnect_keeps_the_flow_and_follows_the_rules() {
    let mut s = session();
    let reconnect = |source: &str, target: &str| Mutation::Reconnect {
        document_id: docid(),
        diagram_id: sel("procurement_diagram"),
        flow_id: sel("flow_order_end"),
        source_id: sel(source),
        target_id: sel(target),
    };
    batch(&mut s, vec![reconnect("order", "decision")]).unwrap();
    assert_eq!(
        ends(&flow(&s, "flow_order_end").unwrap()),
        ("order".into(), "decision".into())
    );
    let points = edge(&s, "flow_order_end").waypoints;
    assert!(
        points
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
    );
    let refused = batch(&mut s, vec![reconnect("order", "start")]);
    assert!(matches!(refused, Err(CommandError::InvalidParams(r)) if r.contains("Start events")));
    let duplicate = batch(&mut s, vec![reconnect("review", "decision")]);
    assert!(
        matches!(duplicate, Err(CommandError::InvalidParams(r)) if r.contains("already connected"))
    );
}

#[test]
fn moving_into_a_lane_or_subprocess_updates_membership_and_scope() {
    let mut s = session();
    let sub = element(
        "sub",
        "SubProcess",
        json!({"type": "subProcess", "content": {"nodes": [], "lanes": [], "flows": []}}),
    );
    let mut setup = create(sub, [100.0, 450.0, 350.0, 200.0], "procurement");
    setup.extend(create(
        element("loose", "Task", json!(null)),
        [700.0, 230.0, 100.0, 80.0],
        "procurement",
    ));
    batch(&mut s, setup).unwrap();
    // It moves up into the request lane (y 40..220), then down into the procurement lane.
    batch(
        &mut s,
        vec![move_elements("loose", b(700.0, 60.0, 100.0, 80.0))],
    )
    .unwrap();
    let doc = document(&s);
    assert!(doc.processes[0].lanes[0].members.contains(&sel("loose")));
    batch(
        &mut s,
        vec![move_elements("loose", b(700.0, 240.0, 100.0, 80.0))],
    )
    .unwrap();
    let doc = document(&s);
    assert!(!doc.processes[0].lanes[0].members.contains(&sel("loose")));
    assert!(doc.processes[0].lanes[1].members.contains(&sel("loose")));
    batch(
        &mut s,
        vec![move_elements("loose", b(150.0, 500.0, 100.0, 80.0))],
    )
    .unwrap();
    let doc = document(&s);
    let located = processcraft_core::rules::locate(&doc, &sel("loose")).unwrap();
    assert_eq!(located.scope, Some(sel("sub")));
    // A connected node moves too; a sequence flow that would cross scopes (and cannot become a
    // message flow inside one pool) is deleted with its edge, in the same undo step.
    let before = s.snapshot();
    batch(
        &mut s,
        vec![move_fixture("end", b(300.0, 520.0, 36.0, 36.0))],
    )
    .unwrap();
    let doc = document(&s);
    let located = processcraft_core::rules::locate(&doc, &sel("end")).unwrap();
    assert_eq!(located.scope, Some(sel("sub")));
    assert!(processcraft_core::queries::find_flow(&doc, &sel("flow_order_end")).is_none());
    assert!(
        doc.diagrams[0]
            .edges
            .iter()
            .all(|e| e.element != sel("flow_order_end"))
    );
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
}
