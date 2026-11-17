//! Pure canvas geometry: coordinate transforms, handles, snapping, route editing, raster sizing and fit.
use super::*;
/// A handle on a selected edge: a waypoint or the middle of a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EdgePart {
    Point(usize),
    Segment(usize),
}
pub(super) fn edge_handles(points: &[Pos2]) -> Vec<(EdgePart, Pos2)> {
    let mut handles: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, p)| (EdgePart::Point(i), *p))
        .collect();
    handles.extend(
        points
            .windows(2)
            .enumerate()
            .map(|(i, w)| (EdgePart::Segment(i), w[0].lerp(w[1], 0.5))),
    );
    handles
}
/// The route after dragging `part` by `(dx, dy)` (bpmn-js bendpoint move and segment move).
/// `ends` are the bounds of the source and target shapes. An interior bendpoint drags its two
/// adjacent segments along, so an orthogonal route stays orthogonal; an orthogonal segment moves
/// across its axis; an endpoint dragged that way slides along its shape's border, clamped to
/// the shape's extent. A diagonal segment gets a new bendpoint at its middle; an endpoint itself
/// moves freely (the caller reconnects or re-docks it). `grid` rounds the moved coordinates to 10.
pub(super) fn edit_route(
    points: &[Point],
    part: EdgePart,
    dx: f64,
    dy: f64,
    ends: &[Option<Bounds>; 2],
    grid: bool,
) -> Vec<Point> {
    let snap = |v: f64| if grid { (v / 10.0).round() * 10.0 } else { v };
    let last = points.len().saturating_sub(1);
    // Clamp `v` to the side extent of endpoint `j` along x (`along_x`) or y.
    let clamp = |v: f64, j: usize, along_x: bool| {
        let shape = match j {
            0 => ends[0].as_ref(),
            j if j == last => ends[1].as_ref(),
            _ => None,
        };
        shape.map_or(v, |b| {
            if along_x {
                v.clamp(b.x, b.x + b.width)
            } else {
                v.clamp(b.y, b.y + b.height)
            }
        })
    };
    let mut out = points.to_vec();
    match part {
        EdgePart::Point(i) if i > 0 && i < last => {
            let (mut x, mut y) = (snap(points[i].x + dx), snap(points[i].y + dy));
            for j in [i - 1, i + 1] {
                if points[j].y == points[i].y {
                    y = clamp(y, j, false);
                } else if points[j].x == points[i].x {
                    x = clamp(x, j, true);
                }
            }
            for j in [i - 1, i + 1] {
                if points[j].y == points[i].y {
                    out[j].y = y;
                } else if points[j].x == points[i].x {
                    out[j].x = x;
                }
            }
            out[i] = Point { x, y };
        }
        EdgePart::Point(i) => {
            if let Some(p) = out.get_mut(i) {
                p.x += dx;
                p.y += dy;
            }
        }
        EdgePart::Segment(i) if i + 1 < out.len() => {
            let (a, b) = (out[i].clone(), out[i + 1].clone());
            if a.y == b.y {
                let y = clamp(clamp(snap(a.y + dy), i, false), i + 1, false);
                out[i].y = y;
                out[i + 1].y = y;
            } else if a.x == b.x {
                let x = clamp(clamp(snap(a.x + dx), i, true), i + 1, true);
                out[i].x = x;
                out[i + 1].x = x;
            } else {
                out.insert(
                    i + 1,
                    Point {
                        x: snap((a.x + b.x) / 2.0 + dx),
                        y: snap((a.y + b.y) / 2.0 + dy),
                    },
                );
            }
        }
        EdgePart::Segment(_) => {}
    }
    out
}
/// The route of a dragged end handle (`part`) when the pointer `at` is on or near (12 screen px)
/// its own shape: the end re-docked there by [`redock_end`]. `None` for other parts, or away
/// from the shape (the caller then reconnects or leaves the route).
pub(super) fn end_dock(
    points: &[Point],
    part: EdgePart,
    ends: &[Option<Bounds>; 2],
    kinds: &[Option<NodeKind>; 2],
    at: (f64, f64),
    zoom: f32,
) -> Option<Vec<Point>> {
    let last = points.len().checked_sub(1)?;
    let (i, own) = match part {
        EdgePart::Point(0) => (0, 0),
        EdgePart::Point(i) if i == last => (i, 1),
        _ => return None,
    };
    let b = ends[own].as_ref()?;
    let margin = 12.0 / zoom as f64;
    let near = at.0 >= b.x - margin
        && at.0 <= b.x + b.width + margin
        && at.1 >= b.y - margin
        && at.1 <= b.y + b.height + margin;
    near.then(|| {
        redock_end(
            points,
            i,
            (b, kinds[own]),
            ends[1 - own].as_ref(),
            at,
            7.0 / zoom as f64,
        )
    })
}
/// The route with end `i` docked on the outline of its shape `own` (bounds and kind), on the
/// side nearest to `at` (any side), at the pointer's position along that side, snapped within
/// `tolerance` to the side's middle, the neighbouring waypoint, or the far end (a straight
/// segment). The end segment stays orthogonal: an interior neighbour beyond the new side
/// follows along the segment's axis or gains an elbow; otherwise the end leaves through a short
/// stub and the bounded orthogonal router joins it to the neighbour (or to a stub off the far
/// shape `far` on a two-point route), around both shapes.
pub(super) fn redock_end(
    points: &[Point],
    i: usize,
    own: (&Bounds, Option<NodeKind>),
    far: Option<&Bounds>,
    at: (f64, f64),
    tolerance: f64,
) -> Vec<Point> {
    use processcraft_core::geometry::{self, Side};
    const STUB: f64 = 20.0;
    let mut out = points.to_vec();
    if out.len() < 2 {
        return out;
    }
    // Work with the dragged end last.
    if i == 0 {
        out.reverse();
    }
    let (b, kind) = own;
    let last = out.len() - 1;
    let side = geometry::side_of(&Point { x: at.0, y: at.1 }, b);
    let level = matches!(side, Side::Left | Side::Right);
    let (n, far_end, old) = (out[last - 1].clone(), out[0].clone(), out[last].clone());
    let axis = |q: &Point| if level { q.y } else { q.x };
    let mid = if level {
        b.y + b.height / 2.0
    } else {
        b.x + b.width / 2.0
    };
    let pointer = if level { at.1 } else { at.0 };
    let along = [mid, axis(&n), axis(&far_end)]
        .into_iter()
        .filter(|c| (c - pointer).abs() <= tolerance)
        .min_by(|a, c| (a - pointer).abs().total_cmp(&(c - pointer).abs()))
        .unwrap_or(pointer);
    let end = geometry::side_point(kind, b, side, along);
    let outward = |side: Side| match side {
        Side::Left => (-1.0, 0.0),
        Side::Right => (1.0, 0.0),
        Side::Top => (0.0, -1.0),
        Side::Bottom => (0.0, 1.0),
    };
    let dir = outward(side);
    let beyond = (n.x - end.x) * dir.0 + (n.y - end.y) * dir.1 > 0.0;
    let aligned = axis(&n) == axis(&end);
    let interior = last > 1;
    if beyond && (interior || aligned) {
        if !aligned {
            if level && n.y == old.y {
                out[last - 1].y = end.y;
            } else if !level && n.x == old.x {
                out[last - 1].x = end.x;
            } else {
                let elbow = if level {
                    Point { x: n.x, y: end.y }
                } else {
                    Point { x: end.x, y: n.y }
                };
                out.insert(last, elbow);
            }
        }
        *out.last_mut().unwrap() = end;
    } else {
        let stub = |q: &Point, d: (f64, f64)| Point {
            x: q.x + d.0 * STUB,
            y: q.y + d.1 * STUB,
        };
        let s = stub(&end, dir);
        let mut obstacles = vec![b.clone()];
        let (mut path, from) = match far.filter(|_| !interior) {
            Some(fb) => {
                obstacles.push(fb.clone());
                let fs = stub(&far_end, outward(geometry::side_of(&far_end, fb)));
                (vec![far_end.clone()], fs)
            }
            None => (out[..last - 1].to_vec(), n.clone()),
        };
        let joined = geometry::route(from.clone(), s.clone(), &obstacles, 0.0, 256)
            .ok()
            .and_then(|r| r.waypoints)
            .unwrap_or_else(|| {
                let elbow = if level {
                    Point { x: s.x, y: from.y }
                } else {
                    Point { x: from.x, y: s.y }
                };
                vec![from.clone(), elbow, s.clone()]
            });
        path.extend(joined);
        path.push(end);
        out = path;
    }
    if i == 0 {
        out.reverse();
    }
    out
}
/// Snap correction for a drag (bpmn-js): the first dragged shape snaps to its siblings (shapes
/// of elements in the same scope; for pools, lanes, and artifacts every shape that does not
/// contain it) and its mid to the mids of the elements it is connected to, then the optional
/// 10 px grid on free axes. `moved` is where the first shape would land (a resize passes the
/// dragged corner as an empty box, so only that point snaps).
pub(super) fn snap_drag(
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    items: &[(ElementSelector, Bounds)],
    moved: &Bounds,
    tolerance: f64,
    grid: bool,
) -> processcraft_core::geometry::Snap {
    use processcraft_core::{bpmn_activity::ModelReference, queries, rules};
    let Some((first_id, _)) = items.first() else {
        return Default::default();
    };
    let contains = |b: &Bounds| {
        b.x <= moved.x
            && b.y <= moved.y
            && b.x + b.width >= moved.x + moved.width
            && b.y + b.height >= moved.y + moved.height
    };
    let element = diagram
        .shapes
        .iter()
        .find(|s| s.id == *first_id)
        .map(|s| s.element.clone());
    let scope_of = |id: &ElementSelector| {
        rules::locate(d, id)
            .filter(|l| l.node.is_some())
            .and_then(|l| l.scope)
    };
    let scope = element.as_ref().and_then(scope_of);
    let candidates = diagram
        .shapes
        .iter()
        .filter(|s| !items.iter().any(|(id, _)| s.id == *id));
    let siblings: Vec<Bounds> = candidates
        .clone()
        .filter(|s| match &scope {
            Some(scope) => scope_of(&s.element).as_ref() == Some(scope),
            None => !contains(&s.bounds),
        })
        .map(|s| s.bounds.clone())
        .collect();
    // Elements at the other end of a flow shown in this diagram.
    let local = |r: &Option<ModelReference>| match r {
        Some(ModelReference::Local(id)) => Some(id.clone()),
        _ => None,
    };
    let partners: Vec<ElementSelector> = element
        .iter()
        .flat_map(|element| {
            diagram.edges.iter().filter_map(move |edge| {
                let flow = queries::find_flow(d, &edge.element)?;
                let (source, target) = (local(&flow.source)?, local(&flow.target)?);
                if source == *element {
                    Some(target)
                } else if target == *element {
                    Some(source)
                } else {
                    None
                }
            })
        })
        .collect();
    let connected: Vec<Bounds> = candidates
        .filter(|s| partners.contains(&s.element))
        .map(|s| s.bounds.clone())
        .collect();
    let mut snap = processcraft_core::geometry::snap(moved, &siblings, &connected, tolerance);
    if grid {
        let (cx, cy) = (moved.x + moved.width / 2.0, moved.y + moved.height / 2.0);
        if snap.guide_x.is_none() {
            snap.dx = (cx / 10.0).round() * 10.0 - cx;
        }
        if snap.guide_y.is_none() {
            snap.dy = (cy / 10.0).round() * 10.0 - cy;
        }
    }
    snap
}
/// Resize handles of a screen rectangle: its four corners, plus the four edge midpoints unless
/// the shape keeps its proportions.
pub(super) fn resize_handles(r: Rect, aspect: bool) -> Vec<((i8, i8), Pos2)> {
    let corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    let edges = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    corners
        .iter()
        .chain(if aspect { &[][..] } else { &edges[..] })
        .map(|&(x, y)| {
            (
                (x, y),
                r.center() + Vec2::new(x as f32 * r.width() / 2.0, y as f32 * r.height() / 2.0),
            )
        })
        .collect()
}
/// Pick radius of a resize handle, in screen pixels.
pub(super) const RESIZE_PICK: f32 = 9.0;
/// The resize handle nearest to `pos` within the pick radius.
pub(super) fn handle_at(handles: &[((i8, i8), Pos2)], pos: Pos2) -> Option<(i8, i8)> {
    handles
        .iter()
        .map(|(h, c)| (*h, c.distance(pos)))
        .filter(|(_, d)| *d <= RESIZE_PICK)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(h, _)| h)
}
pub(super) fn resize_cursor((hx, hy): (i8, i8)) -> egui::CursorIcon {
    match (hx, hy) {
        (0, _) => egui::CursorIcon::ResizeVertical,
        (_, 0) => egui::CursorIcon::ResizeHorizontal,
        _ if hx == hy => egui::CursorIcon::ResizeNwSe,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

/// Whether "Fit to content" applies: activities (not expanded subprocesses) and annotations.
pub(super) fn fits_content(d: &processcraft_core::model::BpmnDocument, shape: &Shape) -> bool {
    processcraft_core::queries::find_node(d, &shape.element).is_some_and(|n| {
        n.kind == NodeKind::TextAnnotation
            || (n.kind.is_activity() && shape.is_expanded != Some(true))
    })
}
/// `shape` with the height its label needs, measured like the renderer's label (12 px text in
/// the width minus 8, 4 px padding above and below, plus the icon row of typed activities and
/// the loop marker row). Width and top-left stay; never below the rule minimum.
pub(super) fn fitted_bounds(
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    shape: &Shape,
) -> Option<Bounds> {
    if !fits_content(d, shape) {
        return None;
    }
    let node = processcraft_core::queries::find_node(d, &shape.element)?;
    let text = label_edit(d, diagram, &shape.element, None).ok()?.text;
    let width = ((shape.bounds.width as f32) - 8.0).clamp(4.0, 16384.0);
    let text_height = if text.is_empty() {
        0.0
    } else {
        processcraft_render::TextEngine::new()
            .layout(processcraft_render::TextRequest::new(&text, 12.0, width))
            .ok()?
            .height()
    };
    let icon = if matches!(node.kind, NodeKind::Task | NodeKind::TextAnnotation) {
        0.0
    } else {
        20.0
    };
    let markers = if node.loop_characteristics.is_some() {
        18.0
    } else {
        0.0
    };
    let min = processcraft_core::manipulate::resize_rule(d, shape)?.height;
    Some(Bounds {
        height: (f64::from(text_height).ceil() + 8.0 + icon + markers).max(min),
        ..shape.bounds.clone()
    })
}
pub(super) fn fit(diagram: &processcraft_core::model::Diagram, size: Vec2) -> ((f64, f64), f32) {
    fit_extents(diagram_extents(diagram), size)
}
/// Camera and zoom showing `extents` (min x, min y, max x, max y) in `size`.
pub(super) fn fit_extents(extents: Option<(f64, f64, f64, f64)>, size: Vec2) -> ((f64, f64), f32) {
    if let Some((minx, miny, maxx, maxy)) = extents {
        let zoom = (((size.x as f64 - 48.0).max(1.0) / (maxx - minx).max(1.0))
            .min((size.y as f64 - 48.0).max(1.0) / (maxy - miny).max(1.0))
            as f32)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        (
            (
                (minx + maxx - size.x as f64 / zoom as f64) * 0.5,
                (miny + maxy - size.y as f64 / zoom as f64) * 0.5,
            ),
            zoom,
        )
    } else {
        ((0.0, 0.0), 1.0)
    }
}

/// Elements a marquee from `from` to `to` (model points) selects, as (element, DI id): dragged
/// left to right, shapes fully inside; right to left, shapes it touches. Edges count when all
/// their waypoints are inside.
pub(super) fn marquee(
    diagram: &processcraft_core::model::Diagram,
    from: (f64, f64),
    to: (f64, f64),
) -> Vec<(ElementSelector, ElementSelector)> {
    let touch = to.0 < from.0;
    let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
    let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
    let inside = |x: f64, y: f64| x >= x0 && x <= x1 && y >= y0 && y <= y1;
    let shapes = diagram.shapes.iter().filter(|s| {
        let b = &s.bounds;
        if touch {
            b.x <= x1 && b.x + b.width >= x0 && b.y <= y1 && b.y + b.height >= y0
        } else {
            inside(b.x, b.y) && inside(b.x + b.width, b.y + b.height)
        }
    });
    let edges = diagram
        .edges
        .iter()
        .filter(|e| !e.waypoints.is_empty() && e.waypoints.iter().all(|p| inside(p.x, p.y)));
    shapes
        .map(|s| (s.element.clone(), s.id.selector()))
        .chain(edges.map(|e| (e.element.clone(), e.id.selector())))
        .collect()
}
/// Shift during a move keeps only the dominant axis of the drag.
pub(super) fn lock_axis(offset: Vec2) -> Vec2 {
    if offset.x.abs() >= offset.y.abs() {
        Vec2::new(offset.x, 0.0)
    } else {
        Vec2::new(0.0, offset.y)
    }
}

/// Change zoom while keeping the model point under `anchor` (offset from the canvas origin) fixed.
pub(super) fn zoom_at(
    camera: (f64, f64),
    zoom: f32,
    new_zoom: f32,
    anchor: Vec2,
) -> ((f64, f64), f32) {
    let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let (ax, ay) = (anchor.x as f64, anchor.y as f64);
    (
        (
            camera.0 + ax / zoom as f64 - ax / new_zoom as f64,
            camera.1 + ay / zoom as f64 - ay / new_zoom as f64,
        ),
        new_zoom,
    )
}

pub(super) fn to_model(rect: Rect, camera: (f64, f64), zoom: f32, pos: Pos2) -> (f64, f64) {
    (
        camera.0 + (pos.x - rect.left()) as f64 / zoom as f64,
        camera.1 + (pos.y - rect.top()) as f64 / zoom as f64,
    )
}

pub(super) fn to_screen(rect: Rect, camera: (f64, f64), zoom: f32, b: &Bounds) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            rect.left() + ((b.x - camera.0) as f32 * zoom),
            rect.top() + ((b.y - camera.1) as f32 * zoom),
        ),
        Vec2::new(b.width as f32 * zoom, b.height as f32 * zoom),
    )
}

