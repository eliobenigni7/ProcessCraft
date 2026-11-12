//! Pools, lanes, and containers shared by the GUI, CLI, and MCP, re-implemented from bpmn-js
//! and diagram-js behavior (not copied): adding, splitting, resizing, and deleting lanes;
//! auto-resizing containers; creating a pool around existing content; collapsing and expanding
//! subprocesses; and the space tool. Builders edit the session's working copy in place (or
//! return mutations), and the caller applies everything as one batch and undo step.
use crate::{
    bpmn_activity::ModelReference,
    bpmn_model::NodeDetails,
    commands::Mutation,
    geometry,
    ids::{ElementIdentity, ElementSelector},
    model::*,
    queries, rules,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Child lanes start this far right of their pool or parent lane (the pool's header).
pub const LANE_INDENTATION: f64 = 30.0;
/// Height of a lane added above or below another.
pub const LANE_SIZE: f64 = 120.0;

/// The axis the space tool works along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    X,
    Y,
}
/// Where [`add_lane`] puts the new lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LaneLocation {
    Above,
    Below,
}

fn fresh(prefix: &str) -> ElementIdentity {
    ElementIdentity::Legacy(
        format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
            .parse()
            .expect("prefix and UUID form an NCName"),
    )
}
fn lo(b: &Bounds, axis: Axis) -> f64 {
    match axis {
        Axis::X => b.x,
        Axis::Y => b.y,
    }
}
fn len(b: &Bounds, axis: Axis) -> f64 {
    match axis {
        Axis::X => b.width,
        Axis::Y => b.height,
    }
}
fn set(b: &mut Bounds, axis: Axis, start: f64, length: f64) {
    match axis {
        Axis::X => (b.x, b.width) = (start, length),
        Axis::Y => (b.y, b.height) = (start, length),
    }
}
fn mid(b: &Bounds) -> Point {
    Point {
        x: b.x + b.width / 2.0,
        y: b.y + b.height / 2.0,
    }
}
fn contains(b: &Bounds, p: &Point) -> bool {
    p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height
}
fn union(all: impl IntoIterator<Item = Bounds>) -> Option<Bounds> {
    all.into_iter().reduce(|a, b| {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        Bounds {
            x,
            y,
            width: (a.x + a.width).max(b.x + b.width) - x,
            height: (a.y + a.height).max(b.y + b.height) - y,
        }
    })
}
fn local(reference: &Option<ModelReference>) -> Option<&ElementSelector> {
    match reference {
        Some(ModelReference::Local(id)) => Some(id),
        _ => None,
    }
}
fn diagram_index(doc: &BpmnDocument, id: &ElementSelector) -> Result<usize, String> {
    doc.diagrams
        .iter()
        .position(|d| d.id == *id)
        .ok_or_else(|| "diagram not found".into())
}
fn shape_index(diagram: &Diagram, element: &ElementSelector) -> Result<usize, String> {
    diagram
        .shapes
        .iter()
        .position(|s| s.element == *element)
        .ok_or_else(|| "The element is not shown in this diagram".into())
}
/// Boundary events and their hosts.
fn hosts(doc: &BpmnDocument) -> HashMap<ElementSelector, ElementSelector> {
    queries::scopes(doc)
        .iter()
        .flat_map(|s| s.nodes)
        .filter_map(|n| Some((n.id.selector(), rules::attached_to(n)?.clone())))
        .collect()
}
fn container_shape(
    id: ElementIdentity,
    element: ElementSelector,
    bounds: Bounds,
    horizontal: Option<bool>,
) -> Shape {
    Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        id,
        element,
        is_expanded: None,
        is_horizontal: horizontal,
        is_marker_visible: None,
        bounds,
        label: Some(Label::default()),
        label_visibility: Some(DiLabelVisibility::Explicit),
        style: None,
    }
}

/// A space-tool step (diagram-js `calculateAdjustments` + `makeSpace`): shapes wholly past
/// `start` on `axis` (before it when `after` is false) move by `delta`; resizable shapes that
/// cross the line grow or shrink by it on that side.
#[derive(Clone, Copy, Debug)]
struct Space {
    axis: Axis,
    start: f64,
    delta: f64,
    after: bool,
}
/// New bounds per shape index of one diagram.
type Changes = HashMap<usize, Bounds>;

fn resizable(doc: &BpmnDocument, shape: &Shape) -> bool {
    crate::manipulate::resize_rule(doc, shape).is_some_and(|rule| !rule.east_west_only)
}
fn adjust(
    doc: &BpmnDocument,
    diagram: &Diagram,
    candidates: &[usize],
    space: Space,
    changes: &mut Changes,
) {
    let Space {
        axis,
        start,
        delta,
        after,
    } = space;
    let beyond = |v: f64| if after { v > start } else { v < start };
    let hosts = hosts(doc);
    let (mut moving, mut resizing, mut attachers) = (Vec::new(), Vec::new(), Vec::new());
    for &i in candidates {
        let shape = &diagram.shapes[i];
        let b = &shape.bounds;
        let (s, e) = (lo(b, axis), lo(b, axis) + len(b, axis));
        if hosts.contains_key(&shape.element) && beyond(s + len(b, axis) / 2.0) {
            attachers.push(i);
        } else if beyond(if after { s } else { e }) {
            moving.push(i);
        } else if s < start && e > start && resizable(doc, shape) {
            resizing.push(i);
        }
    }
    // Boundary events follow a moving host, or a resized one when their middle is past the line.
    let elements = |set: &[usize]| -> Vec<ElementSelector> {
        set.iter()
            .map(|&i| diagram.shapes[i].element.clone())
            .collect()
    };
    let (moved_hosts, resized_hosts) = (elements(&moving), elements(&resizing));
    for (i, shape) in diagram.shapes.iter().enumerate() {
        let Some(host) = hosts.get(&shape.element) else {
            continue;
        };
        if !moving.contains(&i)
            && (moved_hosts.contains(host)
                || (attachers.contains(&i) && resized_hosts.contains(host)))
        {
            moving.push(i);
        }
    }
    for i in moving {
        let mut b = diagram.shapes[i].bounds.clone();
        let (s, l) = (lo(&b, axis), len(&b, axis));
        set(&mut b, axis, s + delta, l);
        changes.insert(i, b);
    }
    for i in resizing {
        let mut b = diagram.shapes[i].bounds.clone();
        let (s, l) = (lo(&b, axis), len(&b, axis));
        if after {
            set(&mut b, axis, s, l + delta);
        } else {
            set(&mut b, axis, s + delta, l - delta);
        }
        changes.insert(i, b);
    }
}

