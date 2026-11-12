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

/// The sequence flow in `scope` whose edge in `diagram` passes through `point` and that `node`
/// can be inserted into (bpmn-js `canInsert`: a flow node other than a boundary event, not
/// already an end of the flow, connectable from the source and to the target).
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
    diagram
        .edges
        .iter()
        .filter(|e| geometry::distance_to_path(point, &e.waypoints) <= FLOW_HIT)
        .find_map(|edge| {
            let flow = scope
                .flows
                .iter()
                .find(|f| f.id == edge.element && f.kind == FlowKind::Sequence)?;
            let (source, target) = (local(&flow.source)?, local(&flow.target)?);
            if *source == node.id || *target == node.id {
                return None;
            }
            let (s, t) = (
                queries::find_node(doc, source)?,
                queries::find_node(doc, target)?,
            );
            (rules::sequence_refusal(s, node).is_none()
                && rules::sequence_refusal(node, t).is_none())
            .then_some(flow)
        })
}

/// Split `flow_id` at `node` (bpmn-js DropOnFlowBehavior): the flow now ends at the node, a new
/// sequence flow runs from the node to the old target, both are routed again, and a flow that
/// would duplicate an existing connection is removed instead.
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
    let connected = |from: &ElementSelector, to: &ElementSelector| {
        all_flows(doc).any(|f| {
            f.kind == FlowKind::Sequence
                && f.id != *flow_id
                && local(&f.source) == Some(from)
                && local(&f.target) == Some(to)
        })
    };
    let (duplicate_in, duplicate_out) = (connected(&source, node), connected(node, &target));
    let source_node = queries::find_node(doc, &source).cloned();
    let node_kind = queries::find_node(doc, node).map(|n| n.kind);
    let node_value = queries::find_node(doc, node).cloned();
    let into = modeling::route(
        diagram,
        rules::ConnectionKind::Sequence,
        source_node.as_ref(),
        node_kind,
        &a,
        node_bounds,
    );
    let out = modeling::route(
        diagram,
        rules::ConnectionKind::Sequence,
        node_value.as_ref(),
        Some(
            queries::find_node(doc, &target)
                .ok_or("target not found")?
                .kind,
        ),
        node_bounds,
        &b,
    );
    let scope = queries::scopes(doc)
        .into_iter()
        .find(|s| s.flows.iter().any(|f| f.id == *flow_id))
        .map(|s| s.id.selector());
    let mut batch = Vec::new();
    if duplicate_in {
        batch.push(Mutation::Delete {
            document_id: doc.id,
            element_id: flow_id.clone(),
        });
    } else {
        flow_mut(doc, flow_id).ok_or("flow not found")?.target =
            Some(ModelReference::Local(node.clone()));
        batch.push(Mutation::Reroute {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            edge_id: edge.id.selector(),
            waypoints: into,
        });
    }
    if !duplicate_out {
        batch.push(Mutation::CreateFlow {
            document_id: doc.id,
            process_id: scope,
            flow: modeling::flow(ids.0.clone(), FlowKind::Sequence, node, &target),
        });
        batch.push(Mutation::CreateEdge {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            edge: modeling::edge(ids.1, ids.0.selector(), out),
        });
    }
    Ok(batch)
}

