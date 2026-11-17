//! The diagram canvas: raster/texture presentation, hit-testing, gestures, context pad, palette popups and inline label editing.
//! Gestures are turned into core modeling mutations (`Connect`, `CreateAt`, `Reconnect`, `ResizeShape`, `Rename`, ...); the canvas never edits the model directly.
use super::{ProcessCraft, View, advance_drag, begin_drag, diagram_extents, finish_drag};
use crate::theme::{CONNECT_OK, CONNECT_REFUSED, Theme};
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use processcraft_core::{
    arrange::{Alignment, Distribution},
    commands::Mutation,
    containers::{Axis, LaneLocation},
    ids::ElementSelector,
    model::{Bounds, FlowKind, NodeKind, Point, Shape},
};
use processcraft_render::{Appearance, HitGeometry, Viewport};
use std::sync::Arc;
mod geometry;
mod icons;
mod render;
#[cfg(test)]
mod tests;
pub(super) use geometry::vacant_bounds;
use geometry::*;
pub(super) use icons::paint_replace_icon;
use icons::*;

const MIN_ZOOM: f32 = *processcraft_render::ZOOM_RANGE.start();
const MAX_ZOOM: f32 = *processcraft_render::ZOOM_RANGE.end();
pub(super) const ZOOM_STEP: f32 = 1.25;

/// Model origin, zoom and logical size of a rendered texture, so it can be moved and scaled on
/// the GPU under another camera. `request` is the canvas camera and zoom it was rendered for.
#[derive(Clone, Copy)]
pub(super) struct TextureView {
    camera: (f64, f64),
    zoom: f32,
    size: Vec2,
    request: ((f64, f64), f32),
}

/// `PROCESSCRAFT_CANVAS_TRACE=1` logs, per shown raster, submit-to-display latency and the
/// UI-thread texture update time to stderr.
fn trace_canvas() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("PROCESSCRAFT_CANVAS_TRACE").is_some_and(|v| v == "1"))
}

enum Gesture {
    Pan,
    BoxSelect(Pos2),
    Connect {
        element: ElementSelector,
        bounds: Bounds,
    },
    Resize(ResizeDrag),
    /// Space tool drag from this model point.
    Space((f64, f64)),
    /// Dragging a waypoint or segment of the selected edge (`edge` is its DI id).
    Bendpoint {
        edge: ElementSelector,
        flow: ElementSelector,
        points: Vec<Point>,
        part: EdgePart,
        /// Bounds and kinds of the source and target shapes.
        ends: [Option<Bounds>; 2],
        kinds: [Option<NodeKind>; 2],
        /// Model point where the drag started (auto-scroll safe).
        start: (f64, f64),
    },
}

struct ResizeDrag {
    shape: ElementSelector,
    bounds: Bounds,
    /// Handle direction per axis (-1 west/north, 0 none, 1 east/south).
    handle: (i8, i8),
    min: (f64, f64),
    /// Keep the width/height ratio (events, gateways, data references).
    aspect: bool,
    /// Model point where the drag started, so panning or auto-scroll mid-drag does not skew it.
    start: (f64, f64),
    /// Extent the shape must keep covering: contained shapes plus the core's child padding.
    keep: Option<Bounds>,
}
impl ResizeDrag {
    /// Bounds with the pointer at model point `at`, and the snap used for guide lines. Shift
    /// locks the ratio, Alt resizes around the centre, Ctrl turns snapping off; only the
    /// dragged edges snap (to siblings like a move, then to the optional grid).
    fn preview(
        &self,
        d: &processcraft_core::model::BpmnDocument,
        diagram: &processcraft_core::model::Diagram,
        at: (f64, f64),
        modifiers: egui::Modifiers,
        tolerance: f64,
        grid: bool,
    ) -> (Bounds, processcraft_core::geometry::Snap) {
        let aspect = self.aspect || modifiers.shift;
        let resize = |dx: f64, dy: f64| {
            resize_bounds(
                &self.bounds,
                self.handle,
                (dx, dy),
                self.min,
                aspect,
                modifiers.alt,
                self.keep.as_ref(),
            )
        };
        let (mut dx, mut dy) = (at.0 - self.start.0, at.1 - self.start.1);
        let mut snap = processcraft_core::geometry::Snap::default();
        if !aspect && !modifiers.ctrl {
            let r = resize(dx, dy);
            let (hx, hy) = self.handle;
            let corner = Bounds {
                x: if hx < 0 { r.x } else { r.x + r.width },
                y: if hy < 0 { r.y } else { r.y + r.height },
                width: 0.0,
                height: 0.0,
            };
            let items = [(self.shape.clone(), self.bounds.clone())];
            snap = snap_drag(d, diagram, &items, &corner, tolerance, grid);
            if hx == 0 {
                (snap.dx, snap.guide_x) = (0.0, None);
            }
            if hy == 0 {
                (snap.dy, snap.guide_y) = (0.0, None);
            }
            dx += snap.dx;
            dy += snap.dy;
        }
        (resize(dx, dy), snap)
    }
}
struct LabelEdit {
    element: ElementSelector,
    text: String,
    original: String,
    center: (f64, f64),
    width: f64,
    focused: bool,
    /// Text annotation body: multi-line, Ctrl+Enter or focus loss commits.
    annotation: bool,
    /// Editor height in diagram units (annotations).
    height: f64,
}
/// A palette entry: the element kind and, for catch and boundary events, the trigger the user
/// picked (message, timer) instead of a silent default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaletteEntry {
    pub(super) kind: NodeKind,
    pub(super) trigger: Option<processcraft_core::replace::Trigger>,
}
impl From<NodeKind> for PaletteEntry {
    fn from(kind: NodeKind) -> Self {
        Self {
            kind,
            trigger: None,
        }
    }
}
/// Event definition the palette gives a picked trigger: a message without a message reference,
/// a timer without a time (the user sets it in the properties).
fn trigger_definition(
    trigger: processcraft_core::replace::Trigger,
) -> Option<processcraft_core::bpmn_model::EventDefinition> {
    use processcraft_core::{bpmn_model::EventDefinition, replace::Trigger};
    match trigger {
        Trigger::Message => Some(EventDefinition::Message {
            id: None,
            base: None,
            message_ref: None,
            operation_ref: None,
        }),
        Trigger::Timer => Some(EventDefinition::Timer {
            id: None,
            base: None,
            kind: None,
            expression: None,
        }),
        _ => None,
    }
}
/// Give an event node the picked trigger's definition.
pub(super) fn apply_trigger(
    node: &mut processcraft_core::model::Node,
    trigger: Option<processcraft_core::replace::Trigger>,
) {
    if let Some(definition) = trigger.and_then(trigger_definition)
        && let Some(processcraft_core::bpmn_model::NodeDetails::Event { definitions, .. }) =
            &mut node.details
    {
        *definitions = vec![definition];
    }
}
/// `SetNodeDetails` giving the node created by `batch` (a core builder's output) the picked
/// trigger, to submit in the same batch after the create mutation.
pub(super) fn trigger_mutation(
    batch: &[Mutation],
    node: &ElementSelector,
    trigger: Option<processcraft_core::replace::Trigger>,
) -> Option<Mutation> {
    trigger?;
    batch.iter().find_map(|m| match m {
        Mutation::CreateElement {
            document_id,
            element,
            ..
        } if element.id == *node => {
            let mut element = element.clone();
            apply_trigger(&mut element, trigger);
            Some(Mutation::SetNodeDetails {
                document_id: *document_id,
                element_id: node.clone(),
                details: element.details,
            })
        }
        _ => None,
    })
}
/// Inline editor for `element` (bpmn-js direct editing): its name, or a text annotation's text
/// (multi-line). Placed over `at` (the hit region) or the element's shape, or the middle of its
/// edge. Associations have no label (`Err(None)`); an annotation whose text is rich XML content
/// is refused with the reason, so it is never flattened.
fn label_edit(
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    element: &ElementSelector,
    at: Option<&Bounds>,
) -> Result<LabelEdit, Option<&'static str>> {
    use processcraft_core::{bpmn_model::NodeDetails, queries};
    if queries::find_flow(d, element).is_some_and(|f| f.kind == FlowKind::Association) {
        return Err(None);
    }
    let shape = diagram.shapes.iter().find(|s| s.element == *element);
    let (center, width, height) = if let Some(b) = at.or(shape.map(|s| &s.bounds)) {
        (
            (b.x + b.width / 2.0, b.y + b.height / 2.0),
            b.width,
            b.height,
        )
    } else {
        let points = &diagram
            .edges
            .iter()
            .find(|e| e.element == *element)
            .ok_or(None)?
            .waypoints;
        let mid = points.get(points.len() / 2).ok_or(None)?;
        let prev = &points[(points.len() / 2).saturating_sub(1)];
        (((mid.x + prev.x) / 2.0, (mid.y + prev.y) / 2.0), 120.0, 0.0)
    };
    let node = queries::find_node(d, element);
    let annotation = node.filter(|n| n.kind == NodeKind::TextAnnotation);
    let original = match annotation.map(|n| &n.details) {
        Some(Some(NodeDetails::TextAnnotation {
            text: Some(body), ..
        })) => body
            .plain_text()
            .ok_or(Some(
                "This annotation has formatted content; edit it in the Properties panel.",
            ))?
            .to_owned(),
        _ => queries::element_name(d, element)
            .unwrap_or_default()
            .to_owned(),
    };
    Ok(LabelEdit {
        element: element.clone(),
        text: original.clone(),
        original,
        center,
        width,
        focused: false,
        annotation: annotation.is_some(),
        height: height.max(60.0),
    })
}
/// Popup offering elements to create where a connection was dropped on empty canvas.
struct AppendMenu {
    source: ElementSelector,
    at: Point,
    screen: Pos2,
}
/// Entries of the append popup (bpmn-js append menu subset).
const APPEND_ENTRIES: [(&str, NodeKind, Option<processcraft_core::replace::Trigger>); 8] = [
    ("Task", NodeKind::Task, None),
    ("User task", NodeKind::UserTask, None),
    ("Exclusive gateway", NodeKind::ExclusiveGateway, None),
    ("Parallel gateway", NodeKind::ParallelGateway, None),
    (
        "Intermediate throw event",
        NodeKind::IntermediateThrowEvent,
        None,
    ),
    (
        "Message intermediate catch event",
        NodeKind::IntermediateCatchEvent,
        Some(processcraft_core::replace::Trigger::Message),
    ),
    (
        "Timer intermediate catch event",
        NodeKind::IntermediateCatchEvent,
        Some(processcraft_core::replace::Trigger::Timer),
    ),
    ("End event", NodeKind::EndEvent, None),
];
/// Whether `text` (from the OS clipboard) is a BPMN 2.0 XML document.
fn looks_like_bpmn(text: &str) -> bool {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    text.starts_with('<')
        && text.contains("http://www.omg.org/spec/BPMN/20100524/MODEL")
        && text.contains("definitions")
}