/// The shapes (indices) at the ends of every edge of `diagram`: explicit DI ends first, then
/// the flow's or data association's ends (a data port falls back to its activity).
fn edge_ends(doc: &BpmnDocument, diagram: &Diagram) -> Vec<(Option<usize>, Option<usize>)> {
    let mut by_element = HashMap::new();
    let mut by_shape = HashMap::new();
    for (i, s) in diagram.shapes.iter().enumerate() {
        by_element.entry(s.element.clone()).or_insert(i);
        by_shape.insert(s.id.selector(), i);
    }
    let mut ends: HashMap<ElementSelector, [Vec<ElementSelector>; 2]> = HashMap::new();
    let scopes = queries::scopes(doc);
    for f in scopes
        .iter()
        .flat_map(|s| s.flows)
        .chain(doc.collaborations().flat_map(|c| &c.flows))
    {
        let end = |r: &Option<ModelReference>| local(r).cloned().into_iter().collect();
        ends.insert(f.id.selector(), [end(&f.source), end(&f.target)]);
    }
    for a in scopes.iter().flat_map(|s| s.data_associations) {
        if let Some(id) = &a.id {
            let source = a.sources.first().into_iter().cloned();
            ends.insert(
                id.selector(),
                [
                    source.chain([a.owner.clone()]).collect(),
                    vec![a.target.clone(), a.owner.clone()],
                ],
            );
        }
    }
    diagram
        .edges
        .iter()
        .map(|edge| {
            let found = ends.get(&edge.element);
            let end = |explicit: &Option<ElementSelector>, side: usize| {
                explicit
                    .as_ref()
                    .and_then(|s| by_shape.get(s))
                    .or_else(|| found?[side].iter().find_map(|e| by_element.get(e)))
                    .copied()
            };
            (end(&edge.source_element, 0), end(&edge.target_element, 1))
        })
        .collect()
}

/// Apply `changes` to diagram `di` (labels follow their shapes) and update its edges: an edge
/// whose ends both changed in a space step keeps its points before the line and shifts the rest
/// (diagram-js); one whose ends moved together is translated; otherwise each changed end is
/// re-docked on its new outline.
fn commit(doc: &mut BpmnDocument, di: usize, changes: &Changes, space: Option<Space>) {
    let ends = edge_ends(doc, &doc.diagrams[di]);
    let diagram = &mut doc.diagrams[di];
    let old: Vec<Bounds> = diagram.shapes.iter().map(|s| s.bounds.clone()).collect();
    for (&i, b) in changes {
        crate::commands::move_shape(&mut diagram.shapes[i], b.clone());
    }
    let shift = |i: usize| {
        let (o, n) = (&old[i], &changes[&i]);
        (o.width == n.width && o.height == n.height).then_some((n.x - o.x, n.y - o.y))
    };
    for (edge, (s, t)) in diagram.edges.iter_mut().zip(ends) {
        let changed = |i: Option<usize>| i.filter(|i| changes.contains_key(i));
        let (s, t) = (changed(s), changed(t));
        let translate = |edge: &mut Edge, dx: f64, dy: f64, only: &dyn Fn(&Point) -> bool| {
            for p in edge.waypoints.iter_mut().filter(|p| only(p)) {
                (p.x, p.y) = (p.x + dx, p.y + dy);
            }
            if let Some(b) = edge.label.as_mut().and_then(|l| l.bounds.as_mut())
                && only(&mid(b))
            {
                (b.x, b.y) = (b.x + dx, b.y + dy);
            }
        };
        match (s, t, space) {
            (None, None, _) => {}
            (Some(_), Some(_), Some(sp)) => {
                let beyond = |p: &Point| {
                    let v = match sp.axis {
                        Axis::X => p.x,
                        Axis::Y => p.y,
                    };
                    if sp.after { v > sp.start } else { v < sp.start }
                };
                let (dx, dy) = match sp.axis {
                    Axis::X => (sp.delta, 0.0),
                    Axis::Y => (0.0, sp.delta),
                };
                translate(edge, dx, dy, &beyond);
            }
            (Some(a), Some(b), None) if shift(a).is_some() && shift(a) == shift(b) => {
                let (dx, dy) = shift(a).unwrap_or_default();
                translate(edge, dx, dy, &|_| true);
            }
            _ => {
                for i in [s, t].into_iter().flatten() {
                    if let Some(points) = geometry::redock(&edge.waypoints, &old[i], &changes[&i]) {
                        edge.waypoints = points;
                    }
                }
            }
        }
    }
}

