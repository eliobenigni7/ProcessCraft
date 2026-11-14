//! Direct-manipulation behaviors shared by the GUI, CLI, and MCP, re-implemented from bpmn-js
//! (not copied): moves with attach/detach, drop on flow, lane membership and re-parenting; resize
//! rules; reconnecting a flow end; and creating an element at a drop point. Builders edit the
//! session's working copy where no mutation exists (kind changes, flow ends, scope moves) and
//! return the remaining mutations; the caller applies everything as one batch and undo step.
use crate::{
    bpmn_activity::ModelReference,
    bpmn_model::NodeDetails,
    commands::{Mutation, ShapeBounds},
    geometry,
    ids::{ElementIdentity, ElementSelector},
    model::*,
    modeling, queries,
    rules::{self, DropTarget, Dropped},
};

/// Distance (diagram units) within which a dropped shape's centre lies on a flow.
pub const FLOW_HIT: f64 = 5.0;
/// Children keep this distance from the border of a resized container (bpmn-js).
const CHILD_PADDING: f64 = 20.0;

fn mid(b: &Bounds) -> Point {
    Point {
        x: b.x + b.width / 2.0,
        y: b.y + b.height / 2.0,
    }
}
fn contains(b: &Bounds, p: &Point) -> bool {
    p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height
}
fn local(reference: &Option<ModelReference>) -> Option<&ElementSelector> {
    match reference {
        Some(ModelReference::Local(id)) => Some(id),
        _ => None,
    }
}
fn all_flows(doc: &BpmnDocument) -> impl Iterator<Item = &Flow> {
    queries::scopes(doc)
        .into_iter()
        .flat_map(|scope| scope.flows)
        .chain(doc.collaborations().flat_map(|c| &c.flows))
}
fn flow_mut<'a>(doc: &'a mut BpmnDocument, id: &ElementSelector) -> Option<&'a mut Flow> {
    let scope = queries::scopes(doc)
        .iter()
        .find(|s| s.flows.iter().any(|f| f.id == *id))
        .map(|s| s.id.selector());
    match scope {
        Some(scope) => crate::commands::scope_parts_mut(doc, &scope)
            .ok()?
            .2
            .iter_mut()
            .find(|f| f.id == *id),
        None => doc
            .collaboration
            .iter_mut()
            .chain(&mut doc.additional_collaborations)
            .flat_map(|c| &mut c.flows)
            .find(|f| f.id == *id),
    }
}
fn edges_of<'a>(
    doc: &'a BpmnDocument,
    element: &'a ElementSelector,
) -> impl Iterator<Item = &'a Edge> {
    doc.diagrams
        .iter()
        .flat_map(|d| &d.edges)
        .filter(move |e| e.element == *element)
}

/// Where a point lands: the scope (process or expanded subprocess) and lane that would own an
/// element dropped there, and the container kind for [`rules::can_drop`].
#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub scope: Option<ElementSelector>,
    pub lane: Option<ElementSelector>,
    pub target: DropTarget,
}
/// The innermost expanded subprocess, lane, or pool shape under `point` (ignoring `moving` and
/// its descendants), else the diagram's plane.
pub fn container_at(
    doc: &BpmnDocument,
    diagram: &Diagram,
    point: &Point,
    moving: Option<&ElementSelector>,
) -> Container {
    let scopes = queries::scopes(doc);
    let mut best: Option<(f64, Container)> = None;
    for shape in diagram.shapes.iter().filter(|s| contains(&s.bounds, point)) {
        let id = &shape.element;
        let candidate = if let Some(located) = rules::locate(doc, id) {
            if let Some(node) = located.node {
                let sub = matches!(node.details, Some(NodeDetails::SubProcess { .. }));
                if !sub
                    || shape.is_expanded != Some(true)
                    || moving.is_some_and(|m| m == id || located.ancestors.contains(m))
                {
                    continue;
                }
                Container {
                    scope: Some(id.clone()),
                    lane: None,
                    target: DropTarget::SubProcess { expanded: true },
                }
            } else if let Some(participant) = located.participant {
                let process = local(&participant.process).cloned();
                Container {
                    target: DropTarget::Participant {
                        collapsed: shape.is_expanded == Some(false),
                        has_process: process.is_some(),
                    },
                    scope: process,
                    lane: None,
                }
            } else {
                continue;
            }
        } else if let Some(scope) = scopes.iter().find(|s| s.lanes.iter().any(|l| l.id == *id)) {
            Container {
                scope: Some(scope.id.selector()),
                lane: Some(id.clone()),
                target: DropTarget::Lane,
            }
        } else {
            continue;
        };
        let area = shape.bounds.width * shape.bounds.height;
        if best.as_ref().is_none_or(|(a, _)| area < *a) {
            best = Some((area, candidate));
        }
    }
    if let Some((_, container)) = best {
        return container;
    }
    let plane = &diagram.plane;
    if doc.processes.iter().any(|p| p.id == *plane) {
        Container {
            scope: Some(plane.clone()),
            lane: None,
            target: DropTarget::Process,
        }
    } else if scopes.iter().any(|s| *s.id == *plane) {
        Container {
            scope: Some(plane.clone()),
            lane: None,
            target: DropTarget::SubProcess { expanded: true },
        }
    } else {
        Container {
            scope: None,
            lane: None,
            target: DropTarget::Collaboration {
                has_process_pool: doc
                    .collaborations()
                    .flat_map(|c| &c.participants)
                    .any(|p| p.process.is_some()),
            },
        }
    }
}