/// Where a texture rendered for `view` lands under the current camera.
pub(super) fn texture_rect(rect: Rect, view: TextureView, camera: (f64, f64), zoom: f32) -> Rect {
    Rect::from_min_size(
        rect.min
            + Vec2::new(
                (view.camera.0 - camera.0) as f32 * zoom,
                (view.camera.1 - camera.1) as f32 * zoom,
            ),
        view.size * (zoom / view.zoom),
    )
}

/// The extent a resized `shape` must keep covering: the shapes it fully contains (as the core
/// checks them) plus the core's child padding.
pub(super) fn child_extent(
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    shape: &Shape,
) -> Option<Bounds> {
    use processcraft_core::manipulate::{CHILD_PADDING, children};
    let o = &shape.bounds;
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for c in children(d, diagram, &shape.element)
        .into_iter()
        .filter(|c| {
            c.x >= o.x
                && c.y >= o.y
                && c.x + c.width <= o.x + o.width
                && c.y + c.height <= o.y + o.height
        })
    {
        lo = (lo.0.min(c.x), lo.1.min(c.y));
        hi = (hi.0.max(c.x + c.width), hi.1.max(c.y + c.height));
    }
    (lo.0 <= hi.0).then_some(Bounds {
        x: lo.0 - CHILD_PADDING,
        y: lo.1 - CHILD_PADDING,
        width: hi.0 - lo.0 + 2.0 * CHILD_PADDING,
        height: hi.1 - lo.1 + 2.0 * CHILD_PADDING,
    })
}
/// Bounds after dragging the resize handle `(hx, hy)` by `(dx, dy)`, in whole units. `centred`
/// moves the opposite side the other way; `aspect` keeps the ratio, scaling from the opposite
/// corner (or the centre on an axis without a handle). The size never drops below `min` nor
/// uncovers `keep`, the extent contained shapes need, so the core accepts the result.
pub(super) fn resize_bounds(
    b: &Bounds,
    (hx, hy): (i8, i8),
    (dx, dy): (f64, f64),
    min: (f64, f64),
    aspect: bool,
    centred: bool,
    keep: Option<&Bounds>,
) -> Bounds {
    let k = if centred { 2.0 } else { 1.0 };
    let (mut w, h) = (
        b.width + f64::from(hx) * dx * k,
        b.height + f64::from(hy) * dy * k,
    );
    // The smallest length along an axis: the minimum, and the room `keep` needs on the
    // moving sides.
    let need = |dir: i8, start: f64, len: f64, lo: f64, hi: f64, min: f64| {
        let centre = start + len / 2.0;
        min.max(match dir {
            0 => 0.0,
            _ if centred => 2.0 * (centre - lo).max(hi - centre),
            1 => hi - start,
            _ => start + len - lo,
        })
    };
    let (mut min_w, mut min_h) = min;
    if let Some(c) = keep {
        min_w = need(hx, b.x, b.width, c.x, c.x + c.width, min_w);
        min_h = need(hy, b.y, b.height, c.y, c.y + c.height, min_h);
    }
    let ratio = b.height / b.width;
    if aspect {
        let scale = match (hx, hy) {
            (0, _) => h / b.height,
            (_, 0) => w / b.width,
            _ => (w / b.width).max(h / b.height),
        };
        w = (b.width * scale).max(min_w).max(min_h / ratio);
    }
    let w = w.round().max(min_w.ceil());
    let h = if aspect {
        (w * ratio).round()
    } else {
        h.round()
    }
    .max(min_h.ceil());
    let place = |dir: i8, start: f64, old: f64, len: f64| match dir {
        _ if centred || dir == 0 => start + (old - len) / 2.0,
        -1 => start + old - len,
        _ => start,
    };
    Bounds {
        x: place(hx, b.x, b.width, w).round(),
        y: place(hy, b.y, b.height, h).round(),
        width: w,
        height: h,
    }
}