/// bpmn-js UpdateFlowNodeRefs: a flow node shown in `diagram_id` whose lane under its centre
/// changed between `old` and `doc`, or that has no lane yet, joins the (innermost) lane now under
/// its centre. The model keeps one lane per node.
pub(crate) fn sync_lanes(old: &BpmnDocument, doc: &mut BpmnDocument, diagram_id: &ElementSelector) {
    fn centres(
        doc: &BpmnDocument,
        diagram_id: &ElementSelector,
    ) -> HashMap<ElementSelector, Bounds> {
        let mut out = HashMap::new();
        for s in doc
            .diagrams
            .iter()
            .filter(|d| d.id == *diagram_id)
            .flat_map(|d| &d.shapes)
        {
            out.entry(s.element.clone())
                .or_insert_with(|| s.bounds.clone());
        }
        out
    }
    fn lane_at(
        lanes: &[Lane],
        shapes: &HashMap<ElementSelector, Bounds>,
        p: &Point,
    ) -> Option<ElementSelector> {
        lanes
            .iter()
            .filter_map(|l| Some((l, shapes.get(&l.id.selector())?)))
            .filter(|(_, b)| contains(b, p))
            .min_by(|a, b| (a.1.width * a.1.height).total_cmp(&(b.1.width * b.1.height)))
            .map(|(l, _)| l.id.selector())
    }
    let (now, before) = (centres(doc, diagram_id), centres(old, diagram_id));
    let old_scopes = queries::scopes(old);
    let mut assign = Vec::new();
    for scope in queries::scopes(doc).iter().filter(|s| !s.lanes.is_empty()) {
        let old_lanes = old_scopes
            .iter()
            .find(|s| s.id == scope.id)
            .map_or(&[][..], |s| s.lanes);
        for node in scope
            .nodes
            .iter()
            .filter(|n| rules::is_flow_node(n.kind) && n.kind != NodeKind::BoundaryEvent)
        {
            let id = node.id.selector();
            let Some(after) = now
                .get(&id)
                .and_then(|b| lane_at(scope.lanes, &now, &mid(b)))
            else {
                continue;
            };
            let current = scope
                .lanes
                .iter()
                .find(|l| l.members.contains(&id))
                .map(|l| l.id.selector());
            if current.as_ref() == Some(&after) {
                continue;
            }
            let was = before
                .get(&id)
                .and_then(|b| lane_at(old_lanes, &before, &mid(b)));
            if current.is_none() || was.as_ref() != Some(&after) {
                assign.push((scope.id.selector(), id, after));
            }
        }
    }
    for (scope, id, lane) in assign {
        if let Ok((_, lanes, _)) = crate::commands::scope_parts_mut(doc, &scope) {
            for l in lanes.iter_mut() {
                l.members.retain(|m| *m != id);
                if l.id == lane {
                    l.members.push(id.clone());
                }
            }
        }
    }
}

/// The lane set a lane or pool belongs to: the semantic scope holding the lanes, the pool shape
/// (if any) that is the lanes' root in this diagram, and the lane itself (none for a pool).
struct LaneContext {
    scope: ElementSelector,
    root: Option<usize>,
    lane: Option<Lane>,
}
fn lane_context(
    doc: &BpmnDocument,
    diagram: &Diagram,
    element: &ElementSelector,
) -> Result<LaneContext, String> {
    if let Some(p) = doc
        .collaborations()
        .flat_map(|c| &c.participants)
        .find(|p| p.id == *element)
    {
        return Ok(LaneContext {
            scope: local(&p.process)
                .ok_or("This pool has no process to hold lanes")?
                .clone(),
            root: Some(shape_index(diagram, element)?),
            lane: None,
        });
    }
    let (scope, lane) = queries::scopes(doc)
        .iter()
        .find_map(|s| {
            let lane = s.lanes.iter().find(|l| l.id == *element)?;
            Some((s.id.selector(), lane.clone()))
        })
        .ok_or("Select a lane or a pool")?;
    let centre = mid(&diagram.shapes[shape_index(diagram, element)?].bounds);
    let root = diagram.shapes.iter().position(|s| {
        contains(&s.bounds, &centre)
            && doc
                .collaborations()
                .flat_map(|c| &c.participants)
                .any(|p| p.id == s.element && local(&p.process) == Some(&scope))
    });
    Ok(LaneContext {
        scope,
        root,
        lane: Some(lane),
    })
}
/// Lanes of the context's set shown in the diagram (inside its pool), with their shape index.
fn shown_lanes(doc: &BpmnDocument, diagram: &Diagram, ctx: &LaneContext) -> Vec<(Lane, usize)> {
    let Some(scope) = queries::scopes(doc)
        .into_iter()
        .find(|s| *s.id == ctx.scope)
    else {
        return Vec::new();
    };
    scope
        .lanes
        .iter()
        .filter_map(|l| {
            let i = diagram.shapes.iter().position(|s| s.element == l.id)?;
            let inside = ctx.root.is_none_or(|r| {
                contains(&diagram.shapes[r].bounds, &mid(&diagram.shapes[i].bounds))
            });
            inside.then(|| (l.clone(), i))
        })
        .collect()
}
/// Lanes nested (at any depth) in `ancestor`.
fn descendants(lanes: &[(Lane, usize)], ancestor: &ElementSelector) -> Vec<ElementSelector> {
    let mut out = vec![ancestor.clone()];
    for _ in 0..32 {
        let before = out.len();
        for (l, _) in lanes {
            if l.parent.as_ref().is_some_and(|p| out.contains(p)) && !out.contains(&l.id.selector())
            {
                out.push(l.id.selector());
            }
        }
        if out.len() == before {
            break;
        }
    }
    out.remove(0);
    out
}
fn create_lanes(
    doc: &mut BpmnDocument,
    di: usize,
    scope: &ElementSelector,
    created: Vec<(Option<ElementSelector>, Bounds)>,
    horizontal: Option<bool>,
) -> Result<(), String> {
    for (parent, bounds) in created {
        let (lane, shape) = (fresh("Lane"), fresh("Lane_di"));
        crate::commands::scope_parts_mut(doc, scope)
            .map_err(|_| "scope not found")?
            .1
            .push(Lane {
                base: None,
                child_lane_set_id: None,
                child_lane_set_base: None,
                child_lane_set_name: None,
                partition_element: None,
                partition_element_ref: None,
                id: lane.clone(),
                name: None,
                parent,
                members: Vec::new(),
            });
        let diagram = &mut doc.diagrams[di];
        if let Some(order) = &mut diagram.plane_order {
            order.push(shape.selector());
        }
        diagram
            .shapes
            .push(container_shape(shape, lane.selector(), bounds, horizontal));
    }
    Ok(())
}
/// The selected lane or pool of a lane command.
fn lane_target(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    element: &ElementSelector,
) -> Result<(usize, usize, LaneContext), String> {
    let di = diagram_index(doc, diagram_id)?;
    let diagram = &doc.diagrams[di];
    let target = shape_index(diagram, element)?;
    Ok((di, target, lane_context(doc, diagram, element)?))
}
/// Lanes stack along y in a horizontal pool and along x in a vertical one; the indentation
/// (the pool's header) is on the other axis. Returns `(stack, across)`.
fn lane_axes(shape: &Shape) -> (Axis, Axis) {
    if shape.is_horizontal == Some(false) {
        (Axis::X, Axis::Y)
    } else {
        (Axis::Y, Axis::X)
    }
}
/// Bounds inside `s` from `start` for `length` along `stack`, indented by `indent` across.
fn lane_bounds(
    s: &Bounds,
    (stack, across): (Axis, Axis),
    start: f64,
    length: f64,
    indent: f64,
) -> Bounds {
    let mut b = s.clone();
    set(
        &mut b,
        across,
        lo(s, across) + indent,
        len(s, across) - indent,
    );
    set(&mut b, stack, start, length);
    b
}

