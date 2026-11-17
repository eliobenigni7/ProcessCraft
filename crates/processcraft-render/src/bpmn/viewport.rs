//! `BpmnViewport`: culled, viewport-sized scene slices for the canvas.

use crate::{DiagramScene, RenderError, TextEngine};
use processcraft_core::{
    ids::{ElementKey, ElementSelector},
    model::{Bounds, BpmnDocument, Capability},
    overlay::Overlay,
};

use tiny_skia::PathBuilder;

use super::appearance::{Appearance, DARK_INK, NON_INITIATING_FILL, color, low_contrast};
use super::edges::{EdgeFrame, draw_edge};
use super::hit::{HitGeometry, HitRegion};
use super::labels::{
    LabelAlign, group_label_collision, label, label_font, label_visible, shape_label_layout,
};
use super::markers::self_reference_resolves;
use super::overlay::OverlayView;
use super::scene::{BpmnScene, BuildOptions};
use super::semantics::{DI_EXTENSION_DIAGNOSTIC, SemanticIndex, ShapeRefs, has_uninterpreted_di};

/// Viewport scenes keep labels this far outside the visible area.
pub(super) const CULL_MARGIN: f32 = 64.0;

/// Shapes beyond this coordinate magnitude are omitted by the viewport renderer.
pub(super) const VIEWPORT_GEOMETRY_LIMIT: f64 = 999_000.0;

/// Range of the user-selectable zoom and export scale, shared with the apps.
pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.125..=8.0;

/// Canvas dot grid: one dot every `DOT_SPACING` model units, hidden when they would crowd.
pub(super) const DOT_SPACING: f64 = 16.0;

/// View rectangle in absolute document units; zoom is pixels per document unit.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f32,
}

/// Pixels and hit geometry for one viewport. Hit geometry remains in document coordinates.
pub struct BpmnViewport {
    pub pixels: tiny_skia::Pixmap,
    pub hits: Vec<HitRegion>,
    pub diagnostics: Vec<String>,
    pub capability: Capability,
    pub viewport: Viewport,
}