/// Nearest free spot around `center`; shapes that fully contain a candidate (pools, lanes,
/// expanded subprocesses) are containers, not obstacles.
pub(in crate::app) fn vacant_bounds(
    shapes: &[Shape],
    center: (f64, f64),
    width: f64,
    height: f64,
) -> Bounds {
    let mut offsets: Vec<(i32, i32)> = (-8..=8)
        .flat_map(|i| (-8..=8).map(move |j| (i, j)))
        .collect();
    offsets.sort_by_key(|(i, j)| i * i + j * j);
    let candidate = |(i, j): (i32, i32)| Bounds {
        x: center.0 - width / 2.0 + i as f64 * (width + 40.0),
        y: center.1 - height / 2.0 + j as f64 * (height + 40.0),
        width,
        height,
    };
    offsets
        .into_iter()
        .map(candidate)
        .find(|c| {
            shapes.iter().all(|shape| {
                let b = &shape.bounds;
                let contains = b.x <= c.x
                    && b.y <= c.y
                    && b.x + b.width >= c.x + c.width
                    && b.y + b.height >= c.y + c.height;
                contains
                    || c.x + c.width + 20.0 <= b.x
                    || b.x + b.width + 20.0 <= c.x
                    || c.y + c.height + 20.0 <= b.y
                    || b.y + b.height + 20.0 <= c.y
            })
        })
        .unwrap_or_else(|| candidate((0, 0)))
}

