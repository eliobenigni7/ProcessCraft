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
/// New-connection route from the shared core router, avoiding the diagram's other shapes.
fn connection_route(
    a: &processcraft_core::model::Bounds,
    b: &processcraft_core::model::Bounds,
    diagram: &Diagram,
) -> Vec<processcraft_core::model::Point> {
    let shapes: Vec<_> = diagram.shapes.iter().map(|s| s.bounds.clone()).collect();
    processcraft_core::geometry::connection_route(a, b, &shapes)
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
}
impl DragGesture {
    fn new(items: Vec<(ElementSelector, Bounds)>, origin: Pos2) -> Self {
        Self {
            items,
            origin,
            offset: Vec2::ZERO,
            context: None,
            label: false,
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
    m3: m3::State,
    collaboration: collaboration::State,
    shell: shell::ShellState,
    doc: usize,
    process_id: Option<ElementSelector>,
    diagram_id: Option<ElementSelector>,
    view: u8,
    library_filter: String,
    dictionary_filter: String,
    dictionary_kind_filter: Option<DictionaryKind>,
    folder_name: String,
    folder_selected: Option<XmlId>,
    dictionary_selected: Option<processcraft_core::ids::DictionaryEntryId>,
    dictionary_name: String,
    dictionary_description: String,
    dictionary_kind: DictionaryKind,
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
    expression_draft: Option<inspector::ExpressionDraft>,
    catalog_draft: Option<catalog::CatalogDraft>,
    binding_draft: Option<inspector::BindingDraft>,
    data_draft: Option<data::DataDraft>,
    graph_data_draft: Option<graph_data::GraphDataDraft>,
    xml_id_draft: Option<inspector::XmlIdDraft>,
    di_draft: Option<di::DiDraft>,
    documentation_draft: Option<documentation::DocumentationDraft>,
    body_draft: Option<mixed_body::BodyDraft>,
    metadata_draft: Option<base_metadata::MetadataDraft>,
    style_draft: Option<di_style::StyleDraft>,
    definitions_extra_draft: Option<definitions_extra::DefinitionsExtraDraft>,
    correlation_catalog_draft: Option<correlation::CorrelationCatalogDraft>,
    activity_roles_draft: Option<roles::ActivityRolesDraft>,
    callable_catalog_draft: Option<callable::CallableCatalogDraft>,
    call_draft: Option<call::CallDraft>,
    process_activity_draft: Option<process_activity::ProcessActivityDraft>,
    flow_metadata_draft: Option<flow_metadata::FlowDraft>,
    correlation_keys_draft: Option<correlation_keys::CorrelationKeysDraft>,
    loop_advanced_draft: Option<loop_advanced::LoopAdvancedDraft>,
    choreography_flow_draft: Option<choreography_flow::ChoreographyFlowDraft>,
    multiplicity_draft: Option<participant_di::MultiplicityDraft>,
    message_ref_draft: Option<participant_di::MessageRefDraft>,
    band_draft: Option<band_create::BandDraft>,
    event_draft: Option<event::EventDraft>,
    event_catalog_draft: Option<event::EventCatalogDraft>,
    identity_page: usize,
    association_direction: Option<processcraft_core::bpmn_model::AssociationDirection>,
    connection_kind: FlowKind,
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
    confirm: Option<&'static str>,
    after_save: Option<&'static str>,
    after_save_ready: Option<bool>,
    allow_close: bool,
    draft_dirty: bool,
    source: Option<ElementSelector>,
    target: Option<ElementSelector>,
    lane: usize,
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
            m3: m3::State::default(),
            collaboration: collaboration::State::default(),
            shell: shell::ShellState::default(),
            doc: 0,
            process_id: None,
            diagram_id: None,
            view: 0,
            library_filter: String::new(),
            dictionary_filter: String::new(),
            dictionary_kind_filter: None,
            folder_name: String::new(),
            folder_selected: None,
            dictionary_selected: None,
            dictionary_name: String::new(),
            dictionary_description: String::new(),
            dictionary_kind: DictionaryKind::Role,
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
            expression_draft: None,
            catalog_draft: None,
            binding_draft: None,
            data_draft: None,
            graph_data_draft: None,
            xml_id_draft: None,
            di_draft: None,
            documentation_draft: None,
            body_draft: None,
            metadata_draft: None,
            style_draft: None,
            definitions_extra_draft: None,
            correlation_catalog_draft: None,
            activity_roles_draft: None,
            callable_catalog_draft: None,
            call_draft: None,
            process_activity_draft: None,
            flow_metadata_draft: None,
            correlation_keys_draft: None,
            loop_advanced_draft: None,
            choreography_flow_draft: None,
            multiplicity_draft: None,
            message_ref_draft: None,
            band_draft: None,
            event_draft: None,
            event_catalog_draft: None,
            identity_page: 0,
            association_direction: None,
            connection_kind: FlowKind::Sequence,
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
            lane: 0,
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
        if (self.draft_dirty
            || self.expression_draft.is_some()
            || self.catalog_draft.is_some()
            || self.binding_draft.is_some()
            || self.data_draft.is_some()
            || self.graph_data_draft.is_some()
            || self.xml_id_draft.is_some()
            || self.di_draft.is_some()
            || self.documentation_draft.is_some()
            || self.body_draft.is_some()
            || self.metadata_draft.is_some()
            || self.style_draft.is_some()
            || self.definitions_extra_draft.is_some()
            || self.correlation_catalog_draft.is_some()
            || self.activity_roles_draft.is_some()
            || self.callable_catalog_draft.is_some()
            || self.call_draft.is_some()
            || self.process_activity_draft.is_some()
            || self.flow_metadata_draft.is_some()
            || self.correlation_keys_draft.is_some()
            || self.loop_advanced_draft.is_some()
            || self.choreography_flow_draft.is_some()
            || self.multiplicity_draft.is_some()
            || self.message_ref_draft.is_some()
            || self.band_draft.is_some()
            || self.event_draft.is_some()
            || self.event_catalog_draft.is_some())
            && !self.commit_draft()
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
            if let Some(document) = snap.project.documents.get(self.doc) {
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
        if let Some(d) = snap.project.documents.get(self.doc) {
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
        if let Some(document) = snap.project.documents.get(self.doc)
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
        if let Some(d) = snap.project.documents.get(self.doc)
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
        if let Some(document) = snap.project.documents.get(self.doc)
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
        if let Some(document) = snap.project.documents.get(self.doc)
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
    fn commit_draft(&mut self) -> bool {
        if self.event_draft.is_some() || self.event_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the event draft first.".into();
            return false;
        }
        if self.multiplicity_draft.is_some()
            || self.message_ref_draft.is_some()
            || self.band_draft.is_some()
        {
            self.ctl.error =
                "Apply or cancel the participant, band, or message draft first.".into();
            return false;
        }
        if self.choreography_flow_draft.is_some() {
            self.ctl.error = "Apply or cancel the choreography details draft first.".into();
            return false;
        }
        if self.loop_advanced_draft.is_some() {
            self.ctl.error = "Apply or cancel the multi-instance draft first.".into();
            return false;
        }
        if self.correlation_keys_draft.is_some() {
            self.ctl.error = "Apply or cancel the correlation keys draft first.".into();
            return false;
        }
        if self.flow_metadata_draft.is_some() {
            self.ctl.error = "Apply or cancel the flow details draft first.".into();
            return false;
        }
        if self.process_activity_draft.is_some() {
            self.ctl.error = "Apply or cancel the process behavior draft first.".into();
            return false;
        }
        if self.call_draft.is_some() {
            self.ctl.error = "Apply or cancel the called element draft first.".into();
            return false;
        }
        if self.callable_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the global task draft first.".into();
            return false;
        }
        if self.activity_roles_draft.is_some() {
            self.ctl.error = "Apply or cancel the activity roles draft first.".into();
            return false;
        }
        if self.correlation_catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the correlation draft first.".into();
            return false;
        }
        if self.definitions_extra_draft.is_some() {
            self.ctl.error = "Apply or cancel the definitions details draft first.".into();
            return false;
        }
        if self.style_draft.is_some() {
            self.ctl.error = "Apply or cancel the diagram font draft first.".into();
            return false;
        }
        if self.metadata_draft.is_some() {
            self.ctl.error = "Apply or cancel the advanced metadata draft first.".into();
            return false;
        }
        if self.body_draft.is_some() {
            self.ctl.error = "Apply or cancel the content draft first.".into();
            return false;
        }
        if self.documentation_draft.is_some() {
            self.ctl.error = "Apply or cancel the documentation draft first.".into();
            return false;
        }
        if self.di_draft.is_some() {
            self.ctl.error = "Apply or cancel the diagram properties draft first.".into();
            return false;
        }
        if self.xml_id_draft.is_some() {
            self.ctl.error = "Apply or cancel the XML ID draft first.".into();
            return false;
        }
        if self.data_draft.is_some() {
            self.ctl.error = "Apply or cancel the data draft first.".into();
            return false;
        }
        if self.graph_data_draft.is_some() {
            self.ctl.error =
                "Apply or cancel the choreography data association draft first.".into();
            return false;
        }
        if self.binding_draft.is_some() {
            self.ctl.error = "Apply or cancel the activity reference draft first.".into();
            return false;
        }
        if self.catalog_draft.is_some() {
            self.ctl.error = "Apply or cancel the supporting catalog draft first.".into();
            return false;
        }
        if self.expression_draft.is_some() {
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
            let Some(document) = snapshot.project.documents.get(self.doc) else {
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
            if let Some(d) = snap.project.documents.get(self.doc) {
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
            let Some(d) = snap.project.documents.get(self.doc) else {
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
        if self.m3.has_draft() {
            self.ctl.error = "Post or discard the comment draft before using history.".into();
            return;
        }
        if self.commit_draft()
            && self.commit_dictionary_draft()
            && let Some(changes) = self.ctl.apply_changes(request)
        {
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
        self.catalog_draft.is_some()
            || self.data_draft.is_some()
            || self.graph_data_draft.is_some()
            || self.xml_id_draft.is_some()
            || self.di_draft.is_some()
            || self.documentation_draft.is_some()
            || self.body_draft.is_some()
            || self.metadata_draft.is_some()
            || self.style_draft.is_some()
            || self.definitions_extra_draft.is_some()
            || self.event_draft.is_some()
            || self.event_catalog_draft.is_some()
            || self.correlation_catalog_draft.is_some()
            || self.activity_roles_draft.is_some()
            || self.callable_catalog_draft.is_some()
            || self.call_draft.is_some()
            || self.process_activity_draft.is_some()
            || self.flow_metadata_draft.is_some()
            || self.correlation_keys_draft.is_some()
            || self.loop_advanced_draft.is_some()
            || self.choreography_flow_draft.is_some()
            || self.multiplicity_draft.is_some()
            || self.message_ref_draft.is_some()
            || self.band_draft.is_some()
    }
    fn has_drafts(&self) -> bool {
        self.collaboration.has_draft()
            || self.expression_draft.is_some()
            || self.catalog_draft.is_some()
            || self.binding_draft.is_some()
            || self.data_draft.is_some()
            || self.graph_data_draft.is_some()
            || self.xml_id_draft.is_some()
            || self.di_draft.is_some()
            || self.documentation_draft.is_some()
            || self.body_draft.is_some()
            || self.metadata_draft.is_some()
            || self.style_draft.is_some()
            || self.definitions_extra_draft.is_some()
            || self.correlation_catalog_draft.is_some()
            || self.activity_roles_draft.is_some()
            || self.callable_catalog_draft.is_some()
            || self.call_draft.is_some()
            || self.process_activity_draft.is_some()
            || self.flow_metadata_draft.is_some()
            || self.correlation_keys_draft.is_some()
            || self.loop_advanced_draft.is_some()
            || self.choreography_flow_draft.is_some()
            || self.multiplicity_draft.is_some()
            || self.message_ref_draft.is_some()
            || self.band_draft.is_some()
            || self.event_draft.is_some()
            || self.event_catalog_draft.is_some()
            || self.m3.has_draft()
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
    fn take_ready_transition(&mut self) -> Option<(&'static str, bool)> {
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
    fn new_or_open(&mut self, action: &'static str) {
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
    fn discard_transition(&mut self) -> Option<&'static str> {
        if self.transition_blocked() {
            self.ctl.error = "Finish the current operation before continuing.".into();
            return None;
        }
        self.confirm.take()
    }
    fn discard_property_drafts(&mut self) {
        self.expression_draft = None;
        self.catalog_draft = None;
        self.binding_draft = None;
        self.data_draft = None;
        self.graph_data_draft = None;
        self.xml_id_draft = None;
        self.di_draft = None;
        self.documentation_draft = None;
        self.body_draft = None;
        self.metadata_draft = None;
        self.style_draft = None;
        self.definitions_extra_draft = None;
        self.correlation_catalog_draft = None;
        self.activity_roles_draft = None;
        self.callable_catalog_draft = None;
        self.call_draft = None;
        self.process_activity_draft = None;
        self.flow_metadata_draft = None;
        self.correlation_keys_draft = None;
        self.loop_advanced_draft = None;
        self.choreography_flow_draft = None;
        self.multiplicity_draft = None;
        self.message_ref_draft = None;
        self.band_draft = None;
        self.event_draft = None;
        self.event_catalog_draft = None;
        self.definitions_dirty = false;
        self.docs_dirty = false;
        self.draft_dirty = false;
    }
    fn execute(&mut self, action: &'static str) {
        if self.transition_blocked() {
            self.ctl.error = "Finish the current operation before continuing.".into();
            self.confirm = Some(action);
            return;
        }
        #[cfg(windows)]
        if action == "new" {
            self.attachment = None;
        }
        match action {
            "new" => {
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
            "open" => {
                let path = self.pending_open.take().or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("ProcessCraft", &["processcraft", "pc"])
                        .pick_file()
                });
                self.open_selected_path(path);
            }
            "close" => {}
            _ => {}
        }
        self.render_key.clear();
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
        if self.m3.has_draft() {
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
    fn add(&mut self, kind: NodeKind) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let eid: processcraft_core::ids::ElementIdentity = if self.create_anonymous {
            processcraft_core::ids::ElementIdentity::anonymous()
        } else {
            controller::id("Node").into()
        };
        let event = matches!(
            kind,
            NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        );
        let gateway = matches!(
            kind,
            NodeKind::ExclusiveGateway
                | NodeKind::ParallelGateway
                | NodeKind::InclusiveGateway
                | NodeKind::ComplexGateway
                | NodeKind::EventBasedGateway
        );
        let subprocess = matches!(
            kind,
            NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
        );
        let (width, height) = if event {
            (40.0, 40.0)
        } else if gateway {
            (50.0, 50.0)
        } else if subprocess {
            (360.0, 240.0)
        } else {
            (100.0, 70.0)
        };
        let label = node_kind_label(kind);
        let details = if kind == NodeKind::TextAnnotation {
            Some(processcraft_core::bpmn_model::NodeDetails::TextAnnotation {
                text_format: None,
                text: Some(String::new().into()),
            })
        } else if subprocess {
            Some(processcraft_core::bpmn_model::NodeDetails::SubProcess {
                content: Box::new(FlowContainer::default()),
            })
        } else if matches!(
            kind,
            NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
        ) {
            Some(processcraft_core::bpmn_model::NodeDetails::Event {
                definition_refs: Vec::new(),
                parallel_multiple: None,
                is_interrupting: None,
                properties: Vec::new(),
                io: None,
                definitions: vec![processcraft_core::bpmn_model::EventDefinition::Timer {
                    base: None,
                    id: None,
                    kind: Some(processcraft_core::bpmn_model::TimerKind::Duration),
                    expression: Some("PT1M".into()),
                }],
                attached_to: if kind == NodeKind::BoundaryEvent {
                    self.selected
                        .clone()
                        .map(processcraft_core::bpmn_activity::ModelReference::Local)
                } else {
                    None
                },
                cancel_activity: (kind == NodeKind::BoundaryEvent).then_some(true),
            })
        } else {
            None
        };
        let selected_shape = self.selected.as_ref().and_then(|id| {
            let node = p.nodes.iter().find(|n| &n.id == id)?;
            let shape = diagram.shapes.iter().find(|s| &s.element == id)?;
            Some((node.kind, shape))
        });
        if kind == NodeKind::BoundaryEvent
            && !selected_shape.is_some_and(|(selected, _)| selected.is_activity())
        {
            self.ctl.error =
                "Select an activity first, then add a boundary event to attach it.".into();
            return;
        }
        // bpmn.io-style append: a flow node added while one flow node is selected follows it.
        let flow_target = !matches!(
            kind,
            NodeKind::StartEvent
                | NodeKind::BoundaryEvent
                | NodeKind::TextAnnotation
                | NodeKind::DataObjectReference
                | NodeKind::DataStoreReference
        );
        let append = selected_shape.filter(|(source, _)| {
            flow_target
                && self.multi.len() == 1
                && !matches!(
                    source,
                    NodeKind::EndEvent
                        | NodeKind::TextAnnotation
                        | NodeKind::DataObjectReference
                        | NodeKind::DataStoreReference
                )
        });
        let parent = diagram
            .shapes
            .iter()
            .find(|s| &s.element == p.id && s.is_expanded == Some(true));
        let (x, y) = if kind == NodeKind::BoundaryEvent
            && let Some((_, activity)) = selected_shape
        {
            let a = &activity.bounds;
            (
                a.x + a.width * 0.75 - width / 2.0,
                a.y + a.height - height / 2.0,
            )
        } else if let Some((_, source)) = append {
            let s = &source.bounds;
            let center = (s.x + s.width + 80.0 + width / 2.0, s.y + s.height / 2.0);
            let b = canvas::vacant_bounds(&diagram.shapes, center, width, height);
            (b.x, b.y)
        } else if let Some(parent) = parent {
            (
                parent.bounds.x + 40.0 + (p.nodes.len() % 2) as f64 * 140.0,
                parent.bounds.y + 55.0 + (p.nodes.len() / 2) as f64 * 90.0,
            )
        } else {
            let b = canvas::vacant_bounds(&diagram.shapes, self.viewport_center(), width, height);
            (b.x, b.y)
        };
        let shape = Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: Some(DiLabelVisibility::Explicit),
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: subprocess.then_some(true),
            id: controller::id("Shape").into(),
            element: eid.selector(),
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
            label: if event || gateway {
                Some(Label {
                    di_metadata: None,
                    id: None,
                    style_ref: None,
                    bounds: Some(Bounds {
                        x: x + (width - 140.0) / 2.0,
                        y: y + height + 4.0,
                        width: 140.0,
                        height: 30.0,
                    }),
                    style: None,
                })
            } else {
                Some(Label::default())
            },
            style: None,
        };
        let append_flow = append.map(|(_, source)| {
            let flow = controller::id("Flow");
            [
                Mutation::CreateFlow {
                    document_id: d.id,
                    process_id: Some(p.id.selector()),
                    flow: Flow {
                        message_ref: None,
                        flow_metadata: None,
                        base_extras: None,
                        association_direction: None,
                        id: flow.clone().into(),
                        kind: FlowKind::Sequence,
                        source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                            source.element.clone(),
                        )),
                        target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                            eid.selector(),
                        )),
                        name: Some(String::new()),
                        documentation: String::new().into(),
                        condition: None,
                    },
                },
                Mutation::CreateEdge {
                    document_id: d.id,
                    diagram_id: diagram.id.selector(),
                    edge: Edge {
                        di_metadata: None,
                        message_visible_kind: None,
                        label_visibility: Some(DiLabelVisibility::Explicit),
                        source_element: None,
                        target_element: None,
                        id: controller::id("Edge").into(),
                        element: flow.into(),
                        waypoints: connection_route(&source.bounds, &shape.bounds, diagram),
                        label: Some(Label::default()),
                        style: None,
                    },
                },
            ]
        });
        if self.ctl.mutate(
            [
                Mutation::CreateElement {
                    document_id: d.id,
                    process_id: p.id.selector(),
                    element: Node {
                        flow_metadata: None,
                        base_extras: None,
                        item_aware: None,
                        activity: None,
                        io_specification: None,
                        loop_characteristics: None,
                        details,
                        id: eid.clone(),
                        name: (kind != NodeKind::TextAnnotation).then(|| label.into()),
                        documentation: String::new().into(),
                        kind,
                        owner: String::new(),
                        contact: String::new(),
                    },
                },
                Mutation::CreateShape {
                    document_id: d.id,
                    diagram_id: diagram.id.selector(),
                    shape,
                },
            ]
            .into_iter()
            .chain(append_flow.into_iter().flatten())
            .collect(),
        ) {
            self.select(Some(eid.selector()));
        }
    }
    fn add_pool(&mut self) {
        if !self.prepare_navigation() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let process = d
            .processes
            .iter()
            .find(|p| self.process_id.as_ref().is_none_or(|id| &p.id == id))
            .map(|p| processcraft_core::bpmn_activity::ModelReference::Local(p.id.selector()));
        let participant = controller::id("Participant");
        let mut batch = Vec::new();
        let plane_collaboration = d.collaborations().find(|root| root.id == diagram.plane);
        let collaboration_id = d
            .collaboration
            .as_ref()
            .filter(|_| plane_collaboration.is_none())
            .or(plane_collaboration)
            .map_or_else(|| controller::id("Collaboration").into(), |c| c.id.clone());
        if d.collaboration.is_none() && plane_collaboration.is_none() {
            batch.push(Mutation::CreateCollaboration {
                document_id: d.id,
                collaboration: Collaboration {
                    kind: None,
                    extras: None,
                    correlation_keys: Vec::new(),
                    base: None,
                    name: None,
                    conversation_nodes: vec![],
                    conversation_links: vec![],
                    id: collaboration_id.clone(),
                    participants: vec![],
                    flows: vec![],
                },
            });
        }
        if plane_collaboration.is_none() {
            batch.push(Mutation::SetDiagramPlane {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                plane: collaboration_id.selector(),
            });
        }
        let value = Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            base: None,
            id: participant.clone().into(),
            name: Some("Pool".into()),
            process,
        };
        if let Some(root) = plane_collaboration {
            let mut root = root.clone();
            root.participants.push(value);
            batch.push(Mutation::SetCollaborationRoot {
                document_id: d.id,
                collaboration: root,
            });
        } else {
            batch.push(Mutation::CreateParticipant {
                document_id: d.id,
                participant: value,
            });
        }
        batch.push(Mutation::CreateShape {
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
                id: controller::id("PoolShape").into(),
                element: participant.into(),
                bounds: Bounds {
                    x: self.camera.0 + 20.0,
                    y: self.camera.1 + 60.0,
                    width: 700.0,
                    height: 350.0,
                },
                label: Some(Label::default()),
                style: None,
            },
        });
        self.ctl.mutate_at_generation(snap.generation, batch);
    }
    fn add_lane(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let lane_id = if self.lane == 0 {
            None
        } else {
            p.lanes.get(self.lane - 1).map(|l| l.id.selector())
        };
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
                    self.ctl.error = prepared.warnings.join("; ");
                }
            }
            Err(e) => self.ctl.error = format!("Could not move selection: {e}"),
        }
    }
    fn move_selected(&mut self, dx: f64, dy: f64) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
        let Some(flow) = p.flows.iter().find(|f| {
            self.selected
                .as_ref()
                .is_some_and(|id| f.id.selector() == *id)
        }) else {
            return;
        };
        let (Some(a), Some(b), Some(edge)) = (
            diagram.shapes.iter().find(|s| matches!(&flow.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if s.element == *id)),
            diagram.shapes.iter().find(|s| matches!(&flow.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if s.element == *id)),
            diagram.edges.iter().find(|e| e.element == flow.id),
        ) else {
            return;
        };
        self.ctl.mutate(vec![Mutation::Reroute {
            document_id: d.id,
            diagram_id: diagram.id.selector(),
            edge_id: edge.id.selector(),
            waypoints: connection_route(&a.bounds, &b.bounds, diagram),
        }]);
    }
    fn connect(&mut self) {
        let (Some(a), Some(b)) = (self.source.clone(), self.target.clone()) else {
            self.ctl.error = "Choose a source and a target.".into();
            return;
        };
        self.connect_elements(a, b, Some(self.connection_kind));
    }
    /// Connect two nodes of one process scope; `kind: None` picks association for annotations.
    fn connect_elements(
        &mut self,
        a: ElementSelector,
        b: ElementSelector,
        kind: Option<FlowKind>,
    ) -> bool {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return false;
        };
        if a == b {
            self.ctl.error = "Choose two different nodes.".into();
            return false;
        }
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return false;
        };
        let Some(p) = processcraft_core::queries::scopes(d)
            .into_iter()
            .find(|p| p.nodes.iter().any(|n| n.id == a) && p.nodes.iter().any(|n| n.id == b))
        else {
            self.ctl.error = "Both elements must be in the same process to connect them; use Collaboration for message flows.".into();
            return false;
        };
        let (Some(sa), Some(sb)) = (
            diagram.shapes.iter().find(|s| s.element == a),
            diagram.shapes.iter().find(|s| s.element == b),
        ) else {
            return false;
        };
        let kind = kind.unwrap_or_else(|| {
            if p.nodes
                .iter()
                .any(|n| (n.id == a || n.id == b) && n.kind == NodeKind::TextAnnotation)
            {
                FlowKind::Association
            } else {
                FlowKind::Sequence
            }
        });
        let eid = controller::id("Flow");
        let points = connection_route(&sa.bounds, &sb.bounds, diagram);
        self.ctl.mutate(vec![
            Mutation::CreateFlow {
                document_id: d.id,
                process_id: Some(p.id.selector()),
                flow: Flow {
                    message_ref: None,
                    flow_metadata: None,
                    base_extras: None,
                    association_direction: None,
                    id: eid.clone().into(),
                    kind,
                    source: Some(processcraft_core::bpmn_activity::ModelReference::Local(a)),
                    target: Some(processcraft_core::bpmn_activity::ModelReference::Local(b)),
                    name: (kind != FlowKind::Association).then(String::new),
                    documentation: String::new().into(),
                    condition: None,
                },
            },
            Mutation::CreateEdge {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                edge: Edge {
                    di_metadata: None,
                    message_visible_kind: None,
                    label_visibility: Some(DiLabelVisibility::Explicit),
                    source_element: None,
                    target_element: None,
                    id: controller::id("Edge").into(),
                    element: eid.into(),
                    waypoints: points,
                    label: Some(Label::default()),
                    style: None,
                },
            },
        ])
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
        let Some(d) = snap.project.documents.get(self.doc) else {
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
                if action == "close" {
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
                    let prepared = *prepared;
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
                        let mut mutations: Vec<_> = prepared
                            .resources
                            .into_iter()
                            .map(|resource| Mutation::AddResource { resource })
                            .collect();
                        mutations.push(Mutation::CreateDocument {
                            document: prepared.document,
                        });
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
                            self.ctl.error = prepared.report.limitations.join("; ");
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
        if self.shell.problems && self.problem_key.as_ref() != Some(&problem_key) {
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
                if action == "close" {
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
            self.confirm = Some("close");
        }
        self.canvas_shortcuts(&ctx);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.new_or_open("open");
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.new_or_open("new");
        }
        self.workbench_ui(ui);
        self.dialogs_ui(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_projects_are_pristine_and_file_names_are_safe() {
        let mut project = controller::blank_project();
        assert!(is_pristine_project(&project));
        project.name = "Renamed".into();
        assert!(!is_pristine_project(&project));
        assert_eq!(file_stem("Order: Q1/Q2?"), "Order_ Q1_Q2_");
        assert_eq!(file_stem("  ..."), "project");
        let app = ProcessCraft::default();
        assert!(!app.ctl.session.snapshot().dirty);
        assert!(!app.has_drafts());
    }
    #[test]
    fn pool_creation_uses_the_selected_collaboration_plane() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let root = |id: &str| Collaboration {
            id: id.parse().unwrap(),
            name: None,
            kind: None,
            extras: None,
            correlation_keys: vec![],
            base: None,
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        let diagram = Diagram {
            id: "SecondDiagram".parse().unwrap(),
            plane_id: "SecondPlane".parse().unwrap(),
            plane: "SecondRoot".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: vec![],
            edges: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::SetCollaborationRoot {
                document_id,
                collaboration: root("FirstRoot")
            },
            Mutation::SetCollaborationRoot {
                document_id,
                collaboration: root("SecondRoot")
            },
            Mutation::CreateDiagram {
                document_id,
                diagram
            }
        ]));
        app.diagram_id = Some("SecondDiagram".parse().unwrap());
        app.add_pool();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        assert!(
            document
                .collaboration
                .as_ref()
                .unwrap()
                .participants
                .is_empty()
        );
        assert_eq!(document.additional_collaborations[0].participants.len(), 1);
        assert_eq!(
            document
                .diagrams
                .iter()
                .find(|diagram| diagram.id == "SecondDiagram".parse::<ElementSelector>().unwrap())
                .unwrap()
                .plane,
            "SecondRoot".parse::<ElementSelector>().unwrap()
        );
    }

    #[test]
    fn graph_gateway_properties_keep_metadata_and_representation_through_history() {
        use processcraft_core::bpmn_collaboration::{
            Choreography, ChoreographyFlowElement, ChoreographyGraph,
        };
        use processcraft_core::bpmn_model::{GatewayDirection, NodeDetails};
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let node = Node {
            id: "GraphGateway".parse().unwrap(),
            name: None,
            documentation: "Original".to_owned().into(),
            base_extras: None,
            kind: NodeKind::ExclusiveGateway,
            details: Some(NodeDetails::Gateway {
                default_flow: None,
                direction: None,
                instantiate: None,
                event_gateway_type: None,
                activation_condition: None,
            }),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            flow_metadata: None,
            item_aware: None,
            owner: String::new(),
            contact: String::new(),
        };
        let root = Choreography {
            id: "GraphRoot".parse().unwrap(),
            kind: None,
            graph: Some(ChoreographyGraph {
                flow_elements: vec![ChoreographyFlowElement::Gateway(Box::new(node))],
                ..Default::default()
            }),
            name: None,
            extras: None,
            base: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            participants: vec![],
            message_flows: vec![],
            activities: vec![],
            sequence_flows: vec![],
        };
        let shape = |id: &str| Shape {
            id: id.parse().unwrap(),
            element: "GraphGateway".parse().unwrap(),
            bounds: Bounds {
                x: 10.0,
                y: 10.0,
                width: 60.0,
                height: 60.0,
            },
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            label: None,
            label_visibility: None,
            style: None,
        };
        let diagram = Diagram {
            id: "GraphDiagram".parse().unwrap(),
            plane_id: "GraphPlane".parse().unwrap(),
            plane: "GraphRoot".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: vec![shape("FirstView"), shape("SecondView")],
            edges: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::CreateChoreography {
                document_id,
                choreography: root
            },
            Mutation::CreateDiagram {
                document_id,
                diagram
            }
        ]));
        app.diagram_id = Some("GraphDiagram".parse().unwrap());
        app.select(Some("GraphGateway".parse().unwrap()));
        app.representation_diagram = app.diagram_id.clone();
        app.representation_selection = vec!["SecondView".parse().unwrap()];
        app.draft_name_present = true;
        app.draft = "Decision".into();
        if let Some(NodeDetails::Gateway { direction, .. }) = &mut app.bpmn_details {
            *direction = Some(GatewayDirection::Diverging);
        } else {
            panic!("gateway details");
        }
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let scope = processcraft_core::queries::choreography_scopes(&snapshot.project.documents[0])
            .into_iter()
            .find(|scope| scope.owner == "GraphRoot".parse::<ElementSelector>().unwrap())
            .unwrap();
        assert_eq!(scope.gateways[0].name.as_deref(), Some("Decision"));
        assert_eq!(
            scope.gateways[0].documentation.plain_text(),
            Some("Original")
        );
        assert!(matches!(
            &scope.gateways[0].details,
            Some(NodeDetails::Gateway {
                direction: Some(GatewayDirection::Diverging),
                ..
            })
        ));
        app.history(Request::Undo);
        assert_eq!(
            app.representation_selection,
            vec!["SecondView".parse::<ElementSelector>().unwrap()]
        );
        assert_eq!(app.draft, "");
        app.history(Request::Redo);
        assert_eq!(app.draft, "Decision");
        assert_eq!(
            app.representation_selection,
            vec!["SecondView".parse::<ElementSelector>().unwrap()]
        );
        app.draft = "Stale edit".into();
        app.draft_dirty = true;
        let project = app.ctl.session.snapshot().project.clone();
        assert!(app.ctl.mutate(vec![Mutation::ProjectProperties {
            name: "Changed externally".into(),
            description: project.description.clone()
        }]));
        assert!(!app.commit_draft());
        let snapshot = app.ctl.session.snapshot();
        let scope = processcraft_core::queries::choreography_scopes(&snapshot.project.documents[0])
            .into_iter()
            .find(|scope| scope.owner == "GraphRoot".parse::<ElementSelector>().unwrap())
            .unwrap();
        assert_eq!(scope.gateways[0].name.as_deref(), Some("Decision"));
    }

    #[test]
    fn collaboration_property_selection_preserves_optional_names_and_metadata() {
        use processcraft_core::bpmn_collaboration::{
            CollaborationArtifact, CollaborationExtras, EndPoint, PartnerEntity, PartnerRole,
        };
        use processcraft_core::bpmn_metadata::{BaseMetadata, Documentation};
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let metadata = Some(BaseMetadata {
            documentation: vec![Documentation {
                id: None,
                text_format: Some("text/plain".into()),
                content: "Keep this".into(),
            }],
            ..Default::default()
        });
        let entity_id = processcraft_core::ids::ElementIdentity::anonymous();
        let role_id = processcraft_core::ids::ElementIdentity::anonymous();
        let endpoint_id = processcraft_core::ids::ElementIdentity::anonymous();
        let group_id = processcraft_core::ids::ElementIdentity::anonymous();
        let root = Collaboration {
            id: "Root".parse().unwrap(),
            name: Some("Collaboration name".into()),
            kind: None,
            extras: Some(CollaborationExtras {
                artifacts: vec![CollaborationArtifact::Group(
                    processcraft_core::bpmn_model::Group {
                        id: group_id.clone(),
                        base: None,
                        category_value_ref: None,
                    },
                )],
                ..Default::default()
            }),
            base: metadata.clone(),
            correlation_keys: vec![],
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        let choreography = processcraft_core::bpmn_collaboration::Choreography {
            kind: None,
            graph: None,
            id: "Choreography".parse().unwrap(),
            name: Some(String::new()),
            base: metadata.clone(),
            extras: None,
            correlation_keys: vec![],
            participants: vec![],
            message_flows: vec![],
            activities: vec![],
            sequence_flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::SetCollaborationRoot {
                document_id,
                collaboration: root
            },
            Mutation::CreateChoreography {
                document_id,
                choreography
            },
            Mutation::SetPartnerCatalog {
                document_id,
                partner_entities: vec![PartnerEntity {
                    id: Some(entity_id.clone()),
                    base: metadata.clone(),
                    name: Some("Customers".into()),
                    participant_refs: vec![]
                }],
                partner_roles: vec![PartnerRole {
                    id: Some(role_id.clone()),
                    base: metadata.clone(),
                    name: None,
                    participant_refs: vec![]
                }],
                endpoints: vec![EndPoint {
                    id: Some(endpoint_id.clone()),
                    base: metadata.clone()
                }]
            }
        ]));
        let context = egui::Context::default();
        for (selector, expected, present) in [
            (entity_id.selector(), "Customers", true),
            (role_id.selector(), "", false),
            ("Root".parse().unwrap(), "Collaboration name", true),
            ("Choreography".parse().unwrap(), "", true),
        ] {
            app.select(Some(selector));
            assert_eq!(app.draft, expected);
            assert_eq!(app.draft_name_present, present);
            let mut output =
                context.run_ui(egui::RawInput::default(), |ui| app.inspector_ui(ui, true));
            output.textures_delta.clear();
            assert_eq!(app.draft, expected);
            assert_eq!(app.draft_name_present, present);
        }
        app.select(Some(entity_id.selector()));
        app.draft = "Clients".into();
        app.draft_dirty = true;
        assert!(app.commit_draft());
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(
            snapshot.project.documents[0].partner_entities[0]
                .name
                .as_deref(),
            Some("Clients")
        );
        assert_eq!(
            snapshot.project.documents[0].partner_entities[0].base,
            metadata
        );
        app.history(Request::Undo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].partner_entities[0]
                .name
                .as_deref(),
            Some("Customers")
        );
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        assert_eq!(
            inspector::selection_kind(document, &entity_id.selector()),
            Some("Partner entity")
        );
        assert_eq!(
            inspector::selection_kind(document, &role_id.selector()),
            Some("Partner role")
        );
        assert_eq!(
            inspector::selection_kind(document, &endpoint_id.selector()),
            Some("Endpoint")
        );
        assert_eq!(
            inspector::selection_kind(document, &group_id.selector()),
            Some("Group")
        );
        assert_eq!(
            inspector::selection_kind(document, &"Root".parse().unwrap()),
            Some("Collaboration")
        );
        assert_eq!(
            inspector::selection_kind(document, &"Choreography".parse().unwrap()),
            Some("Choreography")
        );
        assert!(!inspector::selection_has_name(
            document,
            &endpoint_id.selector()
        ));
        assert!(!inspector::selection_has_name(
            document,
            &group_id.selector()
        ));
        let mut with_association = document.as_ref().clone();
        with_association
            .collaboration
            .as_mut()
            .unwrap()
            .extras
            .as_mut()
            .unwrap()
            .artifacts
            .push(CollaborationArtifact::Association(Box::new(Flow {
                id: "Association".parse().unwrap(),
                kind: FlowKind::Association,
                source: None,
                target: None,
                name: None,
                documentation: String::new().into(),
                base_extras: None,
                condition: None,
                association_direction: None,
                flow_metadata: None,
                message_ref: None,
            })));
        let association: ElementSelector = "Association".parse().unwrap();
        assert!(!inspector::selection_has_name(
            &with_association,
            &association
        ));
        assert_eq!(
            inspector::selection_kind(&with_association, &association),
            Some("Association")
        );
        app.select(Some(endpoint_id.selector()));
        let mut output = context.run_ui(egui::RawInput::default(), |ui| app.inspector_ui(ui, true));
        output.textures_delta.clear();
        assert!(!app.draft_name_present);
    }

    #[test]
    fn clicking_choreography_band_preserves_participant_name_through_di_history() {
        use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let participant = |id: &str, name: Option<&str>| Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: id.parse().unwrap(),
            name: name.map(str::to_owned),
            base: None,
            process: None,
            multiplicity: None,
        };
        let choreography = Choreography {
            kind: None,
            graph: None,
            extras: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            id: "Choreography".parse().unwrap(),
            name: None,
            base: None,
            participants: vec![
                participant("Buyer", Some("Buyer")),
                participant("Seller", None),
            ],
            message_flows: vec![Flow {
                id: "Message".parse().unwrap(),
                kind: FlowKind::Message,
                source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                    "Buyer".parse().unwrap(),
                )),
                target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                    "Seller".parse().unwrap(),
                )),
                name: Some(String::new()),
                documentation: "Original documentation".into(),
                base_extras: None,
                flow_metadata: None,
                condition: None,
                association_direction: None,
                message_ref: None,
            }],
            activities: vec![ChoreographyActivity {
                loop_type: None,
                details: None,
                id: "Task".parse().unwrap(),
                name: Some("Order".into()),
                base: None,
                flow_metadata: None,
                correlation_keys: vec![],
                participant_refs: vec![
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Buyer".parse().unwrap(),
                    ),
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Seller".parse().unwrap(),
                    ),
                ],
                initiating_participant: Some(
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Buyer".parse().unwrap(),
                    ),
                ),
                message_flow_refs: vec![processcraft_core::bpmn_activity::ModelReference::Local(
                    "Message".parse().unwrap(),
                )],
            }],
            sequence_flows: vec![],
        };
        let shape = |id: &str,
                     element: &str,
                     bounds: Bounds,
                     band: Option<ParticipantBandKind>,
                     owner: Option<DiReference>| Shape {
            id: id.parse().unwrap(),
            element: element.parse().unwrap(),
            bounds,
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: band,
            choreography_activity_shape: owner,
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            label: Some(Label::default()),
            label_visibility: Some(DiLabelVisibility::Explicit),
            style: None,
        };
        let diagram = Diagram {
            id: "ChoreographyDiagram".parse().unwrap(),
            plane_id: "ChoreographyPlane".parse().unwrap(),
            plane: "Choreography".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: Some(vec![
                "ActivityShape".parse().unwrap(),
                "BuyerBand".parse().unwrap(),
                "OtherShape".parse().unwrap(),
                "OtherBand".parse().unwrap(),
            ]),
            shapes: vec![
                shape(
                    "ActivityShape",
                    "Task",
                    Bounds {
                        x: 50.,
                        y: 50.,
                        width: 160.,
                        height: 100.,
                    },
                    None,
                    None,
                ),
                shape(
                    "BuyerBand",
                    "Buyer",
                    Bounds {
                        x: 50.,
                        y: 50.,
                        width: 160.,
                        height: 30.,
                    },
                    Some(ParticipantBandKind::TopInitiating),
                    Some(DiReference::Local("ActivityShape".parse().unwrap())),
                ),
                shape(
                    "OtherShape",
                    "Task",
                    Bounds {
                        x: 350.,
                        y: 50.,
                        width: 160.,
                        height: 100.,
                    },
                    None,
                    None,
                ),
                shape(
                    "OtherBand",
                    "Seller",
                    Bounds {
                        x: 350.,
                        y: 50.,
                        width: 160.,
                        height: 30.,
                    },
                    Some(ParticipantBandKind::TopNonInitiating),
                    Some(DiReference::Local("OtherShape".parse().unwrap())),
                ),
            ],
            edges: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::CreateChoreography {
                document_id,
                choreography
            },
            Mutation::CreateDiagram {
                document_id,
                diagram
            }
        ]));
        app.diagram_id = Some("ChoreographyDiagram".parse().unwrap());
        let context = egui::Context::default();
        let frame = |app: &mut ProcessCraft, events| {
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
        for _ in 0..300 {
            frame(&mut app, vec![]);
            if app
                .hits
                .iter()
                .any(|h| h.diagram_element_id == "BuyerBand".parse::<ElementSelector>().unwrap())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let hit = app
            .hits
            .iter()
            .rev()
            .find(|h| h.contains(130., 65., 0.))
            .expect("band hit");
        assert_eq!(
            hit.diagram_element_id,
            "BuyerBand".parse::<ElementSelector>().unwrap()
        );
        let canvas = context
            .data(|data| data.get_temp::<Rect>(egui::Id::unique("test_canvas_rect")))
            .unwrap();
        let point = Pos2::new(
            canvas.left() + 12. + ((130. - app.camera.0) as f32 * app.zoom),
            canvas.top() + 12. + ((65. - app.camera.1) as f32 * app.zoom),
        );
        let button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(point), button(true)],
        );
        frame(&mut app, vec![button(false)]);
        assert_eq!(app.selected, Some("Buyer".parse().unwrap()));
        assert!(app.draft_name_present);
        assert_eq!(app.draft, "Buyer");
        // Dragging one of two representations of the same task must not move the other.
        let owner_point = Pos2::new(
            canvas.left() + 12. + ((130. - app.camera.0) as f32 * app.zoom),
            canvas.top() + 12. + ((115. - app.camera.1) as f32 * app.zoom),
        );
        let moved = owner_point + Vec2::new(30., 0.);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(owner_point),
                pointer(owner_point, true),
            ],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(moved)]);
        frame(&mut app, vec![pointer(moved, false)]);
        let bounds = |app: &ProcessCraft, id: &str| {
            app.ctl.session.snapshot().project.documents[0].diagrams[1]
                .shapes
                .iter()
                .find(|shape| shape.id == id.parse::<ElementSelector>().unwrap())
                .unwrap()
                .bounds
                .clone()
        };
        assert!(bounds(&app, "ActivityShape").x > 50.);
        assert_eq!(bounds(&app, "OtherShape").x, 350.);
        assert_eq!(bounds(&app, "OtherBand").x, 350.);
        app.history(Request::Undo);
        assert_eq!(bounds(&app, "ActivityShape").x, 50.);
        app.history(Request::Redo);
        assert!(bounds(&app, "ActivityShape").x > 50.);
        app.history(Request::Undo);
        // A concurrent model edit cancels a gesture instead of applying stale bounds.
        for _ in 0..300 {
            frame(&mut app, vec![]);
            if app.hits.iter().any(|h| {
                h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(owner_point),
                pointer(owner_point, true),
            ],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(moved)]);
        assert!(app.drag.is_some());
        assert!(app.ctl.mutate(vec![Mutation::Rename {
            document_id,
            element_id: "Task".parse().unwrap(),
            name: Some("Updated order".into())
        }]));
        frame(&mut app, vec![pointer(moved, false)]);
        assert_eq!(bounds(&app, "ActivityShape").x, 50.);
        assert!(app.drag.is_none());
        app.history(Request::Undo);
        assert!(app.ctl.mutate(vec![Mutation::SetLabelBounds {
            document_id,
            diagram_id: "ChoreographyDiagram".parse().unwrap(),
            representation_id: "ActivityShape".parse().unwrap(),
            bounds: Bounds {
                x: 100.,
                y: 100.,
                width: 60.,
                height: 30.
            },
        }]));
        for _ in 0..300 {
            frame(&mut app, vec![]);
            if app.hits.iter().any(|h| {
                h.is_label
                    && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let alt = egui::Modifiers {
            alt: true,
            ..egui::Modifiers::NONE
        };
        let frame_alt = |app: &mut ProcessCraft, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events: std::iter::once(egui::Event::ModifiersChanged(alt))
                        .chain(events)
                        .collect(),
                    ..Default::default()
                },
                |ui| app.canvas_ui(ui),
            );
            output.textures_delta.clear();
        };
        let alt_pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: alt,
        };
        frame_alt(
            &mut app,
            vec![
                egui::Event::PointerMoved(owner_point),
                alt_pointer(owner_point, true),
            ],
        );
        frame_alt(&mut app, vec![egui::Event::PointerMoved(moved)]);
        frame_alt(&mut app, vec![alt_pointer(moved, false)]);
        assert_eq!(bounds(&app, "ActivityShape").x, 50.);
        let label_x = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
                .label
                .as_ref()
                .unwrap()
                .bounds
                .as_ref()
                .unwrap()
                .x
        };
        assert!(label_x(&app) > 100.);
        app.history(Request::Undo);
        assert_eq!(label_x(&app), 100.);
        app.history(Request::Redo);
        assert!(label_x(&app) > 100.);
        app.history(Request::Undo);
        app.history(Request::Undo);
        for _ in 0..300 {
            frame(
                &mut app,
                vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
            );
            if let Some(hit) = app.hits.iter().find(|h| {
                h.is_label
                    && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
            }) && matches!(&hit.geometry, processcraft_render::HitGeometry::Shape(b) if b.width != 60.)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let fallback = match &app
            .hits
            .iter()
            .find(|h| {
                h.is_label
                    && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
            })
            .unwrap()
            .geometry
        {
            processcraft_render::HitGeometry::Shape(bounds) => bounds.clone(),
            _ => panic!("expected label bounds"),
        };
        frame_alt(
            &mut app,
            vec![
                egui::Event::PointerMoved(owner_point),
                alt_pointer(owner_point, true),
            ],
        );
        frame_alt(&mut app, vec![egui::Event::PointerMoved(moved)]);
        frame_alt(&mut app, vec![alt_pointer(moved, false)]);
        let actual = app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .clone()
            .unwrap();
        assert!((actual.x - fallback.x - 30. / app.zoom as f64).abs() < 0.001);
        assert_eq!(actual.width, fallback.width);
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
                .label
                .as_ref()
                .unwrap()
                .bounds
                .is_none()
        );
        app.select(Some("Buyer".parse().unwrap()));
        assert!(
            app.ctl
                .mutate(vec![Mutation::SetShapeRepresentationMetadata {
                    document_id,
                    diagram_id: "ChoreographyDiagram".parse().unwrap(),
                    shape_id: "BuyerBand".parse().unwrap(),
                    metadata: Some(DiElementMetadata::default()),
                    is_message_visible: None,
                    participant_band_kind: Some(ParticipantBandKind::TopInitiating),
                    choreography_activity_shape: Some(DiReference::Local(
                        "ActivityShape".parse().unwrap()
                    ))
                }])
        );
        let buyer_name = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].choreographies[0].participants[0]
                .name
                .clone()
        };
        assert_eq!(buyer_name(&app), Some("Buyer".into()));
        app.history(Request::Undo);
        assert_eq!(buyer_name(&app), Some("Buyer".into()));
        let snapshot = app.ctl.session.snapshot();
        let buyer = &snapshot.project.documents[0].choreographies[0].participants[0];
        assert_eq!(buyer.name.as_deref(), Some("Buyer"));
        assert!(app.remove_shape_representation(
            document_id,
            "ChoreographyDiagram".parse().unwrap(),
            "BuyerBand".parse().unwrap()
        ));
        assert_eq!(app.selected, Some("Buyer".parse().unwrap()));
        assert_eq!(buyer_name(&app), Some("Buyer".into()));
        let snapshot = app.ctl.session.snapshot();
        let diagram = &snapshot.project.documents[0].diagrams[1];
        assert!(
            !diagram
                .shapes
                .iter()
                .any(|shape| shape.id == "BuyerBand".parse::<ElementSelector>().unwrap())
        );
        assert!(
            diagram
                .shapes
                .iter()
                .any(|shape| shape.id == "OtherBand".parse::<ElementSelector>().unwrap())
        );
        app.history(Request::Undo);
        let snapshot = app.ctl.session.snapshot();
        let diagram = &snapshot.project.documents[0].diagrams[1];
        assert!(
            diagram
                .shapes
                .iter()
                .any(|shape| shape.id == "BuyerBand".parse::<ElementSelector>().unwrap())
        );
        assert!(
            diagram
                .shapes
                .iter()
                .any(|shape| shape.id == "OtherBand".parse::<ElementSelector>().unwrap())
        );
        assert_eq!(buyer_name(&app), Some("Buyer".into()));
        app.select(Some("Task".parse().unwrap()));
        assert!(app.draft_name_present);
        assert_eq!(app.draft, "Order");
        for name in [Some(String::new()), None] {
            assert!(app.ctl.mutate(vec![Mutation::Rename {
                document_id,
                element_id: "Buyer".parse().unwrap(),
                name: name.clone()
            }]));
            app.select(Some("Buyer".parse().unwrap()));
            assert_eq!(app.draft_name_present, name.is_some());
            assert_eq!(app.draft, name.clone().unwrap_or_default());
            assert_eq!(buyer_name(&app), name);
        }
        app.select(Some("Message".parse().unwrap()));
        assert!(app.is_flow);
        assert!(app.draft_name_present);
        assert!(app.draft.is_empty());
        assert_eq!(app.docs, "Original documentation");
        app.docs = "Updated documentation".into();
        app.docs_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let flow = &snapshot.project.documents[0].choreographies[0].message_flows[0];
        assert_eq!(flow.name.as_deref(), Some(""));
        assert_eq!(
            flow.documentation.plain_text(),
            Some("Updated documentation")
        );
    }
    #[test]
    fn new_task_has_explicit_label_and_exports_in_memory() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let task = app.selected.as_ref().expect("new task selected");
        let shape = document.diagrams[0]
            .shapes
            .iter()
            .find(|shape| &shape.element == task)
            .expect("task shape");
        assert!(matches!(
            shape.label_visibility,
            Some(DiLabelVisibility::Explicit)
        ));
        assert!(shape.label.is_some());
        let exported =
            processcraft_io::interchange::export_bpmn(&snapshot.project, document.id, true)
                .expect("new diagram exports");
        let xml = String::from_utf8(exported.bytes).unwrap();
        assert!(xml.contains("BPMNLabel"));
    }

    #[test]
    fn discard_keeps_confirmation_and_draft_while_export_is_pending() {
        let mut app = ProcessCraft::default();
        let session = app.ctl.session.id();
        app.confirm = Some("new");
        app.draft_dirty = true;
        app.export_ready = Some((vec![1], vec![], "bpmn"));
        assert_eq!(app.discard_transition(), None);
        assert_eq!(app.confirm, Some("new"));
        assert!(app.draft_dirty);
        app.execute("new");
        assert_eq!(app.ctl.session.id(), session);
        assert_eq!(app.confirm, Some("new"));
        app.export_ready = None;
        assert_eq!(app.discard_transition(), Some("new"));
    }
    #[test]
    fn canceled_or_failed_open_preserves_uncommitted_drafts_and_selection() {
        let mut app = ProcessCraft {
            draft: "Uncommitted".into(),
            draft_dirty: true,
            dictionary_name: "Draft entry".into(),
            process_id: Some(controller::id("Process").into()),
            ..Default::default()
        };
        let process = app.process_id.clone();
        let session = app.ctl.session.id();
        app.confirm = Some("open");
        assert_eq!(app.discard_transition(), Some("open"));
        app.open_selected_path(None);
        assert!(app.draft_dirty);
        assert_eq!(app.draft, "Uncommitted");
        assert_eq!(app.dictionary_name, "Draft entry");
        assert_eq!(app.process_id, process);
        assert_eq!(app.ctl.session.id(), session);
        let missing =
            std::env::temp_dir().join(format!("missing-{}.processcraft", uuid::Uuid::new_v4()));
        app.open_selected_path(Some(missing));
        assert!(app.draft_dirty);
        assert_eq!(app.draft, "Uncommitted");
        assert_eq!(app.dictionary_name, "Draft entry");
        assert_eq!(app.process_id, process);
        assert_eq!(app.ctl.session.id(), session);
    }
    #[test]
    fn dictionary_history_and_external_update_do_not_create_phantom_drafts() {
        let mut app = ProcessCraft::default();
        let id = processcraft_core::ids::DictionaryEntryId::new();
        let mut entry = DictionaryEntry {
            id,
            kind: DictionaryKind::System,
            name: "ERP".into(),
            description: String::new(),
        };
        assert!(app.ctl.mutate(vec![Mutation::CreateDictionaryEntry {
            entry: entry.clone()
        }]));
        app.dictionary_selected = Some(id);
        app.refresh_dictionary_editor();
        app.dictionary_name = "ERP Cloud".into();
        assert!(app.commit_dictionary_draft());
        app.history(Request::Undo);
        assert_eq!(app.dictionary_name, "ERP");
        assert!(!app.has_drafts());
        let generation = app.ctl.session.snapshot().generation;
        assert!(app.commit_dictionary_draft());
        assert_eq!(app.ctl.session.snapshot().generation, generation);
        app.history(Request::Redo);
        assert_eq!(app.dictionary_name, "ERP Cloud");
        assert!(!app.has_drafts());
        entry.name = "Remote ERP".into();
        assert!(
            app.ctl
                .mutate(vec![Mutation::UpdateDictionaryEntry { entry }])
        );
        app.refresh_dictionary_editor();
        assert_eq!(app.dictionary_name, "Remote ERP");
        assert!(!app.has_drafts());
    }
    #[test]
    fn recovery_respects_pending_export_and_dictionary_draft() {
        let mut app = ProcessCraft::default();
        assert!(app.can_restore_recovery());
        app.export_ready = Some((vec![1], vec![], "bpmn"));
        assert!(!app.can_restore_recovery());
        app.export_ready = None;
        app.dictionary_name = "Uncommitted entry".into();
        assert!(!app.can_restore_recovery());
        app.dictionary_name.clear();
        assert!(app.can_restore_recovery());
    }
    #[test]
    fn saved_transition_waits_for_export_and_rechecks_drafts() {
        let mut app = ProcessCraft::default();
        let content = app.ctl.session.snapshot().content_id;
        app.ctl.session.mark_saved(content);
        app.after_save = Some("close");
        app.after_save_ready = Some(true);
        app.export_ready = Some((vec![1], vec![], "bpmn"));
        assert!(app.take_ready_transition().is_none());
        assert_eq!(app.after_save, Some("close"));
        app.export_ready = None;
        app.dictionary_name = "New draft".into();
        assert_eq!(app.take_ready_transition(), Some(("close", false)));
        app.dictionary_name.clear();
        app.after_save = Some("new");
        app.after_save_ready = Some(true);
        assert!(app.ctl.mutate(vec![Mutation::RenameProject {
            name: "Edited after save".into()
        }]));
        assert_eq!(app.take_ready_transition(), Some(("new", false)));
    }
    #[test]
    fn widget_drag_commits_once_and_escape_commits_nothing() {
        let mut project = controller::blank_project();
        let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
        let node: XmlId = "Node".parse().unwrap();
        let shape: XmlId = "Shape".parse().unwrap();
        doc.processes[0].nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: node.clone().into(),
            name: Some("Task".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        });
        doc.diagrams[0].shapes.push(Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: shape.clone().into(),
            element: node.into(),
            bounds: Bounds {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 70.0,
            },
            label: None,
            style: None,
        });
        let mut session = processcraft_core::commands::Session::new(project).unwrap();
        let ctx = egui::Context::default();
        let mut drag = None;
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut pass = |events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let (_, response) =
                        ui.allocate_exact_size(Vec2::splat(300.0), Sense::click_and_drag());
                    let snap = session.snapshot();
                    let d = &snap.project.documents[0];
                    let diagram = &d.diagrams[0];
                    if response.drag_started() {
                        let items = diagram
                            .shapes
                            .iter()
                            .map(|s| (s.id.selector(), s.bounds.clone()))
                            .collect();
                        drag = begin_drag(&response, ui.ctx(), items);
                    }
                    advance_drag(&mut drag, ui.ctx());
                    if let Some(done) = finish_drag(&response, &mut drag)
                        && done.offset.length() > 1.0
                    {
                        let bounds = done
                            .items
                            .into_iter()
                            .map(|(shape_id, mut bounds)| {
                                bounds.x += done.offset.x as f64;
                                bounds.y += done.offset.y as f64;
                                processcraft_core::commands::ShapeBounds {
                                    diagram_id: diagram.id.selector(),
                                    shape_id,
                                    bounds,
                                }
                            })
                            .collect();
                        let prepared = processcraft_core::geometry::prepare_move(
                            d,
                            &diagram.id.selector(),
                            bounds,
                        )
                        .unwrap();
                        session
                            .apply(
                                processcraft_core::commands::Envelope {
                                    request_id: "drag".into(),
                                    session_id: session.id(),
                                    expected_generation: Some(snap.generation),
                                },
                                prepared.request,
                            )
                            .unwrap();
                    }
                },
            );
            output.textures_delta.clear();
        };
        let start = Pos2::new(20.0, 20.0);
        let end = Pos2::new(70.0, 35.0);
        pass(vec![]);
        pass(vec![egui::Event::PointerMoved(start), button(start, true)]);
        pass(vec![egui::Event::PointerMoved(Pos2::new(40.0, 25.0))]);
        pass(vec![egui::Event::PointerMoved(end)]);
        pass(vec![button(end, false)]);

        pass(vec![egui::Event::PointerMoved(start), button(start, true)]);
        pass(vec![egui::Event::PointerMoved(end)]);
        pass(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        pass(vec![button(end, false)]);
        let snap = session.snapshot();
        assert_eq!(snap.undo_count, 1);
        assert_eq!(
            snap.project.documents[0].diagrams[0].shapes[0].bounds.x,
            60.0
        );
    }
    #[test]
    fn fit_extents_include_external_label_and_edge() {
        let diagram = Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            label_styles: Vec::new(),
            resolution: None,
            name: None,
            id: "Diagram".parse().unwrap(),
            plane_id: "Plane".parse().unwrap(),
            plane: "Process".parse().unwrap(),
            shapes: vec![Shape {
                di_metadata: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                label_visibility: None,
                is_horizontal: None,
                is_marker_visible: None,
                is_expanded: None,
                id: "Shape".parse().unwrap(),
                element: "Node".parse().unwrap(),
                bounds: Bounds {
                    x: 50.0,
                    y: 30.0,
                    width: 40.0,
                    height: 40.0,
                },
                label: Some(Label {
                    di_metadata: None,
                    id: None,
                    style_ref: None,
                    bounds: Some(Bounds {
                        x: 0.0,
                        y: 74.0,
                        width: 140.0,
                        height: 30.0,
                    }),
                    style: None,
                }),
                style: None,
            }],
            edges: vec![Edge {
                di_metadata: None,
                message_visible_kind: None,
                label_visibility: None,
                source_element: None,
                target_element: None,
                id: "Edge".parse().unwrap(),
                element: "Flow".parse().unwrap(),
                waypoints: vec![Point { x: 90.0, y: 50.0 }, Point { x: 300.0, y: 50.0 }],
                label: None,
                style: None,
            }],
        };
        let (minx, _, maxx, maxy) = diagram_extents(&diagram).unwrap();
        assert_eq!(minx, 0.0);
        assert!(maxx >= 300.0);
        assert!(maxy >= 104.0);
    }
    #[test]
    fn definitions_draft_is_guarded_and_uses_history() {
        let mut app = ProcessCraft::default();
        app.select(None);
        app.definitions_draft.name = Some(String::new());
        app.definitions_draft.target_namespace.clear();
        app.definitions_dirty = true;
        app.draft_dirty = true;
        assert!(!app.commit_draft());
        assert!(app.draft_dirty);
        assert!(
            app.ctl.session.snapshot().project.documents[0]
                .definitions
                .is_none()
        );
        app.definitions_draft.target_namespace = "urn:example".into();
        assert!(app.commit_draft(), "{}", app.ctl.error);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0]
                .definitions
                .as_ref()
                .unwrap()
                .name,
            Some(String::new())
        );
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0]
                .definitions
                .is_none()
        );
        app.history(Request::Redo);
        assert_eq!(app.definitions_draft.target_namespace, "urn:example");
    }
    #[test]
    fn definitions_import_drafts_add_edit_remove_and_undo() {
        use processcraft_core::model::DefinitionsImport;
        let mut app = ProcessCraft::default();
        app.select(None);
        app.definitions_draft.target_namespace = "urn:example".into();
        app.definitions_draft.imports.push(DefinitionsImport {
            namespace: "urn:other".into(),
            location: "other.bpmn".into(),
            import_type: "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
        });
        app.definitions_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.definitions_draft.imports[0].location = "renamed.bpmn".into();
        app.definitions_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.definitions_draft.imports.clear();
        app.definitions_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        assert!(app.definitions_draft.imports.is_empty());
        app.history(Request::Undo);
        assert_eq!(app.definitions_draft.imports[0].location, "renamed.bpmn");
        app.history(Request::Undo);
        assert_eq!(app.definitions_draft.imports[0].location, "other.bpmn");
        app.history(Request::Redo);
        assert_eq!(app.definitions_draft.imports[0].location, "renamed.bpmn");
        app.history(Request::Redo);
        assert!(app.definitions_draft.imports.is_empty());
    }
    #[test]
    fn link_and_compensation_drafts_use_shared_history() {
        use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let link = EventDefinition::Link {
            base: None,
            id: Some("LinkDefinition".parse().unwrap()),
            name: "handoff".into(),
            sources: Vec::new(),
            target: None,
        };
        app.bpmn_details = Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![link.clone()],
            attached_to: None,
            cancel_activity: None,
        });
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.history(Request::Undo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { .. }))
        );
        app.history(Request::Redo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if definitions[0] == link)
        );
        app.add(NodeKind::IntermediateThrowEvent);
        let compensate = EventDefinition::Compensate {
            base: None,
            id: None,
            wait_for_completion: Some(false),
            activity_ref: None,
        };
        app.bpmn_details = Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![compensate.clone()],
            attached_to: None,
            cancel_activity: None,
        });
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.history(Request::Undo);
        app.history(Request::Redo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if definitions[0] == compensate)
        );
    }

    #[test]
    fn cancel_boundary_draft_uses_shared_history() {
        use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Transaction);
        let transaction = app.selected.clone().unwrap();
        app.add(NodeKind::BoundaryEvent);
        app.bpmn_details = Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![EventDefinition::Cancel {
                base: None,
                id: None,
            }],
            attached_to: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                transaction,
            )),
            cancel_activity: Some(true),
        });
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.history(Request::Undo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { .. }))
        );
        app.history(Request::Redo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Cancel { .. }))
        );
    }

    #[test]
    fn activity_references_use_shared_history_with_local_catalog() {
        use processcraft_core::bpmn_catalog::{Interface, Operation};
        use processcraft_core::bpmn_model::{EventResource, QNameRef};
        let namespace = "urn:processcraft:generated";
        let message = QNameRef {
            namespace: namespace.into(),
            local: "Message".parse().unwrap(),
        };
        let operation = QNameRef {
            namespace: namespace.into(),
            local: "Operation".parse().unwrap(),
        };
        let mut project = crate::controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        document.event_resources.push(EventResource::Message {
            base: None,
            item_ref: None,
            id: message.local.clone().into(),
            name: None,
        });
        document.catalog.interfaces.push(Interface {
            base: None,
            id: None,
            name: "Service".into(),
            implementation_ref: None,
            operations: vec![Operation {
                base: None,
                id: Some(operation.local.clone().into()),
                name: "Send".into(),
                implementation_ref: None,
                in_message_ref: message.clone(),
                out_message_ref: None,
                error_refs: Vec::new(),
            }],
        });
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        app.add(NodeKind::SendTask);
        let mut properties = app.bpmn_activity.clone().unwrap_or_default();
        properties.message_ref = Some(message);
        properties.operation_ref = Some(operation);
        app.bpmn_activity = Some(properties.clone());
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        app.history(Request::Undo);
        assert!(
            app.bpmn_activity
                .as_ref()
                .is_none_or(|value| value.message_ref.is_none())
        );
        app.history(Request::Redo);
        assert_eq!(app.bpmn_activity, Some(properties));
    }

    #[test]
    fn process_and_diagram_name_drafts_preserve_absent_and_empty() {
        let mut app = ProcessCraft::default();
        app.select(None);
        app.process_name = None;
        app.diagram_name = Some(String::new());
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        assert!(document.processes[0].name.is_none());
        assert_eq!(document.diagrams[0].name, Some(String::new()));
        app.history(Request::Undo);
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(
            snapshot.project.documents[0].processes[0].name.as_deref(),
            Some("Process")
        );
        assert!(snapshot.project.documents[0].diagrams[0].name.is_none());
        app.history(Request::Redo);
        let snapshot = app.ctl.session.snapshot();
        assert!(snapshot.project.documents[0].processes[0].name.is_none());
        assert_eq!(
            snapshot.project.documents[0].diagrams[0].name,
            Some(String::new())
        );
    }

    #[test]
    fn process_and_diagram_context_follow_id_changes_and_history() {
        let mut app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let document_id = document.id;
        let process = document.processes[0].id.selector();
        let diagram = document.diagrams[0].id.selector();
        app.process_id = Some(process.clone());
        app.diagram_id = Some(diagram.clone());
        let changes = app
            .ctl
            .mutate_changes(vec![Mutation::SetElementXmlId {
                document_id,
                element_id: process.clone(),
                xml_id: Some("RenamedProcess".parse().unwrap()),
            }])
            .expect("process ID change");
        app.remap_context(&changes.selector_mappings);
        let renamed = app.process_id.clone().unwrap();
        assert!(matches!(renamed, ElementSelector::Native { .. }));
        assert_ne!(renamed, process);
        assert_eq!(app.diagram_id, Some(diagram));
        app.history(Request::Undo);
        assert_eq!(app.process_id, Some(process));
        app.history(Request::Redo);
        assert_eq!(app.process_id, Some(renamed));
    }

    #[test]
    fn anonymous_node_selection_name_and_xml_id_survive_history() {
        let mut app = ProcessCraft {
            create_anonymous: true,
            ..Default::default()
        };
        app.add(NodeKind::Task);
        let selected = app.selected.clone().expect("anonymous node selected");
        assert!(matches!(selected, ElementSelector::Native { .. }));
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        assert!(
            processcraft_core::queries::element_identity(document, &selected)
                .unwrap()
                .xml_id()
                .is_none()
        );
        app.draft_name_present = false;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        assert!(
            processcraft_core::queries::find_node(
                &app.ctl.session.snapshot().project.documents[0],
                &selected
            )
            .unwrap()
            .name
            .is_none()
        );
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        assert!(
            app.ctl.mutate(vec![Mutation::SetElementXmlId {
                document_id,
                element_id: selected.clone(),
                xml_id: Some("TaskId".parse().unwrap())
            }]),
            "{}",
            app.ctl.error
        );
        app.restore_selection();
        assert_eq!(app.selected, Some(selected.clone()));
        app.history(Request::Undo);
        assert_eq!(app.selected, Some(selected.clone()));
        assert!(
            processcraft_core::queries::element_identity(
                &app.ctl.session.snapshot().project.documents[0],
                &selected
            )
            .unwrap()
            .xml_id()
            .is_none()
        );
        app.history(Request::Redo);
        assert_eq!(
            processcraft_core::queries::element_identity(
                &app.ctl.session.snapshot().project.documents[0],
                &selected
            )
            .unwrap()
            .xml_id()
            .unwrap()
            .to_string(),
            "TaskId"
        );
    }

    #[test]
    fn legacy_annotation_fallback_is_preserved_until_body_edit() {
        use processcraft_core::bpmn_model::NodeDetails;
        let mut project = controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        document.processes[0].nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            id: "Note".parse().unwrap(),
            name: Some("Old body".into()),
            documentation: String::new().into(),
            kind: NodeKind::TextAnnotation,
            item_aware: None,
            details: Some(NodeDetails::TextAnnotation {
                text_format: None,
                text: None,
            }),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        });
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let selected: ElementSelector = "Note".parse().unwrap();
        app.select(Some(selected.clone()));
        assert_eq!(app.draft, "Old body");
        app.docs = "Description".into();
        app.docs_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        assert_eq!(node.name.as_deref(), Some("Old body"));
        assert!(matches!(
            &node.details,
            Some(NodeDetails::TextAnnotation { text: None, .. })
        ));
        app.draft = "New body".into();
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        assert!(node.name.is_none());
        assert!(
            matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("New body"))
        );
        app.history(Request::Undo);
        assert_eq!(app.draft, "Old body");
    }

    #[test]
    fn annotation_body_draft_does_not_create_a_name() {
        use processcraft_core::bpmn_model::NodeDetails;
        let mut app = ProcessCraft::default();
        app.add(NodeKind::TextAnnotation);
        let selected = app.selected.clone().unwrap();
        app.draft = "A note".into();
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        assert!(node.name.is_none());
        assert!(
            matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
        );
        app.docs = "Description".into();
        app.docs_dirty = true;
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        assert!(
            matches!(&processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap().details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
        );
        app.history(Request::Undo);
        app.history(Request::Redo);
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        assert!(node.name.is_none());
        assert!(
            matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
        );
    }

    #[test]
    fn expression_text_draft_preserves_metadata_and_history() {
        use processcraft_core::bpmn_model::{
            EventDefinition, Expression, ExpressionData, ExpressionKind, NodeDetails, TimerKind,
        };
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let expression = Expression::Structured(ExpressionData {
            id: Some("TimerExpression".parse().unwrap()),
            kind: ExpressionKind::Formal {
                explicit_type: true,
                language: Some("urn:language".into()),
                evaluates_to_type_ref: None,
            },
            text: "PT1M".into(),
            namespaces: vec![
                (
                    String::new(),
                    "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
                ),
                (
                    "xsi".into(),
                    "http://www.w3.org/2001/XMLSchema-instance".into(),
                ),
                ("ex".into(), "urn:example".into()),
            ],
            foreign_attributes: Vec::new(),
        });
        app.bpmn_details = Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![EventDefinition::Timer {
                base: None,
                id: None,
                kind: Some(TimerKind::Duration),
                expression: Some(expression.clone()),
            }],
            attached_to: None,
            cancel_activity: None,
        });
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        if let Some(NodeDetails::Event { definitions, .. }) = &mut app.bpmn_details
            && let EventDefinition::Timer { expression, .. } = &mut definitions[0]
        {
            expression
                .as_mut()
                .unwrap()
                .set_text("PT5M".into())
                .unwrap();
        }
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
            panic!("event missing")
        };
        let EventDefinition::Timer {
            expression: updated,
            ..
        } = &definitions[0]
        else {
            panic!("timer missing")
        };
        let mut expected = expression.clone();
        expected.set_text("PT5M".into()).unwrap();
        assert_eq!(updated.as_ref(), Some(&expected));
        app.history(Request::Undo);
        let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
            panic!("event missing")
        };
        assert!(
            matches!(&definitions[0], EventDefinition::Timer { expression: actual, .. } if actual.as_ref() == Some(&expression))
        );
        app.history(Request::Redo);
        let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
            panic!("event missing")
        };
        assert!(
            matches!(&definitions[0], EventDefinition::Timer { expression: actual, .. } if actual.as_ref() == Some(&expected))
        );
    }
    #[test]
    fn nested_scope_and_event_properties_use_shared_history() {
        use processcraft_core::{
            bpmn_model::{EventDefinition, NodeDetails, TimerKind},
            queries,
        };
        let mut app = ProcessCraft::default();
        app.add(NodeKind::SubProcess);
        let parent = app.selected.clone().expect("subprocess created");
        app.process_id = Some(parent.clone());
        app.select(None);
        app.add(NodeKind::IntermediateCatchEvent);
        let child = app.selected.clone().expect("child created");
        assert_eq!(app.process_id.as_ref(), Some(&parent));
        app.bpmn_details = Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![EventDefinition::Timer {
                base: None,
                id: None,
                kind: Some(TimerKind::Duration),
                expression: Some("PT5M".into()),
            }],
            attached_to: None,
            cancel_activity: None,
        });
        app.draft_dirty = true;
        assert!(app.commit_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let node = queries::find_node(&snapshot.project.documents[0], &child).unwrap();
        assert_eq!(node.details, app.bpmn_details);
        app.history(Request::Undo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression.as_ref().is_some_and(|value| value.plain_text() == Some("PT1M"))))
        );
        app.history(Request::Redo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression.as_ref().is_some_and(|value| value.plain_text() == Some("PT5M"))))
        );
    }
}

