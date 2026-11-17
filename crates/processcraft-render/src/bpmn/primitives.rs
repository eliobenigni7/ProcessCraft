//! Path and stroke primitives: ellipses, rounded rects, polylines, clipping and dashing.

use crate::{DiagramScene, RenderError};
use processcraft_core::model::Point;
use tiny_skia::PathBuilder;

use super::appearance::{STROKE_ARROW, STROKE_OUTLINE};
use super::viewport::Viewport;

pub(super) fn cylinder(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    let r = (h * 0.12).min(10.0);
    p.move_to(x, y + r);
    p.cubic_to(x, y - r * 0.33, x + w, y - r * 0.33, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h + r * 0.33, x, y + h + r * 0.33, x, y + h - r);
    p.close();
}

pub(super) fn hexagon_points(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 7] {
    [
        (x + w * 0.25, y),
        (x + w * 0.75, y),
        (x + w, y + h * 0.5),
        (x + w * 0.75, y + h),
        (x + w * 0.25, y + h),
        (x, y + h * 0.5),
        (x + w * 0.25, y),
    ]
}

pub(super) fn polygon(p: &mut PathBuilder, points: &[(f32, f32)]) {
    p.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        p.line_to(x, y)
    }
    p.close()
}

pub(super) fn ring(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    let mut p = PathBuilder::new();
    ellipse(&mut p, x, y, w, h);
    if let Some(path) = p.finish() {
        outline(scene, &path, color, width)?;
    }
    Ok(())
}

pub(super) fn filled_ellipse(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    let mut p = PathBuilder::new();
    ellipse(&mut p, x, y, w, h);
    if let Some(path) = p.finish() {
        scene.add_path(path, color)?;
    }
    Ok(())
}

pub(super) fn polyline_midpoint(points: &[(f32, f32)]) -> Option<(f32, f32)> {
    polyline_midpoint_segment(points).map(|(point, _)| point)
}

/// Arc-length midpoint and the direction vector of the segment containing it.
pub(super) fn polyline_midpoint_segment(points: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    let total: f32 = points
        .windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
        .sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let mut remaining = total / 2.0;
    for pair in points.windows(2) {
        let length = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        if length > 0.0 && remaining <= length {
            let t = remaining / length;
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            return Some(((pair[0].0 + dx * t, pair[0].1 + dy * t), (dx, dy)));
        }
        remaining -= length;
    }
    None
}

pub(super) fn dashed_ellipse(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    for i in 0..16 {
        let mut pts = Vec::new();
        for j in 0..=3 {
            let a = (i as f32 + j as f32 / 6.0) * std::f32::consts::TAU / 16.0;
            pts.push((x + w * (1.0 + a.cos()) / 2.0, y + h * (1.0 + a.sin()) / 2.0));
        }
        lines(scene, &pts, color, STROKE_ARROW)?;
    }
    Ok(())
}

pub(super) fn ellipse(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    let k = 0.5522848;
    p.move_to(cx + rx, cy);
    p.cubic_to(cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry);
    p.cubic_to(cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy);
    p.cubic_to(cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry);
    p.cubic_to(cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy);
    p.close();
}

pub(super) fn outline(
    scene: &mut DiagramScene,
    path: &tiny_skia::Path,
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    let stroke = tiny_skia::Stroke {
        width,
        ..Default::default()
    };
    let p = path
        .stroke(&stroke, STROKE_OUTLINE)
        .ok_or(RenderError("Invalid stroke"))?;
    scene.add_path(p, color)
}

pub(super) fn lines(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let mut p = PathBuilder::new();
    p.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        p.line_to(x, y)
    }
    if let Some(path) = p.finish() {
        outline(scene, &path, color, width)?
    }
    Ok(())
}

pub(super) fn dashed_lines(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    dash: f32,
) -> Result<(), RenderError> {
    stroke_polyline(scene, points, color, 0.0, Some(dash), 0.0)
}

/// Corner radius of edge polylines.
pub(super) const EDGE_RADIUS: f32 = 5.0;

/// One path along `points` whose corners are rounded by `radius`, at most half of each
/// adjacent segment; repeated points are skipped.
pub(super) fn polyline_path(points: &[(f32, f32)], radius: f32) -> Option<tiny_skia::Path> {
    let mut points = points.to_vec();
    points.dedup();
    let (first, last) = (*points.first()?, *points.last()?);
    let mut path = PathBuilder::new();
    path.move_to(first.0, first.1);
    for w in points.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let (l1, l2) = ((b.0 - a.0).hypot(b.1 - a.1), (c.0 - b.0).hypot(c.1 - b.1));
        let r = radius.min(l1 / 2.0).min(l2 / 2.0);
        if r > 0.0 {
            path.line_to(b.0 - (b.0 - a.0) * r / l1, b.1 - (b.1 - a.1) * r / l1);
            path.quad_to(
                b.0,
                b.1,
                b.0 + (c.0 - b.0) * r / l2,
                b.1 + (c.1 - b.1) * r / l2,
            );
        } else {
            path.line_to(b.0, b.1);
        }
    }
    path.line_to(last.0, last.1);
    path.finish()
}

