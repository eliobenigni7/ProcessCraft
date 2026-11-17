//! Edge drawing shared by the full scene (exports) and the viewport renderer.
//!
//! With `EdgeFrame::clip` set, edges stay in absolute document coordinates and only the
//! parts inside the view are stroked; without it the whole route is drawn.

use crate::{DiagramScene, RenderError, TextEngine};
use processcraft_core::{
    bpmn_model::AssociationDirection,
    ids::ElementKey,
    model::{Bounds, BpmnDocument, Diagram, Edge, FlowKind, MessageVisibleKind, Point},
};
use tiny_skia::PathBuilder;

use super::appearance::{
    DARK_INK, DOTTED_DASH, MESSAGE_DASH, NON_INITIATING_FILL, STROKE_ARROW, color, low_contrast,
};
use super::hit::{HitGeometry, HitRegion};
use super::labels::{
    edge_label_layout, label, label_font, label_visible, last_segment, message_name_label,
};
use super::markers::{
    double_link, draw_default_slash, draw_flow_source_marker, envelope, open_arrow,
};
use super::primitives::{
    EDGE_RADIUS, clip_segment, clipped_runs, lines, polygon, polyline_midpoint, stroke_polyline,
};
use super::semantics::SemanticIndex;
use super::viewport::{CULL_MARGIN, Viewport};

/// Everything edge drawing reads besides the edge itself.
#[derive(Clone, Copy)]
pub(super) struct EdgeFrame<'a, 'd> {
    pub(super) document: &'d BpmnDocument,
    pub(super) diagram: &'a Diagram,
    pub(super) semantics: &'a SemanticIndex<'d>,
    /// Scene origin: the offset between scene and document (or view) coordinates.
    pub(super) origin: (f64, f64),
    pub(super) fg: [u8; 3],
    pub(super) bg: [u8; 3],
    pub(super) family: &'a str,
    /// Viewport rendering: the view in document coordinates.
    pub(super) clip: Option<Viewport>,
}

impl EdgeFrame<'_, '_> {
    fn local(&self, p: &Point) -> (f32, f32) {
        match self.clip {
            Some(view) => (
                (p.x - view.x - self.origin.0) as f32,
                (p.y - view.y - self.origin.1) as f32,
            ),
            None => ((p.x - self.origin.0) as f32, (p.y - self.origin.1) as f32),
        }
    }
    fn points(&self, edge: &Edge) -> Vec<(f32, f32)> {
        edge.waypoints.iter().map(|p| self.local(p)).collect()
    }
    /// Label origin: labels carry document coordinates.
    fn label_origin(&self) -> (f64, f64) {
        match self.clip {
            Some(view) => (view.x + self.origin.0, view.y + self.origin.1),
            None => self.origin,
        }
    }
    fn in_view(&self, p: &Point) -> bool {
        self.clip.is_none_or(|view| {
            p.x >= view.x
                && p.x <= view.x + view.width
                && p.y >= view.y
                && p.y <= view.y + view.height
        })
    }
    fn bounds_in_view(&self, b: &Bounds) -> bool {
        self.clip.is_none_or(|view| {
            b.x <= view.x + view.width
                && b.y <= view.y + view.height
                && b.x + b.width >= view.x
                && b.y + b.height >= view.y
        })
    }
    /// Hit bounds are view-local while rendering a viewport (the caller shifts them back).
    fn hit_bounds(&self, b: &Bounds) -> Bounds {
        let mut local = b.clone();
        if let Some(view) = self.clip {
            local.x -= view.x;
            local.y -= view.y;
        }
        local
    }
    /// Parts of the route to stroke, each with its dash phase.
    fn runs(&self, edge: &Edge, points: &[(f32, f32)]) -> Vec<(Vec<(f32, f32)>, f32)> {
        match self.clip {
            Some(view) => clipped_runs(&edge.waypoints, view, self.origin),
            None => vec![(points.to_vec(), 0.0)],
        }
    }
}

fn edge_hit(f: &EdgeFrame<'_, '_>, edge: &Edge) -> HitRegion {
    HitRegion {
        key: ElementKey {
            document_id: f.document.id,
            xml_id: edge.element.clone(),
        },
        diagram_element_id: edge.id.selector(),
        label_id: None,
        is_label: false,
        geometry: HitGeometry::Edge(edge.waypoints.clone()),
    }
}