/// Add a lane above or below a lane or pool (bpmn-js AddLaneHandler): a 120 high lane at the
/// pool's indentation; lanes and shapes beyond it in the pool move away and the pool and parent
/// lanes grow. A pool without lanes first gets one lane covering its body. In a vertical pool
/// lanes stack along x: above means left and below means right.
pub fn add_lane(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    element: &ElementSelector,
    location: LaneLocation,
) -> Result<(), String> {
    let old = doc.clone();
    let (di, target, ctx) = lane_target(doc, diagram_id, element)?;
    let diagram = &doc.diagrams[di];
    let lanes = shown_lanes(doc, diagram, &ctx);
    let s = diagram.shapes[target].bounds.clone();
    let horizontal = diagram.shapes[target].is_horizontal;
    let axes = lane_axes(&diagram.shapes[target]);
    let stack = axes.0;
    let is_root = ctx.lane.is_none();
    let mut created = Vec::new();
    if is_root && !lanes.iter().any(|(l, _)| l.parent.is_none()) {
        created.push((
            None,
            lane_bounds(&s, axes, lo(&s, stack), len(&s, stack), LANE_INDENTATION),
        ));
    }
    // The pool alone grows when it is the target; otherwise everything in the pool but the
    // target lane and its children makes room.
    let candidates: Vec<usize> = if is_root {
        vec![target]
    } else {
        let mut skip = descendants(&lanes, element);
        skip.push(element.clone());
        (0..diagram.shapes.len())
            .filter(|&i| {
                let shape = &diagram.shapes[i];
                !skip.contains(&shape.element)
                    && ctx
                        .root
                        .is_none_or(|r| contains(&diagram.shapes[r].bounds, &mid(&shape.bounds)))
            })
            .collect()
    };
    let above = location == LaneLocation::Above;
    let space = if above {
        Space {
            axis: stack,
            start: lo(&s, stack) + 10.0,
            delta: -LANE_SIZE,
            after: false,
        }
    } else {
        Space {
            axis: stack,
            start: lo(&s, stack) + len(&s, stack) - 10.0,
            delta: LANE_SIZE,
            after: true,
        }
    };
    let mut changes = Changes::new();
    adjust(doc, diagram, &candidates, space, &mut changes);
    let indent = if is_root { LANE_INDENTATION } else { 0.0 };
    let start = if above {
        lo(&s, stack) - LANE_SIZE
    } else {
        lo(&s, stack) + len(&s, stack)
    };
    created.push((
        ctx.lane.as_ref().and_then(|l| l.parent.clone()),
        lane_bounds(&s, axes, start, LANE_SIZE, indent),
    ));
    commit(doc, di, &changes, Some(space));
    create_lanes(doc, di, &ctx.scope, created, horizontal)?;
    sync_lanes(&old, doc, diagram_id);
    Ok(())
}

/// Split a lane or pool into `count` lanes of even height, the last taking the remainder
/// (bpmn-js SplitLaneHandler); existing child lanes are reused in order.
pub fn split_lane(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    element: &ElementSelector,
    count: u8,
) -> Result<(), String> {
    if !(2..=10).contains(&count) {
        return Err("Split into 2 to 10 lanes".into());
    }
    let old = doc.clone();
    let (di, target, ctx) = lane_target(doc, diagram_id, element)?;
    let diagram = &doc.diagrams[di];
    let parent = ctx.lane.as_ref().map(|_| element.clone());
    let mut children: Vec<(Lane, usize)> = shown_lanes(doc, diagram, &ctx)
        .into_iter()
        .filter(|(l, _)| l.parent == parent)
        .collect();
    let axes = lane_axes(&diagram.shapes[target]);
    let stack = axes.0;
    children.sort_by(|a, b| {
        lo(&diagram.shapes[a.1].bounds, stack).total_cmp(&lo(&diagram.shapes[b.1].bounds, stack))
    });
    if children.len() > usize::from(count) {
        return Err(format!("It already has more than {count} lanes"));
    }
    let s = diagram.shapes[target].bounds.clone();
    let horizontal = diagram.shapes[target].is_horizontal;
    let size = (len(&s, stack) / f64::from(count)).round();
    let mut changes = Changes::new();
    let mut created = Vec::new();
    for idx in 0..usize::from(count) {
        let length = if idx + 1 == usize::from(count) {
            len(&s, stack) - size * idx as f64
        } else {
            size
        };
        let bounds = lane_bounds(
            &s,
            axes,
            lo(&s, stack) + idx as f64 * size,
            length,
            LANE_INDENTATION,
        );
        match children.get(idx) {
            Some((_, i)) => {
                changes.insert(*i, bounds);
            }
            None => created.push((parent.clone(), bounds)),
        }
    }
    commit(doc, di, &changes, None);
    create_lanes(doc, di, &ctx.scope, created, horizontal)?;
    sync_lanes(&old, doc, diagram_id);
    Ok(())
}

