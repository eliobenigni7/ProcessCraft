//! Task/event/gateway/choreography markers, loop markers, envelopes and flow-source markers.

use crate::{DiagramScene, RenderError};
use processcraft_core::{
    bpmn_collaboration::ChoreographyLoopType,
    bpmn_model::{EventDefinition, EventGatewayType, LoopCharacteristics, NodeDetails},
    ids::ElementSelector,
    model::NodeKind,
};
use tiny_skia::PathBuilder;

use super::appearance::{
    NON_INITIATING_FILL, STROKE_ARROW, STROKE_HAIRLINE, STROKE_HEAVY, STROKE_ICON,
};
use super::labels::last_segment;
use super::primitives::{dashed_ellipse, filled_ellipse, lines, polygon, ring};
use super::semantics::SemanticIndex;

pub(super) fn draw_default_slash(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let a = points[0];
    let b = points[1];
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = dx.hypot(dy);
    if length <= 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / length, dy / length);
    let distance = (length * 0.35).min(12.0);
    let center = (a.0 + ux * distance, a.1 + uy * distance);
    let (sx, sy) = ((ux - uy) * 4.0, (uy + ux) * 4.0);
    lines(
        scene,
        &[
            (center.0 - sx, center.1 - sy),
            (center.0 + sx, center.1 + sy),
        ],
        stroke,
        1.5,
    )
}

pub(super) fn draw_flow_source_marker(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
    background: [u8; 3],
    message: bool,
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let a = points[0];
    let dx = points[1].0 - a.0;
    let dy = points[1].1 - a.1;
    let length = dx.hypot(dy);
    if length == 0.0 {
        return Ok(());
    }
    if message {
        filled_ellipse(scene, a.0 - 4.0, a.1 - 4.0, 8.0, 8.0, background)?;
        return ring(scene, a.0 - 4.0, a.1 - 4.0, 8.0, 8.0, stroke, STROKE_ICON);
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy * 4.0, ux * 4.0);
    let mid = (a.0 + ux * 6.0, a.1 + uy * 6.0);
    let end = (a.0 + ux * 12.0, a.1 + uy * 12.0);
    let mut interior = PathBuilder::new();
    polygon(
        &mut interior,
        &[a, (mid.0 + nx, mid.1 + ny), end, (mid.0 - nx, mid.1 - ny)],
    );
    if let Some(path) = interior.finish() {
        scene.add_path(path, background)?;
    }
    lines(
        scene,
        &[
            a,
            (mid.0 + nx, mid.1 + ny),
            end,
            (mid.0 - nx, mid.1 - ny),
            a,
        ],
        stroke,
        1.2,
    )
}

pub(super) fn open_arrow(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
) -> Result<(), RenderError> {
    if let Some((a, b)) = last_segment(points) {
        let angle = (b.1 - a.1).atan2(b.0 - a.0);
        let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
        lines(scene, &[wing(0.5), b, wing(-0.5)], stroke, STROKE_ARROW)?;
    }
    Ok(())
}