fn edge_label_hit(f: &EdgeFrame<'_, '_>, edge: &Edge, bounds: &Bounds) -> HitRegion {
    HitRegion {
        key: ElementKey {
            document_id: f.document.id,
            xml_id: edge.element.clone(),
        },
        diagram_element_id: edge.id.selector(),
        label_id: edge
            .label
            .as_ref()
            .and_then(|label| label.id.as_ref())
            .map(|id| id.selector()),
        is_label: true,
        geometry: HitGeometry::Shape(f.hit_bounds(bounds)),
    }
}

/// Draws the name of an edge when its label is visible and inside the view.
fn draw_edge_name(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    edge: &Edge,
    name: Option<&str>,
) -> Result<(), RenderError> {
    if label_visible(edge.label.as_ref(), edge.label_visibility)
        && name.is_some_and(|name| !name.is_empty())
        && let Some((bounds, align)) = edge_label_layout(edge)
        && f.bounds_in_view(&bounds)
    {
        label(
            scene,
            fonts,
            f.family,
            name.unwrap_or(""),
            &bounds,
            f.label_origin(),
            edge.label
                .as_ref()
                .and_then(|l| l.style.as_ref())
                .or(edge.style.as_ref()),
            label_font(f.document, f.diagram, edge.label.as_ref(), diagnostics),
            f.fg,
            align,
            diagnostics,
        )?;
        hits.push(edge_label_hit(f, edge, &bounds));
    }
    Ok(())
}

pub(super) fn draw_edge(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    edge: &Edge,
) -> Result<(), RenderError> {
    let semantics = f.semantics;
    if semantics.data_association(&edge.element).is_some() {
        draw_data_association(f, scene, hits, edge)
    } else if let Some(link) = semantics.conversation_link(&edge.element) {
        if semantics.resolved(&link.source).is_none() || semantics.resolved(&link.target).is_none()
        {
            diagnostics.push(format!(
                "Conversation link {} has an unresolved endpoint; authored DI route is retained",
                link.id
            ));
        }
        draw_conversation_link(
            f,
            scene,
            hits,
            diagnostics,
            fonts,
            edge,
            link.name.as_deref(),
        )
    } else if let Some(flow) = semantics.flow(&edge.element) {
        draw_flow(f, scene, hits, diagnostics, fonts, edge, flow)
    } else {
        // The viewport leaves unknown edges to the scene diagnostics of the full build.
        if f.clip.is_none() {
            diagnostics.push(format!("Unknown edge element {}", edge.element));
        }
        Ok(())
    }
}

fn draw_data_association(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    edge: &Edge,
) -> Result<(), RenderError> {
    let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), f.fg);
    let points = f.points(edge);
    let runs = f.runs(edge, &points);
    for (run, offset) in &runs {
        stroke_polyline(scene, run, stroke, EDGE_RADIUS, Some(DOTTED_DASH), *offset)?;
    }
    if edge.waypoints.last().is_none_or(|end| f.in_view(end)) {
        open_arrow(scene, &points, stroke)?;
    }
    if !runs.is_empty() {
        hits.push(edge_hit(f, edge));
    }
    Ok(())
}

fn draw_conversation_link(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    edge: &Edge,
    name: Option<&str>,
) -> Result<(), RenderError> {
    let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), f.fg);
    let visible = match f.clip {
        None => {
            double_link(scene, &f.points(edge), stroke)?;
            true
        }
        Some(view) => {
            let mut visible = false;
            for pair in edge.waypoints.windows(2) {
                if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
                    let local = |(x, y): (f64, f64)| {
                        (
                            (x - view.x - f.origin.0) as f32,
                            (y - view.y - f.origin.1) as f32,
                        )
                    };
                    double_link(scene, &[local(a), local(b)], stroke)?;
                    visible = true;
                }
            }
            visible
        }
    };
    draw_edge_name(f, scene, hits, diagnostics, fonts, edge, name)?;
    if visible {
        hits.push(edge_hit(f, edge));
    }
    Ok(())
}

