//! `BpmnViewport`: culled, viewport-sized scene slices for the canvas.

use crate::{DiagramScene, RenderError, TextEngine};
use processcraft_core::{
    bpmn_model::AssociationDirection,
    ids::{ElementKey, ElementSelector},
    model::{Bounds, BpmnDocument, Capability, FlowKind, NodeKind},
    overlay::Overlay,
};
use tiny_skia::PathBuilder;

use super::appearance::{
    Appearance, DARK_INK, DOTTED_DASH, MESSAGE_DASH, NON_INITIATING_FILL, STROKE_ARROW, color,
    low_contrast,
};
use super::hit::{HitGeometry, HitRegion};
use super::labels::{
    LabelAlign, edge_label_layout, group_label_collision, label, label_font, label_visible,
    message_name_label, shape_label_layout,
};
use super::markers::{
    double_link, draw_default_slash, draw_flow_source_marker, envelope, open_arrow,
    self_reference_resolves,
};
use super::overlay::OverlayView;
use super::primitives::{
    EDGE_RADIUS, clip_segment, clipped_runs, lines, polygon, polyline_midpoint, stroke_polyline,
};
use super::scene::{BpmnScene, BuildOptions};
use super::semantics::{
    CalledTarget, DI_EXTENSION_DIAGNOSTIC, SemanticIndex, called_target, data_label,
    has_uninterpreted_di, node_display_name,
};

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
                let owner = match &shape.choreography_activity_shape {
                    Some(processcraft_core::model::DiReference::Local(id)) => {
                        original.shapes.iter().find(|s| &s.id == id)
                    }
                    Some(processcraft_core::model::DiReference::Qualified(q))
                        if document
                            .definitions
                            .as_ref()
                            .is_some_and(|defs| defs.target_namespace == q.namespace) =>
                    {
                        original
                            .shapes
                            .iter()
                            .find(|s| s.id.xml_id() == Some(&q.local))
                    }
                    _ => None,
                };
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
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let port = semantics.port(&shape.element);
            let store = document.data_stores.iter().find(|store| {
                store
                    .id
                    .as_ref()
                    .is_some_and(|identity| identity == &shape.element)
            });
            let kind = node
                .map(|node| node.kind)
                .or(port.map(|_| NodeKind::DataObjectReference))
                .or(store.map(|_| NodeKind::DataStoreReference));
            let marker_kind = match called_target(node, document) {
                CalledTarget::Global(task_kind) => Some(task_kind),
                _ => kind,
            };
            let conversation = semantics.conversation(&shape.element);
            let choreography = semantics.choreography(&shape.element);
            let node_name = node.map(|node| node_display_name(node, document));
            if node_name.as_ref().is_some_and(|(_, truncated)| *truncated) {
                built.diagnostics.push(format!(
                    "Annotation {} text exceeds display limit",
                    shape.element
                ));
            }
            let group_text = semantics.group_label(&shape.element, document);
            let mut name = group_text
                .as_deref()
                .or_else(|| {
                    choreography.map(|(_, activity)| activity.name.as_deref().unwrap_or(""))
                })
                .or_else(|| conversation.map(|node| node.name.as_deref().unwrap_or("")))
                .or(node_name.as_ref().map(|(text, _)| text.as_ref()))
                .or_else(|| port.map(|(port, _)| port.name.as_deref().unwrap_or("")))
                .or_else(|| store.map(|store| store.name.as_deref().unwrap_or("")))
                .or_else(|| {
                    semantics
                        .lane(&shape.element)
                        .map(|(lane, _)| lane.name.as_deref().unwrap_or(""))
                })
                .or_else(|| {
                    semantics
                        .choreographies
                        .iter()
                        .flat_map(|(c, _)| &c.participants)
                        .find(|p| p.id == shape.element)
                        .map(|p| p.name.as_deref().unwrap_or(""))
                });
            if let Some(p) = semantics.participant(&shape.element) {
                name = Some(p.name.as_deref().unwrap_or(""));
            }
            let Some(name) = name else { continue };
            let display_name = if let Some((port, _)) = port {
                match port
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|name| !name.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else if let Some(store) = store {
                match store
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|state| !state.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else {
                data_label(node, name, semantics)
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
        for edge in &original.edges {
            if let Some(ranks) = &paint_ranks {
                built
                    .scene
                    .set_paint_rank(semantics.edge_paint_rank(ranks, edge));
            }
            if semantics.edge_visibility(edge, original).0 {
                continue;
            }
            if semantics.data_association(&edge.element).is_some() {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                let runs = clipped_runs(&edge.waypoints, view, built.origin);
                let visible = !runs.is_empty();
                for (points, offset) in &runs {
                    stroke_polyline(
                        &mut built.scene,
                        points,
                        stroke,
                        EDGE_RADIUS,
                        Some(DOTTED_DASH),
                        *offset,
                    )?;
                }
                if let Some(end) = edge.waypoints.last()
                    && end.x >= view.x
                    && end.x <= view.x + view.width
                    && end.y >= view.y
                    && end.y <= view.y + view.height
                    && edge.waypoints.len() > 1
                {
                    let pts: Vec<_> = edge
                        .waypoints
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    open_arrow(&mut built.scene, &pts, stroke)?;
                }
                if visible {
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: None,
                        is_label: false,
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            if let Some(link) = semantics.conversation_link(&edge.element) {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                if semantics.resolved(&link.source).is_none()
                    || semantics.resolved(&link.target).is_none()
                {
                    built.diagnostics.push(format!("Conversation link {} has an unresolved endpoint; authored DI route is retained", link.id));
                }
                let mut visible = false;
                for pair in edge.waypoints.windows(2) {
                    if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
                        let points = [
                            (
                                (a.0 - view.x - built.origin.0) as f32,
                                (a.1 - view.y - built.origin.1) as f32,
                            ),
                            (
                                (b.0 - view.x - built.origin.0) as f32,
                                (b.1 - view.y - built.origin.1) as f32,
                            ),
                        ];
                        double_link(&mut built.scene, &points, stroke)?;
                        visible = true;
                    }
                }
                if label_visible(edge.label.as_ref(), edge.label_visibility)
                    && link.name.as_deref().is_some_and(|name| !name.is_empty())
                    && let Some((bounds, align)) = edge_label_layout(edge)
                {
                    let bounds = &bounds;
                    if inside(bounds) {
                        label(
                            &mut built.scene,
                            fonts,
                            family,
                            link.name.as_deref().unwrap_or(""),
                            bounds,
                            (view.x + built.origin.0, view.y + built.origin.1),
                            edge.label
                                .as_ref()
                                .and_then(|l| l.style.as_ref())
                                .or(edge.style.as_ref()),
                            label_font(
                                document,
                                original,
                                edge.label.as_ref(),
                                &mut built.diagnostics,
                            ),
                            fg,
                            align,
                            &mut built.diagnostics,
                        )?;
                        if label_visible(edge.label.as_ref(), edge.label_visibility) {
                            let mut local = bounds.clone();
                            rebase(&mut local);
                            built.hits.push(HitRegion {
                                key: ElementKey {
                                    document_id: document.id,
                                    xml_id: edge.element.clone(),
                                },
                                diagram_element_id: edge.id.selector(),
                                label_id: edge
                                    .label
                                    .as_ref()
                                    .and_then(|label| label.id.as_ref())
                                    .map(|id| id.selector()),
                                is_label: true,
                                geometry: HitGeometry::Shape(local),
                            });
                        }
                    }
                }
                if visible {
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: None,
                        is_label: false,
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            let Some(flow) = semantics.flow(&edge.element) else {
                continue;
            };
            if semantics.resolved(&flow.source).is_none()
                || semantics.resolved(&flow.target).is_none()
            {
                built.diagnostics.push(format!(
                    "Flow {} has an unresolved endpoint; authored DI route is retained",
                    flow.id
                ));
            }
            let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
            let dash = match flow.kind {
                FlowKind::Sequence => None,
                FlowKind::Message => Some(MESSAGE_DASH),
                _ => Some(DOTTED_DASH),
            };
            let runs = clipped_runs(&edge.waypoints, view, built.origin);
            let segments = runs.len();
            for (points, offset) in &runs {
                stroke_polyline(&mut built.scene, points, stroke, EDGE_RADIUS, dash, *offset)?;
            }
            if matches!(flow.kind, FlowKind::Message)
                && let Some(visible) = edge.message_visible_kind
            {
                let points: Vec<_> = edge
                    .waypoints
                    .iter()
                    .map(|p| {
                        (
                            (p.x - view.x - built.origin.0) as f32,
                            (p.y - view.y - built.origin.1) as f32,
                        )
                    })
                    .collect();
                if let Some((cx, cy)) = polyline_midpoint(&points)
                    && cx >= -CULL_MARGIN
                    && cy >= -CULL_MARGIN
                    && cx <= view.width as f32 + CULL_MARGIN
                    && cy <= view.height as f32 + CULL_MARGIN
                {
                    let background = appearance.paper();
                    let filled = matches!(
                        visible,
                        processcraft_core::model::MessageVisibleKind::NonInitiating
                    );
                    let explicit = edge.style.as_ref().and_then(|s| s.stroke.as_deref());
                    let envelope_stroke = if filled && explicit.is_none() {
                        DARK_INK
                    } else {
                        stroke
                    };
                    if filled
                        && explicit.is_some()
                        && low_contrast(envelope_stroke, NON_INITIATING_FILL)
                    {
                        built.diagnostics.push(format!("Message edge {} explicit stroke has low contrast against the envelope fill", edge.id));
                    }
                    envelope(
                        &mut built.scene,
                        cx,
                        cy,
                        envelope_stroke,
                        filled,
                        background,
                    )?;
                    if let Some(text) = semantics.message_name(flow) {
                        message_name_label(
                            &mut built.scene,
                            fonts,
                            family,
                            text,
                            (cx, cy),
                            (view.x + built.origin.0, view.y + built.origin.1),
                            fg,
                            &mut built.diagnostics,
                        )?;
                    }
                }
            }
            if segments > 0
                && matches!(flow.kind, FlowKind::Sequence)
                && semantics.is_default_flow(flow)
                && edge.waypoints.len() > 1
            {
                let a = &edge.waypoints[0];
                if a.x >= view.x
                    && a.x <= view.x + view.width
                    && a.y >= view.y
                    && a.y <= view.y + view.height
                {
                    let points: Vec<_> = edge.waypoints[..2]
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    draw_default_slash(&mut built.scene, &points, stroke)?;
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && (matches!(flow.kind, FlowKind::Message)
                    || semantics.conditional_flow_marker(flow))
            {
                let a = &edge.waypoints[0];
                if a.x >= view.x
                    && a.x <= view.x + view.width
                    && a.y >= view.y
                    && a.y <= view.y + view.height
                {
                    let points: Vec<_> = edge.waypoints[..2]
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    draw_flow_source_marker(
                        &mut built.scene,
                        &points,
                        stroke,
                        appearance.paper(),
                        matches!(flow.kind, FlowKind::Message),
                    )?;
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && (!matches!(flow.kind, FlowKind::Association)
                    || matches!(
                        flow.association_direction,
                        Some(AssociationDirection::One | AssociationDirection::Both)
                    ))
            {
                let b = &edge.waypoints[edge.waypoints.len() - 1];
                let a = edge
                    .waypoints
                    .iter()
                    .rev()
                    .find(|p| p.x != b.x || p.y != b.y)
                    .unwrap_or(&edge.waypoints[edge.waypoints.len() - 2]);
                if b.x >= view.x
                    && b.x <= view.x + view.width
                    && b.y >= view.y
                    && b.y <= view.y + view.height
                {
                    let end = (
                        (b.x - view.x - built.origin.0) as f32,
                        (b.y - view.y - built.origin.1) as f32,
                    );
                    let angle = (b.y - a.y).atan2(b.x - a.x) as f32;
                    let wing = |v: f32| {
                        (
                            end.0 - 9.0 * (angle + v).cos(),
                            end.1 - 9.0 * (angle + v).sin(),
                        )
                    };
                    if matches!(flow.kind, FlowKind::Sequence) {
                        let mut p = PathBuilder::new();
                        polygon(&mut p, &[wing(0.5), end, wing(-0.5)]);
                        if let Some(p) = p.finish() {
                            built.scene.add_path(p, stroke)?;
                        }
                    } else {
                        lines(
                            &mut built.scene,
                            &[wing(0.5), end, wing(-0.5)],
                            stroke,
                            STROKE_ARROW,
                        )?;
                    }
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && matches!(flow.kind, FlowKind::Association)
                && matches!(flow.association_direction, Some(AssociationDirection::Both))
            {
                let b = &edge.waypoints[0];
                let a = edge
                    .waypoints
                    .iter()
                    .find(|p| p.x != b.x || p.y != b.y)
                    .unwrap_or(&edge.waypoints[1]);
                if b.x >= view.x
                    && b.x <= view.x + view.width
                    && b.y >= view.y
                    && b.y <= view.y + view.height
                {
                    let end = (
                        (b.x - view.x - built.origin.0) as f32,
                        (b.y - view.y - built.origin.1) as f32,
                    );
                    let angle = (b.y - a.y).atan2(b.x - a.x) as f32;
                    let wing = |v: f32| {
                        (
                            end.0 - 9.0 * (angle + v).cos(),
                            end.1 - 9.0 * (angle + v).sin(),
                        )
                    };
                    lines(
                        &mut built.scene,
                        &[wing(0.5), end, wing(-0.5)],
                        stroke,
                        STROKE_ARROW,
                    )?;
                }
            }
            if label_visible(edge.label.as_ref(), edge.label_visibility)
                && flow.name.as_deref().is_some_and(|name| !name.is_empty())
                && let Some((bounds, align)) = edge_label_layout(edge)
            {
                let bounds = &bounds;
                if inside(bounds) {
                    let mut local = bounds.clone();
                    rebase(&mut local);
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: edge
                            .label
                            .as_ref()
                            .and_then(|label| label.id.as_ref())
                            .map(|id| id.selector()),
                        is_label: true,
                        geometry: HitGeometry::Shape(local),
                    });
                    label(
                        &mut built.scene,
                        fonts,
                        family,
                        flow.name.as_deref().unwrap_or(""),
                        bounds,
                        (view.x + built.origin.0, view.y + built.origin.1),
                        edge.label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(edge.style.as_ref()),
                        label_font(
                            document,
                            original,
                            edge.label.as_ref(),
                            &mut built.diagnostics,
                        ),
                        fg,
                        align,
                        &mut built.diagnostics,
                    )?;
                }
            }
            if segments == 0 {
                continue;
            }
            built.hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: edge.element.clone(),
                },
                diagram_element_id: edge.id.selector(),
                label_id: None,
                is_label: false,
                geometry: HitGeometry::Edge(edge.waypoints.clone()),
            });
        }
        let bg = appearance.paper();
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