/// Lanes and the pool that follow a lane or pool resized to `new` (bpmn-js computeLanesResize):
/// every other lane takes the same change across the lanes (left/right for horizontal lanes),
/// and an edge shared with the resized one (within 10, or 5 for the same edge) moves with it.
pub(crate) fn lanes_resize(
    doc: &BpmnDocument,
    diagram: &Diagram,
    target: usize,
    new: &Bounds,
) -> Vec<(usize, Bounds)> {
    let shape = &diagram.shapes[target];
    let Ok(ctx) = lane_context(doc, diagram, &shape.element) else {
        return Vec::new();
    };
    let horizontal = shape.is_horizontal != Some(false);
    let trbl = |b: &Bounds| (b.y, b.x + b.width, b.y + b.height, b.x);
    let (ot, or, ob, ol) = trbl(&shape.bounds);
    let (nt, nr, nb, nl) = trbl(new);
    let (dt, dr, db, dl) = (nt - ot, nr - or, nb - ob, nl - ol);
    let others = shown_lanes(doc, diagram, &ctx)
        .into_iter()
        .map(|(_, i)| i)
        .chain(ctx.root)
        .filter(|&i| i != target);
    let mut out = Vec::new();
    for i in others {
        let (t0, r0, b0, l0) = trbl(&diagram.shapes[i].bounds);
        let (mut t, mut r, mut b, mut l) = if horizontal {
            (0.0, dr, 0.0, dl)
        } else {
            (dt, 0.0, db, 0.0)
        };
        if dt != 0.0 {
            if (b0 - ot).abs() < 10.0 {
                b = nt - b0;
            }
            if (t0 - ot).abs() < 5.0 {
                t = nt - t0;
            }
        }
        if dl != 0.0 {
            if (r0 - ol).abs() < 10.0 {
                r = nl - r0;
            }
            if (l0 - ol).abs() < 5.0 {
                l = nl - l0;
            }
        }
        if db != 0.0 {
            if (t0 - ob).abs() < 10.0 {
                t = nb - t0;
            }
            if (b0 - ob).abs() < 5.0 {
                b = nb - b0;
            }
        }
        if dr != 0.0 {
            if (l0 - or).abs() < 10.0 {
                l = nr - l0;
            }
            if (r0 - or).abs() < 5.0 {
                r = nr - r0;
            }
        }
        if t != 0.0 || r != 0.0 || b != 0.0 || l != 0.0 {
            let (top, right, bottom, left) = (t0 + t, r0 + r, b0 + b, l0 + l);
            out.push((
                i,
                Bounds {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                },
            ));
        }
    }
    out
}

/// Before lane `id` is deleted, its sibling lanes take its space in every diagram showing it
/// (bpmn-js DeleteLaneBehavior): all of it for the sibling at an edge, half from each side when
/// it sits between two. Returns the diagrams whose lane membership must be synchronized.
pub(crate) fn compensate_lane_delete(
    doc: &mut BpmnDocument,
    id: &ElementSelector,
) -> Vec<ElementSelector> {
    let mut touched = Vec::new();
    for di in 0..doc.diagrams.len() {
        let diagram = &doc.diagrams[di];
        let Ok(target) = shape_index(diagram, id) else {
            continue;
        };
        let Ok(ctx) = lane_context(doc, diagram, id) else {
            continue;
        };
        let Some(lane) = &ctx.lane else {
            continue;
        };
        let lanes = shown_lanes(doc, diagram, &ctx);
        let s = diagram.shapes[target].bounds.clone();
        let axis = if diagram.shapes[target].is_horizontal == Some(false) {
            Axis::X
        } else {
            Axis::Y
        };
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for (sibling, i) in lanes
            .iter()
            .filter(|(l, _)| l.parent == lane.parent && l.id != *id)
        {
            let nested = descendants(&lanes, &sibling.id.selector());
            for j in std::iter::once(*i).chain(
                lanes
                    .iter()
                    .filter(|(l, _)| nested.contains(&l.id.selector()))
                    .map(|(_, j)| *j),
            ) {
                if lo(&diagram.shapes[j].bounds, axis) > lo(&s, axis) {
                    after.push(j);
                } else {
                    before.push(j);
                }
            }
        }
        if before.is_empty() && after.is_empty() {
            continue;
        }
        let offset = if !before.is_empty() && !after.is_empty() {
            len(&s, axis) / 2.0
        } else {
            len(&s, axis)
        };
        let mut changes = Changes::new();
        if !before.is_empty() {
            let space = Space {
                axis,
                start: lo(&s, axis) - 10.0,
                delta: offset,
                after: true,
            };
            adjust(doc, diagram, &before, space, &mut changes);
        }
        if !after.is_empty() {
            let space = Space {
                axis,
                start: lo(&s, axis) + len(&s, axis) + 10.0,
                delta: -offset,
                after: false,
            };
            adjust(doc, diagram, &after, space, &mut changes);
        }
        touched.push(diagram.id.selector());
        commit(doc, di, &changes, None);
    }
    touched
}

/// One auto-resize step (diagram-js AutoResize with the bpmn-js provider): the pools and
/// expanded subprocesses holding `elements` in `diagram_id` whose border the elements come
/// within 2 (top/bottom) or 15 (left/right) of, or pass, with the bounds that put the border
/// 60 / 100 beyond them. Lanes and pools never trigger; plane diagrams are not resized.
/// Returns `(container element, shape, bounds)`.
pub(crate) fn auto_resize_step(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    elements: &[ElementSelector],
) -> Vec<(ElementSelector, ElementSelector, Bounds)> {
    let Some(diagram) = doc.diagrams.iter().find(|d| d.id == *diagram_id) else {
        return Vec::new();
    };
    let mut groups: Vec<(usize, Vec<Bounds>)> = Vec::new();
    for e in elements {
        let Some(shape) = diagram.shapes.iter().find(|s| s.element == *e) else {
            continue;
        };
        let Some(scope) = rules::locate(doc, e).and_then(|l| l.node.and(l.scope)) else {
            continue;
        };
        let container = diagram
            .shapes
            .iter()
            .position(|s| {
                s.element == scope
                    && s.is_expanded == Some(true)
                    && queries::find_node(doc, &scope).is_some()
            })
            .or_else(|| {
                diagram.shapes.iter().position(|s| {
                    doc.collaborations()
                        .flat_map(|c| &c.participants)
                        .any(|p| p.id == s.element && local(&p.process) == Some(&scope))
                })
            });
        let Some(container) = container else {
            continue;
        };
        match groups.iter_mut().find(|(c, _)| *c == container) {
            Some((_, all)) => all.push(shape.bounds.clone()),
            None => groups.push((container, vec![shape.bounds.clone()])),
        }
    }
    groups
        .into_iter()
        .filter_map(|(c, all)| {
            let e = union(all)?;
            let t = &diagram.shapes[c].bounds;
            let (mut top, mut right, mut bottom, mut left) =
                (t.y, t.x + t.width, t.y + t.height, t.x);
            if e.y - t.y < 2.0 {
                top = e.y - 60.0;
            }
            if e.x - t.x < 15.0 {
                left = e.x - 100.0;
            }
            if t.x + t.width - (e.x + e.width) < 15.0 {
                right = e.x + e.width + 100.0;
            }
            if t.y + t.height - (e.y + e.height) < 2.0 {
                bottom = e.y + e.height + 60.0;
            }
            let bounds = Bounds {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            };
            (bounds != *t).then(|| {
                let shape = &diagram.shapes[c];
                (shape.element.clone(), shape.id.selector(), bounds)
            })
        })
        .collect()
}