#[derive(Default)]
pub(super) struct CanvasState {
    gesture: Option<Gesture>,
    label_edit: Option<LabelEdit>,
    revealed: Option<ElementSelector>,
    view: Option<TextureView>,
    pending: Vec<(String, TextureView, std::time::Instant)>,
    /// Key and view of the latest raster asked for.
    submitted: Option<(String, TextureView)>,
    /// Camera and zoom seen last, and since when.
    moved: Option<((f64, f64), f32, std::time::Instant)>,
    scene: String,
    size: Vec2,
    rect: Option<Rect>,
    /// In-process clipboard of copied diagram content.
    clipboard: Option<processcraft_core::clipboard::Clipboard>,
    /// Canvas search query while the search overlay is open, and whether it still needs focus.
    search: Option<String>,
    search_focus: bool,
    /// Open replace popup (bpmn-js wrench menu).
    replace: Option<ReplaceMenu>,
    /// Snap moved shapes to a 10 px grid (View menu).
    pub(super) grid_snap: bool,
    /// Palette entry being dragged onto the canvas.
    pub(super) palette_drag: Option<PaletteEntry>,
    /// Click-to-connect: the pad's connect entry was clicked on this shape; the next click on
    /// a target connects.
    connect_click: Option<(ElementSelector, Bounds)>,
    /// Append popup after a connection was dropped on empty canvas.
    append_menu: Option<AppendMenu>,
    /// BPMN XML pasted from the OS clipboard, waiting for the import confirmation.
    paste_bpmn: Option<String>,
    /// Space tool mode (S): canvas drags insert or remove space.
    pub(super) space_tool: bool,
    /// Choreography scopes of the current document, rebuilt only when the generation or
    /// document changes.
    scope_cache: Option<(u64, processcraft_core::ids::DocumentId, Arc<Vec<ScopeInfo>>)>,
    /// Attribute overlay (M5.4): view state only, never saved, never a mutation.
    pub(super) overlay: Option<processcraft_core::overlay::OverlaySpec>,
    /// Badge instead of heatmap mode for the next overlay choice.
    pub(super) overlay_badges: bool,
    /// Image exports include the active overlay and legend.
    pub(super) export_overlay: bool,
    overlay_cache: Option<OverlayCache>,
}
type OverlayCache = (
    String,
    u64,
    processcraft_core::ids::DocumentId,
    processcraft_core::overlay::OverlaySpec,
    Arc<processcraft_core::overlay::Overlay>,
);
impl CanvasState {
    /// The active overlay for this session, generation and document, recomputed only when
    /// one of them or the spec changes. A spec whose definition was deleted is dropped.
    pub(super) fn overlay_for(
        &mut self,
        session: &str,
        generation: u64,
        project: &processcraft_core::model::ProjectData,
        document: processcraft_core::ids::DocumentId,
    ) -> Option<Arc<processcraft_core::overlay::Overlay>> {
        let spec = self.overlay?;
        if !project
            .attribute_definitions
            .iter()
            .any(|d| d.id == spec.definition)
        {
            self.overlay = None;
            self.export_overlay = false;
            return None;
        }
        if let Some((s, g, d, sp, overlay)) = &self.overlay_cache
            && s == session
            && *g == generation
            && *d == document
            && *sp == spec
        {
            return Some(overlay.clone());
        }
        let overlay = Arc::new(processcraft_core::overlay::attribute_overlay(
            project, document, &spec,
        ));
        self.overlay_cache = Some((
            session.to_owned(),
            generation,
            document,
            spec,
            overlay.clone(),
        ));
        Some(overlay)
    }
}
/// Owned summary of a choreography scope (see `queries::choreography_scopes`).
pub(super) struct ScopeInfo {
    pub(super) owner: ElementSelector,
    pub(super) root: ElementSelector,
    pub(super) parent: Option<ElementSelector>,
    /// Breadcrumb label: the root or sub-choreography name.
    pub(super) label: String,
    /// The root choreography has a flow-element graph (not a legacy choreography).
    pub(super) graph: bool,
}
fn scope_infos(document: &processcraft_core::model::BpmnDocument) -> Vec<ScopeInfo> {
    let scopes = processcraft_core::queries::choreography_scopes(document);
    scopes
        .iter()
        .map(|scope| ScopeInfo {
            owner: scope.owner.clone(),
            root: scope.root.id.selector(),
            parent: scope.parent.clone(),
            label: if scope.parent.is_none() {
                scope
                    .root
                    .name
                    .clone()
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "Unnamed choreography".into())
            } else {
                scopes
                    .iter()
                    .flat_map(|parent| parent.activities.iter())
                    .find(|activity| activity.id == scope.owner)
                    .and_then(|activity| activity.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "Unnamed sub-choreography".into())
            },
            graph: scope.root.graph.is_some(),
        })
        .collect()
}
/// Replace popup for one element: filter text and whether the field still needs focus.
struct ReplaceMenu {
    element: ElementSelector,
    query: String,
    focus: bool,
}

/// Consume a Shift+`digit` press, matched by logical or physical key so layouts where Shift
/// turns the digit into a symbol still work.
fn take_shift_digit(i: &mut egui::InputState, digit: egui::Key) -> bool {
    let before = i.events.len();
    i.events.retain(|e| {
        !matches!(e, egui::Event::Key { key, physical_key, pressed: true, modifiers, .. }
            if modifiers.shift_only() && (*key == digit || *physical_key == Some(digit)))
    });
    i.events.len() != before
}
const PAD_ENTRY: f32 = 22.0;