/// Whether `host` is a receive task that follows an event-based gateway.
fn after_event_gateway(doc: &BpmnDocument, host: &Node) -> bool {
    host.kind == NodeKind::ReceiveTask
        && all_flows(doc).any(|f| {
            local(&f.target) == Some(&host.id.selector())
                && local(&f.source)
                    .and_then(|s| queries::find_node(doc, s))
                    .is_some_and(|s| s.kind == NodeKind::EventBasedGateway)
        })
}
/// The topmost activity in `diagram` whose border band holds `point` and which `event` may
/// attach to (bpmn-js `canAttach`).
pub fn attach_host<'a>(
    doc: &'a BpmnDocument,
    diagram: &Diagram,
    event: &Node,
    point: &Point,
) -> Option<&'a Node> {
    diagram.shapes.iter().rev().find_map(|shape| {
        if shape.element == event.id || !geometry::on_border(point, &shape.bounds) {
            return None;
        }
        let host = queries::find_node(doc, &shape.element)?;
        rules::can_attach(event, host, true, after_event_gateway(doc, host))
            .ok()
            .map(|_| host)
    })
}

fn flow_kind(kind: rules::ConnectionKind) -> Option<FlowKind> {
    match kind {
        rules::ConnectionKind::Sequence => Some(FlowKind::Sequence),
        rules::ConnectionKind::Message => Some(FlowKind::Message),
        _ => None,
    }
}
/// The flow kinds of `source -> node` and `node -> target` when both are sequence or message
/// flows and one of them keeps `kind` (so the split flow can keep its ID on that part).
fn split_kinds(
    doc: &BpmnDocument,
    source: &ElementSelector,
    node: &ElementSelector,
    target: &ElementSelector,
    kind: FlowKind,
) -> Option<(FlowKind, FlowKind)> {
    let into = flow_kind(rules::can_connect(doc, source, node).ok()?)?;
    let out = flow_kind(rules::can_connect(doc, node, target).ok()?)?;
    (into == kind || out == kind).then_some((into, out))
}

/// The sequence flow of `scope`, or a message flow, whose edge in `diagram` passes through
/// `point` and that `node` (already in `doc`) can be inserted into (bpmn-js `canInsert`: a flow
/// node other than a boundary event, not already an end of the flow, connectable from the source
/// and to the target). A message flow splits into the connections the rules pick for each part,
/// e.g. a message flow into the node and a sequence flow on to the target in the same pool.
pub fn flow_under<'a>(
    doc: &'a BpmnDocument,
    diagram: &Diagram,
    node: &Node,
    scope: &ElementSelector,
    point: &Point,
) -> Option<&'a Flow> {
    if !rules::is_flow_node(node.kind) || node.kind == NodeKind::BoundaryEvent {
        return None;
    }
    let scope = queries::scopes(doc).into_iter().find(|s| *s.id == *scope)?;
    let id = node.id.selector();
    diagram
        .edges
        .iter()
        .filter(|e| geometry::distance_to_path(point, &e.waypoints) <= FLOW_HIT)
        .find_map(|edge| {
            let flow = scope
                .flows
                .iter()
                .find(|f| f.id == edge.element && f.kind == FlowKind::Sequence)
                .or_else(|| {
                    doc.collaborations()
                        .flat_map(|c| &c.flows)
                        .find(|f| f.id == edge.element && f.kind == FlowKind::Message)
                })?;
            let (source, target) = (local(&flow.source)?, local(&flow.target)?);
            if *source == id || *target == id {
                return None;
            }
            split_kinds(doc, source, &id, target, flow.kind).map(|_| flow)
        })
}

/// A flow's former source loses its default-flow marker for it.
fn clear_default(doc: &mut BpmnDocument, source: &ElementSelector, flow_id: &ElementSelector) {
    if let Ok(node) = crate::commands::node_mut(doc, source) {
        if let Some(a) = &mut node.activity
            && a.default_flow.as_ref() == Some(flow_id)
        {
            a.default_flow = None;
        }
        if let Some(NodeDetails::Gateway { default_flow, .. }) = &mut node.details
            && default_flow.as_ref() == Some(flow_id)
        {
            *default_flow = None;
        }
    }
}