/// IDs owned by a subprocess's content at any depth: nodes, lanes, flows, data associations,
/// groups, and activity data ports.
fn content_ids(content: &FlowContainer, out: &mut Vec<ElementSelector>, depth: usize) {
    if depth > 32 {
        return;
    }
    for n in &content.nodes {
        out.push(n.id.selector());
        if let Some(io) = &n.io_specification {
            out.extend(
                io.inputs
                    .iter()
                    .chain(&io.outputs)
                    .filter_map(|p| p.id.as_ref().map(|id| id.selector())),
            );
        }
        if let Some(NodeDetails::SubProcess { content }) = &n.details {
            content_ids(content, out, depth + 1);
        }
    }
    out.extend(content.lanes.iter().map(|l| l.id.selector()));
    out.extend(content.flows.iter().map(|f| f.id.selector()));
    out.extend(content.groups.iter().map(|g| g.id.selector()));
    out.extend(
        content
            .data_associations
            .iter()
            .filter_map(|a| a.id.as_ref().map(|id| id.selector())),
    );
}
fn translate_edge(edge: &mut Edge, dx: f64, dy: f64) {
    for p in &mut edge.waypoints {
        (p.x, p.y) = (p.x + dx, p.y + dy);
    }
    if let Some(b) = edge.label.as_mut().and_then(|l| l.bounds.as_mut()) {
        (b.x, b.y) = (b.x + dx, b.y + dy);
    }
}
fn translate_shape(shape: &mut Shape, dx: f64, dy: f64) {
    let b = Bounds {
        x: shape.bounds.x + dx,
        y: shape.bounds.y + dy,
        ..shape.bounds.clone()
    };
    crate::commands::move_shape(shape, b);
}
/// Resize shape `target` of diagram `di` to `new`, moving its boundary events with the outline
/// and re-docking its edges.
fn resize_with_attachers(doc: &mut BpmnDocument, di: usize, target: usize, new: Bounds) {
    let diagram = &doc.diagrams[di];
    let (old, host) = (
        diagram.shapes[target].bounds.clone(),
        diagram.shapes[target].element.clone(),
    );
    let ratio = |n: f64, o: f64| if o == 0.0 { 1.0 } else { n / o };
    let hosts = hosts(doc);
    let mut changes = Changes::from([(target, new.clone())]);
    for (i, s) in diagram.shapes.iter().enumerate() {
        if hosts.get(&s.element) == Some(&host) {
            let c = mid(&s.bounds);
            let x = new.x + (c.x - old.x) * ratio(new.width, old.width);
            let y = new.y + (c.y - old.y) * ratio(new.height, old.height);
            changes.insert(
                i,
                Bounds {
                    x: x - s.bounds.width / 2.0,
                    y: y - s.bounds.height / 2.0,
                    ..s.bounds.clone()
                },
            );
        }
    }
    commit(doc, di, &changes, None);
}

