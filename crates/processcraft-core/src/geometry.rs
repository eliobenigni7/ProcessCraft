//! Bounded, toolkit-independent geometry helpers. Routing is always explicit.
use crate::{
    commands::{Mutation, Request, ShapeBounds},
    ids::{DocumentId, XmlId},
    model::{Bounds, NodeKind, Point},
};

pub const COORDINATE_LIMIT: f64 = 1_000_000_000.0;
const MAX_OBSTACLES: usize = 256;

fn coordinate(v: f64) -> bool {
    v.is_finite() && v.abs() <= COORDINATE_LIMIT
}
pub fn valid_point(p: &Point) -> bool {
    coordinate(p.x) && coordinate(p.y)
}
pub fn valid_bounds(b: &Bounds) -> bool {
    coordinate(b.x)
        && coordinate(b.y)
        && coordinate(b.width)
        && coordinate(b.height)
        && b.width >= 0.0
        && b.height >= 0.0
        && coordinate(b.x + b.width)
        && coordinate(b.y + b.height)
}

/// Hit testing in diagram coordinates; tolerance is in screen pixels and zoom is screen pixels per diagram unit.
pub fn hit_shape(
    kind: NodeKind,
    bounds: &Bounds,
    point: &Point,
    screen_tolerance: f64,
    zoom: f64,
) -> Result<bool, &'static str> {
    if !valid_bounds(bounds)
        || !valid_point(point)
        || !screen_tolerance.is_finite()
        || screen_tolerance < 0.0
        || !zoom.is_finite()
        || zoom <= 0.0
    {
        return Err("invalid geometry or zoom");
    }
    let t = screen_tolerance / zoom;
    if !t.is_finite() {
        return Err("invalid tolerance");
    }
    let cx = bounds.x + bounds.width / 2.0;
    let cy = bounds.y + bounds.height / 2.0;
    let rx = bounds.width / 2.0 + t;
    let ry = bounds.height / 2.0 + t;
    let dx = (point.x - cx).abs();
    let dy = (point.y - cy).abs();
    if rx == 0.0 || ry == 0.0 {
        return Ok(dx <= rx && dy <= ry);
    }
    Ok(match kind {
        NodeKind::StartEvent
        | NodeKind::EndEvent
        | NodeKind::IntermediateCatchEvent
        | NodeKind::IntermediateThrowEvent
        | NodeKind::BoundaryEvent => (dx / rx).powi(2) + (dy / ry).powi(2) <= 1.0,
        NodeKind::ExclusiveGateway
        | NodeKind::ParallelGateway
        | NodeKind::InclusiveGateway
        | NodeKind::ComplexGateway
        | NodeKind::EventBasedGateway => dx / rx + dy / ry <= 1.0,
        _ => dx <= rx && dy <= ry,
    })
}

