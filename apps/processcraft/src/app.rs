//! The `ProcessCraft` eframe app: shared UI state, open-model lifecycle, and draft handling.
//! Editors keep a draft (typed draft fields, or a canvas gesture) while the user types; commit_* /
//! apply_*_draft turn a draft into one `Mutation` batch and submit it through `Controller::mutate`,
//! which runs it on the `Session`. A refused batch leaves the draft open with the error; history,
//! selection and navigation commit or refuse open drafts first. Panels live in `app/`.
use crate::{
    controller::{self, Controller},
    theme::Theme,
};
#[cfg(test)]
use egui::Sense;
use egui::{Color32, Pos2, Rect, Vec2};
use processcraft_core::{
    commands::{Mutation, Request},
    ids::{ElementSelector, XmlId},
    model::*,
};
use processcraft_render::Appearance;

/// What the unsaved-changes prompt or a finished save should do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingAction {
    New,
    Open,
    Close,
}

/// The workbench place shown in the main area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum View {
    /// The canvas for the open model.
    #[default]
    Model,
    Library,
    Dictionary,
    /// Activity table: another way of looking at the open model.
    Table,
}

fn recovery_age(ms: u64) -> String {
    if ms == 0 {
        return "Time unknown".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let minutes = now.saturating_sub(ms as u128) / 60000;
    if minutes < 1 {
        "Just now".into()
    } else if minutes < 60 {
        format!("{minutes} minutes ago")
    } else if minutes < 1440 {
        format!("{} hours ago", minutes / 60)
    } else {
        format!("{} days ago", minutes / 1440)
    }
}
/// Suggested file name stem for a project name (Windows-safe, never empty).
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim().trim_end_matches('.');
    if stem.is_empty() {
        "project".into()
    } else {
        stem.into()
    }
}
/// A recovery snapshot of an untouched new project holds nothing worth restoring.
fn is_pristine_project(project: &ProjectData) -> bool {
    let [document] = project.documents.as_slice() else {
        return false;
    };
    project.name == "Untitled project"
        && project.dictionary_entries.is_empty()
        && project.comments.is_empty()
        && project.description.is_empty()
        && project.resources.is_empty()
        && project.checkpoints.is_empty()
        && document.collaboration.is_none()
        && document.choreographies.is_empty()
        && document
            .processes
            .iter()
            .all(|p| p.nodes.is_empty() && p.lanes.is_empty() && p.flows.is_empty())
        && document
            .diagrams
            .iter()
            .all(|d| d.shapes.is_empty() && d.edges.is_empty())
}
fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Task => "Task",
        NodeKind::UserTask => "User task",
        NodeKind::ManualTask => "Manual task",
        NodeKind::StartEvent => "Start event",
        NodeKind::EndEvent => "End event",
        NodeKind::ExclusiveGateway => "Exclusive gateway",
        NodeKind::ParallelGateway => "Parallel gateway",
        NodeKind::TextAnnotation => "Annotation",
        NodeKind::DataObjectReference => "Data object",
        NodeKind::DataStoreReference => "Data store",
        NodeKind::ServiceTask => "Service task",
        NodeKind::SendTask => "Send task",
        NodeKind::ReceiveTask => "Receive task",
        NodeKind::ScriptTask => "Script task",
        NodeKind::BusinessRuleTask => "Business rule task",
        NodeKind::InclusiveGateway => "Inclusive gateway",
        NodeKind::ComplexGateway => "Complex gateway",
        NodeKind::EventBasedGateway => "Event-based gateway",
        NodeKind::IntermediateCatchEvent => "Intermediate catch event",
        NodeKind::IntermediateThrowEvent => "Intermediate throw event",
        NodeKind::BoundaryEvent => "Boundary event",
        NodeKind::SubProcess => "Subprocess",
        NodeKind::AdHocSubProcess => "Ad-hoc subprocess",
        NodeKind::Transaction => "Transaction",
        NodeKind::CallActivity => "Call activity",
    }
}
fn diagram_extents(diagram: &Diagram) -> Option<(f64, f64, f64, f64)> {
    let mut result: Option<(f64, f64, f64, f64)> = None;
    let mut include = |x: f64, y: f64| {
        result = Some(match result {
            Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
            None => (x, y, x, y),
        });
    };
    for shape in &diagram.shapes {
        let b = &shape.bounds;
        include(b.x, b.y);
        include(b.x + b.width, b.y + b.height);
        if let Some(b) = shape.label.as_ref().and_then(|label| label.bounds.as_ref()) {
            include(b.x, b.y);
            include(b.x + b.width, b.y + b.height);
        }
    }
    for edge in &diagram.edges {
        for point in &edge.waypoints {
            include(point.x, point.y);
        }
        if let Some(b) = edge.label.as_ref().and_then(|label| label.bounds.as_ref()) {
            include(b.x, b.y);
            include(b.x + b.width, b.y + b.height);
        } else if let Some(p) = edge.waypoints.get(edge.waypoints.len() / 2) {
            include(p.x + 120.0, p.y + 40.0);
        }
    }
    result
}
struct DragGesture {
    items: Vec<(ElementSelector, Bounds)>,
    origin: Pos2,
    offset: Vec2,
    context: Option<(u64, processcraft_core::ids::DocumentId, ElementSelector)>,
    label: bool,
    /// Snap correction (diagram units) and guides for the current pointer position.
    snap: processcraft_core::geometry::Snap,
}
impl DragGesture {
    fn new(items: Vec<(ElementSelector, Bounds)>, origin: Pos2) -> Self {
        Self {
            items,
            origin,
            offset: Vec2::ZERO,
            context: None,
            label: false,
            snap: Default::default(),
        }
    }
    fn update(&mut self, position: Option<Pos2>) {
        if let Some(pos) = position {
            self.offset = pos - self.origin;
        }
    }
}
fn begin_drag(
    response: &egui::Response,
    ctx: &egui::Context,
    items: Vec<(ElementSelector, Bounds)>,
) -> Option<DragGesture> {
    if response.drag_started() {
        ctx.input(|i| i.pointer.press_origin())
            .map(|origin| DragGesture::new(items, origin))
    } else {
        None
    }
}
fn finish_drag(response: &egui::Response, drag: &mut Option<DragGesture>) -> Option<DragGesture> {
    if response.drag_stopped() {
        drag.take()
    } else {
        None
    }
}
fn advance_drag(drag: &mut Option<DragGesture>, ctx: &egui::Context) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *drag = None;
    } else if let Some(drag) = drag {
        drag.update(ctx.input(|i| i.pointer.latest_pos()));
    }
}
pub struct ProcessCraft {
    ctl: Controller,
    #[cfg(windows)]
    attachment: Option<crate::attachment::Attachment>,
    effects: crate::effects::Effects,
    export_ready: Option<(Vec<u8>, Vec<String>, &'static str)>,
    export_uncertain: Option<Box<crate::effects::UncertainExport>>,
    export_lost: Option<Box<crate::effects::WriteContext>>,
    theme: Theme,
    review: review::State,
    collaboration: collaboration::State,
    shell: shell::ShellState,
    doc: usize,
    process_id: Option<ElementSelector>,
    diagram_id: Option<ElementSelector>,
    view: View,
    library_filter: String,
    dictionary_filter: String,
    dictionary_kind_filter: Option<DictionaryKind>,
    folder_name: String,
    folder_selected: Option<XmlId>,
    dictionary_selected: Option<processcraft_core::ids::DictionaryEntryId>,
    dictionary_name: String,
    dictionary_description: String,
    dictionary_kind: DictionaryKind,
    attributes: attributes::State,
    conventions: conventions::State,
    selected: Option<ElementSelector>,
    graph_selection: Option<(ElementSelector, ElementSelector, u64)>,
    multi: Vec<ElementSelector>,
    representation_selection: Vec<ElementSelector>,
    representation_diagram: Option<ElementSelector>,
    draft: String,
    draft_name_present: bool,
    bpmn_details: Option<processcraft_core::bpmn_model::NodeDetails>,
    bpmn_loop: Option<processcraft_core::bpmn_model::LoopCharacteristics>,
    bpmn_activity: Option<processcraft_core::bpmn_activity::ActivityProperties>,
    project_name: String,
    document_name: String,
    process_name: Option<String>,
    collaboration_name: Option<String>,
    diagram_name: Option<String>,
    definitions_draft: processcraft_core::model::DefinitionsMetadata,
    definitions_dirty: bool,
    problem_key: Option<(String, u64)>,
    problems: Vec<processcraft_core::Diagnostic>,
    docs: String,
    docs_dirty: bool,
    condition: Option<processcraft_core::bpmn_model::Expression>,
    drafts: drafts::Drafts,
    identity_page: usize,
    association_direction: Option<processcraft_core::bpmn_model::AssociationDirection>,
    owner: String,
    contact: String,
    is_flow: bool,
    is_node: bool,
    camera: (f64, f64),
    zoom: f32,
    left_width: f32,
    right_width: f32,
    show_left: bool,
    show_right: bool,
    create_anonymous: bool,
    renderer: crate::jobs::Renderer,
    recovery: crate::jobs::RecoveryWriter,
    diagnostics: crate::jobs::DiagnosticsWorker,
    /// Context of the running UI, used to wake it when background work finishes.
    repaint: egui::Context,
    settings: processcraft_io::settings::Settings,
    /// Path chosen from Open recent, opened once any unsaved-changes prompt is resolved.
    pending_open: Option<std::path::PathBuf>,
    /// File given on the command line (`processcraft <file>`), opened or imported on the first frame.
    pub startup_path: Option<std::path::PathBuf>,
    recovery_records: Vec<processcraft_io::recovery::RecoveryRecord>,
    recovery_visible: bool,
    recovery_generation: Option<(String, u64)>,
    recovery_queued: Option<(String, u64)>,
    changed_at: std::time::Instant,
    dirty_since: Option<std::time::Instant>,
    texture: Option<egui::TextureHandle>,
    render_key: String,
    render_failed: bool,
    hits: Vec<processcraft_render::HitRegion>,
    drag: Option<DragGesture>,
    canvas: canvas::CanvasState,
    confirm: Option<PendingAction>,
    after_save: Option<PendingAction>,
    after_save_ready: Option<bool>,
    allow_close: bool,
    draft_dirty: bool,
    source: Option<ElementSelector>,
    target: Option<ElementSelector>,
    /// Lane picked in the inspector for "Apply lane"; `None` removes the node from lanes.
    lane: Option<usize>,
}
impl Default for ProcessCraft {
    fn default() -> Self {
        let mut ctl = Controller::default();
        let recovery_records =
            match processcraft_io::recovery::RecoveryStore::new(crate::jobs::recovery_location())
                .and_then(|store| {
                    let mut records = store.list()?;
                    // Snapshots of an untouched new project are noise: drop them instead of offering them.
                    records.retain(|record| {
                        let pristine = store
                            .open(record)
                            .is_ok_and(|project| is_pristine_project(&project));
                        !(pristine && store.discard(record).is_ok())
                    });
                    Ok(records)
                }) {
                Ok(records) => records,
                Err(e) => {
                    ctl.error = format!("Could not list recovery snapshots: {e}");
                    Vec::new()
                }
            };
        let initial_process_name = ctl
            .session
            .snapshot()
            .project
            .documents
            .first()
            .and_then(|document| document.processes.first())
            .and_then(|process| process.name.clone());
        Self {
            ctl,
            #[cfg(windows)]
            attachment: None,
            effects: crate::effects::Effects::new(),
            export_ready: None,
            export_uncertain: None,
            export_lost: None,
            theme: Theme::Light,
            review: review::State::default(),
            collaboration: collaboration::State::default(),
            shell: shell::ShellState::default(),
            doc: 0,
            process_id: None,
            diagram_id: None,
            view: View::Model,
            library_filter: String::new(),
            dictionary_filter: String::new(),
            dictionary_kind_filter: None,
            folder_name: String::new(),
            folder_selected: None,
            dictionary_selected: None,
            dictionary_name: String::new(),
            dictionary_description: String::new(),
            dictionary_kind: DictionaryKind::Role,
            attributes: attributes::State::default(),
            conventions: conventions::State::default(),
            selected: None,
            graph_selection: None,
            multi: vec![],
            representation_selection: vec![],
            representation_diagram: None,
            draft: String::new(),
            draft_name_present: false,
            bpmn_details: None,
            bpmn_loop: None,
            bpmn_activity: None,
            project_name: "Untitled project".into(),
            document_name: "Untitled process".into(),
            process_name: initial_process_name,
            collaboration_name: None,
            diagram_name: None,
            definitions_draft: empty_definitions_metadata(),
            definitions_dirty: false,
            problem_key: None,
            problems: vec![],
            docs: String::new(),
            docs_dirty: false,
            condition: None,
            drafts: drafts::Drafts::default(),
            identity_page: 0,
            association_direction: None,
            owner: String::new(),
            contact: String::new(),
            is_flow: false,
            is_node: false,
            camera: (0.0, 0.0),
            zoom: 1.0,
            left_width: 220.0,
            right_width: 300.0,
            show_left: true,
            show_right: true,
            create_anonymous: false,
            renderer: crate::jobs::Renderer::new(),
            recovery: crate::jobs::RecoveryWriter::new(),
            diagnostics: crate::jobs::DiagnosticsWorker::new(),
            repaint: egui::Context::default(),
            settings: processcraft_io::settings::Settings::default(),
            pending_open: None,
            startup_path: None,
            recovery_records,
            recovery_visible: true,
            recovery_generation: None,
            recovery_queued: None,
            changed_at: std::time::Instant::now(),
            dirty_since: None,
            texture: None,
            render_key: String::new(),
            render_failed: false,
            hits: vec![],
            drag: None,
            canvas: Default::default(),
            confirm: None,
            after_save: None,
            after_save_ready: None,
            allow_close: false,
            draft_dirty: false,
            source: None,
            target: None,
            lane: None,
        }
    }
}
fn empty_definitions_metadata() -> processcraft_core::model::DefinitionsMetadata {
    processcraft_core::model::DefinitionsMetadata {
        target_namespace: processcraft_core::model::GENERATED_TARGET_NAMESPACE.into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: Vec::new(),
        namespaces: Vec::new(),
        foreign_attributes: Vec::new(),
    }
}
impl ProcessCraft {
    /// The open document of `snap`; the result borrows the snapshot, not `self`.
    fn current_document<'a>(
        &self,
        snap: &'a processcraft_core::commands::Snapshot,
    ) -> Option<&'a std::sync::Arc<BpmnDocument>> {
        snap.project.documents.get(self.doc)
    }
    pub fn apply_theme(&self, ctx: &egui::Context) {
        self.theme.apply(ctx);
    }
    pub fn apply_settings(&mut self, settings: processcraft_io::settings::Settings) {
        self.theme = if settings.dark {
            Theme::Dark
        } else {
            Theme::Light
        };
        self.left_width = settings.left_width.unwrap_or(self.left_width);
        self.right_width = settings.right_width.unwrap_or(self.right_width);
        self.show_left = settings.show_left.unwrap_or(self.show_left);
        self.show_right = settings.show_right.unwrap_or(self.show_right);
        self.settings = settings;
    }
    fn remember_recent(&mut self, path: &std::path::Path) {
        self.settings.remember(path);
        self.settings.save();
    }
    fn select(&mut self, id: Option<ElementSelector>) {
        // Selection-bound drafts only: collaboration, review and dictionary drafts survive a
        // selection change (see `has_drafts` for the full set), and attribute definition drafts
        // are not tied to the selected element.
        if (self.draft_dirty || self.attributes.has_cell_drafts() || self.drafts.any())
            && !self.commit_all_drafts()
        {
            return;
        }
        self.representation_selection.clear();
        self.representation_diagram = None;
        self.multi = id.clone().into_iter().collect();
        self.selected = id.clone();
        self.graph_selection = None;
        self.draft.clear();
        self.draft_name_present = false;
        self.bpmn_details = None;
        self.bpmn_loop = None;
        self.bpmn_activity = None;
        self.docs.clear();
        self.docs_dirty = false;
        self.condition = None;
        self.association_direction = None;
        self.owner.clear();
        self.contact.clear();
        self.is_flow = false;
        self.is_node = false;
        let snap = self.ctl.session.snapshot();
        if id.is_none() {
            self.project_name = snap.project.name.clone();
            self.definitions_dirty = false;
            self.definitions_draft = snap
                .project
                .documents
                .get(self.doc)
                .and_then(|d| d.definitions.clone())
                .unwrap_or_else(empty_definitions_metadata);
            if snap
                .project
                .documents
                .get(self.doc)
                .is_some_and(|d| d.definitions.is_none() && d.provenance.is_some())
            {
                self.definitions_draft.target_namespace.clear();
            }
            if let Some(document) = self.current_document(&snap) {
                self.process_name = document
                    .processes
                    .iter()
                    .find(|process| self.process_id.as_ref().is_none_or(|id| process.id == *id))
                    .and_then(|process| process.name.clone());
                self.collaboration_name = document
                    .collaboration
                    .as_ref()
                    .and_then(|collaboration| collaboration.name.clone());
                self.diagram_name = document
                    .diagrams
                    .iter()
                    .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id))
                    .and_then(|diagram| diagram.name.clone());
            }
            self.document_name = snap
                .project
                .documents
                .get(self.doc)
                .map_or(String::new(), |d| d.name.clone());
        }
        if let Some(d) = self.current_document(&snap) {
            if let Some(selected_id) = &id {
                if let Some(process) = processcraft_core::queries::scopes(d).into_iter().find(|p| {
                    p.id == selected_id
                        || p.nodes.iter().any(|n| n.id == *selected_id)
                        || p.lanes.iter().any(|l| l.id == *selected_id)
                        || p.flows.iter().any(|f| f.id == *selected_id)
                }) {
                    self.process_id = Some(process.id.selector());
                }
                let current_contains = d
                    .diagrams
                    .iter()
                    .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| &diagram.id == id))
                    .is_some_and(|diagram| {
                        diagram.shapes.iter().any(|s| s.element == *selected_id)
                            || diagram.edges.iter().any(|e| e.element == *selected_id)
                    });
                if !current_contains
                    && let Some(diagram) = d.diagrams.iter().find(|diagram| {
                        diagram.shapes.iter().any(|s| s.element == *selected_id)
                            || diagram.edges.iter().any(|e| e.element == *selected_id)
                    })
                {
                    self.diagram_id = Some(diagram.id.selector());
                }
            }
            for p in processcraft_core::queries::scopes(d) {
                for n in p.nodes {
                    if id
                        .as_ref()
                        .is_some_and(|selected| n.id.selector() == *selected)
                    {
                        self.draft_name_present = n.name.is_some();
                        self.draft = n.name.clone().unwrap_or_default();
                        if n.kind == NodeKind::TextAnnotation
                            && let Some(
                                processcraft_core::bpmn_model::NodeDetails::TextAnnotation {
                                    text: Some(text),
                                    ..
                                },
                            ) = &n.details
                        {
                            self.draft = text.plain_text().unwrap_or_default().to_owned();
                        }
                        self.docs = n.documentation.plain_text().unwrap_or_default().to_owned();
                        self.is_node = true;
                        self.bpmn_details = n.details.clone();
                        self.bpmn_loop = n.loop_characteristics.clone();
                        self.bpmn_activity = n.activity.clone();
                        self.owner = n.owner.clone();
                        self.contact = n.contact.clone();
                    }
                }
                for lane in p.lanes {
                    if id
                        .as_ref()
                        .is_some_and(|selected| lane.id.selector() == *selected)
                    {
                        self.draft_name_present = lane.name.is_some();
                        self.draft = lane.name.clone().unwrap_or_default();
                    }
                }
                for f in p.flows {
                    if id
                        .as_ref()
                        .is_some_and(|selected| f.id.selector() == *selected)
                    {
                        self.draft_name_present = f.name.is_some();
                        self.draft = f.name.clone().unwrap_or_default();
                        self.docs = f.documentation.plain_text().unwrap_or_default().to_owned();
                        self.condition = f.condition.clone();
                        self.is_flow = true;
                        self.association_direction = f.association_direction;
                    }
                }
            }
        }
        if let Some(document) = self.current_document(&snap)
            && let Some(selected) = &id
        {
            if !self.is_node
                && let Some(node) = processcraft_core::queries::find_node(document, selected)
            {
                self.draft_name_present = node.name.is_some();
                self.draft = match &node.details {
                    Some(processcraft_core::bpmn_model::NodeDetails::TextAnnotation {
                        text: Some(body),
                        ..
                    }) => body.plain_text().unwrap_or_default().to_owned(),
                    _ => node.name.clone().unwrap_or_default(),
                };
                self.docs = node
                    .documentation
                    .plain_text()
                    .unwrap_or_default()
                    .to_owned();
                self.is_node = true;
            }
            if !self.is_flow
                && let Some(flow) = processcraft_core::queries::find_flow(document, selected)
            {
                self.draft_name_present = flow.name.is_some();
                self.draft = flow.name.clone().unwrap_or_default();
                self.docs = flow
                    .documentation
                    .plain_text()
                    .unwrap_or_default()
                    .to_owned();
                self.is_flow = true;
            }
        }
        if let Some(d) = self.current_document(&snap)
            && let Some(selected) = &id
        {
            for root in d
                .collaboration
                .iter()
                .chain(d.additional_collaborations.iter())
            {
                if root.id == *selected {
                    self.draft_name_present = root.name.is_some();
                    self.draft = root.name.clone().unwrap_or_default();
                }
                if let Some(participant) = root.participants.iter().find(|v| v.id == *selected) {
                    self.draft_name_present = participant.name.is_some();
                    self.draft = participant.name.clone().unwrap_or_default();
                }
                if let Some(node) = root.conversation_nodes.iter().find(|v| v.id == *selected) {
                    self.draft_name_present = node.name.is_some();
                    self.draft = node.name.clone().unwrap_or_default();
                }
                if let Some(link) = root.conversation_links.iter().find(|v| v.id == *selected) {
                    self.draft_name_present = link.name.is_some();
                    self.draft = link.name.clone().unwrap_or_default();
                }
                if let Some(flow) = root.flows.iter().find(|v| v.id == *selected) {
                    self.draft_name_present = flow.name.is_some();
                    self.draft = flow.name.clone().unwrap_or_default();
                    self.docs = flow
                        .documentation
                        .plain_text()
                        .unwrap_or_default()
                        .to_owned();
                    self.is_flow = true;
                }
            }
        }
        if let Some(document) = self.current_document(&snap)
            && let Some(selected) = &id
        {
            for choreography in &document.choreographies {
                if choreography.id == *selected {
                    self.draft_name_present = choreography.name.is_some();
                    self.draft = choreography.name.clone().unwrap_or_default();
                }
                if let Some(node) = choreography
                    .conversation_nodes
                    .iter()
                    .find(|node| node.id == *selected)
                {
                    self.draft_name_present = node.name.is_some();
                    self.draft = node.name.clone().unwrap_or_default();
                }
                if let Some(link) = choreography
                    .conversation_links
                    .iter()
                    .find(|link| link.id == *selected)
                {
                    self.draft_name_present = link.name.is_some();
                    self.draft = link.name.clone().unwrap_or_default();
                }
                if let Some(participant) = choreography
                    .participants
                    .iter()
                    .find(|participant| participant.id == *selected)
                {
                    self.draft_name_present = participant.name.is_some();
                    self.draft = participant.name.clone().unwrap_or_default();
                }
                if let Some(activity) = choreography
                    .activities
                    .iter()
                    .find(|activity| activity.id == *selected)
                {
                    self.draft_name_present = activity.name.is_some();
                    self.draft = activity.name.clone().unwrap_or_default();
                }
                if let Some(flow) = choreography
                    .message_flows
                    .iter()
                    .chain(&choreography.sequence_flows)
                    .find(|flow| flow.id == *selected)
                {
                    self.draft_name_present = flow.name.is_some();
                    self.draft = flow.name.clone().unwrap_or_default();
                    self.docs = flow
                        .documentation
                        .plain_text()
                        .unwrap_or_default()
                        .to_owned();
                    self.condition = flow.condition.clone();
                    self.is_flow = true;
                    self.association_direction = flow.association_direction;
                }
            }
            for scope in processcraft_core::queries::choreography_scopes(document) {
                if scope.root.graph.is_some() {
                    for element in &scope.ordered {
                        use processcraft_core::queries::ChoreographyElementRef;
                        let identity = match element {
                            ChoreographyElementRef::Activity(value) => &value.id,
                            ChoreographyElementRef::Event(value)
                            | ChoreographyElementRef::Gateway(value) => &value.id,
                            ChoreographyElementRef::SequenceFlow(value) => &value.id,
                        };
                        if identity != selected {
                            continue;
                        }
                        self.graph_selection = Some((
                            scope.root.id.selector(),
                            scope.owner.clone(),
                            snap.generation,
                        ));
                        match element {
                            ChoreographyElementRef::Activity(value) => {
                                self.draft_name_present = value.name.is_some();
                                self.draft = value.name.clone().unwrap_or_default();
                            }
                            ChoreographyElementRef::Event(value)
                            | ChoreographyElementRef::Gateway(value) => {
                                self.draft_name_present = value.name.is_some();
                                self.draft = value.name.clone().unwrap_or_default();
                                self.docs = value
                                    .documentation
                                    .plain_text()
                                    .unwrap_or_default()
                                    .to_owned();
                                self.bpmn_details = value.details.clone();
                                self.is_node = true;
                            }
                            ChoreographyElementRef::SequenceFlow(value) => {
                                self.draft_name_present = value.name.is_some();
                                self.draft = value.name.clone().unwrap_or_default();
                                self.docs = value
                                    .documentation
                                    .plain_text()
                                    .unwrap_or_default()
                                    .to_owned();
                                self.condition = value.condition.clone();
                                self.is_flow = true;
                            }
                        }
                    }
                }
                if let Some(activity) = scope
                    .activities
                    .iter()
                    .find(|activity| activity.id == *selected)
                {
                    self.draft_name_present = activity.name.is_some();
                    self.draft = activity.name.clone().unwrap_or_default();
                }
                if let Some(flow) = scope.flows.iter().find(|flow| flow.id == *selected) {
                    self.draft_name_present = flow.name.is_some();
                    self.draft = flow.name.clone().unwrap_or_default();
                    self.docs = flow
                        .documentation
                        .plain_text()
                        .unwrap_or_default()
                        .to_owned();
                    self.condition = flow.condition.clone();
                    self.is_flow = true;
                    self.association_direction = flow.association_direction;
                }
            }
        }
        if let Some(document) = self.current_document(&snap)
            && let Some(selected) = &id
        {
            let name = document
                .partner_entities
                .iter()
                .find(|value| value.id.as_ref().is_some_and(|id| id == selected))
                .map(|value| &value.name)
                .or_else(|| {
                    document
                        .partner_roles
                        .iter()
                        .find(|value| value.id.as_ref().is_some_and(|id| id == selected))
                        .map(|value| &value.name)
                });
            if let Some(name) = name {
                self.draft_name_present = name.is_some();
                self.draft = name.clone().unwrap_or_default();
            }
        }
        self.ctl.session.set_selection(
            id.map(|xml_id| processcraft_core::ids::ElementKey {
                document_id: snap.project.documents[self.doc].id,
                xml_id,
            })
            .into_iter()
            .collect(),
        );
    }
    /// Apply the typed draft in `slot`: `build` turns it into one batch (recording its own
    /// validation error on the draft); on success the draft closes. No draft is a success.
    fn apply_typed_draft<D>(
        &mut self,
        slot: fn(&mut Self) -> &mut Option<D>,
        build: impl FnOnce(&mut D, &BpmnDocument) -> Result<Vec<Mutation>, String>,
    ) -> Result<(), String> {
        let snap = self.ctl.session.snapshot();
        let Some(document) = self.current_document(&snap) else {
            return Err("no model is open".into());
        };
        let Some(draft) = slot(self).as_mut() else {
            return Ok(());
        };
        let mutations = build(draft, document)?;
        // An empty batch means nothing changed: the draft just closes.
        if !mutations.is_empty() && !self.ctl.mutate(mutations) {
            return Err(self.ctl.error.clone());
        }
        *slot(self) = None;
        Ok(())
    }
    /// Apply every open typed draft (selection change, history, navigation), as simple fields are:
    /// an unchanged draft applies as a no-op (history and redo stay intact); an invalid one is
    /// refused with its reason and stays open. Untouched creation drafts just close.
    fn commit_typed_drafts(&mut self) -> bool {
        // NOTE: the early exit ignores the expression draft, so it only blocks below when another
        // typed draft is also open; `commit_draft` refuses it otherwise. Kept as it was.
        if !self.drafts.any_typed() {
            return true;
        }
        // The expression metadata editor writes into its owner's field, which only its panel can
        // reach, so it is the one draft that must be finished by hand.
        if self.drafts.expression_draft.is_some() {
            self.ctl.error = "Apply or cancel the expression metadata draft first.".into();
            return false;
        }
        if self.band_draft_untouched() {
            self.drafts.band_draft = None;
        }
        if self.graph_data_draft_untouched() {
            self.drafts.graph_data_draft = None;
        }
        // Simple-field mirrors the user did not edit follow what the drafts change.
        let mirrored = |app: &Self| {
            let snap = app.ctl.session.snapshot();
            let document = snap.project.documents.get(app.doc)?;
            let id = app.selected.as_ref()?;
            let node = processcraft_core::queries::find_node(document, id);
            Some((
                node.map(|n| n.details.clone()),
                node.map(|n| n.loop_characteristics.clone()),
                node.map(|n| n.activity.clone()),
                processcraft_core::queries::find_flow(document, id).map(|f| f.condition.clone()),
            ))
        };
        let before = mirrored(self);
        for &(name, apply) in drafts::Drafts::COMMIT {
            if let Err(reason) = apply(self) {
                self.ctl.error = format!(
                    "The {name} draft is not valid: {}. Fix it or cancel it first.",
                    reason.trim_end_matches('.')
                );
                return false;
            }
        }
        if let (Some((details, lp, activity, condition)), Some(after)) = (before, mirrored(self)) {
            if details.as_ref() == Some(&self.bpmn_details) {
                self.bpmn_details = after.0.flatten();
            }
            if lp.as_ref() == Some(&self.bpmn_loop) {
                self.bpmn_loop = after.1.flatten();
            }
            if activity.as_ref() == Some(&self.bpmn_activity) {
                self.bpmn_activity = after.2.flatten();
            }
            if condition.as_ref() == Some(&self.condition) {
                self.condition = after.3.flatten();
            }
        }
        true
    }
    /// Typed drafts, then the simple fields.
    fn commit_all_drafts(&mut self) -> bool {
        self.commit_typed_drafts() && self.commit_attribute_drafts() && self.commit_draft()
    }
    fn commit_draft(&mut self) -> bool {
        if self.drafts.event_draft.is_some() || self.drafts.event_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the event draft first.".into();
            return false;
        }
        if self.drafts.multiplicity_draft.is_some()
            || self.drafts.message_ref_draft.is_some()
            || self.drafts.band_draft.is_some()
        {
            self.ctl.error =
                "Apply or cancel the participant, band, or message draft first.".into();
            return false;
        }
        if self.drafts.choreography_flow_draft.is_some() {
            self.ctl.error = "Apply or cancel the choreography details draft first.".into();
            return false;
        }
        if self.drafts.loop_advanced_draft.is_some() {
            self.ctl.error = "Apply or cancel the multi-instance draft first.".into();
            return false;
        }
        if self.drafts.correlation_keys_draft.is_some() {
            self.ctl.error = "Apply or cancel the correlation keys draft first.".into();
            return false;
        }
        if self.drafts.flow_metadata_draft.is_some() {
            self.ctl.error = "Apply or cancel the flow details draft first.".into();
            return false;
        }
        if self.drafts.process_activity_draft.is_some() {
            self.ctl.error = "Apply or cancel the process behavior draft first.".into();
            return false;
        }
        if self.drafts.call_draft.is_some() {
            self.ctl.error = "Apply or cancel the called element draft first.".into();
            return false;
        }
        if self.drafts.callable_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the global task draft first.".into();
            return false;
        }
        if self.drafts.activity_roles_draft.is_some() {
            self.ctl.error = "Apply or cancel the activity roles draft first.".into();
            return false;
        }
        if self.drafts.correlation_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the correlation draft first.".into();
            return false;
        }
        if self.drafts.definitions_extra_draft.is_some() {
            self.ctl.error = "Apply or cancel the definitions details draft first.".into();
            return false;
        }
        if self.drafts.style_draft.is_some() {
            self.ctl.error = "Apply or cancel the diagram font draft first.".into();
            return false;
        }
        if self.drafts.metadata_draft.is_some() {
            self.ctl.error = "Apply or cancel the advanced metadata draft first.".into();
            return false;
        }
        if self.drafts.body_draft.is_some() {
            self.ctl.error = "Apply or cancel the content draft first.".into();
            return false;
        }
        if self.drafts.documentation_draft.is_some() {
            self.ctl.error = "Apply or cancel the documentation draft first.".into();
            return false;
        }
        if self.drafts.di_draft.is_some() {
            self.ctl.error = "Apply or cancel the diagram properties draft first.".into();
            return false;
        }
        if self.drafts.xml_id_draft.is_some() {
            self.ctl.error = "Apply or cancel the XML ID draft first.".into();
            return false;
        }
        if self.drafts.data_draft.is_some() {
            self.ctl.error = "Apply or cancel the data draft first.".into();
            return false;
        }
        if self.drafts.graph_data_draft.is_some() {
            self.ctl.error =
                "Apply or cancel the choreography data association draft first.".into();
            return false;
        }
        if self.drafts.binding_draft.is_some() {
            self.ctl.error = "Apply or cancel the activity reference draft first.".into();
            return false;
        }
        if self.drafts.catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the supporting catalog draft first.".into();
            return false;
        }
        if self.drafts.expression_draft.is_some() {
            self.ctl.error = "Apply or cancel the expression metadata draft first.".into();
            return false;
        }
        if !self.draft_dirty {
            return true;
        }
        if let (Some(id), Some((root, owner, generation))) =
            (self.selected.clone(), self.graph_selection.clone())
        {
            let snapshot = self.ctl.session.snapshot();
            if snapshot.generation != generation {
                self.ctl.error =
                    "The model changed; discard this draft and select the element again.".into();
                return false;
            }
            let Some(document) = self.current_document(&snapshot) else {
                return false;
            };
            let Some(scope) = processcraft_core::queries::choreography_scopes(document)
                .into_iter()
                .find(|scope| scope.root.id == root && scope.owner == owner)
            else {
                self.ctl.error = "The choreography scope is no longer available.".into();
                return false;
            };
            use processcraft_core::bpmn_collaboration::ChoreographyFlowElement;
            use processcraft_core::queries::ChoreographyElementRef;
            let name = self.draft_name_present.then(|| self.draft.clone());
            let element = scope.ordered.into_iter().find_map(|element| match element {
                ChoreographyElementRef::Activity(original) if original.id == id => {
                    let mut value = original.clone();
                    value.name = name.clone();
                    Some(ChoreographyFlowElement::Activity(Box::new(value)))
                }
                ChoreographyElementRef::Event(original) if original.id == id => {
                    let mut value = original.clone();
                    value.name = name.clone();
                    if self.docs_dirty {
                        value.documentation = self.docs.clone().into();
                    }
                    value.details = self.bpmn_details.clone();
                    Some(ChoreographyFlowElement::Event(Box::new(value)))
                }
                ChoreographyElementRef::Gateway(original) if original.id == id => {
                    let mut value = original.clone();
                    value.name = name.clone();
                    if self.docs_dirty {
                        value.documentation = self.docs.clone().into();
                    }
                    value.details = self.bpmn_details.clone();
                    Some(ChoreographyFlowElement::Gateway(Box::new(value)))
                }
                ChoreographyElementRef::SequenceFlow(original) if original.id == id => {
                    let mut value = original.clone();
                    value.name = name.clone();
                    if self.docs_dirty {
                        value.documentation = self.docs.clone().into();
                    }
                    value.condition = self.condition.clone();
                    Some(ChoreographyFlowElement::SequenceFlow(Box::new(value)))
                }
                _ => None,
            });
            let Some(element) = element else {
                self.ctl.error = "The choreography element is no longer available.".into();
                return false;
            };
            if !self.ctl.mutate_at_generation(
                generation,
                vec![Mutation::UpdateChoreographyFlowElement {
                    document_id: document.id,
                    choreography_id: root.clone(),
                    scope_id: owner.clone(),
                    element,
                }],
            ) {
                return false;
            }
            self.graph_selection = Some((root, owner, self.ctl.session.snapshot().generation));
            self.draft_dirty = false;
            self.docs_dirty = false;
            return true;
        }
        if let Some(id) = self.selected.clone() {
            let snap = self.ctl.session.snapshot();
            if let Some(d) = self.current_document(&snap) {
                let annotation = processcraft_core::queries::find_node(d, &id)
                    .filter(|node| node.kind == NodeKind::TextAnnotation);
                let mut mutations = Vec::new();
                if let Some(node) = annotation {
                    let original = match &node.details {
                        Some(processcraft_core::bpmn_model::NodeDetails::TextAnnotation {
                            text,
                            ..
                        }) => match text {
                            Some(body) => body.plain_text(),
                            None => node.name.as_deref(),
                        }
                        .unwrap_or(""),
                        _ => node.name.as_deref().unwrap_or(""),
                    };
                    if original != self.draft {
                        mutations.push(Mutation::SetTextAnnotation {
                            document_id: d.id,
                            element_id: id.clone(),
                            text: Some(self.draft.clone()),
                        });
                    }
                } else if inspector::selection_has_name(d, &id) {
                    mutations.push(Mutation::Rename {
                        document_id: d.id,
                        element_id: id.clone(),
                        name: self.draft_name_present.then(|| self.draft.clone()),
                    });
                }
                if (self.is_node || self.is_flow) && self.docs_dirty {
                    mutations.push(Mutation::Documentation {
                        document_id: d.id,
                        element_id: id.clone(),
                        documentation: self.docs.clone(),
                    });
                }
                if self.is_flow
                    && d.capability.allows_structural_editing()
                    && processcraft_core::queries::find_flow(d, &id)
                        .is_some_and(|f| f.kind == FlowKind::Sequence)
                {
                    mutations.push(Mutation::SetFlowCondition {
                        document_id: d.id,
                        flow_id: id.clone(),
                        condition: self.condition.clone(),
                    });
                } else if self.is_node && d.capability.allows_structural_editing() {
                    if processcraft_core::queries::find_node(d, &id)
                        .is_some_and(|n| n.details != self.bpmn_details)
                    {
                        mutations.push(Mutation::SetNodeDetails {
                            document_id: d.id,
                            element_id: id.clone(),
                            details: self.bpmn_details.clone(),
                        });
                    }
                    if processcraft_core::queries::find_node(d, &id)
                        .is_some_and(|n| n.loop_characteristics != self.bpmn_loop)
                    {
                        mutations.push(Mutation::SetNodeLoop {
                            document_id: d.id,
                            element_id: id.clone(),
                            loop_characteristics: self.bpmn_loop.clone(),
                        });
                    }
                    if processcraft_core::queries::find_node(d, &id)
                        .is_some_and(|n| n.activity != self.bpmn_activity)
                    {
                        mutations.push(Mutation::SetActivityProperties {
                            document_id: d.id,
                            element_id: id.clone(),
                            activity: self.bpmn_activity.clone(),
                        });
                    }
                    mutations.push(Mutation::SetNodeOwnerContact {
                        document_id: d.id,
                        element_id: id.clone(),
                        owner: self.owner.clone(),
                        contact: self.contact.clone(),
                    });
                }
                if self.is_flow
                    && d.capability.allows_structural_editing()
                    && processcraft_core::queries::find_flow(d, &id).is_some_and(|f| {
                        f.kind == FlowKind::Association
                            && f.association_direction != self.association_direction
                    })
                {
                    mutations.push(Mutation::SetAssociationDirection {
                        document_id: d.id,
                        flow_id: id.clone(),
                        direction: self.association_direction,
                    });
                }
                let ok = self.ctl.mutate(mutations);
                if !ok {
                    return false;
                }
                if self.is_node {
                    let current = self.ctl.session.snapshot();
                    self.bpmn_details = current
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|document| processcraft_core::queries::find_node(document, &id))
                        .and_then(|node| node.details.clone());
                }
            }
        }
        if self.selected.is_none() {
            let snap = self.ctl.session.snapshot();
            let Some(d) = self.current_document(&snap) else {
                return false;
            };
            let mut mutations = vec![
                Mutation::ProjectProperties {
                    name: self.project_name.clone(),
                    description: snap.project.description.clone(),
                },
                Mutation::DocumentProperties {
                    document_id: d.id,
                    name: self.document_name.clone(),
                    folder: d.folder.clone(),
                },
            ];
            if let Some(process) = d
                .processes
                .iter()
                .find(|process| self.process_id.as_ref().is_none_or(|id| process.id == *id))
                && process.name != self.process_name
            {
                mutations.push(Mutation::Rename {
                    document_id: d.id,
                    element_id: process.id.selector(),
                    name: self.process_name.clone(),
                });
            }
            if let Some(collaboration) = &d.collaboration
                && collaboration.name != self.collaboration_name
            {
                mutations.push(Mutation::SetCollaborationName {
                    document_id: d.id,
                    name: self.collaboration_name.clone(),
                });
            }
            if let Some(diagram) = d
                .diagrams
                .iter()
                .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id))
                && diagram.name != self.diagram_name
            {
                mutations.push(Mutation::SetDiagramName {
                    document_id: d.id,
                    diagram_id: diagram.id.selector(),
                    name: self.diagram_name.clone(),
                });
            }
            if self.definitions_dirty {
                if self.definitions_draft.target_namespace.is_empty() {
                    self.ctl.error =
                        "Target namespace is required for definitions metadata.".into();
                    return false;
                }
                mutations.push(Mutation::SetDefinitionsMetadata {
                    document_id: d.id,
                    metadata: self.definitions_draft.clone(),
                });
            }
            if !self.ctl.mutate(mutations) {
                return false;
            }
        }
        self.definitions_dirty = false;
        self.docs_dirty = false;
        self.draft_dirty = false;
        true
    }
    fn history(&mut self, request: Request) {
        if self.collaboration.has_draft() {
            self.ctl.error =
                "Apply or discard the collaboration draft before using history.".into();
            return;
        }
        if self.review.has_draft() {
            self.ctl.error = "Post or discard the comment draft before using history.".into();
            return;
        }
        let generation = self.ctl.session.snapshot().generation;
        if !self.commit_all_drafts() || !self.commit_dictionary_draft() {
            return;
        }
        // An unchanged draft applies as a no-op, so redo still works; a changed one is a new
        // edit, which leaves nothing to redo.
        if matches!(request, Request::Redo) && self.ctl.session.snapshot().generation != generation
        {
            self.ctl
                .notify("The open draft was applied, so there is nothing left to redo.");
            return;
        }
        if let Some(changes) = self.ctl.apply_changes(request) {
            self.remap_context(&changes.selector_mappings);
            self.restore_selection();
            self.refresh_dictionary_editor();
        }
    }
    fn remap_context(&mut self, mappings: &[processcraft_core::commands::SelectorMapping]) {
        self.collaboration.remap_selectors(mappings);
        let document_id = self
            .ctl
            .session
            .snapshot()
            .project
            .documents
            .get(self.doc)
            .map(|document| document.id);
        for mapping in mappings
            .iter()
            .filter(|mapping| Some(mapping.document_id) == document_id)
        {
            for value in [
                &mut self.process_id,
                &mut self.diagram_id,
                &mut self.representation_diagram,
                &mut self.selected,
            ] {
                if value.as_ref() == Some(&mapping.from) {
                    *value = Some(mapping.to.clone());
                }
            }
            for value in self
                .multi
                .iter_mut()
                .chain(self.representation_selection.iter_mut())
            {
                if *value == mapping.from {
                    *value = mapping.to.clone();
                }
            }
        }
    }
    fn restore_selection(&mut self) {
        let previous = self.selected.clone();
        let representations = self.representation_selection.clone();
        let representation_diagram = self.representation_diagram.clone();
        let keys = self.ctl.session.snapshot().selection;
        let ids: Vec<_> = keys
            .iter()
            .filter(|k| {
                self.ctl
                    .session
                    .snapshot()
                    .project
                    .documents
                    .get(self.doc)
                    .is_some_and(|d| d.id == k.document_id)
            })
            .map(|k| k.xml_id.clone())
            .collect();
        self.select(ids.first().cloned());
        if previous == self.selected && representation_diagram == self.diagram_id {
            let snapshot = self.ctl.session.snapshot();
            if let Some(diagram) = snapshot
                .project
                .documents
                .get(self.doc)
                .and_then(|document| {
                    document
                        .diagrams
                        .iter()
                        .find(|diagram| Some(diagram.id.selector()) == representation_diagram)
                })
                && representations.iter().all(|id| {
                    diagram.shapes.iter().any(|shape| shape.id == *id)
                        || diagram.edges.iter().any(|edge| edge.id == *id)
                })
            {
                self.representation_selection = representations;
                self.representation_diagram = representation_diagram;
            }
        }
        self.multi = ids;
        self.ctl.session.set_selection(keys);
    }
    fn ordinary_property_locked(&self) -> bool {
        // Intentionally narrower than `any`: the expression and activity-reference drafts have
        // inline editors that do not replace the ordinary property fields.
        self.drafts.locks_ordinary_properties()
    }
    fn has_drafts(&self) -> bool {
        self.collaboration.has_draft()
            || self.attributes.has_draft()
            || self.drafts.any()
            || self.review.has_draft()
            || self.draft_dirty
            || self.dictionary_draft_changed(&self.ctl.session.snapshot().project)
    }
    fn transition_blocked(&self) -> bool {
        #[cfg(windows)]
        if self
            .attachment
            .as_ref()
            .is_some_and(|a| a.has_pending_effect())
        {
            return true;
        }
        !self.ctl.can_transition()
            || self.effects.busy()
            || self.export_ready.is_some()
            || self.export_uncertain.is_some()
            || self.export_lost.is_some()
    }
    fn can_restore_recovery(&self) -> bool {
        !self.transition_blocked()
            && self.after_save.is_none()
            && self.confirm.is_none()
            && !self.has_drafts()
            && self.ctl.session.snapshot().generation == 0
    }
    fn take_ready_transition(&mut self) -> Option<(PendingAction, bool)> {
        if self.transition_blocked() {
            return None;
        }
        let success = self.after_save_ready.take()?;
        let action = self.after_save.take()?;
        Some((
            action,
            success && !self.ctl.session.snapshot().dirty && !self.has_drafts(),
        ))
    }
    fn new_or_open(&mut self, action: PendingAction) {
        if self.transition_blocked() || self.after_save.is_some() {
            self.ctl.error = "Finish the current operation before continuing.".into();
            return;
        }
        if self.ctl.session.snapshot().dirty || self.has_drafts() {
            self.confirm = Some(action);
        } else {
            self.execute(action);
        }
    }
    fn discard_transition(&mut self) -> Option<PendingAction> {
        if self.transition_blocked() {
            self.ctl.error = "Finish the current operation before continuing.".into();
            return None;
        }
        self.confirm.take()
    }
    fn discard_property_drafts(&mut self) {
        self.attributes.reset();
        self.drafts.clear();
        self.definitions_dirty = false;
        self.docs_dirty = false;
        self.draft_dirty = false;
    }
    fn execute(&mut self, action: PendingAction) {
        if self.transition_blocked() {
            self.ctl.error = "Finish the current operation before continuing.".into();
            self.confirm = Some(action);
            return;
        }
        #[cfg(windows)]
        if action == PendingAction::New {
            self.attachment = None;
        }
        match action {
            PendingAction::New => {
                self.ctl = Controller::default();
                self.discard_property_drafts();
                self.dictionary_selected = None;
                self.dictionary_name.clear();
                self.dictionary_description.clear();
                self.doc = 0;
                self.process_id = None;
                self.diagram_id = None;
                self.select(None);
            }
            PendingAction::Open => {
                let path = self.pending_open.take().or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("ProcessCraft", &["processcraft", "pc"])
                        .pick_file()
                });
                self.open_selected_path(path);
            }
            PendingAction::Close => {}
        }
        self.render_key.clear();
    }
    /// BPMN XML is imported as a new model; anything else is opened as a native project.
    fn open_startup_path(&mut self, path: std::path::PathBuf) {
        let is_bpmn = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bpmn") || e.eq_ignore_ascii_case("xml"));
        if is_bpmn {
            self.effects.import(
                self.repaint.clone(),
                path,
                self.ctl.session.id(),
                self.ctl.session.snapshot().generation,
            );
        } else {
            self.open_selected_path(Some(path));
        }
    }
    fn open_selected_path(&mut self, path: Option<std::path::PathBuf>) {
        let Some(path) = path else { return };
        if self.transition_blocked() {
            self.ctl.error = "Finish the current operation before opening.".into();
            return;
        }
        // Read and parse off the UI thread; the result is adopted in `poll_effects`.
        self.effects.open(
            self.repaint.clone(),
            path,
            self.ctl.session.id(),
            self.ctl.session.snapshot().generation,
        );
    }
    fn adopt_opened(&mut self, opened: crate::controller::OpenedProject) {
        #[cfg(windows)]
        {
            self.attachment = None;
        }
        let path = opened.file.target().to_path_buf();
        self.ctl.adopt(opened);
        self.remember_recent(&path);
        self.discard_property_drafts();
        self.dictionary_selected = None;
        self.dictionary_name.clear();
        self.dictionary_description.clear();
        self.doc = 0;
        self.process_id = None;
        self.diagram_id = None;
        self.select(None);
        self.render_key.clear();
    }
    fn save(&mut self, force_as: bool) -> bool {
        if self.collaboration.has_draft() {
            self.ctl.error = "Apply or discard the collaboration draft before saving.".into();
            return false;
        }
        if self.review.has_draft() {
            self.ctl.error = "Post or discard the comment draft before saving.".into();
            return false;
        }
        if self.ctl.is_saving() || self.ctl.uncertain {
            self.ctl.error = "Wait for the current save to settle.".into();
            return false;
        }
        if !self.commit_draft() || !self.commit_dictionary_draft() {
            return false;
        }
        if force_as
            || self.ctl.file.is_none()
            || self.ctl.migration
            || self.ctl.file.as_ref().is_some_and(|f| !f.is_writable())
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ProcessCraft", &["processcraft"])
                .set_file_name(format!(
                    "{}.processcraft",
                    file_stem(&self.ctl.session.snapshot().project.name)
                ))
                .save_file()
            {
                self.ctl.save(Some(path))
            } else {
                false
            }
        } else {
            self.ctl.save(None)
        }
    }
    #[cfg(test)]
    fn add(&mut self, kind: NodeKind) {
        self.add_entry(kind.into());
    }
    /// New element identity; a picked trigger needs a named one to address the new node.
    fn new_identity(&self, named: bool) -> processcraft_core::ids::ElementIdentity {
        if self.create_anonymous && !named {
            processcraft_core::ids::ElementIdentity::anonymous()
        } else {
            controller::id("Node").into()
        }
    }
    /// Palette click: append after a selected flow node, otherwise place in free space.
    fn add_entry(&mut self, entry: canvas::PaletteEntry) {
        use processcraft_core::modeling;
        let kind = entry.kind;
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let (Some(p), Some(diagram)) = (
            processcraft_core::queries::scopes(d)
                .into_iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id)),
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id)),
        ) else {
            return;
        };
        let selected = self.selected.as_ref().and_then(|id| {
            let node = p.nodes.iter().find(|n| &n.id == id)?;
            let shape = diagram.shapes.iter().find(|s| &s.element == id)?;
            Some((node, shape))
        });
        // bpmn.io-style append: a flow node added while one flow node is selected follows it.
        if let Some((source, _)) = selected
            && self.multi.len() == 1
            && processcraft_core::rules::is_flow_node(kind)
            && !matches!(kind, NodeKind::StartEvent | NodeKind::BoundaryEvent)
            && processcraft_core::rules::sequence_source_refusal(source).is_none()
        {
            self.append_entry(source.id.selector(), entry);
            return;
        }
        let host = selected.filter(|(node, _)| node.kind.is_activity());
        if kind == NodeKind::BoundaryEvent && host.is_none() {
            self.ctl.error =
                "Select an activity first, then add a boundary event to attach it.".into();
            return;
        }
        let (width, height) = modeling::default_size(kind);
        let bounds = if let Some((_, activity)) = host.filter(|_| kind == NodeKind::BoundaryEvent) {
            modeling::boundary_bounds(&activity.bounds)
        } else if let Some(parent) = diagram
            .shapes
            .iter()
            .find(|s| &s.element == p.id && s.is_expanded == Some(true))
        {
            Bounds {
                x: parent.bounds.x + 40.0 + (p.nodes.len() % 2) as f64 * 140.0,
                y: parent.bounds.y + 55.0 + (p.nodes.len() / 2) as f64 * 90.0,
                width,
                height,
            }
        } else {
            canvas::vacant_bounds(&diagram.shapes, self.viewport_center(), width, height)
        };
        let eid = self.new_identity(entry.trigger.is_some());
        let mut node = modeling::new_node(
            kind,
            eid.clone(),
            (kind != NodeKind::TextAnnotation).then(|| node_kind_label(kind).into()),
            host.map(|(activity, _)| activity.id.selector()),
        );
        canvas::apply_trigger(&mut node, entry.trigger);
        let backing = modeling::backing_data_object(d.id, &p.id.selector(), &node);
        if self.ctl.mutate(
            backing
                .into_iter()
                .chain([
                    Mutation::CreateElement {
                        document_id: d.id,
                        process_id: p.id.selector(),
                        element: node,
                    },
                    Mutation::CreateShape {
                        document_id: d.id,
                        diagram_id: diagram.id.selector(),
                        shape: modeling::new_shape(
                            kind,
                            controller::id("Shape").into(),
                            eid.selector(),
                            bounds,
                        ),
                    },
                ])
                .collect(),
        ) {
            self.select(Some(eid.selector()));
        }
    }
    /// Append `kind` after `source` through the core append builder and select it.
    fn append(&mut self, source: ElementSelector, kind: NodeKind) -> bool {
        self.append_entry(source, kind.into())
    }
    fn append_entry(&mut self, source: ElementSelector, entry: canvas::PaletteEntry) -> bool {
        let kind = entry.kind;
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return false;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return false;
        };
        let node = self.new_identity(entry.trigger.is_some());
        let ids = processcraft_core::modeling::AppendIds {
            node: node.clone(),
            shape: controller::id("Shape").into(),
            flow: controller::id("Flow").into(),
            edge: controller::id("Edge").into(),
        };
        let name = (kind != NodeKind::TextAnnotation).then(|| node_kind_label(kind).into());
        // Preflight for the refusal reason; the mutation adds auto-resize of the container.
        match processcraft_core::modeling::append(
            d,
            &diagram.id.selector(),
            &source,
            kind,
            name.clone(),
            ids.clone(),
        ) {
            Ok(batch) => {
                let mut mutations = vec![Mutation::Append {
                    document_id: d.id,
                    diagram_id: diagram.id.selector(),
                    source_id: source,
                    kind,
                    name,
                    node_id: ids.node,
                    shape_id: ids.shape,
                    flow_id: ids.flow,
                    edge_id: ids.edge,
                }];
                mutations.extend(canvas::trigger_mutation(
                    &batch,
                    &node.selector(),
                    entry.trigger,
                ));
                let done = self.ctl.mutate(mutations);
                if done {
                    self.select(Some(node.selector()));
                }
                done
            }
            Err(reason) => {
                self.ctl.error = reason;
                false
            }
        }
    }
    fn add_pool(&mut self) {
        if !self.prepare_navigation() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        // Core wraps a process diagram's content, or places an empty pool at the viewport centre.
        let (x, y) = self.viewport_center();
        let batch = vec![Mutation::CreatePool {
            document_id: d.id,
            diagram_id: diagram.id.selector(),
            name: Some("Pool".into()),
            center: Some(processcraft_core::model::Point { x, y }),
        }];
        self.ctl.mutate_at_generation(snap.generation, batch);
    }
    fn add_lane(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let (Some(p), Some(diagram)) = (
            processcraft_core::queries::scopes(d)
                .into_iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id)),
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id)),
        ) else {
            return;
        };
        let lane = controller::id("Lane");
        let y = self.camera.1 + 80.0 + p.lanes.len() as f64 * 160.0;
        self.ctl.mutate(vec![
            Mutation::CreateLane {
                document_id: d.id,
                process_id: p.id.selector(),
                lane: Lane {
                    base: None,
                    child_lane_set_base: None,
                    child_lane_set_name: None,
                    partition_element: None,
                    partition_element_ref: None,
                    child_lane_set_id: None,
                    id: lane.clone().into(),
                    name: Some("Lane".into()),
                    parent: None,
                    members: vec![],
                },
            },
            Mutation::CreateShape {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                shape: Shape {
                    di_metadata: None,
                    is_message_visible: None,
                    participant_band_kind: None,
                    choreography_activity_shape: None,
                    label_visibility: Some(DiLabelVisibility::Explicit),
                    is_horizontal: None,
                    is_marker_visible: None,
                    is_expanded: None,
                    id: controller::id("LaneShape").into(),
                    element: lane.into(),
                    bounds: Bounds {
                        x: self.camera.0 + 30.0,
                        y,
                        width: 600.0,
                        height: 150.0,
                    },
                    label: Some(Label::default()),
                    style: None,
                },
            },
        ]);
    }
    fn assign_lane(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(p) = processcraft_core::queries::scopes(d)
            .into_iter()
            .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        else {
            return;
        };
        let Some(element) = self.selected.clone() else {
            return;
        };
        if !p.nodes.iter().any(|n| n.id == element) {
            return;
        }
        let lane_id = self
            .lane
            .and_then(|i| p.lanes.get(i))
            .map(|l| l.id.selector());
        let bounds = d
            .diagrams
            .iter()
            .flat_map(|diagram| {
                diagram
                    .shapes
                    .iter()
                    .filter(|s| s.element == element)
                    .map(move |s| processcraft_core::commands::ShapeBounds {
                        diagram_id: diagram.id.selector(),
                        shape_id: s.id.selector(),
                        bounds: s.bounds.clone(),
                    })
            })
            .collect();
        self.ctl.mutate(vec![Mutation::AssignLane {
            document_id: d.id,
            process_id: p.id.selector(),
            element_id: element,
            lane_id,
            bounds,
        }]);
    }
    fn move_shapes(&mut self, items: Vec<(ElementSelector, Bounds)>, dx: f64, dy: f64) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let bounds = items
            .into_iter()
            .map(|(shape_id, mut bounds)| {
                bounds.x += dx;
                bounds.y += dy;
                processcraft_core::commands::ShapeBounds {
                    diagram_id: diagram.id.selector(),
                    shape_id,
                    bounds,
                }
            })
            .collect();
        match processcraft_core::geometry::prepare_move(d, &diagram.id.selector(), bounds) {
            Ok(prepared) => {
                if self.ctl.apply(prepared.request) && !prepared.warnings.is_empty() {
                    self.ctl.notify(prepared.warnings.join("; "));
                }
            }
            Err(e) => self.ctl.error = format!("Could not move selection: {e}"),
        }
    }
    /// A canvas drag: the core move with its behaviors (attach/detach, drop on flow, lanes).
    fn drag_move(&mut self, items: Vec<(ElementSelector, Bounds)>, dx: f64, dy: f64) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let shapes: Vec<_> = items
            .into_iter()
            .map(|(shape_id, mut bounds)| {
                bounds.x += dx;
                bounds.y += dy;
                processcraft_core::commands::ShapeBounds {
                    diagram_id: diagram.id.selector(),
                    shape_id,
                    bounds,
                }
            })
            .collect();
        // Route-repair warnings of the plain move, reported as a notice once the move applies.
        let warnings =
            processcraft_core::geometry::prepare_move(d, &diagram.id.selector(), shapes.clone())
                .map(|prepared| prepared.warnings)
                .unwrap_or_default();
        if self.ctl.mutate(vec![Mutation::MoveElements {
            document_id: d.id,
            diagram_id: diagram.id.selector(),
            shapes,
            flow_id: Some(controller::id("Flow").into()),
            edge_id: Some(controller::id("Edge").into()),
        }]) && !warnings.is_empty()
        {
            self.ctl.notify(warnings.join("; "));
        }
    }
    fn move_selected(&mut self, dx: f64, dy: f64) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let selected = if self.representation_diagram.as_ref() == Some(&diagram.id.selector())
            && !self.representation_selection.is_empty()
        {
            self.representation_selection.clone()
        } else {
            let mut selected = Vec::new();
            for semantic in &self.multi {
                let matches: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|s| &s.element == semantic)
                    .collect();
                if matches.len() > 1 {
                    self.ctl.error =
                        "Select a specific representation on the diagram before moving it.".into();
                    return;
                }
                if let Some(shape) = matches.first() {
                    selected.push(shape.id.selector());
                }
            }
            selected
        };
        let items = diagram
            .shapes
            .iter()
            .filter(|s| selected.contains(&s.id.selector()))
            .map(|s| (s.id.selector(), s.bounds.clone()))
            .collect();
        self.move_shapes(items, dx, dy);
    }
    fn reroute_selected(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let Some(edge) = diagram
            .edges
            .iter()
            .find(|e| self.selected.as_ref() == Some(&e.element))
        else {
            return;
        };
        let (Some(a), Some(b)) = processcraft_core::modeling::edge_ends(d, diagram, edge) else {
            return;
        };
        match processcraft_core::modeling::route_between(d, diagram, &a.element, &b.element) {
            Ok(waypoints) => {
                self.ctl.mutate(vec![Mutation::Reroute {
                    document_id: d.id,
                    diagram_id: diagram.id.selector(),
                    edge_id: edge.id.selector(),
                    waypoints,
                }]);
            }
            Err(reason) => self.ctl.error = reason,
        }
    }
    fn connect(&mut self) {
        let (Some(a), Some(b)) = (self.source.clone(), self.target.clone()) else {
            self.ctl.error = "Choose a source and a target.".into();
            return;
        };
        self.connect_elements(a, b);
    }
    /// Connect two shown elements; the core rules pick sequence, message, data association,
    /// or association, and a refusal reports its reason.
    fn connect_elements(&mut self, a: ElementSelector, b: ElementSelector) -> bool {
        let snap = self.ctl.session.snapshot();
        let Some(d) = self.current_document(&snap) else {
            return false;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return false;
        };
        match processcraft_core::modeling::connect(
            d,
            &diagram.id.selector(),
            &a,
            &b,
            controller::id("Flow").into(),
            controller::id("Edge").into(),
        ) {
            Ok(batch) => self.ctl.mutate(batch),
            Err(reason) => {
                self.ctl.error = reason;
                false
            }
        }
    }
    fn delete(&mut self) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        if self
            .graph_selection
            .as_ref()
            .is_some_and(|(_, _, generation)| *generation != snap.generation)
        {
            self.ctl.error =
                "The model changed; select the choreography element again before deleting.".into();
            return;
        }
        let Some(d) = self.current_document(&snap) else {
            return;
        };
        let mut ids = self.multi.clone();
        ids.sort_by_key(|id| {
            if processcraft_core::queries::find_flow(d, id).is_some() || processcraft_core::queries::choreography_scopes(d).iter().any(|scope| scope.flows.iter().any(|flow| flow.id == *id) || scope.artifacts.iter().any(|artifact| matches!(artifact, processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(flow) if flow.id == *id))) {
                0
            } else {
                1
            }
        });
        if ids.is_empty() {
            return;
        }
        let batch = ids
            .into_iter()
            .map(|element_id| {
                for scope in processcraft_core::queries::choreography_scopes(d) {
                    if scope.root.graph.is_none() { continue; }
                    if scope.ordered.iter().any(|element| match element { processcraft_core::queries::ChoreographyElementRef::Activity(value) => value.id == element_id, processcraft_core::queries::ChoreographyElementRef::Event(value) | processcraft_core::queries::ChoreographyElementRef::Gateway(value) => value.id == element_id, processcraft_core::queries::ChoreographyElementRef::SequenceFlow(value) => value.id == element_id }) {
                        return Mutation::DeleteChoreographyFlowElement { document_id: d.id, choreography_id: scope.root.id.selector(), scope_id: scope.owner, element_id };
                    }
                    if scope.artifacts.iter().any(|artifact| match artifact { processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(value) => value.id == element_id, processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(value) => value.id == element_id, processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(value) => value.id == element_id }) {
                        return Mutation::DeleteChoreographyArtifact { document_id: d.id, choreography_id: scope.root.id.selector(), scope_id: scope.owner, artifact_id: element_id };
                    }
                }
                Mutation::Delete { document_id: d.id, element_id }
            })
            .collect();
        if self.ctl.mutate_at_generation(snap.generation, batch) {
            self.select(None);
        }
    }
}
/// Centered modal dialog; returns the content result and whether Escape (or a click on the
/// backdrop) asked to close it.
fn modal<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let response = egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(480.0);
        ui.heading(title);
        ui.add_space(6.0);
        add(ui)
    });
    let close = response.should_close();
    (response.inner, close)
}
impl ProcessCraft {
    fn dialogs_ui(&mut self, ctx: &egui::Context) {
        if let Some(lost) = &self.export_lost {
            let details = format!(
                "Target: {} Â· session: {} Â· operation: {} Â· SHA-256: {} Â· {} bytes",
                lost.path.display(),
                lost.session,
                lost.operation,
                lost.digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                lost.bytes.len()
            );
            let (close, _) = modal(ctx, "export_lost", "Export outcome unknown", |ui| {
                ui.label("The export worker stopped and the write outcome cannot be established automatically. Inspect the destination before exporting there again.");
                ui.label(&details);
                ui.button("Close").clicked()
            });
            if close {
                self.export_lost = None;
                self.ctl.error = format!("Export outcome unknown. {details}");
            }
        }
        if let Some(pending) = &self.export_uncertain {
            let message = format!(
                "{}; destination: {}; stage: {}; backup: {}",
                pending.error,
                pending.path.display(),
                pending.stage.display(),
                pending.backup.display()
            );
            // Not dismissable: dropping it would lose the only handle able to settle the write.
            let (check, _) = modal(ctx, "export_uncertain", "Export outcome uncertain", |ui| {
                ui.label(message);
                ui.label("Inspect the destination and retained files, then check the outcome.");
                ui.button("Check export outcome").clicked()
            });
            if check && let Some(pending) = self.export_uncertain.take() {
                self.effects.reconcile(ctx.clone(), pending);
            }
        }
        if let Some((_, report, extension)) = &self.export_ready {
            let report = report.clone();
            let extension = *extension;
            let ((cancel, accept), close) = modal(ctx, "export_ready", "Review export", |ui| {
                if report.is_empty() {
                    ui.label("Export is ready.");
                } else {
                    ui.label("Review and accept these export notes:");
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for note in &report {
                                ui.label(note);
                            }
                        });
                }
                ui.horizontal(|ui| {
                    (
                        ui.button("Cancel").clicked(),
                        ui.button("Accept and choose destinationâ€¦").clicked(),
                    )
                })
                .inner
            });
            if cancel || close {
                self.export_ready = None;
            } else if accept
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter(extension, &[extension])
                    .set_file_name(format!("diagram.{extension}"))
                    .save_file()
            {
                if self.ctl.is_saving() {
                    self.ctl.error = "Wait for the project save to finish before exporting.".into();
                } else {
                    match processcraft_io::FileGrant::new(&path, processcraft_io::Access::Write)
                        .and_then(|grant| {
                            if self
                                .ctl
                                .file
                                .as_ref()
                                .map(|file| file.same_target(&grant))
                                .transpose()?
                                .unwrap_or(false)
                            {
                                return Err(std::io::Error::other(
                                    "Choose a destination other than the open project file.",
                                ));
                            }
                            Ok(grant)
                        }) {
                        Ok(grant) => {
                            if let Some((bytes, _, _)) = self.export_ready.take() {
                                self.effects.write(
                                    ctx.clone(),
                                    grant,
                                    bytes,
                                    self.ctl.session.id(),
                                );
                            }
                        }
                        Err(error) => {
                            self.ctl.error = format!("Cannot use export destination: {error}")
                        }
                    }
                }
            }
        }
        if self.recovery_visible && !self.recovery_records.is_empty() {
            let records = self.recovery_records.clone();
            let ((restore, discard, later), close) =
                modal(ctx, "recovery", "Recover unsaved work", |ui| {
                    ui.label(
                        "Recovered work opens as an unsaved project. Choose Save As to keep it.",
                    );
                    let mut restore = None;
                    let mut discard = None;
                    for record in &records {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} Â· {} Â· revision {}",
                                record
                                    .origin
                                    .original_path
                                    .as_ref()
                                    .and_then(|p| p.file_name())
                                    .map_or("Unsaved project".into(), |n| n
                                        .to_string_lossy()
                                        .into_owned()),
                                recovery_age(record.written_unix_ms),
                                record.generation
                            ));
                            if ui.button("Restore").clicked() {
                                restore = Some(record.clone());
                            }
                            if ui.button("Discard recovery").clicked() {
                                discard = Some(record.clone());
                            }
                        });
                    }
                    (restore, discard, ui.button("Later").clicked())
                });
            if later || close {
                self.recovery_visible = false;
            }
            if let Some(record) = restore
                && self.can_restore_recovery()
            {
                match processcraft_io::recovery::RecoveryStore::new(crate::jobs::recovery_location())
                    .and_then(|s| s.open(&record))
                {
                    Ok(project) => match processcraft_core::commands::Session::new(project) {
                        Ok(mut session) => {
                            session.mark_unsaved();
                            #[cfg(windows)]
                            {
                                self.attachment = None;
                            }
                            self.ctl = Controller::default();
                            self.ctl.session = session;
                            self.doc = 0;
                            self.process_id = None;
                            self.diagram_id = None;
                            self.discard_property_drafts();
                            self.select(None);
                            self.render_key.clear();
                            self.recovery_visible = false;
                        }
                        Err(e) => {
                            self.ctl.error = format!(
                                "Cannot recover project: {}",
                                controller::command_error_text(&e)
                            )
                        }
                    },
                    Err(e) => self.ctl.error = format!("Cannot recover project: {e}"),
                }
            }
            if let Some(record) = discard {
                match processcraft_io::recovery::RecoveryStore::new(crate::jobs::recovery_location())
                    .and_then(|s| s.discard(&record))
                {
                    Ok(()) => self.recovery_records.retain(|r| r.id != record.id),
                    Err(e) => self.ctl.error = format!("Cannot discard recovery: {e}"),
                }
            }
        }
        if let Some(action) = self.confirm {
            let ((save, discard, cancel), close) = modal(ctx, "unsaved", "Unsaved changes", |ui| {
                ui.label("Save changes before continuing?");
                ui.horizontal(|ui| {
                    (
                        ui.button("Save").clicked(),
                        ui.button("Discard").clicked(),
                        ui.button("Cancel").clicked(),
                    )
                })
                .inner
            });
            if save && self.save(false) {
                self.confirm = None;
                self.after_save = Some(action);
            } else if discard && let Some(action) = self.discard_transition() {
                if action == PendingAction::Close {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.execute(action);
                }
            } else if cancel || close {
                self.confirm = None;
            }
        }
    }
}
impl eframe::App for ProcessCraft {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.dark = self.theme == Theme::Dark;
        self.settings.left_width = Some(self.left_width);
        self.settings.right_width = Some(self.right_width);
        self.settings.show_left = Some(self.show_left);
        self.settings.show_right = Some(self.show_right);
        self.settings.save();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        let ctx = ui.ctx().clone();
        self.repaint = ctx.clone();
        if let Some(path) = self.startup_path.take() {
            self.open_startup_path(path);
        }
        #[cfg(windows)]
        if self.attachment.is_some() {
            {
                let accept_requests =
                    !self.has_drafts() && self.after_save.is_none() && self.confirm.is_none();
                let previous = self.ctl.session.snapshot();
                let before = previous.generation;
                let document_id = previous.project.documents.get(self.doc).map(|d| d.id);
                if let Some(attachment) = &mut self.attachment
                    && !attachment.poll(&mut self.ctl, accept_requests)
                {
                    self.attachment = None;
                }
                if before != self.ctl.session.snapshot().generation {
                    let current = self.ctl.session.snapshot();
                    if let Some(index) = current
                        .project
                        .documents
                        .iter()
                        .position(|d| Some(d.id) == document_id)
                    {
                        self.doc = index;
                    } else {
                        self.doc = 0;
                        self.process_id = None;
                        self.diagram_id = None;
                    }
                    if accept_requests {
                        self.restore_selection();
                        self.refresh_dictionary_editor();
                    }
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(result) = self.effects.poll() {
            match result {
                crate::effects::ResultValue::Import {
                    session,
                    generation,
                    prepared,
                } => {
                    let mut prepared = *prepared;
                    if session != self.ctl.session.id()
                        || generation != self.ctl.session.snapshot().generation
                        || self.has_drafts()
                    {
                        self.ctl.error = "Import cancelled: the model was edited (or an edit was \
                            left unapplied) while the BPMN file was being prepared. Apply or \
                            discard pending edits, then import again."
                            .into();
                    } else {
                        let id = prepared.document.id;
                        let plan = processcraft_io::interchange::plan_attribute_import(
                            &self.ctl.session.snapshot().project,
                            &mut prepared.document,
                            &prepared.attributes,
                        );
                        let mut mutations: Vec<_> = prepared
                            .resources
                            .into_iter()
                            .map(|resource| Mutation::AddResource { resource })
                            .collect();
                        mutations.push(Mutation::CreateDocument {
                            document: prepared.document,
                        });
                        mutations.extend(plan.mutations);
                        if self.ctl.mutate(mutations) {
                            let snapshot = self.ctl.session.snapshot();
                            if let Some(index) =
                                snapshot.project.documents.iter().position(|d| d.id == id)
                            {
                                self.doc = index;
                                self.process_id = None;
                                self.diagram_id = None;
                                self.select(None);
                            }
                            self.ctl.error = prepared
                                .report
                                .limitations
                                .into_iter()
                                .chain(plan.notes)
                                .collect::<Vec<_>>()
                                .join("; ");
                        }
                    }
                }
                crate::effects::ResultValue::Export {
                    bytes,
                    report,
                    extension,
                } => self.export_ready = Some((bytes, report, extension)),
                crate::effects::ResultValue::Opened {
                    session,
                    generation,
                    opened,
                } => {
                    if session != self.ctl.session.id()
                        || generation != self.ctl.session.snapshot().generation
                    {
                        self.ctl.error = "Open cancelled: the current project was edited while \
                            the file was loading. Open it again to replace the project."
                            .into();
                    } else {
                        self.adopt_opened(*opened);
                    }
                }
                crate::effects::ResultValue::Written(message) => self.ctl.notify(message),
                crate::effects::ResultValue::Failed(message) => self.ctl.error = message,
                crate::effects::ResultValue::Indeterminate(message) => {
                    self.export_uncertain = Some(message)
                }
                crate::effects::ResultValue::LostWrite(context) => self.export_lost = Some(context),
            }
        }
        if self.effects.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        #[cfg(windows)]
        if self
            .attachment
            .as_ref()
            .is_some_and(|a| a.has_pending_effect())
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ctl.error = "Wait for the attachment operation to finish.".into();
        }
        if (self.effects.busy()
            || self.export_ready.is_some()
            || self.export_uncertain.is_some()
            || self.export_lost.is_some())
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ctl.error = "Wait for the interchange operation to finish.".into();
        }
        let snapshot = self.ctl.session.snapshot();
        // Validation runs off the UI thread and only while Problems is visible.
        let problem_key = (self.ctl.session.id(), snapshot.generation);
        if let Some((key, problems)) = self.diagnostics.poll()
            && self.problem_key.as_ref() == Some(&key)
        {
            self.problems = problems;
        }
        if self.shell.bottom.is_some() && self.problem_key.as_ref() != Some(&problem_key) {
            self.diagnostics
                .submit(problem_key.clone(), snapshot.project.clone(), ctx.clone());
            self.problem_key = Some(problem_key);
        }
        let now = std::time::Instant::now();
        let recovery_key = (self.ctl.session.id(), snapshot.generation);
        if snapshot.dirty && self.recovery_queued.as_ref() != Some(&recovery_key) {
            if self.dirty_since.is_none() {
                self.dirty_since = Some(now);
            }
            if self.recovery_generation.as_ref() != Some(&recovery_key) {
                self.changed_at = now;
                self.recovery_generation = Some(recovery_key.clone());
            }
            if now.duration_since(self.changed_at) >= std::time::Duration::from_secs(2)
                || self
                    .dirty_since
                    .is_some_and(|t| now.duration_since(t) >= std::time::Duration::from_secs(30))
            {
                self.recovery.submit(crate::jobs::RecoveryJob {
                    project: snapshot.project.clone(),
                    origin: self.ctl.recovery_origin(),
                    generation: snapshot.generation,
                    context: ctx.clone(),
                });
                self.dirty_since = None;
                self.recovery_queued = Some(recovery_key);
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        } else {
            self.dirty_since = None;
        }

        if let Some(error) = self.recovery.error() {
            self.recovery_queued = None;
            self.changed_at = now;
            self.ctl.error = format!("Recovery snapshot failed: {error}");
        }
        if self.ctl.is_saving() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        #[cfg(windows)]
        let attachment_save_pending = self
            .attachment
            .as_ref()
            .is_some_and(|a| a.has_pending_save());
        #[cfg(not(windows))]
        let attachment_save_pending = false;
        if !attachment_save_pending && let Some(success) = self.ctl.poll_save() {
            if success && let Some(path) = self.ctl.file.as_ref().map(|f| f.target().to_path_buf())
            {
                self.remember_recent(&path);
            }
            if self.after_save.is_some() {
                self.after_save_ready = Some(success);
            }
        }
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.settings.window = Some([rect.width(), rect.height()]);
        }
        if let Some((action, proceed)) = self.take_ready_transition() {
            if proceed {
                if action == PendingAction::Close {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.execute(action);
                }
            } else {
                self.confirm = Some(action);
            }
        }
        if !self.allow_close
            && ctx.input(|i| i.viewport().close_requested())
            && !self.ctl.can_transition()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ctl.error = "Resolve the uncertain save before closing.".into();
        }
        if !self.allow_close
            && self.ctl.can_transition()
            && ctx.input(|i| i.viewport().close_requested())
            && (self.ctl.session.snapshot().dirty || self.has_drafts())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(PendingAction::Close);
        }
        self.canvas_shortcuts(&ctx);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.new_or_open(PendingAction::Open);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.new_or_open(PendingAction::New);
        }
        self.workbench_ui(ui);
        self.dialogs_ui(&ctx);
    }
}

mod analyst_views;
mod attributes;
mod band_create;
mod base_metadata;
mod call;
mod callable;
mod callable_io;
mod canvas;
mod catalog;
mod choreography_flow;
mod collaboration;
mod collaboration_artifacts;
mod collaboration_associations;
mod collaboration_connection;
mod collaboration_participant;
mod collaboration_root;
mod conventions;
mod conversation_details;
mod correlation;
mod correlation_keys;
mod data;
mod definitions_extra;
mod di;
mod di_metadata;
mod di_style;
mod documentation;
mod drafts;
mod event;
mod flow_metadata;
mod graph_data;
mod inspector;
mod loop_advanced;
mod mixed_body;
mod overlays;
mod participant_di;
mod partners;
mod process_activity;
mod review;
mod roles;
mod shell;
#[cfg(test)]
mod tests;
mod tree;
mod xml;