impl BpmnViewport {
    pub fn build(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        view: Viewport,
    ) -> Result<Self, RenderError> {
        Self::build_with_overlay(document, diagram_id, appearance, fonts, family, view, None)
    }
    /// Like [`Self::build`], with activity fills and badges of an attribute overlay.
    /// The legend is not rasterized here; the desktop shows it beside the canvas.
    pub fn build_with_overlay(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        view: Viewport,
        overlay: Option<&Overlay>,
    ) -> Result<Self, RenderError> {
        if !view.x.is_finite()
            || !view.y.is_finite()
            || !view.width.is_finite()
            || !view.height.is_finite()
            || view.width <= 0.0
            || view.height <= 0.0
            || !view.zoom.is_finite()
            || !(0.125..=8.0).contains(&view.zoom)
            || view.width * f64::from(view.zoom) > 16384.0
            || view.height * f64::from(view.zoom) > 16384.0
        {
            return Err(RenderError("Invalid viewport"));
        }
        if matches!(document.capability, Capability::PreservedSource { .. }) {
            let mut empty = DiagramScene::new(view.width as f32, view.height as f32)?;
            if appearance != Appearance::Light {
                empty.add_background_rect(
                    0.0,
                    0.0,
                    view.width as f32,
                    view.height as f32,
                    appearance.paper(),
                )?;
            }
            let pixels = empty.rasterize(view.zoom)?;
            return Ok(Self {
                pixels,
                hits: vec![],
                diagnostics: vec!["Preserved source: semantic preview is unavailable".into()],
                capability: document.capability.clone(),
                viewport: view,
            });
        }
        let original_diagram = document
            .diagrams
            .iter()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        let index = SemanticIndex::new(document)?;
        let (semantics, visibility) = (&index, &index);
        // Only the active diagram is culled and rebased; the semantic model is shared.
        let mut diagram = original_diagram.clone();
        let inside = |b: &Bounds| {
            b.x <= view.x + view.width
                && b.y <= view.y + view.height
                && b.x + b.width >= view.x
                && b.y + b.height >= view.y
        };
        let rebase = |b: &mut Bounds| {
            b.x -= view.x;
            b.y -= view.y;
        };
        let mut viewport_diagnostics = Vec::new();
        if original_diagram
            .shapes
            .iter()
            .any(|shape| visibility.shape_visibility(shape, original_diagram).1)
            || original_diagram
                .edges
                .iter()
                .any(|edge| visibility.edge_visibility(edge, original_diagram).1)
        {
            viewport_diagnostics.push("Choreography representation ownership is ambiguous; affected elements remain visible".into());
        }
        diagram
            .shapes
            .retain(|s| !visibility.shape_visibility(s, original_diagram).0);
        diagram.shapes.retain(|s| {
            if !inside(&s.bounds) {
                return false;
            }
            let b = &s.bounds;
            if [
                b.x - view.x,
                b.y - view.y,
                b.x + b.width - view.x,
                b.y + b.height - view.y,
            ]
            .iter()
            .any(|n| !n.is_finite() || n.abs() > VIEWPORT_GEOMETRY_LIMIT)
            {
                viewport_diagnostics.push(format!(
                    "Shape {} omitted: geometry exceeds viewport renderer limits",
                    s.id
                ));
                false
            } else {
                true
            }
        });
        for s in &mut diagram.shapes {
            rebase(&mut s.bounds);
            if s.label
                .as_ref()
                .and_then(|label| label.bounds.as_ref())
                .is_some_and(|bounds| !inside(bounds))
            {
                s.label = None;
            }
            if let Some(l) = &mut s.label
                && let Some(bounds) = &mut l.bounds
            {
                rebase(bounds);
            }
        }
        diagram.edges.clear();
        let mut built = BpmnScene::build_internal(
            document,
            diagram_id,
            Some(&diagram),
            Some(semantics),
            fonts,
            family,
            BuildOptions {
                appearance,
                shape_labels: false,
                overlay,
                with_legend: false,
            },
        )?;
        built.diagnostics.extend(viewport_diagnostics);
        let original = document
            .diagrams
            .iter()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        if has_uninterpreted_di(original)
            && !built
                .diagnostics
                .iter()
                .any(|d| d == DI_EXTENSION_DIAGNOSTIC)
        {
            built.diagnostics.push(DI_EXTENSION_DIAGNOSTIC.into());
        }
        let paint_ranks = original.plane_order.as_ref().map(|order| {
            order
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i + 1))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        let fg = appearance.ink();
        let overlay_view = overlay.map(OverlayView::new);
        // Labels are culled independently of their owning shape.
        for shape in &original.shapes {
            if let Some(ranks) = &paint_ranks {
                built.scene.set_paint_rank(
                    *ranks
                        .get(&shape.id.selector())
                        .unwrap_or(&(ranks.len() + 1)),
                );
            }
            if semantics.activity_ports.contains(&shape.element)
                || semantics.shape_visibility(shape, original).0
            {
                continue;
            }
            if shape.participant_band_kind.is_some() {
                let owner = semantics.participant_band_owner(shape, original);
                let valid = owner
                    .and_then(|owner| semantics.choreography(&owner.element))
                    .is_some_and(|(root, activity)| {
                        root.participants.iter().any(|p| p.id == shape.element)
                            && activity
                                .participant_refs
                                .iter()
                                .any(|r| self_reference_resolves(semantics, r, &shape.element))
                    });
                if !valid {
                    continue;
                }
            }
            let refs = ShapeRefs::resolve(semantics, document, &shape.element);
            let ShapeRefs {
                node,
                kind,
                marker_kind,
                choreography,
                ..
            } = refs;
            if refs
                .node_name
                .as_ref()
                .is_some_and(|(_, truncated)| *truncated)
            {
                built.diagnostics.push(format!(
                    "Annotation {} text exceeds display limit",
                    shape.element
                ));
            }
            let Some(display_name) = refs.display_name(semantics, document, &shape.element, true)
            else {
                continue;
            };
            let name = display_name.as_str();
            if shape.label.is_none() && !built.hits.iter().any(|h| h.diagram_element_id == shape.id)
            {
                continue;
            }
            let b = &shape.bounds;
            let is_lane_or_pool = semantics.lane(&shape.element).is_some()
                || semantics.participant(&shape.element).is_some();
            let (label_bounds, label_align) =
                if semantics.groups.iter().any(|g| g.id == shape.element)
                    && shape
                        .label
                        .as_ref()
                        .and_then(|l| l.bounds.as_ref())
                        .is_none()
                {
                    (
                        Bounds {
                            x: b.x + 8.0,
                            y: b.y + 4.0,
                            width: (b.width - 16.0).max(0.0),
                            height: b.height.min(28.0),
                        },
                        LabelAlign::TopLeft,
                    )
                } else {
                    shape_label_layout(
                        shape,
                        kind,
                        marker_kind,
                        node,
                        choreography.is_some(),
                        is_lane_or_pool,
                    )
                };
            let bounds = &label_bounds;
            if !label_visible(shape.label.as_ref(), shape.label_visibility)
                || name.is_empty()
                || !inside(bounds)
            {
                continue;
            }
            if semantics.groups.iter().any(|g| g.id == shape.element)
                && group_label_collision(bounds, &shape.id.selector(), original, semantics)
            {
                built.diagnostics.push(format!(
                    "Group {} label overlaps another shape",
                    shape.element
                ));
            }
            let rotate_pool_label = is_lane_or_pool
                && shape.is_horizontal != Some(false)
                && shape
                    .label
                    .as_ref()
                    .and_then(|l| l.bounds.as_ref())
                    .is_none();
            let rotated_bounds = Bounds {
                x: 0.0,
                y: 0.0,
                width: bounds.height,
                height: bounds.width,
            };
            label(
                &mut built.scene,
                fonts,
                family,
                name,
                if rotate_pool_label {
                    &rotated_bounds
                } else {
                    bounds
                },
                if rotate_pool_label {
                    (0.0, 0.0)
                } else {
                    (view.x + built.origin.0, view.y + built.origin.1)
                },
                shape
                    .label
                    .as_ref()
                    .and_then(|l| l.style.as_ref())
                    .or(shape.style.as_ref())
                    .filter(|_| {
                        overlay_view
                            .as_ref()
                            .and_then(|view| view.fill(appearance, &shape.element, kind))
                            .is_none()
                    }),
                label_font(
                    document,
                    original,
                    shape.label.as_ref(),
                    &mut built.diagnostics,
                ),
                if matches!(
                    shape.participant_band_kind,
                    Some(
                        processcraft_core::model::ParticipantBandKind::TopNonInitiating
                            | processcraft_core::model::ParticipantBandKind::MiddleNonInitiating
                            | processcraft_core::model::ParticipantBandKind::BottomNonInitiating
                    )
                ) {
                    let band_fill = color(
                        shape.style.as_ref().and_then(|s| s.fill.as_deref()),
                        NON_INITIATING_FILL,
                    );
                    if low_contrast(DARK_INK, band_fill) {
                        [255, 255, 255]
                    } else {
                        DARK_INK
                    }
                } else {
                    fg
                },
                if shape.participant_band_kind.is_some() {
                    LabelAlign::Center
                } else {
                    label_align
                },
                &mut built.diagnostics,
            )?;
            if rotate_pool_label {
                built.scene.rotate_last_label(
                    (bounds.x - view.x - built.origin.0) as f32,
                    (bounds.y + bounds.height - view.y - built.origin.1) as f32,
                );
            }
            if label_visible(shape.label.as_ref(), shape.label_visibility) {
                let mut local = bounds.clone();
                rebase(&mut local);
                built.hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.selector(),
                    label_id: shape
                        .label
                        .as_ref()
                        .and_then(|label| label.id.as_ref())
                        .map(|id| id.selector()),
                    is_label: true,
                    geometry: HitGeometry::Shape(local),
                });
            }
        }
        let bg = appearance.paper();
        let edge_frame = EdgeFrame {
            document,
            diagram: original,
            semantics,
            origin: built.origin,
            fg,
            bg,
            family,
            clip: Some(view),
        };
        for edge in &original.edges {
            if let Some(ranks) = &paint_ranks {
                built
                    .scene
                    .set_paint_rank(semantics.edge_paint_rank(ranks, edge));
            }
            if semantics.edge_visibility(edge, original).0 {
                continue;
            }
            draw_edge(
                &edge_frame,
                &mut built.scene,
                &mut built.hits,
                &mut built.diagnostics,
                fonts,
                edge,
            )?;
        }
        if appearance.is_canvas() {
            // Dots stay 1.5 px on screen and vanish when closer than 10 px.
            let zoom = f64::from(view.zoom);
            if DOT_SPACING * zoom >= 10.0 {
                let mut dots = PathBuilder::new();
                let size = (1.5 / zoom) as f32;
                let first_x = (view.x / DOT_SPACING).ceil() * DOT_SPACING;
                let first_y = (view.y / DOT_SPACING).ceil() * DOT_SPACING;
                let mut gy = first_y;
                while gy < view.y + view.height {
                    let mut gx = first_x;
                    while gx < view.x + view.width {
                        if let Some(dot) = tiny_skia::Rect::from_xywh(
                            (gx - view.x - built.origin.0) as f32 - size / 2.0,
                            (gy - view.y - built.origin.1) as f32 - size / 2.0,
                            size,
                            size,
                        ) {
                            dots.push_rect(dot);
                        }
                        gx += DOT_SPACING;
                    }
                    gy += DOT_SPACING;
                }
                if let Some(dots) = dots.finish() {
                    built
                        .scene
                        .prepend_background_path(dots, appearance.grid_dot());
                }
            }
        }
        built.scene.prepend_background_rect(
            (-built.origin.0) as f32,
            (-built.origin.1) as f32,
            view.width as f32,
            view.height as f32,
            bg,
        )?;
        let pixels = built.scene.rasterize_viewport(
            (-built.origin.0) as f32,
            (-built.origin.1) as f32,
            view.width as f32,
            view.height as f32,
            view.zoom,
        )?;
        let mut hits = built.hits;
        if let Some(ranks) = &paint_ranks {
            hits.sort_by_key(|hit| {
                *ranks
                    .get(&hit.diagram_element_id)
                    .unwrap_or(&(ranks.len() + 1))
            });
        }
        for h in &mut hits {
            match &mut h.geometry {
                HitGeometry::Shape(b)
                | HitGeometry::Ellipse(b)
                | HitGeometry::Diamond(b)
                | HitGeometry::Hexagon(b)
                | HitGeometry::DataObject(b)
                | HitGeometry::DataStore(b) => {
                    b.x += view.x;
                    b.y += view.y;
                }
                HitGeometry::Edge(_) => {}
            }
        }
        Ok(Self {
            pixels,
            hits,
            diagnostics: built.diagnostics,
            capability: built.capability,
            viewport: view,
        })
    }
    pub fn hit_test(&self, x: f64, y: f64, tolerance: f64) -> Option<&HitRegion> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.contains(x, y, tolerance.max(0.0)))
    }
}