#[derive(Clone, Debug)]
pub struct RouteResult {
    pub waypoints: Option<Vec<Point>>,
    pub warning: Option<String>,
}
fn blocked(a: &Point, b: &Point, r: &Bounds) -> bool {
    let right = r.x + r.width;
    let bottom = r.y + r.height;
    if a.x == b.x {
        a.x > r.x && a.x < right && a.y.max(b.y) > r.y && a.y.min(b.y) < bottom
    } else if a.y == b.y {
        a.y > r.y && a.y < bottom && a.x.max(b.x) > r.x && a.x.min(b.x) < right
    } else {
        true
    }
}
fn clear(path: &[Point], obstacles: &[Bounds]) -> bool {
    path.windows(2).all(|p| {
        (p[0].x == p[1].x || p[0].y == p[1].y)
            && obstacles.iter().all(|r| !blocked(&p[0], &p[1], r))
    })
}
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn compact(path: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for point in path {
        if out.last().is_some_and(|v| v.x == point.x && v.y == point.y) {
            continue;
        }
        while out.len() >= 2 {
            let a = &out[out.len() - 2];
            let b = &out[out.len() - 1];
            if (a.x == b.x && b.x == point.x) || (a.y == b.y && b.y == point.y) {
                out.pop();
            } else {
                break;
            }
        }
        out.push(point);
    }
    if out.len() == 1 {
        out.push(out[0].clone());
    }
    out
}
/// Searches a bounded set of orthogonal corridors. A failure is explicit; no unsafe fallback is returned.
/// The caller must exclude the source and destination shapes from `obstacles`.
pub fn route(
    start: Point,
    end: Point,
    obstacles: &[Bounds],
    clearance: f64,
    budget: usize,
) -> Result<RouteResult, &'static str> {
    if !valid_point(&start)
        || !valid_point(&end)
        || !clearance.is_finite()
        || clearance < 0.0
        || !coordinate(clearance)
        || obstacles.iter().any(|r| !valid_bounds(r))
    {
        return Err("invalid geometry");
    }
    if obstacles.len() > MAX_OBSTACLES {
        return Ok(RouteResult {
            waypoints: None,
            warning: Some("routing obstacle limit exceeded".into()),
        });
    }
    let inflated: Vec<_> = obstacles
        .iter()
        .map(|r| Bounds {
            x: r.x - clearance,
            y: r.y - clearance,
            width: r.width + 2.0 * clearance,
            height: r.height + 2.0 * clearance,
        })
        .collect();
    if inflated.iter().any(|r| !valid_bounds(r)) {
        return Err("inflated obstacle exceeds coordinate limit");
    }
    let mut xs = vec![start.x, end.x];
    let mut ys = vec![start.y, end.y];
    for r in &inflated {
        xs.extend([r.x, r.x + r.width]);
        ys.extend([r.y, r.y + r.height]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let mut best: Option<(f64, Vec<Point>)> = None;
    let mut examined = 0usize;
    for x in xs {
        if examined >= budget {
            return Ok(RouteResult {
                waypoints: None,
                warning: Some("routing search budget exceeded".into()),
            });
        }
        examined += 1;
        let path = compact(vec![start.clone(), p(x, start.y), p(x, end.y), end.clone()]);
        if clear(&path, &inflated) {
            let length = path
                .windows(2)
                .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
                .sum();
            if best.as_ref().is_none_or(|(v, _)| length < *v) {
                best = Some((length, path));
            }
        }
    }
    for y in ys {
        if examined >= budget {
            return Ok(RouteResult {
                waypoints: None,
                warning: Some("routing search budget exceeded".into()),
            });
        }
        examined += 1;
        let path = compact(vec![start.clone(), p(start.x, y), p(end.x, y), end.clone()]);
        if clear(&path, &inflated) {
            let length = path
                .windows(2)
                .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
                .sum();
            if best.as_ref().is_none_or(|(v, _)| length < *v) {
                best = Some((length, path));
            }
        }
    }
    Ok(match best {
        Some((_, waypoints)) => RouteResult {
            waypoints: Some(waypoints),
            warning: None,
        },
        None => RouteResult {
            waypoints: None,
            warning: Some("no clear route found; explicit reroute required".into()),
        },
    })
}
/// Repair adjacent segments while retaining all manual interior points. Caller supplies moved endpoints.
/// The caller must exclude the source and destination shapes from `obstacles`.
pub fn repair_endpoints(
    points: &[Point],
    start: Option<Point>,
    end: Option<Point>,
    obstacles: &[Bounds],
) -> Result<RouteResult, &'static str> {
    if points.len() < 2
        || points.iter().any(|p| !valid_point(p))
        || start.as_ref().is_some_and(|p| !valid_point(p))
        || end.as_ref().is_some_and(|p| !valid_point(p))
        || obstacles.iter().any(|r| !valid_bounds(r))
    {
        return Err("invalid geometry");
    }
    let start_moved = start.is_some();
    let end_moved = end.is_some();
    let mut path = Vec::new();
    let first = start.unwrap_or_else(|| points[0].clone());
    let last = end.unwrap_or_else(|| points[points.len() - 1].clone());
    path.push(first.clone());
    if points.len() == 2 {
        if (start_moved || end_moved) && first.x != last.x && first.y != last.y {
            path.push(p(last.x, first.y));
        }
    } else {
        let next = &points[1];
        if start_moved && first.x != next.x && first.y != next.y {
            path.push(if points[0].y == next.y {
                p(next.x, first.y)
            } else {
                p(first.x, next.y)
            });
        }
        path.extend_from_slice(&points[1..points.len() - 1]);
        let prev = &points[points.len() - 2];
        if end_moved && prev.x != last.x && prev.y != last.y {
            path.push(if prev.y == points[points.len() - 1].y {
                p(prev.x, last.y)
            } else {
                p(last.x, prev.y)
            });
        }
    }
    path.push(last);
    let warning = if clear(&path, obstacles) {
        None
    } else {
        Some("route intersects an obstacle or is not orthogonal; explicit reroute required".into())
    };
    Ok(RouteResult {
        waypoints: Some(path),
        warning,
    })
}
/// One batch represents one drag gesture and therefore one undo step.
pub fn move_multiple(
    document_id: DocumentId,
    bounds: Vec<ShapeBounds>,
    routes: Vec<(XmlId, XmlId, Vec<Point>)>,
) -> Result<Request, &'static str> {
    if bounds.iter().any(|b| !valid_bounds(&b.bounds))
        || routes
            .iter()
            .any(|(_, _, p)| p.len() < 2 || p.iter().any(|v| !valid_point(v)))
    {
        return Err("invalid geometry");
    }
    let mut mutations = Vec::new();
    for b in bounds {
        mutations.push(Mutation::MoveShape {
            document_id,
            diagram_id: b.diagram_id,
            shape_id: b.shape_id,
            bounds: b.bounds,
        });
    }
    for (diagram_id, edge_id, waypoints) in routes {
        mutations.push(Mutation::Reroute {
            document_id,
            diagram_id,
            edge_id,
            waypoints,
        });
    }
    if mutations.len() > 1000 {
        return Err("batch exceeds 1000 mutations");
    }
    Ok(Request::Batch { mutations })
}