/// Whether `id` has no flows, data associations, or boundary events, so it can change scope.
fn detached(doc: &BpmnDocument, id: &ElementSelector) -> bool {
    !all_flows(doc).any(|f| local(&f.source) == Some(id) || local(&f.target) == Some(id))
        && !queries::scopes(doc).iter().any(|s| {
            s.data_associations
                .iter()
                .any(|a| a.owner == *id || a.target == *id || a.sources.contains(id))
                || s.nodes.iter().any(|n| rules::attached_to(n) == Some(id))
        })
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
/// hosts ([`geometry::prepare_move`]). With `semantic` set, moved flow nodes join the lane under
/// their centre, and a single moved element also: attaches to an activity border (intermediate
/// event becomes a boundary event) or detaches from its host (boundary becomes an intermediate
/// event); changes scope when dropped into another expanded subprocess or pool, if it has no
/// connections; and, with `split` IDs, is inserted into the sequence flow under its centre.
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
    if semantic
        && shapes.len() == 1
        && let Some(id) = element(&shapes[0])
        && let Some(node) = queries::find_node(doc, &id).cloned()
        && rules::locate(doc, &id).is_some_and(|l| l.node.is_some())
    {
        let bounds = shapes[0].bounds.clone();
        let center = mid(&bounds);
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
            if current.as_ref() != Some(&host) {
                if rules::locate(doc, &host).and_then(|l| l.scope) != scope {
                    return Err("Attach to an activity in the same process or subprocess".into());
                }
                follow_up = crate::replace::set_attachment(doc, &id, Some(&host))?;
            }
        } else if current.is_some() {
            rules::can_drop(Dropped::Node(&node), container.target)?;
            follow_up = crate::replace::set_attachment(doc, &id, None)?;
        } else if let Some(target) = container
            .scope
            .clone()
            .filter(|s| rules::is_flow_node(node.kind) && Some(s) != scope.as_ref())
        {
            rules::can_drop(Dropped::Node(&node), container.target)?;
            if !detached(doc, &id) {
                return Err(
                    "Only an element without connections can move into another process or subprocess"
                        .into(),
                );
            }
            reparent(doc, &id, &target)?;
        } else if let (Some(ids), Some(scope)) = (split_ids, &scope)
            && let Some(flow) = flow_under(doc, &diagram, &node, scope, &center)
        {
            split_at = Some((flow.id.selector(), id.clone(), bounds, ids));
        }
    }
    let prepared = geometry::prepare_move(doc, diagram_id, shapes.clone())?;
    let crate::commands::Request::Batch {
        mutations: mut batch,
    } = prepared.request
    else {
        return Err("invalid move".into());
    };
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

/// The resize rule for a shape: its minimum size, or `None` when it keeps its size.
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
    rules::can_resize(resized)
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
/// Resize a shape under the bpmn-js rules: only resizable elements, not below their minimum
/// size, east/west only for text annotations, and never cutting off contained elements (20 px
/// padding). Attached boundary events keep their place on the outline and docked edge ends
/// move with it.
pub fn resize(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    shape_id: &ElementSelector,
    bounds: Bounds,
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
    let rule = resize_rule(doc, shape).ok_or("This element keeps its size")?;
    if rule.east_west_only && (bounds.y != old.y || bounds.height != old.height) {
        return Err("This element only resizes horizontally".into());
    }
    if bounds.width < rule.width || (!rule.east_west_only && bounds.height < rule.height) {
        return Err(format!(
            "The minimum size is {} x {}",
            rule.width, rule.height
        ));
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
    // Every shape that changes: the resized one and its boundary events (same relative place).
    let mut changed = vec![(shape, bounds.clone())];
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
    for edge in &diagram.edges {
        let Some(flow) = all_flows(doc).find(|f| f.id == edge.element) else {
            continue;
        };
        let mut points = edge.waypoints.clone();
        let mut touched = false;
        for (s, b) in &changed {
            let ends = [local(&flow.source), local(&flow.target)];
            if ends.contains(&Some(&s.element))
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
            | (rules::ConnectionKind::Association, FlowKind::Association)
    );
    if !same_kind {
        return Err("These elements need another kind of connection; delete this one and connect them instead".into());
    }
    let scope_of_flow = queries::scopes(doc)
        .into_iter()
        .find(|s| s.flows.iter().any(|f| f.id == *flow_id))
        .map(|s| s.id.selector());
    if flow.kind != FlowKind::Message
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
    let waypoints = modeling::route(
        diagram,
        kind,
        queries::find_node(doc, source),
        queries::find_node(doc, target).map(|n| n.kind),
        a,
        b,
    );
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
    let mut batch = vec![
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
    ];
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
    if let (Some(flow), Some(edge)) = (ids.flow, ids.edge)
        && let Some(found) = flow_under(doc, &diagram, &node, &scope, &center)
    {
        let found = found.id.selector();
        // The split routes from the new node, so it needs the node in the document first.
        let mut probe = doc.clone();
        crate::commands::scope_parts_mut(&mut probe, &scope)
            .map_err(|_| "scope not found")?
            .0
            .push(node);
        let mut tail = split(&mut probe, diagram_id, &found, &id, &bounds, (flow, edge))?;
        let retargeted = all_flows(&probe).find(|f| f.id == found).cloned();
        if let Some(value) = retargeted {
            *flow_mut(doc, &found).ok_or("flow not found")? = value;
        }
        batch.append(&mut tail);
    }
    Ok(batch)
}