/// Split `flow_id` at `node` (bpmn-js DropOnFlowBehavior): the flow keeps the part of its own
/// kind (the incoming one when both match) and ends, or starts, at the node; a new flow of the
/// kind the rules pick covers the other part; both are routed again, and a part that would
/// duplicate an existing connection is removed instead.
fn split(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    flow_id: &ElementSelector,
    node: &ElementSelector,
    node_bounds: &Bounds,
    ids: (ElementIdentity, ElementIdentity),
) -> Result<Vec<Mutation>, String> {
    let flow = all_flows(doc)
        .find(|f| f.id == *flow_id)
        .ok_or("flow not found")?
        .clone();
    let (source, target) = (
        local(&flow.source).ok_or("flow source not found")?.clone(),
        local(&flow.target).ok_or("flow target not found")?.clone(),
    );
    let (kind_in, kind_out) = split_kinds(doc, &source, node, &target, flow.kind)
        .ok_or("The element cannot be inserted into this flow")?;
    let keep_in = kind_in == flow.kind;
    let diagram = modeling::find_diagram(doc, diagram_id)?;
    let edge = edges_of(doc, flow_id)
        .next()
        .ok_or("flow edge not found")?
        .clone();
    let (Some(a), Some(b)) = (
        modeling::shape_bounds(diagram, &source).cloned(),
        modeling::shape_bounds(diagram, &target).cloned(),
    ) else {
        return Err("The flow's ends are not shown in this diagram".into());
    };
    let connected = |from: &ElementSelector, to: &ElementSelector, kind: FlowKind| {
        all_flows(doc).any(|f| {
            f.kind == kind
                && f.id != *flow_id
                && local(&f.source) == Some(from)
                && local(&f.target) == Some(to)
        })
    };
    let (duplicate_in, duplicate_out) = (
        connected(&source, node, kind_in),
        connected(node, &target, kind_out),
    );
    let node_value = queries::find_node(doc, node).cloned();
    let connection = |kind| match kind {
        FlowKind::Message => rules::ConnectionKind::Message,
        _ => rules::ConnectionKind::Sequence,
    };
    let into = modeling::route(
        diagram,
        connection(kind_in),
        queries::find_node(doc, &source),
        node_value.as_ref().map(|n| n.kind),
        &a,
        node_bounds,
    );
    let out = modeling::route(
        diagram,
        connection(kind_out),
        node_value.as_ref(),
        queries::find_node(doc, &target).map(|n| n.kind),
        node_bounds,
        &b,
    );
    let (kept, kept_route, new_kind, new_ends, new_route, kept_duplicate, new_duplicate) =
        if keep_in {
            (
                (&source, node),
                into,
                kind_out,
                (node, &target),
                out,
                duplicate_in,
                duplicate_out,
            )
        } else {
            (
                (node, &target),
                out,
                kind_in,
                (&source, node),
                into,
                duplicate_out,
                duplicate_in,
            )
        };
    let new_scope = match new_kind {
        FlowKind::Sequence => rules::locate(doc, node).and_then(|l| l.scope),
        _ => None,
    };
    let mut batch = Vec::new();
    if kept_duplicate {
        batch.push(Mutation::Delete {
            document_id: doc.id,
            element_id: flow_id.clone(),
        });
    } else {
        if !keep_in {
            clear_default(doc, &source, flow_id);
        }
        let value = flow_mut(doc, flow_id).ok_or("flow not found")?;
        value.source = Some(ModelReference::Local(kept.0.clone()));
        value.target = Some(ModelReference::Local(kept.1.clone()));
        batch.push(Mutation::Reroute {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            edge_id: edge.id.selector(),
            waypoints: kept_route,
        });
    }
    if !new_duplicate {
        batch.push(Mutation::CreateFlow {
            document_id: doc.id,
            process_id: new_scope,
            flow: modeling::flow(ids.0.clone(), new_kind, new_ends.0, new_ends.1),
        });
        batch.push(Mutation::CreateEdge {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            edge: modeling::edge(ids.1, ids.0.selector(), new_route),
        });
    }
    Ok(batch)
}