pub(super) fn double_link(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
) -> Result<(), RenderError> {
    for pair in points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        let length = dx.hypot(dy);
        if length == 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / length * 2.0, dx / length * 2.0);
        for sign in [-1.0, 1.0] {
            lines(
                scene,
                &[
                    (pair[0].0 + nx * sign, pair[0].1 + ny * sign),
                    (pair[1].0 + nx * sign, pair[1].1 + ny * sign),
                ],
                color,
                1.1,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_new_markers(
    scene: &mut DiagramScene,
    kind: Option<NodeKind>,
    details: Option<&NodeDetails>,
    effective: Option<(Vec<&EventDefinition>, bool)>,
    instantiate: bool,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: [u8; 3],
    fill: [u8; 3],
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    match kind {
        Some(
            NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent,
        ) => {
            let inset = (w.min(h) * 0.10).max(2.0);
            if matches!(kind, Some(NodeKind::BoundaryEvent)) && non_interrupting(details) {
                dashed_ellipse(
                    scene,
                    x + inset,
                    y + inset,
                    (w - 2.0 * inset).max(0.0),
                    (h - 2.0 * inset).max(0.0),
                    stroke,
                )?;
            } else {
                ring(
                    scene,
                    x + inset,
                    y + inset,
                    (w - 2.0 * inset).max(0.0),
                    (h - 2.0 * inset).max(0.0),
                    stroke,
                    1.2,
                )?;
            }
        }
        Some(NodeKind::InclusiveGateway) => {
            let r = w.min(h) * 0.19;
            ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, 1.6)?;
        }
        Some(NodeKind::ComplexGateway) => {
            let r = w.min(h) * 0.2;
            for angle in [
                0.0_f32,
                std::f32::consts::FRAC_PI_4,
                std::f32::consts::FRAC_PI_2,
                3.0 * std::f32::consts::FRAC_PI_4,
            ] {
                let (s, c) = angle.sin_cos();
                lines(
                    scene,
                    &[(cx - r * c, cy - r * s), (cx + r * c, cy + r * s)],
                    stroke,
                    2.0,
                )?;
            }
        }
        Some(NodeKind::EventBasedGateway) => {
            let (instantiate, parallel) = match details {
                Some(NodeDetails::Gateway {
                    instantiate,
                    event_gateway_type,
                    ..
                }) => (
                    instantiate.unwrap_or(false),
                    *event_gateway_type == Some(EventGatewayType::Parallel),
                ),
                _ => (false, false),
            };
            let r = w.min(h) * 0.25;
            ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, STROKE_ICON)?;
            if !instantiate {
                ring(
                    scene,
                    cx - r * 0.82,
                    cy - r * 0.82,
                    1.64 * r,
                    1.64 * r,
                    stroke,
                    1.0,
                )?;
            }
            if parallel {
                lines(
                    scene,
                    &[(cx - r * 0.55, cy), (cx + r * 0.55, cy)],
                    stroke,
                    1.6,
                )?;
                lines(
                    scene,
                    &[(cx, cy - r * 0.55), (cx, cy + r * 0.55)],
                    stroke,
                    1.6,
                )?;
            } else {
                let points: Vec<_> = (0..=5)
                    .map(|i| {
                        let a =
                            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
                        (cx + a.cos() * r * 0.55, cy + a.sin() * r * 0.55)
                    })
                    .collect();
                lines(scene, &points, stroke, STROKE_ICON)?;
            }
        }
        Some(NodeKind::ServiceTask) => {
            let (mx, my) = (x + 14.0, y + 13.0);
            ring(scene, mx - 4.0, my - 4.0, 8.0, 8.0, stroke, STROKE_ICON)?;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                lines(
                    scene,
                    &[
                        (mx + a.cos() * 4.0, my + a.sin() * 4.0),
                        (mx + a.cos() * 7.0, my + a.sin() * 7.0),
                    ],
                    stroke,
                    1.8,
                )?;
            }
        }
        Some(NodeKind::SendTask | NodeKind::ReceiveTask) => {
            if matches!(kind, Some(NodeKind::ReceiveTask)) && instantiate {
                ring(scene, x + 3.0, y + 2.0, 22.0, 22.0, stroke, STROKE_ICON)?;
            }
            let pts = [
                (x + 7.0, y + 8.0),
                (x + 21.0, y + 8.0),
                (x + 21.0, y + 18.0),
                (x + 7.0, y + 18.0),
                (x + 7.0, y + 8.0),
            ];
            if matches!(kind, Some(NodeKind::SendTask)) {
                let mut p = PathBuilder::new();
                polygon(&mut p, &pts);
                if let Some(p) = p.finish() {
                    scene.add_path(p, stroke)?;
                }
                lines(
                    scene,
                    &[
                        (x + 7.0, y + 8.0),
                        (x + 14.0, y + 13.0),
                        (x + 21.0, y + 8.0),
                    ],
                    fill,
                    1.0,
                )?;
            } else {
                lines(scene, &pts, stroke, STROKE_ICON)?;
                lines(
                    scene,
                    &[
                        (x + 7.0, y + 8.0),
                        (x + 14.0, y + 13.0),
                        (x + 21.0, y + 8.0),
                    ],
                    stroke,
                    1.0,
                )?;
            }
        }
        Some(NodeKind::ScriptTask) => {
            lines(
                scene,
                &[
                    (x + 8.0, y + 7.0),
                    (x + 19.0, y + 7.0),
                    (x + 19.0, y + 19.0),
                    (x + 8.0, y + 19.0),
                    (x + 8.0, y + 7.0),
                ],
                stroke,
                1.2,
            )?;
            for yy in [10.0, 13.0, 16.0] {
                lines(
                    scene,
                    &[(x + 10.0, y + yy), (x + 17.0, y + yy)],
                    stroke,
                    0.8,
                )?;
            }
        }
        Some(NodeKind::BusinessRuleTask) => {
            lines(
                scene,
                &[
                    (x + 7.0, y + 7.0),
                    (x + 21.0, y + 7.0),
                    (x + 21.0, y + 19.0),
                    (x + 7.0, y + 19.0),
                    (x + 7.0, y + 7.0),
                ],
                stroke,
                1.2,
            )?;
            for yy in [11.0, 15.0] {
                lines(
                    scene,
                    &[(x + 7.0, y + yy), (x + 21.0, y + yy)],
                    stroke,
                    STROKE_HAIRLINE,
                )?;
            }
            lines(
                scene,
                &[(x + 14.0, y + 7.0), (x + 14.0, y + 19.0)],
                stroke,
                0.8,
            )?;
        }
        _ => {}
    }
    if let Some((definitions, parallel_multiple)) = effective {
        if definitions.len() > 1 && parallel_multiple {
            let r = w.min(h) * 0.26;
            lines(scene, &[(cx - r, cy), (cx + r, cy)], stroke, STROKE_HEAVY)?;
            lines(scene, &[(cx, cy - r), (cx, cy + r)], stroke, STROKE_HEAVY)?;
        } else if definitions.len() > 1 {
            let r = w.min(h) * 0.26;
            let pts: Vec<_> = (0..=5)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
                    (cx + a.cos() * r, cy + a.sin() * r)
                })
                .collect();
            if matches!(
                kind,
                Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
            ) {
                let mut path = PathBuilder::new();
                polygon(&mut path, &pts);
                if let Some(path) = path.finish() {
                    scene.add_path(path, stroke)?;
                }
            } else {
                lines(scene, &pts, stroke, STROKE_ICON)?;
            }
        }
        for definition in definitions.iter().filter(|_| definitions.len() == 1) {
            match definition {
                EventDefinition::Message { .. } => {
                    let r = w.min(h) * 0.25;
                    let pts = [
                        (cx - r, cy - r * 0.65),
                        (cx + r, cy - r * 0.65),
                        (cx + r, cy + r * 0.65),
                        (cx - r, cy + r * 0.65),
                        (cx - r, cy - r * 0.65),
                    ];
                    let throwing = matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    );
                    if throwing {
                        let mut path = PathBuilder::new();
                        polygon(&mut path, &pts);
                        if let Some(path) = path.finish() {
                            scene.add_path(path, stroke)?;
                        }
                    } else {
                        lines(scene, &pts, stroke, 1.1)?;
                    }
                    lines(
                        scene,
                        &[(cx - r, cy - r * 0.65), (cx, cy), (cx + r, cy - r * 0.65)],
                        if throwing { fill } else { stroke },
                        1.0,
                    )?;
                }
                EventDefinition::Signal { .. } => {
                    let r = w.min(h) * 0.25;
                    let pts = [
                        (cx, cy - r),
                        (cx + r, cy + r),
                        (cx - r, cy + r),
                        (cx, cy - r),
                    ];
                    if matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    ) {
                        let mut path = PathBuilder::new();
                        polygon(&mut path, &pts);
                        if let Some(path) = path.finish() {
                            scene.add_path(path, stroke)?;
                        }
                    } else {
                        lines(scene, &pts, stroke, STROKE_ICON)?;
                    }
                }
                EventDefinition::Error { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx - r * 0.75, cy - r),
                        (cx + r * 0.30, cy - r * 0.12),
                        (cx - r * 0.05, cy - r * 0.12),
                        (cx + r * 0.75, cy + r),
                        (cx - r * 0.35, cy + r * 0.12),
                        (cx + r * 0.05, cy + r * 0.12),
                        (cx - r * 0.75, cy - r),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Escalation { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx, cy - r),
                        (cx + r, cy + r),
                        (cx, cy + r * 0.35),
                        (cx - r, cy + r),
                        (cx, cy - r),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Conditional { .. } => {
                    let r = w.min(h) * 0.24;
                    let pts = [
                        (cx - r * 0.7, cy - r),
                        (cx + r * 0.7, cy - r),
                        (cx + r * 0.7, cy + r),
                        (cx - r * 0.7, cy + r),
                        (cx - r * 0.7, cy - r),
                    ];
                    lines(scene, &pts, stroke, 1.1)?;
                    for dy in [-0.45_f32, 0.0, 0.45] {
                        lines(
                            scene,
                            &[(cx - r * 0.48, cy + r * dy), (cx + r * 0.48, cy + r * dy)],
                            stroke,
                            0.8,
                        )?;
                    }
                }
                EventDefinition::Link { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx - r, cy - r * 0.45),
                        (cx + r * 0.1, cy - r * 0.45),
                        (cx + r * 0.1, cy - r),
                        (cx + r, cy),
                        (cx + r * 0.1, cy + r),
                        (cx + r * 0.1, cy + r * 0.45),
                        (cx - r, cy + r * 0.45),
                        (cx - r, cy - r * 0.45),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Cancel { .. } => {
                    let r = w.min(h) * 0.21;
                    let t = r * 0.32;
                    let pts = [
                        (cx - r, cy - r + t),
                        (cx - r + t, cy - r),
                        (cx, cy - t),
                        (cx + r - t, cy - r),
                        (cx + r, cy - r + t),
                        (cx + t, cy),
                        (cx + r, cy + r - t),
                        (cx + r - t, cy + r),
                        (cx, cy + t),
                        (cx - r + t, cy + r),
                        (cx - r, cy + r - t),
                        (cx - t, cy),
                        (cx - r, cy - r + t),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(kind, Some(NodeKind::EndEvent)),
                    )?;
                }
                EventDefinition::Compensate { .. } => {
                    let r = w.min(h) * 0.24;
                    let left = [(cx - r, cy), (cx, cy - r), (cx, cy + r), (cx - r, cy)];
                    let right = [(cx, cy), (cx + r, cy - r), (cx + r, cy + r), (cx, cy)];
                    let filled = matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    );
                    marker_polygon(scene, &left, stroke, filled)?;
                    marker_polygon(scene, &right, stroke, filled)?;
                }
                EventDefinition::Timer { .. } => {
                    let r = w.min(h) * 0.27;
                    ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, STROKE_ICON)?;
                    for i in 0..12 {
                        let a = i as f32 * std::f32::consts::TAU / 12.0;
                        lines(
                            scene,
                            &[
                                (cx + a.cos() * r * 0.75, cy + a.sin() * r * 0.75),
                                (cx + a.cos() * r * 0.95, cy + a.sin() * r * 0.95),
                            ],
                            stroke,
                            0.8,
                        )?;
                    }
                    lines(
                        scene,
                        &[(cx, cy - r * 0.62), (cx, cy), (cx + r * 0.48, cy)],
                        stroke,
                        1.1,
                    )?;
                }
                EventDefinition::Terminate { .. } if matches!(kind, Some(NodeKind::EndEvent)) => {
                    let r = w.min(h) * 0.29;
                    filled_ellipse(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke)?;
                }
                _ => diagnostics.push(format!(
                    "Event definition {:?} cannot be rendered",
                    definition
                )),
            }
        }
    }
    Ok(())
}

