//! Core modeling parity items closed after phase 6: compensation associations, self-loops,
//! nested message flows, cross-scope and pool associations, vertical layouts, flow conversion
//! on moves, multi-element moves, message-flow splitting, route-repair warnings, lane and
//! collapse behaviors, the pool-limited space tool, the extended clipboard, and the align /
//! distribute / duplicate commands.
use processcraft_core::{
    bpmn_model::{AssociationDirection, EventReference},
    commands::{ChangeSet, CommandError, Envelope, Mutation, Request, Session, ShapeBounds},
    geometry::{RouteLayout, layout_route_vertical},
    ids::{DocumentId, ElementSelector},
    model::*,
    queries::{find_flow, find_node},
    replace::{ReplaceTarget, Trigger},
    rules::{self, locate},
};
use serde_json::json;

const DIAGRAM: &str = "procurement_diagram";

fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn send(s: &mut Session, request: Request) -> Result<ChangeSet, CommandError> {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request)
}
fn batch(s: &mut Session, mutations: Vec<Mutation>) -> Result<ChangeSet, CommandError> {
    send(s, Request::Batch { mutations })
}
fn run(s: &mut Session, value: serde_json::Value) -> Result<ChangeSet, CommandError> {
    batch(s, serde_json::from_value(value).unwrap())
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
fn bounds(s: &Session, element: &str) -> Bounds {
    document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(element))
        .unwrap()
        .bounds
        .clone()
}
fn create_at(kind: &str, id: &str, x: f64, y: f64) -> serde_json::Value {
    json!({"type": "createAt", "document_id": docid(), "diagram_id": DIAGRAM, "kind": kind,
           "name": null, "center": {"x": x, "y": y}, "node_id": id, "shape_id": format!("{id}_di"),
           "flow_id": format!("{id}_split"), "edge_id": format!("{id}_split_di")})
}
fn connect(source: &str, target: &str, id: &str) -> serde_json::Value {
    json!({"type": "connect", "document_id": docid(), "diagram_id": DIAGRAM,
           "source_id": source, "target_id": target, "flow_id": id, "edge_id": format!("{id}_di")})
}
fn move_to(s: &Session, moves: &[(&str, Bounds)]) -> Mutation {
    let diagram = &document(s).diagrams[0];
    Mutation::MoveElements {
        document_id: docid(),
        diagram_id: sel(DIAGRAM),
        shapes: moves
            .iter()
            .map(|(element, b)| ShapeBounds {
                diagram_id: sel(DIAGRAM),
                shape_id: diagram
                    .shapes
                    .iter()
                    .find(|x| x.element == sel(element))
                    .unwrap()
                    .id
                    .selector(),
                bounds: b.clone(),
            })
            .collect(),
        flow_id: Some("moved_split".parse().unwrap()),
        edge_id: Some("moved_split_di".parse().unwrap()),
    }
}
fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}
fn ends(flow: &Flow) -> (ElementSelector, ElementSelector) {
    let end = |r: &Option<EventReference>| match r {
        Some(EventReference::Local(id)) => id.clone(),
        _ => panic!("local end expected"),
    };
    (end(&flow.source), end(&flow.target))
}
fn coords(points: &[Point]) -> Vec<(f64, f64)> {
    points.iter().map(|p| (p.x, p.y)).collect()
}

/// The fixture wrapped in pool `Buyer` plus pool `Supplier` (600x250 at 300,675) holding
/// task `t2` and an expanded subprocess `sub` with task `inner`.
struct Pools {
    s: Session,
    buyer: ElementSelector,
    supplier: ElementSelector,
    supplier_process: ElementSelector,
}
fn pools() -> Pools {
    let mut s = session();
    run(
        &mut s,
        json!([{"type": "createPool", "document_id": docid(), "diagram_id": DIAGRAM, "name": "Buyer"}]),
    )
    .unwrap();
    run(
        &mut s,
        json!([{"type": "createPool", "document_id": docid(), "diagram_id": DIAGRAM,
                "name": "Supplier", "center": {"x": 600.0, "y": 800.0}}]),
    )
    .unwrap();
    run(
        &mut s,
        json!([
            create_at("Task", "t2", 380.0, 760.0),
            create_at("SubProcess", "sub", 700.0, 800.0),
            create_at("Task", "inner", 650.0, 800.0),
        ]),
    )
    .unwrap();
    let doc = document(&s);
    let participants = &doc.collaboration.as_ref().unwrap().participants;
    let buyer = participants
        .iter()
        .find(|p| p.process == Some(EventReference::Local(sel("procurement"))))
        .unwrap();
    let supplier = participants.iter().find(|p| p.id != buyer.id).unwrap();
    let Some(EventReference::Local(supplier_process)) = supplier.process.clone() else {
        panic!("supplier process")
    };
    assert_eq!(locate(&doc, &sel("inner")).unwrap().scope, Some(sel("sub")));
    Pools {
        buyer: buyer.id.selector(),
        supplier: supplier.id.selector(),
        supplier_process,
        s,
    }
}

