use super::{ProcessCraft, advance_drag, begin_drag, diagram_extents, finish_drag};
use crate::theme::Theme;
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

const MIN_ZOOM: f32 = 0.125;
const MAX_ZOOM: f32 = 8.0;
pub(super) const ZOOM_STEP: f32 = 1.25;

/// Camera of a rendered texture, so an older frame can be shown scaled until the new one arrives.
#[derive(Clone, Copy)]
pub(super) struct TextureView {
    camera: (f64, f64),
    zoom: f32,
    size: Vec2,
}

enum Gesture {
    Pan,
    BoxSelect(Pos2),
    Connect {
        element: ElementSelector,
        bounds: Bounds,
    },
    Resize {
        shape: ElementSelector,
        bounds: Bounds,
        /// Handle direction per axis (-1 west/north, 0 none, 1 east/south).
        handle: (i8, i8),
        min: (f64, f64),
    },
    /// Space tool drag from this screen point.
    Space(Pos2),
    /// Dragging a waypoint or segment of the selected edge (`edge` is its DI id).
    Bendpoint {
        edge: ElementSelector,
        flow: ElementSelector,
        points: Vec<Point>,
        part: EdgePart,
    },
}

/// A handle on a selected edge: a waypoint or the middle of a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdgePart {
    Point(usize),
    Segment(usize),
}
fn edge_handles(points: &[Pos2]) -> Vec<(EdgePart, Pos2)> {
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
/// The route after dragging `part` by `(dx, dy)` (bpmn-js bendpoint move and segment move): a
/// waypoint moves; an orthogonal segment moves across its axis; a diagonal segment gets a new
/// bendpoint at its middle.
fn edit_route(points: &[Point], part: EdgePart, dx: f64, dy: f64) -> Vec<Point> {
    let mut out = points.to_vec();
    match part {
        EdgePart::Point(i) => {
            if let Some(p) = out.get_mut(i) {
                p.x += dx;
                p.y += dy;
            }
        }
        EdgePart::Segment(i) if i + 1 < out.len() => {
            let (a, b) = (out[i].clone(), out[i + 1].clone());
            if a.y == b.y {
                out[i].y += dy;
                out[i + 1].y += dy;
            } else if a.x == b.x {
                out[i].x += dx;
                out[i + 1].x += dx;
            } else {
                out.insert(
                    i + 1,
                    Point {
                        x: (a.x + b.x) / 2.0 + dx,
                        y: (a.y + b.y) / 2.0 + dy,
                    },
                );
            }
        }
        EdgePart::Segment(_) => {}
    }
    out
}
/// Snap correction for a drag (bpmn-js): the first dragged shape snaps to its siblings (shapes
/// of elements in the same scope; for pools, lanes, and artifacts every shape that does not
/// contain it) and its mid to the mids of the elements it is connected to, then the optional
/// 10 px grid on free axes.
fn snap_drag(
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    items: &[(ElementSelector, Bounds)],
    offset: (f64, f64),
    tolerance: f64,
    grid: bool,
) -> processcraft_core::geometry::Snap {
    use processcraft_core::{bpmn_activity::ModelReference, queries, rules};
    let Some((first_id, first)) = items.first() else {
        return Default::default();
    };
    let moved = Bounds {
        x: first.x + offset.0,
        y: first.y + offset.1,
        ..first.clone()
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
    let mut snap = processcraft_core::geometry::snap(&moved, &siblings, &connected, tolerance);
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
/// Highlight where `node` would go if dropped with its centre at `center`: the border of the
/// activity it would attach to, else the sequence flow it would be inserted into.
fn drop_feedback(
    painter: &egui::Painter,
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    node: &processcraft_core::model::Node,
    center: &Point,
    rect_of: impl Fn(&Bounds) -> Rect,
    pos_of: impl Fn(&Point) -> Pos2,
) {
    use processcraft_core::manipulate;
    let stroke = egui::Stroke::new(3.0, CONNECT_OK);
    if let Some(host) = manipulate::attach_host(d, diagram, node, center)
        && let Some(shape) = diagram.shapes.iter().find(|s| s.element == host.id)
    {
        painter.rect_stroke(
            rect_of(&shape.bounds).expand(2.0),
            4.0,
            stroke,
            egui::StrokeKind::Outside,
        );
        return;
    }
    let scope = processcraft_core::rules::locate(d, &node.id.selector())
        .and_then(|l| l.scope)
        .or_else(|| manipulate::container_at(d, diagram, center, None).scope);
    if let Some(scope) = scope
        && let Some(flow) = manipulate::flow_under(d, diagram, node, &scope, center)
        && let Some(edge) = diagram.edges.iter().find(|e| e.element == flow.id)
    {
        painter.add(egui::Shape::line(
            edge.waypoints.iter().map(pos_of).collect(),
            stroke,
        ));
    }
}
/// Resize handles of a screen rectangle: four corners, or east and west only.
fn resize_handles(r: Rect, east_west_only: bool) -> Vec<((i8, i8), Pos2)> {
    let dirs: &[(i8, i8)] = if east_west_only {
        &[(1, 0), (-1, 0)]
    } else {
        &[(-1, -1), (1, -1), (1, 1), (-1, 1)]
    };
    dirs.iter()
        .map(|&(x, y)| {
            (
                (x, y),
                r.center() + Vec2::new(x as f32 * r.width() / 2.0, y as f32 * r.height() / 2.0),
            )
        })
        .collect()
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
    pending: Vec<(String, TextureView)>,
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

fn fit(diagram: &processcraft_core::model::Diagram, size: Vec2) -> ((f64, f64), f32) {
    if let Some((minx, miny, maxx, maxy)) = diagram_extents(diagram) {
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

/// Change zoom while keeping the model point under `anchor` (offset from the canvas origin) fixed.
fn zoom_at(camera: (f64, f64), zoom: f32, new_zoom: f32, anchor: Vec2) -> ((f64, f64), f32) {
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

fn to_model(rect: Rect, camera: (f64, f64), zoom: f32, pos: Pos2) -> (f64, f64) {
    (
        camera.0 + (pos.x - rect.left()) as f64 / zoom as f64,
        camera.1 + (pos.y - rect.top()) as f64 / zoom as f64,
    )
}

fn to_screen(rect: Rect, camera: (f64, f64), zoom: f32, b: &Bounds) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            rect.left() + ((b.x - camera.0) as f32 * zoom),
            rect.top() + ((b.y - camera.1) as f32 * zoom),
        ),
        Vec2::new(b.width as f32 * zoom, b.height as f32 * zoom),
    )
}

/// Where a texture rendered for `view` lands under the current camera.
fn texture_rect(rect: Rect, view: TextureView, camera: (f64, f64), zoom: f32) -> Rect {
    Rect::from_min_size(
        rect.min
            + Vec2::new(
                (view.camera.0 - camera.0) as f32 * zoom,
                (view.camera.1 - camera.1) as f32 * zoom,
            ),
        view.size * (zoom / view.zoom),
    )
}

/// Bounds after dragging the resize handle `(hx, hy)` by `(dx, dy)`, never below `min`.
fn resize_bounds(b: &Bounds, (hx, hy): (i8, i8), dx: f64, dy: f64, min: (f64, f64)) -> Bounds {
    let (mut left, mut top, mut right, mut bottom) = (b.x, b.y, b.x + b.width, b.y + b.height);
    match hx {
        -1 => left = (left + dx).min(right - min.0),
        1 => right = (right + dx).max(left + min.0),
        _ => {}
    }
    match hy {
        -1 => top = (top + dy).min(bottom - min.1),
        1 => bottom = (bottom + dy).max(top + min.1),
        _ => {}
    }
    Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// Connect-gesture target colors: allowed and refused.
const CONNECT_OK: Color32 = Color32::from_rgb(34, 139, 74);
const CONNECT_REFUSED: Color32 = Color32::from_rgb(200, 50, 50);
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
            Self::Append(NodeKind::Task) => "Append task",
            Self::Append(NodeKind::ExclusiveGateway) => "Append exclusive gateway",
            Self::Append(NodeKind::IntermediateThrowEvent) => "Append intermediate event",
            Self::Append(NodeKind::EndEvent) => "Append end event",
            Self::Append(_) => "Append text annotation",
            Self::Connect => "Connect (click or drag to the target)",
            Self::Replace => "Change type (R)",
            Self::Delete => "Delete",
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

fn paint_pad_icon(painter: &egui::Painter, action: PadAction, r: Rect, color: Color32) {
    let stroke = egui::Stroke::new(1.5, color);
    let c = r.center();
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    match action {
        PadAction::Append(NodeKind::Task) => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::new(14.0, 10.0)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        PadAction::Append(NodeKind::ExclusiveGateway) => {
            painter.add(egui::Shape::closed_line(
                vec![at(0.0, -7.0), at(7.0, 0.0), at(0.0, 7.0), at(-7.0, 0.0)],
                stroke,
            ));
            painter.line_segment([at(-2.5, -2.5), at(2.5, 2.5)], stroke);
            painter.line_segment([at(-2.5, 2.5), at(2.5, -2.5)], stroke);
        }
        PadAction::Append(NodeKind::IntermediateThrowEvent) => {
            painter.circle_stroke(c, 6.5, stroke);
            painter.circle_stroke(c, 4.5, stroke);
        }
        PadAction::Append(NodeKind::EndEvent) => {
            painter.circle_stroke(c, 6.0, egui::Stroke::new(2.5, color));
        }
        PadAction::Append(_) => {
            painter.add(egui::Shape::line(
                vec![at(-2.0, -7.0), at(-6.0, -7.0), at(-6.0, 7.0), at(-2.0, 7.0)],
                stroke,
            ));
            painter.line_segment([at(-2.0, -2.0), at(6.0, -2.0)], stroke);
            painter.line_segment([at(-2.0, 2.0), at(4.0, 2.0)], stroke);
        }
        PadAction::Connect => painter.arrow(at(-6.0, 6.0), Vec2::new(11.0, -11.0), stroke),
        PadAction::Flow(variant) => paint_replace_icon(painter, &variant.target(), r, color),
        PadAction::Replace => {
            // Wrench: a handle and a ring-shaped head.
            painter.line_segment(
                [at(-6.0, 6.0), at(1.0, -1.0)],
                egui::Stroke::new(2.5, color),
            );
            painter.circle_stroke(at(3.5, -3.5), 3.5, stroke);
        }
        PadAction::Delete => {
            painter.rect_stroke(
                Rect::from_min_max(at(-5.0, -3.0), at(5.0, 7.0)),
                1.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([at(-7.0, -5.0), at(7.0, -5.0)], stroke);
            painter.line_segment([at(-2.0, -7.0), at(2.0, -7.0)], stroke);
        }
        // A pool outline with the new lane filled (above/below) or divided into lanes.
        PadAction::AddLane(_) | PadAction::Split(_) => {
            let body = Rect::from_center_size(c, Vec2::new(16.0, 12.0));
            painter.rect_stroke(body, 1.0, stroke, egui::StrokeKind::Middle);
            match action {
                PadAction::AddLane(location) => {
                    let y = if location == LaneLocation::Above {
                        body.top()
                    } else {
                        body.bottom() - 4.0
                    };
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(16.0, 4.0)),
                        0.0,
                        color,
                    );
                }
                PadAction::Split(n) => {
                    for i in 1..n {
                        let y = body.top() + body.height() * i as f32 / n as f32;
                        painter.line_segment(
                            [Pos2::new(body.left() + 4.0, y), Pos2::new(body.right(), y)],
                            stroke,
                        );
                    }
                    painter.line_segment(
                        [
                            Pos2::new(body.left() + 4.0, body.top()),
                            Pos2::new(body.left() + 4.0, body.bottom()),
                        ],
                        stroke,
                    );
                }
                _ => {}
            }
        }
    }
}

/// Small icon of a replace target: event circles with a trigger letter, gateway diamonds with
/// their marker, activity boxes with a type letter or subprocess marker, data shapes, flows.
pub(super) fn paint_replace_icon(
    painter: &egui::Painter,
    target: &processcraft_core::replace::ReplaceTarget,
    r: Rect,
    color: Color32,
) {
    use processcraft_core::{replace::ReplaceTarget, rules};
    let stroke = egui::Stroke::new(1.2, color);
    let c = r.center();
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    let letter = |text: &str, size: f32| {
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(size),
            color,
        );
    };
    let ReplaceTarget::Node {
        kind,
        trigger,
        non_interrupting,
        event_sub_process,
        collapsed,
    } = target
    else {
        painter.arrow(at(-7.0, 5.0), Vec2::new(14.0, -10.0), stroke);
        match target {
            ReplaceTarget::DefaultFlow => {
                painter.line_segment([at(-6.0, -1.0), at(-2.0, 5.0)], stroke);
            }
            ReplaceTarget::ConditionalFlow => {
                painter.add(egui::Shape::closed_line(
                    vec![at(-7.0, 5.0), at(-4.0, 1.0), at(-1.0, 3.0), at(-4.0, 7.0)],
                    stroke,
                ));
            }
            _ => {}
        }
        return;
    };
    if rules::is_event(*kind) {
        if *non_interrupting {
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    at(7.5 * a.cos(), 7.5 * a.sin())
                })
                .collect();
            painter.extend(egui::Shape::dashed_line(&points, stroke, 2.0, 2.0));
        } else {
            let width = if *kind == NodeKind::EndEvent {
                2.4
            } else {
                1.2
            };
            painter.circle_stroke(c, 7.5, egui::Stroke::new(width, color));
        }
        if matches!(
            kind,
            NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ) {
            painter.circle_stroke(c, 5.5, stroke);
        }
        if let Some(t) = trigger {
            use processcraft_core::replace::Trigger as T;
            let mark = match t {
                T::Message => "M",
                T::Timer => "T",
                T::Conditional => "C",
                T::Signal => "S",
                T::Error => "E",
                T::Escalation => "Es",
                T::Compensate => "Cp",
                T::Cancel => "X",
                T::Link => "L",
                T::Terminate => "Te",
            };
            letter(mark, 8.0);
        }
    } else if rules::is_gateway(*kind) {
        painter.add(egui::Shape::closed_line(
            vec![at(0.0, -8.0), at(8.0, 0.0), at(0.0, 8.0), at(-8.0, 0.0)],
            stroke,
        ));
        let cross = || {
            painter.line_segment([at(-2.5, -2.5), at(2.5, 2.5)], stroke);
            painter.line_segment([at(-2.5, 2.5), at(2.5, -2.5)], stroke);
        };
        let plus = || {
            painter.line_segment([at(0.0, -3.5), at(0.0, 3.5)], stroke);
            painter.line_segment([at(-3.5, 0.0), at(3.5, 0.0)], stroke);
        };
        match kind {
            NodeKind::ExclusiveGateway => cross(),
            NodeKind::ParallelGateway => plus(),
            NodeKind::ComplexGateway => {
                cross();
                plus();
            }
            NodeKind::InclusiveGateway => {
                painter.circle_stroke(c, 3.0, stroke);
            }
            _ => {
                painter.circle_stroke(c, 3.5, stroke);
                painter.circle_stroke(c, 2.0, stroke);
            }
        }
    } else if *kind == NodeKind::DataObjectReference {
        painter.add(egui::Shape::closed_line(
            vec![
                at(-5.0, -7.0),
                at(2.0, -7.0),
                at(5.0, -4.0),
                at(5.0, 7.0),
                at(-5.0, 7.0),
            ],
            stroke,
        ));
    } else if *kind == NodeKind::DataStoreReference {
        painter.rect_stroke(
            Rect::from_center_size(c, Vec2::new(14.0, 12.0)),
            3.0,
            stroke,
            egui::StrokeKind::Middle,
        );
        painter.line_segment([at(-7.0, -3.0), at(7.0, -3.0)], stroke);
    } else {
        let expanded = matches!(
            kind,
            NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
        ) && !collapsed;
        let outline = Rect::from_center_size(
            c,
            if expanded {
                Vec2::new(17.0, 13.0)
            } else {
                Vec2::new(15.0, 11.0)
            },
        );
        if *event_sub_process {
            let corners = [
                outline.left_top(),
                outline.right_top(),
                outline.right_bottom(),
                outline.left_bottom(),
                outline.left_top(),
            ];
            painter.extend(egui::Shape::dashed_line(&corners, stroke, 2.0, 1.5));
        } else {
            let width = if *kind == NodeKind::CallActivity {
                2.4
            } else {
                1.2
            };
            painter.rect_stroke(
                outline,
                2.0,
                egui::Stroke::new(width, color),
                egui::StrokeKind::Middle,
            );
        }
        if *kind == NodeKind::Transaction {
            painter.rect_stroke(outline.shrink(2.0), 1.0, stroke, egui::StrokeKind::Middle);
        }
        let marker = match kind {
            NodeKind::UserTask => "U",
            NodeKind::ServiceTask => "S",
            NodeKind::SendTask => "Sd",
            NodeKind::ReceiveTask => "Rc",
            NodeKind::ManualTask => "M",
            NodeKind::BusinessRuleTask => "B",
            NodeKind::ScriptTask => "Sc",
            NodeKind::AdHocSubProcess => "~",
            _ => "",
        };
        if !marker.is_empty() {
            letter(marker, 7.5);
        }
        if *collapsed {
            let plus = Rect::from_center_size(at(0.0, 3.5), Vec2::splat(4.0));
            painter.rect_stroke(plus, 0.0, stroke, egui::StrokeKind::Middle);
        }
    }
}

/// Drag-create ghost: the BPMN shape of `entry` filling `r` (event circle, gateway diamond,
/// rounded activity box, data shapes, annotation bracket) with its marker.
fn paint_ghost(painter: &egui::Painter, entry: PaletteEntry, r: Rect, color: Color32) {
    use processcraft_core::{replace::ReplaceTarget, rules};
    let kind = entry.kind;
    let fill = color.gamma_multiply(0.15);
    let stroke = egui::Stroke::new(2.0, color);
    let c = r.center();
    let marker = |painter: &egui::Painter| {
        // The replace-menu icon carries the type and trigger marker; drawn at the centre.
        let target = ReplaceTarget::Node {
            kind,
            trigger: entry.trigger,
            non_interrupting: false,
            event_sub_process: false,
            collapsed: false,
        };
        paint_replace_icon(
            painter,
            &target,
            Rect::from_center_size(c, Vec2::splat(18.0)),
            color,
        );
    };
    if rules::is_event(kind) {
        let radius = r.width().min(r.height()) / 2.0;
        let width = if kind == NodeKind::EndEvent { 4.0 } else { 2.0 };
        painter.circle(c, radius, fill, egui::Stroke::new(width, color));
        if matches!(
            kind,
            NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ) {
            painter.circle_stroke(c, (radius - 3.0).max(1.0), egui::Stroke::new(1.0, color));
        }
        if entry.trigger.is_some() {
            marker(painter);
        }
    } else if rules::is_gateway(kind) {
        let (w, h) = (r.width() / 2.0, r.height() / 2.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + Vec2::new(0.0, -h),
                c + Vec2::new(w, 0.0),
                c + Vec2::new(0.0, h),
                c + Vec2::new(-w, 0.0),
            ],
            fill,
            stroke,
        ));
        let s = w.min(h) * 0.4;
        let cross = || {
            painter.line_segment([c + Vec2::new(-s, -s), c + Vec2::new(s, s)], stroke);
            painter.line_segment([c + Vec2::new(-s, s), c + Vec2::new(s, -s)], stroke);
        };
        let plus = || {
            painter.line_segment([c + Vec2::new(0.0, -s), c + Vec2::new(0.0, s)], stroke);
            painter.line_segment([c + Vec2::new(-s, 0.0), c + Vec2::new(s, 0.0)], stroke);
        };
        match kind {
            NodeKind::ExclusiveGateway => cross(),
            NodeKind::ParallelGateway => plus(),
            NodeKind::ComplexGateway => {
                cross();
                plus();
            }
            NodeKind::InclusiveGateway => {
                painter.circle_stroke(c, s, stroke);
            }
            _ => {
                painter.circle_stroke(c, s, egui::Stroke::new(1.0, color));
                painter.circle_stroke(c, s * 0.7, egui::Stroke::new(1.0, color));
            }
        }
    } else if kind == NodeKind::TextAnnotation {
        painter.rect_filled(r, 0.0, fill);
        painter.add(egui::Shape::line(
            vec![
                r.left_top() + Vec2::new(12.0, 0.0),
                r.left_top(),
                r.left_bottom(),
                r.left_bottom() + Vec2::new(12.0, 0.0),
            ],
            stroke,
        ));
    } else if matches!(
        kind,
        NodeKind::DataObjectReference | NodeKind::DataStoreReference
    ) {
        painter.rect(r, 2.0, fill, stroke, egui::StrokeKind::Inside);
        marker(painter);
    } else {
        painter.rect(r, 10.0, fill, stroke, egui::StrokeKind::Inside);
        marker(painter);
    }
}