/// Remove flow `id` from its scope or collaboration root.
fn take_flow(doc: &mut BpmnDocument, id: &ElementSelector) -> Option<Flow> {
    let scope = queries::scopes(doc)
        .iter()
        .find(|s| s.flows.iter().any(|f| f.id == *id))
        .map(|s| s.id.selector());
    let flows = match scope {
        Some(scope) => crate::commands::scope_parts_mut(doc, &scope).ok()?.2,
        None => doc
            .collaboration
            .iter_mut()
            .chain(&mut doc.additional_collaborations)
            .map(|c| &mut c.flows)
            .find(|flows| flows.iter().any(|f| f.id == *id))?,
    };
    let index = flows.iter().position(|f| f.id == *id)?;
    Some(flows.remove(index))
}
/// Move flow nodes into other scopes (each with its boundary events) and fix their flows
/// (bpmn-js ReplaceConnectionBehavior after a move): a flow still allowed keeps its kind and
/// moves to the scope that now owns it; a sequence flow that now crosses pools becomes a message
/// flow and a message flow now inside one scope a sequence flow, when nothing kind-specific
/// (condition, message) would be lost; other flows are deleted. Elements with data associations
/// are refused. Returns the deletions and the flows whose route must be laid out again.
fn change_scope(
    doc: &mut BpmnDocument,
    moves: &[(ElementSelector, ElementSelector)],
) -> Result<(Vec<Mutation>, Vec<ElementSelector>), String> {
    let mut moved = Vec::new();
    for (id, target) in moves {
        if queries::scopes(doc).iter().any(|s| {
            s.data_associations
                .iter()
                .any(|a| a.owner == *id || a.target == *id || a.sources.contains(id))
        }) {
            return Err(
                "Only an element without data associations can move into another process or subprocess"
                    .into(),
            );
        }
        let attachers: Vec<ElementSelector> = queries::scopes(doc)
            .iter()
            .flat_map(|s| s.nodes)
            .filter(|n| rules::attached_to(n) == Some(id))
            .map(|n| n.id.selector())
            .collect();
        for element in std::iter::once(id.clone()).chain(attachers) {
            reparent(doc, &element, target)?;
            moved.push(element);
        }
    }
    let touched: Vec<Flow> = all_flows(doc)
        .filter(|f| {
            f.kind != FlowKind::Association
                && [local(&f.source), local(&f.target)]
                    .iter()
                    .any(|end| end.is_some_and(|e| moved.contains(e)))
        })
        .cloned()
        .collect();
    let (mut deleted, mut relaid) = (Vec::new(), Vec::new());
    for flow in touched {
        let id = flow.id.selector();
        let (Some(source), Some(target)) = (local(&flow.source), local(&flow.target)) else {
            continue;
        };
        let scope = rules::locate(doc, source).and_then(|l| l.scope);
        let home = queries::scopes(doc)
            .iter()
            .find(|s| s.flows.iter().any(|f| f.id == id))
            .map(|s| s.id.selector());
        let allowed = rules::can_connect(doc, source, target)
            .ok()
            .and_then(flow_kind);
        let representable = match allowed {
            Some(FlowKind::Sequence) => flow.message_ref.is_none(),
            Some(FlowKind::Message) => flow.condition.is_none(),
            _ => false,
        };
        if !representable {
            deleted.push(Mutation::Delete {
                document_id: doc.id,
                element_id: id,
            });
            continue;
        }
        let kind = allowed.unwrap_or(flow.kind);
        if kind == flow.kind && (kind == FlowKind::Message || home == scope) {
            continue;
        }
        if kind == FlowKind::Message {
            clear_default(doc, source, &id);
        }
        let mut value = take_flow(doc, &id).ok_or("flow not found")?;
        value.kind = kind;
        match kind {
            FlowKind::Sequence => {
                let scope = scope.ok_or("scope not found")?;
                let flows = crate::commands::scope_parts_mut(doc, &scope)
                    .map_err(|_| "scope not found")?
                    .2;
                // Sequence flows precede associations, as BPMN export writes them.
                let at = flows
                    .iter()
                    .position(|f| f.kind == FlowKind::Association)
                    .unwrap_or(flows.len());
                flows.insert(at, value);
            }
            _ => doc
                .collaboration
                .as_mut()
                .ok_or("Message flows need pools; add a pool for each process first")?
                .flows
                .push(value),
        }
        if kind != flow.kind {
            relaid.push(id);
        }
    }
    Ok((deleted, relaid))
}
/// Move node `id` from its scope into `scope`, leaving its lanes.
fn reparent(
    doc: &mut BpmnDocument,
    id: &ElementSelector,
    scope: &ElementSelector,
) -> Result<(), String> {
    let from = rules::locate(doc, id)
        .and_then(|l| l.scope)
        .ok_or("element not found")?;
    let (nodes, lanes, _) =
        crate::commands::scope_parts_mut(doc, &from).map_err(|_| "scope not found")?;
    let index = nodes
        .iter()
        .position(|n| n.id == *id)
        .ok_or("element not found")?;
    let node = nodes.remove(index);
    for lane in lanes.iter_mut() {
        lane.members.retain(|m| m != id);
    }
    crate::commands::scope_parts_mut(doc, scope)
        .map_err(|_| "scope not found")?
        .0
        .push(node);
    Ok(())
}
/// `AssignLane` for `id` into `lane` with its shapes at their current (or `moved`) bounds.
fn assign_lane(
    doc: &BpmnDocument,
    scope: &ElementSelector,
    id: &ElementSelector,
    lane: Option<ElementSelector>,
    moved: &[ShapeBounds],
) -> Mutation {
    let bounds = doc
        .diagrams
        .iter()
        .flat_map(|d| {
            d.shapes
                .iter()
                .filter(|s| s.element == *id)
                .map(|s| ShapeBounds {
                    diagram_id: d.id.selector(),
                    shape_id: s.id.selector(),
                    bounds: moved
                        .iter()
                        .find(|b| b.diagram_id == d.id && b.shape_id == s.id)
                        .map_or_else(|| s.bounds.clone(), |b| b.bounds.clone()),
                })
        })
        .collect();
    Mutation::AssignLane {
        document_id: doc.id,
        process_id: scope.clone(),
        element_id: id.clone(),
        lane_id: lane,
        bounds,
    }
}
fn lane_of(
    doc: &BpmnDocument,
    scope: &ElementSelector,
    id: &ElementSelector,
) -> Option<ElementSelector> {
    queries::scopes(doc)
        .into_iter()
        .find(|s| *s.id == *scope)?
        .lanes
        .iter()
        .find(|l| l.members.contains(id))
        .map(|l| l.id.selector())
}