/// Collapse an expanded subprocess or expand a collapsed one (bpmn-js ToggleShapeCollapse with
/// the subprocess plane behavior). Collapsing shrinks it to 100x80 around its centre and moves
/// its content's DI into the subprocess's own diagram (created if needed, content placed at
/// 180,160); expanding moves that content back, centred on the subprocess, and sizes it to the
/// content and itself, at least 350x200. Returns whether it is now expanded.
pub fn toggle_collapse(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    id: &ElementSelector,
) -> Result<bool, String> {
    let mut ids = Vec::new();
    match queries::find_node(doc, id).and_then(|n| n.details.as_ref()) {
        Some(NodeDetails::SubProcess { content }) => content_ids(content, &mut ids, 0),
        _ => return Err("Only subprocesses collapse and expand".into()),
    }
    let di = diagram_index(doc, diagram_id)?;
    let target = shape_index(&doc.diagrams[di], id)?;
    if doc.diagrams[di].shapes[target].is_expanded == Some(true) {
        let diagram = &mut doc.diagrams[di];
        let (mut shapes, mut edges) = (Vec::new(), Vec::new());
        for s in std::mem::take(&mut diagram.shapes) {
            if ids.contains(&s.element) {
                shapes.push(s);
            } else {
                diagram.shapes.push(s);
            }
        }
        for e in std::mem::take(&mut diagram.edges) {
            if ids.contains(&e.element) {
                edges.push(e);
            } else {
                diagram.edges.push(e);
            }
        }
        let moved: Vec<ElementSelector> = shapes
            .iter()
            .map(|s| s.id.selector())
            .chain(edges.iter().map(|e| e.id.selector()))
            .collect();
        if let Some(order) = &mut diagram.plane_order {
            order.retain(|o| !moved.contains(o));
        }
        if let Some(bbox) = union(shapes.iter().map(|s| s.bounds.clone())) {
            let (dx, dy) = (180.0 - bbox.x, 160.0 - bbox.y);
            for s in &mut shapes {
                translate_shape(s, dx, dy);
            }
            for e in &mut edges {
                translate_edge(e, dx, dy);
            }
        }
        // The collapsed subprocess gets its own diagram, empty when it has no content yet
        // (bpmn-js SubProcessPlaneBehavior).
        match doc.diagrams.iter_mut().find(|d| d.plane == *id) {
            Some(plane) => {
                if let Some(order) = &mut plane.plane_order {
                    order.extend(moved);
                }
                plane.shapes.extend(shapes);
                plane.edges.extend(edges);
            }
            None => doc.diagrams.push(Diagram {
                plane_metadata: None,
                plane_order: None,
                name: None,
                documentation: None,
                resolution: None,
                label_styles: Vec::new(),
                id: fresh("BPMNDiagram"),
                plane_id: fresh("BPMNPlane"),
                plane: id.clone(),
                shapes,
                edges,
            }),
        }
        let di = diagram_index(doc, diagram_id)?;
        let target = shape_index(&doc.diagrams[di], id)?;
        let s = doc.diagrams[di].shapes[target].bounds.clone();
        let new = Bounds {
            x: s.x + (s.width - 100.0) / 2.0,
            y: s.y + (s.height - 80.0) / 2.0,
            width: 100.0,
            height: 80.0,
        };
        doc.diagrams[di].shapes[target].is_expanded = Some(false);
        resize_with_attachers(doc, di, target, new);
        return Ok(false);
    }
    let centre = mid(&doc.diagrams[di].shapes[target].bounds);
    if let Some(pi) = doc
        .diagrams
        .iter()
        .position(|d| d.plane == *id && d.id != *diagram_id)
    {
        let plane = doc.diagrams.remove(pi);
        let (mut shapes, mut edges) = (plane.shapes, plane.edges);
        if let Some(bbox) = union(shapes.iter().map(|s| s.bounds.clone())) {
            let c = mid(&bbox);
            let (dx, dy) = (centre.x - c.x, centre.y - c.y);
            for s in &mut shapes {
                translate_shape(s, dx, dy);
            }
            for e in &mut edges {
                translate_edge(e, dx, dy);
            }
        }
        let di = diagram_index(doc, diagram_id)?;
        let diagram = &mut doc.diagrams[di];
        if let Some(order) = &mut diagram.plane_order {
            order.extend(shapes.iter().map(|s| s.id.selector()));
            order.extend(edges.iter().map(|e| e.id.selector()));
        }
        diagram.shapes.extend(shapes);
        diagram.edges.extend(edges);
    }
    let di = diagram_index(doc, diagram_id)?;
    let diagram = &doc.diagrams[di];
    let target = shape_index(diagram, id)?;
    let visible = union(
        diagram
            .shapes
            .iter()
            .filter(|s| ids.contains(&s.element))
            .chain([&diagram.shapes[target]])
            .map(|s| s.bounds.clone()),
    )
    .unwrap_or_else(|| diagram.shapes[target].bounds.clone());
    let (width, height) = (visible.width.max(350.0), visible.height.max(200.0));
    let new = Bounds {
        x: visible.x + (visible.width - width) / 2.0,
        y: visible.y + (visible.height - height) / 2.0,
        width,
        height,
    };
    doc.diagrams[di].shapes[target].is_expanded = Some(true);
    resize_with_attachers(doc, di, target, new);
    Ok(true)
}

/// Whether replacing `id` with `target` only collapses or expands it (the replace menu's
/// collapsed/expanded subprocess entries); returns the diagram showing it.
pub fn collapse_toggle_target(
    doc: &BpmnDocument,
    id: &ElementSelector,
    target: &crate::replace::ReplaceTarget,
) -> Option<ElementSelector> {
    let crate::replace::ReplaceTarget::Node {
        kind,
        trigger: None,
        non_interrupting: false,
        event_sub_process,
        collapsed,
    } = *target
    else {
        return None;
    };
    let node = queries::find_node(doc, id)?;
    let (diagram, shape) = doc
        .diagrams
        .iter()
        .find_map(|d| Some((d, d.shapes.iter().find(|s| s.element == *id)?)))?;
    let expanded = shape.is_expanded == Some(true);
    (node.kind == kind
        && matches!(node.details, Some(NodeDetails::SubProcess { .. }))
        && rules::is_event_subprocess(node) == event_sub_process
        && collapsed == expanded)
        .then(|| diagram.id.selector())
}

/// Insert or remove space (diagram-js space tool): every shape of the diagram wholly past the
/// line at `start` on `axis` (before it with `after` false) moves by `delta`, resizable shapes
/// crossing the line grow or shrink, boundary events follow their hosts, edges between affected
/// shapes keep their points before the line, and lane membership follows the new geometry.
/// Refused when a container would shrink below its minimum size. `semantic` allows the lane
/// membership update (a diagram-only edit otherwise). When the drag starts at `at` inside a
/// pool, only that pool and the shapes in it take part (whole diagram otherwise).
#[allow(clippy::too_many_arguments)]
pub fn make_space(
    doc: &mut BpmnDocument,
    diagram_id: &ElementSelector,
    axis: Axis,
    start: f64,
    delta: f64,
    after: bool,
    semantic: bool,
    at: Option<Point>,
) -> Result<(), String> {
    if at.as_ref().is_some_and(|p| !geometry::valid_point(p)) {
        return Err("invalid position".into());
    }
    if !start.is_finite()
        || !delta.is_finite()
        || start.abs() > geometry::COORDINATE_LIMIT
        || delta.abs() > geometry::COORDINATE_LIMIT
    {
        return Err("invalid space".into());
    }
    if delta == 0.0 {
        return Ok(());
    }
    let old = doc.clone();
    let di = diagram_index(doc, diagram_id)?;
    let diagram = &doc.diagrams[di];
    let space = Space {
        axis,
        start,
        delta,
        after,
    };
    let mut changes = Changes::new();
    // bpmn-js: a drag that starts in a pool only makes space in that pool.
    let pool = at.and_then(|p| {
        diagram.shapes.iter().find(|s| {
            contains(&s.bounds, &p)
                && doc
                    .collaborations()
                    .flat_map(|c| &c.participants)
                    .any(|x| x.id == s.element)
        })
    });
    let all: Vec<usize> = (0..diagram.shapes.len())
        .filter(|&i| {
            pool.is_none_or(|pool| {
                diagram.shapes[i].id == pool.id
                    || contains(&pool.bounds, &mid(&diagram.shapes[i].bounds))
            })
        })
        .collect();
    adjust(doc, diagram, &all, space, &mut changes);
    for (i, b) in &changes {
        let shape = &diagram.shapes[*i];
        if !geometry::valid_bounds(b) {
            return Err("The space would move shapes out of range".into());
        }
        if let Some(rule) = crate::manipulate::resize_rule(doc, shape)
            && (b.width < rule.width.min(shape.bounds.width)
                || b.height < rule.height.min(shape.bounds.height))
        {
            return Err(format!(
                "Removing this much space would make a container smaller than {} x {}",
                rule.width, rule.height
            ));
        }
    }
    commit(doc, di, &changes, Some(space));
    if semantic {
        sync_lanes(&old, doc, diagram_id);
    }
    Ok(())
}

