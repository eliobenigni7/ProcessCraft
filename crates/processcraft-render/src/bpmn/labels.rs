//! Label layout, label fonts and label drawing for shapes, edges and messages.

use crate::{DiagramScene, RenderError, TextEngine, TextRequest};
use processcraft_core::{
    ids::ElementSelector,
    model::{Bounds, BpmnDocument, Node, NodeKind, Style},
};

use super::HEADER;
use super::appearance::color;
use super::primitives::{lines, polyline_midpoint_segment};
use super::semantics::SemanticIndex;

/// BPMN DI: a shape or edge without a BPMNLabel still shows its name at the
/// tool's default position, so the explicit-label policy never hides a name.
pub(super) fn label_visible(
    _label: Option<&processcraft_core::model::Label>,
    _policy: Option<processcraft_core::model::DiLabelVisibility>,
) -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LabelAlign {
    TopLeft,
    TopCenter,
    Center,
}

/// Events, gateways and data references carry their name below the shape.
pub(super) fn has_external_label(kind: Option<NodeKind>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
                | NodeKind::ExclusiveGateway
                | NodeKind::ParallelGateway
                | NodeKind::InclusiveGateway
                | NodeKind::ComplexGateway
                | NodeKind::EventBasedGateway
                | NodeKind::DataObjectReference
                | NodeKind::DataStoreReference
        )
    )
}

/// Default external label box: centered under the shape, room for three lines.
pub(super) fn external_label_bounds(b: &Bounds) -> Bounds {
    let width = b.width.max(100.0);
    Bounds {
        x: b.x + (b.width - width) / 2.0,
        y: b.y + b.height + 2.0,
        width,
        height: 64.0,
    }
}

/// Authored DI label bounds, or the default placement for the shape kind.
pub(super) fn shape_label_layout(
    shape: &processcraft_core::model::Shape,
    kind: Option<NodeKind>,
    marker_kind: Option<NodeKind>,
    node: Option<&Node>,
    choreography: bool,
    is_lane_or_pool: bool,
) -> (Bounds, LabelAlign) {
    let b = &shape.bounds;
    if let Some(bounds) = shape.label.as_ref().and_then(|l| l.bounds.as_ref()) {
        let align = if has_external_label(kind) {
            LabelAlign::TopCenter
        } else {
            LabelAlign::TopLeft
        };
        return (bounds.clone(), align);
    }
    if is_lane_or_pool {
        return if shape.is_horizontal == Some(false) {
            (
                Bounds {
                    height: b.height.min(HEADER),
                    ..b.clone()
                },
                LabelAlign::TopLeft,
            )
        } else {
            (
                Bounds {
                    width: b.width.min(HEADER),
                    ..b.clone()
                },
                LabelAlign::Center,
            )
        };
    }
    if choreography {
        return (
            Bounds {
                x: b.x,
                y: b.y + b.height * 0.25,
                width: b.width,
                height: b.height * 0.5,
            },
            LabelAlign::Center,
        );
    }
    let bottom_markers = node.is_some_and(|node| {
        node.loop_characteristics.is_some()
            || node
                .activity
                .as_ref()
                .is_some_and(|activity| activity.is_for_compensation == Some(true))
    });
    if is_task_kind(marker_kind) {
        let top = if matches!(marker_kind, Some(NodeKind::Task)) {
            0.0
        } else {
            20.0
        };
        let bottom = if bottom_markers { 18.0 } else { 0.0 };
        return (
            Bounds {
                x: b.x,
                y: b.y + top,
                width: b.width,
                height: (b.height - top - bottom).max(0.0),
            },
            LabelAlign::Center,
        );
    }
    if has_external_label(kind) {
        return (external_label_bounds(b), LabelAlign::TopCenter);
    }
    if matches!(
        kind,
        Some(
            NodeKind::SubProcess
                | NodeKind::AdHocSubProcess
                | NodeKind::Transaction
                | NodeKind::CallActivity
        )
    ) && shape.is_expanded != Some(true)
    {
        // Collapsed: center like a task, above the collapse/loop marker row.
        return (
            Bounds {
                height: (b.height - 18.0).max(0.0),
                ..b.clone()
            },
            LabelAlign::Center,
        );
    }
    (b.clone(), LabelAlign::TopLeft)
}