/// Stroke [`polyline_path`] 1.5 wide with round joins, optionally dashed (`dash` on, `dash`
/// off) continuously along the whole line, the pattern starting `offset` units in.
pub(super) fn stroke_polyline(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    radius: f32,
    dash: Option<f32>,
    offset: f32,
) -> Result<(), RenderError> {
    let Some(mut path) = polyline_path(points, radius) else {
        return Ok(());
    };
    if let Some(dash) = dash {
        // At most 100 000 dashes per line keeps the stroked outline within the scene budget.
        let length: f32 = points
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        if length.is_nan() || length / (dash * 2.0) > 100_000.0 {
            return Err(RenderError("Dashed edge exceeds complexity limit"));
        }
        let pattern = tiny_skia::StrokeDash::new(vec![dash, dash], offset)
            .ok_or(RenderError("Invalid dash"))?;
        let Some(dashed) = path.dash(&pattern, 1.0) else {
            return Ok(());
        };
        path = dashed;
    }
    let stroke = tiny_skia::Stroke {
        width: 1.5,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    let p = path
        .stroke(&stroke, STROKE_OUTLINE)
        .ok_or(RenderError("Invalid stroke"))?;
    scene.add_path(p, color)
}

/// The parts of `waypoints` inside `view`, in scene coordinates, each with the route distance
/// where it starts (the dash phase). A new run starts only where clipping breaks the route.
pub(super) fn clipped_runs(
    waypoints: &[Point],
    view: Viewport,
    origin: (f64, f64),
) -> Vec<(Vec<(f32, f32)>, f32)> {
    let local = |(x, y): (f64, f64)| {
        (
            (x - view.x - origin.0) as f32,
            (y - view.y - origin.1) as f32,
        )
    };
    let mut runs: Vec<(Vec<(f32, f32)>, f32)> = Vec::new();
    let (mut travelled, mut open) = (0.0f64, false);
    for pair in waypoints.windows(2) {
        let (s, e) = ((pair[0].x, pair[0].y), (pair[1].x, pair[1].y));
        if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
            match runs.last_mut() {
                Some((run, _)) if open && a == s => run.push(local(b)),
                _ => runs.push((
                    vec![local(a), local(b)],
                    (travelled + (a.0 - s.0).hypot(a.1 - s.1)) as f32,
                )),
            }
            open = b == e;
        } else {
            open = false;
        }
        travelled += (e.0 - s.0).hypot(e.1 - s.1);
    }
    runs
}

pub(super) fn rounded(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    rounded_corners(p, x, y, w, h, rounding(w, h), [true; 4]);
}

/// Corner radius of a rounded activity outline.
pub(super) fn rounding(w: f32, h: f32) -> f32 {
    10.0_f32.min(w / 2.0).min(h / 2.0)
}

/// Rectangle with the selected corners (top-left, top-right, bottom-right,
/// bottom-left) rounded by `r`.
pub(super) fn rounded_corners(
    p: &mut PathBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    c: [bool; 4],
) {
    let k = 0.5522848;
    let [tl, tr, br, bl] = c.map(|on| {
        if on {
            r.min(w / 2.0).min(h).max(0.0)
        } else {
            0.0
        }
    });
    p.move_to(x + tl, y);
    p.line_to(x + w - tr, y);
    if tr > 0.0 {
        p.cubic_to(
            x + w - tr + k * tr,
            y,
            x + w,
            y + tr - k * tr,
            x + w,
            y + tr,
        );
    }
    p.line_to(x + w, y + h - br);
    if br > 0.0 {
        p.cubic_to(
            x + w,
            y + h - br + k * br,
            x + w - br + k * br,
            y + h,
            x + w - br,
            y + h,
        );
    }
    p.line_to(x + bl, y + h);
    if bl > 0.0 {
        p.cubic_to(
            x + bl - k * bl,
            y + h,
            x,
            y + h - bl + k * bl,
            x,
            y + h - bl,
        );
    }
    p.line_to(x, y + tl);
    if tl > 0.0 {
        p.cubic_to(x, y + tl - k * tl, x + tl - k * tl, y, x + tl, y);
    }
    p.close();
}

pub(super) fn clip_segment(a: &Point, b: &Point, v: Viewport) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut lo: f64 = 0.0;
    let mut hi: f64 = 1.0;
    for (p, q) in [
        (-dx, a.x - v.x),
        (dx, v.x + v.width - a.x),
        (-dy, a.y - v.y),
        (dy, v.y + v.height - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                lo = lo.max(r)
            } else {
                hi = hi.min(r)
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some((
        (a.x + lo * dx, a.y + lo * dy),
        (a.x + hi * dx, a.y + hi * dy),
    ))
}