/// Move shapes as one gesture (bpmn-js move with its behaviors). Boundary events follow their
/// hosts ([`geometry::prepare_move`]). With `semantic` set, every moved flow node (not inside
/// another moved subprocess) joins the lane under its centre and changes scope when dropped into
/// another expanded subprocess or pool (its flows are kept, converted between sequence and
/// message flow, or deleted, see [`change_scope`]); a single moved element also attaches to an
/// activity border (intermediate event becomes a boundary event) or detaches from its host
/// (boundary becomes an intermediate event), and, with `split` IDs, is inserted into the
/// sequence or message flow under its centre. Route-repair warnings go to the change set.
pub fn move_elements(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    shapes: Vec<ShapeBounds>,
    split_ids: Option<(ElementIdentity, ElementIdentity)>,
    semantic: bool,
) -> Result<Vec<Mutation>, String> {
    let diagram = modeling::find_diagram(doc, diagram_id)?.clone();
    let element = |b: &ShapeBounds| {
        diagram
            .shapes
            .iter()
            .find(|s| s.id == b.shape_id)
            .map(|s| s.element.clone())
    };
    let mut follow_up = Vec::new();
    let mut split_at = None;
    let mut relaid = Vec::new();
    let moved_ids: Vec<ElementSelector> = shapes.iter().filter_map(element).collect();
    let mut attach_handled = false;
    if semantic
        && shapes.len() == 1
        && let Some(id) = element(&shapes[0])
        && let Some(node) = queries::find_node(doc, &id).cloned()
        && rules::locate(doc, &id).is_some_and(|l| l.node.is_some())
    {
        let center = mid(&shapes[0].bounds);
        let container = container_at(doc, &diagram, &center, Some(&id));
        let scope = rules::locate(doc, &id).and_then(|l| l.scope);
        let event = matches!(
            node.kind,
            NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        );
        let host = event
            .then(|| attach_host(doc, &diagram, &node, &center))
            .flatten()
            .map(|h| h.id.selector());
        let current = rules::attached_to(&node).cloned();
        if let Some(host) = host {
            attach_handled = true;
            if current.as_ref() != Some(&host) {
                if rules::locate(doc, &host).and_then(|l| l.scope) != scope {
                    return Err("Attach to an activity in the same process or subprocess".into());
                }
                follow_up = crate::replace::set_attachment(doc, &id, Some(&host))?;
            }
        } else if current.is_some() {
            attach_handled = true;
            rules::can_drop(Dropped::Node(&node), container.target)?;
            follow_up = crate::replace::set_attachment(doc, &id, None)?;
        }
    }
    if semantic && !attach_handled {
        // Scope changes of every top-level moved flow node (bpmn-js moves the whole selection);
        // nothing changes scope when its old or new container moves along.
        let container_moving = |scope: &ElementSelector| {
            moved_ids.contains(scope)
                || doc.collaborations().flat_map(|c| &c.participants).any(|p| {
                    local(&p.process) == Some(scope) && moved_ids.contains(&p.id.selector())
                })
        };
        let mut moves = Vec::new();
        for b in &shapes {
            let Some(id) = element(b) else { continue };
            let Some(located) = rules::locate(doc, &id) else {
                continue;
            };
            let (Some(node), Some(scope)) = (located.node, located.scope.clone()) else {
                continue;
            };
            if !rules::is_flow_node(node.kind)
                || node.kind == NodeKind::BoundaryEvent
                || located.ancestors.iter().any(|a| moved_ids.contains(a))
                || container_moving(&scope)
            {
                continue;
            }
            let container = container_at(doc, &diagram, &mid(&b.bounds), Some(&id));
            if let Some(target) = container
                .scope
                .filter(|s| *s != scope && !container_moving(s))
            {
                rules::can_drop(Dropped::Node(node), container.target)?;
                moves.push((id, target));
            }
        }
        if moves.is_empty() {
            if let (Some(ids), [moved]) = (split_ids, shapes.as_slice())
                && let Some(id) = element(moved)
                && let Some(node) = queries::find_node(doc, &id)
                && let Some(scope) = rules::locate(doc, &id).and_then(|l| l.scope)
                && let Some(flow) = flow_under(doc, &diagram, node, &scope, &mid(&moved.bounds))
            {
                split_at = Some((flow.id.selector(), id.clone(), moved.bounds.clone(), ids));
            }
        } else {
            let (deleted, converted) = change_scope(doc, &moves)?;
            follow_up = deleted;
            relaid = converted;
        }
    }
    let prepared = geometry::prepare_move(doc, diagram_id, shapes.clone())?;
    crate::commands::warn(prepared.warnings);
    let crate::commands::Request::Batch {
        mutations: mut batch,
    } = prepared.request
    else {
        return Err("invalid move".into());
    };
    // Converted flows get the preferred layout of their new kind between the moved shapes.
    for flow_id in relaid {
        let Some(flow) = all_flows(doc).find(|f| f.id == flow_id) else {
            continue;
        };
        let (Some(source), Some(target)) = (local(&flow.source), local(&flow.target)) else {
            continue;
        };
        let moved_bounds = |e: &ElementSelector| {
            let shape = diagram.shapes.iter().find(|s| s.element == *e)?;
            Some(
                batch
                    .iter()
                    .find_map(|m| match m {
                        Mutation::MoveShape {
                            shape_id, bounds, ..
                        } if *shape_id == shape.id => Some(bounds.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| shape.bounds.clone()),
            )
        };
        let (Some(a), Some(b)) = (moved_bounds(source), moved_bounds(target)) else {
            continue;
        };
        let kind = match flow.kind {
            FlowKind::Message => rules::ConnectionKind::Message,
            _ => rules::ConnectionKind::Sequence,
        };
        let waypoints = modeling::route(
            &diagram,
            kind,
            queries::find_node(doc, source),
            queries::find_node(doc, target).map(|n| n.kind),
            &a,
            &b,
        );
        for edge in diagram.edges.iter().filter(|e| e.element == flow_id) {
            batch.push(Mutation::Reroute {
                document_id: doc.id,
                diagram_id: diagram_id.clone(),
                edge_id: edge.id.selector(),
                waypoints: waypoints.clone(),
            });
        }
    }
    batch.extend(follow_up);
    if let Some((flow, node, bounds, ids)) = split_at {
        batch.extend(split(doc, diagram_id, &flow, &node, &bounds, ids)?);
    }
    if semantic {
        for moved in &shapes {
            let Some(id) = element(moved) else { continue };
            let Some(node) = queries::find_node(doc, &id) else {
                continue;
            };
            if node.kind == NodeKind::BoundaryEvent || !rules::is_flow_node(node.kind) {
                continue;
            }
            let Some(scope) = rules::locate(doc, &id).and_then(|l| l.scope) else {
                continue;
            };
            let container = container_at(doc, &diagram, &mid(&moved.bounds), Some(&id));
            if container.scope.as_ref() == Some(&scope)
                && container.lane.is_some()
                && container.lane != lane_of(doc, &scope, &id)
            {
                batch.push(assign_lane(doc, &scope, &id, container.lane, &shapes));
            }
        }
    }
    Ok(batch)
}

/// The resize rule for a shape: its minimum size, or `None` when the shape shows no known
/// element. A node without `isExpanded` counts as collapsed.
pub fn resize_rule(doc: &BpmnDocument, shape: &Shape) -> Option<rules::MinSize> {
    let horizontal = shape.is_horizontal != Some(false);
    let element = &shape.element;
    let resized = if let Some(node) = queries::find_node(doc, element) {
        rules::Resized::Node {
            kind: node.kind,
            expanded: shape.is_expanded == Some(true),
        }
    } else if doc
        .collaborations()
        .flat_map(|c| &c.participants)
        .any(|p| p.id == *element)
    {
        rules::Resized::Participant { horizontal }
    } else if queries::scopes(doc)
        .iter()
        .any(|s| s.lanes.iter().any(|l| l.id == *element))
    {
        rules::Resized::Lane { horizontal }
    } else if queries::scopes(doc)
        .iter()
        .any(|s| s.groups.iter().any(|g| g.id == *element))
    {
        rules::Resized::Group
    } else {
        return None;
    };
    Some(rules::can_resize(resized))
}
/// Shapes a resize must keep inside: subprocess content, a pool's process nodes, lane members.
fn children<'a>(
    doc: &BpmnDocument,
    diagram: &'a Diagram,
    element: &ElementSelector,
) -> Vec<&'a Bounds> {
    let mut ids: Vec<ElementSelector> = Vec::new();
    for scope in queries::scopes(doc) {
        if *scope.id == *element {
            ids.extend(scope.nodes.iter().map(|n| n.id.selector()));
        }
        for lane in scope.lanes.iter().filter(|l| l.id == *element) {
            ids.extend(lane.members.iter().cloned());
        }
    }
    for participant in doc.collaborations().flat_map(|c| &c.participants) {
        if participant.id == *element
            && let Some(process) = local(&participant.process)
            && let Some(scope) = queries::scopes(doc).into_iter().find(|s| *s.id == *process)
        {
            ids.extend(scope.nodes.iter().map(|n| n.id.selector()));
        }
    }
    diagram
        .shapes
        .iter()
        .filter(|s| ids.contains(&s.element))
        .map(|s| &s.bounds)
        .collect()
}
/// Resize a shape under the bpmn-js rules: not below its minimum size, keeping the proportions
/// of events, gateways and data references, and never cutting off contained elements (20 px
/// padding). A boundary event keeps its centre on the host's border; an external label stays
/// centred below its shape. Attached boundary events keep their place on the outline and
/// docked edge ends move with it.
pub fn resize(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    shape_id: &ElementSelector,
    mut bounds: Bounds,
) -> Result<Vec<Mutation>, String> {
    let diagram = modeling::find_diagram(doc, diagram_id)?;
    let shape = diagram
        .shapes
        .iter()
        .find(|s| s.id == *shape_id)
        .ok_or("shape not found")?;
    let old = &shape.bounds;
    if !geometry::valid_bounds(&bounds) {
        return Err("invalid bounds".into());
    }
    let rule = resize_rule(doc, shape).ok_or("This element cannot be resized")?;
    if bounds.width < rule.width || bounds.height < rule.height {
        return Err(format!(
            "The minimum size is {} x {}",
            rule.width, rule.height
        ));
    }
    if rule.keep_aspect
        && old.width > 0.0
        && (bounds.width * old.height / old.width - bounds.height).abs() > 1.0
    {
        return Err("This element keeps its proportions".into());
    }
    let node = queries::find_node(doc, &shape.element);
    if node.is_some_and(|n| rules::attached_to(n).is_some()) {
        let c = mid(old);
        bounds.x = c.x - bounds.width / 2.0;
        bounds.y = c.y - bounds.height / 2.0;
    }
    let inside = children(doc, diagram, &shape.element)
        .into_iter()
        .filter(|c| {
            contains(old, &Point { x: c.x, y: c.y })
                && contains(
                    old,
                    &Point {
                        x: c.x + c.width,
                        y: c.y + c.height,
                    },
                )
        });
    for c in inside {
        if c.x - CHILD_PADDING < bounds.x
            || c.y - CHILD_PADDING < bounds.y
            || c.x + c.width + CHILD_PADDING > bounds.x + bounds.width
            || c.y + c.height + CHILD_PADDING > bounds.y + bounds.height
        {
            return Err("The resize would cut off contained elements".into());
        }
    }
    // Every shape that changes: the resized one, lanes and the pool sharing a lane edge with it
    // (bpmn-js resizeLane), and its boundary events (same relative place).
    let mut changed = vec![(shape, bounds.clone())];
    let target = diagram
        .shapes
        .iter()
        .position(|s| s.id == *shape_id)
        .ok_or("shape not found")?;
    for (i, b) in crate::containers::lanes_resize(doc, diagram, target, &bounds) {
        let lane = &diagram.shapes[i];
        let o = &lane.bounds;
        if let Some(rule) = resize_rule(doc, lane)
            && ((b.width < o.width && b.width < rule.width)
                || (b.height < o.height && b.height < rule.height))
        {
            return Err(format!(
                "A lane would become smaller than {} x {}",
                rule.width, rule.height
            ));
        }
        changed.push((lane, b));
    }
    let ratio = |n: f64, o: f64| if o == 0.0 { 1.0 } else { n / o };
    for scope in queries::scopes(doc) {
        for event in scope
            .nodes
            .iter()
            .filter(|n| rules::attached_to(n) == Some(&shape.element))
        {
            for s in diagram.shapes.iter().filter(|s| s.element == event.id) {
                let c = mid(&s.bounds);
                let x = bounds.x + (c.x - old.x) * ratio(bounds.width, old.width);
                let y = bounds.y + (c.y - old.y) * ratio(bounds.height, old.height);
                changed.push((
                    s,
                    Bounds {
                        x: x - s.bounds.width / 2.0,
                        y: y - s.bounds.height / 2.0,
                        ..s.bounds.clone()
                    },
                ));
            }
        }
    }
    let mut batch = Vec::new();
    for (s, b) in &changed {
        batch.push(Mutation::MoveShape {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            shape_id: s.id.selector(),
            bounds: b.clone(),
        });
    }
    if node.is_some_and(|n| modeling::has_external_label(n.kind))
        && let Some(label) = shape.label.as_ref().and_then(|l| l.bounds.as_ref())
    {
        batch.push(Mutation::SetLabelBounds {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            representation_id: shape.id.selector(),
            bounds: Bounds {
                x: bounds.x + bounds.width / 2.0 - label.width / 2.0,
                y: label.y + bounds.y + bounds.height - (old.y + old.height),
                ..label.clone()
            },
        });
    }
    for edge in &diagram.edges {
        let (from, to) = modeling::edge_ends(doc, diagram, edge);
        let mut points = edge.waypoints.clone();
        let mut touched = false;
        for (s, b) in &changed {
            if [from, to].into_iter().flatten().any(|e| e.id == s.id)
                && let Some(p) = geometry::redock(&points, &s.bounds, b)
            {
                points = p;
                touched = true;
            }
        }
        if touched {
            batch.push(Mutation::Reroute {
                document_id: doc.id,
                diagram_id: diagram_id.clone(),
                edge_id: edge.id.selector(),
                waypoints: points,
            });
        }
    }
    Ok(batch)
}