/// Nearest free spot around `center`; shapes that fully contain a candidate (pools, lanes,
/// expanded subprocesses) are containers, not obstacles.
pub(super) fn vacant_bounds(
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        if self.view != 0 || ctx.egui_wants_keyboard_input() || self.canvas.label_edit.is_some() {
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
    pub(super) fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let preview = self.layout_preview_document();
        let display = preview
            .as_ref()
            .or_else(|| snap.project.documents.get(self.doc));
        let diagram = display.and_then(|d| {
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        });
        let mut fit_clicked = false;
        if preview.is_some() {
            ui.label("Auto-fit preview");
        } else {
            if let (Some(document), Some(diagram)) = (snap.project.documents.get(self.doc), diagram)
            {
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
        let document_key = snap.project.documents.get(self.doc).and_then(|document| {
            diagram
                .map(|diagram| format!("{}:{}:{}", self.ctl.session.id(), document.id, diagram.id))
        });
        let previous = ui.ctx().data(|data| data.get_temp::<String>(fit_id));
        if preview.is_none()
            && let (Some(diagram), Some(key)) = (diagram, document_key)
            && (fit_clicked || previous.as_ref() != Some(&key))
        {
            // Fit beside the palette capsule, not under it.
            let inset = super::overlays::PALETTE_INSET;
            let (camera, zoom) = fit(diagram, Vec2::new((size.x - inset).max(1.0), size.y));
            (self.camera, self.zoom) = ((camera.0 - (inset / zoom) as f64, camera.1), zoom);
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
        let Some(document) = snap.project.documents.get(self.doc) else {
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
            if let Err(reason) = verdict {
                response.clone().on_hover_text_at_pointer(reason);
            }
        }
        painter.extend(egui::Shape::dashed_line(
            &[from, pos],
            egui::Stroke::new(2.0, color),
            6.0,
            4.0,
        ));
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
    fn canvas_render(&mut self, ui: &mut egui::Ui, size: Vec2, interactive: bool) {
        let ctx = ui.ctx().clone();
        let snap = self.ctl.session.snapshot();
        let preview = self.layout_preview_document();
        let interactive = interactive && preview.is_none();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        let (mut camera, mut zoom) = if interactive {
            (self.camera, self.zoom)
        } else {
            preview
                .as_ref()
                .or_else(|| snap.project.documents.get(self.doc))
                .and_then(|d| {
                    d.diagrams
                        .iter()
                        .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
                })
                .map_or(((0.0, 0.0), 1.0), |d| fit(d, size))
        };
        if interactive {
            self.canvas.size = size;
        }
        let (rect, response) = ui.allocate_exact_size(
            size,
            if interactive {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        if interactive {
            self.canvas.rect = Some(rect);
        }
        let palette = self.theme.palette();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4, palette.canvas);
        if let Some(d) = preview
            .as_ref()
            .or_else(|| snap.project.documents.get(self.doc))
        {
            if let Some(diagram) = d
                .diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
            {
                let scene = format!(
                    "{}:{}:{}:{:?}:{}",
                    self.ctl.session.id(),
                    if preview.is_some() {
                        "layout-preview"
                    } else if interactive {
                        "editor"
                    } else {
                        "preview"
                    },
                    self.doc,
                    self.diagram_id,
                    if self.theme == Theme::Dark { "d" } else { "l" }
                );
                let key = format!(
                    "{scene}:{}:{}:{}:{}:{}:{}:{}",
                    snap.generation,
                    camera.0,
                    camera.1,
                    zoom,
                    size.x,
                    size.y,
                    ctx.pixels_per_point()
                );
                // Another document, diagram, or theme: the previous frame is not a valid placeholder.
                if self.canvas.scene != scene {
                    self.canvas.scene = scene;
                    self.canvas.pending.clear();
                    self.canvas.view = None;
                    self.texture = None;
                    if interactive {
                        self.hits.clear();
                    }
                }
                if self.render_key != key {
                    let view = Viewport {
                        x: camera.0,
                        y: camera.1,
                        width: size.x as f64 / zoom as f64,
                        height: size.y as f64 / zoom as f64,
                        zoom: (zoom * ctx.pixels_per_point()).clamp(MIN_ZOOM, MAX_ZOOM),
                    };
                    self.renderer.submit(crate::jobs::RenderJob {
                        key: key.clone(),
                        document: d.clone(),
                        diagram: diagram.id.selector(),
                        appearance: match (self.theme == Theme::Dark, interactive) {
                            (false, false) => Appearance::Light,
                            (true, false) => Appearance::Dark,
                            (false, true) => Appearance::CanvasLight,
                            (true, true) => Appearance::CanvasDark,
                        },
                        view,
                    });
                    self.render_failed = false;
                    self.render_key = key.clone();
                    self.canvas
                        .pending
                        .push((key.clone(), TextureView { camera, zoom, size }));
                    if self.canvas.pending.len() > 32 {
                        self.canvas.pending.remove(0);
                    }
                }
                // Results arrive in submission order; any pending one is newer than what is shown.
                while let Some(result) = self.renderer.poll() {
                    let Some(index) = self
                        .canvas
                        .pending
                        .iter()
                        .position(|(pending, _)| *pending == result.key)
                    else {
                        continue;
                    };
                    let view = self.canvas.pending[index].1;
                    self.canvas.pending.drain(..=index);
                    match result.result {
                        Ok((pixels, w, h, hits)) => {
                            let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
                            self.texture =
                                Some(ctx.load_texture("diagram", image, Default::default()));
                            self.canvas.view = Some(view);
                            if interactive {
                                self.hits = hits;
                            }
                        }
                        Err(e) => {
                            if crate::jobs::is_current(&result.key, &key) {
                                self.render_failed = true;
                                self.ctl.error = e;
                            }
                        }
                    }
                }
                if !self.canvas.pending.is_empty() && !self.render_failed {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
                match (&self.texture, self.canvas.view) {
                    (Some(t), Some(view)) => {
                        painter.image(
                            t.id(),
                            texture_rect(rect, view, camera, zoom),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    _ => {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            if self.render_failed {
                                "Diagram unavailable"
                            } else {
                                "Rendering diagram…"
                            },
                            egui::FontId::proportional(13.0),
                            palette.muted,
                        );
                    }
                }
                if diagram.shapes.is_empty()
                    && diagram.edges.is_empty()
                    && d.capability.allows_structural_editing()
                {
                    painter.text(
                        rect.center() - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        "Build your process",
                        egui::FontId::proportional(20.0),
                        palette.text,
                    );
                    painter.text(
                        rect.center() + egui::vec2(0.0, 16.0),
                        egui::Align2::CENTER_CENTER,
                        "Add an element from the BPMN palette to begin.",
                        egui::FontId::proportional(12.0),
                        palette.muted,
                    );
                    if interactive {
                        painter.text(
                            rect.center() + egui::vec2(0.0, 38.0),
                            egui::Align2::CENTER_CENTER,
                            "Drag empty space to pan · Shift-drag to select · Ctrl+wheel to zoom · Double-click or E to rename · Select an element for its context pad",
                            egui::FontId::proportional(11.0),
                            palette.muted,
                        );
                    }
                }
                if interactive {
                    // Canvas selection is blue; amber stays the app's identity colour.
                    let accent = palette.selection;
                    if self.canvas.space_tool {
                        painter.text(
                            rect.left_top() + egui::vec2(12.0, 12.0),
                            egui::Align2::LEFT_TOP,
                            "Space tool: drag to insert or remove space (Ctrl: shapes before the line) · S or Esc to leave",
                            egui::FontId::proportional(12.0),
                            accent,
                        );
                    }
                    let diagram_id = diagram.id.selector();
                    let by_representation = self.representation_diagram.as_ref()
                        == Some(&diagram_id)
                        && !self.representation_selection.is_empty();
                    let selected_shapes: Vec<&Shape> = diagram
                        .shapes
                        .iter()
                        .filter(|s| {
                            if by_representation {
                                self.representation_selection.contains(&s.id.selector())
                            } else {
                                self.multi.contains(&s.element)
                            }
                        })
                        .collect();
                    for shape in &selected_shapes {
                        let outline = to_screen(rect, camera, zoom, &shape.bounds).expand(3.0);
                        painter.rect_filled(outline, 6.0, accent.gamma_multiply(0.08));
                        painter.rect_stroke(
                            outline,
                            6.0,
                            egui::Stroke::new(2.0, accent),
                            egui::StrokeKind::Outside,
                        );
                    }
                    for edge in diagram.edges.iter().filter(|e| {
                        if by_representation {
                            self.representation_selection.contains(&e.id.selector())
                        } else {
                            self.multi.contains(&e.element)
                        }
                    }) {
                        let points = edge
                            .waypoints
                            .iter()
                            .map(|p| {
                                Pos2::new(
                                    rect.left() + (p.x - camera.0) as f32 * zoom,
                                    rect.top() + (p.y - camera.1) as f32 * zoom,
                                )
                            })
                            .collect();
                        painter.add(egui::Shape::line(
                            points,
                            egui::Stroke::new(3.0, accent.gamma_multiply(0.85)),
                        ));
                    }
                    // Handles of a single selected shape: resize corners and the context pad,
                    // hidden while a gesture or label edit is running.
                    let single = (editable && selected_shapes.len() == 1 && self.drag.is_none())
                        .then(|| selected_shapes[0]);
                    // Resize handles only where the rules allow (bpmn-js `canResize`).
                    let rule = single
                        .and_then(|shape| processcraft_core::manipulate::resize_rule(d, shape));
                    // Bendpoint and segment handles of a single selected edge.
                    let edge_selection: Vec<_> = diagram
                        .edges
                        .iter()
                        .filter(|e| {
                            if by_representation {
                                self.representation_selection.contains(&e.id.selector())
                            } else {
                                self.multi.contains(&e.element)
                            }
                        })
                        .collect();
                    let selected_edge = (editable
                        && selected_shapes.is_empty()
                        && edge_selection.len() == 1
                        && self.drag.is_none())
                    .then(|| edge_selection[0]);
                    let edge_screen = |edge: &processcraft_core::model::Edge| -> Vec<Pos2> {
                        edge.waypoints
                            .iter()
                            .map(|p| {
                                Pos2::new(
                                    rect.left() + (p.x - camera.0) as f32 * zoom,
                                    rect.top() + (p.y - camera.1) as f32 * zoom,
                                )
                            })
                            .collect()
                    };
                    let bend_handles = selected_edge
                        .map(|edge| edge_handles(&edge_screen(edge)))
                        .unwrap_or_default();
                    if !matches!(self.canvas.gesture, Some(Gesture::Bendpoint { .. })) {
                        for (part, pos) in &bend_handles {
                            match part {
                                EdgePart::Point(_) => {
                                    painter.circle_filled(*pos, 4.0, palette.base);
                                    painter.circle_stroke(
                                        *pos,
                                        4.0,
                                        egui::Stroke::new(1.5, accent),
                                    );
                                }
                                EdgePart::Segment(_) => {
                                    painter.rect_filled(
                                        Rect::from_center_size(*pos, Vec2::splat(6.0)),
                                        1.0,
                                        accent,
                                    );
                                }
                            }
                        }
                    }
                    let hover = response.hover_pos();
                    let mut pad = Vec::new();
                    if let Some(shape) = single {
                        let screen = to_screen(rect, camera, zoom, &shape.bounds);
                        for (_, pos) in rule
                            .map(|r| resize_handles(screen, r.east_west_only))
                            .unwrap_or_default()
                        {
                            let handle = Rect::from_center_size(pos, Vec2::splat(8.0));
                            painter.rect_filled(handle, 2.0, palette.base);
                            painter.rect_stroke(
                                handle,
                                2.0,
                                egui::Stroke::new(1.5, accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    let pad_idle = self.canvas.gesture.is_none()
                        && self.canvas.label_edit.is_none()
                        && self.canvas.connect_click.is_none();
                    // The element the pad belongs to: the single selected shape or flow.
                    let mut pad_owner = None;
                    if let Some(shape) = single
                        && pad_idle
                    {
                        let screen = to_screen(rect, camera, zoom, &shape.bounds);
                        pad = context_pad(d, &shape.element, screen, rect);
                        pad_owner = Some(shape.element.clone());
                    } else if let Some(edge) = selected_edge
                        && pad_idle
                        && processcraft_core::queries::find_flow(d, &edge.element)
                            .is_some_and(|f| f.kind == FlowKind::Sequence)
                        && let Some(mid) = edge_screen(edge).get(edge.waypoints.len() / 2)
                    {
                        pad = flow_pad(d, &edge.element, *mid, rect);
                        pad_owner = Some(edge.element.clone());
                    }
                    for (action, r) in &pad {
                        {
                            let hovered = hover.is_some_and(|pos| r.contains(pos));
                            painter.rect_filled(
                                *r,
                                crate::theme::RADIUS,
                                if hovered {
                                    palette.selection_soft
                                } else {
                                    palette.base
                                },
                            );
                            painter.rect_stroke(
                                *r,
                                crate::theme::RADIUS,
                                egui::Stroke::new(1.0, palette.border),
                                egui::StrokeKind::Inside,
                            );
                            paint_pad_icon(
                                &painter,
                                *action,
                                *r,
                                if hovered { accent } else { palette.text },
                            );
                            if hovered {
                                if *action == PadAction::Connect {
                                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                                }
                                response.clone().on_hover_text_at_pointer(action.label());
                            }
                        }
                    }
                    let pad_at = |pos: Pos2| {
                        pad.iter()
                            .find(|(_, r)| r.contains(pos))
                            .map(|(action, _)| *action)
                    };
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.canvas.gesture = None;
                        self.canvas.space_tool = false;
                        self.canvas.connect_click = None;
                        self.canvas.append_menu = None;
                    }
                    // Click-to-connect: the next click picks the target (empty canvas offers the
                    // append popup); the preview follows the pointer meanwhile.
                    let mut click_consumed = false;
                    if let Some((source, bounds)) = self.canvas.connect_click.clone() {
                        if response.clicked() || response.drag_started() {
                            click_consumed = true;
                            self.canvas.connect_click = None;
                            if let Some(pos) = response.interact_pointer_pos() {
                                self.finish_connect(source, pos, rect, camera, zoom);
                            }
                        } else if let Some(pos) = hover {
                            self.paint_connect(
                                &painter, &response, d, diagram, &source, &bounds, pos, rect,
                                camera, zoom, accent,
                            );
                        }
                    }
                    let alt = ctx.input(|i| i.modifiers.alt);
                    let shift = ctx.input(|i| i.modifiers.shift);
                    if response.drag_started()
                        && !click_consumed
                        && let Some(pos) = ctx.input(|i| i.pointer.press_origin())
                    {
                        let pan = ctx.input(|i| {
                            i.pointer.middle_down()
                                || (i.key_down(egui::Key::Space)
                                    && !ctx.egui_wants_keyboard_input())
                        });
                        if pan {
                            self.canvas.gesture = Some(Gesture::Pan);
                        } else if self.canvas.space_tool && editable {
                            self.canvas.gesture = Some(Gesture::Space(pos));
                        } else if let Some(edge) = selected_edge
                            && let Some((part, _)) =
                                bend_handles.iter().find(|(_, h)| h.distance(pos) <= 6.0)
                        {
                            self.canvas.gesture = Some(Gesture::Bendpoint {
                                edge: edge.id.selector(),
                                flow: edge.element.clone(),
                                points: edge.waypoints.clone(),
                                part: *part,
                            });
                        } else if let Some(shape) = single {
                            let screen = to_screen(rect, camera, zoom, &shape.bounds);
                            if let Some(rule) = rule
                                && let Some((handle, _)) =
                                    resize_handles(screen, rule.east_west_only)
                                        .into_iter()
                                        .find(|(_, c)| c.distance(pos) <= 6.0)
                            {
                                self.canvas.gesture = Some(Gesture::Resize {
                                    shape: shape.id.selector(),
                                    bounds: shape.bounds.clone(),
                                    handle,
                                    min: (rule.width, rule.height),
                                });
                            } else if pad_at(pos) == Some(PadAction::Connect) {
                                self.canvas.gesture = Some(Gesture::Connect {
                                    element: shape.element.clone(),
                                    bounds: shape.bounds.clone(),
                                });
                            }
                        }
                    }
                    let pad_pressed = if response.drag_started() {
                        ctx.input(|i| i.pointer.press_origin())
                    } else if response.clicked() {
                        response.interact_pointer_pos()
                    } else {
                        None
                    }
                    .and_then(pad_at);
                    if response.clicked()
                        && let (Some(action), Some(owner)) = (pad_pressed, pad_owner)
                    {
                        match (action, single) {
                            // Click-to-connect: wait for the target click.
                            (PadAction::Connect, Some(shape)) => {
                                self.canvas.connect_click =
                                    Some((shape.element.clone(), shape.bounds.clone()));
                            }
                            _ => self.pad_action(owner, action),
                        }
                    }
                    // Any other canvas click or drag closes the append popup.
                    if !click_consumed && (response.clicked() || response.drag_started()) {
                        self.canvas.append_menu = None;
                    }
                    if !click_consumed
                        && pad_pressed.is_none()
                        && ((response.drag_started() && self.canvas.gesture.is_none())
                            || response.clicked())
                        && self.commit_draft()
                        && let Some(pos) = if response.drag_started() {
                            ctx.input(|i| i.pointer.press_origin())
                        } else {
                            response.interact_pointer_pos()
                        }
                    {
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        if let Some(h) = self.hit_at(x, y, zoom, alt) {
                            let id = h.key.xml_id.clone();
                            let representation = h.diagram_element_id.clone();
                            let same_diagram =
                                self.representation_diagram.as_ref() == Some(&diagram_id);
                            if !shift
                                && (!same_diagram
                                    || !self.representation_selection.contains(&representation))
                            {
                                self.select(Some(id.clone()));
                                self.representation_selection = vec![representation.clone()];
                            } else if shift {
                                let mut multi = self.multi.clone();
                                let mut representations = if same_diagram {
                                    self.representation_selection.clone()
                                } else {
                                    Vec::new()
                                };
                                if response.clicked() && multi.contains(&id) {
                                    multi.retain(|value| value != &id);
                                    representations.retain(|value| value != &representation);
                                } else {
                                    if !multi.contains(&id) {
                                        multi.push(id.clone());
                                    }
                                    if !representations.contains(&representation) {
                                        representations.push(representation.clone());
                                    }
                                }
                                if multi.first() != self.selected.as_ref() {
                                    self.select(multi.first().cloned());
                                }
                                self.multi = multi;
                                self.representation_selection = representations;
                                self.ctl.session.set_selection(
                                    self.multi
                                        .iter()
                                        .cloned()
                                        .map(|xml_id| processcraft_core::ids::ElementKey {
                                            document_id: d.id,
                                            xml_id,
                                        })
                                        .collect(),
                                );
                            }
                            self.representation_diagram = Some(diagram_id.clone());
                            self.canvas.revealed = self.selected.clone();
                            if editable && response.drag_started() {
                                let inside_shape = diagram
                                    .shapes
                                    .iter()
                                    .find(|s| s.id == representation)
                                    .is_some_and(|s| {
                                        x >= s.bounds.x
                                            && x <= s.bounds.x + s.bounds.width
                                            && y >= s.bounds.y
                                            && y <= s.bounds.y + s.bounds.height
                                    });
                                let label_drag = h.is_label && (!inside_shape || alt);
                                let items = if label_drag {
                                    diagram
                                        .shapes
                                        .iter()
                                        .find(|s| s.id == representation)
                                        .and_then(|s| s.label.as_ref())
                                        .or_else(|| {
                                            diagram
                                                .edges
                                                .iter()
                                                .find(|e| e.id == representation)
                                                .and_then(|e| e.label.as_ref())
                                        })
                                        .and_then(|l| l.bounds.clone())
                                        .or_else(|| match &h.geometry {
                                            HitGeometry::Shape(bounds) => Some(bounds.clone()),
                                            _ => None,
                                        })
                                        .map(|b| vec![(representation.clone(), b)])
                                        .unwrap_or_default()
                                } else {
                                    diagram
                                        .shapes
                                        .iter()
                                        .filter(|s| {
                                            self.representation_selection.contains(&s.id.selector())
                                        })
                                        .map(|s| (s.id.selector(), s.bounds.clone()))
                                        .collect()
                                };
                                if label_drag && items.is_empty() {
                                    self.ctl.error = "Set label bounds in Diagram representation properties before dragging the label.".into();
                                } else if self.ctl.session.snapshot().generation != snap.generation
                                {
                                    self.ctl.error =
                                        "Properties changed; start the move again.".into();
                                } else if !items.is_empty() {
                                    self.drag = begin_drag(&response, &ctx, items);
                                    if let Some(drag) = &mut self.drag {
                                        drag.context =
                                            Some((snap.generation, d.id, diagram_id.clone()));
                                        drag.label = label_drag;
                                    }
                                }
                            }
                        } else if response.drag_started() {
                            self.canvas.gesture = Some(if shift {
                                Gesture::BoxSelect(pos)
                            } else {
                                Gesture::Pan
                            });
                        } else {
                            self.select(None);
                            self.canvas.revealed = None;
                        }
                    }
                    // Double-click on a bendpoint removes it (bpmn-js).
                    let bend = (response.double_clicked())
                        .then(|| response.interact_pointer_pos())
                        .flatten()
                        .zip(selected_edge)
                        .and_then(|(pos, edge)| {
                            bend_handles.iter().find_map(|(part, h)| match part {
                                EdgePart::Point(i)
                                    if *i > 0
                                        && *i + 1 < edge.waypoints.len()
                                        && h.distance(pos) <= 6.0 =>
                                {
                                    Some((edge, *i))
                                }
                                _ => None,
                            })
                        });
                    if let Some((edge, i)) = bend {
                        let mut waypoints = edge.waypoints.clone();
                        waypoints.remove(i);
                        self.ctl.mutate(vec![Mutation::Reroute {
                            document_id: d.id,
                            diagram_id: diagram_id.clone(),
                            edge_id: edge.id.selector(),
                            waypoints,
                        }]);
                    } else if editable
                        && response.double_clicked()
                        && self.canvas.label_edit.is_none()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        if let Some(h) = self.hit_at(x, y, zoom, alt) {
                            let bounds = match &h.geometry {
                                HitGeometry::Edge(_) => None,
                                HitGeometry::Shape(b)
                                | HitGeometry::Ellipse(b)
                                | HitGeometry::Diamond(b)
                                | HitGeometry::Hexagon(b)
                                | HitGeometry::DataObject(b)
                                | HitGeometry::DataStore(b) => Some(b.clone()),
                            };
                            match label_edit(d, diagram, &h.key.xml_id, bounds.as_ref()) {
                                Ok(edit) => {
                                    self.drag = None;
                                    self.canvas.label_edit = Some(edit);
                                }
                                Err(reason) => {
                                    if let Some(reason) = reason {
                                        self.ctl.error = reason.into();
                                    }
                                }
                            }
                        }
                    }
                    if self
                        .drag
                        .as_ref()
                        .and_then(|drag| drag.context.as_ref())
                        .is_some_and(|(generation, document, diagram_id)| {
                            *generation != self.ctl.session.snapshot().generation
                                || *document != d.id
                                || *diagram_id != diagram.id.selector()
                        })
                    {
                        self.drag = None;
                        self.ctl.error =
                            "The diagram changed while dragging. Start the move again.".into();
                    }
                    advance_drag(&mut self.drag, &ctx);
                    // Snapping (Alt or Ctrl moves freely) and its guide lines.
                    let free = ctx.input(|i| i.modifiers.alt || i.modifiers.ctrl);
                    if response.dragged()
                        && let Some(drag) = &mut self.drag
                        && !drag.label
                    {
                        drag.snap = if free {
                            Default::default()
                        } else {
                            snap_drag(
                                d,
                                diagram,
                                &drag.items,
                                ((drag.offset.x / zoom) as f64, (drag.offset.y / zoom) as f64),
                                7.0 / zoom as f64,
                                self.canvas.grid_snap,
                            )
                        };
                    }
                    if response.dragged()
                        && let Some(drag) = &self.drag
                    {
                        let offset = drag.offset
                            + Vec2::new(drag.snap.dx as f32, drag.snap.dy as f32) * zoom;
                        let guide = egui::Stroke::new(1.0, palette.selection.gamma_multiply(0.6));
                        if let Some(x) = drag.snap.guide_x {
                            let x = rect.left() + (x - camera.0) as f32 * zoom;
                            painter.line_segment(
                                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                                guide,
                            );
                        }
                        if let Some(y) = drag.snap.guide_y {
                            let y = rect.top() + (y - camera.1) as f32 * zoom;
                            painter.line_segment(
                                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                                guide,
                            );
                        }
                        // Attach and drop-on-flow feedback for a single dragged element.
                        if let [(shape_id, bounds)] = drag.items.as_slice()
                            && !drag.label
                            && let Some(node) = diagram
                                .shapes
                                .iter()
                                .find(|s| s.id == *shape_id)
                                .and_then(|s| processcraft_core::queries::find_node(d, &s.element))
                        {
                            let center = Point {
                                x: bounds.x + bounds.width / 2.0 + (offset.x / zoom) as f64,
                                y: bounds.y + bounds.height / 2.0 + (offset.y / zoom) as f64,
                            };
                            drop_feedback(
                                &painter,
                                d,
                                diagram,
                                node,
                                &center,
                                |b| to_screen(rect, camera, zoom, b),
                                |p| {
                                    Pos2::new(
                                        rect.left() + (p.x - camera.0) as f32 * zoom,
                                        rect.top() + (p.y - camera.1) as f32 * zoom,
                                    )
                                },
                            );
                        }
                        for (_, bounds) in &drag.items {
                            let preview = to_screen(rect, camera, zoom, bounds).translate(offset);
                            if let (Some(t), Some(view)) = (&self.texture, self.canvas.view) {
                                let uv = |x: f64, y: f64| {
                                    Pos2::new(
                                        (x - view.camera.0) as f32 * view.zoom / view.size.x,
                                        (y - view.camera.1) as f32 * view.zoom / view.size.y,
                                    )
                                };
                                painter.image(
                                    t.id(),
                                    preview,
                                    Rect::from_min_max(
                                        uv(bounds.x, bounds.y),
                                        uv(bounds.x + bounds.width, bounds.y + bounds.height),
                                    ),
                                    Color32::from_white_alpha(220),
                                );
                            }
                            painter.rect_stroke(
                                preview,
                                0.0,
                                egui::Stroke::new(2.0, accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    if let Some(drag) = finish_drag(&response, &mut self.drag) {
                        let delta = drag.offset / zoom;
                        if delta.length() > 1.0 {
                            if drag.label {
                                if let Some((representation_id, mut bounds)) =
                                    drag.items.into_iter().next()
                                {
                                    bounds.x += delta.x as f64;
                                    bounds.y += delta.y as f64;
                                    let mutation =
                                        if let Some(shape) = diagram.shapes.iter().find(|s| {
                                            s.id == representation_id && s.label.is_none()
                                        }) {
                                            Mutation::SetShapeLabel {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                shape_id: representation_id,
                                                label: Some(processcraft_core::model::Label {
                                                    bounds: Some(bounds),
                                                    ..Default::default()
                                                }),
                                                label_visibility: shape.label_visibility,
                                            }
                                        } else if let Some(edge) = diagram.edges.iter().find(|e| {
                                            e.id == representation_id && e.label.is_none()
                                        }) {
                                            Mutation::SetEdgeLabel {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                edge_id: representation_id,
                                                label: Some(processcraft_core::model::Label {
                                                    bounds: Some(bounds),
                                                    ..Default::default()
                                                }),
                                                label_visibility: edge.label_visibility,
                                            }
                                        } else {
                                            Mutation::SetLabelBounds {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                representation_id,
                                                bounds,
                                            }
                                        };
                                    self.ctl.mutate(vec![mutation]);
                                }
                            } else {
                                self.drag_move(
                                    drag.items,
                                    delta.x as f64 + drag.snap.dx,
                                    delta.y as f64 + drag.snap.dy,
                                );
                            }
                        }
                    }
                    // Canvas gestures: pan, box select, connect, resize.
                    let pointer = ctx.input(|i| i.pointer.latest_pos());
                    let origin = ctx.input(|i| i.pointer.press_origin());
                    if response.dragged() {
                        match &self.canvas.gesture {
                            Some(Gesture::Pan) => {
                                let delta = response.drag_delta();
                                camera.0 -= delta.x as f64 / zoom as f64;
                                camera.1 -= delta.y as f64 / zoom as f64;
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            Some(Gesture::Space(start)) => {
                                // The line where space opens and how far the shapes past it go.
                                if let Some(pos) = pointer {
                                    let d = pos - *start;
                                    let stroke = egui::Stroke::new(1.5, accent);
                                    let (line, moved, cursor) = if d.x.abs() >= d.y.abs() {
                                        (
                                            [
                                                Pos2::new(start.x, rect.top()),
                                                Pos2::new(start.x, rect.bottom()),
                                            ],
                                            Rect::from_x_y_ranges(
                                                start.x.min(pos.x)..=start.x.max(pos.x),
                                                rect.y_range(),
                                            ),
                                            egui::CursorIcon::ResizeHorizontal,
                                        )
                                    } else {
                                        (
                                            [
                                                Pos2::new(rect.left(), start.y),
                                                Pos2::new(rect.right(), start.y),
                                            ],
                                            Rect::from_x_y_ranges(
                                                rect.x_range(),
                                                start.y.min(pos.y)..=start.y.max(pos.y),
                                            ),
                                            egui::CursorIcon::ResizeVertical,
                                        )
                                    };
                                    painter.rect_filled(moved, 0.0, accent.gamma_multiply(0.12));
                                    painter
                                        .extend(egui::Shape::dashed_line(&line, stroke, 6.0, 4.0));
                                    ctx.set_cursor_icon(cursor);
                                }
                            }
                            Some(Gesture::BoxSelect(start)) => {
                                if let Some(pos) = pointer {
                                    let area = Rect::from_two_pos(*start, pos);
                                    painter.rect_filled(area, 0.0, accent.gamma_multiply(0.12));
                                    painter.rect_stroke(
                                        area,
                                        0.0,
                                        egui::Stroke::new(1.0, accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                            Some(Gesture::Connect { element, bounds }) => {
                                if let Some(pos) = pointer {
                                    self.paint_connect(
                                        &painter, &response, d, diagram, element, bounds, pos,
                                        rect, camera, zoom, accent,
                                    );
                                }
                            }
                            Some(Gesture::Bendpoint { points, part, .. }) => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let edited =
                                        edit_route(points, *part, delta.x as f64, delta.y as f64);
                                    let screen: Vec<Pos2> = edited
                                        .iter()
                                        .map(|p| {
                                            Pos2::new(
                                                rect.left() + (p.x - camera.0) as f32 * zoom,
                                                rect.top() + (p.y - camera.1) as f32 * zoom,
                                            )
                                        })
                                        .collect();
                                    painter.add(egui::Shape::line(
                                        screen,
                                        egui::Stroke::new(2.0, accent),
                                    ));
                                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                            }
                            Some(Gesture::Resize {
                                bounds,
                                handle,
                                min,
                                ..
                            }) => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let resized = resize_bounds(
                                        bounds,
                                        *handle,
                                        delta.x as f64,
                                        delta.y as f64,
                                        *min,
                                    );
                                    painter.rect_stroke(
                                        to_screen(rect, camera, zoom, &resized),
                                        0.0,
                                        egui::Stroke::new(2.0, accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                            None => {}
                        }
                    }
                    if response.drag_stopped()
                        && let Some(gesture) = self.canvas.gesture.take()
                    {
                        match gesture {
                            Gesture::Pan => {}
                            Gesture::Space(start) => {
                                if let Some(pos) = pointer {
                                    let (x0, y0) = to_model(rect, camera, zoom, start);
                                    let (x1, y1) = to_model(rect, camera, zoom, pos);
                                    let (axis, from, delta) = if (x1 - x0).abs() >= (y1 - y0).abs()
                                    {
                                        (Axis::X, x0, x1 - x0)
                                    } else {
                                        (Axis::Y, y0, y1 - y0)
                                    };
                                    // Under 5 screen px is a click, not a space drag.
                                    if delta.abs() * zoom as f64 >= 5.0 {
                                        self.ctl.mutate(vec![Mutation::MakeSpace {
                                            document_id: d.id,
                                            diagram_id: diagram_id.clone(),
                                            axis,
                                            start: from.round(),
                                            delta: delta.round(),
                                            before: ctx.input(|i| i.modifiers.command),
                                            at: Some(Point { x: x0, y: y0 }),
                                        }]);
                                    }
                                }
                            }
                            Gesture::BoxSelect(start) => {
                                if let Some(pos) = pointer {
                                    let (x0, y0) = to_model(rect, camera, zoom, start);
                                    let (x1, y1) = to_model(rect, camera, zoom, pos);
                                    let (x0, x1) = (x0.min(x1), x0.max(x1));
                                    let (y0, y1) = (y0.min(y1), y0.max(y1));
                                    let inside: Vec<&Shape> = diagram
                                        .shapes
                                        .iter()
                                        .filter(|s| {
                                            let b = &s.bounds;
                                            b.x >= x0
                                                && b.y >= y0
                                                && b.x + b.width <= x1
                                                && b.y + b.height <= y1
                                        })
                                        .collect();
                                    if let Some(first) = inside.first() {
                                        self.select(Some(first.element.clone()));
                                        let mut multi: Vec<ElementSelector> = Vec::new();
                                        for shape in &inside {
                                            if !multi.contains(&shape.element) {
                                                multi.push(shape.element.clone());
                                            }
                                        }
                                        self.multi = multi;
                                        self.representation_selection =
                                            inside.iter().map(|s| s.id.selector()).collect();
                                        self.representation_diagram = Some(diagram_id.clone());
                                        self.canvas.revealed = self.selected.clone();
                                        self.ctl.session.set_selection(
                                            self.multi
                                                .iter()
                                                .cloned()
                                                .map(|xml_id| processcraft_core::ids::ElementKey {
                                                    document_id: d.id,
                                                    xml_id,
                                                })
                                                .collect(),
                                        );
                                    }
                                }
                            }
                            Gesture::Connect { element, .. } => {
                                if let Some(pos) = pointer {
                                    self.finish_connect(element, pos, rect, camera, zoom);
                                }
                            }
                            Gesture::Resize {
                                shape,
                                bounds,
                                handle,
                                min,
                            } => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let resized = resize_bounds(
                                        &bounds,
                                        handle,
                                        delta.x as f64,
                                        delta.y as f64,
                                        min,
                                    );
                                    if resized != bounds {
                                        self.ctl.mutate(vec![Mutation::ResizeShape {
                                            document_id: d.id,
                                            diagram_id: diagram_id.clone(),
                                            shape_id: shape,
                                            bounds: resized,
                                        }]);
                                    }
                                }
                            }
                            Gesture::Bendpoint {
                                edge,
                                flow,
                                points,
                                part,
                            } => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let last = points.len().saturating_sub(1);
                                    let (x, y) = to_model(rect, camera, zoom, pos);
                                    let ends = processcraft_core::queries::find_flow(d, &flow)
                                        .map(|f| (f.source.clone(), f.target.clone()));
                                    let local = |r: &Option<
                                        processcraft_core::bpmn_activity::ModelReference,
                                    >| match r {
                                        Some(
                                            processcraft_core::bpmn_activity::ModelReference::Local(
                                                id,
                                            ),
                                        ) => Some(id.clone()),
                                        _ => None,
                                    };
                                    match (part, ends) {
                                        // Dragging an end onto another shape reconnects the flow.
                                        (EdgePart::Point(i), Some((source, target)))
                                            if i == 0 || i == last =>
                                        {
                                            if let (Some(source), Some(target)) =
                                                (local(&source), local(&target))
                                                && let Some(hit) = self.connect_target_at(
                                                    x,
                                                    y,
                                                    zoom,
                                                    if i == 0 { &source } else { &target },
                                                )
                                            {
                                                let element = hit.key.xml_id;
                                                let (source_id, target_id) = if i == 0 {
                                                    (element, target)
                                                } else {
                                                    (source, element)
                                                };
                                                self.ctl.mutate(vec![Mutation::Reconnect {
                                                    document_id: d.id,
                                                    diagram_id: diagram_id.clone(),
                                                    flow_id: flow,
                                                    source_id,
                                                    target_id,
                                                }]);
                                            }
                                        }
                                        _ if delta.length() > 1.0 => {
                                            self.ctl.mutate(vec![Mutation::Reroute {
                                                document_id: d.id,
                                                diagram_id: diagram_id.clone(),
                                                edge_id: edge,
                                                waypoints: edit_route(
                                                    &points,
                                                    part,
                                                    delta.x as f64,
                                                    delta.y as f64,
                                                ),
                                            }]);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    // A palette entry dragged over the canvas: ghost, feedback, drop on release.
                    if let Some(entry) = self.canvas.palette_drag {
                        let kind = entry.kind;
                        let released = ctx.input(|i| !i.pointer.primary_down());
                        let over = pointer.filter(|p| rect.contains(*p));
                        if let Some(pos) = over {
                            let (x, y) = to_model(rect, camera, zoom, pos);
                            let (w, h) = processcraft_core::modeling::default_size(kind);
                            let ghost = to_screen(
                                rect,
                                camera,
                                zoom,
                                &Bounds {
                                    x: x - w / 2.0,
                                    y: y - h / 2.0,
                                    width: w,
                                    height: h,
                                },
                            );
                            paint_ghost(&painter, entry, ghost, accent);
                            let probe = processcraft_core::modeling::new_node(
                                kind,
                                processcraft_core::ids::ElementIdentity::anonymous(),
                                None,
                                None,
                            );
                            drop_feedback(
                                &painter,
                                d,
                                diagram,
                                &probe,
                                &Point { x, y },
                                |b| to_screen(rect, camera, zoom, b),
                                |p| {
                                    Pos2::new(
                                        rect.left() + (p.x - camera.0) as f32 * zoom,
                                        rect.top() + (p.y - camera.1) as f32 * zoom,
                                    )
                                },
                            );
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            if released && editable {
                                self.drop_new(entry, Point { x, y });
                            }
                        }
                        if released {
                            self.canvas.palette_drag = None;
                        } else {
                            ctx.request_repaint();
                        }
                    }
                    // Inline editor: a name commits on Enter, an annotation text (multi-line) on
                    // Ctrl+Enter; focus loss commits, Escape cancels.
                    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                    if let Some(edit) = &mut self.canvas.label_edit {
                        let width = (edit.width as f32 * zoom).max(140.0);
                        let center = Pos2::new(
                            rect.left() + (edit.center.0 - camera.0) as f32 * zoom,
                            rect.top() + (edit.center.1 - camera.1) as f32 * zoom,
                        );
                        let id = egui::Id::unique("processcraft-canvas-label-edit");
                        let (field, submit) = if edit.annotation {
                            // Consumed before the field sees it, so no newline is inserted.
                            let submit = ctx.input_mut(|i| {
                                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                            });
                            let height = (edit.height as f32 * zoom).max(60.0);
                            let field = ui.put(
                                Rect::from_center_size(center, Vec2::new(width, height)),
                                egui::TextEdit::multiline(&mut edit.text)
                                    .id(id)
                                    .hint_text("Annotation text · Ctrl+Enter to apply"),
                            );
                            (field, submit)
                        } else {
                            let field = ui.put(
                                Rect::from_center_size(center, Vec2::new(width, 28.0)),
                                egui::TextEdit::singleline(&mut edit.text).id(id),
                            );
                            (field, false)
                        };
                        if !edit.focused {
                            field.request_focus();
                            edit.focused = true;
                        } else if escape {
                            self.canvas.label_edit = None;
                        } else if (field.lost_focus() || submit)
                            && let Some(edit) = self.canvas.label_edit.take()
                            && edit.text != edit.original
                            && self.ctl.mutate(vec![if edit.annotation {
                                Mutation::SetTextAnnotation {
                                    document_id: d.id,
                                    element_id: edit.element.clone(),
                                    text: Some(edit.text.clone()),
                                }
                            } else {
                                Mutation::Rename {
                                    document_id: d.id,
                                    element_id: edit.element.clone(),
                                    name: Some(edit.text.clone()),
                                }
                            }])
                            && self.selected.as_ref() == Some(&edit.element)
                            && !self.draft_dirty
                        {
                            self.draft = edit.text;
                            self.draft_name_present = true;
                        }
                    }
                    if response.hovered() {
                        let (scroll, zoom_delta) =
                            ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
                        if zoom_delta != 1.0 {
                            let anchor = hover.map_or(size / 2.0, |pos| pos - rect.min);
                            (camera, zoom) = zoom_at(camera, zoom, zoom * zoom_delta, anchor);
                        } else if scroll != Vec2::ZERO {
                            camera.0 -= scroll.x as f64 / zoom as f64;
                            camera.1 -= scroll.y as f64 / zoom as f64;
                        }
                    }
                }
            } else {
                ui.label("No diagram in this document.");
            }
        }
        painter.rect_stroke(
            rect,
            4,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        if interactive {
            self.camera = camera;
            self.zoom = zoom;
        }
    }
}

/// Pixels from the canvas border that start auto-scroll while dragging, and the scroll step
/// per frame (diagram-js AutoScroll: 20 and 10).
const AUTO_SCROLL_BAND: f32 = 20.0;
const AUTO_SCROLL_STEP: f32 = 10.0;

/// Screen-space camera motion for a drag at `pos` near the border of `rect`.
fn auto_scroll_step(rect: Rect, pos: Pos2) -> Vec2 {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let d = snap.project.documents.get(self.doc)?;
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
                    let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        // E: direct editing of the selected element's label (bpmn-js).
        if editable
            && self.multi.len() <= 1
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::E))
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
                Some(Gesture::BoxSelect(_) | Gesture::Connect { .. })
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let anchor = Vec2::new(200.0, 100.0);
        let before = (10.0 + 200.0 / 1.0, 20.0 + 100.0 / 1.0);
        let (camera, zoom) = zoom_at((10.0, 20.0), 1.0, 2.0, anchor);
        assert_eq!(zoom, 2.0);
        let after = (camera.0 + 200.0 / 2.0, camera.1 + 100.0 / 2.0);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        assert_eq!(zoom_at((0.0, 0.0), 1.0, 100.0, anchor).1, MAX_ZOOM);
    }

    #[test]
    fn stale_texture_is_scaled_and_offset() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(100.0, 50.0));
        let view = TextureView {
            camera: (0.0, 0.0),
            zoom: 1.0,
            size: Vec2::new(100.0, 50.0),
        };
        assert_eq!(texture_rect(rect, view, (0.0, 0.0), 1.0), rect);
        let panned = texture_rect(rect, view, (5.0, 0.0), 2.0);
        assert_eq!(panned.min, Pos2::new(0.0, 10.0));
        assert_eq!(panned.size(), Vec2::new(200.0, 100.0));
    }

    #[test]
    fn resize_moves_only_the_dragged_corner_and_keeps_a_minimum() {
        let start = b(0.0, 0.0, 100.0, 80.0);
        let min = (20.0, 20.0);
        assert_eq!(
            resize_bounds(&start, (1, 1), 20.0, 10.0, min),
            b(0.0, 0.0, 120.0, 90.0)
        );
        assert_eq!(
            resize_bounds(&start, (-1, -1), 10.0, 10.0, min),
            b(10.0, 10.0, 90.0, 70.0)
        );
        assert_eq!(
            resize_bounds(&start, (1, -1), -500.0, 0.0, min),
            b(0.0, 0.0, 20.0, 80.0)
        );
        assert_eq!(
            resize_bounds(&start, (-1, 1), 0.0, -500.0, min),
            b(0.0, 0.0, 100.0, 20.0)
        );
        // East/west handles leave the height alone.
        assert_eq!(
            resize_bounds(&start, (1, 0), 30.0, 50.0, (100.0, 40.0)),
            b(0.0, 0.0, 130.0, 80.0)
        );
        let handles = resize_handles(Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0)), true);
        assert_eq!(
            handles.iter().map(|(h, _)| *h).collect::<Vec<_>>(),
            [(1, 0), (-1, 0)]
        );
    }

    #[test]
    fn bendpoint_and_segment_drags_edit_the_route() {
        let p = |x: f64, y: f64| Point { x, y };
        let route = vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 40.0), p(100.0, 40.0)];
        // A waypoint moves freely.
        assert_eq!(
            edit_route(&route, EdgePart::Point(1), 5.0, 5.0)[1],
            p(55.0, 5.0)
        );
        // A vertical segment moves sideways only; a horizontal one up or down only.
        let moved = edit_route(&route, EdgePart::Segment(1), 10.0, 99.0);
        assert_eq!((moved[1].x, moved[2].x, moved[1].y), (60.0, 60.0, 0.0));
        let moved = edit_route(&route, EdgePart::Segment(2), 99.0, -10.0);
        assert_eq!((moved[2].y, moved[3].y, moved[3].x), (30.0, 30.0, 100.0));
        // A diagonal segment gets a new bendpoint in its middle.
        let diagonal = vec![p(0.0, 0.0), p(100.0, 100.0)];
        assert_eq!(
            edit_route(&diagonal, EdgePart::Segment(0), 0.0, 10.0),
            vec![p(0.0, 0.0), p(50.0, 60.0), p(100.0, 100.0)]
        );
    }

    #[test]
    fn adding_after_a_selected_node_appends_and_connects_by_id() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let diagram = &document.diagrams[0];
        let bounds = |id: &ElementSelector| {
            diagram
                .shapes
                .iter()
                .find(|s| &s.element == id)
                .unwrap()
                .bounds
                .clone()
        };
        assert!(bounds(&task).x >= bounds(&start).x + bounds(&start).width);
        assert_eq!(diagram.edges.len(), 1);
        app.select(None);
        app.add(NodeKind::EndEvent);
        let end = app.selected.clone().unwrap();
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0]
                .edges
                .len(),
            1
        );
        assert!(app.connect_elements(task, end));
        let snap = app.ctl.session.snapshot();
        let edges = &snap.project.documents[0].diagrams[0].edges;
        assert_eq!(edges.len(), 2);
        assert!(
            edges[1]
                .waypoints
                .windows(2)
                .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
        );
    }

    #[test]
    fn empty_drag_pans_and_ctrl_plus_zooms_the_canvas() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        };
        run(&mut app, vec![]);
        let (camera, zoom) = (app.camera, app.zoom);
        let (from, to) = (Pos2::new(400., 300.), Pos2::new(500., 300.));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(
            &mut app,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        run(&mut app, vec![egui::Event::PointerMoved(to)]);
        run(&mut app, vec![button(to, false)]);
        assert!(app.camera.0 < camera.0);
        assert_eq!(app.zoom, zoom);
        run(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Plus,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
        );
        assert!((app.zoom - zoom * ZOOM_STEP).abs() < 1e-5);
        assert_eq!(context.zoom_factor(), 1.0);
    }

    /// Runs canvas frames on one egui context; waits for hit regions from the background render.
    fn canvas_runner(
        app: &mut ProcessCraft,
    ) -> impl FnMut(&mut ProcessCraft, Vec<egui::Event>) + use<> {
        let context = egui::Context::default();
        let run = move |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| app.canvas_ui(ui),
            );
            output.textures_delta.clear();
        };
        for _ in 0..200 {
            run(app, vec![]);
            if !app.hits.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!app.hits.is_empty(), "render produced no hit regions");
        run
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    /// Screen rectangle of `id`'s shape and its context pad entry `action`.
    fn on_screen(app: &ProcessCraft, id: &ElementSelector, action: PadAction) -> (Rect, Pos2) {
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let shape = document.diagrams[0]
            .shapes
            .iter()
            .find(|s| &s.element == id)
            .unwrap();
        let screen = to_screen(
            app.canvas.rect.unwrap(),
            app.camera,
            app.zoom,
            &shape.bounds,
        );
        let entry = context_pad(document, id, screen, app.canvas.rect.unwrap())
            .into_iter()
            .find(|(a, _)| *a == action)
            .unwrap();
        (screen, entry.1.center())
    }

    #[test]
    fn dragging_the_connect_handle_onto_a_task_creates_a_flow() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.select(None);
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        app.select(Some(start.clone()));
        let mut run = canvas_runner(&mut app);
        let drag = |app: &mut ProcessCraft,
                    run: &mut dyn FnMut(&mut ProcessCraft, Vec<egui::Event>),
                    from: Pos2,
                    to: Pos2| {
            run(app, vec![egui::Event::PointerMoved(from)]);
            run(app, vec![button(from, true)]);
            for step in 1..=5 {
                run(
                    app,
                    vec![egui::Event::PointerMoved(
                        from + (to - from) * (step as f32 / 5.0),
                    )],
                );
            }
            run(app, vec![button(to, false)]);
            run(app, vec![]);
        };
        let edges = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].diagrams[0]
                .edges
                .len()
        };
        let edges_before = edges(&app);
        let (_, handle) = on_screen(&app, &start, PadAction::Connect);
        let (target, _) = on_screen(&app, &task, PadAction::Delete);
        drag(&mut app, &mut run, handle, target.center());
        let snap = app.ctl.session.snapshot();
        assert_eq!(edges(&app), edges_before + 1);
        assert!(snap.project.documents[0].processes[0].flows.iter().any(|f| {
            matches!(&f.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == start)
                && matches!(&f.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == task)
        }));
        // Back into the start event is refused with the rule's reason and changes nothing.
        app.select(Some(task.clone()));
        run(&mut app, vec![]);
        let (_, handle) = on_screen(&app, &task, PadAction::Connect);
        let (start_rect, _) = on_screen(&app, &start, PadAction::Delete);
        drag(&mut app, &mut run, handle, start_rect.center());
        assert_eq!(edges(&app), edges_before + 1);
        assert!(app.ctl.error.contains("Start events"), "{}", app.ctl.error);
    }

    /// Press at `from`, move in five steps, release at `to`.
    fn drag_on(
        app: &mut ProcessCraft,
        run: &mut dyn FnMut(&mut ProcessCraft, Vec<egui::Event>),
        from: Pos2,
        to: Pos2,
    ) {
        run(app, vec![egui::Event::PointerMoved(from)]);
        run(app, vec![button(from, true)]);
        for step in 1..=5 {
            run(
                app,
                vec![egui::Event::PointerMoved(
                    from + (to - from) * (step as f32 / 5.0),
                )],
            );
        }
        run(app, vec![button(to, false)]);
        run(app, vec![]);
    }
    fn shape_bounds(app: &ProcessCraft, id: &ElementSelector) -> Bounds {
        app.ctl.session.snapshot().project.documents[0].diagrams[0]
            .shapes
            .iter()
            .find(|s| &s.element == id)
            .unwrap()
            .bounds
            .clone()
    }

    #[test]
    fn dragging_a_task_carries_its_boundary_event() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        app.add(NodeKind::BoundaryEvent);
        let event = app.selected.clone().unwrap();
        app.select(None);
        let (task_before, event_before) = (shape_bounds(&app, &task), shape_bounds(&app, &event));
        let mut run = canvas_runner(&mut app);
        let (screen, _) = on_screen(&app, &task, PadAction::Delete);
        let from = screen.center() - Vec2::new(0.0, screen.height() / 4.0);
        drag_on(&mut app, &mut run, from, from + Vec2::new(60.0, 40.0));
        let (task_after, event_after) = (shape_bounds(&app, &task), shape_bounds(&app, &event));
        assert_ne!(task_after, task_before, "{}", app.ctl.error);
        assert_eq!(event_after.x - event_before.x, task_after.x - task_before.x);
        assert_eq!(event_after.y - event_before.y, task_after.y - task_before.y);
    }

    #[test]
    fn dropping_a_task_on_a_flow_inserts_it() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.add(NodeKind::Task);
        let first = app.selected.clone().unwrap();
        app.select(None);
        app.add(NodeKind::Task);
        let second = app.selected.clone().unwrap();
        app.select(None);
        let mut run = canvas_runner(&mut app);
        let snap = app.ctl.session.snapshot();
        let edge = snap.project.documents[0].diagrams[0].edges[0].clone();
        let (a, b) = (
            &edge.waypoints[0],
            &edge.waypoints[edge.waypoints.len() - 1],
        );
        let mid = Bounds {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
            width: 0.0,
            height: 0.0,
        };
        let target = to_screen(app.canvas.rect.unwrap(), app.camera, app.zoom, &mid).min;
        let (screen, _) = on_screen(&app, &second, PadAction::Delete);
        drag_on(&mut app, &mut run, screen.center(), target);
        let snap = app.ctl.session.snapshot();
        let flows = &snap.project.documents[0].processes[0].flows;
        let ends = |f: &processcraft_core::model::Flow| {
            let id = |r: &Option<processcraft_core::bpmn_activity::ModelReference>| match r {
                Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) => id.clone(),
                _ => panic!("external end"),
            };
            (id(&f.source), id(&f.target))
        };
        assert_eq!(flows.len(), 2, "{}", app.ctl.error);
        assert!(
            flows
                .iter()
                .any(|f| ends(f) == (start.clone(), second.clone()))
        );
        assert!(
            flows
                .iter()
                .any(|f| ends(f) == (second.clone(), first.clone()))
        );
    }

    #[test]
    fn ctrl_a_selects_the_diagram_and_ctrl_d_duplicates_it() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        app.add(NodeKind::Task);
        let context = egui::Context::default();
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        };
        run(&mut app, vec![key(egui::Key::A)]);
        // Two shapes and the connecting flow.
        assert_eq!(app.multi.len(), 3);
        let first_x = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0]
                .bounds
                .x
        };
        // Arrow keys move the selection by 1 px.
        let x = first_x(&app);
        run(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(first_x(&app), x + 1.0, "{}", app.ctl.error);
        let before = app.ctl.session.snapshot();
        run(&mut app, vec![key(egui::Key::D)]);
        let after = app.ctl.session.snapshot();
        assert_eq!(after.undo_count, before.undo_count + 1);
        let process = &after.project.documents[0].processes[0];
        assert_eq!((process.nodes.len(), process.flows.len()), (4, 2));
        let diagram = &after.project.documents[0].diagrams[0];
        assert_eq!((diagram.shapes.len(), diagram.edges.len()), (4, 2));
        // The copies are selected, and offset by 20/20 from the originals.
        assert_eq!(app.multi.len(), 3);
        let shape = |id: &ElementSelector| diagram.shapes.iter().find(|s| &s.element == id);
        let copy = app.multi.iter().find_map(shape).unwrap();
        assert_eq!(copy.bounds.x, diagram.shapes[0].bounds.x + 20.0);
        assert_eq!(copy.bounds.y, diagram.shapes[0].bounds.y + 20.0);
    }
    #[test]
    fn auto_scroll_only_near_the_border() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        assert_eq!(auto_scroll_step(rect, Pos2::new(200.0, 150.0)), Vec2::ZERO);
        assert_eq!(
            auto_scroll_step(rect, Pos2::new(5.0, 295.0)),
            Vec2::new(-AUTO_SCROLL_STEP, AUTO_SCROLL_STEP)
        );
        assert_eq!(auto_scroll_step(rect, Pos2::new(-5.0, 150.0)), Vec2::ZERO);
    }

    #[test]
    fn context_pad_append_creates_a_connected_task_and_selects_it() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        let mut run = canvas_runner(&mut app);
        let (_, entry) = on_screen(&app, &start, PadAction::Append(NodeKind::Task));
        run(&mut app, vec![egui::Event::PointerMoved(entry)]);
        run(&mut app, vec![button(entry, true)]);
        run(&mut app, vec![button(entry, false)]);
        let task = app.selected.clone().unwrap();
        assert_ne!(task, start);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        assert_eq!(
            processcraft_core::queries::find_node(document, &task).map(|n| n.kind),
            Some(NodeKind::Task)
        );
        assert_eq!(document.diagrams[0].edges.len(), 1);
        // An end event offers no append entries.
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::splat(36.0));
        app.add(NodeKind::EndEvent);
        let end = app.selected.clone().unwrap();
        let snap = app.ctl.session.snapshot();
        let pad = context_pad(&snap.project.documents[0], &end, screen, Rect::EVERYTHING);
        assert!(
            pad.iter()
                .all(|(a, _)| !matches!(a, PadAction::Append(k) if *k != NodeKind::TextAnnotation))
        );
    }
    #[test]
    fn context_pad_adds_a_lane_below_a_pool() {
        let mut app = ProcessCraft::default();
        app.add_pool();
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let pool = document.collaboration.as_ref().unwrap().participants[0]
            .id
            .selector();
        let before = document.diagrams[0]
            .shapes
            .iter()
            .find(|s| s.element == pool)
            .unwrap()
            .bounds
            .clone();
        app.select(Some(pool.clone()));
        let mut run = canvas_runner(&mut app);
        let (_, entry) = on_screen(&app, &pool, PadAction::AddLane(LaneLocation::Below));
        run(&mut app, vec![egui::Event::PointerMoved(entry)]);
        run(&mut app, vec![button(entry, true)]);
        run(&mut app, vec![button(entry, false)]);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let shapes = &document.diagrams[0].shapes;
        let after = &shapes.iter().find(|s| s.element == pool).unwrap().bounds;
        assert_eq!(after.height, before.height + 120.0, "{}", app.ctl.error);
        // The pool had no lanes: it gets one covering its body and the new 120 high lane.
        let lanes = &document.processes[0].lanes;
        assert_eq!(lanes.len(), 2);
        let mut heights: Vec<f64> = lanes
            .iter()
            .map(|l| {
                shapes
                    .iter()
                    .find(|s| s.element == l.id)
                    .unwrap()
                    .bounds
                    .height
            })
            .collect();
        heights.sort_by(f64::total_cmp);
        assert_eq!(heights, vec![120.0, before.height]);
    }
    #[test]
    fn r_opens_the_replace_menu_and_enter_turns_a_task_into_a_user_task() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        let context = egui::Context::default();
        let frame = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // The context pad offers the wrench for the task.
        let snap = app.ctl.session.snapshot();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 80.0));
        assert!(
            context_pad(&snap.project.documents[0], &task, screen, Rect::EVERYTHING)
                .iter()
                .any(|(a, _)| *a == PadAction::Replace)
        );
        frame(&mut app, vec![key(egui::Key::R)]);
        assert!(app.canvas.replace.is_some());
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("User".into())]);
        assert_eq!(app.canvas.replace.as_ref().unwrap().query, "User");
        frame(&mut app, vec![key(egui::Key::Enter)]);
        assert!(app.canvas.replace.is_none(), "{}", app.ctl.error);
        let snap = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snap.project.documents[0], &task)
            .expect("same id");
        assert_eq!(node.kind, NodeKind::UserTask);
    }
    #[test]
    fn vacant_bounds_avoids_shapes_but_not_containers() {
        let shape = |bounds: Bounds| Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: "S".parse::<processcraft_core::ids::XmlId>().unwrap().into(),
            element: "E".parse().unwrap(),
            bounds,
            label: None,
            style: None,
        };
        let free = vacant_bounds(&[], (100.0, 100.0), 100.0, 80.0);
        assert_eq!(free, b(50.0, 60.0, 100.0, 80.0));
        let pool = shape(b(-1000.0, -1000.0, 3000.0, 3000.0));
        assert_eq!(
            vacant_bounds(std::slice::from_ref(&pool), (100.0, 100.0), 100.0, 80.0),
            free
        );
        let blocker = shape(b(80.0, 80.0, 100.0, 80.0));
        let moved = vacant_bounds(&[pool, blocker.clone()], (100.0, 100.0), 100.0, 80.0);
        assert_ne!(moved, free);
        let o = &blocker.bounds;
        assert!(
            moved.x + moved.width + 20.0 <= o.x
                || o.x + o.width + 20.0 <= moved.x
                || moved.y + moved.height + 20.0 <= o.y
                || o.y + o.height + 20.0 <= moved.y
        );
    }

    /// Frames running the editor shortcuts and the canvas on one egui context.
    fn shortcut_runner() -> impl FnMut(&mut ProcessCraft, Vec<egui::Event>) {
        let context = egui::Context::default();
        move |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        }
    }
    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    fn node_of(app: &ProcessCraft, id: &ElementSelector) -> processcraft_core::model::Node {
        processcraft_core::queries::find_node(&app.ctl.session.snapshot().project.documents[0], id)
            .unwrap()
            .clone()
    }

    #[test]
    fn e_edits_names_and_annotation_text_inline() {
        use processcraft_core::bpmn_model::NodeDetails;
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        let mut frame = shortcut_runner();
        // E opens the single-line name editor; Enter commits.
        frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
        assert!(
            app.canvas
                .label_edit
                .as_ref()
                .is_some_and(|e| !e.annotation)
        );
        frame(&mut app, vec![]);
        app.canvas.label_edit.as_mut().unwrap().text = "Review order".into();
        frame(&mut app, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert!(app.canvas.label_edit.is_none());
        assert_eq!(node_of(&app, &task).name.as_deref(), Some("Review order"));
        // A text annotation edits its text in a multi-line editor; Escape cancels.
        app.select(None);
        app.add(NodeKind::TextAnnotation);
        let note = app.selected.clone().unwrap();
        let text = |app: &ProcessCraft| match node_of(app, &note).details {
            Some(NodeDetails::TextAnnotation { text: Some(t), .. }) => {
                t.plain_text().unwrap_or_default().to_owned()
            }
            _ => String::new(),
        };
        frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
        assert!(app.canvas.label_edit.as_ref().is_some_and(|e| e.annotation));
        frame(&mut app, vec![]);
        app.canvas.label_edit.as_mut().unwrap().text = "discarded".into();
        frame(
            &mut app,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(app.canvas.label_edit.is_none());
        assert_eq!(text(&app), "");
        // Ctrl+Enter commits the multi-line text.
        frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
        frame(&mut app, vec![]);
        app.canvas.label_edit.as_mut().unwrap().text = "Line one\nLine two".into();
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&mut app, vec![key(egui::Key::Enter, ctrl)]);
        assert!(app.canvas.label_edit.is_none(), "{}", app.ctl.error);
        assert_eq!(text(&app), "Line one\nLine two");
    }

    #[test]
    fn flow_pad_switches_a_gateway_flow_to_default() {
        use processcraft_core::bpmn_model::NodeDetails;
        let mut app = ProcessCraft::default();
        app.add(NodeKind::ExclusiveGateway);
        let gateway = app.selected.clone().unwrap();
        app.add(NodeKind::Task);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let flow = document.processes[0].flows[0].id.selector();
        let pad = flow_pad(document, &flow, Pos2::ZERO, Rect::EVERYTHING);
        let actions: Vec<PadAction> = pad.iter().map(|(a, _)| *a).collect();
        // Gateways offer default flows; conditional flows are offered from activities.
        assert!(actions.contains(&PadAction::Flow(FlowVariant::Default)));
        assert!(actions.contains(&PadAction::Delete));
        // The current type (plain) is not offered.
        assert!(!actions.contains(&PadAction::Flow(FlowVariant::Plain)));
        app.select(Some(flow.clone()));
        app.pad_action(flow.clone(), PadAction::Flow(FlowVariant::Default));
        assert!(
            matches!(node_of(&app, &gateway).details, Some(NodeDetails::Gateway { default_flow: Some(ref f), .. }) if *f == flow),
            "{}",
            app.ctl.error
        );
    }

    #[test]
    fn click_to_connect_and_drop_on_empty_canvas_offers_append() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.select(None);
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        app.select(Some(start.clone()));
        let mut run = canvas_runner(&mut app);
        let flows = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].processes[0]
                .flows
                .len()
        };
        // Click the connect entry, then click the target.
        let (_, entry) = on_screen(&app, &start, PadAction::Connect);
        run(&mut app, vec![egui::Event::PointerMoved(entry)]);
        run(&mut app, vec![button(entry, true)]);
        run(&mut app, vec![button(entry, false)]);
        assert!(app.canvas.connect_click.is_some());
        let (target, _) = on_screen(&app, &task, PadAction::Delete);
        run(&mut app, vec![egui::Event::PointerMoved(target.center())]);
        run(&mut app, vec![button(target.center(), true)]);
        run(&mut app, vec![button(target.center(), false)]);
        assert!(app.canvas.connect_click.is_none());
        assert_eq!(flows(&app), 1, "{}", app.ctl.error);
        // Dropping a connection on empty canvas opens the append popup; Escape closes it.
        app.select(Some(task.clone()));
        run(&mut app, vec![]);
        let (_, handle) = on_screen(&app, &task, PadAction::Connect);
        let empty = app.canvas.rect.unwrap().right_bottom() - Vec2::new(40.0, 40.0);
        drag_on(&mut app, &mut run, handle, empty);
        assert!(app.canvas.append_menu.is_some(), "{}", app.ctl.error);
        run(
            &mut app,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(app.canvas.append_menu.is_none());
        drag_on(&mut app, &mut run, handle, empty);
        let menu = app.canvas.append_menu.take().unwrap();
        assert!(
            app.append_choices(&menu.source)
                .iter()
                .any(|(_, e)| e.kind == NodeKind::EndEvent)
        );
        let before = app.ctl.session.snapshot().undo_count;
        assert!(app.create_connected(menu.source, NodeKind::EndEvent.into(), menu.at));
        assert_eq!(app.ctl.session.snapshot().undo_count, before + 1);
        assert_eq!(flows(&app), 2);
        let end = app.selected.clone().unwrap();
        assert_eq!(node_of(&app, &end).kind, NodeKind::EndEvent);
        // An end event cannot start a sequence flow: no append popup entries.
        assert!(app.append_choices(&end).is_empty());
    }

    #[test]
    fn palette_triggers_are_explicit_and_timers_start_empty() {
        use processcraft_core::{
            bpmn_model::{EventDefinition, NodeDetails},
            replace::Trigger,
        };
        let entry = |kind, trigger| PaletteEntry {
            kind,
            trigger: Some(trigger),
        };
        let definitions =
            |app: &ProcessCraft| match node_of(app, app.selected.as_ref().unwrap()).details {
                Some(NodeDetails::Event { definitions, .. }) => definitions,
                _ => panic!("not an event"),
            };
        let mut app = ProcessCraft::default();
        app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Message));
        assert!(matches!(
            definitions(&app).as_slice(),
            [EventDefinition::Message {
                message_ref: None,
                ..
            }]
        ));
        app.select(None);
        app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Timer));
        assert!(matches!(
            definitions(&app).as_slice(),
            [EventDefinition::Timer {
                kind: None,
                expression: None,
                ..
            }]
        ));
        // Appended after a task (one undo step) and as a boundary event of it.
        app.select(None);
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        let undo = app.ctl.session.snapshot().undo_count;
        app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Message));
        assert_eq!(app.ctl.session.snapshot().undo_count, undo + 1);
        assert!(matches!(
            definitions(&app).as_slice(),
            [EventDefinition::Message { .. }]
        ));
        app.select(Some(task.clone()));
        app.add_entry(entry(NodeKind::BoundaryEvent, Trigger::Timer));
        assert_eq!(
            node_of(&app, app.selected.as_ref().unwrap()).kind,
            NodeKind::BoundaryEvent
        );
        assert!(matches!(
            definitions(&app).as_slice(),
            [EventDefinition::Timer {
                expression: None,
                ..
            }]
        ));
        // Dropped from a drag: the core create path gets the same trigger.
        app.select(None);
        assert!(app.drop_new(
            entry(NodeKind::IntermediateCatchEvent, Trigger::Message),
            Point {
                x: -600.0,
                y: -600.0
            }
        ));
        assert!(matches!(
            definitions(&app).as_slice(),
            [EventDefinition::Message { .. }]
        ));
        // The palette never offers a catch or boundary event without its trigger.
        assert!(
            super::super::shell::more_palette_entries()
                .iter()
                .all(|(_, e)| !matches!(
                    e.kind,
                    NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
                ) || e.trigger.is_some())
        );
    }

    #[test]
    fn snapping_uses_siblings_not_shapes_of_other_scopes() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        app.select(None);
        app.add(NodeKind::SubProcess);
        let sub = app.selected.clone().unwrap();
        let s = shape_bounds(&app, &sub);
        app.select(None);
        assert!(app.drop_new(
            NodeKind::Task.into(),
            Point {
                x: s.x + 80.0,
                y: s.y + 60.0
            }
        ));
        let inner = app.selected.clone().unwrap();
        let snap = app.ctl.session.snapshot();
        let d = &snap.project.documents[0];
        assert_eq!(
            processcraft_core::rules::locate(d, &inner).and_then(|l| l.scope),
            Some(sub.clone())
        );
        let diagram = &d.diagrams[0];
        let shape = diagram.shapes.iter().find(|s| s.element == task).unwrap();
        let a = shape.bounds.clone();
        let items = vec![(shape.id.selector(), a.clone())];
        // Mid 2 below the inner task's mid (another scope): no snap.
        let dx = s.x - 1000.0 - a.x;
        let dy = s.y + 62.0 - (a.y + a.height / 2.0);
        let free = snap_drag(d, diagram, &items, (dx, dy), 7.0, false);
        assert_eq!((free.dy, free.guide_y), (0.0, None));
        // Mid 3 below the subprocess mid (a sibling): snaps.
        let dy = s.y + 103.0 - (a.y + a.height / 2.0);
        let snapped = snap_drag(d, diagram, &items, (dx, dy), 7.0, false);
        assert_eq!(snapped.dy, -3.0);
    }

    #[test]
    fn choreography_scopes_are_cached_per_generation() {
        let mut app = ProcessCraft::default();
        let snap = app.ctl.session.snapshot();
        let first = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
        let again = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
        assert!(Arc::ptr_eq(&first, &again));
        app.add(NodeKind::Task);
        let snap = app.ctl.session.snapshot();
        let next = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
        assert!(!Arc::ptr_eq(&first, &next));
    }

    #[test]
    fn pasted_bpmn_xml_is_offered_as_a_new_model() {
        let xml = include_str!("../../../../tests/fixtures/bpmn/minimal.bpmn");
        assert!(looks_like_bpmn(xml));
        assert!(!looks_like_bpmn("Task\nReview order"));
        assert!(!looks_like_bpmn("<html><body>definitions</body></html>"));
        let mut app = ProcessCraft::default();
        let mut frame = shortcut_runner();
        frame(&mut app, vec![egui::Event::Paste("Task".into())]);
        assert!(app.canvas.paste_bpmn.is_none());
        assert!(
            app.ctl.error.contains("Nothing to paste"),
            "{}",
            app.ctl.error
        );
        frame(&mut app, vec![egui::Event::Paste(xml.into())]);
        assert!(app.canvas.paste_bpmn.is_some());
        // Importing goes through the file import preparation in the background.
        let ctx = egui::Context::default();
        let pasted = app.canvas.paste_bpmn.take().unwrap();
        app.import_bpmn_text(&ctx, pasted);
        let mut result = None;
        for _ in 0..500 {
            result = app.effects.poll();
            if result.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            matches!(result, Some(crate::effects::ResultValue::Import { .. })),
            "import did not prepare"
        );
    }
}
