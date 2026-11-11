use crate::{
    controller::{self, Controller},
    theme::Theme,
};
#[cfg(test)]
use egui::Sense;
use egui::{Color32, Pos2, Rect, Vec2};
use processcraft_core::{
    commands::{Mutation, Request},
    ids::XmlId,
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
        if let Some(l) = &shape.label {
            let b = &l.bounds;
            include(b.x, b.y);
            include(b.x + b.width, b.y + b.height);
        }
    }
    for edge in &diagram.edges {
        for point in &edge.waypoints {
            include(point.x, point.y);
        }
        if let Some(l) = &edge.label {
            let b = &l.bounds;
            include(b.x, b.y);
            include(b.x + b.width, b.y + b.height);
        } else if let Some(p) = edge.waypoints.get(edge.waypoints.len() / 2) {
            include(p.x + 120.0, p.y + 40.0);
        }
    }
    result
}
struct DragGesture {
    items: Vec<(XmlId, Bounds)>,
    origin: Pos2,
    offset: Vec2,
}
impl DragGesture {
    fn new(items: Vec<(XmlId, Bounds)>, origin: Pos2) -> Self {
        Self {
            items,
            origin,
            offset: Vec2::ZERO,
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
    items: Vec<(XmlId, Bounds)>,
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
    process_id: Option<XmlId>,
    diagram_id: Option<XmlId>,
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
    selected: Option<XmlId>,
    multi: Vec<XmlId>,
    draft: String,
    bpmn_details: Option<processcraft_core::bpmn_model::NodeDetails>,
    bpmn_loop: Option<processcraft_core::bpmn_model::LoopCharacteristics>,
    bpmn_activity: Option<processcraft_core::bpmn_activity::ActivityProperties>,
    project_name: String,
    document_name: String,
    definitions_draft: processcraft_core::model::DefinitionsMetadata,
    definitions_dirty: bool,
    problem_key: Option<(String, u64)>,
    problems: Vec<processcraft_core::Diagnostic>,
    docs: String,
    condition: String,
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
    renderer: crate::jobs::Renderer,
    recovery: crate::jobs::RecoveryWriter,
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
    confirm: Option<&'static str>,
    after_save: Option<&'static str>,
    after_save_ready: Option<bool>,
    allow_close: bool,
    draft_dirty: bool,
    source: usize,
    target: usize,
    lane: usize,
}
impl Default for ProcessCraft {
    fn default() -> Self {
        let mut ctl = Controller::default();
        let recovery_records =
            match processcraft_io::recovery::RecoveryStore::new(crate::jobs::recovery_location())
                .and_then(|s| s.list())
            {
                Ok(records) => records,
                Err(e) => {
                    ctl.error = format!("Could not list recovery snapshots: {e}");
                    Vec::new()
                }
            };
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
            multi: vec![],
            draft: String::new(),
            bpmn_details: None,
            bpmn_loop: None,
            bpmn_activity: None,
            project_name: "Untitled project".into(),
            document_name: "Untitled process".into(),
            definitions_draft: empty_definitions_metadata(),
            definitions_dirty: false,
            problem_key: None,
            problems: vec![],
            docs: String::new(),
            condition: String::new(),
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
            renderer: crate::jobs::Renderer::new(),
            recovery: crate::jobs::RecoveryWriter::new(),
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
            confirm: None,
            after_save: None,
            after_save_ready: None,
            allow_close: false,
            draft_dirty: false,
            source: 0,
            target: 0,
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
    }
}
impl ProcessCraft {
    pub fn apply_theme(&self, ctx: &egui::Context) {
        self.theme.apply(ctx);
    }
    fn select(&mut self, id: Option<XmlId>) {
        if self.draft_dirty && !self.commit_draft() {
            return;
        }
        self.multi = id.clone().into_iter().collect();
        self.selected = id.clone();
        self.draft.clear();
        self.bpmn_details = None;
        self.bpmn_loop = None;
        self.bpmn_activity = None;
        self.docs.clear();
        self.condition.clear();
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
                    self.process_id = Some(process.id.clone());
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
                    self.diagram_id = Some(diagram.id.clone());
                }
            }
            for p in processcraft_core::queries::scopes(d) {
                for n in p.nodes {
                    if Some(&n.id) == id.as_ref() {
                        self.draft = n.name.clone();
                        self.docs = n.documentation.clone();
                        self.is_node = true;
                        self.bpmn_details = n.details.clone();
                        self.bpmn_loop = n.loop_characteristics.clone();
                        self.bpmn_activity = n.activity.clone();
                        self.owner = n.owner.clone();
                        self.contact = n.contact.clone();
                    }
                }
                for lane in p.lanes {
                    if Some(&lane.id) == id.as_ref() {
                        self.draft = lane.name.clone();
                    }
                }
                for f in p.flows {
                    if Some(&f.id) == id.as_ref() {
                        self.draft = f.name.clone();
                        self.docs = f.documentation.clone();
                        self.condition = f.condition.clone().unwrap_or_default();
                        self.is_flow = true;
                        self.association_direction = f.association_direction;
                    }
                }
            }
        }
        if let Some(d) = snap.project.documents.get(self.doc)
            && let Some(c) = &d.collaboration
        {
            for participant in &c.participants {
                if Some(&participant.id) == id.as_ref() {
                    self.draft = participant.name.clone();
                }
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
        if !self.draft_dirty {
            return true;
        }
        if let Some(id) = self.selected.clone() {
            let snap = self.ctl.session.snapshot();
            if let Some(d) = snap.project.documents.get(self.doc) {
                let mut mutations = vec![Mutation::Rename {
                    document_id: d.id,
                    element_id: id.clone(),
                    name: self.draft.clone(),
                }];
                if self.is_node || self.is_flow {
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
                        condition: if self.condition.is_empty() {
                            None
                        } else {
                            Some(self.condition.clone())
                        },
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
        if self.commit_draft() && self.commit_dictionary_draft() && self.ctl.apply(request) {
            self.restore_selection();
            self.refresh_dictionary_editor();
        }
    }
    fn restore_selection(&mut self) {
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
        self.multi = ids;
        self.ctl.session.set_selection(keys);
    }
    fn has_drafts(&self) -> bool {
        self.collaboration.has_draft()
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
        if self.ctl.session.snapshot().dirty
            || self.draft_dirty
            || self.collaboration.has_draft()
            || self.m3.has_draft()
            || self.dictionary_draft_changed(&self.ctl.session.snapshot().project)
        {
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
                self.draft_dirty = false;
                self.dictionary_selected = None;
                self.dictionary_name.clear();
                self.dictionary_description.clear();
                self.doc = 0;
                self.process_id = None;
                self.diagram_id = None;
                self.select(None);
            }
            "open" => {
                let path = rfd::FileDialog::new()
                    .add_filter("ProcessCraft", &["processcraft", "pc"])
                    .pick_file();
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
        let old_session = self.ctl.session.id();
        self.ctl.open(path);
        if self.ctl.session.id() != old_session {
            #[cfg(windows)]
            {
                self.attachment = None;
            }
            self.draft_dirty = false;
            self.dictionary_selected = None;
            self.dictionary_name.clear();
            self.dictionary_description.clear();
            self.doc = 0;
            self.process_id = None;
            self.diagram_id = None;
            self.select(None);
        }
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
                .set_file_name("project.processcraft")
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
        let eid = controller::id("Node");
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
            Some(processcraft_core::bpmn_model::NodeDetails::TextAnnotation { text_format: None })
        } else if subprocess {
            Some(processcraft_core::bpmn_model::NodeDetails::SubProcess {
                content: Box::new(FlowContainer::default()),
            })
        } else if matches!(
            kind,
            NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
        ) {
            Some(processcraft_core::bpmn_model::NodeDetails::Event {
                definitions: vec![processcraft_core::bpmn_model::EventDefinition::Timer {
                    id: None,
                    kind: processcraft_core::bpmn_model::TimerKind::Duration,
                    expression: "PT1M".into(),
                }],
                attached_to: if kind == NodeKind::BoundaryEvent {
                    self.selected.clone()
                } else {
                    None
                },
                cancel_activity: (kind == NodeKind::BoundaryEvent).then_some(true),
            })
        } else {
            None
        };
        if kind == NodeKind::BoundaryEvent
            && !self
                .selected
                .as_ref()
                .is_some_and(|id| p.nodes.iter().any(|n| &n.id == id && n.kind.is_activity()))
        {
            self.ctl.error =
                "Select an activity in this scope before adding a boundary event.".into();
            return;
        }
        let parent = diagram
            .shapes
            .iter()
            .find(|s| &s.element == p.id && s.is_expanded == Some(true));
        let (x, y) = if let Some(parent) = parent {
            (
                parent.bounds.x + 40.0 + (p.nodes.len() % 2) as f64 * 140.0,
                parent.bounds.y + 55.0 + (p.nodes.len() / 2) as f64 * 90.0,
            )
        } else {
            (
                self.camera.0 + 100.0 + (p.nodes.len() % 5) as f64 * 130.0,
                self.camera.1 + 100.0 + (p.nodes.len() / 5) as f64 * 100.0,
            )
        };
        let shape = Shape {
            is_expanded: subprocess.then_some(true),
            id: controller::id("Shape"),
            element: eid.clone(),
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
            label: if event || gateway {
                Some(Label {
                    bounds: Bounds {
                        x: x + (width - 140.0) / 2.0,
                        y: y + height + 4.0,
                        width: 140.0,
                        height: 30.0,
                    },
                    style: None,
                })
            } else {
                None
            },
            style: None,
        };
        if self.ctl.mutate(vec![
            Mutation::CreateElement {
                document_id: d.id,
                process_id: p.id.clone(),
                element: Node {
                    activity: None,
                    io_specification: None,
                    loop_characteristics: None,
                    details,
                    id: eid.clone(),
                    name: label.into(),
                    documentation: String::new(),
                    kind,
                    owner: String::new(),
                    contact: String::new(),
                },
            },
            Mutation::CreateShape {
                document_id: d.id,
                diagram_id: diagram.id.clone(),
                shape,
            },
        ]) {
            self.select(Some(eid));
        }
    }
    fn add_pool(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let (Some(p), Some(diagram)) = (
            d.processes
                .iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| &p.id == id)),
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id)),
        ) else {
            return;
        };
        let participant = controller::id("Participant");
        let mut batch = Vec::new();
        let collaboration_id = d
            .collaboration
            .as_ref()
            .map_or_else(|| controller::id("Collaboration"), |c| c.id.clone());
        if d.collaboration.is_none() {
            batch.push(Mutation::CreateCollaboration {
                document_id: d.id,
                collaboration: Collaboration {
                    conversation_nodes: vec![],
                    conversation_links: vec![],
                    id: collaboration_id.clone(),
                    participants: vec![],
                    flows: vec![],
                },
            });
        }
        batch.push(Mutation::SetDiagramPlane {
            document_id: d.id,
            diagram_id: diagram.id.clone(),
            plane: collaboration_id,
        });
        batch.push(Mutation::CreateParticipant {
            document_id: d.id,
            participant: Participant {
                id: participant.clone(),
                name: "Pool".into(),
                process: Some(p.id.clone()),
            },
        });
        batch.push(Mutation::CreateShape {
            document_id: d.id,
            diagram_id: diagram.id.clone(),
            shape: Shape {
                is_expanded: None,
                id: controller::id("PoolShape"),
                element: participant,
                bounds: Bounds {
                    x: self.camera.0 + 20.0,
                    y: self.camera.1 + 60.0,
                    width: 700.0,
                    height: 350.0,
                },
                label: None,
                style: None,
            },
        });
        self.ctl.mutate(batch);
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
                process_id: p.id.clone(),
                lane: Lane {
                    child_lane_set_id: None,
                    id: lane.clone(),
                    name: "Lane".into(),
                    parent: None,
                    members: vec![],
                },
            },
            Mutation::CreateShape {
                document_id: d.id,
                diagram_id: diagram.id.clone(),
                shape: Shape {
                    is_expanded: None,
                    id: controller::id("LaneShape"),
                    element: lane,
                    bounds: Bounds {
                        x: self.camera.0 + 30.0,
                        y,
                        width: 600.0,
                        height: 150.0,
                    },
                    label: None,
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
            p.lanes.get(self.lane - 1).map(|l| l.id.clone())
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
                        diagram_id: diagram.id.clone(),
                        shape_id: s.id.clone(),
                        bounds: s.bounds.clone(),
                    })
            })
            .collect();
        self.ctl.mutate(vec![Mutation::AssignLane {
            document_id: d.id,
            process_id: p.id.clone(),
            element_id: element,
            lane_id,
            bounds,
        }]);
    }
    fn move_shapes(&mut self, items: Vec<(XmlId, Bounds)>, dx: f64, dy: f64) {
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
                    diagram_id: diagram.id.clone(),
                    shape_id,
                    bounds,
                }
            })
            .collect();
        match processcraft_core::geometry::prepare_move(d, &diagram.id, bounds) {
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
        let items = diagram
            .shapes
            .iter()
            .filter(|s| self.multi.contains(&s.element))
            .map(|s| (s.id.clone(), s.bounds.clone()))
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
        let Some(flow) = p
            .flows
            .iter()
            .find(|f| Some(&f.id) == self.selected.as_ref())
        else {
            return;
        };
        let (Some(a), Some(b), Some(edge)) = (
            diagram.shapes.iter().find(|s| s.element == flow.source),
            diagram.shapes.iter().find(|s| s.element == flow.target),
            diagram.edges.iter().find(|e| e.element == flow.id),
        ) else {
            return;
        };
        let waypoints = vec![
            Point {
                x: a.bounds.x + a.bounds.width,
                y: a.bounds.y + a.bounds.height / 2.0,
            },
            Point {
                x: b.bounds.x,
                y: b.bounds.y + b.bounds.height / 2.0,
            },
        ];
        self.ctl.mutate(vec![Mutation::Reroute {
            document_id: d.id,
            diagram_id: diagram.id.clone(),
            edge_id: edge.id.clone(),
            waypoints,
        }]);
        self.ctl.error = "Direct path applied; check for overlaps.".into();
    }
    fn connect(&mut self) {
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
        let (Some(a), Some(b)) = (p.nodes.get(self.source), p.nodes.get(self.target)) else {
            return;
        };
        if a.id == b.id {
            self.ctl.error = "Choose two different nodes.".into();
            return;
        }
        let (Some(sa), Some(sb)) = (
            diagram.shapes.iter().find(|s| s.element == a.id),
            diagram.shapes.iter().find(|s| s.element == b.id),
        ) else {
            return;
        };
        let eid = controller::id("Flow");
        let points = vec![
            Point {
                x: sa.bounds.x + sa.bounds.width,
                y: sa.bounds.y + sa.bounds.height / 2.0,
            },
            Point {
                x: sb.bounds.x,
                y: sb.bounds.y + sb.bounds.height / 2.0,
            },
        ];
        self.ctl.mutate(vec![
            Mutation::CreateFlow {
                document_id: d.id,
                process_id: Some(p.id.clone()),
                flow: Flow {
                    association_direction: None,
                    id: eid.clone(),
                    kind: self.connection_kind,
                    source: a.id.clone(),
                    target: b.id.clone(),
                    name: String::new(),
                    documentation: String::new(),
                    condition: None,
                },
            },
            Mutation::CreateEdge {
                document_id: d.id,
                diagram_id: diagram.id.clone(),
                edge: Edge {
                    id: controller::id("Edge"),
                    element: eid,
                    waypoints: points,
                    label: None,
                    style: None,
                },
            },
        ]);
        self.ctl.error = "Connection uses a direct path; check for overlaps.".into();
    }
    fn delete(&mut self) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let mut ids = self.multi.clone();
        ids.sort_by_key(|id| {
            if processcraft_core::queries::find_flow(d, id).is_some() {
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
            .map(|element_id| Mutation::Delete {
                document_id: d.id,
                element_id,
            })
            .collect();
        if self.ctl.mutate(batch) {
            self.select(None);
        }
    }
}
impl eframe::App for ProcessCraft {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        let ctx = ui.ctx().clone();
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
                        || self.draft_dirty
                        || self.collaboration.has_draft()
                        || self.m3.has_draft()
                        || self.dictionary_draft_changed(&self.ctl.session.snapshot().project)
                    {
                        self.ctl.error =
                            "Import cancelled because the project changed during preparation."
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
                crate::effects::ResultValue::Written(message)
                | crate::effects::ResultValue::Failed(message) => self.ctl.error = message,
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
        let problem_key = (self.ctl.session.id(), snapshot.generation);
        if self.problem_key.as_ref() != Some(&problem_key) {
            self.problems = snapshot.project.diagnostics();
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
        if !attachment_save_pending
            && let Some(success) = self.ctl.poll_save()
            && self.after_save.is_some()
        {
            self.after_save_ready = Some(success);
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
            && (self.ctl.session.snapshot().dirty
                || self.draft_dirty
                || self.collaboration.has_draft()
                || self.m3.has_draft()
                || self.dictionary_draft_changed(&self.ctl.session.snapshot().project))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some("close");
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
        }) {
            ctx.set_zoom_factor(2.0);
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0)
        }) {
            ctx.set_zoom_factor(1.0);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.new_or_open("open");
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.new_or_open("new");
        }
        if self.ctl.uncertain && ui.button("Check save outcome").clicked() {
            self.ctl.reconcile();
        }
        if !self.ctl.error.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(self.theme.palette().warning, &self.ctl.error);
                if ui.small_button("Dismiss message").clicked() {
                    self.ctl.error.clear();
                }
            });
        }
        if let Some(lost) = &self.export_lost {
            egui::Window::new("Export outcome unknown").collapsible(false).show(&ctx, |ui| {
                ui.label("The export worker stopped and the write outcome cannot be established automatically. Inspect the destination and retained artifacts; this operation cannot be safely retried or dismissed.");
                ui.label(format!("Target: {} · session: {} · operation: {} · SHA-256: {:02x?} · {} bytes", lost.path.display(), lost.session, lost.operation, lost.digest, lost.bytes.len()));
            });
        }
        if let Some(pending) = &self.export_uncertain {
            let message = format!(
                "{}; destination: {}; stage: {}; backup: {}",
                pending.error,
                pending.path.display(),
                pending.stage.display(),
                pending.backup.display()
            );
            egui::Window::new("Export outcome uncertain")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label(message);
                    ui.label("Inspect the destination and retained files, then check the outcome.");
                    if ui.button("Check export outcome").clicked()
                        && let Some(pending) = self.export_uncertain.take()
                    {
                        self.effects.reconcile(ctx.clone(), pending);
                    }
                });
        }
        if let Some((_, report, extension)) = &self.export_ready {
            let report = report.clone();
            let extension = *extension;
            egui::Window::new("Review export").collapsible(false).show(&ctx, |ui| {
                if report.is_empty() { ui.label("Export is ready."); } else { ui.label("Review and accept these export notes:"); for note in &report { ui.label(note); } }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { self.export_ready = None; }
                    if ui.button("Accept and choose destination…").clicked() && let Some(path) = rfd::FileDialog::new().add_filter(extension, &[extension]).set_file_name(format!("diagram.{extension}")).save_file() {
                        if self.ctl.is_saving() { self.ctl.error = "Wait for the project save to finish before exporting.".into(); }
                        else {
                            match processcraft_io::FileGrant::new(&path, processcraft_io::Access::Write).and_then(|grant| {
                                if self.ctl.file.as_ref().map(|file| file.same_target(&grant)).transpose()?.unwrap_or(false) {
                                    return Err(std::io::Error::other("Choose a destination other than the open project file."));
                                }
                                Ok(grant)
                            }) {
                                Ok(grant) => if let Some((bytes, _, _)) = self.export_ready.take() { self.effects.write(ctx.clone(), grant, bytes, self.ctl.session.id()); },
                                Err(error) => self.ctl.error = format!("Cannot use export destination: {error}"),
                            }
                        }
                    }
                });
            });
        }
        self.workbench_ui(ui);
        if self.recovery_visible && !self.recovery_records.is_empty() {
            egui::Window::new("Recover unsaved work")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label(
                        "Recovered work opens as an unsaved project. Choose Save As to keep it.",
                    );
                    for record in self.recovery_records.clone() {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} · {} · revision {}",
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
                            if ui.button("Restore").clicked() && self.can_restore_recovery() {
                                match processcraft_io::recovery::RecoveryStore::new(
                                    crate::jobs::recovery_location(),
                                )
                                .and_then(|s| s.open(&record))
                                {
                                    Ok(project) => {
                                        match processcraft_core::commands::Session::new(project) {
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
                                                self.draft_dirty = false;
                                                self.select(None);
                                                self.render_key.clear();
                                                self.recovery_visible = false;
                                            }
                                            Err(e) => {
                                                self.ctl.error =
                                                    format!("Cannot recover project: {e:?}")
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.ctl.error = format!("Cannot recover project: {e}")
                                    }
                                }
                            }
                            if ui.button("Discard recovery").clicked() {
                                match processcraft_io::recovery::RecoveryStore::new(
                                    crate::jobs::recovery_location(),
                                )
                                .and_then(|s| s.discard(&record))
                                {
                                    Ok(()) => self.recovery_records.retain(|r| r.id != record.id),
                                    Err(e) => {
                                        self.ctl.error = format!("Cannot discard recovery: {e}")
                                    }
                                }
                            }
                        });
                    }
                    if ui.button("Later").clicked() {
                        self.recovery_visible = false;
                    }
                });
        }
        if let Some(action) = self.confirm {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label("Save changes before continuing?");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() && self.save(false) {
                            self.confirm = None;
                            self.after_save = Some(action);
                        }
                        if ui.button("Discard").clicked()
                            && let Some(action) = self.discard_transition()
                        {
                            if action == "close" {
                                self.allow_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                self.execute(action);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm = None;
                        }
                    });
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            process_id: Some(controller::id("Process")),
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
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: node.clone(),
            name: "Task".into(),
            documentation: String::new(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        });
        doc.diagrams[0].shapes.push(Shape {
            is_expanded: None,
            id: shape.clone(),
            element: node,
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
                            .map(|s| (s.id.clone(), s.bounds.clone()))
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
                                    diagram_id: diagram.id.clone(),
                                    shape_id,
                                    bounds,
                                }
                            })
                            .collect();
                        let prepared =
                            processcraft_core::geometry::prepare_move(d, &diagram.id, bounds)
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
            id: "Diagram".parse().unwrap(),
            plane_id: "Plane".parse().unwrap(),
            plane: "Process".parse().unwrap(),
            shapes: vec![Shape {
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
                    bounds: Bounds {
                        x: 0.0,
                        y: 74.0,
                        width: 140.0,
                        height: 30.0,
                    },
                    style: None,
                }),
                style: None,
            }],
            edges: vec![Edge {
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
            definitions: vec![EventDefinition::Timer {
                id: None,
                kind: TimerKind::Duration,
                expression: "PT5M".into(),
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
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression == "PT1M"))
        );
        app.history(Request::Redo);
        assert!(
            matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression == "PT5M"))
        );
    }
}

#[path = "app_m2.rs"]
mod m2;

#[path = "app_canvas.rs"]
mod canvas;
#[path = "app_inspector.rs"]
mod inspector;
#[path = "app_shell.rs"]
mod shell;

#[path = "app_tree.rs"]
mod tree;

#[path = "app_m3.rs"]
mod m3;

#[path = "app_collaboration.rs"]
mod collaboration;
