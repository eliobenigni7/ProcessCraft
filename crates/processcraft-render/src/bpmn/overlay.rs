//! Attribute-overlay view plus the legend and badge drawing it needs.

use crate::{DiagramScene, RenderError, TextEngine, TextRequest};
use processcraft_core::{
    ids::ElementSelector,
    model::{Bounds, NodeKind},
    overlay::{Overlay, OverlayEntry},
};
use tiny_skia::PathBuilder;

use super::appearance::{Appearance, STROKE_OUTLINE, overlay_fill};
use super::labels::{LabelAlign, label};
use super::primitives::{outline, rounded_corners};

/// Overlay entries by element, built once per scene.
pub(super) struct OverlayView<'a> {
    pub(super) overlay: &'a Overlay,
    pub(super) entries: std::collections::HashMap<&'a ElementSelector, &'a OverlayEntry>,
}

impl<'a> OverlayView<'a> {
    pub(super) fn new(overlay: &'a Overlay) -> Self {
        Self {
            overlay,
            entries: overlay.entries.iter().map(|e| (&e.element, e)).collect(),
        }
    }
    /// The heatmap fill of an activity, which also replaces its label ink.
    pub(super) fn fill(
        &self,
        appearance: Appearance,
        element: &ElementSelector,
        kind: Option<NodeKind>,
    ) -> Option<[u8; 3]> {
        if !kind.is_some_and(NodeKind::is_activity) {
            return None;
        }
        let bucket = self.entries.get(element)?.bucket?;
        Some(overlay_fill(appearance, self.overlay.scale, bucket))
    }
}

pub(super) const LEGEND_ROW: f32 = 18.0;

pub(super) const LEGEND_SWATCH: f32 = 12.0;

/// `label` lays text out at 12 px unless a DI font says otherwise.
pub(super) const LEGEND_FONT: f32 = 12.0;

pub(super) const BADGE_FONT: f32 = 12.0;

pub(super) fn text_width(
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    size: f32,
) -> Result<f32, RenderError> {
    let mut request = TextRequest::new(text, size, 2000.0);
    request.family = family;
    let layout = fonts.layout(request)?;
    Ok(layout
        .glyphs()
        .iter()
        .map(|g| g.x + g.advance)
        .fold(0.0, f32::max))
}

/// Width and height of the legend block drawn below the diagram.
pub(super) fn legend_size(
    fonts: &mut TextEngine,
    family: &str,
    overlay: &Overlay,
) -> Result<(f32, f32), RenderError> {
    if overlay.legend.is_empty() {
        return Ok((0.0, 0.0));
    }
    let mut width = text_width(fonts, family, &overlay.title, LEGEND_FONT)?;
    for item in &overlay.legend {
        width =
            width.max(LEGEND_SWATCH + 8.0 + text_width(fonts, family, &item.label, LEGEND_FONT)?);
    }
    Ok((
        width + 32.0,
        LEGEND_ROW * (1 + overlay.legend.len()) as f32 + 16.0,
    ))
}

/// Title row and one swatch row per bucket, starting at `top` in scene space.
pub(super) fn draw_legend(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    appearance: Appearance,
    overlay: &Overlay,
    top: f32,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let ink = appearance.ink();
    let mut text =
        |scene: &mut DiagramScene, text: &str, x: f32, y: f32| -> Result<(), RenderError> {
            let bounds = Bounds {
                x: f64::from(x),
                y: f64::from(y),
                width: f64::from(text_width(fonts, family, text, LEGEND_FONT)? + 12.0),
                // Taller than a row: `label` clips text that nearly fills its box.
                height: f64::from(LEGEND_ROW) + 6.0,
            };
            label(
                scene,
                fonts,
                family,
                text,
                &bounds,
                (0.0, 0.0),
                None,
                None,
                ink,
                LabelAlign::TopLeft,
                diagnostics,
            )
        };
    text(scene, &overlay.title, 12.0, top + 8.0)?;
    for (i, item) in overlay.legend.iter().enumerate() {
        let y = top + 8.0 + LEGEND_ROW * (1 + i) as f32;
        if let Some(rect) = tiny_skia::Rect::from_xywh(16.0, y + 2.0, LEGEND_SWATCH, LEGEND_SWATCH)
        {
            let path = PathBuilder::from_rect(rect);
            scene.add_path(
                path.clone(),
                overlay_fill(appearance, overlay.scale, item.bucket),
            )?;
            outline(scene, &path, ink, STROKE_OUTLINE)?;
        }
        text(scene, &item.label, 16.0 + LEGEND_SWATCH + 4.0, y)?;
    }
    Ok(())
}

/// A pill centred on the shape's top-right corner: ink-coloured with paper-coloured text.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_badge(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    appearance: Appearance,
    shape: &Bounds,
    text: &str,
    origin: (f64, f64),
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let width = f64::from(text_width(fonts, family, text, BADGE_FONT)?.ceil()) + 12.0;
    let pill = Bounds {
        x: shape.x + shape.width + 6.0 - width,
        y: shape.y - 9.0,
        width,
        height: 18.0,
    };
    let mut path = PathBuilder::new();
    rounded_corners(
        &mut path,
        (pill.x - origin.0) as f32,
        (pill.y - origin.1) as f32,
        pill.width as f32,
        pill.height as f32,
        9.0,
        [true; 4],
    );
    if let Some(path) = path.finish() {
        scene.add_path(path, appearance.ink())?;
    }
    label(
        scene,
        fonts,
        family,
        text,
        &Bounds {
            y: pill.y - 3.0,
            height: pill.height + 6.0,
            ..pill
        },
        origin,
        None,
        None,
        appearance.paper(),
        LabelAlign::Center,
        diagnostics,
    )
}
