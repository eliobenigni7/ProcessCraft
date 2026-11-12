//! Bounded, toolkit-independent geometry helpers. Routing is always explicit.
use crate::{
    commands::{Mutation, Request, ShapeBounds},
    ids::{DocumentId, ElementSelector},
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
        NodeKind::DataObjectReference => {
            let fold = bounds.width.min(bounds.height).min(12.0);
            let px = point.x - bounds.x;
            let py = point.y - bounds.y;
            px >= -t
                && py >= -t
                && px <= bounds.width + t
                && py <= bounds.height + t
                && px - py <= bounds.width - fold + t * std::f64::consts::SQRT_2
        }
        NodeKind::DataStoreReference => {
            let px = point.x - bounds.x;
            let py = point.y - bounds.y;
            if px < -t || px > bounds.width + t || py < -t || py > bounds.height + t {
                false
            } else if bounds.width == 0.0 {
                true
            } else {
                // Invert the cubic x coordinate of the cylinder's top and bottom arcs.
                let x = (px / bounds.width).clamp(0.0, 1.0);
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..32 {
                    let m = (lo + hi) / 2.0;
                    if m * m * (3.0 - 2.0 * m) < x {
                        lo = m;
                    } else {
                        hi = m;
                    }
                }
                let u = (lo + hi) / 2.0;
                let r = (bounds.height * 0.12).min(10.0);
                let top = r * (1.0 - 3.99 * u * (1.0 - u));
                py >= top - t && py <= bounds.height - top + t
            }
        }
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
/// Side of a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}
/// Preferred Manhattan layout of a new connection (bpmn-js `BpmnLayouter` behavior).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteLayout {
    /// Facing sides along the dominant axis.
    Auto,
    /// Sequence flow: h:h.
    Sequence,
    /// Out of a gateway: v:h when the target is diagonal.
    FromGateway,
    /// Into a gateway: h:v when the source is diagonal.
    IntoGateway,
    /// Message flow: straight when the shapes overlap horizontally, else v:v.
    Message,
    /// Out of a boundary event: leave from the side it is attached to.
    FromBoundary(Side),
}
/// Route for a new connection between two shapes, shared by every entry point.
/// Equivalent to [`layout_route`] with [`RouteLayout::Auto`].
pub fn connection_route(a: &Bounds, b: &Bounds, shapes: &[Bounds]) -> Vec<Point> {
    layout_route(a, b, shapes, RouteLayout::Auto)
}
/// Route from `a` to `b`: shapes apart only horizontally get h:h, only vertically v:v, and
/// diagonal ones the preferred `layout`. When the preferred route crosses another shape it tries
/// the bounded [`route`] search, and otherwise keeps it (a visible route the user can reroute).
/// Bounds containing either endpoint's centre (pools, lanes, expanded subprocesses) and the
/// endpoints themselves are not obstacles.
pub fn layout_route(a: &Bounds, b: &Bounds, shapes: &[Bounds], layout: RouteLayout) -> Vec<Point> {
    const APART: f64 = 20.0;
    let (ax, ay) = (a.x + a.width / 2.0, a.y + a.height / 2.0);
    let (bx, by) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let horizontal_apart = b.x >= a.x + a.width + APART || b.x + b.width <= a.x - APART;
    let below = b.y >= a.y + a.height + APART;
    let vertical_apart = below || b.y + b.height <= a.y - APART;
    let (forward, down) = (bx >= ax, by >= ay);
    let h_start = p(if forward { a.x + a.width } else { a.x }, ay);
    let h_end = p(if forward { b.x } else { b.x + b.width }, by);
    let v_start = p(ax, if down { a.y + a.height } else { a.y });
    let v_end = p(bx, if down { b.y } else { b.y + b.height });
    let hh = || {
        let mid = (h_start.x + h_end.x) / 2.0;
        compact(vec![h_start.clone(), p(mid, ay), p(mid, by), h_end.clone()])
    };
    let vv = || {
        let mid = (v_start.y + v_end.y) / 2.0;
        compact(vec![v_start.clone(), p(ax, mid), p(bx, mid), v_end.clone()])
    };
    let vh = || compact(vec![v_start.clone(), p(ax, by), h_end.clone()]);
    let hv = || compact(vec![h_start.clone(), p(bx, ay), v_end.clone()]);
    let (overlap_from, overlap_to) = (a.x.max(b.x), (a.x + a.width).min(b.x + b.width));
    let elbow =
        match layout {
            RouteLayout::Message if vertical_apart && overlap_from <= overlap_to => {
                let x = (overlap_from + overlap_to) / 2.0;
                vec![p(x, v_start.y), p(x, v_end.y)]
            }
            // Leave a top/bottom boundary event vertically when the target lies beyond that side.
            RouteLayout::FromBoundary(Side::Bottom) if below => {
                if horizontal_apart {
                    vh()
                } else {
                    vv()
                }
            }
            RouteLayout::FromBoundary(Side::Top) if vertical_apart && !below => {
                if horizontal_apart { vh() } else { vv() }
            }
            _ if horizontal_apart && !vertical_apart => hh(),
            _ if vertical_apart && !horizontal_apart => vv(),
            RouteLayout::Sequence | RouteLayout::FromBoundary(_) if horizontal_apart => hh(),
            RouteLayout::FromGateway if horizontal_apart => vh(),
            RouteLayout::IntoGateway if horizontal_apart => hv(),
            RouteLayout::Message if horizontal_apart => vv(),
            _ if (bx - ax).abs() >= (by - ay).abs() => hh(),
            _ => vv(),
        };
    let (start, end) = (elbow[0].clone(), elbow[elbow.len() - 1].clone());
    let contains = |r: &Bounds, x: f64, y: f64| {
        x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
    };
    let obstacles: Vec<Bounds> = shapes
        .iter()
        .filter(|r| *r != a && *r != b && !contains(r, ax, ay) && !contains(r, bx, by))
        .cloned()
        .collect();
    if clear(&elbow, &obstacles) {
        return elbow;
    }
    match route(start, end, &obstacles, 10.0, 512) {
        Ok(RouteResult {
            waypoints: Some(waypoints),
            ..
        }) => waypoints,
        _ => elbow,
    }
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
// Route between authored interior points while treating both endpoint owners as
// obstacles. A short outward stub fixes the port direction before routing.
fn repair_owned_endpoints(
    points: &[Point],
    start: Point,
    end: Point,
    source: &Bounds,
    target: &Bounds,
    obstacles: &[Bounds],
) -> Result<Vec<Point>, &'static str> {
    fn stub(
        anchor: &Point,
        neighbor: &Point,
        bounds: &Bounds,
    ) -> Result<(Point, Point), &'static str> {
        let scale = [
            anchor.x.abs(),
            anchor.y.abs(),
            bounds.x.abs(),
            bounds.y.abs(),
            bounds.width,
            bounds.height,
        ]
        .into_iter()
        .fold(1.0_f64, f64::max);
        let tolerance = 1e-9 + scale * f64::EPSILON * 16.0;
        let near = |a: f64, b: f64| (a - b).abs() <= tolerance;
        let within = |v: f64, lo: f64, hi: f64| v >= lo - tolerance && v <= hi + tolerance;
        let mut sides = Vec::new();
        if near(anchor.x, bounds.x) && within(anchor.y, bounds.y, bounds.y + bounds.height) {
            sides.push((-1.0, 0.0));
        }
        if near(anchor.x, bounds.x + bounds.width)
            && within(anchor.y, bounds.y, bounds.y + bounds.height)
        {
            sides.push((1.0, 0.0));
        }
        if near(anchor.y, bounds.y) && within(anchor.x, bounds.x, bounds.x + bounds.width) {
            sides.push((0.0, -1.0));
        }
        if near(anchor.y, bounds.y + bounds.height)
            && within(anchor.x, bounds.x, bounds.x + bounds.width)
        {
            sides.push((0.0, 1.0));
        }
        let direction = if sides.len() == 1 {
            sides[0]
        } else {
            let candidates: Vec<_> = sides
                .into_iter()
                .filter(|(dx, dy)| {
                    (*dx != 0.0
                        && near(neighbor.y, anchor.y)
                        && (neighbor.x - anchor.x) * dx > tolerance)
                        || (*dy != 0.0
                            && near(neighbor.x, anchor.x)
                            && (neighbor.y - anchor.y) * dy > tolerance)
                })
                .collect();
            if candidates.len() != 1 {
                return Err("ambiguous endpoint exit direction");
            }
            candidates[0]
        };
        let snapped = match direction {
            (-1.0, 0.0) => p(bounds.x, anchor.y.clamp(bounds.y, bounds.y + bounds.height)),
            (1.0, 0.0) => p(
                bounds.x + bounds.width,
                anchor.y.clamp(bounds.y, bounds.y + bounds.height),
            ),
            (0.0, -1.0) => p(anchor.x.clamp(bounds.x, bounds.x + bounds.width), bounds.y),
            _ => p(
                anchor.x.clamp(bounds.x, bounds.x + bounds.width),
                bounds.y + bounds.height,
            ),
        };
        let point = p(snapped.x + direction.0 * 8.0, snapped.y + direction.1 * 8.0);
        if !valid_point(&point) {
            return Err("endpoint exit exceeds coordinate limit");
        }
        Ok((snapped, point))
    }
    let (start, first_stub) = stub(
        &start,
        &p(
            points[1].x + start.x - points[0].x,
            points[1].y + start.y - points[0].y,
        ),
        source,
    )?;
    let last_index = points.len() - 1;
    let (end, last_stub) = stub(
        &end,
        &p(
            points[last_index - 1].x + end.x - points[last_index].x,
            points[last_index - 1].y + end.y - points[last_index].y,
        ),
        target,
    )?;
    let mut blocked_owners = obstacles.to_vec();
    blocked_owners.push(source.clone());
    blocked_owners.push(target.clone());
    let mut anchors = vec![first_stub];
    anchors.extend_from_slice(&points[1..last_index]);
    anchors.push(last_stub.clone());
    let mut result = vec![start];
    for pair in anchors.windows(2) {
        if result.last() != Some(&pair[0]) {
            result.push(pair[0].clone());
        }
        let segment = route(pair[0].clone(), pair[1].clone(), &blocked_owners, 0.0, 2048)?
            .waypoints
            .ok_or("cannot safely repair endpoint route")?;
        result.extend(segment.into_iter().skip(1));
    }
    result.push(end);
    // A stub collinear with the adjacent routed segment need not add a DI bend.
    if result.len() >= 3 {
        let n = result.len();
        let (a, b, c) = (&result[n - 3], &result[n - 2], &result[n - 1]);
        if b == &last_stub
            && !points[1..last_index].contains(b)
            && ((a.x == b.x && b.x == c.x && (b.y - a.y) * (c.y - b.y) >= 0.0)
                || (a.y == b.y && b.y == c.y && (b.x - a.x) * (c.x - b.x) >= 0.0))
            && clear(&[a.clone(), c.clone()], &blocked_owners)
        {
            result.remove(n - 2);
        }
    }
    if !clear(&result, &blocked_owners) {
        return Err("cannot safely repair endpoint route");
    }
    Ok(result)
}
/// One batch represents one drag gesture and therefore one undo step.
pub fn move_multiple(
    document_id: DocumentId,
    bounds: Vec<ShapeBounds>,
    routes: Vec<(ElementSelector, ElementSelector, Vec<Point>)>,
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

// Map an existing attachment to the same normalized arc-length on a repaired edge.
// Ambiguous crossings and off-route attachments cannot be translated safely.
fn edge_attachment(old: &[Point], new: &[Point], point: &Point) -> Result<Point, &'static str> {
    let length = |points: &[Point]| {
        points
            .windows(2)
            .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
            .sum::<f64>()
    };
    let old_total = length(old);
    let new_total = length(new);
    if !old_total.is_finite() || !new_total.is_finite() || old_total == 0.0 || new_total == 0.0 {
        return Err("edge attachment route has zero or invalid length");
    }
    let mut best: Option<(f64, f64)> = None;
    let mut travelled = 0.0;
    for pair in old.windows(2) {
        let dx = pair[1].x - pair[0].x;
        let dy = pair[1].y - pair[0].y;
        let segment = dx.hypot(dy);
        if segment == 0.0 {
            continue;
        }
        let t = (((point.x - pair[0].x) * dx + (point.y - pair[0].y) * dy) / (segment * segment))
            .clamp(0.0, 1.0);
        let distance = (point.x - pair[0].x - t * dx).hypot(point.y - pair[0].y - t * dy);
        let position = travelled + t * segment;
        if let Some((best_distance, best_position)) = best {
            if (distance - best_distance).abs() <= 1e-7
                && (position - best_position).abs() > 1e-7
                && distance <= 1e-6
            {
                return Err("edge attachment position is ambiguous");
            }
            if distance < best_distance {
                best = Some((distance, position));
            }
        } else {
            best = Some((distance, position));
        }
        travelled += segment;
    }
    let (distance, position) = best.ok_or("edge attachment route has zero or invalid length")?;
    if distance > 1e-6 {
        return Err("edge attachment is not on its referenced route");
    }
    let mut remaining = position / old_total * new_total;
    for pair in new.windows(2) {
        let dx = pair[1].x - pair[0].x;
        let dy = pair[1].y - pair[0].y;
        let segment = dx.hypot(dy);
        if segment == 0.0 {
            continue;
        }
        if remaining <= segment {
            return Ok(p(
                pair[0].x + dx * remaining / segment,
                pair[0].y + dy * remaining / segment,
            ));
        }
        remaining -= segment;
    }
    Ok(new
        .last()
        .ok_or("edge attachment route has zero or invalid length")?
        .clone())
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
    diagram_id: &ElementSelector,
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
        let (dx, dy) = (
            parent.bounds.x - parent_shape.bounds.x,
            parent.bounds.y - parent_shape.bounds.y,
        );
        if let Some(io) = &node.io_specification {
            for id in io
                .inputs
                .iter()
                .chain(&io.outputs)
                .filter_map(|port| port.id.as_ref())
            {
                for shape in diagram.shapes.iter().filter(|shape| &shape.element == id) {
                    let mut translated = shape.bounds.clone();
                    translated.x += dx;
                    translated.y += dy;
                    if let Some(existing) = bounds.iter().find(|b| b.shape_id == shape.id) {
                        if existing.bounds != translated {
                            return Err("conflicting parent and port moves");
                        }
                    } else {
                        bounds.push(ShapeBounds {
                            diagram_id: diagram_id.clone(),
                            shape_id: shape.id.selector(),
                            bounds: translated,
                        });
                    }
                }
            }
        }
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
                .map(|n| n.id.selector())
                .chain(scope.lanes.iter().map(|l| l.id.selector()))
                .chain(
                    scope
                        .nodes
                        .iter()
                        .filter_map(|node| node.io_specification.as_ref())
                        .flat_map(|io| {
                            io.inputs
                                .iter()
                                .chain(&io.outputs)
                                .filter_map(|port| port.id.as_ref().map(|id| id.selector()))
                        }),
                );
            for id in ids {
                for shape in diagram.shapes.iter().filter(|shape| shape.element == id) {
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
                            shape_id: shape.id.selector(),
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
    // Choreography DI has no parent pointer. Establish ownership from the
    // selected representation and unique containment at each graph scope.
    let graph_scopes = crate::queries::choreography_scopes(document);
    let mut ignored_graph_edges = std::collections::BTreeSet::new();
    let mut selected_graph_edges = std::collections::BTreeSet::new();
    let contains = |outer: &Bounds, inner: &Bounds| {
        outer.x <= inner.x
            && outer.y <= inner.y
            && outer.x + outer.width >= inner.x + inner.width
            && outer.y + outer.height >= inner.y + inner.height
    };
    let intersects = |a: &Bounds, b: &Bounds| {
        a.x <= b.x + b.width
            && a.x + a.width >= b.x
            && a.y <= b.y + b.height
            && a.y + a.height >= b.y
    };
    for requested_parent in &requested {
        let parent_shape = diagram
            .shapes
            .iter()
            .find(|s| s.id == requested_parent.shape_id)
            .ok_or("shape not found")?;
        if !graph_scopes
            .iter()
            .any(|scope| scope.owner == parent_shape.element && scope.parent.is_some())
        {
            continue;
        }
        let dx = requested_parent.bounds.x - parent_shape.bounds.x;
        let dy = requested_parent.bounds.y - parent_shape.bounds.y;
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        if requested_parent.bounds.width != parent_shape.bounds.width
            || requested_parent.bounds.height != parent_shape.bounds.height
        {
            return Err("cannot resize and move a choreography subtree together");
        }
        let mut pending = vec![(parent_shape, 1usize)];
        while let Some((owner, depth)) = pending.pop() {
            if depth > 32 {
                return Err("nested choreography exceeds depth limit");
            }
            let Some(scope) = graph_scopes
                .iter()
                .find(|scope| scope.owner == owner.element && scope.parent.is_some())
            else {
                continue;
            };
            let ids: std::collections::BTreeSet<_> = scope
                .activities
                .iter()
                .map(|a| a.id.selector())
                .chain(
                    scope
                        .events
                        .iter()
                        .chain(&scope.gateways)
                        .map(|n| n.id.selector()),
                )
                .chain(scope.artifacts.iter().filter_map(|a| match a {
                    crate::bpmn_collaboration::CollaborationArtifact::Group(v) => {
                        Some(v.id.selector())
                    }
                    crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(v) => {
                        Some(v.id.selector())
                    }
                    _ => None,
                }))
                .collect();
            let owner_candidates: Vec<_> = diagram
                .shapes
                .iter()
                .filter(|s| s.element == owner.element)
                .collect();
            for id in ids {
                let mut selected_count = 0;
                for child in diagram
                    .shapes
                    .iter()
                    .filter(|s| s.element == id && s.participant_band_kind.is_none())
                {
                    if !intersects(&owner.bounds, &child.bounds) {
                        if owner_candidates
                            .iter()
                            .filter(|candidate| contains(&candidate.bounds, &child.bounds))
                            .count()
                            != 1
                        {
                            return Err("ambiguous choreography descendant representation");
                        }
                        continue;
                    }
                    if !contains(&owner.bounds, &child.bounds)
                        || owner_candidates
                            .iter()
                            .filter(|candidate| contains(&candidate.bounds, &child.bounds))
                            .count()
                            != 1
                    {
                        return Err("ambiguous choreography descendant representation");
                    }
                    selected_count += 1;
                    if selected_count > 1 {
                        return Err("ambiguous choreography descendant representation");
                    }
                    let mut translated = child.bounds.clone();
                    translated.x += dx;
                    translated.y += dy;
                    if let Some(existing) = bounds.iter().find(|b| b.shape_id == child.id) {
                        if existing.bounds != translated {
                            return Err("conflicting parent and child moves");
                        }
                    } else {
                        bounds.push(ShapeBounds {
                            diagram_id: diagram_id.clone(),
                            shape_id: child.id.selector(),
                            bounds: translated,
                        });
                    }
                    nested_translated.insert(child.element.clone());
                    if graph_scopes
                        .iter()
                        .any(|s| s.owner == child.element && s.parent.is_some())
                    {
                        pending.push((child, depth + 1));
                    }
                }
            }
            let edge_ids: std::collections::BTreeSet<_> = scope
                .flows
                .iter()
                .map(|f| f.id.selector())
                .chain(
                    scope
                        .data_associations
                        .iter()
                        .filter_map(|a| a.id.as_ref().map(|id| id.selector())),
                )
                .chain(scope.artifacts.iter().filter_map(|a| match a {
                    crate::bpmn_collaboration::CollaborationArtifact::Association(f) => {
                        Some(f.id.selector())
                    }
                    _ => None,
                }))
                .collect();
            for edge in diagram
                .edges
                .iter()
                .filter(|e| edge_ids.contains(&e.element))
            {
                let Some(first) = edge.waypoints.first() else {
                    return Err("choreography edge has insufficient geometry");
                };
                let (mut x, mut y, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
                for p in &edge.waypoints {
                    x = x.min(p.x);
                    y = y.min(p.y);
                    right = right.max(p.x);
                    bottom = bottom.max(p.y);
                }
                let extent = Bounds {
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                };
                if !intersects(&owner.bounds, &extent) {
                    if owner_candidates
                        .iter()
                        .filter(|candidate| contains(&candidate.bounds, &extent))
                        .count()
                        != 1
                    {
                        return Err("ambiguous choreography edge representation");
                    }
                    ignored_graph_edges.insert(edge.id.selector());
                    continue;
                }
                if !contains(&owner.bounds, &extent)
                    || owner_candidates
                        .iter()
                        .filter(|candidate| contains(&candidate.bounds, &extent))
                        .count()
                        != 1
                {
                    return Err("ambiguous choreography edge representation");
                }
                selected_graph_edges.insert(edge.id.selector());
            }
        }
    }
    for id in &selected_graph_edges {
        ignored_graph_edges.remove(id);
    }
    // Bands belong to a particular activity representation, not to every
    // representation of the same semantic activity.
    let owners = bounds.clone();
    for owner in owners {
        let source = diagram
            .shapes
            .iter()
            .find(|s| s.id == owner.shape_id)
            .ok_or("shape not found")?;
        for band in diagram
            .shapes
            .iter()
            .filter(|shape| match &shape.choreography_activity_shape {
                Some(crate::model::DiReference::Local(id)) => id == &owner.shape_id,
                Some(crate::model::DiReference::Qualified(q)) => {
                    document
                        .definitions
                        .as_ref()
                        .is_some_and(|defs| defs.target_namespace == q.namespace)
                        && source.id.xml_id() == Some(&q.local)
                }
                None => false,
            })
        {
            if !valid_bounds(&source.bounds)
                || !valid_bounds(&band.bounds)
                || !valid_bounds(&owner.bounds)
            {
                return Err("invalid choreography or participant band bounds");
            }
            let sx = owner.bounds.width / source.bounds.width;
            let sy = owner.bounds.height / source.bounds.height;
            let mut translated = band.bounds.clone();
            translated.x = owner.bounds.x + (band.bounds.x - source.bounds.x) * sx;
            translated.y = owner.bounds.y + (band.bounds.y - source.bounds.y) * sy;
            translated.width *= sx;
            translated.height *= sy;
            if !valid_bounds(&translated) {
                return Err("resized participant band exceeds geometry limits");
            }
            if let Some(existing) = bounds.iter().find(|b| b.shape_id == band.id) {
                if existing.bounds != translated {
                    return Err("conflicting choreography and participant band moves");
                }
            } else {
                bounds.push(ShapeBounds {
                    diagram_id: diagram_id.clone(),
                    shape_id: band.id.selector(),
                    bounds: translated,
                });
            }
        }
    }
    let mut moved = Vec::new();
    for b in &bounds {
        if &b.diagram_id != diagram_id
            || !valid_bounds(&b.bounds)
            || moved.iter().any(
                |(id, _, _, _): &(&ElementSelector, &ElementSelector, f64, f64)| *id == &b.shape_id,
            )
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
    let references = crate::reference_resolution::ReferenceIndex::new(document);
    let resolve = |reference: &Option<crate::bpmn_activity::ModelReference>| {
        reference
            .as_ref()
            .and_then(|reference| match references.resolve(reference) {
                crate::reference_resolution::ReferenceResolution::Resolved(id) => Some(id),
                _ => None,
            })
    };
    let choreography_scopes = crate::queries::choreography_scopes(document);
    let artifact_flows: Vec<_> = document
        .collaboration
        .iter()
        .chain(&document.additional_collaborations)
        .filter_map(|c| c.extras.as_ref())
        .chain(
            document
                .choreographies
                .iter()
                .filter_map(|c| c.extras.as_ref()),
        )
        .flat_map(|extras| extras.artifacts.iter())
        .chain(
            choreography_scopes
                .iter()
                .filter(|scope| scope.parent.is_some())
                .flat_map(|scope| scope.artifacts.iter().copied()),
        )
        .filter_map(|artifact| match artifact {
            crate::bpmn_collaboration::CollaborationArtifact::Association(flow) => {
                Some(flow.as_ref())
            }
            _ => None,
        })
        .collect();
    // Normalize the qualified binary edge families to their visible endpoints.
    let mut connections: Vec<(ElementSelector, ElementSelector, ElementSelector)> = scopes
        .iter()
        .flat_map(|scope| scope.flows.iter())
        .chain(
            document
                .collaboration
                .iter()
                .chain(&document.additional_collaborations)
                .flat_map(|c| c.flows.iter()),
        )
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| c.message_flows.iter()),
        )
        .chain(artifact_flows.iter().copied())
        .filter_map(|flow| {
            Some((
                flow.id.selector(),
                resolve(&flow.source)?,
                resolve(&flow.target)?,
            ))
        })
        .collect();
    for scope in &choreography_scopes {
        let members: std::collections::BTreeSet<_> = scope
            .activities
            .iter()
            .map(|a| a.id.selector())
            .chain(
                scope
                    .events
                    .iter()
                    .chain(&scope.gateways)
                    .map(|n| n.id.selector()),
            )
            .collect();
        for flow in &scope.flows {
            if let (Some(source), Some(target)) = (resolve(&flow.source), resolve(&flow.target))
                && members.contains(&source)
                && members.contains(&target)
            {
                connections.push((flow.id.selector(), source, target));
            }
        }
    }
    for scope in &scopes {
        for association in scope.data_associations {
            let Some(association_id) = association.id.as_ref() else {
                continue;
            };
            if !diagram
                .edges
                .iter()
                .any(|edge| &edge.element == association_id)
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
            let visible =
                |id: &ElementSelector| diagram.shapes.iter().any(|shape| &shape.element == id);
            let owner_io = crate::queries::find_node(document, &association.owner)
                .and_then(|node| node.io_specification.as_ref());
            let input_port = owner_io.is_some_and(|io| {
                io.inputs
                    .iter()
                    .any(|port| port.id.as_ref().is_some_and(|id| *id == association.target))
            });
            let output_port = owner_io.is_some_and(|io| {
                io.outputs.iter().any(|port| {
                    port.id
                        .as_ref()
                        .is_some_and(|id| *id == association.sources[0])
                })
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
            let has_explicit = |source_side: bool| {
                diagram
                    .edges
                    .iter()
                    .filter(|edge| edge.element == association_id.selector())
                    .any(|edge| {
                        if source_side {
                            edge.source_element.is_some()
                        } else {
                            edge.target_element.is_some()
                        }
                    })
            };
            if (!visible(source) && !has_explicit(true))
                || (!visible(target) && !has_explicit(false))
            {
                if affected {
                    return Err("data association has unresolved visible endpoint");
                }
                continue;
            }
            connections.push((association_id.selector(), source.clone(), target.clone()));
        }
    }
    for collaboration in document
        .collaboration
        .iter()
        .chain(&document.additional_collaborations)
    {
        connections.extend(collaboration.conversation_links.iter().filter_map(|link| {
            Some((
                link.id.selector(),
                resolve(&link.source)?,
                resolve(&link.target)?,
            ))
        }));
    }
    for choreography in &document.choreographies {
        connections.extend(choreography.conversation_links.iter().filter_map(|link| {
            Some((
                link.id.selector(),
                resolve(&link.source)?,
                resolve(&link.target)?,
            ))
        }));
    }
    let partial_connections: Vec<_> = scopes
        .iter()
        .flat_map(|scope| scope.flows.iter())
        .chain(
            document
                .collaboration
                .iter()
                .chain(&document.additional_collaborations)
                .flat_map(|c| c.flows.iter()),
        )
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| c.message_flows.iter()),
        )
        .chain(
            choreography_scopes
                .iter()
                .flat_map(|scope| scope.flows.iter().copied()),
        )
        .chain(artifact_flows.iter().copied())
        .map(|flow| {
            (
                flow.id.selector(),
                resolve(&flow.source),
                resolve(&flow.target),
            )
        })
        .chain(
            document
                .collaboration
                .iter()
                .chain(&document.additional_collaborations)
                .flat_map(|c| c.conversation_links.iter())
                .map(|link| {
                    (
                        link.id.selector(),
                        resolve(&link.source),
                        resolve(&link.target),
                    )
                }),
        )
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| c.conversation_links.iter())
                .map(|link| {
                    (
                        link.id.selector(),
                        resolve(&link.source),
                        resolve(&link.target),
                    )
                }),
        )
        .collect();
    let mut routes: Vec<(ElementSelector, ElementSelector, Vec<Point>)> = Vec::new();
    let mut label_moves = Vec::new();
    let mut warnings = Vec::new();
    // Limit dependency analysis to the edges whose endpoints actually move, then
    // propagate to their dependants. Unrelated cyclic or incomplete clusters are
    // retained unchanged, while affected cycles are refused atomically.
    let mut affected = std::collections::BTreeSet::new();
    for edge in &diagram.edges {
        if ignored_graph_edges.contains(&edge.id.selector()) {
            continue;
        }
        let Some((_, source, target)) = connections.iter().find(|(id, _, _)| *id == edge.element)
        else {
            // Preserve unrelated incomplete/external edges. Explicitly attached
            // moved DI shapes require a route we cannot resolve safely.
            let Some((_, partial_source, partial_target)) = partial_connections
                .iter()
                .find(|(id, _, _)| id == &edge.element)
            else {
                return Err("cannot repair dependent edge with unsupported semantics");
            };
            let known_endpoint_moved = {
                let (source, target) = (partial_source, partial_target);
                [source.as_ref(), target.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|semantic| {
                        moved
                            .iter()
                            .any(|(_, id, dx, dy)| *id == semantic && (*dx != 0.0 || *dy != 0.0))
                    })
            };
            if selected_graph_edges.contains(&edge.id.selector())
                || known_endpoint_moved
                || [edge.source_element.as_ref(), edge.target_element.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|rep| {
                        moved
                            .iter()
                            .any(|(id, _, dx, dy)| *id == rep && (*dx != 0.0 || *dy != 0.0))
                    })
            {
                return Err("cannot repair dependent edge with unresolved semantics");
            }
            continue;
        };
        let moved_endpoint =
            |semantic: &ElementSelector, representation: Option<&ElementSelector>| {
                moved.iter().any(|(shape_id, element, dx, dy)| {
                    (*dx != 0.0 || *dy != 0.0)
                        && match representation {
                            Some(id) => *shape_id == id,
                            None => *element == semantic,
                        }
                })
            };
        if selected_graph_edges.contains(&edge.id.selector()) {
            let endpoint_owned = |semantic: &ElementSelector,
                                  representation: Option<&ElementSelector>,
                                  point: Option<&Point>| {
                let Some(point) = point else {
                    return false;
                };
                let candidates: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|shape| {
                        &shape.element == semantic
                            && moved.iter().any(|(id, _, dx, dy)| {
                                *id == &shape.id.selector() && (*dx != 0.0 || *dy != 0.0)
                            })
                            && representation.is_none_or(|id| &shape.id == id)
                            && (representation.is_some()
                                || (point.x >= shape.bounds.x
                                    && point.x <= shape.bounds.x + shape.bounds.width
                                    && point.y >= shape.bounds.y
                                    && point.y <= shape.bounds.y + shape.bounds.height))
                    })
                    .collect();
                candidates.len() == 1
            };
            if !endpoint_owned(source, edge.source_element.as_ref(), edge.waypoints.first())
                || !endpoint_owned(target, edge.target_element.as_ref(), edge.waypoints.last())
            {
                return Err("cannot prove choreography edge endpoint ownership");
            }
        }
        if moved_endpoint(source, edge.source_element.as_ref())
            || moved_endpoint(target, edge.target_element.as_ref())
        {
            affected.insert(edge.id.selector());
        }
    }
    loop {
        let mut changed = false;
        for edge in &diagram.edges {
            if !affected.contains(&edge.id.selector())
                && [edge.source_element.as_ref(), edge.target_element.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|id| affected.contains(id))
            {
                affected.insert(edge.id.selector());
                changed = true;
                if affected.len() > 1000 {
                    return Err("edge dependency limit exceeded");
                }
            }
        }
        if !changed {
            break;
        }
    }
    if affected.len() > 1000 {
        return Err("edge dependency limit exceeded");
    }
    let mut pending: Vec<_> = diagram
        .edges
        .iter()
        .filter(|edge| affected.contains(&edge.id.selector()))
        .collect();
    let mut remaining = affected;
    let mut ordered = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let index = pending.iter().position(|edge| {
            [edge.source_element.as_ref(), edge.target_element.as_ref()]
                .into_iter()
                .flatten()
                .all(|id| !remaining.contains(id))
        });
        let Some(index) = index else {
            return Err("cyclic edge representation endpoints are unsupported");
        };
        let edge = pending.remove(index);
        remaining.remove(&edge.id.selector());
        ordered.push(edge);
    }
    for edge in ordered {
        let Some((_, source, target)) = connections.iter().find(|(id, _, _)| *id == edge.element)
        else {
            continue;
        };
        // Explicit DI endpoints name representations, which may themselves be edges.
        for representation in [edge.source_element.as_ref(), edge.target_element.as_ref()]
            .into_iter()
            .flatten()
        {
            if !diagram
                .shapes
                .iter()
                .any(|candidate| candidate.id == *representation)
                && !diagram
                    .edges
                    .iter()
                    .any(|candidate| candidate.id == *representation)
            {
                return Err("edge representation endpoint not found in diagram");
            }
        }
        let delta = |element: &ElementSelector,
                     representation: Option<&ElementSelector>,
                     point: Option<&Point>|
         -> Result<Option<(f64, f64)>, &'static str> {
            if let Some(reference) = representation
                && let Some(anchor_edge) = diagram
                    .edges
                    .iter()
                    .find(|candidate| candidate.id == *reference)
            {
                if let Some((_, _, repaired)) = routes.iter().find(|(_, id, _)| id == reference) {
                    let point = point.ok_or("connected edge has insufficient waypoints")?;
                    let anchor = edge_attachment(&anchor_edge.waypoints, repaired, point)?;
                    return Ok(Some((anchor.x - point.x, anchor.y - point.y)));
                }
                return Ok(None);
            }
            let matches: Vec<_> = moved
                .iter()
                .filter(|(shape_id, id, _, _)| match representation {
                    Some(representation) => *shape_id == representation,
                    None => *id == element,
                })
                .collect();
            if matches.len() > 1 {
                return Err("ambiguous moved endpoint");
            }
            Ok(matches.first().map(|(_, _, dx, dy)| (*dx, *dy)))
        };
        let source_delta = delta(source, edge.source_element.as_ref(), edge.waypoints.first())?;
        let target_delta = delta(target, edge.target_element.as_ref(), edge.waypoints.last())?;
        if edge.waypoints.len() < 2 {
            if source_delta.is_some_and(|d| d != (0.0, 0.0))
                || target_delta.is_some_and(|d| d != (0.0, 0.0))
            {
                return Err("connected edge has insufficient waypoints");
            }
            continue;
        }
        let move_endpoint = |point: &Point, delta: Option<(f64, f64)>| {
            delta.and_then(|(dx, dy)| {
                if dx == 0.0 && dy == 0.0 {
                    None
                } else {
                    Some(p(point.x + dx, point.y + dy))
                }
            })
        };
        let start = move_endpoint(&edge.waypoints[0], source_delta);
        let end = move_endpoint(&edge.waypoints[edge.waypoints.len() - 1], target_delta);
        if let (Some((dx, dy)), Some(other)) = (source_delta, target_delta)
            && (dx, dy) == other
            && (dx != 0.0 || dy != 0.0)
            && nested_translated.contains(source)
            && nested_translated.contains(target)
        {
            if let Some(mut bounds) = edge.label.as_ref().and_then(|label| label.bounds.clone()) {
                bounds.x += dx;
                bounds.y += dy;
                if !valid_bounds(&bounds) {
                    return Err("translated edge label exceeds coordinate limit");
                }
                label_moves.push(Mutation::SetLabelBounds {
                    document_id: document.id,
                    diagram_id: diagram_id.clone(),
                    representation_id: edge.id.selector(),
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
            routes.push((diagram_id.clone(), edge.id.selector(), translated));
            continue;
        }
        if start.is_none() && end.is_none() {
            continue;
        }
        let obstacles: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| {
                let source_is_shape = edge
                    .source_element
                    .as_ref()
                    .map_or(s.element == *source, |id| s.id == *id);
                let target_is_shape = edge
                    .target_element
                    .as_ref()
                    .map_or(s.element == *target, |id| s.id == *id);
                !source_is_shape && !target_is_shape
            })
            // Ignore only containers of an endpoint; unrelated containers remain obstacles.
            .filter(|s| {
                let is_scope_ancestor = |element: &ElementSelector, target: &ElementSelector| {
                    let mut current = target.clone();
                    for _ in 0..=32 {
                        let Some(scope) = scopes
                            .iter()
                            .find(|scope| scope.nodes.iter().any(|node| node.id == current))
                        else {
                            break;
                        };
                        if scope.id == element {
                            return true;
                        }
                        current = scope.id.selector();
                    }
                    false
                };
                let contains = |target: &ElementSelector| {
                    is_scope_ancestor(&s.element, target)
                        || scopes.iter().any(|scope| {
                            scope
                                .lanes
                                .iter()
                                .any(|lane| lane.id == s.element && lane.members.contains(target))
                        })
                        || document
                            .collaboration
                            .iter()
                            .chain(&document.additional_collaborations)
                            .any(|c| {
                                c.participants.iter().any(|participant| {
                                    participant.id == s.element
                                        && resolve(&participant.process).is_some_and(|process| {
                                            is_scope_ancestor(&process, target)
                                        })
                                })
                            })
                };
                let owns_port = |target: &ElementSelector| {
                    scopes
                        .iter()
                        .flat_map(|scope| scope.nodes.iter())
                        .any(|node| {
                            node.id == s.element
                                && node.io_specification.as_ref().is_some_and(|io| {
                                    io.inputs
                                        .iter()
                                        .chain(&io.outputs)
                                        .any(|port| port.id.as_ref().is_some_and(|id| id == target))
                                })
                        })
                };
                !contains(source) && !contains(target) && !owns_port(source) && !owns_port(target)
            })
            .map(|s| {
                bounds
                    .iter()
                    .find(|b| b.shape_id == s.id)
                    .map_or_else(|| s.bounds.clone(), |b| b.bounds.clone())
            })
            .collect();
        if crate::queries::find_flow(document, &edge.element)
            .is_some_and(|flow| flow.kind == crate::model::FlowKind::Sequence)
        {
            let owner = |semantic: &ElementSelector,
                         representation: Option<&ElementSelector>|
             -> Result<Bounds, &'static str> {
                let candidates: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|shape| {
                        &shape.element == semantic
                            && representation.is_none_or(|id| &shape.id == id)
                    })
                    .collect();
                if candidates.len() != 1 {
                    return Err("ambiguous endpoint shape representation");
                }
                let shape = candidates[0];
                let proposed = bounds
                    .iter()
                    .find(|b| b.shape_id == shape.id)
                    .map_or(&shape.bounds, |b| &b.bounds);
                if proposed.width != shape.bounds.width || proposed.height != shape.bounds.height {
                    return Err("cannot safely repair resized endpoint");
                }
                Ok(proposed.clone())
            };
            let source_bounds = owner(source, edge.source_element.as_ref())?;
            let target_bounds = owner(target, edge.target_element.as_ref())?;
            let start_point = start.unwrap_or_else(|| edge.waypoints[0].clone());
            let end_point = end.unwrap_or_else(|| edge.waypoints[edge.waypoints.len() - 1].clone());
            let repaired = repair_owned_endpoints(
                &edge.waypoints,
                start_point,
                end_point,
                &source_bounds,
                &target_bounds,
                &obstacles,
            )?;
            routes.push((diagram_id.clone(), edge.id.selector(), repaired));
            continue;
        }
        let repaired = repair_endpoints(&edge.waypoints, start, end, &obstacles)?;
        if let Some(warning) = repaired.warning {
            warnings.push(format!("edge {}: {}", edge.id, warning));
        }
        routes.push((
            diagram_id.clone(),
            edge.id.selector(),
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