/// Pixels from the canvas border that start auto-scroll while dragging, and the scroll step
/// per frame (diagram-js AutoScroll: 20 and 10).
pub(super) const AUTO_SCROLL_BAND: f32 = 20.0;
pub(super) const AUTO_SCROLL_STEP: f32 = 10.0;

/// Screen-space camera motion for a drag at `pos` near the border of `rect`.
pub(super) fn auto_scroll_step(rect: Rect, pos: Pos2) -> Vec2 {
    let near = |distance: f32| distance > 0.0 && distance < AUTO_SCROLL_BAND;
    let axis = |low: f32, high: f32| {
        if near(low) {
            -AUTO_SCROLL_STEP
        } else if near(high) {
            AUTO_SCROLL_STEP
        } else {
            0.0
        }
    };
    Vec2::new(
        axis(pos.x - rect.left(), rect.right() - pos.x),
        axis(pos.y - rect.top(), rect.bottom() - pos.y),
    )
}

/// The texture to render for a canvas camera: the canvas plus an overscan margin on every side,
/// shrunk to stay within the raster limits (16384 px per axis, 64 MiB RGBA; with slack here).
pub(super) fn raster_view(
    camera: (f64, f64),
    zoom: f32,
    size: Vec2,
    pixels_per_point: f32,
) -> TextureView {
    let (w, h) = (size.x * pixels_per_point, size.y * pixels_per_point);
    let grow = (16000.0 / w)
        .min(16000.0 / h)
        .min((60.0 * 1024.0 * 1024.0 / 4.0 / (w * h)).sqrt());
    let margin = size * ((grow - 1.0) / 2.0).clamp(0.0, OVERSCAN);
    TextureView {
        camera: (
            camera.0 - (margin.x / zoom) as f64,
            camera.1 - (margin.y / zoom) as f64,
        ),
        zoom,
        size: size + margin * 2.0,
        request: (camera, zoom),
    }
}