/// Default edge label: below a horizontal midpoint segment, beside a vertical one.
pub(super) fn edge_label_layout(
    edge: &processcraft_core::model::Edge,
) -> Option<(Bounds, LabelAlign)> {
    if let Some(bounds) = edge.label.as_ref().and_then(|l| l.bounds.as_ref()) {
        return Some((bounds.clone(), LabelAlign::TopCenter));
    }
    let first = edge.waypoints.first()?;
    let points: Vec<_> = edge
        .waypoints
        .iter()
        .map(|p| (p.x as f32, p.y as f32))
        .collect();
    let ((mx, my), (dx, dy)) = polyline_midpoint_segment(&points)
        .unwrap_or(((first.x as f32, first.y as f32), (1.0, 0.0)));
    let (mx, my) = (f64::from(mx), f64::from(my));
    Some(if dx.abs() >= dy.abs() {
        (
            Bounds {
                x: mx - 60.0,
                y: my + 2.0,
                width: 120.0,
                height: 40.0,
            },
            LabelAlign::TopCenter,
        )
    } else {
        (
            Bounds {
                x: mx + 4.0,
                y: my - 20.0,
                width: 120.0,
                height: 40.0,
            },
            LabelAlign::TopLeft,
        )
    })
}

/// Last segment with non-zero length, so coincident end waypoints keep a defined angle.
pub(super) fn last_segment(points: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    points
        .windows(2)
        .rev()
        .find(|p| p[0] != p[1])
        .map(|p| (p[0], p[1]))
}

pub(super) fn label_font<'a>(
    document: &BpmnDocument,
    diagram: &'a processcraft_core::model::Diagram,
    label: Option<&processcraft_core::model::Label>,
    diagnostics: &mut Vec<String>,
) -> Option<&'a processcraft_core::model::DiFont> {
    use processcraft_core::model::DiStyleRef;
    let reference = label.and_then(|label| label.style_ref.as_ref())?;
    let style = diagram.label_styles.iter().find(|style| {
        style.id.as_ref().is_some_and(|id| match reference {
            DiStyleRef::Local(selector) => id == selector,
            DiStyleRef::Qualified(qname) => {
                document
                    .definitions
                    .as_ref()
                    .is_some_and(|definitions| definitions.target_namespace == qname.namespace)
                    && id.xml_id() == Some(&qname.local)
            }
        })
    });
    match style {
        Some(style) => match style.font.as_ref() {
            Some(font) => Some(font),
            None => {
                diagnostics.push("Label style has no font; using default".into());
                None
            }
        },
        None => {
            diagnostics.push("Label style is unresolved; using default".into());
            None
        }
    }
}

pub(super) fn is_task_kind(kind: Option<NodeKind>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::Task
                | NodeKind::UserTask
                | NodeKind::ManualTask
                | NodeKind::ServiceTask
                | NodeKind::SendTask
                | NodeKind::ReceiveTask
                | NodeKind::ScriptTask
                | NodeKind::BusinessRuleTask
        )
    )
}

pub(super) fn group_label_collision(
    bounds: &Bounds,
    shape_id: &ElementSelector,
    diagram: &processcraft_core::model::Diagram,
    semantics: &SemanticIndex<'_>,
) -> bool {
    diagram
        .shapes
        .iter()
        .filter(|shape| {
            &shape.id != shape_id
                && semantics.node(&shape.element).is_some()
                && !semantics.shape_visibility(shape, diagram).0
        })
        .any(|shape| {
            let b = &shape.bounds;
            bounds.x < b.x + b.width
                && bounds.x + bounds.width > b.x
                && bounds.y < b.y + b.height
                && bounds.y + bounds.height > b.y
        })
}