#[test]
fn compensation_association_marks_the_handler_and_drops_its_sequence_flows() {
    let mut s = session();
    // A catch event dropped on review's bottom border becomes a boundary event; make it a
    // compensation boundary event.
    run(
        &mut s,
        json!([create_at("IntermediateCatchEvent", "comp", 260.0, 168.0)]),
    )
    .unwrap();
    batch(
        &mut s,
        vec![Mutation::ReplaceElement {
            document_id: docid(),
            element_id: sel("comp"),
            target: ReplaceTarget::Node {
                kind: NodeKind::BoundaryEvent,
                trigger: Some(Trigger::Compensate),
                non_interrupting: false,
                event_sub_process: false,
                collapsed: false,
            },
        }],
    )
    .unwrap();
    let host = run(&mut s, json!([connect("comp", "review", "to_host")]));
    assert!(matches!(host, Err(CommandError::InvalidParams(r)) if r.contains("own host")));
    let before = s.snapshot();
    run(&mut s, json!([connect("comp", "order", "handler")])).unwrap();
    let doc = document(&s);
    let flow = find_flow(&doc, &sel("handler")).unwrap();
    assert_eq!(flow.kind, FlowKind::Association);
    assert_eq!(flow.association_direction, Some(AssociationDirection::One));
    assert_eq!(ends(flow), (sel("comp"), sel("order")));
    let order = find_node(&doc, &sel("order")).unwrap();
    assert!(rules::is_for_compensation(order));
    // The handler may not keep sequence flows (bpmn-js removes them).
    assert!(find_flow(&doc, &sel("flow_approved")).is_none());
    assert!(find_flow(&doc, &sel("flow_order_end")).is_none());
    assert_eq!(s.snapshot().undo_count, before.undo_count + 1);
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
}

#[test]
fn self_loop_uses_the_loop_layout_and_moves_with_its_shape() {
    let mut s = session();
    assert!(run(&mut s, json!([connect("end", "end", "bad")])).is_err());
    run(&mut s, json!([connect("review", "review", "loop")])).unwrap();
    let doc = document(&s);
    let flow = find_flow(&doc, &sel("loop")).unwrap();
    assert_eq!(flow.kind, FlowKind::Sequence);
    let edge = |s: &Session| {
        document(s).diagrams[0]
            .edges
            .iter()
            .find(|e| e.element == sel("loop"))
            .unwrap()
            .waypoints
            .clone()
    };
    // review is 190,88 140x80: out of the bottom, back into the left side (b:l).
    assert_eq!(
        coords(&edge(&s)),
        vec![
            (260.0, 168.0),
            (260.0, 188.0),
            (170.0, 188.0),
            (170.0, 128.0),
            (190.0, 128.0)
        ]
    );
    let moved = move_to(&s, &[("review", b(200.0, 88.0, 140.0, 80.0))]);
    batch(&mut s, vec![moved]).unwrap();
    assert_eq!(edge(&s)[0], Point { x: 270.0, y: 168.0 });
    assert_eq!(edge(&s).len(), 5);
}

#[test]
fn message_flows_reach_nodes_nested_in_a_subprocess() {
    let mut p = pools();
    run(
        &mut p.s,
        json!([connect("review", "inner", "nested_message")]),
    )
    .unwrap();
    let doc = document(&p.s);
    let root = doc.collaboration.as_ref().unwrap();
    let flow = root
        .flows
        .iter()
        .find(|f| f.id == sel("nested_message"))
        .unwrap();
    assert_eq!(flow.kind, FlowKind::Message);
    // Inside one pool, the scopes still separate sequence flows.
    assert!(run(&mut p.s, json!([connect("t2", "inner", "cross")])).is_err());
}