/// Reconnect a flow end (bpmn-js reconnect): the flow keeps its ID and kind and gets a new route.
/// Refused when the rules forbid the new pair, when it would need another flow kind or scope, or
/// when it would duplicate a connection. A changed source drops a default-flow marker.
pub fn reconnect(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    flow_id: &ElementSelector,
    source: &ElementSelector,
    target: &ElementSelector,
) -> Result<Vec<Mutation>, String> {
    let flow = all_flows(doc)
        .find(|f| f.id == *flow_id)
        .ok_or("flow not found")?
        .clone();
    let (old_source, old_target) = (
        local(&flow.source)
            .ok_or("The flow has no local source")?
            .clone(),
        local(&flow.target)
            .ok_or("The flow has no local target")?
            .clone(),
    );
    if old_source == *source && old_target == *target {
        return Ok(Vec::new());
    }
    let kind = rules::can_connect(doc, source, target)?;
    let same_kind = matches!(
        (kind, flow.kind),
        (rules::ConnectionKind::Sequence, FlowKind::Sequence)
            | (rules::ConnectionKind::Message, FlowKind::Message)
            | (
                rules::ConnectionKind::Association | rules::ConnectionKind::CompensationAssociation,
                FlowKind::Association
            )
    );
    if !same_kind {
        return Err("These elements need another kind of connection; delete this one and connect them instead".into());
    }
    let scope_of_flow = queries::scopes(doc)
        .into_iter()
        .find(|s| s.flows.iter().any(|f| f.id == *flow_id))
        .map(|s| s.id.selector());
    if flow.kind == FlowKind::Sequence
        && rules::locate(doc, source).and_then(|l| l.scope) != scope_of_flow
    {
        return Err("The flow must stay within its process or subprocess".into());
    }
    if all_flows(doc).any(|f| {
        f.id != *flow_id
            && f.kind == flow.kind
            && local(&f.source) == Some(source)
            && local(&f.target) == Some(target)
    }) {
        return Err("These elements are already connected".into());
    }
    let edges: Vec<Edge> = edges_of(doc, flow_id).cloned().collect();
    let edge = edges.first().ok_or("The flow is not shown")?.clone();
    if edges.len() > 1 || !edge_in(doc, diagram_id, &edge) {
        return Err("Reconnect a flow shown once, in this diagram".into());
    }
    let diagram = modeling::find_diagram(doc, diagram_id)?;
    let (Some(a), Some(b)) = (
        modeling::shape_bounds(diagram, source),
        modeling::shape_bounds(diagram, target),
    ) else {
        return Err("Both elements must be shown in this diagram".into());
    };
    // Keep the interior bendpoints when the new end docks on them cleanly; otherwise route anew.
    let points = &edge.waypoints;
    let kept = (points.len() > 2)
        .then(|| {
            let start = (old_source != *source).then(|| dock_toward(a, &points[1]));
            let end = (old_target != *target).then(|| dock_toward(b, &points[points.len() - 2]));
            geometry::repair_endpoints(points, start, end, &[a.clone(), b.clone()]).ok()
        })
        .flatten()
        .filter(|r| r.warning.is_none())
        .and_then(|r| r.waypoints);
    let waypoints = kept.unwrap_or_else(|| {
        modeling::route(
            diagram,
            kind,
            queries::find_node(doc, source),
            queries::find_node(doc, target).map(|n| n.kind),
            a,
            b,
        )
    });
    if old_source != *source
        && let Ok(node) = crate::commands::node_mut(doc, &old_source)
    {
        if let Some(a) = &mut node.activity
            && a.default_flow.as_ref() == Some(flow_id)
        {
            a.default_flow = None;
        }
        if let Some(NodeDetails::Gateway { default_flow, .. }) = &mut node.details
            && default_flow.as_ref() == Some(flow_id)
        {
            *default_flow = None;
        }
    }
    let value = flow_mut(doc, flow_id).ok_or("flow not found")?;
    value.source = Some(ModelReference::Local(source.clone()));
    value.target = Some(ModelReference::Local(target.clone()));
    Ok(vec![Mutation::Reroute {
        document_id: doc.id,
        diagram_id: diagram_id.clone(),
        edge_id: edge.id.selector(),
        waypoints,
    }])
}
/// The border point of `b` facing `n`: straight across when `n` is level with a side (so the
/// end segment stays orthogonal), else the middle of the side on the dominant axis.
fn dock_toward(b: &Bounds, n: &Point) -> Point {
    let (right, bottom) = (b.x + b.width, b.y + b.height);
    let c = mid(b);
    let (dx, dy) = (n.x - c.x, n.y - c.y);
    if n.y > b.y && n.y < bottom && (n.x <= b.x || n.x >= right) {
        Point {
            x: if dx < 0.0 { b.x } else { right },
            y: n.y,
        }
    } else if n.x > b.x && n.x < right && (n.y <= b.y || n.y >= bottom) {
        Point {
            x: n.x,
            y: if dy < 0.0 { b.y } else { bottom },
        }
    } else if dx.abs() * b.height >= dy.abs() * b.width {
        Point {
            x: if dx < 0.0 { b.x } else { right },
            y: c.y,
        }
    } else {
        Point {
            x: c.x,
            y: if dy < 0.0 { b.y } else { bottom },
        }
    }
}
fn edge_in(doc: &BpmnDocument, diagram_id: &ElementSelector, edge: &Edge) -> bool {
    doc.diagrams
        .iter()
        .any(|d| d.id == *diagram_id && d.edges.iter().any(|e| e.id == edge.id))
}