#[allow(clippy::too_many_arguments)]
/// Message name beside an envelope decorator (BPMN 2.0.2 section 12.3.2 BPMNShape,
/// Choreography Participant Bands: "The name of the message may be used as a label").
pub(super) fn message_name_label(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    center: (f32, f32),
    origin: (f64, f64),
    fg: [u8; 3],
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let bounds = Bounds {
        x: origin.0 + f64::from(center.0) + 14.0,
        y: origin.1 + f64::from(center.1) - 8.0,
        width: 120.0,
        height: 16.0,
    };
    label(
        scene,
        fonts,
        family,
        text,
        &bounds,
        origin,
        None,
        None,
        fg,
        LabelAlign::TopLeft,
        diagnostics,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn label(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    b: &Bounds,
    origin: (f64, f64),
    style: Option<&Style>,
    di_font: Option<&processcraft_core::model::DiFont>,
    fg: [u8; 3],
    align: LabelAlign,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    if text.is_empty() {
        return Ok(());
    }
    let center = align == LabelAlign::Center;
    let size = match di_font.and_then(|font| font.size) {
        Some(size) if size.is_finite() && (4.0..=256.0).contains(&size) => size as f32,
        Some(_) => {
            diagnostics
                .push("Label font size is outside the supported range; using default".into());
            12.0
        }
        None => 12.0,
    };
    let mut request = TextRequest::new(text, size, ((b.width as f32) - 8.0).clamp(4.0, 16384.0));
    request.family = di_font
        .and_then(|font| font.name.as_deref())
        .unwrap_or(family);
    request.bold = di_font.is_some_and(|font| font.is_bold == Some(true));
    request.italic = di_font.is_some_and(|font| font.is_italic == Some(true));
    request.center = align != LabelAlign::TopLeft;
    let mut layout = fonts.layout(request)?;
    // Preserve glyph descenders when the text itself fits the label box.
    // Padding is discretionary and must not consume space needed by outlines.
    let required_height = layout
        .glyphs()
        .iter()
        .filter_map(|g| g.outline.as_ref())
        .fold(layout.height(), |height, path| {
            height.max(path.bounds().bottom())
        });
    let vertical_padding = (((b.height as f32) - required_height) / 2.0).clamp(0.0, 4.0);
    let vertical_offset = if center {
        (((b.height as f32) - vertical_padding * 2.0 - required_height) / 2.0).max(0.0)
    } else {
        0.0
    };
    diagnostics.extend(layout.diagnostics().iter().cloned());
    let (fit_w, fit_h) = (
        ((b.width as f32) - 8.0).max(0.0),
        ((b.height as f32) - vertical_padding * 2.0 - vertical_offset).max(0.0),
    );
    let mut ellipsized = false;
    if layout.overflows(fit_w, fit_h) {
        // Replace the hidden tail with an ellipsis: the longest prefix that still fits
        // wins (binary search, so at most ~log2(chars) extra layouts).
        let ends: Vec<_> = text.char_indices().map(|(i, _)| i).skip(1).collect();
        let (mut lo, mut hi, mut best) = (0, ends.len(), None);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let candidate = format!("{}\u{2026}", text[..ends[mid]].trim_end());
            let attempt = fonts.layout(TextRequest {
                text: &candidate,
                ..request
            })?;
            if attempt.overflows(fit_w, fit_h) {
                hi = mid;
            } else {
                best = Some(attempt);
                lo = mid + 1;
            }
        }
        if let Some(best) = best {
            layout = best;
            layout.set_source_text(text);
            ellipsized = true;
        }
    }
    if layout.clip_to_bounds(fit_w, fit_h) || ellipsized {
        diagnostics.push(format!(
            "Label clipped because it exceeds its bounds: {text}"
        ));
    }
    let (x, y) = (
        (b.x - origin.0) as f32 + 4.0,
        (b.y - origin.1) as f32 + vertical_padding + vertical_offset,
    );
    let ink = color(style.and_then(|s| s.font.as_deref()), fg);
    let mut decorations = Vec::new();
    if let Some(font) = di_font
        && (font.is_underline == Some(true) || font.is_strike_through == Some(true))
    {
        for line in 0..layout.line_count() {
            let glyphs: Vec<_> = layout
                .glyphs()
                .iter()
                .filter(|glyph| glyph.visual_line == line)
                .collect();
            if glyphs.is_empty() {
                continue;
            }
            let left = glyphs
                .iter()
                .map(|glyph| glyph.x)
                .fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.advance)
                .fold(f32::NEG_INFINITY, f32::max);
            let baseline = glyphs[0].baseline;
            if font.is_underline == Some(true) {
                decorations.push((left, right, baseline + size * 0.10));
            }
            if font.is_strike_through == Some(true) {
                decorations.push((left, right, baseline - size * 0.30));
            }
        }
    }
    scene.add_colored_label(layout, x, y, ink)?;
    for (left, right, line_y) in decorations {
        if line_y >= 0.0 && line_y <= (b.height as f32 - 8.0 - vertical_offset).max(0.0) {
            lines(
                scene,
                &[(x + left, y + line_y), (x + right, y + line_y)],
                ink,
                (size / 16.0).max(0.75),
            )?;
        } else {
            diagnostics.push(format!(
                "Label decoration clipped because it exceeds its bounds: {text}"
            ));
        }
    }
    Ok(())
}