#[test]
fn associations_cross_scopes_and_reach_other_pools() {
    let mut p = pools();
    run(
        &mut p.s,
        json!([create_at("TextAnnotation", "note", 700.0, 180.0)]),
    )
    .unwrap();
    run(
        &mut p.s,
        json!([
            connect("note", "inner", "note_inner"),
            connect("note", p.supplier.to_string().as_str(), "note_pool"),
        ]),
    )
    .unwrap();
    let doc = document(&p.s);
    let procurement = &doc.processes[0];
    let nested = procurement
        .flows
        .iter()
        .find(|f| f.id == sel("note_inner"))
        .unwrap();
    assert_eq!(nested.kind, FlowKind::Association);
    let artifacts = &doc
        .collaboration
        .as_ref()
        .unwrap()
        .extras
        .as_ref()
        .unwrap()
        .artifacts;
    assert!(artifacts.iter().any(|a| matches!(
        a,
        processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(f)
            if f.id == sel("note_pool")
    )));
    // An annotation inside a pool does not connect to that pool.
    let own = run(
        &mut p.s,
        json!([connect(p.buyer.to_string().as_str(), "note", "own")]),
    );
    assert!(matches!(own, Err(CommandError::InvalidParams(r)) if r.contains("own container")));
}

#[test]
fn vertical_pools_place_south_and_add_lanes_side_by_side() {
    let swap = |b: &Bounds| Bounds {
        x: b.y,
        y: b.x,
        width: b.height,
        height: b.width,
    };
    let a = b(0.0, 0.0, 100.0, 80.0);
    let below = b(30.0, 200.0, 100.0, 80.0);
    // Vertical sequence flows are v:v, the transpose of the horizontal h:h route.
    let route = layout_route_vertical(&a, &below, &[], RouteLayout::Sequence);
    assert_eq!(route[0], Point { x: 50.0, y: 80.0 });
    assert_eq!(route.last().unwrap(), &Point { x: 80.0, y: 200.0 });
    let horizontal = processcraft_core::geometry::layout_route(
        &swap(&a),
        &swap(&below),
        &[],
        RouteLayout::Sequence,
    );
    assert_eq!(route.len(), horizontal.len());

    let mut p = pools();
    let supplier_shape = document(&p.s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == p.supplier)
        .unwrap()
        .clone();
    let vertical = Mutation::SetShapeDiAttributes {
        document_id: docid(),
        diagram_id: sel(DIAGRAM),
        shape_id: supplier_shape.id.selector(),
        is_horizontal: Some(false),
        is_marker_visible: None,
    };
    batch(&mut p.s, vec![vertical]).unwrap();
    // Append places south of t2 (330,720 100x80) at the default 50 gap, on the same x.
    run(
        &mut p.s,
        json!([{"type": "append", "document_id": docid(), "diagram_id": DIAGRAM,
                "source_id": "t2", "kind": "Task", "name": null, "node_id": "south",
                "shape_id": "south_di", "flow_id": "to_south", "edge_id": "to_south_di"}]),
    )
    .unwrap();
    assert_eq!(bounds(&p.s, "south"), b(330.0, 850.0, 100.0, 80.0));
    let doc = document(&p.s);
    let edge = doc.diagrams[0]
        .edges
        .iter()
        .find(|e| e.element == sel("to_south"))
        .unwrap();
    assert_eq!(
        coords(&edge.waypoints),
        vec![(380.0, 800.0), (380.0, 850.0)]
    );
    // A lane added below a vertical pool goes to its right; the pool grows by 120.
    let pool = bounds(&p.s, &p.supplier.to_string());
    run(
        &mut p.s,
        json!([{"type": "addLane", "document_id": docid(), "diagram_id": DIAGRAM,
                "element_id": p.supplier, "location": "below"}]),
    )
    .unwrap();
    let doc = document(&p.s);
    let process = doc
        .processes
        .iter()
        .find(|x| x.id == p.supplier_process)
        .unwrap();
    assert_eq!(process.lanes.len(), 2);
    let grown = bounds(&p.s, &p.supplier.to_string());
    assert_eq!((grown.x, grown.width), (pool.x, pool.width + 120.0));
    let lane = bounds(&p.s, &process.lanes[1].id.to_string());
    assert_eq!(
        lane,
        b(
            pool.x + pool.width,
            pool.y + 30.0,
            120.0,
            pool.height - 30.0
        )
    );
    run(
        &mut p.s,
        json!([{"type": "splitLane", "document_id": docid(), "diagram_id": DIAGRAM,
                "element_id": process.lanes[0].id, "count": 2}]),
    )
    .unwrap();
}