#[path = "app_m2.rs"]
mod m2;

#[path = "app_band_create.rs"]
mod band_create;
#[path = "app_base_metadata.rs"]
mod base_metadata;
#[path = "app_call.rs"]
mod call;
#[path = "app_callable.rs"]
mod callable;
#[path = "app_callable_io.rs"]
mod callable_io;
#[path = "app_canvas.rs"]
mod canvas;
#[path = "app_catalog.rs"]
mod catalog;
#[path = "app_choreography_flow.rs"]
mod choreography_flow;
#[path = "app_correlation.rs"]
mod correlation;
#[path = "app_correlation_keys.rs"]
mod correlation_keys;
#[path = "app_data.rs"]
mod data;
#[path = "app_definitions_extra.rs"]
mod definitions_extra;
#[path = "app_di.rs"]
mod di;
#[path = "app_di_metadata.rs"]
mod di_metadata;
#[path = "app_di_style.rs"]
mod di_style;
#[path = "app_documentation.rs"]
mod documentation;
#[path = "app_event.rs"]
mod event;
#[path = "app_flow_metadata.rs"]
mod flow_metadata;
#[path = "app_graph_data.rs"]
mod graph_data;
#[path = "app_inspector.rs"]
mod inspector;
#[path = "app_loop_advanced.rs"]
mod loop_advanced;
#[path = "app_mixed_body.rs"]
mod mixed_body;
#[path = "app_participant_di.rs"]
mod participant_di;
#[path = "app_process_activity.rs"]
mod process_activity;
#[path = "app_roles.rs"]
mod roles;
#[path = "app_shell.rs"]
mod shell;
#[path = "app_xml.rs"]
mod xml;

#[path = "app_tree.rs"]
mod tree;

#[path = "app_m3.rs"]
mod m3;

#[path = "app_collaboration.rs"]
mod collaboration;

#[path = "app_partners.rs"]
mod partners;

#[path = "app_conversation_details.rs"]
mod conversation_details;

#[path = "app_collaboration_associations.rs"]
mod collaboration_associations;

#[path = "app_collaboration_participant.rs"]
mod collaboration_participant;

#[path = "app_collaboration_root.rs"]
mod collaboration_root;

#[path = "app_collaboration_connection.rs"]
mod collaboration_connection;

#[path = "app_collaboration_artifacts.rs"]
mod collaboration_artifacts;