/// A move batch and warnings which the caller should display to the user.
pub struct PreparedMove {
    pub request: Request,
    pub warnings: Vec<String>,
}

/// Prepare a gesture without changing the document. Connected manual routes retain their interior points.
/// Shapes in `bounds` must belong to the selected diagram and appear only once.
pub fn prepare_move(
    document: &crate::model::BpmnDocument,
    diagram_id: &XmlId,
    mut bounds: Vec<ShapeBounds>,
) -> Result<PreparedMove, &'static str> {
    let diagram = document
        .diagrams
        .iter()
        .find(|d| &d.id == diagram_id)
        .ok_or("diagram not found")?;
    // Include visible descendant DI for moved subprocesses, without following a different diagram.
    // Conflicting explicit parent/child deltas are ambiguous and must not be partially applied.
    let requested = bounds.clone();
    let mut nested_translated = std::collections::BTreeSet::new();
    for parent in &requested {
        let parent_shape = diagram
            .shapes
            .iter()
            .find(|s| s.id == parent.shape_id)
            .ok_or("shape not found")?;
        let Some(node) = crate::queries::find_node(document, &parent_shape.element) else {
            continue;
        };
        let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details else {
            continue;
        };
        let (dx, dy) = (
            parent.bounds.x - parent_shape.bounds.x,
            parent.bounds.y - parent_shape.bounds.y,
        );
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        let mut pending = vec![(content.as_ref(), 1usize)];
        while let Some((scope, depth)) = pending.pop() {
            if depth > 32 {
                return Err("nested subprocess exceeds depth limit");
            }
            let ids = scope
                .nodes
                .iter()
                .map(|n| &n.id)
                .chain(scope.lanes.iter().map(|l| &l.id));
            for id in ids {
                for shape in diagram.shapes.iter().filter(|shape| &shape.element == id) {
                    nested_translated.insert(shape.element.clone());
                    let mut translated = shape.bounds.clone();
                    translated.x += dx;
                    translated.y += dy;
                    if let Some(existing) = bounds.iter().find(|b| b.shape_id == shape.id) {
                        if existing.bounds.x != translated.x
                            || existing.bounds.y != translated.y
                            || existing.bounds.width != translated.width
                            || existing.bounds.height != translated.height
                        {
                            return Err("conflicting parent and child moves");
                        }
                    } else {
                        bounds.push(ShapeBounds {
                            diagram_id: diagram_id.clone(),
                            shape_id: shape.id.clone(),
                            bounds: translated,
                        });
                    }
                }
            }
            for child in &scope.nodes {
                if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &child.details
                {
                    pending.push((content.as_ref(), depth + 1));
                }
            }
        }
    }
    let mut moved = Vec::new();
    for b in &bounds {
        if &b.diagram_id != diagram_id
            || !valid_bounds(&b.bounds)
            || moved
                .iter()
                .any(|(id, _, _, _): &(&XmlId, &XmlId, f64, f64)| *id == &b.shape_id)
        {
            return Err("invalid or duplicate moved shape");
        }
        let shape = diagram
            .shapes
            .iter()
            .find(|s| s.id == b.shape_id)
            .ok_or("shape not found")?;
        if !valid_bounds(&shape.bounds) {
            return Err("invalid existing shape bounds");
        }
        moved.push((
            &b.shape_id,
            &shape.element,
            b.bounds.x - shape.bounds.x,
            b.bounds.y - shape.bounds.y,
        ));
    }
    let scopes = crate::queries::scopes(document);
    // Normalize the qualified binary edge families to their visible endpoints.
    let mut connections: Vec<(&XmlId, &XmlId, &XmlId)> = scopes
        .iter()
        .flat_map(|scope| scope.flows.iter())
        .chain(document.collaboration.iter().flat_map(|c| c.flows.iter()))
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| c.sequence_flows.iter().chain(c.message_flows.iter())),
        )
        .map(|flow| (&flow.id, &flow.source, &flow.target))
        .collect();
    for scope in &scopes {
        for association in scope.data_associations {
            if !diagram
                .edges
                .iter()
                .any(|edge| edge.element == association.id)
            {
                continue;
            }
            let affected = moved.iter().any(|(_, id, dx, dy)| {
                (*dx != 0.0 || *dy != 0.0)
                    && (association.sources.iter().any(|source| source == *id)
                        || &association.target == *id
                        || &association.owner == *id)
            });
            // A data port normally has no DI shape: its activity is the visible endpoint.
            // Multiple sources cannot be represented unambiguously by one DI edge.
            if association.sources.len() != 1 {
                if affected {
                    return Err("moving multi-source data association is unsupported");
                }
                continue;
            }
            let visible = |id: &XmlId| diagram.shapes.iter().any(|shape| &shape.element == id);
            let owner_io = crate::queries::find_node(document, &association.owner)
                .and_then(|node| node.io_specification.as_ref());
            let input_port = owner_io
                .is_some_and(|io| io.inputs.iter().any(|port| port.id == association.target));
            let output_port = owner_io.is_some_and(|io| {
                io.outputs
                    .iter()
                    .any(|port| port.id == association.sources[0])
            });
            if (association.kind == crate::bpmn_data::DataAssociationKind::Input
                && !visible(&association.target)
                && !input_port)
                || (association.kind == crate::bpmn_data::DataAssociationKind::Output
                    && !visible(&association.sources[0])
                    && !output_port)
            {
                if affected {
                    return Err("data association has unresolved activity port");
                }
                continue;
            }
            let (source, target) = match association.kind {
                crate::bpmn_data::DataAssociationKind::Input => (
                    &association.sources[0],
                    if visible(&association.target) {
                        &association.target
                    } else {
                        &association.owner
                    },
                ),
                crate::bpmn_data::DataAssociationKind::Output => (
                    if visible(&association.sources[0]) {
                        &association.sources[0]
                    } else {
                        &association.owner
                    },
                    &association.target,
                ),
            };
            if !visible(source) || !visible(target) {
                if affected {
                    return Err("data association has unresolved visible endpoint");
                }
                continue;
            }
            connections.push((&association.id, source, target));
        }
    }
    if let Some(collaboration) = &document.collaboration {
        connections.extend(
            collaboration
                .conversation_links
                .iter()
                .map(|link| (&link.id, &link.source, &link.target)),
        );
    }
    let mut routes = Vec::new();
    let mut label_moves = Vec::new();
    let mut warnings = Vec::new();
    for edge in &diagram.edges {
        let Some((_, source, target)) = connections
            .iter()
            .find(|(id, _, _)| **id == edge.element)
            .copied()
        else {
            continue;
        };
        let endpoint = |element: &XmlId, point: &Point| -> Result<Option<Point>, &'static str> {
            let matches: Vec<_> = moved
                .iter()
                .filter(|(_, id, _, _)| *id == element)
                .collect();
            if matches.len() > 1 {
                return Err("ambiguous moved endpoint");
            }
            Ok(matches.first().and_then(|(_, _, dx, dy)| {
                if *dx == 0.0 && *dy == 0.0 {
                    None
                } else {
                    Some(p(point.x + dx, point.y + dy))
                }
            }))
        };
        if edge.waypoints.len() < 2 {
            if moved
                .iter()
                .any(|(_, id, _, _)| *id == source || *id == target)
            {
                return Err("connected edge has insufficient waypoints");
            }
            continue;
        }
        let start = endpoint(source, &edge.waypoints[0])?;
        let end = endpoint(target, &edge.waypoints[edge.waypoints.len() - 1])?;
        let source_delta = moved
            .iter()
            .find(|(_, id, _, _)| *id == source)
            .map(|(_, _, dx, dy)| (*dx, *dy));
        let target_delta = moved
            .iter()
            .find(|(_, id, _, _)| *id == target)
            .map(|(_, _, dx, dy)| (*dx, *dy));
        if let (Some((dx, dy)), Some(other)) = (source_delta, target_delta)
            && (dx, dy) == other
            && (dx != 0.0 || dy != 0.0)
            && nested_translated.contains(source)
            && nested_translated.contains(target)
        {
            if let Some(label) = &edge.label {
                let mut bounds = label.bounds.clone();
                bounds.x += dx;
                bounds.y += dy;
                if !valid_bounds(&bounds) {
                    return Err("translated edge label exceeds coordinate limit");
                }
                label_moves.push(Mutation::SetLabelBounds {
                    document_id: document.id,
                    diagram_id: diagram_id.clone(),
                    representation_id: edge.id.clone(),
                    bounds,
                });
            }
            let translated: Vec<_> = edge
                .waypoints
                .iter()
                .map(|point| p(point.x + dx, point.y + dy))
                .collect();
            if translated.iter().any(|point| !valid_point(point)) {
                return Err("translated route exceeds coordinate limit");
            }
            routes.push((diagram_id.clone(), edge.id.clone(), translated));
            continue;
        }
        if start.is_none() && end.is_none() {
            continue;
        }
        let obstacles: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| s.element != *source && s.element != *target)
            // Ignore only containers of an endpoint; unrelated containers remain obstacles.
            .filter(|s| {
                let is_scope_ancestor = |element: &XmlId, target: &XmlId| {
                    let mut current = target;
                    for _ in 0..=32 {
                        let Some(scope) = scopes
                            .iter()
                            .find(|scope| scope.nodes.iter().any(|node| &node.id == current))
                        else {
                            break;
                        };
                        if scope.id == element {
                            return true;
                        }
                        current = scope.id;
                    }
                    false
                };
                let contains =
                    |target: &XmlId| {
                        is_scope_ancestor(&s.element, target)
                            || scopes.iter().any(|scope| {
                                scope.lanes.iter().any(|lane| {
                                    lane.id == s.element && lane.members.contains(target)
                                })
                            })
                            || document.collaboration.as_ref().is_some_and(|c| {
                                c.participants.iter().any(|participant| {
                                    participant.id == s.element
                                        && participant.process.as_ref().is_some_and(|process| {
                                            is_scope_ancestor(process, target)
                                        })
                                })
                            })
                    };
                !contains(source) && !contains(target)
            })
            .map(|s| {
                bounds
                    .iter()
                    .find(|b| b.shape_id == s.id)
                    .map_or_else(|| s.bounds.clone(), |b| b.bounds.clone())
            })
            .collect();
        let repaired = repair_endpoints(&edge.waypoints, start, end, &obstacles)?;
        if let Some(warning) = repaired.warning {
            warnings.push(format!("edge {}: {}", edge.id, warning));
        }
        routes.push((
            diagram_id.clone(),
            edge.id.clone(),
            repaired.waypoints.ok_or("route repair failed")?,
        ));
    }
    let mut request = move_multiple(document.id, bounds, routes)?;
    if let Request::Batch { mutations } = &mut request {
        if mutations.len() + label_moves.len() > 1000 {
            return Err("batch exceeds 1000 mutations");
        }
        mutations.extend(label_moves);
    }
    Ok(PreparedMove { request, warnings })
}