#[test]
fn moving_into_another_pool_turns_sequence_flows_into_message_flows() {
    let mut p = pools();
    run(
        &mut p.s,
        json!([
            create_at("Task", "t3", 440.0, 860.0),
            connect("t2", "t3", "t2_t3")
        ]),
    )
    .unwrap();
    let moved = move_to(&p.s, &[("t3", b(250.0, 290.0, 100.0, 80.0))]);
    batch(&mut p.s, vec![moved]).unwrap();
    let doc = document(&p.s);
    assert_eq!(
        locate(&doc, &sel("t3")).unwrap().scope,
        Some(sel("procurement"))
    );
    assert!(doc.processes[0].lanes[1].members.contains(&sel("t3")));
    let root = doc.collaboration.as_ref().unwrap();
    let flow = root.flows.iter().find(|f| f.id == sel("t2_t3")).unwrap();
    assert_eq!(flow.kind, FlowKind::Message);
    assert!(
        doc.processes
            .iter()
            .all(|x| x.flows.iter().all(|f| f.id != sel("t2_t3")))
    );
}

#[test]
fn multi_element_moves_change_scope_together() {
    let mut p = pools();
    run(
        &mut p.s,
        json!([
            create_at("Task", "a1", 380.0, 880.0),
            create_at("Task", "a2", 480.0, 880.0)
        ]),
    )
    .unwrap();
    let moves = [
        ("a1", b(710.0, 710.0, 100.0, 80.0)),
        ("a2", b(710.0, 805.0, 100.0, 80.0)),
    ];
    let moved = move_to(&p.s, &moves);
    batch(&mut p.s, vec![moved]).unwrap();
    let doc = document(&p.s);
    for id in ["a1", "a2"] {
        assert_eq!(locate(&doc, &sel(id)).unwrap().scope, Some(sel("sub")));
    }
}

#[test]
fn dropping_on_a_message_flow_splits_it_by_the_rules() {
    let mut p = pools();
    run(&mut p.s, json!([connect("review", "t2", "msg")])).unwrap();
    // The message flow runs straight down at x 330; a task created on it inside the supplier
    // pool receives the message and passes on to t2 by sequence flow.
    run(
        &mut p.s,
        json!([{"type": "reroute", "document_id": docid(), "diagram_id": DIAGRAM,
                "edge_id": "msg_di", "waypoints": [{"x": 330.0, "y": 168.0}, {"x": 330.0, "y": 720.0}]}]),
    )
    .unwrap();
    run(
        &mut p.s,
        json!([create_at("ReceiveTask", "rx", 330.0, 700.0)]),
    )
    .unwrap();
    let doc = document(&p.s);
    let message = find_flow(&doc, &sel("msg")).unwrap();
    assert_eq!(message.kind, FlowKind::Message);
    assert_eq!(ends(message), (sel("review"), sel("rx")));
    let process = doc
        .processes
        .iter()
        .find(|x| x.id == p.supplier_process)
        .unwrap();
    let onward = process
        .flows
        .iter()
        .find(|f| f.id == sel("rx_split"))
        .unwrap();
    assert_eq!(onward.kind, FlowKind::Sequence);
    assert_eq!(ends(onward), (sel("rx"), sel("t2")));
}