pub(super) fn envelope(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    stroke: [u8; 3],
    filled: bool,
    background: [u8; 3],
) -> Result<(), RenderError> {
    let fill = if filled {
        NON_INITIATING_FILL
    } else {
        background
    };
    scene.add_background_rect(x - 10.0, y - 6.0, 20.0, 12.0, fill)?;
    let line = stroke;
    lines(
        scene,
        &[
            (x - 10.0, y - 6.0),
            (x + 10.0, y - 6.0),
            (x + 10.0, y + 6.0),
            (x - 10.0, y + 6.0),
            (x - 10.0, y - 6.0),
        ],
        stroke,
        1.0,
    )?;
    lines(
        scene,
        &[(x - 10.0, y - 6.0), (x, y + 1.0), (x + 10.0, y - 6.0)],
        line,
        1.0,
    )
}

pub(super) fn marker_polygon(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    filled: bool,
) -> Result<(), RenderError> {
    if filled {
        let mut path = PathBuilder::new();
        polygon(&mut path, points);
        if let Some(path) = path.finish() {
            scene.add_path(path, color)?;
        }
    } else {
        lines(scene, points, color, STROKE_ICON)?;
    }
    Ok(())
}

pub(super) fn self_reference_resolves(
    index: &SemanticIndex<'_>,
    reference: &processcraft_core::bpmn_activity::ModelReference,
    id: &ElementSelector,
) -> bool {
    index.resolved(&Some(reference.clone())).as_ref() == Some(id)
}

