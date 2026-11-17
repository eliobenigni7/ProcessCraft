//! `BpmnScene`: the document-level scene builder and its hit testing.

use crate::{DiagramScene, RenderError, TextEngine};
use processcraft_core::{
    bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyLoopType, ConversationKind},
    bpmn_model::NodeDetails,
    ids::{ElementKey, ElementSelector},
    model::{Bounds, BpmnDocument, Capability, NodeKind, Shape},
    overlay::Overlay,
};
use tiny_skia::PathBuilder;

use super::HEADER;
use super::appearance::{
    Appearance, DARK_INK, NON_INITIATING_FILL, OUTLINE_DASH, STROKE_ARROW, STROKE_OUTLINE, color,
    low_contrast,
};
use super::edges::{EdgeFrame, draw_edge};
use super::hit::{HitGeometry, HitRegion};
use super::labels::{
    LabelAlign, edge_label_layout, external_label_bounds, group_label_collision,
    has_external_label, label, label_font, label_visible, message_name_label, shape_label_layout,
};
use super::markers::{
    draw_choreography_loop, draw_loop_marker, draw_new_markers, envelope, marker_polygon,
    non_interrupting, self_reference_resolves,
};
use super::overlay::{OverlayView, draw_badge, draw_legend, legend_size};
use super::primitives::{
    cylinder, dashed_ellipse, dashed_lines, ellipse, hexagon_points, lines, outline, polygon,
    rounded, rounded_corners, rounding,
};
use super::semantics::{
    CalledTarget, DI_EXTENSION_DIAGNOSTIC, SemanticIndex, ShapeRefs, data_label,
    effective_event_definitions, has_uninterpreted_di, node_display_name,
};

/// Size of the scene drawn in place of a preserved-source diagram.
pub(super) const EMPTY_SCENE_SIZE: f32 = 33.0;

/// Padding around the diagram content in exports.
pub(super) const EXPORT_MARGIN: f64 = 16.0;

/// Coordinates beyond this magnitude make the diagram unrenderable.
pub(super) const GEOMETRY_LIMIT: f64 = 1_000_000.0;

/// What `BpmnScene::build_internal` draws besides the diagram geometry.
#[derive(Clone, Copy)]
pub(super) struct BuildOptions<'a> {
    pub(super) appearance: Appearance,
    /// Draw shape labels (viewport scenes draw them separately, culled on their own).
    pub(super) shape_labels: bool,
    pub(super) overlay: Option<&'a Overlay>,
    /// Reserve and draw the overlay legend below the diagram.
    pub(super) with_legend: bool,
}

/// Document-only scene. Coordinates in hit regions remain in absolute document space.
#[derive(Clone, Debug)]
pub struct BpmnScene {
    pub scene: DiagramScene,
    pub origin: (f64, f64),
    pub hits: Vec<HitRegion>,
    pub diagnostics: Vec<String>,
    pub capability: Capability,
}

impl BpmnScene {
    pub fn build(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
    ) -> Result<Self, RenderError> {
        Self::build_with_overlay(document, diagram_id, appearance, fonts, family, None)
    }
    /// Like [`Self::build`], with an attribute overlay: activity fills, badges and a
    /// legend block below the diagram. The model and explicit styles are untouched.
    pub fn build_with_overlay(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        overlay: Option<&Overlay>,
    ) -> Result<Self, RenderError> {
        Self::build_internal(
            document,
            diagram_id,
            None,
            None,
            fonts,
            family,
            BuildOptions {
                appearance,
                shape_labels: true,
                overlay,
                with_legend: true,
            },
        )
    }
    /// `culled` replaces the stored diagram `diagram_id` and `index` is a prebuilt index of
    /// `document` (the viewport passes both, so it needs no document copy or second index).
    pub(super) fn build_internal<'d>(
        document: &'d BpmnDocument,
        diagram_id: &ElementSelector,
        culled: Option<&processcraft_core::model::Diagram>,
        index: Option<&SemanticIndex<'d>>,
        fonts: &mut TextEngine,
        family: &str,
        options: BuildOptions<'_>,
    ) -> Result<Self, RenderError> {
        let BuildOptions {
            appearance,
            shape_labels,
            overlay,
            with_legend,
        } = options;
        if matches!(document.capability, Capability::PreservedSource { .. }) {
            return preserved_source_scene(document, appearance);
        }
        let diagram = match culled {
            Some(diagram) => diagram,
            None => document
                .diagrams
                .iter()
                .find(|d| &d.id == diagram_id)
                .ok_or(RenderError("Diagram not found"))?,
        };
        let owned;
        let semantics = match index {
            Some(index) => index,
            None => {
                owned = SemanticIndex::new(document)?;
                &owned
            }
        };
        let (minx, miny, maxx, maxy) = diagram_extents(document, semantics, diagram)?;
        let origin = (minx - EXPORT_MARGIN, miny - EXPORT_MARGIN);
        let overlay_view = overlay.map(OverlayView::new);
        let diagram_height = ((maxy - miny) + 2.0 * EXPORT_MARGIN) as f32;
        let (legend_width, legend_height) = match overlay {
            Some(overlay) if with_legend => legend_size(fonts, family, overlay)?,
            _ => (0.0, 0.0),
        };
        let mut scene = DiagramScene::for_diagram(
            (((maxx - minx) + 2.0 * EXPORT_MARGIN) as f32).max(legend_width),
            diagram_height + legend_height,
        )?;
        let bg = appearance.paper();
        let (fg, fill) = (appearance.ink(), appearance.fill());
        scene.add_background_rect(0.0, 0.0, scene.width(), scene.height(), bg)?;
        let mut hits = Vec::new();
        let mut diagnostics = Vec::new();
        if diagram
            .shapes
            .iter()
            .any(|shape| semantics.shape_visibility(shape, diagram).1)
            || diagram
                .edges
                .iter()
                .any(|edge| semantics.edge_visibility(edge, diagram).1)
        {
            diagnostics.push("Choreography representation ownership is ambiguous; affected elements remain visible".into());
        }
        if has_uninterpreted_di(diagram) {
            diagnostics.push(DI_EXTENSION_DIAGNOSTIC.into());
        }
        let paint_ranks = diagram.plane_order.as_ref().map(|order| {
            order
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i + 1))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        let mut ordered_shapes: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|shape| !semantics.shape_visibility(shape, diagram).0)
            .collect();
        ordered_shapes.sort_by_key(|shape| {
            if semantics.groups.iter().any(|g| g.id == shape.element)
                || semantics.participant(&shape.element).is_some()
            {
                (0, 0)
            } else if let Some((_, depth)) = semantics.lane(&shape.element) {
                (depth + 1, 0)
            } else if let Some((_, depth)) = semantics.node(&shape.element) {
                (depth + 1, 1)
            } else {
                (usize::MAX, 0)
            }
        });
        let frame = Frame {
            document,
            diagram,
            semantics,
            appearance,
            origin,
            fg,
            fill,
            bg,
            overlay_view: &overlay_view,
            shape_labels,
            family,
        };
        for shape in ordered_shapes {
            if let Some(ranks) = &paint_ranks {
                scene.set_paint_rank(
                    *ranks
                        .get(&shape.id.selector())
                        .unwrap_or(&(ranks.len() + 1)),
                );
            }
            if semantics.activity_ports.contains(&shape.element) {
                diagnostics.push(format!("Activity-local or event-local data port {} cannot be depicted as a BPMN shape; only process ports can be shown", shape.element));
                continue;
            }
            if let Some(band_kind) = shape.participant_band_kind {
                draw_participant_band(
                    &frame,
                    &mut scene,
                    &mut hits,
                    &mut diagnostics,
                    fonts,
                    shape,
                    band_kind,
                )?;
                continue;
            }
            if semantics.groups.iter().any(|g| g.id == shape.element) {
                draw_group(
                    &frame,
                    &mut scene,
                    &mut hits,
                    &mut diagnostics,
                    fonts,
                    shape,
                )?;
                continue;
            }
            draw_shape(
                &frame,
                &mut scene,
                &mut hits,
                &mut diagnostics,
                fonts,
                shape,
            )?;
        }
        for edge in &diagram.edges {
            if let Some(ranks) = &paint_ranks {
                scene.set_paint_rank(semantics.edge_paint_rank(ranks, edge));
            }
            if semantics.edge_visibility(edge, diagram).0 {
                continue;
            }
            draw_edge(
                &frame.edge_frame(),
                &mut scene,
                &mut hits,
                &mut diagnostics,
                fonts,
                edge,
            )?;
        }
        if let Some(view) = &overlay_view {
            // Badges and the legend are painted last, above every shape and edge.
            if paint_ranks.is_some() {
                scene.set_paint_rank(usize::MAX);
            }
            draw_overlay_decorations(
                &frame,
                view,
                &mut scene,
                &mut diagnostics,
                fonts,
                legend_height,
                diagram_height,
            )?;
        }
        if let Some(ranks) = &paint_ranks {
            hits.sort_by_key(|hit| {
                *ranks
                    .get(&hit.diagram_element_id)
                    .unwrap_or(&(ranks.len() + 1))
            });
        }
        Ok(Self {
            scene,
            origin,
            hits,
            diagnostics,
            capability: document.capability.clone(),
        })
    }
    pub fn hit_test(&self, x: f64, y: f64, tolerance: f64) -> Option<&HitRegion> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.contains(x, y, tolerance.max(0.0)))
    }
}