#[test]
fn moving_a_duplicate_over_its_original_lays_routes_out_again_with_a_warning() {
    let mut s = session();
    run(
        &mut s,
        json!([{"type": "duplicate", "document_id": docid(), "diagram_id": DIAGRAM,
                "element_ids": ["review", "decision"], "dx": 20.0, "dy": 20.0}]),
    )
    .unwrap();
    let doc = document(&s);
    let copy = |prefix: &str| {
        doc.processes[0]
            .nodes
            .iter()
            .find(|n| n.id.to_string().starts_with(prefix))
            .unwrap()
            .id
            .to_string()
    };
    let (review, decision) = (copy("review_"), copy("decision_"));
    let (r, d) = (bounds(&s, &review), bounds(&s, &decision));
    let moved = move_to(
        &s,
        &[
            (
                review.as_str(),
                b(r.x - 10.0, r.y - 10.0, r.width, r.height),
            ),
            (
                decision.as_str(),
                b(d.x - 10.0, d.y - 10.0, d.width, d.height),
            ),
        ],
    );
    let changes = batch(&mut s, vec![moved]).unwrap();
    assert!(
        changes
            .warnings
            .iter()
            .any(|w| w.contains("laid out again")),
        "{:?}",
        changes.warnings
    );
}

#[test]
fn deleting_a_lane_deletes_its_child_lanes() {
    let mut s = session();
    run(
        &mut s,
        json!([{"type": "splitLane", "document_id": docid(), "diagram_id": DIAGRAM,
                "element_id": "lane_request", "count": 2}]),
    )
    .unwrap();
    assert_eq!(document(&s).processes[0].lanes.len(), 4);
    run(
        &mut s,
        json!([{"type": "delete", "document_id": docid(), "element_id": "lane_request"}]),
    )
    .unwrap();
    let doc = document(&s);
    let lanes = &doc.processes[0].lanes;
    assert_eq!(lanes.len(), 1);
    // The remaining lane took the space and its nodes.
    assert!(lanes[0].members.contains(&sel("review")));
    // The child lanes' shapes went with them.
    assert!(
        doc.diagrams[0]
            .shapes
            .iter()
            .all(|x| !x.element.to_string().starts_with("Lane_"))
    );
}

#[test]
fn collapsing_an_empty_subprocess_creates_its_empty_diagram() {
    let mut s = session();
    run(
        &mut s,
        json!([create_at("SubProcess", "empty", 700.0, 600.0)]),
    )
    .unwrap();
    run(
        &mut s,
        json!([{"type": "toggleCollapse", "document_id": docid(), "diagram_id": DIAGRAM,
                "element_id": "empty"}]),
    )
    .unwrap();
    let doc = document(&s);
    let plane = doc
        .diagrams
        .iter()
        .find(|d| d.plane == sel("empty"))
        .unwrap();
    assert!(plane.shapes.is_empty() && plane.edges.is_empty());
}

#[test]
fn the_space_tool_stays_in_the_pool_under_the_drag_start() {
    let mut p = pools();
    let (decision, sub) = (bounds(&p.s, "decision"), bounds(&p.s, "sub"));
    run(
        &mut p.s,
        json!([{"type": "makeSpace", "document_id": docid(), "diagram_id": DIAGRAM,
                "axis": "x", "start": 360.0, "delta": 100.0, "at": {"x": 360.0, "y": 700.0}}]),
    )
    .unwrap();
    assert_eq!(bounds(&p.s, "decision"), decision);
    assert_eq!(bounds(&p.s, "sub").x, sub.x + 100.0);
}

#[test]
fn copying_a_pool_pastes_a_new_pool_with_its_process_and_lanes() {
    let mut p = pools();
    let doc = document(&p.s);
    let clip =
        processcraft_core::clipboard::copy(&doc, &sel(DIAGRAM), std::slice::from_ref(&p.buyer))
            .unwrap();
    assert_eq!(clip.pools.len(), 1);
    let pasted = processcraft_core::clipboard::paste(
        &doc,
        &sel(DIAGRAM),
        &sel("procurement"),
        &clip,
        0.0,
        1000.0,
    )
    .unwrap();
    batch(&mut p.s, pasted.mutations).unwrap();
    let doc = document(&p.s);
    assert_eq!(doc.collaboration.as_ref().unwrap().participants.len(), 3);
    let copy = doc.processes.last().unwrap();
    assert_ne!(copy.id, sel("procurement"));
    assert_eq!((copy.nodes.len(), copy.lanes.len()), (5, 2));
    for lane in &copy.lanes {
        assert!(
            lane.members
                .iter()
                .all(|m| copy.nodes.iter().any(|n| n.id == *m))
        );
    }
}