/// Context pad entries next to a single selected shape (bpmn-js style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadAction {
    Append(NodeKind),
    Connect,
    Replace,
    Delete,
    /// Lane and pool entries (bpmn-js): add a lane, split into lanes.
    AddLane(LaneLocation),
    Split(u8),
    /// Sequence-flow pad: turn the flow into a plain, default, or conditional flow.
    Flow(FlowVariant),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlowVariant {
    Plain,
    Default,
    Conditional,
}
impl FlowVariant {
    fn target(self) -> processcraft_core::replace::ReplaceTarget {
        use processcraft_core::replace::ReplaceTarget;
        match self {
            Self::Plain => ReplaceTarget::SequenceFlow,
            Self::Default => ReplaceTarget::DefaultFlow,
            Self::Conditional => ReplaceTarget::ConditionalFlow,
        }
    }
}
/// Single-key element shortcuts, shared by the palette (add) and the context pad (append).
pub(super) const ELEMENT_KEYS: [(egui::Key, NodeKind); 9] = [
    (egui::Key::Num1, NodeKind::StartEvent),
    (egui::Key::Num2, NodeKind::IntermediateThrowEvent),
    (egui::Key::Num3, NodeKind::EndEvent),
    (egui::Key::X, NodeKind::ExclusiveGateway),
    (egui::Key::P, NodeKind::ParallelGateway),
    (egui::Key::T, NodeKind::Task),
    (egui::Key::U, NodeKind::UserTask),
    (egui::Key::B, NodeKind::SubProcess),
    (egui::Key::N, NodeKind::TextAnnotation),
];
/// Kinds the context pad appends; their keys append to a single selected shape.
const APPEND_KINDS: [NodeKind; 5] = [
    NodeKind::Task,
    NodeKind::ExclusiveGateway,
    NodeKind::IntermediateThrowEvent,
    NodeKind::EndEvent,
    NodeKind::TextAnnotation,
];
/// The shortcut shown in a palette tooltip, e.g. "Task (T)".
pub(super) fn element_key(kind: NodeKind) -> Option<&'static str> {
    ELEMENT_KEYS
        .iter()
        .find(|(_, k)| *k == kind)
        .map(|(key, _)| key.symbol_or_name())
}
impl PadAction {
    fn label(self) -> &'static str {
        match self {
            Self::Flow(FlowVariant::Plain) => "Change to sequence flow",
            Self::Flow(FlowVariant::Default) => "Change to default flow",
            Self::Flow(FlowVariant::Conditional) => "Change to conditional flow",
            Self::AddLane(LaneLocation::Above) => "Add lane above",
            Self::AddLane(LaneLocation::Below) => "Add lane below",
            Self::Split(2) => "Divide into two lanes",
            Self::Split(_) => "Divide into three lanes",
            Self::Append(NodeKind::Task) => "Append task (T)",
            Self::Append(NodeKind::ExclusiveGateway) => "Append exclusive gateway (X)",
            Self::Append(NodeKind::IntermediateThrowEvent) => "Append intermediate event (2)",
            Self::Append(NodeKind::EndEvent) => "Append end event (3)",
            Self::Append(_) => "Append text annotation (N)",
            Self::Connect => "Connect (C, then click or drag to the target)",
            Self::Replace => "Change type (R)",
            Self::Delete => "Delete (Del)",
        }
    }
}

/// Pad entries in a three-column grid right of the shape's top edge (within `area`, the
/// canvas): appends only from process nodes that may start the connection, connect for anything
/// the rules can connect.
fn context_pad(
    document: &processcraft_core::model::BpmnDocument,
    element: &ElementSelector,
    screen: Rect,
    area: Rect,
) -> Vec<(PadAction, Rect)> {
    use processcraft_core::{ids::ElementIdentity, modeling, rules};
    let located = rules::locate(document, element);
    let mut actions = Vec::new();
    if let Some(node) = located.as_ref().and_then(|l| l.node) {
        for kind in [
            NodeKind::Task,
            NodeKind::ExclusiveGateway,
            NodeKind::IntermediateThrowEvent,
            NodeKind::EndEvent,
        ] {
            let probe = modeling::new_node(kind, ElementIdentity::anonymous(), None, None);
            if rules::sequence_refusal(node, &probe).is_none() {
                actions.push(PadAction::Append(kind));
            }
        }
        if node.kind != NodeKind::TextAnnotation {
            actions.push(PadAction::Append(NodeKind::TextAnnotation));
        }
    }
    // Horizontal lanes and pools with a process get the lane entries.
    let horizontal = document
        .diagrams
        .iter()
        .flat_map(|d| &d.shapes)
        .find(|s| &s.element == element)
        .is_some_and(|s| s.is_horizontal != Some(false));
    let lane = processcraft_core::queries::scopes(document)
        .iter()
        .any(|s| s.lanes.iter().any(|l| &l.id == element));
    if horizontal
        && (lane
            || located
                .as_ref()
                .and_then(|l| l.participant)
                .is_some_and(|p| p.process.is_some()))
    {
        actions.extend([
            PadAction::AddLane(LaneLocation::Above),
            PadAction::AddLane(LaneLocation::Below),
            PadAction::Split(2),
            PadAction::Split(3),
        ]);
    }
    if located.is_some() {
        actions.push(PadAction::Connect);
    }
    if !processcraft_core::replace::replace_options(document, element).is_empty() {
        actions.push(PadAction::Replace);
    }
    actions.push(PadAction::Delete);
    // Kept inside the canvas, so pools and lanes wider than the view still show their pad.
    pad_grid(
        actions,
        Pos2::new(
            (screen.right() + 6.0).min(area.right() - 3.0 * PAD_ENTRY),
            screen.top().max(area.top()),
        ),
    )
}
/// Pad of a selected sequence flow (bpmn-js): the plain/default/conditional variants the replace
/// rules offer, and delete; placed beside the flow's middle waypoint.
fn flow_pad(
    document: &processcraft_core::model::BpmnDocument,
    flow: &ElementSelector,
    anchor: Pos2,
    area: Rect,
) -> Vec<(PadAction, Rect)> {
    use processcraft_core::replace::{ReplaceTarget, replace_options};
    let mut actions: Vec<PadAction> = replace_options(document, flow)
        .iter()
        .filter_map(|target| match target {
            ReplaceTarget::SequenceFlow => Some(PadAction::Flow(FlowVariant::Plain)),
            ReplaceTarget::DefaultFlow => Some(PadAction::Flow(FlowVariant::Default)),
            ReplaceTarget::ConditionalFlow => Some(PadAction::Flow(FlowVariant::Conditional)),
            ReplaceTarget::Node { .. } => None,
        })
        .collect();
    actions.push(PadAction::Delete);
    pad_grid(
        actions,
        Pos2::new(
            (anchor.x + 12.0).min(area.right() - 3.0 * PAD_ENTRY),
            (anchor.y + 12.0).clamp(area.top(), (area.bottom() - PAD_ENTRY).max(area.top())),
        ),
    )
}
/// Lay pad entries out in a three-column grid from `origin`.
fn pad_grid(actions: Vec<PadAction>, origin: Pos2) -> Vec<(PadAction, Rect)> {
    let (left, top) = (origin.x, origin.y);
    actions
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let min = Pos2::new(
                left + (i % 3) as f32 * PAD_ENTRY,
                top + (i / 3) as f32 * PAD_ENTRY,
            );
            (
                action,
                Rect::from_min_size(min, Vec2::splat(PAD_ENTRY - 2.0)),
            )
        })
        .collect()
}