/// IDs of what [`create_at`] creates; the flow and edge are used when it splits a flow.
pub struct CreateIds {
    pub node: ElementIdentity,
    pub shape: ElementIdentity,
    pub flow: Option<ElementIdentity>,
    pub edge: Option<ElementIdentity>,
}
/// Create a `kind` element centred on `center` (bpmn-js create with its behaviors): it joins the
/// scope and lane under the point; an intermediate event dropped on an activity border becomes a
/// boundary event of it; a flow node dropped on a sequence flow is inserted into it.
pub fn create_at(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    kind: NodeKind,
    name: Option<String>,
    center: Point,
    ids: CreateIds,
) -> Result<Vec<Mutation>, String> {
    if !geometry::valid_point(&center) {
        return Err("invalid position".into());
    }
    let diagram = modeling::find_diagram(doc, diagram_id)?.clone();
    let (width, height) = modeling::default_size(kind);
    let bounds = Bounds {
        x: center.x - width / 2.0,
        y: center.y - height / 2.0,
        width,
        height,
    };
    let id = ids.node.selector();
    let mut node = modeling::new_node(kind, ids.node.clone(), name.clone(), None);
    let host = matches!(
        kind,
        NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent
    )
    .then(|| attach_host(doc, &diagram, &node, &center))
    .flatten()
    .map(|h| h.id.selector());
    let container = container_at(doc, &diagram, &center, None);
    let scope = if let Some(host) = &host {
        node = modeling::new_node(
            NodeKind::BoundaryEvent,
            ids.node.clone(),
            name,
            Some(host.clone()),
        );
        if kind == NodeKind::IntermediateThrowEvent
            && let Some(NodeDetails::Event { definitions, .. }) = &mut node.details
        {
            definitions.clear();
        }
        rules::locate(doc, host)
            .and_then(|l| l.scope)
            .ok_or("host not found")?
    } else if kind == NodeKind::BoundaryEvent {
        return Err("Drop a boundary event on the border of an activity".into());
    } else {
        rules::can_drop(Dropped::Node(&node), container.target)?;
        container
            .scope
            .clone()
            .ok_or("Flow elements belong in a process, pool, lane, or expanded subprocess")?
    };
    let shape_kind = node.kind;
    let shape_id = ids.shape.selector();
    let mut batch: Vec<Mutation> = modeling::backing_data_object(doc.id, &scope, &node)
        .into_iter()
        .collect();
    batch.extend([
        Mutation::CreateElement {
            document_id: doc.id,
            process_id: scope.clone(),
            element: node.clone(),
        },
        Mutation::CreateShape {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            shape: modeling::new_shape(shape_kind, ids.shape, id.clone(), bounds.clone()),
        },
    ]);
    if host.is_some() {
        return Ok(batch);
    }
    if container.lane.is_some() && rules::is_flow_node(kind) {
        batch.push(Mutation::AssignLane {
            document_id: doc.id,
            process_id: scope.clone(),
            element_id: id.clone(),
            lane_id: container.lane.clone(),
            bounds: vec![ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: shape_id.clone(),
                bounds: bounds.clone(),
            }],
        });
    }
    if let (Some(flow), Some(edge)) = (ids.flow, ids.edge) {
        // The rules and the split route from the new node, so it goes into a probe first.
        let mut probe = doc.clone();
        crate::commands::scope_parts_mut(&mut probe, &scope)
            .map_err(|_| "scope not found")?
            .0
            .push(node);
        let found = queries::find_node(&probe, &id)
            .and_then(|n| flow_under(&probe, &diagram, n, &scope, &center))
            .map(|f| (f.id.selector(), local(&f.source).cloned()));
        if let Some((found, source)) = found {
            let mut tail = split(&mut probe, diagram_id, &found, &id, &bounds, (flow, edge))?;
            // Copy back the split flow and its source (which may lose a default marker).
            if let Some(value) = all_flows(&probe).find(|f| f.id == found).cloned() {
                *flow_mut(doc, &found).ok_or("flow not found")? = value;
            }
            if let Some(source) = source
                && let Some(value) = queries::find_node(&probe, &source).cloned()
                && let Ok(node) = crate::commands::node_mut(doc, &source)
            {
                *node = value;
            }
            batch.append(&mut tail);
        }
    }
    Ok(batch)
}