#[test]
fn copying_data_groups_and_associations_keeps_them_together() {
    let mut s = session();
    run(
        &mut s,
        json!([
            {"type": "createDataObject", "document_id": docid(), "scope_id": "procurement",
             "data_object": {"id": "data_obj", "name": "Order", "is_collection": null}},
            create_at("DataObjectReference", "data_ref", 260.0, 300.0),
        ]),
    )
    .unwrap();
    run(
        &mut s,
        json!([
            {"type": "setNodeDetails", "document_id": docid(), "element_id": "data_ref",
             "details": {"type": "dataObjectReference", "data_object_ref": "data_obj"}},
            connect("data_ref", "review", "reads"),
            {"type": "createGroup", "document_id": docid(), "scope_id": "procurement",
             "group": {"id": "grp", "category_value_ref": null}},
            {"type": "createShape", "document_id": docid(), "diagram_id": DIAGRAM,
             "shape": {"id": "grp_di", "element": "grp", "bounds": {"x": 170.0, "y": 70.0,
                       "width": 300.0, "height": 300.0}, "label": null, "style": null}},
        ]),
    )
    .unwrap();
    let doc = document(&s);
    let clip = processcraft_core::clipboard::copy(
        &doc,
        &sel(DIAGRAM),
        &[sel("review"), sel("data_ref"), sel("grp")],
    )
    .unwrap();
    assert_eq!(
        (
            clip.data_objects.len(),
            clip.groups.len(),
            clip.data_associations.len()
        ),
        (1, 1, 1)
    );
    let pasted = processcraft_core::clipboard::paste(
        &doc,
        &sel(DIAGRAM),
        &sel("procurement"),
        &clip,
        600.0,
        0.0,
    )
    .unwrap();
    if let Err(e) = batch(&mut s, pasted.mutations) {
        panic!("{e:?}");
    }
    let doc = document(&s);
    let process = &doc.processes[0];
    // data_obj, its pasted copy, and the backing object createAt gave data_ref before it was
    // re-pointed at data_obj.
    assert_eq!(process.data_objects.len(), 3);
    assert_eq!(process.groups.len(), 2);
    assert_eq!(process.data_associations.len(), 2);
    let pasted_ref = process
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::DataObjectReference && n.id != sel("data_ref"))
        .unwrap();
    let Some(processcraft_core::bpmn_model::NodeDetails::DataObjectReference {
        data_object_ref: Some(object),
    }) = &pasted_ref.details
    else {
        panic!("pasted reference keeps a data object")
    };
    assert_ne!(object, &sel("data_obj"));
    // The pasted review sits in the request lane and joins it.
    let review = process
        .nodes
        .iter()
        .find(|n| n.id.to_string().starts_with("review_"))
        .unwrap();
    assert!(process.lanes[0].members.contains(&review.id.selector()));
}

#[test]
fn align_distribute_and_duplicate_run_as_batch_commands() {
    let mut s = session();
    let before = s.snapshot();
    run(
        &mut s,
        json!([
            {"type": "align", "document_id": docid(), "diagram_id": DIAGRAM,
             "shape_ids": ["pc_shape_start", "pc_shape_review", "pc_shape_decision"],
             "alignment": "top"},
            {"type": "distribute", "document_id": docid(), "diagram_id": DIAGRAM,
             "shape_ids": ["pc_shape_start", "pc_shape_review", "pc_shape_decision"],
             "distribution": "horizontal"},
            {"type": "duplicate", "document_id": docid(), "diagram_id": DIAGRAM,
             "element_ids": ["order"], "dx": 0.0, "dy": 150.0},
        ]),
    )
    .unwrap();
    assert_eq!(bounds(&s, "start").y, 88.0);
    assert_eq!(bounds(&s, "decision").y, 88.0);
    let doc = document(&s);
    assert_eq!(doc.processes[0].nodes.len(), 6);
    assert_eq!(s.snapshot().undo_count, before.undo_count + 1);
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
}