/// Message envelope at the route midpoint, with its name. Warns when an explicit stroke
/// contrasts too little with the non-initiating envelope fill.
fn draw_message_envelope(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    edge: &Edge,
    flow: &processcraft_core::model::Flow,
    points: &[(f32, f32)],
) -> Result<(), RenderError> {
    let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), f.fg);
    let Some((cx, cy)) = polyline_midpoint(points) else {
        return Ok(());
    };
    if let Some(view) = f.clip
        && !(cx >= -CULL_MARGIN
            && cy >= -CULL_MARGIN
            && cx <= view.width as f32 + CULL_MARGIN
            && cy <= view.height as f32 + CULL_MARGIN)
    {
        return Ok(());
    }
    let filled = matches!(
        edge.message_visible_kind,
        Some(MessageVisibleKind::NonInitiating)
    );
    let explicit = edge.style.as_ref().and_then(|s| s.stroke.as_deref());
    let envelope_stroke = if filled && explicit.is_none() {
        DARK_INK
    } else {
        stroke
    };
    if filled && explicit.is_some() && low_contrast(envelope_stroke, NON_INITIATING_FILL) {
        diagnostics.push(format!(
            "Message edge {} explicit stroke has low contrast against the envelope fill",
            edge.id
        ));
    }
    envelope(scene, cx, cy, envelope_stroke, filled, f.bg)?;
    if let Some(text) = f.semantics.message_name(flow) {
        message_name_label(
            scene,
            fonts,
            f.family,
            text,
            (cx, cy),
            f.label_origin(),
            f.fg,
            diagnostics,
        )?;
    }
    Ok(())
}

fn draw_flow(
    f: &EdgeFrame<'_, '_>,
    scene: &mut DiagramScene,
    hits: &mut Vec<HitRegion>,
    diagnostics: &mut Vec<String>,
    fonts: &mut TextEngine,
    edge: &Edge,
    flow: &processcraft_core::model::Flow,
) -> Result<(), RenderError> {
    let semantics = f.semantics;
    if semantics.resolved(&flow.source).is_none() || semantics.resolved(&flow.target).is_none() {
        diagnostics.push(format!(
            "Flow {} has an unresolved endpoint; authored DI route is retained",
            flow.id
        ));
    }
    let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), f.fg);
    let pts = f.points(edge);
    let dash = match flow.kind {
        FlowKind::Sequence => None,
        FlowKind::Message => Some(MESSAGE_DASH),
        _ => Some(DOTTED_DASH),
    };
    let runs = f.runs(edge, &pts);
    for (run, offset) in &runs {
        stroke_polyline(scene, run, stroke, EDGE_RADIUS, dash, *offset)?;
    }
    let visible = !runs.is_empty();
    let start_in_view = edge.waypoints.first().is_some_and(|p| f.in_view(p));
    let end_in_view = edge.waypoints.last().is_some_and(|p| f.in_view(p));
    if matches!(flow.kind, FlowKind::Message) && edge.message_visible_kind.is_some() {
        draw_message_envelope(f, scene, diagnostics, fonts, edge, flow, &pts)?;
    }
    if visible
        && start_in_view
        && matches!(flow.kind, FlowKind::Sequence)
        && semantics.is_default_flow(flow)
    {
        draw_default_slash(scene, &pts, stroke)?;
    }
    if visible
        && start_in_view
        && (matches!(flow.kind, FlowKind::Message) || semantics.conditional_flow_marker(flow))
    {
        draw_flow_source_marker(
            scene,
            &pts,
            stroke,
            f.bg,
            matches!(flow.kind, FlowKind::Message),
        )?;
    }
    if visible
        && end_in_view
        && (!matches!(flow.kind, FlowKind::Association)
            || matches!(
                flow.association_direction,
                Some(AssociationDirection::One | AssociationDirection::Both)
            ))
        && let Some((a, b)) = last_segment(&pts)
    {
        let angle = (b.1 - a.1).atan2(b.0 - a.0);
        let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
        if matches!(flow.kind, FlowKind::Sequence) {
            let mut arrow = PathBuilder::new();
            polygon(&mut arrow, &[wing(0.5), b, wing(-0.5)]);
            if let Some(path) = arrow.finish() {
                scene.add_path(path, stroke)?;
            }
        } else {
            lines(scene, &[wing(0.5), b, wing(-0.5)], stroke, STROKE_ARROW)?;
        }
    }
    if visible
        && start_in_view
        && matches!(flow.kind, FlowKind::Association)
        && matches!(flow.association_direction, Some(AssociationDirection::Both))
        && let Some((a, b)) = last_segment(&pts.iter().rev().copied().collect::<Vec<_>>())
    {
        let angle = (b.1 - a.1).atan2(b.0 - a.0);
        let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
        lines(scene, &[wing(0.5), b, wing(-0.5)], stroke, STROKE_ARROW)?;
    }
    draw_edge_name(
        f,
        scene,
        hits,
        diagnostics,
        fonts,
        edge,
        flow.name.as_deref(),
    )?;
    if visible {
        hits.push(edge_hit(f, edge));
    }
    Ok(())
}