pub(super) fn draw_choreography_loop(
    scene: &mut DiagramScene,
    marker: ChoreographyLoopType,
    cx: f32,
    cy: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    if marker == ChoreographyLoopType::Standard {
        let mut points = Vec::new();
        for i in 0..=16 {
            let a = (-0.15 + i as f32 / 16.0 * 1.65) * std::f32::consts::PI;
            points.push((cx + a.cos() * 5.0, cy + a.sin() * 5.0));
        }
        lines(scene, &points, color, 1.3)?;
        let tip = points[16];
        lines(
            scene,
            &[(tip.0 - 3.0, tip.1 - 1.0), tip, (tip.0 - 1.0, tip.1 + 3.0)],
            color,
            1.2,
        )?;
    } else {
        for offset in [-3.5_f32, 0.0, 3.5] {
            let points = if marker == ChoreographyLoopType::MultiInstanceSequential {
                [(cx - 5.0, cy + offset), (cx + 5.0, cy + offset)]
            } else {
                [(cx + offset, cy - 5.0), (cx + offset, cy + 5.0)]
            };
            lines(scene, &points, color, 1.4)?;
        }
    }
    Ok(())
}

pub(super) fn draw_loop_marker(
    scene: &mut DiagramScene,
    marker: &LoopCharacteristics,
    cx: f32,
    cy: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    match marker {
        LoopCharacteristics::Standard { .. } => {
            let mut points = Vec::new();
            for i in 0..=16 {
                let a = (-0.15 + i as f32 / 16.0 * 1.65) * std::f32::consts::PI;
                points.push((cx + a.cos() * 5.0, cy + a.sin() * 5.0));
            }
            lines(scene, &points, color, 1.3)?;
            let tip = *points.last().expect("non-empty arc");
            lines(
                scene,
                &[(tip.0 - 3.0, tip.1 - 1.0), tip, (tip.0 - 1.0, tip.1 + 3.0)],
                color,
                1.2,
            )?;
        }
        LoopCharacteristics::MultiInstance { is_sequential, .. } => {
            for offset in [-3.5_f32, 0.0, 3.5] {
                if *is_sequential == Some(true) {
                    lines(
                        scene,
                        &[(cx - 5.0, cy + offset), (cx + 5.0, cy + offset)],
                        color,
                        1.4,
                    )?;
                } else {
                    lines(
                        scene,
                        &[(cx + offset, cy - 5.0), (cx + offset, cy + 5.0)],
                        color,
                        1.4,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn non_interrupting(details: Option<&NodeDetails>) -> bool {
    matches!(
        details,
        Some(NodeDetails::BoundaryEvent {
            cancel_activity: false,
            ..
        }) | Some(NodeDetails::Event {
            cancel_activity: Some(false),
            ..
        })
    )
}

#[cfg(test)]
mod source_marker_tests {
    use super::*;
    use crate::bpmn::primitives::polyline_midpoint;
    #[test]
    fn message_envelope_uses_arc_midpoint_and_bounded_vector() {
        let midpoint = polyline_midpoint(&[(0.0, 0.0), (80.0, 0.0), (80.0, 20.0)]).unwrap();
        assert_eq!(midpoint, (50.0, 0.0));
        let mut scene = DiagramScene::new(100.0, 100.0).unwrap();
        envelope(
            &mut scene,
            midpoint.0,
            midpoint.1 + 20.0,
            [1, 2, 3],
            true,
            [255, 255, 255],
        )
        .unwrap();
        let paths: Vec<_> = scene.paths().collect();
        assert_eq!(paths[0].color, [225, 225, 225]);
        assert_eq!(
            (
                paths[0].path.bounds().left(),
                paths[0].path.bounds().right()
            ),
            (40.0, 60.0)
        );
        assert_eq!(
            (
                paths[0].path.bounds().top(),
                paths[0].path.bounds().bottom()
            ),
            (14.0, 26.0)
        );
    }
    #[test]
    fn explicit_paint_rank_interleaves_paths_and_glyphs() {
        let mut scene = DiagramScene::new(20.0, 20.0).unwrap();
        scene
            .add_background_rect(0.0, 0.0, 20.0, 20.0, [255, 255, 255])
            .unwrap();
        scene.set_paint_rank(2);
        scene
            .add_background_rect(1.0, 1.0, 10.0, 10.0, [255, 0, 0])
            .unwrap();
        scene.set_paint_rank(1);
        scene
            .add_background_rect(1.0, 1.0, 10.0, 10.0, [0, 0, 255])
            .unwrap();
        assert_eq!(
            scene.paths().map(|p| p.color).collect::<Vec<_>>(),
            vec![[255, 255, 255], [0, 0, 255], [255, 0, 0]]
        );
    }
    #[test]
    fn source_markers_have_expected_bounds_and_distinct_outlines() {
        let mut diamond = DiagramScene::new(100.0, 100.0).unwrap();
        draw_flow_source_marker(
            &mut diamond,
            &[(20.0, 30.0), (60.0, 30.0)],
            [0, 0, 0],
            [255, 255, 255],
            false,
        )
        .unwrap();
        let paths: Vec<_> = diamond.paths().collect();
        assert_eq!(paths.len(), 2);
        let bounds = paths[1].path.bounds();
        assert!(bounds.left() <= 20.0 && bounds.right() >= 32.0);
        assert!(bounds.top() <= 26.0 && bounds.bottom() >= 34.0);
        let mut circle = DiagramScene::new(100.0, 100.0).unwrap();
        draw_flow_source_marker(
            &mut circle,
            &[(20.0, 30.0), (60.0, 30.0)],
            [0, 0, 0],
            [255, 255, 255],
            true,
        )
        .unwrap();
        let paths: Vec<_> = circle.paths().collect();
        assert_eq!(paths.len(), 2);
        let bounds = paths[1].path.bounds();
        assert!(bounds.left() <= 16.0 && bounds.right() >= 24.0);
        assert!(bounds.top() <= 26.0 && bounds.bottom() >= 34.0);
    }
}