impl ProcessCraft {
    /// Choreography scopes of `document`, cached per (generation, document).
    pub(super) fn choreography_scope_infos(
        &mut self,
        document: &processcraft_core::model::BpmnDocument,
        generation: u64,
    ) -> Arc<Vec<ScopeInfo>> {
        match &self.canvas.scope_cache {
            Some((g, id, infos)) if *g == generation && *id == document.id => infos.clone(),
            _ => {
                let infos = Arc::new(scope_infos(document));
                self.canvas.scope_cache = Some((generation, document.id, infos.clone()));
                infos
            }
        }
    }
    /// Create a palette element dropped at `center` through the core create behaviors (scope and
    /// lane under the point, attach to an activity border, insert into a sequence flow).
    /// E: inline editing of the selected element's label or annotation text.
    fn start_label_edit(&mut self, element: ElementSelector) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        match label_edit(d, diagram, &element, None) {
            Ok(edit) => {
                self.drag = None;
                self.canvas.label_edit = Some(edit);
            }
            Err(Some(reason)) => self.ctl.error = reason.into(),
            Err(None) => {}
        }
    }
    pub(super) fn drop_new(&mut self, entry: PaletteEntry, center: Point) -> bool {
        self.create_at(entry, center, true, None)
    }
    /// Create `entry` at `center` (`CreateAt`), with the picked trigger, optionally splitting
    /// the flow under it, and optionally connected from `source`; one batch, one undo step.
    fn create_at(
        &mut self,
        entry: PaletteEntry,
        center: Point,
        split: bool,
        source: Option<ElementSelector>,
    ) -> bool {
        let kind = entry.kind;
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return false;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return false;
        };
        let node = self.new_identity(entry.trigger.is_some() || source.is_some());
        let shape = crate::controller::id("Shape");
        let name = (kind != NodeKind::TextAnnotation).then(|| super::node_kind_label(kind).into());
        let (flow, edge) = if split {
            (
                Some(crate::controller::id("Flow")),
                Some(crate::controller::id("Edge")),
            )
        } else {
            (None, None)
        };
        let mut mutations = vec![Mutation::CreateAt {
            document_id: d.id,
            diagram_id: diagram.id.selector(),
            kind,
            name: name.clone(),
            center: center.clone(),
            node_id: node.clone(),
            shape_id: shape.clone().into(),
            flow_id: flow.clone().map(Into::into),
            edge_id: edge.clone().map(Into::into),
        }];
        if entry.trigger.is_some() {
            // The core builder's node, to give it the picked trigger in the same batch.
            let preview = processcraft_core::manipulate::create_at(
                &mut (**d).clone(),
                &diagram.id.selector(),
                kind,
                name,
                center,
                processcraft_core::manipulate::CreateIds {
                    node: node.clone(),
                    shape: shape.into(),
                    flow: flow.map(Into::into),
                    edge: edge.map(Into::into),
                },
            );
            match preview {
                Ok(batch) => {
                    mutations.extend(trigger_mutation(&batch, &node.selector(), entry.trigger))
                }
                Err(reason) => {
                    self.ctl.error = reason;
                    return false;
                }
            }
        }
        if let Some(source) = source {
            mutations.push(Mutation::Connect {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                source_id: source,
                target_id: node.selector(),
                flow_id: crate::controller::id("Flow").into(),
                edge_id: crate::controller::id("Edge").into(),
            });
        }
        let done = self.ctl.mutate(mutations);
        if done {
            self.select(Some(node.selector()));
        }
        done
    }
    /// Visible model-space center of the editor canvas.
    pub(super) fn viewport_center(&self) -> (f64, f64) {
        let size = if self.canvas.size == Vec2::ZERO {
            Vec2::new(800.0, 600.0)
        } else {
            self.canvas.size
        };
        (
            self.camera.0 + size.x as f64 / self.zoom as f64 / 2.0,
            self.camera.1 + size.y as f64 / self.zoom as f64 / 2.0,
        )
    }
    pub(super) fn zoom_canvas(&mut self, new_zoom: f32, anchor: Option<Vec2>) {
        let anchor = anchor.unwrap_or(self.canvas.size / 2.0);
        (self.camera, self.zoom) = zoom_at(self.camera, self.zoom, new_zoom, anchor);
    }
    /// Editor keyboard shortcuts; skipped while a text field has focus.
    pub(super) fn canvas_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        // Ctrl+Plus/Minus/0 zoom the diagram, not the whole UI (View menu keeps UI scale).
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        if self.view != View::Model
            || ctx.egui_wants_keyboard_input()
            || self.canvas.label_edit.is_some()
        {
            return;
        }
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let zoom = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL, Key::Plus)
                || i.consume_key(Modifiers::CTRL, Key::Equals)
            {
                Some(self.zoom * ZOOM_STEP)
            } else if i.consume_key(Modifiers::CTRL, Key::Minus) {
                Some(self.zoom / ZOOM_STEP)
            } else if i.consume_key(Modifiers::CTRL, Key::Num0) {
                Some(1.0)
            } else {
                None
            }
        });
        if let Some(zoom) = zoom {
            self.zoom_canvas(zoom, None);
        }
        if ctx.input_mut(|i| {
            i.consume_key(ctrl_shift, Key::Z) || i.consume_key(Modifiers::CTRL, Key::Y)
        }) {
            self.history(processcraft_core::commands::Request::Redo);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            self.history(processcraft_core::commands::Request::Undo);
        }
        // Shift+1 fits the diagram, Shift+2 zooms to the selection (bpmn-js). Taken before the
        // element keys, so Shift+1 does not add a start event.
        if ctx.input_mut(|i| take_shift_digit(i, Key::Num1)) {
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::unique("processcraft-fit-clicked"), true)
            });
        }
        if ctx.input_mut(|i| take_shift_digit(i, Key::Num2)) {
            self.zoom_to_selection();
        }
        let editable = self
            .ctl
            .session
            .snapshot()
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        if editable
            && !self.multi.is_empty()
            && ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace)
            })
        {
            self.delete();
        }
        self.edit_shortcuts(ctx, editable);
        if editable && !self.multi.is_empty() && self.drag.is_none() {
            let nudge = ctx.input_mut(|i| {
                let mut step = None;
                for (key, x, y) in [
                    (Key::ArrowLeft, -1.0, 0.0),
                    (Key::ArrowRight, 1.0, 0.0),
                    (Key::ArrowUp, 0.0, -1.0),
                    (Key::ArrowDown, 0.0, 1.0),
                ] {
                    // bpmn-js keyboard move: 1 px, Shift 10 px.
                    if i.consume_key(Modifiers::SHIFT, key) {
                        step = Some((x * 10.0, y * 10.0));
                    } else if i.consume_key(Modifiers::NONE, key) {
                        step = Some((x, y));
                    }
                }
                step
            });
            if let Some((dx, dy)) = nudge {
                self.move_selected(dx, dy);
            }
        }
        if self.drag.is_none()
            && self.canvas.gesture.is_none()
            && self.canvas.connect_click.is_none()
            && self.canvas.append_menu.is_none()
            && !self.multi.is_empty()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.select(None);
        }
    }
    /// Scroll a selection made outside the canvas (outline, problems) into view.
    fn reveal_selection(&mut self, diagram: &processcraft_core::model::Diagram, size: Vec2) {
        if self.canvas.revealed == self.selected {
            return;
        }
        self.canvas.revealed = self.selected.clone();
        let Some(selected) = &self.selected else {
            return;
        };
        let target = diagram
            .shapes
            .iter()
            .find(|s| &s.element == selected)
            .map(|s| s.bounds.clone())
            .or_else(|| {
                let edge = diagram.edges.iter().find(|e| &e.element == selected)?;
                let xs = edge.waypoints.iter().map(|p| p.x);
                let ys = edge.waypoints.iter().map(|p| p.y);
                let (x0, x1) = (
                    xs.clone().fold(f64::MAX, f64::min),
                    xs.fold(f64::MIN, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::MAX, f64::min),
                    ys.fold(f64::MIN, f64::max),
                );
                (x0 <= x1).then_some(Bounds {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                })
            });
        let Some(b) = target else {
            return;
        };
        let (w, h) = (
            size.x as f64 / self.zoom as f64,
            size.y as f64 / self.zoom as f64,
        );
        let visible = b.x < self.camera.0 + w
            && b.x + b.width > self.camera.0
            && b.y < self.camera.1 + h
            && b.y + b.height > self.camera.1;
        if !visible {
            self.camera = (
                b.x + b.width / 2.0 - w / 2.0,
                b.y + b.height / 2.0 - h / 2.0,
            );
        }
    }
    /// Show `extents` in a canvas of `size`, beside the palette capsule rather than under it.
    fn fit_view(&mut self, extents: Option<(f64, f64, f64, f64)>, size: Vec2) {
        let inset = super::overlays::PALETTE_INSET;
        let (camera, zoom) = fit_extents(extents, Vec2::new((size.x - inset).max(1.0), size.y));
        (self.camera, self.zoom) = ((camera.0 - (inset / zoom) as f64, camera.1), zoom);
    }
    /// Shift+2: zoom to the selected shapes and edges of the current diagram.
    fn zoom_to_selection(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(diagram) = snap
            .project
            .documents
            .get(self.doc)
            .and_then(|d| self.current_diagram(d))
        else {
            return;
        };
        let mut extents: Option<(f64, f64, f64, f64)> = None;
        let mut add = |x0: f64, y0: f64, x1: f64, y1: f64| {
            extents = Some(extents.map_or((x0, y0, x1, y1), |e| {
                (e.0.min(x0), e.1.min(y0), e.2.max(x1), e.3.max(y1))
            }));
        };
        for b in diagram
            .shapes
            .iter()
            .filter(|s| self.multi.contains(&s.element))
            .map(|s| &s.bounds)
        {
            add(b.x, b.y, b.x + b.width, b.y + b.height);
        }
        for p in diagram
            .edges
            .iter()
            .filter(|e| self.multi.contains(&e.element))
            .flat_map(|e| &e.waypoints)
        {
            add(p.x, p.y, p.x, p.y);
        }
        if extents.is_some() {
            self.fit_view(extents, self.canvas.size);
        }
    }
    pub(super) fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let preview = self.layout_preview_document();
        let display = preview.as_ref().or_else(|| self.current_document(&snap));
        let diagram = display.and_then(|d| {
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        });
        let mut fit_clicked = false;
        if preview.is_some() {
            ui.label("Auto-fit preview");
        } else {
            if let (Some(document), Some(diagram)) = (self.current_document(&snap), diagram) {
                let scopes = self.choreography_scope_infos(document, snap.generation);
                if let Some(scope) = scopes.iter().find(|scope| scope.owner == diagram.plane) {
                    let mut path = Vec::new();
                    let mut cursor = Some(scope.owner.clone());
                    while let Some(owner) = cursor.take() {
                        let Some(current) = scopes.iter().find(|candidate| {
                            candidate.owner == owner && candidate.root == scope.root
                        }) else {
                            break;
                        };
                        path.push((owner, current.label.clone()));
                        cursor = current.parent.clone();
                        if path.len() > scopes.len() {
                            break;
                        }
                    }
                    path.reverse();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Scope:");
                        for (index, (owner, label)) in path.iter().enumerate() {
                            if index > 0 {
                                ui.label("›");
                            }
                            ui.push_id(owner, |ui| {
                                self.scope_diagram_ui(ui, document, owner, label, snap.generation)
                            });
                        }
                    });
                }
            }
        }
        // Fit is clicked in the zoom capsule, drawn after the canvas.
        let fit_id_clicked = egui::Id::unique("processcraft-fit-clicked");
        fit_clicked |= ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(fit_id_clicked))
            .unwrap_or(false);
        let size = Vec2::new(
            ui.available_width().max(1.0),
            ui.available_height().max(1.0),
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("test_canvas_size"), size));
        let fit_id = egui::Id::unique("processcraft-editor-fit");
        let document_key = self.current_document(&snap).and_then(|document| {
            diagram
                .map(|diagram| format!("{}:{}:{}", self.ctl.session.id(), document.id, diagram.id))
        });
        let previous = ui.ctx().data(|data| data.get_temp::<String>(fit_id));
        if preview.is_none()
            && let (Some(diagram), Some(key)) = (diagram, document_key)
            && (fit_clicked || previous.as_ref() != Some(&key))
        {
            self.fit_view(diagram_extents(diagram), size);
            ui.ctx().data_mut(|data| data.insert_temp(fit_id, key));
        }
        if preview.is_none()
            && let Some(diagram) = diagram
        {
            self.reveal_selection(diagram, size);
        }
        let frame = egui::Frame::new()
            .fill(self.theme.palette().canvas)
            .show(ui, |ui| self.canvas_render(ui, size, true));
        if preview.is_none() && self.canvas_overlays(ui.ctx(), frame.response.rect) {
            ui.ctx()
                .data_mut(|data| data.insert_temp(fit_id_clicked, true));
            ui.ctx().request_repaint();
        }
        self.auto_scroll(ui.ctx());
        self.search_ui(ui.ctx());
        self.replace_ui(ui.ctx());
        self.append_menu_ui(ui.ctx());
        self.paste_bpmn_ui(ui.ctx());
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("test_canvas_rect"), frame.response.rect)
        });
        #[cfg(not(test))]
        let _ = frame;
    }
    pub(super) fn diagram_preview(&mut self, ui: &mut egui::Ui, height: f32) {
        self.canvas_render(
            ui,
            Vec2::new(ui.available_width().max(1.0), height.max(1.0)),
            false,
        );
    }
    fn hit_at(
        &self,
        x: f64,
        y: f64,
        zoom: f32,
        labels_first: bool,
    ) -> Option<processcraft_render::HitRegion> {
        let tolerance = 5.0 / zoom as f64;
        if labels_first {
            self.hits
                .iter()
                .rev()
                .find(|h| h.is_label && h.contains(x, y, tolerance))
                .or_else(|| self.hits.iter().rev().find(|h| h.contains(x, y, tolerance)))
        } else {
            self.hits.iter().rev().find(|h| h.contains(x, y, tolerance))
        }
        .cloned()
    }
    /// Run a context pad entry for the selected `element`.
    fn pad_action(&mut self, element: ElementSelector, action: PadAction) {
        match action {
            PadAction::Append(kind) => {
                self.append(element, kind);
            }
            PadAction::Connect => {}
            PadAction::Flow(variant) => {
                let document_id = self.ctl.session.snapshot().project.documents[self.doc].id;
                self.ctl.mutate(vec![Mutation::ReplaceElement {
                    document_id,
                    element_id: element,
                    target: variant.target(),
                }]);
            }
            PadAction::Replace => self.open_replace(element),
            PadAction::Delete => self.delete(),
            PadAction::AddLane(location) => {
                self.lane_command(|document_id, diagram_id| Mutation::AddLane {
                    document_id,
                    diagram_id,
                    element_id: element,
                    location,
                })
            }
            PadAction::Split(count) => {
                self.lane_command(|document_id, diagram_id| Mutation::SplitLane {
                    document_id,
                    diagram_id,
                    element_id: element,
                    count,
                })
            }
        }
    }
    /// Run a lane command on the current diagram.
    fn lane_command(
        &mut self,
        build: impl FnOnce(processcraft_core::DocumentId, ElementSelector) -> Mutation,
    ) {
        let snap = self.ctl.session.snapshot();
        let Some(document) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(document) else {
            return;
        };
        let mutation = build(document.id, diagram.id.selector());
        self.ctl.mutate(vec![mutation]);
    }
    /// Connection preview from `bounds` to the pointer: the target outlined green when the rules
    /// allow the connection, red with the reason as hover text otherwise.
    #[allow(clippy::too_many_arguments)]
    fn paint_connect(
        &self,
        painter: &egui::Painter,
        response: &egui::Response,
        d: &processcraft_core::model::BpmnDocument,
        diagram: &processcraft_core::model::Diagram,
        element: &ElementSelector,
        bounds: &Bounds,
        pos: Pos2,
        rect: Rect,
        camera: (f64, f64),
        zoom: f32,
        accent: Color32,
    ) {
        let from = to_screen(rect, camera, zoom, bounds).center();
        let (x, y) = to_model(rect, camera, zoom, pos);
        let mut color = accent;
        let mut route = None;
        if let Some(target) = self
            .connect_target_at(x, y, zoom, element)
            .and_then(|h| diagram.shapes.iter().find(|s| s.id == h.diagram_element_id))
        {
            let verdict = processcraft_core::rules::can_connect(d, element, &target.element);
            color = if verdict.is_ok() {
                CONNECT_OK
            } else {
                CONNECT_REFUSED
            };
            painter.rect_stroke(
                to_screen(rect, camera, zoom, &target.bounds).expand(2.0),
                2.0,
                egui::Stroke::new(2.0, color),
                egui::StrokeKind::Outside,
            );
            match verdict {
                // Preview the route the connection will get.
                Ok(_) => {
                    route = processcraft_core::modeling::route_between(
                        d,
                        diagram,
                        element,
                        &target.element,
                    )
                    .ok()
                }
                Err(reason) => {
                    response.clone().on_hover_text_at_pointer(reason);
                }
            }
        }
        if let Some(points) = route {
            let screen = points
                .iter()
                .map(|p| {
                    Pos2::new(
                        rect.left() + (p.x - camera.0) as f32 * zoom,
                        rect.top() + (p.y - camera.1) as f32 * zoom,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(screen, egui::Stroke::new(2.0, color)));
        } else {
            painter.extend(egui::Shape::dashed_line(
                &[from, pos],
                egui::Stroke::new(2.0, color),
                6.0,
                4.0,
            ));
        }
        response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    /// End of a connect gesture at `pos`: connect to the shape there, or, on empty canvas,
    /// offer the append popup (bpmn-js) when `source` can start a sequence flow.
    fn finish_connect(
        &mut self,
        source: ElementSelector,
        pos: Pos2,
        rect: Rect,
        camera: (f64, f64),
        zoom: f32,
    ) {
        let (x, y) = to_model(rect, camera, zoom, pos);
        if let Some(target) = self.connect_target_at(x, y, zoom, &source) {
            self.connect_elements(source, target.key.xml_id);
        } else if rect.contains(pos) && !self.append_choices(&source).is_empty() {
            self.canvas.append_menu = Some(AppendMenu {
                source,
                at: Point { x, y },
                screen: pos,
            });
        }
    }
    /// Append popup entries the rules allow after `source`.
    fn append_choices(&self, source: &ElementSelector) -> Vec<(&'static str, PaletteEntry)> {
        use processcraft_core::{ids::ElementIdentity, modeling, rules};
        let snap = self.ctl.session.snapshot();
        let Some(node) = snap
            .project
            .documents
            .get(self.doc)
            .and_then(|d| rules::locate(d, source))
            .and_then(|l| l.node)
        else {
            return Vec::new();
        };
        APPEND_ENTRIES
            .iter()
            .filter(|(_, kind, _)| {
                let probe = modeling::new_node(*kind, ElementIdentity::anonymous(), None, None);
                rules::sequence_refusal(node, &probe).is_none()
            })
            .map(|(label, kind, trigger)| {
                (
                    *label,
                    PaletteEntry {
                        kind: *kind,
                        trigger: *trigger,
                    },
                )
            })
            .collect()
    }
    /// Create `entry` at `at` connected from `source` (append popup), one undo step.
    fn create_connected(
        &mut self,
        source: ElementSelector,
        entry: PaletteEntry,
        at: Point,
    ) -> bool {
        if !self.commit_draft() {
            return false;
        }
        self.create_at(entry, at, false, Some(source))
    }
    /// Append popup at the end of a connection dropped on empty canvas; Escape cancels.
    fn append_menu_ui(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.canvas.append_menu.take() else {
            return;
        };
        let choices = self.append_choices(&menu.source);
        let palette = self.theme.palette();
        let mut chosen = None;
        let mut close = choices.is_empty() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Area::new(egui::Id::unique("processcraft-append-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.screen + Vec2::splat(4.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(240.0);
                    ui.weak("Append and connect");
                    for (label, entry) in &choices {
                        let (row, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 22.0),
                            Sense::click(),
                        );
                        if response.hovered() {
                            ui.painter().rect_filled(row, 3.0, palette.accent_soft);
                        }
                        paint_replace_icon(
                            ui.painter(),
                            &processcraft_core::replace::ReplaceTarget::Node {
                                kind: entry.kind,
                                trigger: entry.trigger,
                                non_interrupting: false,
                                event_sub_process: false,
                                collapsed: false,
                            },
                            Rect::from_center_size(
                                row.left_center() + Vec2::new(13.0, 0.0),
                                Vec2::splat(18.0),
                            ),
                            palette.text,
                        );
                        ui.painter().text(
                            row.left_center() + Vec2::new(28.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            *label,
                            egui::FontId::proportional(13.0),
                            palette.text,
                        );
                        if response.clicked() {
                            chosen = Some(*entry);
                            close = true;
                        }
                    }
                });
            });
        if let Some(entry) = chosen {
            self.create_connected(menu.source.clone(), entry, menu.at.clone());
        }
        if !close {
            self.canvas.append_menu = Some(menu);
        }
    }
    /// Topmost shape (never a label region) under the pointer other than `exclude`.
    fn connect_target_at(
        &self,
        x: f64,
        y: f64,
        zoom: f32,
        exclude: &ElementSelector,
    ) -> Option<processcraft_render::HitRegion> {
        let tolerance = 5.0 / zoom as f64;
        self.hits
            .iter()
            .rev()
            .find(|h| {
                !h.is_label
                    && !matches!(h.geometry, HitGeometry::Edge(_))
                    && &h.key.xml_id != exclude
                    && h.contains(x, y, tolerance)
            })
            .cloned()
    }
}