/// Whether the canvas needs a new raster: none was asked for, the latest one does not cover the
/// canvas under this camera, the zoom left [`RESAMPLE_ZOOM`] of it, or input has settled on a
/// camera it was not rendered for (so the shown pixels are exact, not resampled).
pub(super) fn needs_raster(
    submitted: Option<TextureView>,
    camera: (f64, f64),
    zoom: f32,
    size: Vec2,
    settled: bool,
) -> bool {
    let Some(view) = submitted else {
        return true;
    };
    let edge = |origin: f64, extent: f32, zoom: f32| origin + (extent / zoom) as f64;
    let slack = 1e-3;
    let covers = view.camera.0 <= camera.0 + slack
        && view.camera.1 <= camera.1 + slack
        && edge(view.camera.0, view.size.x, view.zoom) + slack >= edge(camera.0, size.x, zoom)
        && edge(view.camera.1, view.size.y, view.zoom) + slack >= edge(camera.1, size.y, zoom);
    !covers
        || !RESAMPLE_ZOOM.contains(&(zoom / view.zoom))
        || (settled && view.request != (camera, zoom))
}

/// Overscan per side, as a fraction of the canvas size, before the raster limits apply.
pub(super) const OVERSCAN: f32 = 0.5;
/// Zoom ratios (current / rendered) served by scaling the texture instead of a new raster.
pub(super) const RESAMPLE_ZOOM: std::ops::RangeInclusive<f32> = 0.8..=1.25;
/// Input quiet time after which the canvas is re-rasterized at its exact scale.
pub(super) const SETTLE: std::time::Duration = std::time::Duration::from_millis(100);

/// Screen position of an edge's waypoints under `camera` and `zoom`.
pub(super) fn edge_screen(
    rect: Rect,
    camera: (f64, f64),
    zoom: f32,
    edge: &processcraft_core::model::Edge,
) -> Vec<Pos2> {
    edge.waypoints
        .iter()
        .map(|p| {
            Pos2::new(
                rect.left() + (p.x - camera.0) as f32 * zoom,
                rect.top() + (p.y - camera.1) as f32 * zoom,
            )
        })
        .collect()
}