/// Create a pool in `diagram_id` (bpmn-js CreateParticipantBehavior). In a process diagram the
/// diagram becomes a collaboration diagram and the pool takes the process; when the process
/// already has shapes the pool wraps them (20 padding, 30 header, at least 600x250). In a
/// collaboration diagram the pool gets a new empty process and goes to `center` (default: below
/// the diagram's shapes).
pub fn create_pool(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    name: Option<String>,
    center: Option<Point>,
) -> Result<Vec<Mutation>, String> {
    if center.as_ref().is_some_and(|c| !geometry::valid_point(c)) {
        return Err("invalid position".into());
    }
    let diagram = doc
        .diagrams
        .iter()
        .find(|d| d.id == *diagram_id)
        .ok_or("diagram not found")?;
    let plane_collaboration = doc.collaborations().find(|c| c.id == diagram.plane);
    let plane_process = doc.processes.iter().find(|p| p.id == diagram.plane);
    if plane_collaboration.is_none() && plane_process.is_none() {
        return Err("Pools are created in a process or collaboration diagram".into());
    }
    let referenced = |p: &Process| {
        doc.collaborations()
            .flat_map(|c| &c.participants)
            .any(|x| local(&x.process) == Some(&p.id.selector()))
    };
    let mut batch = Vec::new();
    let collaboration_id = plane_collaboration
        .or(doc.collaboration.as_ref())
        .map_or_else(|| fresh("Collaboration"), |c| c.id.clone());
    if plane_collaboration.is_none() && doc.collaboration.is_none() {
        batch.push(Mutation::CreateCollaboration {
            document_id: doc.id,
            collaboration: Collaboration {
                kind: None,
                extras: None,
                correlation_keys: Vec::new(),
                base: None,
                name: None,
                id: collaboration_id.clone(),
                participants: Vec::new(),
                flows: Vec::new(),
                conversation_nodes: Vec::new(),
                conversation_links: Vec::new(),
            },
        });
    }
    if plane_collaboration.is_none() {
        batch.push(Mutation::SetDiagramPlane {
            document_id: doc.id,
            diagram_id: diagram_id.clone(),
            plane: collaboration_id.selector(),
        });
    }
    let wrapped = plane_process.filter(|p| !referenced(p));
    let process = match wrapped {
        Some(p) => p.id.selector(),
        None => {
            let id = fresh("Process");
            batch.push(Mutation::CreateProcess {
                document_id: doc.id,
                process: Process {
                    activity_metadata: None,
                    is_executable: None,
                    io_specification: None,
                    lane_set_id: None,
                    lane_set_base: None,
                    lane_set_name: None,
                    groups: Vec::new(),
                    data_objects: Vec::new(),
                    data_associations: Vec::new(),
                    id: id.clone(),
                    name: None,
                    documentation: String::new().into(),
                    base_extras: None,
                    nodes: Vec::new(),
                    lanes: Vec::new(),
                    flows: Vec::new(),
                },
            });
            id.selector()
        }
    };
    // Content of the wrapped process shown here (groups excluded, as in bpmn-js).
    let content = wrapped.and_then(|p| {
        let owned = |e: &ElementSelector| {
            rules::locate(doc, e).is_some_and(|l| {
                l.node.is_some() && l.organisation.as_ref() == Some(&p.id.selector())
            }) || p.lanes.iter().any(|l| l.id == *e)
        };
        union(
            diagram
                .shapes
                .iter()
                .filter(|s| owned(&s.element))
                .map(|s| s.bounds.clone()),
        )
    });
    let bounds = match content {
        Some(c) => {
            let (w, h) = (
                (c.width + 2.0 * 20.0 + LANE_INDENTATION).max(600.0),
                (c.height + 2.0 * 20.0).max(250.0),
            );
            // Centre the pool body (right of the header) on the content.
            let body = mid(&c);
            Bounds {
                x: body.x - (w - LANE_INDENTATION) / 2.0 - LANE_INDENTATION,
                y: body.y - h / 2.0,
                width: w,
                height: h,
            }
        }
        None => {
            let at = center.unwrap_or_else(|| {
                union(diagram.shapes.iter().map(|s| s.bounds.clone())).map_or(
                    Point { x: 300.0, y: 125.0 },
                    |all| Point {
                        x: all.x + 300.0,
                        y: all.y + all.height + 30.0 + 125.0,
                    },
                )
            });
            Bounds {
                x: at.x - 300.0,
                y: at.y - 125.0,
                width: 600.0,
                height: 250.0,
            }
        }
    };
    let participant = Participant {
        interface_refs: Vec::new(),
        endpoint_refs: Vec::new(),
        multiplicity: None,
        base: None,
        id: fresh("Participant"),
        name,
        process: Some(ModelReference::Local(process)),
    };
    let element = participant.id.selector();
    match plane_collaboration {
        Some(root) => {
            let mut root = root.clone();
            root.participants.push(participant);
            batch.push(Mutation::SetCollaborationRoot {
                document_id: doc.id,
                collaboration: root,
            });
        }
        None => batch.push(Mutation::CreateParticipant {
            document_id: doc.id,
            participant,
        }),
    }
    batch.push(Mutation::CreateShape {
        document_id: doc.id,
        diagram_id: diagram_id.clone(),
        shape: container_shape(fresh("Participant_di"), element, bounds, None),
    });
    Ok(batch)
}