/// What the per-shape and per-edge drawing helpers read, shared across one build.
#[derive(Clone, Copy)]
struct Frame<'a, 'd> {
    document: &'d BpmnDocument,
    diagram: &'a processcraft_core::model::Diagram,
    semantics: &'a SemanticIndex<'d>,
    appearance: Appearance,
    origin: (f64, f64),
    fg: [u8; 3],
    fill: [u8; 3],
    bg: [u8; 3],
    overlay_view: &'a Option<OverlayView<'a>>,
    shape_labels: bool,
    family: &'a str,
}

impl<'a, 'd> Frame<'a, 'd> {
    fn edge_frame(&self) -> EdgeFrame<'a, 'd> {
        EdgeFrame {
            document: self.document,
            diagram: self.diagram,
            semantics: self.semantics,
            origin: self.origin,
            fg: self.fg,
            bg: self.bg,
            family: self.family,
            clip: None,
        }
    }
}

fn preserved_source_scene(
    document: &BpmnDocument,
    appearance: Appearance,
) -> Result<BpmnScene, RenderError> {
    let mut scene = DiagramScene::new(EMPTY_SCENE_SIZE, EMPTY_SCENE_SIZE)?;
    scene.add_background_rect(
        0.0,
        0.0,
        EMPTY_SCENE_SIZE,
        EMPTY_SCENE_SIZE,
        appearance.paper(),
    )?;
    Ok(BpmnScene {
        scene,
        origin: (0.0, 0.0),
        hits: Vec::new(),
        diagnostics: vec!["Preserved source: semantic preview is unavailable".into()],
        capability: document.capability.clone(),
    })
}