const ALIGNMENTS: [(Alignment, &str); 6] = [
    (Alignment::Left, "Align left"),
    (Alignment::Center, "Align center"),
    (Alignment::Right, "Align right"),
    (Alignment::Top, "Align top"),
    (Alignment::Middle, "Align middle"),
    (Alignment::Bottom, "Align bottom"),
];

/// Selection productivity: select all, clipboard, align/distribute, search, auto-scroll.
impl ProcessCraft {
    fn current_diagram<'a>(
        &self,
        document: &'a processcraft_core::model::BpmnDocument,
    ) -> Option<&'a processcraft_core::model::Diagram> {
        document
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
    }
    /// Select several elements of the current diagram (with their representations).
    fn select_many(
        &mut self,
        document_id: processcraft_core::ids::DocumentId,
        diagram_id: ElementSelector,
        elements: Vec<ElementSelector>,
        representations: Vec<ElementSelector>,
    ) {
        self.select(elements.first().cloned());
        if self.selected != elements.first().cloned() {
            return;
        }
        self.ctl.session.set_selection(
            elements
                .iter()
                .cloned()
                .map(|xml_id| processcraft_core::ids::ElementKey {
                    document_id,
                    xml_id,
                })
                .collect(),
        );
        self.multi = elements;
        self.representation_selection = representations;
        self.representation_diagram = Some(diagram_id);
        self.canvas.revealed = self.selected.clone();
    }
    /// Select every shape and connection of the current diagram (Ctrl+A).
    pub(super) fn select_all(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let mut elements: Vec<ElementSelector> = Vec::new();
        for element in diagram
            .shapes
            .iter()
            .map(|s| &s.element)
            .chain(diagram.edges.iter().map(|e| &e.element))
        {
            if !elements.contains(element) {
                elements.push(element.clone());
            }
        }
        let representations = diagram
            .shapes
            .iter()
            .map(|s| s.id.selector())
            .chain(diagram.edges.iter().map(|e| e.id.selector()))
            .collect();
        if !elements.is_empty() {
            self.select_many(d.id, diagram.id.selector(), elements, representations);
        }
    }
    fn copy_selection(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<processcraft_core::clipboard::Clipboard> {
        let snap = self.ctl.session.snapshot();
        let d = self.current_document(&snap)?;
        let diagram = self.current_diagram(d)?;
        match processcraft_core::clipboard::copy(d, &diagram.id.selector(), &self.multi) {
            Ok(clipboard) => {
                // The OS clipboard gets a readable text summary; pasting uses the in-process copy.
                let text: Vec<String> = clipboard
                    .nodes
                    .iter()
                    .map(|(_, n)| n.name.clone().unwrap_or_else(|| n.id.to_string()))
                    .collect();
                ctx.copy_text(text.join("\n"));
                Some(clipboard)
            }
            Err(e) => {
                self.ctl.error = format!("Could not copy the selection: {e}.");
                None
            }
        }
    }
    /// Paste centered on the model point `at`, or offset by 20/20 from the copied position.
    fn paste_clipboard(
        &mut self,
        clipboard: &processcraft_core::clipboard::Clipboard,
        at: Option<(f64, f64)>,
    ) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let (dx, dy) = match (at, clipboard.bounds()) {
            (Some((x, y)), Some(b)) => (
                (x - b.x - b.width / 2.0).round(),
                (y - b.y - b.height / 2.0).round(),
            ),
            _ => (20.0, 20.0),
        };
        let Some(fallback) = self.process_id.clone().or_else(|| {
            processcraft_core::queries::scopes(d)
                .first()
                .map(|s| s.id.selector())
        }) else {
            self.ctl.error = "Create a process before pasting.".into();
            return;
        };
        let diagram_id = diagram.id.selector();
        match processcraft_core::clipboard::paste(d, &diagram_id, &fallback, clipboard, dx, dy) {
            Ok(pasted) => {
                if self.ctl.mutate(pasted.mutations) {
                    let snap = self.ctl.session.snapshot();
                    let Some(d) = self.current_document(&snap) else {
                        return;
                    };
                    let representations = self
                        .current_diagram(d)
                        .map(|diagram| {
                            diagram
                                .shapes
                                .iter()
                                .filter(|s| pasted.selection.contains(&s.element))
                                .map(|s| s.id.selector())
                                .chain(
                                    diagram
                                        .edges
                                        .iter()
                                        .filter(|e| pasted.selection.contains(&e.element))
                                        .map(|e| e.id.selector()),
                                )
                                .collect()
                        })
                        .unwrap_or_default();
                    self.select_many(d.id, diagram_id, pasted.selection, representations);
                }
            }
            Err(e) => self.ctl.error = format!("Could not paste: {e}."),
        }
    }
    fn cut_selection(&mut self, ctx: &egui::Context) {
        if let Some(clipboard) = self.copy_selection(ctx) {
            self.canvas.clipboard = Some(clipboard);
            self.delete();
        }
    }
    fn copy_to_clipboard(&mut self, ctx: &egui::Context) {
        if let Some(clipboard) = self.copy_selection(ctx) {
            self.canvas.clipboard = Some(clipboard);
        }
    }
    /// Paste at the pointer when it is over the canvas, otherwise at the viewport center.
    fn paste_here(&mut self, ctx: &egui::Context) {
        let Some(clipboard) = self.canvas.clipboard.clone() else {
            self.ctl.error = "Nothing to paste; copy diagram elements first.".into();
            return;
        };
        let at = match (self.canvas.rect, ctx.input(|i| i.pointer.hover_pos())) {
            (Some(rect), Some(pos)) if rect.contains(pos) => {
                to_model(rect, self.camera, self.zoom, pos)
            }
            _ => self.viewport_center(),
        };
        self.paste_clipboard(&clipboard, Some(at));
    }
    /// Duplicate the selection next to itself without touching the clipboard (Ctrl+D).
    fn duplicate_selection(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        match processcraft_core::clipboard::copy(d, &diagram.id.selector(), &self.multi) {
            Ok(clipboard) => self.paste_clipboard(&clipboard, None),
            Err(e) => self.ctl.error = format!("Could not duplicate the selection: {e}."),
        }
    }
    pub(super) fn open_replace(&mut self, element: ElementSelector) {
        self.canvas.replace = Some(ReplaceMenu {
            element,
            query: String::new(),
            focus: true,
        });
    }
    /// Replace popup next to the element: filter field and the core replace options with small
    /// icons; Enter or a click applies, Escape or a selection change closes.
    fn replace_ui(&mut self, ctx: &egui::Context) {
        use processcraft_core::replace::{ReplaceTarget, replace_options};
        let (Some(rect), Some(mut menu)) = (self.canvas.rect, self.canvas.replace.take()) else {
            return;
        };
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let options = replace_options(d, &menu.element);
        if options.is_empty() || self.selected.as_ref() != Some(&menu.element) {
            return;
        }
        let anchor = self
            .current_diagram(d)
            .and_then(|diagram| {
                if let Some(shape) = diagram.shapes.iter().find(|s| s.element == menu.element) {
                    let screen = to_screen(rect, self.camera, self.zoom, &shape.bounds);
                    return Some(screen.right_top() + Vec2::new(8.0, 0.0));
                }
                let edge = diagram.edges.iter().find(|e| e.element == menu.element)?;
                let p = edge.waypoints.get(edge.waypoints.len() / 2)?;
                let point = Bounds {
                    x: p.x,
                    y: p.y,
                    width: 0.0,
                    height: 0.0,
                };
                Some(to_screen(rect, self.camera, self.zoom, &point).min + Vec2::splat(8.0))
            })
            .unwrap_or(rect.left_top() + Vec2::splat(8.0));
        let needle = menu.query.to_lowercase();
        let shown: Vec<ReplaceTarget> = options
            .into_iter()
            .filter(|t| t.label().to_lowercase().contains(&needle))
            .collect();
        let palette = self.theme.palette();
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(egui::Id::unique("processcraft-replace-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(250.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut menu.query)
                            .hint_text("Change type")
                            .desired_width(f32::INFINITY),
                    );
                    if menu.focus {
                        field.request_focus();
                        menu.focus = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    } else if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = shown.first().cloned();
                        close = true;
                    }
                    if shown.is_empty() {
                        ui.weak("No matching type");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for target in &shown {
                                let (row, response) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 22.0),
                                    Sense::click(),
                                );
                                if response.hovered() {
                                    ui.painter().rect_filled(row, 3.0, palette.accent_soft);
                                }
                                paint_replace_icon(
                                    ui.painter(),
                                    target,
                                    Rect::from_center_size(
                                        row.left_center() + Vec2::new(13.0, 0.0),
                                        Vec2::splat(18.0),
                                    ),
                                    palette.text,
                                );
                                ui.painter().text(
                                    row.left_center() + Vec2::new(28.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    target.label(),
                                    egui::FontId::proportional(13.0),
                                    palette.text,
                                );
                                if response.clicked() {
                                    chosen = Some(target.clone());
                                    close = true;
                                }
                            }
                        });
                });
            });
        let element = menu.element.clone();
        if !close {
            self.canvas.replace = Some(menu);
        }
        if let Some(target) = chosen {
            self.ctl.mutate(vec![Mutation::ReplaceElement {
                document_id: d.id,
                element_id: element,
                target,
            }]);
        }
    }
    /// Confirmation for BPMN XML pasted from the OS clipboard: import it as a new model through
    /// the file import path (same parser limits and preserved-source rules).
    fn paste_bpmn_ui(&mut self, ctx: &egui::Context) {
        let Some(xml) = self.canvas.paste_bpmn.take() else {
            return;
        };
        let (mut import, mut cancel) = (false, ctx.input(|i| i.key_pressed(egui::Key::Escape)));
        egui::Window::new("Paste BPMN")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("The clipboard holds BPMN XML. Import it as a new model?");
                ui.horizontal(|ui| {
                    import = ui.button("Import as new model").clicked();
                    cancel |= ui.button("Cancel").clicked();
                });
            });
        if import {
            self.import_bpmn_text(ctx, xml);
        } else if !cancel {
            self.canvas.paste_bpmn = Some(xml);
        }
    }
    fn import_bpmn_text(&mut self, ctx: &egui::Context, xml: String) {
        if self.effects.busy() {
            self.ctl.error = "Finish the current operation before importing.".into();
            return;
        }
        if self.commit_draft() && self.commit_dictionary_draft() {
            self.effects.import_bytes(
                ctx.clone(),
                xml.into_bytes(),
                "Pasted BPMN".into(),
                self.ctl.session.id(),
                self.ctl.session.snapshot().generation,
            );
        }
    }
    fn open_search(&mut self) {
        self.canvas.search = Some(String::new());
        self.canvas.search_focus = true;
    }
    /// Ctrl+A/C/X/V/D/F; the caller has already ruled out focused text fields.
    fn edit_shortcuts(&mut self, ctx: &egui::Context, editable: bool) {
        use egui::{Key, Modifiers};
        // egui-winit turns Ctrl+C/X/V into clipboard events; plain key events cover the rest.
        let mut pasted = None;
        let (copy, cut, paste) = ctx.input_mut(|i| {
            let mut found = (false, false, false);
            i.events.retain(|event| match event {
                egui::Event::Copy => {
                    found.0 = true;
                    false
                }
                egui::Event::Cut => {
                    found.1 = true;
                    false
                }
                egui::Event::Paste(text) => {
                    found.2 = true;
                    pasted = Some(text.clone());
                    false
                }
                _ => true,
            });
            (
                found.0 | i.consume_key(Modifiers::CTRL, Key::C),
                found.1 | i.consume_key(Modifiers::CTRL, Key::X),
                found.2 | i.consume_key(Modifiers::CTRL, Key::V),
            )
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.select_all();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            self.open_search();
        }
        let duplicate = ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D));
        // S: space tool (bpmn-js), toggled; Escape also leaves it.
        if editable && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::S)) {
            self.canvas.space_tool = !self.canvas.space_tool;
        }
        // R: replace menu for a single selected element or sequence flow.
        if editable
            && self.multi.len() <= 1
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::R))
            && let Some(element) = self.selected.clone()
        {
            self.open_replace(element);
        }
        // Element keys: with one shape selected they append and connect, like the context pad;
        // otherwise they add the element, like the palette. C starts click-to-connect.
        if editable
            && let Some(kind) = ctx.input_mut(|i| {
                ELEMENT_KEYS
                    .iter()
                    .find(|(key, _)| i.consume_key(Modifiers::NONE, *key))
                    .map(|(_, kind)| *kind)
            })
        {
            match self.selected.clone().filter(|_| self.multi.len() == 1) {
                Some(source) if APPEND_KINDS.contains(&kind) => {
                    self.append(source, kind);
                }
                _ => self.add_entry(kind.into()),
            }
        }
        if editable
            && self.multi.len() == 1
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::C))
            && let Some(element) = self.selected.clone()
            && let Some(bounds) = self
                .ctl
                .session
                .snapshot()
                .project
                .documents
                .get(self.doc)
                .and_then(|d| self.current_diagram(d))
                .and_then(|diagram| diagram.shapes.iter().find(|s| s.element == element))
                .map(|shape| shape.bounds.clone())
        {
            self.canvas.connect_click = Some((element, bounds));
        }
        // E or F2: direct editing of the selected element's label (bpmn-js).
        if editable
            && self.multi.len() <= 1
            && ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::E) || i.consume_key(Modifiers::NONE, Key::F2)
            })
            && let Some(element) = self.selected.clone()
        {
            self.start_label_edit(element);
        }
        if copy && !self.multi.is_empty() {
            self.copy_to_clipboard(ctx);
        }
        if editable && cut && !self.multi.is_empty() {
            self.cut_selection(ctx);
        }
        // BPMN XML copied from another tool is offered as a new model; anything else pastes the
        // in-app clipboard.
        if paste && let Some(xml) = pasted.filter(|text| looks_like_bpmn(text)) {
            self.canvas.paste_bpmn = Some(xml);
        } else if editable && paste {
            self.paste_here(ctx);
        }
        if editable && duplicate && !self.multi.is_empty() {
            self.duplicate_selection();
        }
    }
    fn arrange(
        &mut self,
        prepare: impl FnOnce(
            &processcraft_core::model::BpmnDocument,
            &ElementSelector,
            &[ElementSelector],
        ) -> Result<processcraft_core::geometry::PreparedMove, &'static str>,
    ) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let diagram_id = diagram.id.selector();
        let shapes: Vec<ElementSelector> = if self.representation_diagram.as_ref()
            == Some(&diagram_id)
            && !self.representation_selection.is_empty()
        {
            self.representation_selection.clone()
        } else {
            diagram
                .shapes
                .iter()
                .filter(|s| self.multi.contains(&s.element))
                .map(|s| s.id.selector())
                .collect()
        };
        match prepare(d, &diagram_id, &shapes) {
            Ok(prepared) => {
                if self.ctl.apply(prepared.request) && !prepared.warnings.is_empty() {
                    self.ctl.notify(prepared.warnings.join("; "));
                }
            }
            Err(e) => self.ctl.error = format!("Could not arrange the selection: {e}."),
        }
    }
    /// Align and Distribute submenus (Edit menu and canvas toolbar).
    pub(super) fn arrange_menus(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Align", |ui| {
            for (alignment, label) in ALIGNMENTS {
                if ui.button(label).clicked() {
                    self.arrange(|d, diagram, shapes| {
                        processcraft_core::arrange::prepare_align(d, diagram, shapes, alignment)
                    });
                    ui.close();
                }
            }
        });
        ui.menu_button("Distribute", |ui| {
            for (distribution, label) in [
                (Distribution::Horizontal, "Distribute horizontally"),
                (Distribution::Vertical, "Distribute vertically"),
            ] {
                if ui.button(label).clicked() {
                    self.arrange(|d, diagram, shapes| {
                        processcraft_core::arrange::prepare_distribute(
                            d,
                            diagram,
                            shapes,
                            distribution,
                        )
                    });
                    ui.close();
                }
            }
        });
    }
    /// Resize the shape of `element` to fit its label (see `fitted_bounds`) with an ordinary
    /// `resizeShape`.
    pub(super) fn fit_to_content(&mut self, element: &ElementSelector) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let Some(shape) = diagram.shapes.iter().find(|s| s.element == *element) else {
            return;
        };
        if let Some(bounds) = fitted_bounds(d, diagram, shape)
            && bounds != shape.bounds
        {
            self.ctl.mutate(vec![Mutation::ResizeShape {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                shape_id: shape.id.selector(),
                bounds,
            }]);
        }
    }
    /// The single selected shape's element when "Fit to content" applies to it.
    fn fit_target(&self) -> Option<ElementSelector> {
        let snap = self.ctl.session.snapshot();
        let d = self.current_document(&snap)?;
        let element = self.selected.clone().filter(|_| self.multi.len() == 1)?;
        self.current_diagram(d)?
            .shapes
            .iter()
            .find(|s| s.element == element)
            .filter(|s| d.capability.allows_structural_editing() && fits_content(d, s))
            .map(|_| element)
    }
    /// Clipboard, selection, search, and arrange entries of the Edit menu.
    pub(super) fn edit_menu_items(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let selected = !self.multi.is_empty();
        ui.separator();
        let item = |ui: &mut egui::Ui, enabled: bool, label: &str, shortcut: &str| {
            ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
                .clicked()
        };
        if item(ui, selected, "Cut", "Ctrl+X") {
            self.cut_selection(&ctx);
            ui.close();
        }
        if item(ui, selected, "Copy", "Ctrl+C") {
            self.copy_to_clipboard(&ctx);
            ui.close();
        }
        if item(ui, self.canvas.clipboard.is_some(), "Paste", "Ctrl+V") {
            self.paste_here(&ctx);
            ui.close();
        }
        if item(ui, selected, "Duplicate", "Ctrl+D") {
            self.duplicate_selection();
            ui.close();
        }
        if item(ui, true, "Select all", "Ctrl+A") {
            self.select_all();
            ui.close();
        }
        if item(ui, true, "Find element…", "Ctrl+F") {
            self.open_search();
            ui.close();
        }
        let fit = self.fit_target();
        if item(ui, fit.is_some(), "Fit to content", "Double-click a handle")
            && let Some(element) = fit
        {
            self.fit_to_content(&element);
            ui.close();
        }
        ui.label(
            egui::RichText::new("Move selection: arrows 1 px, Shift+arrows 10 px")
                .small()
                .weak(),
        );
        ui.separator();
        self.arrange_menus(ui);
    }
    /// Scroll the canvas while a drag nears its border, keeping the dragged content under the
    /// pointer.
    fn auto_scroll(&mut self, ctx: &egui::Context) {
        let active = self.drag.is_some()
            || matches!(
                self.canvas.gesture,
                Some(
                    Gesture::BoxSelect(_)
                        | Gesture::Connect { .. }
                        | Gesture::Resize(_)
                        | Gesture::Bendpoint { .. }
                        | Gesture::Space(_)
                )
            );
        let (Some(rect), Some(pos)) = (self.canvas.rect, ctx.input(|i| i.pointer.latest_pos()))
        else {
            return;
        };
        if !active || !ctx.input(|i| i.pointer.primary_down()) {
            return;
        }
        let step = auto_scroll_step(rect, pos);
        if step == Vec2::ZERO {
            return;
        }
        self.camera.0 += (step.x / self.zoom) as f64;
        self.camera.1 += (step.y / self.zoom) as f64;
        if let Some(drag) = &mut self.drag {
            drag.origin -= step;
        }
        if let Some(Gesture::BoxSelect(start)) = &mut self.canvas.gesture {
            *start -= step;
        }
        ctx.request_repaint();
    }
    /// Ctrl+F overlay: filter elements of the current diagram by name or ID; Enter or a click
    /// selects the element and scrolls it into the center.
    fn search_ui(&mut self, ctx: &egui::Context) {
        let (Some(rect), Some(mut query)) = (self.canvas.rect, self.canvas.search.take()) else {
            return;
        };
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let needle = query.to_lowercase();
        let mut results: Vec<(ElementSelector, String, Bounds)> = Vec::new();
        let edge_bounds = |points: &[processcraft_core::model::Point]| {
            let x0 = points.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let y0 = points.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let x1 = points.iter().map(|p| p.x).fold(f64::MIN, f64::max);
            let y1 = points.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            Bounds {
                x: x0,
                y: y0,
                width: (x1 - x0).max(0.0),
                height: (y1 - y0).max(0.0),
            }
        };
        let candidates = diagram
            .shapes
            .iter()
            .map(|s| (&s.element, s.bounds.clone()))
            .chain(
                diagram
                    .edges
                    .iter()
                    .filter(|e| !e.waypoints.is_empty())
                    .map(|e| (&e.element, edge_bounds(&e.waypoints))),
            );
        for (element, bounds) in candidates {
            if results.len() >= 12 || results.iter().any(|(id, _, _)| id == element) {
                continue;
            }
            let name = processcraft_core::queries::element_name(d, element)
                .unwrap_or_default()
                .to_owned();
            let id = element.to_string();
            if needle.is_empty()
                || name.to_lowercase().contains(&needle)
                || id.to_lowercase().contains(&needle)
            {
                let label = if name.is_empty() {
                    id
                } else {
                    format!("{name}  ({id})")
                };
                results.push((element.clone(), label, bounds));
            }
        }
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(egui::Id::unique("processcraft-canvas-search"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_top() + Vec2::new(8.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(320.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .hint_text("Find element by name or ID")
                            .desired_width(f32::INFINITY),
                    );
                    if self.canvas.search_focus {
                        field.request_focus();
                        self.canvas.search_focus = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    } else if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = results.first().map(|(id, _, b)| (id.clone(), b.clone()));
                        close = chosen.is_some();
                    }
                    if results.is_empty() {
                        ui.weak("No matching elements");
                    }
                    for (id, label, bounds) in &results {
                        if ui.selectable_label(false, label).clicked() {
                            chosen = Some((id.clone(), bounds.clone()));
                            close = true;
                        }
                    }
                });
            });
        if !close {
            self.canvas.search = Some(query);
        }
        if let Some((element, b)) = chosen {
            self.select(Some(element.clone()));
            if self.selected.as_ref() == Some(&element) {
                let (w, h) = (
                    self.canvas.size.x as f64 / self.zoom as f64,
                    self.canvas.size.y as f64 / self.zoom as f64,
                );
                self.camera = (
                    b.x + b.width / 2.0 - w / 2.0,
                    b.y + b.height / 2.0 - h / 2.0,
                );
                self.canvas.revealed = self.selected.clone();
            }
        }
    }
}