fn diagram_extents(
    document: &BpmnDocument,
    semantics: &SemanticIndex<'_>,
    diagram: &processcraft_core::model::Diagram,
) -> Result<(f64, f64, f64, f64), RenderError> {
    let mut extents: Option<(f64, f64, f64, f64)> = None;
    let mut include = |x: f64, y: f64| -> Result<(), RenderError> {
        if !x.is_finite() || !y.is_finite() || x.abs() > GEOMETRY_LIMIT || y.abs() > GEOMETRY_LIMIT
        {
            return Err(RenderError("Invalid diagram geometry"));
        }
        extents = Some(match extents {
            None => (x, y, x, y),
            Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
        });
        Ok(())
    };
    for shape in &diagram.shapes {
        if semantics.activity_ports.contains(&shape.element) {
            continue;
        }
        if semantics.shape_visibility(shape, diagram).0 {
            continue;
        }
        let b = &shape.bounds;
        if b.width <= 0.0 || b.height <= 0.0 {
            return Err(RenderError("Invalid shape bounds"));
        }
        include(b.x, b.y)?;
        include(b.x + b.width, b.y + b.height)?;
        if shape.is_message_visible == Some(true) && shape.participant_band_kind.is_some() {
            include(b.x + b.width / 2.0 - 12.0, b.y - 24.0)?;
            include(b.x + b.width / 2.0 + 12.0, b.y + b.height + 24.0)?;
        }
        if let Some(b) = shape.label.as_ref().and_then(|label| label.bounds.as_ref()) {
            include(b.x, b.y)?;
            include(b.x + b.width, b.y + b.height)?;
        } else if label_visible(shape.label.as_ref(), shape.label_visibility) {
            // Default external labels sit outside the shape; keep export margins even.
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let port = semantics.port(&shape.element).map(|(port, _)| port);
            let store = document
                .data_stores
                .iter()
                .find(|store| store.id.as_ref().is_some_and(|id| id == &shape.element));
            let kind = node
                .map(|node| node.kind)
                .or(port.map(|_| NodeKind::DataObjectReference))
                .or(store.map(|_| NodeKind::DataStoreReference));
            let named = match (node, port, store) {
                (Some(node), ..) => {
                    !data_label(Some(node), &node_display_name(node, document).0, semantics)
                        .is_empty()
                }
                (_, Some(port), _) => port.name.as_deref().is_some_and(|n| !n.is_empty()),
                (.., Some(store)) => store.name.as_deref().is_some_and(|n| !n.is_empty()),
                _ => false,
            };
            if named && has_external_label(kind) {
                let lb = external_label_bounds(b);
                include(lb.x, lb.y)?;
                include(lb.x + lb.width, lb.y + lb.height)?;
            }
        }
    }
    for edge in &diagram.edges {
        if semantics.edge_visibility(edge, diagram).0 {
            continue;
        }
        for p in &edge.waypoints {
            include(p.x, p.y)?;
        }
        if let Some(b) = edge.label.as_ref().and_then(|label| label.bounds.as_ref()) {
            include(b.x, b.y)?;
            include(b.x + b.width, b.y + b.height)?;
        }
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = extents.unwrap_or((0.0, 0.0, 1.0, 1.0));
    for edge in &diagram.edges {
        if semantics.edge_visibility(edge, diagram).0 {
            continue;
        }
        if label_visible(edge.label.as_ref(), edge.label_visibility)
            && edge
                .label
                .as_ref()
                .and_then(|label| label.bounds.as_ref())
                .is_none()
            && let Some((p, _)) = edge_label_layout(edge)
        {
            let named = semantics
                .flow(&edge.element)
                .is_some_and(|f| f.name.as_deref().is_some_and(|name| !name.is_empty()))
                || semantics
                    .conversation_link(&edge.element)
                    .is_some_and(|link| link.name.as_deref().is_some_and(|name| !name.is_empty()));
            if named {
                minx = minx.min(p.x);
                miny = miny.min(p.y);
                maxx = maxx.max(p.x + p.width);
                maxy = maxy.max(p.y + p.height);
            }
        }
    }
    Ok((minx, miny, maxx, maxy))
}

fn draw_overlay_decorations(
    frame: &Frame<'_, '_>,
    view: &OverlayView<'_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    legend_height: f32,
    diagram_height: f32,
) -> Result<(), RenderError> {
    let Frame {
        diagram,
        semantics,
        appearance,
        origin,
        family,
        ..
    } = *frame;
    for shape in &diagram.shapes {
        let badge = view
            .entries
            .get(&shape.element)
            .and_then(|entry| entry.badge.as_deref());
        if let Some(text) = badge
            && !semantics.shape_visibility(shape, diagram).0
        {
            draw_badge(
                scene,
                fonts,
                family,
                appearance,
                &shape.bounds,
                text,
                origin,
                diagnostics,
            )?;
        }
    }
    if legend_height > 0.0 {
        draw_legend(
            scene,
            fonts,
            family,
            appearance,
            view.overlay,
            diagram_height,
            diagnostics,
        )?;
    }
    Ok(())
}

fn draw_participant_band(
    frame: &Frame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    shape: &Shape,
    band_kind: processcraft_core::model::ParticipantBandKind,
) -> Result<(), RenderError> {
    let Frame {
        document,
        diagram,
        semantics,
        origin,
        fg,
        fill,
        shape_labels,
        family,
        ..
    } = *frame;
    use processcraft_core::model::ParticipantBandKind;
    let owner = semantics.participant_band_owner(shape, diagram);
    let context = owner.and_then(|owner| semantics.choreography(&owner.element));
    let participant = context.and_then(|(scope, _)| {
        scope.participants.iter().find(|p| {
            p.id == shape.element
                && context.is_some_and(|(_, activity)| {
                    activity
                        .participant_refs
                        .iter()
                        .any(|r| self_reference_resolves(semantics, r, &shape.element))
                })
        })
    });
    let Some(participant) = participant else {
        diagnostics.push(format!(
            "Participant band {} has an unresolved activity representation or participant",
            shape.id
        ));
        return Ok(());
    };
    let b = &shape.bounds;
    let (x, y, w, h) = (
        (b.x - origin.0) as f32,
        (b.y - origin.1) as f32,
        b.width as f32,
        b.height as f32,
    );
    let non_initiating = matches!(
        band_kind,
        ParticipantBandKind::TopNonInitiating
            | ParticipantBandKind::MiddleNonInitiating
            | ParticipantBandKind::BottomNonInitiating
    );
    let default_band_fill = if non_initiating {
        NON_INITIATING_FILL
    } else {
        fill
    };
    let band_fill = color(
        shape.style.as_ref().and_then(|s| s.fill.as_deref()),
        default_band_fill,
    );
    // Corners shared with the owner's rounded outline are clipped to it.
    let band_path = {
        let o = owner.map_or(b, |owner| &owner.bounds);
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        let (top, bottom, left, right) = (
            near(b.y, o.y),
            near(b.y + b.height, o.y + o.height),
            near(b.x, o.x),
            near(b.x + b.width, o.x + o.width),
        );
        let mut p = PathBuilder::new();
        rounded_corners(
            &mut p,
            x,
            y,
            w,
            h,
            rounding(o.width as f32, o.height as f32),
            [top && left, top && right, bottom && right, bottom && left],
        );
        p.finish().ok_or(RenderError("Invalid participant band"))?
    };
    scene.add_path(band_path.clone(), band_fill)?;
    let band_fg = if non_initiating {
        if low_contrast(DARK_INK, band_fill) {
            [255, 255, 255]
        } else {
            DARK_INK
        }
    } else {
        fg
    };
    let explicit_stroke = shape.style.as_ref().and_then(|s| s.stroke.as_deref());
    let stroke = color(explicit_stroke, band_fg);
    if non_initiating && explicit_stroke.is_some() && low_contrast(stroke, band_fill) {
        diagnostics.push(format!(
            "Participant band {} explicit stroke has low contrast against its fill",
            shape.id
        ));
    }
    let label_style = shape
        .label
        .as_ref()
        .and_then(|label| label.style.as_ref())
        .or(shape.style.as_ref());
    if non_initiating
        && label_style
            .and_then(|s| s.font.as_deref())
            .is_some_and(|value| low_contrast(color(Some(value), band_fg), band_fill))
    {
        diagnostics.push(format!(
            "Participant band {} explicit label color has low contrast against its fill",
            shape.id
        ));
    }
    outline(scene, &band_path, stroke, STROKE_OUTLINE)?;
    if participant
        .multiplicity
        .as_ref()
        .and_then(|m| m.maximum)
        .is_some_and(|n| n > 1)
    {
        for offset in [-3.0, 0.0, 3.0] {
            lines(
                scene,
                &[
                    (x + w / 2.0 + offset, y + h - 7.0),
                    (x + w / 2.0 + offset, y + h - 2.0),
                ],
                stroke,
                1.0,
            )?;
        }
    }
    let fallback = b;
    let lb = shape
        .label
        .as_ref()
        .and_then(|l| l.bounds.as_ref())
        .unwrap_or(fallback);
    let name = participant.name.as_deref().unwrap_or("");
    if shape_labels && label_visible(shape.label.as_ref(), shape.label_visibility) {
        label(
            scene,
            fonts,
            family,
            name,
            lb,
            origin,
            label_style,
            label_font(document, diagram, shape.label.as_ref(), diagnostics),
            band_fg,
            LabelAlign::Center,
            diagnostics,
        )?;
    }
    hits.push(HitRegion {
        key: ElementKey {
            document_id: document.id,
            xml_id: shape.element.clone(),
        },
        diagram_element_id: shape.id.selector(),
        label_id: None,
        is_label: false,
        geometry: HitGeometry::Shape(b.clone()),
    });
    if shape_labels
        && label_visible(shape.label.as_ref(), shape.label_visibility)
        && !name.is_empty()
    {
        hits.push(HitRegion {
            key: ElementKey {
                document_id: document.id,
                xml_id: shape.element.clone(),
            },
            diagram_element_id: shape.id.selector(),
            label_id: shape
                .label
                .as_ref()
                .and_then(|l| l.id.as_ref())
                .map(|id| id.selector()),
            is_label: true,
            geometry: HitGeometry::Shape(lb.clone()),
        });
    }
    if shape.is_message_visible == Some(true) {
        let bottom = matches!(
            band_kind,
            ParticipantBandKind::BottomInitiating | ParticipantBandKind::BottomNonInitiating
        );
        if matches!(
            band_kind,
            ParticipantBandKind::MiddleInitiating | ParticipantBandKind::MiddleNonInitiating
        ) {
            diagnostics.push(format!(
                "Participant band {} message visibility cannot be depicted on a middle band",
                shape.id
            ));
        } else if let Some(band_flow) = context.and_then(|(scope, activity)| {
            activity.message_flow_refs.iter().find_map(|id| {
                scope.message_flows.iter().find(|flow| {
                    semantics.resolved(&Some(id.clone())).as_ref() == Some(&flow.id.selector())
                        && semantics.resolved(&flow.source).as_ref() == Some(&shape.element)
                })
            })
        }) {
            let cy = if bottom { y + h + 13.0 } else { y - 13.0 };
            let edge_y = if bottom { y + h } else { y };
            let envelope_y = if bottom { cy - 7.0 } else { cy + 7.0 };
            dashed_lines(
                scene,
                &[(x + w / 2.0, edge_y), (x + w / 2.0, envelope_y)],
                stroke,
                2.0,
            )?;
            envelope(scene, x + w / 2.0, cy, stroke, non_initiating, fill)?;
            if let Some(text) = semantics.message_name(band_flow) {
                message_name_label(
                    scene,
                    fonts,
                    family,
                    text,
                    (x + w / 2.0, cy),
                    origin,
                    fg,
                    diagnostics,
                )?;
            }
        } else {
            diagnostics.push(format!(
                "Participant band {} has no resolved outgoing choreography message",
                shape.id
            ));
        }
    }
    Ok(())
}

fn draw_group(
    frame: &Frame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    shape: &Shape,
) -> Result<(), RenderError> {
    let Frame {
        document,
        diagram,
        semantics,
        origin,
        fg,
        shape_labels,
        family,
        ..
    } = *frame;
    if semantics
        .groups
        .iter()
        .find(|g| g.id == shape.element)
        .is_some_and(|group| {
            group.category_value_ref.is_some()
                && semantics.resolved(&group.category_value_ref).is_none()
        })
    {
        diagnostics.push(format!(
            "Group {} has an unresolved category value; its label is omitted",
            shape.element
        ));
    }
    let b = &shape.bounds;
    let (x, y, w, h) = (
        (b.x - origin.0) as f32,
        (b.y - origin.1) as f32,
        b.width as f32,
        b.height as f32,
    );
    let r = 8.0_f32.min(w / 2.0).min(h / 2.0);
    let pts = [
        (x + r, y),
        (x + w - r, y),
        (x + w - r * 0.293, y + r * 0.293),
        (x + w, y + r),
        (x + w, y + h - r),
        (x + w - r * 0.293, y + h - r * 0.293),
        (x + w - r, y + h),
        (x + r, y + h),
        (x + r * 0.293, y + h - r * 0.293),
        (x, y + h - r),
        (x, y + r),
        (x + r * 0.293, y + r * 0.293),
        (x + r, y),
    ];
    let stroke = color(shape.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
    dashed_lines(scene, &pts, stroke, OUTLINE_DASH)?;
    if shape_labels
        && label_visible(shape.label.as_ref(), shape.label_visibility)
        && let Some(name) = semantics.group_label(&shape.element, document).as_deref()
    {
        let fallback = Bounds {
            x: b.x + 8.0,
            y: b.y + 4.0,
            width: (b.width - 16.0).max(0.0),
            height: b.height.min(28.0),
        };
        let bounds = shape
            .label
            .as_ref()
            .and_then(|l| l.bounds.as_ref())
            .unwrap_or(&fallback);
        if group_label_collision(bounds, &shape.id.selector(), diagram, semantics) {
            diagnostics.push(format!(
                "Group {} label overlaps another shape",
                shape.element
            ));
        }
        label(
            scene,
            fonts,
            family,
            name,
            bounds,
            origin,
            shape
                .label
                .as_ref()
                .and_then(|l| l.style.as_ref())
                .or(shape.style.as_ref()),
            label_font(document, diagram, shape.label.as_ref(), diagnostics),
            fg,
            LabelAlign::TopLeft,
            diagnostics,
        )?;
        if label_visible(shape.label.as_ref(), shape.label_visibility) {
            hits.push(HitRegion {
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
                geometry: HitGeometry::Shape(bounds.clone()),
            });
        }
    }
    hits.push(HitRegion {
        key: ElementKey {
            document_id: document.id,
            xml_id: shape.element.clone(),
        },
        diagram_element_id: shape.id.selector(),
        label_id: None,
        is_label: false,
        geometry: HitGeometry::Shape(b.clone()),
    });
    Ok(())
}

fn draw_shape(
    frame: &Frame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    shape: &Shape,
) -> Result<(), RenderError> {
    let Frame {
        document,
        semantics,
        appearance,
        origin,
        fg,
        overlay_view,
        ..
    } = *frame;
    let refs = ShapeRefs::resolve(semantics, document, &shape.element);
    let ShapeRefs {
        node,
        port,
        kind,
        call_target,
        marker_kind,
        conversation,
        choreography,
        ..
    } = refs;
    let details = node.and_then(|node| node.details.as_ref());
    if matches!(kind, Some(NodeKind::CallActivity))
        && matches!(call_target, CalledTarget::Unresolved)
    {
        diagnostics.push(format!(
            "Call activity {} target cannot be resolved locally; called task marker omitted",
            shape.element
        ));
    }
    if refs
        .node_name
        .as_ref()
        .is_some_and(|(_, truncated)| *truncated)
    {
        diagnostics.push(format!(
            "Annotation {} text exceeds display limit",
            shape.element
        ));
    }
    let Some(display_name) = refs.display_name(semantics, document, &shape.element, false) else {
        if document
            .choreographies
            .iter()
            .any(|root| root.id == shape.element && root.kind.is_some())
        {
            diagnostics.push(format!("Global choreography {} has no standalone shape specified by BPMN 2.0.2 section 11.5.4; representation omitted", shape.element));
        } else {
            diagnostics.push(format!("Unknown shape element {}", shape.element));
        }
        return Ok(());
    };
    let name = display_name.as_str();
    if matches!(kind, Some(NodeKind::BoundaryEvent)) {
        let attached = match details {
            Some(NodeDetails::BoundaryEvent { attached_to, .. }) => Some(attached_to),
            Some(NodeDetails::Event { attached_to, .. }) => attached_to.as_ref(),
            _ => None,
        };
        let attached_id = semantics.resolved(&attached.cloned());
        if attached_id.as_ref().is_none_or(|id| {
            !semantics
                .nodes
                .iter()
                .any(|(n, _)| &n.id == id && n.kind.is_activity())
                && semantics.choreography(id).is_none()
        }) {
            diagnostics.push(format!(
                "Boundary event {} has no valid attached activity",
                shape.element
            ));
            return Ok(());
        }
    }
    let b = &shape.bounds;
    let x = (b.x - origin.0) as f32;
    let y = (b.y - origin.1) as f32;
    let w = b.width as f32;
    let h = b.height as f32;
    let stroke = color(shape.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
    let default_fill = appearance.node_fill(kind);
    let overlaid = overlay_view
        .as_ref()
        .and_then(|view| view.fill(appearance, &shape.element, kind));
    let fc = overlaid.unwrap_or_else(|| {
        color(
            shape.style.as_ref().and_then(|s| s.fill.as_deref()),
            default_fill,
        )
    });
    let fold = w.min(h).min(12.0);
    let is_lane_or_pool =
        semantics.lane(&shape.element).is_some() || semantics.participant(&shape.element).is_some();
    let ctx = ShapeCtx {
        shape,
        node,
        port,
        kind,
        conversation,
        choreography,
        details,
        call_target,
        marker_kind,
        name,
        b,
        x,
        y,
        w,
        h,
        stroke,
        fc,
        overlaid,
        fold,
        is_lane_or_pool,
    };
    draw_outline(&ctx, scene)?;
    draw_kind_decorations(frame, &ctx, scene)?;
    draw_task_icon(frame, &ctx, scene)?;
    draw_activity_markers(frame, &ctx, scene, diagnostics)?;
    draw_choreography_bands(frame, &ctx, scene, diagnostics, fonts)?;
    draw_conversation_and_data(frame, &ctx, scene, diagnostics)?;
    draw_event_definitions(frame, &ctx, scene, diagnostics)?;
    draw_label_and_hits(frame, &ctx, scene, hits, diagnostics, fonts)?;
    Ok(())
}

/// Per-shape values resolved once in `draw_shape` and read by its drawing phases.
#[derive(Clone, Copy)]
struct ShapeCtx<'a> {
    shape: &'a Shape,
    node: Option<&'a processcraft_core::model::Node>,
    port: Option<(&'a processcraft_core::bpmn_data::DataPort, bool)>,
    kind: Option<NodeKind>,
    conversation: Option<&'a processcraft_core::bpmn_collaboration::ConversationNode>,
    choreography: Option<(
        &'a processcraft_core::bpmn_collaboration::Choreography,
        &'a processcraft_core::bpmn_collaboration::ChoreographyActivity,
    )>,
    details: Option<&'a NodeDetails>,
    call_target: CalledTarget,
    marker_kind: Option<NodeKind>,
    name: &'a str,
    b: &'a Bounds,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: [u8; 3],
    fc: [u8; 3],
    overlaid: Option<[u8; 3]>,
    fold: f32,
    is_lane_or_pool: bool,
}

fn draw_outline(ctx: &ShapeCtx<'_>, scene: &mut DiagramScene) -> Result<(), RenderError> {
    let ShapeCtx {
        kind,
        conversation,
        choreography,
        x,
        y,
        w,
        h,
        fc,
        fold,
        ..
    } = *ctx;
    let mut path = PathBuilder::new();
    match kind {
        Some(
            NodeKind::StartEvent
            | NodeKind::EndEvent
            | NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent,
        ) => ellipse(&mut path, x, y, w, h),
        Some(
            NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway
            | NodeKind::InclusiveGateway
            | NodeKind::ComplexGateway
            | NodeKind::EventBasedGateway,
        ) => polygon(
            &mut path,
            &[
                (x + w / 2.0, y),
                (x + w, y + h / 2.0),
                (x + w / 2.0, y + h),
                (x, y + h / 2.0),
            ],
        ),
        Some(
            NodeKind::Task
            | NodeKind::UserTask
            | NodeKind::ManualTask
            | NodeKind::ServiceTask
            | NodeKind::SendTask
            | NodeKind::ReceiveTask
            | NodeKind::ScriptTask
            | NodeKind::BusinessRuleTask
            | NodeKind::SubProcess
            | NodeKind::AdHocSubProcess
            | NodeKind::Transaction
            | NodeKind::CallActivity,
        ) => rounded(&mut path, x, y, w, h),
        Some(NodeKind::TextAnnotation) => {}
        Some(NodeKind::DataStoreReference) => cylinder(&mut path, x, y, w, h),
        Some(NodeKind::DataObjectReference) => polygon(
            &mut path,
            &[
                (x, y),
                (x + w - fold, y),
                (x + w, y + fold),
                (x + w, y + h),
                (x, y + h),
            ],
        ),
        _ if choreography.is_some() => rounded(&mut path, x, y, w, h),
        _ if conversation.is_some() => polygon(&mut path, &hexagon_points(x, y, w, h)),
        _ => polygon(&mut path, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]),
    };
    if let Some(p) = path.finish() {
        scene.add_path(p, fc)?;
    }
    Ok(())
}

fn draw_kind_decorations(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
) -> Result<(), RenderError> {
    let Frame {
        diagram,
        semantics,
        appearance,
        ..
    } = *frame;
    let ShapeCtx {
        shape,
        node,
        kind,
        conversation,
        choreography,
        details,
        x,
        y,
        w,
        h,
        stroke,
        fold,
        ..
    } = *ctx;
    if choreography.is_some_and(|(root, _)| root.graph.is_none()) && diagram.plane_order.is_none() {
        let band_h = (h * 0.22).clamp(12.0, 24.0).min((h - 2.0).max(0.0));
        let band_color = appearance.choreography_band();
        if w > 2.0 && band_h > 1.0 {
            // Bottom corners follow the rounded outline, 1 px inside it.
            let mut band = PathBuilder::new();
            rounded_corners(
                &mut band,
                x + 1.0,
                y + h - band_h,
                w - 2.0,
                band_h - 1.0,
                rounding(w, h) - 1.0,
                [false, false, true, true],
            );
            if let Some(band) = band.finish() {
                scene.add_path(band, band_color)?;
            }
        }
    }
    match kind {
        Some(
            NodeKind::StartEvent
            | NodeKind::EndEvent
            | NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent,
        ) => {
            let non_interrupting_start = matches!(kind, Some(NodeKind::StartEvent))
                && matches!(
                    details,
                    Some(NodeDetails::Event {
                        is_interrupting: Some(false),
                        ..
                    })
                )
                && semantics
                    .parents
                    .get(&shape.element)
                    .and_then(|parent| semantics.node(parent))
                    .and_then(|(node, _)| node.activity.as_ref())
                    .is_some_and(|activity| activity.triggered_by_event == Some(true));
            if (matches!(kind, Some(NodeKind::BoundaryEvent)) && non_interrupting(details))
                || non_interrupting_start
            {
                dashed_ellipse(scene, x, y, w, h, stroke)?;
            } else {
                let mut p = PathBuilder::new();
                ellipse(&mut p, x, y, w, h);
                if let Some(path) = p.finish() {
                    outline(
                        scene,
                        &path,
                        stroke,
                        if matches!(kind, Some(NodeKind::EndEvent)) {
                            3.0
                        } else {
                            1.5
                        },
                    )?;
                }
            }
        }
        Some(
            NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway
            | NodeKind::InclusiveGateway
            | NodeKind::ComplexGateway
            | NodeKind::EventBasedGateway,
        ) => {
            lines(
                scene,
                &[
                    (x + w / 2.0, y),
                    (x + w, y + h / 2.0),
                    (x + w / 2.0, y + h),
                    (x, y + h / 2.0),
                    (x + w / 2.0, y),
                ],
                stroke,
                1.5,
            )?;
            if matches!(kind, Some(NodeKind::ParallelGateway)) {
                lines(
                    scene,
                    &[(x + w * 0.3, y + h * 0.5), (x + w * 0.7, y + h * 0.5)],
                    stroke,
                    2.0,
                )?;
                lines(
                    scene,
                    &[(x + w * 0.5, y + h * 0.3), (x + w * 0.5, y + h * 0.7)],
                    stroke,
                    2.0,
                )?;
            } else if matches!(kind, Some(NodeKind::ExclusiveGateway))
                && shape.is_marker_visible == Some(true)
            {
                lines(
                    scene,
                    &[(x + w * 0.35, y + h * 0.35), (x + w * 0.65, y + h * 0.65)],
                    stroke,
                    2.0,
                )?;
                lines(
                    scene,
                    &[(x + w * 0.65, y + h * 0.35), (x + w * 0.35, y + h * 0.65)],
                    stroke,
                    2.0,
                )?;
            }
        }
        Some(
            NodeKind::Task
            | NodeKind::UserTask
            | NodeKind::ManualTask
            | NodeKind::ServiceTask
            | NodeKind::SendTask
            | NodeKind::ReceiveTask
            | NodeKind::ScriptTask
            | NodeKind::BusinessRuleTask
            | NodeKind::SubProcess
            | NodeKind::AdHocSubProcess
            | NodeKind::Transaction
            | NodeKind::CallActivity,
        ) => {
            let event_subprocess = matches!(kind, Some(NodeKind::SubProcess))
                && node
                    .and_then(|n| n.activity.as_ref())
                    .is_some_and(|a| a.triggered_by_event == Some(true));
            if event_subprocess {
                let r = 8.0_f32.min(w / 2.0).min(h / 2.0);
                let mut points = Vec::new();
                for (cx, cy, start) in [
                    (x + w - r, y + r, -std::f32::consts::FRAC_PI_2),
                    (x + w - r, y + h - r, 0.0),
                    (x + r, y + h - r, std::f32::consts::FRAC_PI_2),
                    (x + r, y + r, std::f32::consts::PI),
                ] {
                    for i in 0..=6 {
                        let a = start + i as f32 * std::f32::consts::FRAC_PI_2 / 6.0;
                        points.push((cx + r * a.cos(), cy + r * a.sin()));
                    }
                }
                points.push(points[0]);
                dashed_lines(scene, &points, stroke, OUTLINE_DASH)?;
            } else {
                let mut p = PathBuilder::new();
                rounded(&mut p, x, y, w, h);
                if let Some(path) = p.finish() {
                    outline(
                        scene,
                        &path,
                        stroke,
                        if matches!(kind, Some(NodeKind::CallActivity)) {
                            3.0
                        } else {
                            1.5
                        },
                    )?;
                }
            }
            if matches!(kind, Some(NodeKind::Transaction)) {
                let inset = 4.0;
                let mut inner = PathBuilder::new();
                rounded(
                    &mut inner,
                    x + inset,
                    y + inset,
                    (w - 2.0 * inset).max(0.0),
                    (h - 2.0 * inset).max(0.0),
                );
                if let Some(path) = inner.finish() {
                    outline(scene, &path, stroke, STROKE_OUTLINE)?;
                }
            }
        }
        Some(NodeKind::DataStoreReference) => {
            let mut p = PathBuilder::new();
            cylinder(&mut p, x, y, w, h);
            if let Some(path) = p.finish() {
                outline(scene, &path, stroke, STROKE_ARROW)?;
            }
            for offset in [0.10_f32, 0.18] {
                let mut arc = PathBuilder::new();
                arc.move_to(x, y + h * offset);
                arc.cubic_to(
                    x + w * 0.2,
                    y + h * (offset + 0.10),
                    x + w * 0.8,
                    y + h * (offset + 0.10),
                    x + w,
                    y + h * offset,
                );
                if let Some(path) = arc.finish() {
                    outline(scene, &path, stroke, STROKE_OUTLINE)?;
                }
            }
        }
        Some(NodeKind::TextAnnotation) => lines(
            scene,
            &[(x + 10.0, y), (x, y), (x, y + h), (x + 10.0, y + h)],
            stroke,
            1.5,
        )?,
        Some(NodeKind::DataObjectReference) => lines(
            scene,
            &[
                (x, y),
                (x + w - fold, y),
                (x + w, y + fold),
                (x + w, y + h),
                (x, y + h),
                (x, y),
            ],
            stroke,
            1.5,
        )?,
        _ if choreography.is_some() => {
            let mut p = PathBuilder::new();
            rounded(&mut p, x, y, w, h);
            if let Some(path) = p.finish() {
                outline(
                    scene,
                    &path,
                    stroke,
                    if choreography.is_some_and(|(_, a)| {
                        matches!(a.details, Some(ChoreographyActivityDetails::Call { .. }))
                    }) {
                        3.0
                    } else {
                        1.5
                    },
                )?;
            }
        }
        _ if conversation.is_some() => {
            let width = if conversation
                .is_some_and(|node| node.kind == ConversationKind::CallConversation)
            {
                3.0
            } else {
                1.5
            };
            lines(scene, &hexagon_points(x, y, w, h), stroke, width)?
        }
        _ => lines(
            scene,
            &[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)],
            stroke,
            1.5,
        )?,
    }
    Ok(())
}

fn draw_task_icon(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
) -> Result<(), RenderError> {
    let Frame { appearance, .. } = *frame;
    let ShapeCtx {
        shape,
        conversation,
        choreography,
        marker_kind,
        x,
        y,
        w,
        h,
        stroke,
        fold,
        ..
    } = *ctx;
    match marker_kind {
        Some(NodeKind::UserTask) => {
            let mut head = PathBuilder::new();
            ellipse(&mut head, x + 8.0, y + 6.0, 7.0, 7.0);
            if let Some(path) = head.finish() {
                outline(scene, &path, stroke, STROKE_OUTLINE)?;
            }
            lines(
                scene,
                &[
                    (x + 7.0, y + 19.0),
                    (x + 7.0, y + 16.0),
                    (x + 10.0, y + 13.0),
                    (x + 14.0, y + 13.0),
                    (x + 17.0, y + 16.0),
                    (x + 17.0, y + 19.0),
                ],
                stroke,
                1.0,
            )?;
        }
        Some(NodeKind::ManualTask) => {
            lines(
                scene,
                &[
                    (x + 7.0, y + 14.0),
                    (x + 9.0, y + 10.0),
                    (x + 11.0, y + 14.0),
                    (x + 11.0, y + 8.0),
                    (x + 13.0, y + 8.0),
                    (x + 13.0, y + 14.0),
                    (x + 15.0, y + 10.0),
                    (x + 17.0, y + 11.0),
                    (x + 16.0, y + 18.0),
                    (x + 10.0, y + 19.0),
                    (x + 7.0, y + 14.0),
                ],
                stroke,
                1.0,
            )?;
        }
        Some(NodeKind::DataObjectReference) => {
            lines(
                scene,
                &[
                    (x + w - fold, y),
                    (x + w - fold, y + fold),
                    (x + w, y + fold),
                ],
                stroke,
                1.0,
            )?;
        }
        None if conversation.is_none() && choreography.is_none() => {
            if appearance.is_canvas() {
                // Tinted header band, inside the outline.
                let header = HEADER as f32;
                let band = if shape.is_horizontal == Some(false) {
                    tiny_skia::Rect::from_xywh(x + 1.0, y + 1.0, w - 2.0, header - 1.0)
                } else {
                    tiny_skia::Rect::from_xywh(x + 1.0, y + 1.0, header - 1.0, h - 2.0)
                };
                if let Some(band) = band.filter(|_| w > header && h > header) {
                    scene.add_path(PathBuilder::from_rect(band), appearance.lane_header_band())?;
                }
            }
            if shape.is_horizontal == Some(false) {
                let header = HEADER as f32;
                lines(scene, &[(x, y + header), (x + w, y + header)], stroke, 1.0)?;
            } else {
                let header = HEADER as f32;
                lines(scene, &[(x + header, y), (x + header, y + h)], stroke, 1.0)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn draw_activity_markers(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let Frame {
        semantics,
        diagram,
        origin,
        ..
    } = *frame;
    let ShapeCtx {
        shape,
        node,
        kind,
        choreography,
        call_target,
        x,
        y,
        w,
        h,
        stroke,
        ..
    } = *ctx;
    let expandable = matches!(
        kind,
        Some(
            NodeKind::SubProcess
                | NodeKind::AdHocSubProcess
                | NodeKind::Transaction
                | NodeKind::CallActivity
        )
    );
    let collapsed = choreography.is_some_and(|(_, a)| {
        matches!(a.details, Some(ChoreographyActivityDetails::Sub { .. }))
            && shape.is_expanded == Some(false)
    }) || expandable
        && match (kind, call_target) {
            (Some(NodeKind::CallActivity), CalledTarget::Process) => {
                shape.is_expanded != Some(true)
            }
            (Some(NodeKind::CallActivity), _) => false,
            _ => shape.is_expanded != Some(true),
        };
    if expandable
        && shape.is_expanded.is_none()
        && matches!(kind, Some(NodeKind::CallActivity))
        && matches!(call_target, CalledTarget::Unresolved)
    {
        diagnostics.push(format!(
            "Activity {} expansion state is unspecified; collapsed marker omitted",
            shape.element
        ));
    }
    let loop_marker = node.and_then(|node| node.loop_characteristics.as_ref());
    let choreography_loop = choreography
        .and_then(|(_, a)| a.loop_type)
        .filter(|v| *v != ChoreographyLoopType::None);
    let compensation = node
        .and_then(|node| node.activity.as_ref())
        .is_some_and(|activity| activity.is_for_compensation == Some(true));
    let ad_hoc = matches!(kind, Some(NodeKind::AdHocSubProcess));
    let marker_count = usize::from(loop_marker.is_some() || choreography_loop.is_some())
        + usize::from(compensation)
        + usize::from(collapsed)
        + usize::from(ad_hoc);
    if marker_count > 0 {
        if w < marker_count as f32 * 18.0 || h < 30.0 {
            diagnostics.push(format!(
                "Activity {} is too small to show its markers",
                shape.element
            ));
        } else {
            let mut cx = x + w / 2.0 - (marker_count as f32 - 1.0) * 9.0;
            let cy = if choreography.is_some() {
                let bottom_band = diagram
                    .shapes
                    .iter()
                    .filter(|band| {
                        semantics.band_belongs_to(band, shape)
                            && matches!(
                                band.participant_band_kind,
                                Some(
                                    processcraft_core::model::ParticipantBandKind::BottomInitiating
                                        | processcraft_core::model::ParticipantBandKind::BottomNonInitiating
                                )
                            )
                    })
                    .map(|band| band.bounds.y as f32 - origin.1 as f32)
                    .fold(y + h, f32::min);
                bottom_band - 10.0
            } else {
                y + h - 10.0
            };
            if let Some(characteristics) = loop_marker {
                draw_loop_marker(scene, characteristics, cx, cy, stroke)?;
                cx += 18.0;
            }
            if let Some(marker) = choreography_loop {
                draw_choreography_loop(scene, marker, cx, cy, stroke)?;
                cx += 18.0;
            }
            if compensation {
                for dx in [-5.0_f32, 1.0] {
                    lines(
                        scene,
                        &[
                            (cx + dx + 5.0, cy - 5.0),
                            (cx + dx, cy),
                            (cx + dx + 5.0, cy + 5.0),
                            (cx + dx + 5.0, cy - 5.0),
                        ],
                        stroke,
                        1.1,
                    )?;
                }
                cx += 18.0;
            }
            if collapsed {
                lines(
                    scene,
                    &[
                        (cx - 6.0, cy - 6.0),
                        (cx + 6.0, cy - 6.0),
                        (cx + 6.0, cy + 6.0),
                        (cx - 6.0, cy + 6.0),
                        (cx - 6.0, cy - 6.0),
                    ],
                    stroke,
                    1.0,
                )?;
                lines(
                    scene,
                    &[(cx - 4.0, cy), (cx + 4.0, cy)],
                    stroke,
                    STROKE_OUTLINE,
                )?;
                lines(
                    scene,
                    &[(cx, cy - 4.0), (cx, cy + 4.0)],
                    stroke,
                    STROKE_OUTLINE,
                )?;
                cx += 18.0;
            }
            if ad_hoc {
                lines(
                    scene,
                    &[
                        (cx - 7.0, cy),
                        (cx - 4.0, cy - 2.0),
                        (cx, cy + 2.0),
                        (cx + 4.0, cy + 2.0),
                        (cx + 7.0, cy),
                    ],
                    stroke,
                    1.2,
                )?;
            }
        }
    }
    Ok(())
}

fn draw_choreography_bands(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
) -> Result<(), RenderError> {
    let Frame {
        diagram,
        semantics,
        origin,
        fg,
        family,
        ..
    } = *frame;
    let ShapeCtx {
        shape,
        choreography,
        b,
        x,
        y,
        w,
        h,
        stroke,
        ..
    } = *ctx;
    let has_explicit_bands = diagram
        .shapes
        .iter()
        .any(|band| semantics.band_belongs_to(band, shape));
    if choreography.is_some() && !has_explicit_bands && diagram.plane_order.is_some() {
        diagnostics.push(format!(
            "Choreography {} has no explicit participant band representations",
            shape.element
        ));
    }
    if let Some((scope, activity)) = choreography.filter(|(root, _)| {
        root.graph.is_none() && !has_explicit_bands && diagram.plane_order.is_none()
    }) {
        if activity.participant_refs.len() != 2 {
            diagnostics.push(format!(
                "Choreography {} participant band order is not represented",
                activity.id
            ));
        }
        let initiating = semantics.resolved(&activity.initiating_participant);
        let other = activity
            .participant_refs
            .iter()
            .filter_map(|r| semantics.resolved(&Some(r.clone())))
            .find(|id| Some(id) != initiating.as_ref());
        let top = scope
            .participants
            .iter()
            .find(|p| Some(&p.id.selector()) == initiating.as_ref())
            .map(|p| p.name.as_deref().unwrap_or(""));
        let bottom = other
            .and_then(|id| scope.participants.iter().find(|p| p.id == id))
            .map(|p| p.name.as_deref().unwrap_or(""));
        let band_h = (h * 0.22).clamp(12.0, 24.0);
        lines(scene, &[(x, y + band_h), (x + w, y + band_h)], stroke, 1.0)?;
        lines(
            scene,
            &[(x, y + h - band_h), (x + w, y + h - band_h)],
            stroke,
            1.0,
        )?;
        for (name, by) in [(top, b.y), (bottom, b.y + b.height - band_h as f64)] {
            if let Some(name) = name {
                let band = Bounds {
                    x: b.x,
                    y: by,
                    width: b.width,
                    height: band_h as f64,
                };
                label(
                    scene,
                    fonts,
                    family,
                    name,
                    &band,
                    origin,
                    shape.style.as_ref(),
                    None,
                    fg,
                    LabelAlign::TopLeft,
                    diagnostics,
                )?;
            }
        }
        if !activity.message_flow_refs.is_empty() {
            diagnostics.push(format!(
                "Choreography {} message-band visibility is not represented",
                activity.id
            ));
        }
    }
    Ok(())
}

fn draw_conversation_and_data(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let Frame { semantics, .. } = *frame;
    let ShapeCtx {
        shape,
        port,
        conversation,
        details,
        x,
        y,
        w,
        h,
        stroke,
        ..
    } = *ctx;
    if conversation.is_some_and(|node| node.kind == ConversationKind::SubConversation)
        && shape.is_expanded.is_none()
    {
        diagnostics.push(format!(
            "Sub-conversation {} expansion state is unspecified; collapsed marker omitted",
            shape.element
        ));
    }
    if conversation.is_some_and(|node| node.kind == ConversationKind::SubConversation)
        && shape.is_expanded == Some(false)
    {
        let (cx, cy) = (x + w / 2.0, y + h - 10.0);
        let pts = [
            (cx - 5.0, cy - 5.0),
            (cx + 5.0, cy - 5.0),
            (cx + 5.0, cy + 5.0),
            (cx - 5.0, cy + 5.0),
            (cx - 5.0, cy - 5.0),
        ];
        lines(scene, &pts, stroke, STROKE_OUTLINE)?;
        lines(
            scene,
            &[(cx - 3.0, cy), (cx + 3.0, cy)],
            stroke,
            STROKE_OUTLINE,
        )?;
        lines(
            scene,
            &[(cx, cy - 3.0), (cx, cy + 3.0)],
            stroke,
            STROKE_OUTLINE,
        )?;
    }
    if let Some((port, output)) = port {
        let r = w.min(h) * 0.12;
        let (ax, ay) = (x + w * 0.3, y + h * 0.3);
        let arrow = [
            (ax - r, ay - r * 0.45),
            (ax, ay - r * 0.45),
            (ax, ay - r),
            (ax + r, ay),
            (ax, ay + r),
            (ax, ay + r * 0.45),
            (ax - r, ay + r * 0.45),
            (ax - r, ay - r * 0.45),
        ];
        marker_polygon(scene, &arrow, stroke, output)?;
        if port.is_collection == Some(true) {
            for offset in [-4.0_f32, 0.0, 4.0] {
                lines(
                    scene,
                    &[
                        (x + w / 2.0 + offset, y + h - 12.0),
                        (x + w / 2.0 + offset, y + h - 4.0),
                    ],
                    stroke,
                    1.2,
                )?;
            }
        }
    }
    if let Some(NodeDetails::DataObjectReference {
        data_object_ref: Some(id),
    }) = details
        && semantics
            .data_object(id)
            .is_some_and(|object| object.is_collection == Some(true))
    {
        for offset in [-4.0_f32, 0.0, 4.0] {
            lines(
                scene,
                &[
                    (x + w / 2.0 + offset, y + h - 12.0),
                    (x + w / 2.0 + offset, y + h - 4.0),
                ],
                stroke,
                1.2,
            )?;
        }
    }
    Ok(())
}

fn draw_event_definitions(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let Frame {
        document,
        semantics,
        ..
    } = *frame;
    let ShapeCtx {
        shape,
        node,
        details,
        marker_kind,
        x,
        y,
        w,
        h,
        stroke,
        fc,
        ..
    } = *ctx;
    let effective = match effective_event_definitions(details, document) {
        Ok(effective) => Some(effective),
        Err(()) => {
            diagnostics.push(format!(
                "Event {} has an unresolved event definition reference; marker omitted",
                shape.element
            ));
            None
        }
    };
    draw_new_markers(
        scene,
        marker_kind,
        details,
        effective,
        node.and_then(|node| node.activity.as_ref())
            .is_some_and(|activity| activity.instantiate == Some(true)),
        x,
        y,
        w,
        h,
        stroke,
        fc,
        diagnostics,
    )?;
    if semantics
        .participant(&shape.element)
        .and_then(|p| p.multiplicity.as_ref())
        .and_then(|m| m.maximum)
        .is_some_and(|maximum| maximum > 1)
    {
        for offset in [-3.0_f32, 0.0, 3.0] {
            lines(
                scene,
                &[
                    (x + w / 2.0 + offset, y + h - 9.0),
                    (x + w / 2.0 + offset, y + h - 3.0),
                ],
                stroke,
                1.0,
            )?;
        }
    }
    Ok(())
}

fn draw_label_and_hits(
    frame: &Frame<'_, '_>,
    ctx: &ShapeCtx<'_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
) -> Result<(), RenderError> {
    let Frame {
        document,
        diagram,
        origin,
        fg,
        shape_labels,
        family,
        ..
    } = *frame;
    let ShapeCtx {
        shape,
        node,
        kind,
        conversation,
        choreography,
        marker_kind,
        name,
        b,
        overlaid,
        is_lane_or_pool,
        ..
    } = *ctx;
    let (label_bounds, label_align) = shape_label_layout(
        shape,
        kind,
        marker_kind,
        node,
        choreography.is_some(),
        is_lane_or_pool,
    );
    let lb = &label_bounds;
    // An overlay fill replaces an explicit label colour so the ink always
    // contrasts with the bucket colour.
    let style = shape
        .label
        .as_ref()
        .and_then(|l| l.style.as_ref())
        .or(shape.style.as_ref())
        .filter(|_| overlaid.is_none());
    if shape_labels && label_visible(shape.label.as_ref(), shape.label_visibility) {
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
            width: lb.height,
            height: lb.width,
        };
        label(
            scene,
            fonts,
            family,
            name,
            if rotate_pool_label {
                &rotated_bounds
            } else {
                lb
            },
            if rotate_pool_label {
                (0.0, 0.0)
            } else {
                origin
            },
            style,
            label_font(document, diagram, shape.label.as_ref(), diagnostics),
            fg,
            label_align,
            diagnostics,
        )?;
        if rotate_pool_label && !name.is_empty() {
            scene.rotate_last_label(
                (lb.x - origin.0) as f32,
                (lb.y + lb.height - origin.1) as f32,
            );
        }
    }
    hits.push(HitRegion {
        key: ElementKey {
            document_id: document.id,
            xml_id: shape.element.clone(),
        },
        diagram_element_id: shape.id.selector(),
        label_id: None,
        is_label: false,
        geometry: match kind {
            Some(
                NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent,
            ) => HitGeometry::Ellipse(b.clone()),
            Some(
                NodeKind::ExclusiveGateway
                | NodeKind::ParallelGateway
                | NodeKind::InclusiveGateway
                | NodeKind::ComplexGateway
                | NodeKind::EventBasedGateway,
            ) => HitGeometry::Diamond(b.clone()),
            Some(NodeKind::DataObjectReference) => HitGeometry::DataObject(b.clone()),
            Some(NodeKind::DataStoreReference) => HitGeometry::DataStore(b.clone()),
            _ if conversation.is_some() => HitGeometry::Hexagon(b.clone()),
            _ => HitGeometry::Shape(b.clone()),
        },
    });
    if shape_labels
        && label_visible(shape.label.as_ref(), shape.label_visibility)
        && !name.is_empty()
    {
        hits.push(HitRegion {
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
            geometry: HitGeometry::Shape(lb.clone()),
        });
    }
    Ok(())
}
