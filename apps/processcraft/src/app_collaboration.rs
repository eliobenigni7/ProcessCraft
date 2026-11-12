use super::*;
use processcraft_core::bpmn_collaboration::{ConversationKind, ConversationLink, ConversationNode};

pub(super) fn fresh_choreography(
    ids: [XmlId; 4],
    names: [String; 4],
) -> processcraft_core::bpmn_collaboration::Choreography {
    let [root, task, first, second] = ids;
    let [root_name, task_name, first_name, second_name] = names;
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyFlowElement, ChoreographyGraph,
    };
    let participant = |id: XmlId, name: String| Participant {
        id: id.into(),
        name: Some(name),
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        process: None,
    };
    let activity = ChoreographyActivity {
        id: task.into(),
        name: Some(task_name),
        loop_type: None,
        details: None,
        correlation_keys: vec![],
        flow_metadata: None,
        base: None,
        participant_refs: vec![
            ModelReference::Local(first.clone().into()),
            ModelReference::Local(second.clone().into()),
        ],
        initiating_participant: Some(ModelReference::Local(first.clone().into())),
        message_flow_refs: vec![],
    };
    Choreography {
        kind: None,
        graph: Some(ChoreographyGraph {
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(activity))],
            ..Default::default()
        }),
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: root.into(),
        name: Some(root_name),
        participants: vec![
            participant(first, first_name),
            participant(second, second_name),
        ],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    }
}

pub(super) fn update_root(document: &BpmnDocument, root: Collaboration) -> Mutation {
    if let Some(existing) = document
        .choreographies
        .iter()
        .find(|choreography| choreography.id == root.id)
    {
        let mut choreography = existing.clone();
        choreography.name = root.name;
        choreography.base = root.base;
        choreography.participants = root.participants;
        choreography.message_flows = root.flows;
        choreography.conversation_nodes = root.conversation_nodes;
        choreography.conversation_links = root.conversation_links;
        choreography.correlation_keys = root.correlation_keys;
        choreography.extras = root.extras;
        Mutation::UpdateChoreography {
            document_id: document.id,
            choreography,
        }
    } else {
        Mutation::SetCollaborationRoot {
            document_id: document.id,
            collaboration: root,
        }
    }
}
pub(super) fn choreography_view(
    value: &processcraft_core::bpmn_collaboration::Choreography,
) -> Collaboration {
    Collaboration {
        kind: None,
        extras: value.extras.clone(),
        correlation_keys: value.correlation_keys.clone(),
        base: value.base.clone(),
        name: value.name.clone(),
        id: value.id.clone(),
        participants: value.participants.clone(),
        flows: value.message_flows.clone(),
        conversation_nodes: value.conversation_nodes.clone(),
        conversation_links: value.conversation_links.clone(),
    }
}

fn conversation_rows(nodes: &[ConversationNode]) -> Vec<(usize, &ConversationNode)> {
    fn visit<'a>(
        nodes: &'a [ConversationNode],
        parent: Option<&ElementSelector>,
        depth: usize,
        seen: &mut std::collections::HashSet<ElementSelector>,
        out: &mut Vec<(usize, &'a ConversationNode)>,
    ) {
        if depth >= 256 {
            return;
        }
        for node in nodes.iter().filter(|node| node.parent.as_ref() == parent) {
            let id = node.id.selector();
            if seen.insert(id.clone()) {
                out.push((depth, node));
                visit(nodes, Some(&id), depth.saturating_add(1), seen, out);
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit(nodes, None, 0, &mut seen, &mut out);
    for node in nodes {
        if seen.insert(node.id.selector()) {
            out.push((0, node));
        }
    }
    out
}

fn valid_conversation_parent(
    nodes: &[ConversationNode],
    current: Option<&ElementSelector>,
    candidate: &ConversationNode,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    let mut cursor = Some(candidate.id.selector());
    for _ in 0..=nodes.len() {
        let Some(id) = cursor else {
            return true;
        };
        if &id == current {
            return false;
        }
        cursor = nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.parent.clone());
    }
    false
}

fn choreography_task_shapes(
    task: ElementSelector,
    initiating: ElementSelector,
    other: ElementSelector,
    bounds: Bounds,
) -> Vec<Shape> {
    let owner: processcraft_core::ids::ElementIdentity = controller::id("ChoreographyShape").into();
    let band = |participant, kind, y| Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: Some(kind),
        choreography_activity_shape: Some(DiReference::Local(owner.selector())),
        label_visibility: Some(DiLabelVisibility::Explicit),
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: controller::id("ParticipantBand").into(),
        element: participant,
        bounds: Bounds {
            x: bounds.x,
            y,
            width: bounds.width,
            height: 25.0,
        },
        label: Some(Label::default()),
        style: None,
    };
    let top = band(initiating, ParticipantBandKind::TopInitiating, bounds.y);
    let bottom = band(
        other,
        ParticipantBandKind::BottomNonInitiating,
        bounds.y + bounds.height - 25.0,
    );
    vec![
        Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: Some(DiLabelVisibility::Explicit),
            is_horizontal: None,
            is_marker_visible: None,
            id: owner,
            element: task,
            is_expanded: None,
            bounds,
            label: Some(Label::default()),
            style: None,
        },
        top,
        bottom,
    ]
}

#[derive(Default)]
pub(super) struct State {
    session: String,
    open: bool,
    pub(super) partner_catalog: Option<super::partners::PartnerCatalogDraft>,
    pub(super) conversation_details: Option<super::conversation_details::ConversationDetailsDraft>,
    pub(super) associations: Option<super::collaboration_associations::AssociationsDraft>,
    pub(super) participant: Option<super::collaboration_participant::ParticipantDraft>,
    pub(super) connection: Option<super::collaboration_connection::ConnectionDraft>,
    pub(super) artifacts: Option<super::collaboration_artifacts::ArtifactsDraft>,
    pub(super) document: Option<processcraft_core::ids::DocumentId>,
    pub(super) root: Option<ElementSelector>,
    pub(super) root_details: Option<super::collaboration_root::RootDraft>,
    pub(super) root_order: Option<super::collaboration_root::RootOrderDraft>,
    node_id: Option<ElementSelector>,
    name: String,
    name_present: bool,
    is_subconversation: bool,
    original_kind: Option<ConversationKind>,
    kind_changed: bool,
    parent: Option<ElementSelector>,
    participants: Vec<ElementSelector>,
    original_participants: Vec<processcraft_core::bpmn_activity::ModelReference>,
    link_participant: Option<ElementSelector>,
    link_conversation: Option<ElementSelector>,
    dirty: bool,
    draft_generation: Option<u64>,
    link_dirty: bool,
    link_generation: Option<u64>,
    data_name: String,
    data_store: bool,
    data_dirty: bool,
    reference_name: String,
    reference_target: Option<ElementSelector>,
    reference_store: bool,
    reference_dirty: bool,
    io_name: String,
    io_activity: Option<ElementSelector>,
    io_reference: Option<ElementSelector>,
    io_output: bool,
    io_dirty: bool,
    choreography_id: Option<ElementSelector>,
    choreography_name: String,
    choreography_first: Option<ElementSelector>,
    choreography_second: Option<ElementSelector>,
    choreography_dirty: bool,
    root_name: String,
    root_first: String,
    root_second: String,
    root_task: String,
    root_dirty: bool,
    category_name: String,
    category_value: String,
    group_dirty: bool,
    draw: bool,
}
impl State {
    pub(super) fn remap_selectors(
        &mut self,
        mappings: &[processcraft_core::commands::SelectorMapping],
    ) {
        for mapping in mappings
            .iter()
            .filter(|mapping| Some(mapping.document_id) == self.document)
        {
            for selector in [
                &mut self.root,
                &mut self.node_id,
                &mut self.parent,
                &mut self.link_participant,
                &mut self.link_conversation,
                &mut self.reference_target,
                &mut self.io_activity,
                &mut self.io_reference,
                &mut self.choreography_id,
                &mut self.choreography_first,
                &mut self.choreography_second,
            ] {
                if selector.as_ref() == Some(&mapping.from) {
                    *selector = Some(mapping.to.clone());
                }
            }
            for participant in &mut self.participants {
                if *participant == mapping.from {
                    *participant = mapping.to.clone();
                }
            }
        }
    }
    pub(super) fn has_draft(&self) -> bool {
        self.partner_catalog.is_some()
            || self.conversation_details.is_some()
            || self.associations.is_some()
            || self.participant.is_some()
            || self.connection.is_some()
            || self.artifacts.is_some()
            || self.root_details.is_some()
            || self.root_order.is_some()
            || self.dirty
            || self.link_dirty
            || self.data_dirty
            || self.reference_dirty
            || self.io_dirty
            || self.choreography_dirty
            || self.root_dirty
            || self.group_dirty
    }
    pub(super) fn clear_node(&mut self) {
        self.node_id = None;
        self.name.clear();
        self.name_present = false;
        self.is_subconversation = false;
        self.original_kind = None;
        self.kind_changed = false;
        self.parent = None;
        self.participants.clear();
        self.original_participants.clear();
        self.dirty = false;
        self.draft_generation = None;
    }
    fn clear(&mut self) {
        self.clear_node();
        self.partner_catalog = None;
        self.conversation_details = None;
        self.associations = None;
        self.participant = None;
        self.connection = None;
        self.artifacts = None;
        self.root_details = None;
        self.root_order = None;
        self.link_participant = None;
        self.link_conversation = None;
        self.link_dirty = false;
        self.link_generation = None;
        self.data_name.clear();
        self.data_dirty = false;
        self.reference_name.clear();
        self.reference_target = None;
        self.reference_dirty = false;
        self.io_name.clear();
        self.io_activity = None;
        self.io_reference = None;
        self.io_dirty = false;
        self.choreography_id = None;
        self.choreography_name.clear();
        self.choreography_first = None;
        self.choreography_second = None;
        self.choreography_dirty = false;
        self.root_name.clear();
        self.root_first.clear();
        self.root_second.clear();
        self.root_task.clear();
        self.root_dirty = false;
        self.category_name.clear();
        self.category_value.clear();
        self.group_dirty = false;
    }
}
impl ProcessCraft {
    pub(super) fn open_collaboration(&mut self) {
        self.collaboration.open = true;
    }
    pub(super) fn collaboration_windows(&mut self, ctx: &egui::Context) {
        let session = self.ctl.session.id();
        if self.collaboration.session != session {
            let open = self.collaboration.open;
            self.collaboration = State {
                session,
                open,
                draw: true,
                ..Default::default()
            };
        }
        if !self.collaboration.open {
            return;
        }
        let mut open = self.collaboration.open;
        egui::Window::new("Collaboration and data")
            .open(&mut open)
            .default_width(680.0)
            .max_width((ctx.content_rect().width() - 32.0).max(150.0))
            .default_height(520.0)
            .max_height((ctx.content_rect().height() - 64.0).max(100.0))
            .vscroll(true)
            .show(ctx, |ui| self.collaboration_contents(ui));
        self.collaboration.open = open;
    }
    fn collaboration_contents(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let Some(doc) = snap.project.documents.get(self.doc) else {
            ui.label("Open a model to manage collaboration.");
            return;
        };
        let id = doc.id;
        ui.strong(&doc.name);
        if self.collaboration.document.is_some_and(|bound| bound != id)
            && self.collaboration.has_draft()
        {
            ui.label("Return to the model with this draft, or discard it before continuing.");
            if ui.button("Discard draft").clicked() {
                self.collaboration.clear();
                self.collaboration.document = None;
            }
            return;
        }
        if !doc.capability.allows_structural_editing() {
            ui.label("This model does not permit structural editing.");
            return;
        }
        self.partner_catalog_ui(ui, doc, snap.generation);
        self.collaboration_data(ui, doc);
        self.collaboration_groups(ui, doc);
        self.collaboration_choreography(ui, doc, snap.generation);
        ui.separator();
        let roots: Vec<_> = doc
            .collaboration
            .iter()
            .chain(doc.additional_collaborations.iter())
            .cloned()
            .chain(doc.choreographies.iter().map(choreography_view))
            .collect();
        if self
            .collaboration
            .root
            .as_ref()
            .is_none_or(|selected| !roots.iter().any(|root| root.id == *selected))
        {
            self.collaboration.root = roots.first().map(|root| root.id.selector());
        }
        let label = |root: &Collaboration, index: usize| {
            let kind = if doc
                .choreographies
                .iter()
                .any(|choreography| choreography.id == root.id)
            {
                "Choreography"
            } else if root.kind.is_some() {
                "Global conversation"
            } else {
                "Collaboration"
            };
            super::event::local_choice_label(root.name.as_deref(), kind, &root.id, index + 1)
        };
        let selected_name = roots
            .iter()
            .enumerate()
            .find(|(_, root)| {
                self.collaboration
                    .root
                    .as_ref()
                    .is_some_and(|id| root.id == *id)
            })
            .map_or_else(
                || "Select a collaboration".to_owned(),
                |(index, root)| label(root, index),
            );
        let mut chosen = self.collaboration.root.clone();
        egui::ComboBox::from_label("Collaboration")
            .selected_text(selected_name)
            .show_ui(ui, |ui| {
                for (index, root) in roots.iter().enumerate() {
                    ui.selectable_value(&mut chosen, Some(root.id.selector()), label(root, index));
                }
            });
        if chosen != self.collaboration.root {
            if self.collaboration.has_draft() {
                self.ctl.error =
                    "Apply or discard the current draft before switching collaboration.".into();
            } else {
                self.collaboration.clear_node();
                self.collaboration.root = chosen;
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (name, kind) in [("Create collaboration", None), ("Create global conversation", Some(processcraft_core::bpmn_collaboration::CollaborationRootKind::GlobalConversation))] {
                if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(name)).clicked() && self.commit_draft() {
                    let identity = processcraft_core::ids::ElementIdentity::anonymous();
                    let root = Collaboration { kind, extras: None, correlation_keys: vec![], base: None, name: None, id: identity.clone(), participants: vec![], flows: vec![], conversation_nodes: vec![], conversation_links: vec![] };
                    if self.ctl.mutate(vec![Mutation::SetCollaborationRoot { document_id: id, collaboration: root }]) { self.collaboration.root = Some(identity.selector()); }
                }
            }
        });
        let Some(collaboration) = roots.iter().find(|root| {
            self.collaboration
                .root
                .as_ref()
                .is_some_and(|selected| root.id == *selected)
        }) else {
            ui.label("Create a collaboration to manage conversations.");
            return;
        };
        self.collaboration_root_ui(ui, doc, collaboration, snap.generation);
        self.collaboration_participants_ui(ui, doc, collaboration, snap.generation);
        self.collaboration_associations_ui(ui, doc, collaboration, snap.generation);
        self.collaboration_connections_ui(ui, doc, collaboration, snap.generation);
        self.collaboration_artifacts_ui(ui, doc, collaboration, snap.generation, None);
        ui.label("Conversations");
        if collaboration.kind.is_some() {
            ui.label("Global conversations do not contain conversation nodes; such entries must be removed before BPMN export.");
        }
        if collaboration.participants.is_empty() {
            ui.label("Add a pool to create a participant.");
        }
        egui::ScrollArea::vertical()
            .id_salt("conversation_list")
            .max_height(130.0)
            .show(ui, |ui| {
                for (depth, node) in conversation_rows(&collaboration.conversation_nodes) {
                    let name = node
                        .name
                        .as_deref()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("Unnamed conversation");
                    let kind = match node.kind {
                        ConversationKind::Conversation => "Conversation",
                        ConversationKind::SubConversation => "Sub-conversation",
                        ConversationKind::CallConversation => "Call conversation",
                    };
                    let label = format!("{}{} · {}", "  ".repeat(depth.min(8)), name, kind);
                    if ui
                        .selectable_label(
                            self.collaboration.node_id.as_ref() == Some(&node.id.selector()),
                            label,
                        )
                        .on_hover_text(
                            node.id
                                .xml_id()
                                .map_or_else(|| "No XML ID".to_owned(), ToString::to_string),
                        )
                        .clicked()
                        && !self.collaboration.has_draft()
                    {
                        self.collaboration.document = Some(id);
                        self.collaboration.node_id = Some(node.id.selector());
                        self.collaboration.name_present = node.name.is_some();
                        self.collaboration.name = node.name.clone().unwrap_or_default();
                        self.collaboration.is_subconversation =
                            matches!(node.kind, ConversationKind::SubConversation);
                        self.collaboration.original_kind = Some(node.kind);
                        self.collaboration.kind_changed = false;
                        self.collaboration.parent = node.parent.clone();
                        self.collaboration.participants = node
                            .participant_refs
                            .iter()
                            .filter_map(|r| match r {
                                processcraft_core::bpmn_activity::ModelReference::Local(id) => {
                                    Some(id.clone())
                                }
                                _ => None,
                            })
                            .collect();
                        self.collaboration.original_participants = node.participant_refs.clone();
                    }
                }
            });
        ui.horizontal(|ui| {
            if ui.button("New conversation").clicked() && !self.collaboration.has_draft() {
                self.collaboration.clear_node();
                self.collaboration.document = Some(id);
            }
            if self.collaboration.dirty {
                ui.label("Save or discard this draft before switching.");
            }
        });
        ui.separator();
        let label = ui.checkbox(&mut self.collaboration.name_present, "Name");
        let changed = label.changed()
            || (self.collaboration.name_present
                && ui
                    .text_edit_singleline(&mut self.collaboration.name)
                    .labelled_by(label.id)
                    .changed());
        if changed {
            self.collaboration.dirty = true;
            self.collaboration.document = Some(id);
        }
        if ui
            .checkbox(
                &mut self.collaboration.is_subconversation,
                "Sub-conversation",
            )
            .changed()
        {
            self.collaboration.dirty = true;
            self.collaboration.kind_changed = true;
            self.collaboration.document = Some(id);
        }
        let current_parent = self
            .collaboration
            .parent
            .as_ref()
            .and_then(|id| {
                collaboration
                    .conversation_nodes
                    .iter()
                    .find(|n| &n.id == id)
            })
            .map_or("None", |n| n.name.as_deref().unwrap_or("Unnamed"));
        egui::ComboBox::from_id_salt("conversation_parent")
            .selected_text(current_parent)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut self.collaboration.parent, None, "None")
                    .changed()
                {
                    self.collaboration.dirty = true;
                    self.collaboration.document = Some(id);
                }
                for parent in collaboration.conversation_nodes.iter().filter(|n| {
                    matches!(n.kind, ConversationKind::SubConversation)
                        && valid_conversation_parent(
                            &collaboration.conversation_nodes,
                            self.collaboration.node_id.as_ref(),
                            n,
                        )
                }) {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.parent,
                            Some(parent.id.selector()),
                            parent.name.as_deref().unwrap_or("Unnamed"),
                        )
                        .changed()
                    {
                        self.collaboration.dirty = true;
                        self.collaboration.document = Some(id);
                    }
                }
            });
        ui.label("Participants");
        for participant in &collaboration.participants {
            let mut selected = self
                .collaboration
                .participants
                .contains(&participant.id.selector());
            if ui
                .checkbox(
                    &mut selected,
                    participant.name.as_deref().unwrap_or("Unnamed"),
                )
                .changed()
            {
                if selected {
                    self.collaboration
                        .participants
                        .push(participant.id.selector());
                } else {
                    self.collaboration
                        .participants
                        .retain(|v| v != &participant.id);
                }
                self.collaboration.dirty = true;
                self.collaboration.document = Some(id);
            }
        }
        if self.collaboration.node_id.is_none() {
            ui.checkbox(&mut self.collaboration.draw, "Add to current diagram");
        }
        if self.collaboration.dirty && self.collaboration.draft_generation.is_none() {
            self.collaboration.draft_generation = Some(snap.generation);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save conversation").clicked() {
                if self.collaboration.draft_generation.is_some_and(|generation| generation != self.ctl.session.snapshot().generation) { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); return; }
                let identity = if let Some(id) = &self.collaboration.node_id {
                    let Some(node) = collaboration.conversation_nodes.iter().find(|node| node.id == *id) else { self.ctl.error = "The conversation no longer exists.".into(); return; };
                    node.id.clone()
                } else { processcraft_core::ids::ElementIdentity::anonymous() };
                let node_id = identity.selector();
                let existing = collaboration
                    .conversation_nodes
                    .iter()
                    .find(|node| node.id.selector() == node_id);
                let node = ConversationNode {
                    message_flow_refs: existing
                        .map_or_else(Vec::new, |node| node.message_flow_refs.clone()),
                    call: existing.and_then(|node| node.call.clone()),
                    correlation_keys: collaboration
                        .conversation_nodes
                        .iter()
                        .find(|node| node.id.selector() == node_id)
                        .map_or_else(Vec::new, |node| node.correlation_keys.clone()),
                    base: collaboration
                        .conversation_nodes
                        .iter()
                        .find(|node| node.id.selector() == node_id)
                        .and_then(|node| node.base.clone()),
                    id: identity,
                    name: self
                        .collaboration
                        .name_present
                        .then(|| self.collaboration.name.clone()),
                    kind: if !self.collaboration.kind_changed {
                        self.collaboration
                            .original_kind
                            .unwrap_or(ConversationKind::Conversation)
                    } else if self.collaboration.is_subconversation {
                        ConversationKind::SubConversation
                    } else {
                        ConversationKind::Conversation
                    },
                    participant_refs: {
                        use processcraft_core::bpmn_activity::ModelReference;
                        let mut refs: Vec<_> = self
                            .collaboration
                            .original_participants
                            .iter()
                            .filter(|r| match r {
                                ModelReference::Local(id) => {
                                    self.collaboration.participants.contains(id)
                                }
                                ModelReference::Qualified(_) => true,
                            })
                            .cloned()
                            .collect();
                        for id in &self.collaboration.participants {
                            let item = ModelReference::Local(id.clone());
                            if !refs.contains(&item) {
                                refs.push(item);
                            }
                        }
                        refs
                    },
                    parent: self.collaboration.parent.clone(),
                };
                let mut updated = collaboration.clone();
                if let Some(existing) = updated.conversation_nodes.iter_mut().find(|existing| existing.id.selector() == node_id) { *existing = node; } else { updated.conversation_nodes.push(node); }
                let mut mutations = vec![update_root(doc, updated)];
                if self.collaboration.node_id.is_none() && self.collaboration.draw {
                    let Some(diagram) = doc.diagrams.iter().find(|d| {
                        self.diagram_id
                            .as_ref()
                            .is_none_or(|selected| &d.id == selected)
                    }) else {
                        self.ctl.error = "The model has no current diagram.".into();
                        return;
                    };
                    if diagram.plane != collaboration.id {
                        self.ctl.error = "Choose a diagram for this collaboration before adding a representation.".into();
                        return;
                    }
                    let count = collaboration.conversation_nodes.len() as f64;
                    mutations.push(Mutation::CreateShape {
                        document_id: id,
                        diagram_id: diagram.id.selector(),
                        shape: Shape {
                            di_metadata: None,
                            is_message_visible: None,
                            participant_band_kind: None,
                            choreography_activity_shape: None,
                            label_visibility: Some(DiLabelVisibility::Explicit),
                            is_horizontal: None,
                            is_marker_visible: None,
                            id: controller::id("ConversationShape").into(),
                            element: node_id.clone(),
                            is_expanded: None,
                            bounds: Bounds {
                                x: self.camera.0 + 120.0,
                                y: self.camera.1 + 80.0 + count * 90.0,
                                width: 120.0,
                                height: 70.0,
                            },
                            label: Some(Label::default()),
                            style: None,
                        },
                    });
                }
                if self.commit_draft() && self.ctl.mutate(mutations) {
                    self.collaboration.node_id = Some(node_id);
                    self.collaboration.document = Some(id);
                    self.collaboration.dirty = false;
                    self.collaboration.draft_generation = None;
                }
            }
            if ui.button("Discard draft").clicked() {
                self.collaboration.clear_node();
                self.collaboration.document = Some(id);
            }
            if let Some(node_id) = &self.collaboration.node_id
                && !self.collaboration.has_draft()
                && ui.button("Delete conversation").clicked()
                && { let mut updated = collaboration.clone(); updated.conversation_nodes.retain(|node| node.id != *node_id); self.ctl.mutate(vec![update_root(doc, updated)]) }
            {
                self.collaboration.clear_node();
            }
        });
        if let Some(node) = self.collaboration.node_id.as_ref().and_then(|id| {
            collaboration
                .conversation_nodes
                .iter()
                .find(|node| node.id == *id)
        }) {
            if ui
                .add_enabled(
                    !self.collaboration.has_draft(),
                    egui::Button::new("Select conversation properties"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.select(Some(node.id.selector()));
            }
            self.conversation_details_ui(ui, doc, collaboration, node, snap.generation);
        }
        ui.separator();
        ui.strong("Conversation links");
        ui.checkbox(&mut self.collaboration.draw, "Add links to current diagram");
        for link in &collaboration.conversation_links {
            let participant = collaboration
                .participants
                .iter()
                .find(|p| {
                    link.source.as_ref().is_some_and(|reference| {
                        reference.matches_identity(
                            &p.id,
                            doc.definitions
                                .as_ref()
                                .map(|d| d.target_namespace.as_str()),
                        )
                    }) || link.target.as_ref().is_some_and(|reference| {
                        reference.matches_identity(
                            &p.id,
                            doc.definitions
                                .as_ref()
                                .map(|d| d.target_namespace.as_str()),
                        )
                    })
                })
                .map_or("Unknown participant", |p| {
                    p.name.as_deref().unwrap_or("Unnamed")
                });
            let node = collaboration
                .conversation_nodes
                .iter()
                .find(|n| {
                    link.source.as_ref().is_some_and(|reference| {
                        reference.matches_identity(
                            &n.id,
                            doc.definitions
                                .as_ref()
                                .map(|d| d.target_namespace.as_str()),
                        )
                    }) || link.target.as_ref().is_some_and(|reference| {
                        reference.matches_identity(
                            &n.id,
                            doc.definitions
                                .as_ref()
                                .map(|d| d.target_namespace.as_str()),
                        )
                    })
                })
                .map_or("Unknown conversation", |n| {
                    n.name.as_deref().unwrap_or("Unnamed")
                });
            ui.horizontal(|ui| {
                ui.label(format!("{participant} / {node}"));
                if !self.collaboration.has_draft() && ui.button("Delete link").clicked() {
                    let mut updated = collaboration.clone();
                    updated
                        .conversation_links
                        .retain(|candidate| candidate.id != link.id);
                    self.ctl.mutate(vec![update_root(doc, updated)]);
                }
            });
        }
        egui::ComboBox::from_id_salt("link_participant")
            .selected_text(
                self.collaboration
                    .link_participant
                    .as_ref()
                    .and_then(|id| collaboration.participants.iter().find(|p| &p.id == id))
                    .map_or("Select participant", |p| {
                        p.name.as_deref().unwrap_or("Unnamed")
                    }),
            )
            .show_ui(ui, |ui| {
                for participant in &collaboration.participants {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.link_participant,
                            Some(participant.id.selector()),
                            participant.name.as_deref().unwrap_or("Unnamed"),
                        )
                        .changed()
                    {
                        self.collaboration.link_dirty = true;
                        self.collaboration.document = Some(id);
                    }
                }
            });
        egui::ComboBox::from_id_salt("link_conversation")
            .selected_text(
                self.collaboration
                    .link_conversation
                    .as_ref()
                    .and_then(|id| {
                        collaboration
                            .conversation_nodes
                            .iter()
                            .find(|n| &n.id == id)
                    })
                    .map_or("Select conversation", |n| {
                        n.name.as_deref().unwrap_or("Unnamed")
                    }),
            )
            .show_ui(ui, |ui| {
                for node in &collaboration.conversation_nodes {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.link_conversation,
                            Some(node.id.selector()),
                            node.name.as_deref().unwrap_or("Unnamed"),
                        )
                        .changed()
                    {
                        self.collaboration.link_dirty = true;
                        self.collaboration.document = Some(id);
                    }
                }
            });
        if self.collaboration.link_dirty && self.collaboration.link_generation.is_none() {
            self.collaboration.link_generation = Some(snap.generation);
        }
        if self.collaboration.link_dirty && ui.button("Discard link draft").clicked() {
            self.collaboration.link_participant = None;
            self.collaboration.link_conversation = None;
            self.collaboration.link_dirty = false;
            self.collaboration.link_generation = None;
        }
        if ui.button("Create link").clicked() && !self.collaboration.dirty {
            if self
                .collaboration
                .link_generation
                .is_some_and(|generation| generation != self.ctl.session.snapshot().generation)
            {
                self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into();
                return;
            }
            let (Some(participant), Some(node)) = (
                self.collaboration.link_participant.clone(),
                self.collaboration.link_conversation.clone(),
            ) else {
                self.ctl.error = "Select a participant and conversation.".into();
                return;
            };
            let link_id = processcraft_core::ids::ElementIdentity::anonymous();
            let mut updated = collaboration.clone();
            updated.conversation_links.push(ConversationLink {
                base: None,
                id: link_id.clone(),
                name: Some(String::new()),
                source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                    participant.clone(),
                )),
                target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                    node.clone(),
                )),
            });
            let mut mutations = vec![update_root(doc, updated)];
            if self.collaboration.draw {
                let Some(diagram) = doc.diagrams.iter().find(|d| {
                    self.diagram_id
                        .as_ref()
                        .is_none_or(|selected| &d.id == selected)
                }) else {
                    self.ctl.error = "The model has no current diagram.".into();
                    return;
                };
                let Some(a) = diagram.shapes.iter().find(|s| s.element == participant) else {
                    self.ctl.error = "The participant needs a shape in the current diagram.".into();
                    return;
                };
                let Some(b) = diagram.shapes.iter().find(|s| s.element == node) else {
                    self.ctl.error =
                        "The conversation needs a shape in the current diagram.".into();
                    return;
                };
                if diagram.plane != collaboration.id {
                    self.ctl.error =
                        "Choose a diagram for this collaboration before adding a representation."
                            .into();
                    return;
                }
                mutations.push(Mutation::CreateEdge {
                    document_id: id,
                    diagram_id: diagram.id.selector(),
                    edge: Edge {
                        di_metadata: None,
                        message_visible_kind: None,
                        label_visibility: Some(DiLabelVisibility::Explicit),
                        source_element: None,
                        target_element: None,
                        id: controller::id("ConversationEdge").into(),
                        element: link_id.selector(),
                        waypoints: vec![
                            Point {
                                x: a.bounds.x + a.bounds.width / 2.0,
                                y: a.bounds.y + a.bounds.height / 2.0,
                            },
                            Point {
                                x: b.bounds.x + b.bounds.width / 2.0,
                                y: b.bounds.y + b.bounds.height / 2.0,
                            },
                        ],
                        label: Some(Label::default()),
                        style: None,
                    },
                });
            }
            if self.commit_draft() && self.ctl.mutate(mutations) {
                self.collaboration.link_participant = None;
                self.collaboration.link_conversation = None;
                self.collaboration.link_dirty = false;
                self.collaboration.link_generation = None;
            }
        }
    }
    fn collaboration_data(&mut self, ui: &mut egui::Ui, doc: &BpmnDocument) {
        ui.collapsing("Data definitions", |ui| {
            let scope = processcraft_core::queries::scopes(doc)
                .into_iter()
                .find(|s| self.process_id.as_ref().is_none_or(|id| s.id == id));
            if let Some(scope) = scope { ui.label(format!("Scope: {}", if scope.name.is_empty() { "Unnamed scope" } else { scope.name })); }
            for store in &doc.data_stores {
                ui.label(format!("Store: {}", store.name.as_deref().unwrap_or("Unnamed")))
                    .on_hover_text(store.id.as_ref().and_then(|id| id.xml_id()).map_or_else(|| "No XML ID".into(), ToString::to_string));
            }
            if let Some(scope) = scope {
                for object in scope.data_objects {
                    ui.label(format!("Object: {}", object.name.as_deref().unwrap_or("Unnamed")))
                        .on_hover_text(object.id.as_ref().and_then(|id| id.xml_id()).map_or_else(|| "No XML ID".into(), ToString::to_string));
                }
            }
            let label = ui.label("Definition name");
            if ui
                .text_edit_singleline(&mut self.collaboration.data_name)
                .labelled_by(label.id)
                .changed()
            {
                self.collaboration.data_dirty = true;
                self.collaboration.document = Some(doc.id);
            }
            if ui
                .checkbox(
                    &mut self.collaboration.data_store,
                    "Data store (otherwise data object)",
                )
                .changed()
            {
                self.collaboration.data_dirty = true;
                self.collaboration.document = Some(doc.id);
            }
            ui.horizontal(|ui| {
                if ui.button("Create definition").clicked() {
                    let mutation = if self.collaboration.data_store {
                        Mutation::CreateDataStore {
                            document_id: doc.id,
                            data_store: processcraft_core::bpmn_data::DataStore { base: None,
                                item_aware: None,
                                id: Some(controller::id("DataStore").into()),
                                name: Some(self.collaboration.data_name.clone()),
                                capacity: None,
                                is_unlimited: None,
                            },
                        }
                    } else {
                        let Some(scope) = scope else {
                            self.ctl.error = "Select a process or subprocess first.".into();
                            return;
                        };
                        Mutation::CreateDataObject {
                            document_id: doc.id,
                            scope_id: scope.id.selector(),
                            data_object: processcraft_core::bpmn_data::DataObject { flow_metadata: None, base: None,
                                item_aware: None,
                                id: Some(controller::id("DataObject").into()),
                                name: Some(self.collaboration.data_name.clone()),
                                is_collection: None,
                            },
                        }
                    };
                    if self.commit_draft() && self.ctl.mutate(vec![mutation]) {
                        self.collaboration.data_name.clear();
                        self.collaboration.data_dirty = false;
                    }
                }
                if self.collaboration.data_dirty && ui.button("Discard definition draft").clicked()
                {
                    self.collaboration.data_name.clear();
                    self.collaboration.data_dirty = false;
                }
            });
            ui.separator();
            ui.strong("Data reference");
            if ui
                .checkbox(&mut self.collaboration.reference_store, "Reference a store")
                .changed()
            {
                self.collaboration.reference_target = None;
                self.collaboration.reference_dirty = true;
                self.collaboration.document = Some(doc.id);
            }
            let targets: Vec<_> = if self.collaboration.reference_store {
                doc.data_stores
                    .iter()
                    .filter_map(|v| v.id.as_ref().map(|id| (id.selector(), v.name.as_deref().unwrap_or("Unnamed"))))
                    .collect()
            } else {
                scope.map(|s| {
                    let ancestors = super::data::ancestor_data_candidates(doc, &s.id.selector()).2;
                    processcraft_core::queries::scopes(doc).into_iter().flat_map(|candidate| candidate.data_objects.iter()).filter_map(|value| {
                        value.id.as_ref().filter(|id| s.data_objects.iter().any(|object| object.id.as_ref() == Some(*id)) || ancestors.contains(&id.selector())).map(|id| (id.selector(), value.name.as_deref().unwrap_or("Unnamed")))
                    }).collect()
                }).unwrap_or_default()
            };
            egui::ComboBox::from_id_salt("data_reference_target")
                .selected_text(
                    self.collaboration
                        .reference_target
                        .as_ref()
                        .and_then(|id| targets.iter().find(|(v, _)| v == id))
                        .map_or("Select definition", |(_, name)| *name),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &targets {
                        if ui
                            .selectable_value(
                                &mut self.collaboration.reference_target,
                                Some(id.clone()),
                                *name,
                            )
                            .changed()
                        {
                            self.collaboration.reference_dirty = true;
                            self.collaboration.document = Some(doc.id);
                        }
                    }
                });
            let label = ui.label("Reference name");
            if ui
                .text_edit_singleline(&mut self.collaboration.reference_name)
                .labelled_by(label.id)
                .changed()
            {
                self.collaboration.reference_dirty = true;
                self.collaboration.document = Some(doc.id);
            }
            ui.horizontal(|ui| {
                if ui.button("Create reference").clicked() {
                    let (Some(scope), Some(target)) =
                        (scope, self.collaboration.reference_target.clone())
                    else {
                        self.ctl.error = "Select a scope and data definition.".into();
                        return;
                    };
                    let node_id = controller::id("DataReference");
                    let (kind, details) = if self.collaboration.reference_store {
                        (
                            NodeKind::DataStoreReference,
                            processcraft_core::bpmn_model::NodeDetails::DataStoreReference {
                                data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Legacy(target)),
                            },
                        )
                    } else {
                        (
                            NodeKind::DataObjectReference,
                            processcraft_core::bpmn_model::NodeDetails::DataObjectReference {
                                data_object_ref: Some(target),
                            },
                        )
                    };
                    let mut mutations = vec![Mutation::CreateElement {
                        document_id: doc.id,
                        process_id: scope.id.selector(),
                        element: Node { flow_metadata: None, base_extras: None,
                            item_aware: None,
                            id: node_id.clone().into(),
                            name: Some(self.collaboration.reference_name.clone()),
                            documentation: String::new().into(),
                            kind,
                            details: Some(details),
                            loop_characteristics: None,
                            io_specification: None,
            activity: None,
                            owner: String::new(),
                            contact: String::new(),
                        },
                    }];
                    if let Some(diagram) = doc.diagrams.iter().find(|d| {
                        self.diagram_id.as_ref().is_none_or(|id| &d.id == id)
                            && &d.plane == scope.id
                    }) {
                        mutations.push(Mutation::CreateShape {
                            document_id: doc.id,
                            diagram_id: diagram.id.selector(),
                            shape: Shape { di_metadata: None, is_message_visible: None, participant_band_kind: None, choreography_activity_shape: None, label_visibility: Some(DiLabelVisibility::Explicit),  is_horizontal: None, is_marker_visible: None,
                                id: controller::id("DataShape").into(),
                                element: node_id.into(),
                                is_expanded: None,
                                bounds: Bounds {
                                    x: self.camera.0 + 100.0,
                                    y: self.camera.1 + 100.0,
                                    width: 50.0,
                                    height: 70.0,
                                },
                                label: Some(Label::default()),
                                style: None,
                            },
                        });
                    }
                    if self.commit_draft() && self.ctl.mutate(mutations) {
                        self.collaboration.reference_name.clear();
                        self.collaboration.reference_target = None;
                        self.collaboration.reference_dirty = false;
                    }
                }
                if self.collaboration.reference_dirty
                    && ui.button("Discard reference draft").clicked()
                {
                    self.collaboration.reference_name.clear();
                    self.collaboration.reference_target = None;
                    self.collaboration.reference_dirty = false;
                }
            });
            ui.label("A shape is added when the current diagram shows the selected scope.");
            ui.separator();
            ui.strong("Activity input/output");
            ui.label("The new port is added to the first input or output set.");
            if let Some(scope) = scope {
                egui::ComboBox::from_id_salt("io_activity")
                    .selected_text(
                        self.collaboration
                            .io_activity
                            .as_ref()
                            .and_then(|id| scope.nodes.iter().find(|n| &n.id == id))
                            .map_or("Select activity", |n| n.name.as_deref().unwrap_or("Unnamed")),
                    )
                    .show_ui(ui, |ui| {
                        for node in scope.nodes.iter().filter(|n| n.kind.is_activity()) {
                            let selected =
                                self.collaboration.io_activity.as_ref() == Some(&node.id.selector());
                            if ui
                                .add_enabled(
                                    !self.collaboration.io_dirty || selected,
                                    egui::Button::selectable(selected, node.name.as_deref().unwrap_or("Unnamed")),
                                )
                                .clicked()
                                && !selected
                            {
                                self.collaboration.io_activity = Some(node.id.selector());
                                self.collaboration.io_dirty = true;
                                self.collaboration.document = Some(doc.id);
                            }
                        }
                    });
                let label = ui.label("Port name");
                if ui
                    .text_edit_singleline(&mut self.collaboration.io_name)
                    .labelled_by(label.id)
                    .changed()
                {
                    self.collaboration.io_dirty = true;
                    self.collaboration.document = Some(doc.id);
                }
                if ui
                    .checkbox(
                        &mut self.collaboration.io_output,
                        "Output (otherwise input)",
                    )
                    .changed()
                {
                    self.collaboration.io_dirty = true;
                    self.collaboration.document = Some(doc.id);
                }
                let (ancestor_inputs, ancestor_outputs, _) = super::data::ancestor_data_candidates(doc, &scope.id.selector());
                let ancestor_ids = if self.collaboration.io_output { &ancestor_outputs } else { &ancestor_inputs };
                let mut candidates: Vec<(ElementSelector, String)> = doc.data_stores.iter().filter_map(|store| store.id.as_ref().map(|id| (id.selector(), format!("Store · {} · {id}", store.name.as_deref().unwrap_or("Unnamed"))))).collect();
                for candidate in processcraft_core::queries::scopes(doc) {
                    for object in candidate.data_objects {
                        if let Some(id) = &object.id && (candidate.id == scope.id || ancestor_ids.contains(&id.selector())) { candidates.push((id.selector(), format!("Object · {} · {id}", object.name.as_deref().unwrap_or("Unnamed")))); }
                    }
                    for node in candidate.nodes.iter().filter(|node| matches!(node.kind, NodeKind::DataObjectReference | NodeKind::DataStoreReference) && (candidate.id == scope.id || ancestor_ids.contains(&node.id.selector()))) {
                        candidates.push((node.id.selector(), format!("Reference · {} · {}", node.name.as_deref().unwrap_or("Unnamed"), node.id)));
                    }
                }
                for process in &doc.processes {
                    if let Some(io) = &process.io_specification {
                        let ports = if self.collaboration.io_output { &io.outputs } else { &io.inputs };
                        for port in ports { if let Some(id) = &port.id && (&process.id == scope.id || ancestor_ids.contains(&id.selector())) { candidates.push((id.selector(), format!("Process port · {} · {id}", port.name.as_deref().unwrap_or("Unnamed")))); } }
                    }
                }
                egui::ComboBox::from_id_salt("io_reference")
                    .selected_text(self.collaboration.io_reference.as_ref().and_then(|id| candidates.iter().find(|(candidate, _)| candidate == id)).map_or("No data association", |(_, label)| label.as_str()))
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut self.collaboration.io_reference, None, "No data association").changed() { self.collaboration.io_dirty = true; self.collaboration.document = Some(doc.id); }
                        for (id, label) in &candidates {
                            if ui.selectable_value(&mut self.collaboration.io_reference, Some(id.clone()), label).changed() { self.collaboration.io_dirty = true; self.collaboration.document = Some(doc.id); }
                        }
                    });
                ui.label("An association edge is added when both shapes are in the current scope diagram.");
                ui.horizontal(|ui| {
                    if ui.button("Create port").clicked() {
                        let Some(node) = self
                            .collaboration
                            .io_activity
                            .as_ref()
                            .and_then(|id| scope.nodes.iter().find(|n| &n.id == id))
                        else {
                            self.ctl.error = "Select an activity.".into();
                            return;
                        };
                        let mut io = node.io_specification.clone().unwrap_or_default();
                        if io.input_sets.is_empty() {
                            io.input_sets.push(Default::default());
                        }
                        if io.output_sets.is_empty() {
                            io.output_sets.push(Default::default());
                        }
                        let port_id = controller::id("DataPort");
                        let port = processcraft_core::bpmn_data::DataPort { base: None,
                            item_aware: None,
                            is_collection: None,
                            id: Some(port_id.clone().into()),
                            name: Some(self.collaboration.io_name.clone()),
                        };
                        if self.collaboration.io_output {
                            io.output_sets[0].data_refs.push(port_id.clone().into());
                            io.outputs.push(port);
                        } else {
                            io.input_sets[0].data_refs.push(port_id.clone().into());
                            io.inputs.push(port);
                        }
                        let mut mutations = vec![Mutation::SetNodeIoSpecification {
                            document_id: doc.id,
                            element_id: node.id.selector(),
                            io_specification: Some(io),
                        }];
                        if let Some(reference) = &self.collaboration.io_reference {
                            let assoc_id = controller::id("DataAssociation");
                            let (kind, sources, target) = if self.collaboration.io_output {
                                (
                                    processcraft_core::bpmn_data::DataAssociationKind::Output,
                                    vec![port_id.clone().into()],
                                    reference.clone(),
                                )
                            } else {
                                (
                                    processcraft_core::bpmn_data::DataAssociationKind::Input,
                                    vec![reference.clone()],
                                    port_id.clone().into(),
                                )
                            };
                            mutations.push(Mutation::CreateDataAssociation {
                                document_id: doc.id,
                                scope_id: scope.id.selector(),
                                association: processcraft_core::bpmn_data::DataAssociation { base: None,
                                    transformation: None,
                                    assignments: Vec::new(),
                                    id: Some(assoc_id.clone().into()),
                                    owner: node.id.selector(),
                                    kind,
                                    sources,
                                    target,
                                },
                            });
                            if let Some(diagram) = doc.diagrams.iter().find(|d| {
                                self.diagram_id.as_ref().is_none_or(|id| &d.id == id)
                                    && &d.plane == scope.id
                            }) && let (Some(activity_shape), Some(reference_shape)) = (
                                diagram.shapes.iter().find(|s| s.element == node.id),
                                diagram.shapes.iter().find(|s| &s.element == reference),
                            ) {
                                let center = |shape: &Shape| Point {
                                    x: shape.bounds.x + shape.bounds.width / 2.0,
                                    y: shape.bounds.y + shape.bounds.height / 2.0,
                                };
                                let waypoints = if self.collaboration.io_output {
                                    vec![center(activity_shape), center(reference_shape)]
                                } else {
                                    vec![center(reference_shape), center(activity_shape)]
                                };
                                mutations.push(Mutation::CreateEdge {
                                    document_id: doc.id,
                                    diagram_id: diagram.id.selector(),
                                    edge: Edge { di_metadata: None, message_visible_kind: None, label_visibility: Some(DiLabelVisibility::Explicit),  source_element: None, target_element: None,
                                        id: controller::id("DataAssociationEdge").into(),
                                        element: assoc_id.into(),
                                        waypoints,
                                        label: Some(Label::default()),
                                        style: None,
                                    },
                                });
                            }
                        }
                        if self.commit_draft() && self.ctl.mutate(mutations) {
                            self.collaboration.io_name.clear();
                            self.collaboration.io_activity = None;
                            self.collaboration.io_reference = None;
                            self.collaboration.io_dirty = false;
                        }
                    }
                    if self.collaboration.io_dirty && ui.button("Discard port draft").clicked() {
                        self.collaboration.io_name.clear();
                        self.collaboration.io_activity = None;
                        self.collaboration.io_reference = None;
                        self.collaboration.io_dirty = false;
                    }
                });
            }
        });
    }
    fn collaboration_groups(&mut self, ui: &mut egui::Ui, doc: &BpmnDocument) {
        ui.collapsing("Groups and categories", |ui| {
            let scope = processcraft_core::queries::scopes(doc)
                .into_iter()
                .find(|scope| self.process_id.as_ref().is_none_or(|id| scope.id == id));
            if let Some(scope) = scope {
                ui.label(format!(
                    "Scope: {}",
                    if scope.name.is_empty() {
                        "Unnamed scope"
                    } else {
                        scope.name
                    }
                ));
                for group in scope.groups {
                    let value = group
                        .category_value_ref
                        .as_ref()
                        .and_then(|id| {
                            doc.categories.iter().flat_map(|c| &c.values).find(|v| {
                                id.matches_identity(
                                    &v.id,
                                    doc.definitions
                                        .as_ref()
                                        .map(|definitions| definitions.target_namespace.as_str()),
                                )
                            })
                        })
                        .and_then(|v| v.value.as_deref())
                        .unwrap_or("Uncategorised group");
                    ui.horizontal(|ui| {
                        ui.label(value).on_hover_text(
                            group
                                .id
                                .xml_id()
                                .map_or_else(|| "No XML ID".into(), ToString::to_string),
                        );
                        if !self.collaboration.has_draft() && ui.button("Delete group").clicked() {
                            self.ctl.mutate(vec![Mutation::DeleteGroup {
                                document_id: doc.id,
                                scope_id: scope.id.selector(),
                                group_id: group.id.selector(),
                            }]);
                        }
                    });
                }
                let label = ui.label("Category name");
                if ui
                    .text_edit_singleline(&mut self.collaboration.category_name)
                    .labelled_by(label.id)
                    .changed()
                {
                    self.collaboration.group_dirty = true;
                    self.collaboration.document = Some(doc.id);
                }
                let label = ui.label("Category value / group label");
                if ui
                    .text_edit_singleline(&mut self.collaboration.category_value)
                    .labelled_by(label.id)
                    .changed()
                {
                    self.collaboration.group_dirty = true;
                    self.collaboration.document = Some(doc.id);
                }
                ui.label("A group shape is added when the current diagram shows this scope.");
                ui.horizontal(|ui| {
                    if ui.button("Create category and group").clicked() {
                        let category_id = controller::id("Category");
                        let value_id = controller::id("CategoryValue");
                        let group_id = controller::id("Group");
                        let category = processcraft_core::bpmn_model::Category {
                            base: None,
                            id: category_id.into(),
                            name: Some(self.collaboration.category_name.clone()),
                            values: vec![processcraft_core::bpmn_model::CategoryValue {
                                base: None,
                                id: value_id.clone().into(),
                                value: Some(self.collaboration.category_value.clone()),
                            }],
                        };
                        let mut mutations = vec![
                            Mutation::CreateCategory {
                                document_id: doc.id,
                                category,
                            },
                            Mutation::CreateGroup {
                                document_id: doc.id,
                                scope_id: scope.id.selector(),
                                group: processcraft_core::bpmn_model::Group {
                                    base: None,
                                    id: group_id.clone().into(),
                                    category_value_ref: Some(
                                        processcraft_core::bpmn_activity::ModelReference::Local(
                                            value_id.into(),
                                        ),
                                    ),
                                },
                            },
                        ];
                        if let Some(diagram) = doc.diagrams.iter().find(|d| {
                            self.diagram_id.as_ref().is_none_or(|id| &d.id == id)
                                && &d.plane == scope.id
                        }) {
                            mutations.push(Mutation::CreateShape {
                                document_id: doc.id,
                                diagram_id: diagram.id.selector(),
                                shape: Shape {
                                    di_metadata: None,
                                    is_message_visible: None,
                                    participant_band_kind: None,
                                    choreography_activity_shape: None,
                                    label_visibility: Some(DiLabelVisibility::Explicit),
                                    is_horizontal: None,
                                    is_marker_visible: None,
                                    id: controller::id("GroupShape").into(),
                                    element: group_id.into(),
                                    is_expanded: None,
                                    bounds: Bounds {
                                        x: self.camera.0 + 80.0,
                                        y: self.camera.1 + 80.0,
                                        width: 260.0,
                                        height: 160.0,
                                    },
                                    label: Some(Label::default()),
                                    style: None,
                                },
                            });
                        }
                        if self.commit_draft() && self.ctl.mutate(mutations) {
                            self.collaboration.category_name.clear();
                            self.collaboration.category_value.clear();
                            self.collaboration.group_dirty = false;
                        }
                    }
                    if self.collaboration.group_dirty && ui.button("Discard group draft").clicked()
                    {
                        self.collaboration.category_name.clear();
                        self.collaboration.category_value.clear();
                        self.collaboration.group_dirty = false;
                    }
                });
            } else {
                ui.label("Select a process or subprocess first.");
            }
        });
    }
    fn collaboration_choreography(
        &mut self,
        ui: &mut egui::Ui,
        doc: &BpmnDocument,
        generation: u64,
    ) {
        ui.collapsing("Choreography tasks", |ui| {
            ui.collapsing("New choreography", |ui| {
                let mut changed = false;
                for (label, text) in [
                    ("Choreography name", &mut self.collaboration.root_name),
                    ("Initiating participant", &mut self.collaboration.root_first),
                    ("Other participant", &mut self.collaboration.root_second),
                    ("First task name", &mut self.collaboration.root_task),
                ] {
                    let label = ui.label(label);
                    changed |= ui
                        .text_edit_singleline(text)
                        .labelled_by(label.id)
                        .changed();
                }
                if changed {
                    self.collaboration.root_dirty = true;
                    self.collaboration.document = Some(doc.id);
                }
                ui.horizontal(|ui| {
                    if ui.button("Create global choreography and diagram").clicked() {
                        let root = controller::id("GlobalChoreography");
                        let diagram = controller::id("ChoreographyDiagram");
                        let choreography = processcraft_core::bpmn_collaboration::Choreography {
                            kind: Some(processcraft_core::bpmn_collaboration::ChoreographyRootKind::Global { initiating_participant_ref: None }),
                            graph: Some(Default::default()), extras: None, conversation_nodes: vec![], conversation_links: vec![], correlation_keys: vec![], base: None,
                            id: root.clone().into(), name: if self.collaboration.root_name.is_empty() { None } else { Some(self.collaboration.root_name.clone()) }, participants: vec![], message_flows: vec![], activities: vec![], sequence_flows: vec![],
                        };
                        let diagram_value = Diagram { plane_metadata: None, plane_order: None, documentation: None, label_styles: Vec::new(), resolution: None, name: None, id: diagram.clone().into(), plane_id: controller::id("ChoreographyPlane").into(), plane: root.clone().into(), shapes: vec![], edges: vec![] };
                        if self.commit_draft() && self.ctl.mutate(vec![Mutation::CreateChoreography { document_id: doc.id, choreography }, Mutation::CreateDiagram { document_id: doc.id, diagram: diagram_value }]) {
                            self.diagram_id = Some(diagram.into()); self.process_id = None;
                            self.collaboration.root_name.clear(); self.collaboration.root_first.clear(); self.collaboration.root_second.clear(); self.collaboration.root_task.clear(); self.collaboration.root_dirty = false;
                            self.collaboration.choreography_id = Some(root.into());
                        }
                    }
                    if ui.button("Create choreography and diagram").clicked() {
                        if self.collaboration.root_name.trim().is_empty()
                            || self.collaboration.root_first.trim().is_empty()
                            || self.collaboration.root_second.trim().is_empty()
                            || self.collaboration.root_task.trim().is_empty()
                        {
                            self.ctl.error =
                                "Enter a choreography, two participants, and a first task.".into();
                            return;
                        }
                        let root = controller::id("Choreography");
                        let diagram = controller::id("ChoreographyDiagram");
                        let task = controller::id("ChoreographyTask");
                        let first = controller::id("Participant");
                        let second = controller::id("Participant");
                        let choreography = fresh_choreography(
                            [root.clone(), task.clone(), first.clone(), second.clone()],
                            [self.collaboration.root_name.clone(), self.collaboration.root_task.clone(), self.collaboration.root_first.clone(), self.collaboration.root_second.clone()],
                        );
                        let diagram_value = Diagram {
                            plane_metadata: None,
                            plane_order: None,
                            documentation: None,
                            label_styles: Vec::new(),
                            resolution: None,
                            name: None,
                            id: diagram.clone().into(),
                            plane_id: controller::id("ChoreographyPlane").into(),
                            plane: root.clone().into(),
                            shapes: choreography_task_shapes(
                                task.into(),
                                first.into(),
                                second.into(),
                                Bounds {
                                    x: 80.0,
                                    y: 80.0,
                                    width: 180.0,
                                    height: 100.0,
                                },
                            ),
                            edges: vec![],
                        };
                        if self.commit_draft()
                            && self.ctl.mutate_at_generation(generation, vec![
                                Mutation::CreateChoreography {
                                    document_id: doc.id,
                                    choreography,
                                },
                                Mutation::CreateDiagram {
                                    document_id: doc.id,
                                    diagram: diagram_value,
                                },
                            ])
                        {
                            self.diagram_id = Some(diagram.into());
                            self.process_id = None;
                            self.collaboration.root_name.clear();
                            self.collaboration.root_first.clear();
                            self.collaboration.root_second.clear();
                            self.collaboration.root_task.clear();
                            self.collaboration.root_dirty = false;
                            self.collaboration.choreography_id = Some(root.into());
                        }
                    }
                    if self.collaboration.root_dirty
                        && ui.button("Discard choreography draft").clicked()
                    {
                        self.collaboration.root_name.clear();
                        self.collaboration.root_first.clear();
                        self.collaboration.root_second.clear();
                        self.collaboration.root_task.clear();
                        self.collaboration.root_dirty = false;
                    }
                });
            });
            if doc.choreographies.is_empty() {
                ui.label("No choreography in this model.");
                return;
            }
            egui::ComboBox::from_id_salt("choreography_choice")
                .selected_text(
                    self.collaboration
                        .choreography_id
                        .as_ref()
                        .and_then(|id| doc.choreographies.iter().find(|c| &c.id == id))
                        .map_or("Select choreography", |c| {
                            c.name.as_deref().unwrap_or("Unnamed")
                        }),
                )
                .show_ui(ui, |ui| {
                    for c in &doc.choreographies {
                        let selected =
                            self.collaboration.choreography_id.as_ref() == Some(&c.id.selector());
                        if ui
                            .add_enabled(
                                !self.collaboration.choreography_dirty || selected,
                                egui::Button::selectable(
                                    selected,
                                    c.name.as_deref().unwrap_or("Unnamed"),
                                ),
                            )
                            .clicked()
                            && !selected
                        {
                            self.collaboration.choreography_id = Some(c.id.selector());
                            self.collaboration.choreography_first = None;
                            self.collaboration.choreography_second = None;
                            self.collaboration.choreography_dirty = true;
                            self.collaboration.document = Some(doc.id);
                        }
                    }
                });
            let Some(choreo) = self
                .collaboration
                .choreography_id
                .as_ref()
                .and_then(|id| doc.choreographies.iter().find(|c| &c.id == id))
            else {
                return;
            };
            let scope = processcraft_core::queries::choreography_scopes(doc).into_iter().find(|scope| scope.owner == choreo.id.selector());
            for activity in scope.iter().flat_map(|scope| scope.activities.iter()) {
                ui.label(activity.name.as_deref().unwrap_or("Unnamed"))
                    .on_hover_text(
                        activity
                            .id
                            .xml_id()
                            .map_or_else(|| "No XML ID".into(), ToString::to_string),
                    );
            }
            let name_label = ui.label("Task name");
            if ui
                .text_edit_singleline(&mut self.collaboration.choreography_name)
                .labelled_by(name_label.id)
                .changed()
            {
                self.collaboration.choreography_dirty = true;
                self.collaboration.document = Some(doc.id);
            }
            egui::ComboBox::from_id_salt("choreography_first")
                .selected_text(
                    self.collaboration
                        .choreography_first
                        .as_ref()
                        .and_then(|id| choreo.participants.iter().find(|p| &p.id == id))
                        .map_or("Initiating participant", |p| {
                            p.name.as_deref().unwrap_or("Unnamed")
                        }),
                )
                .show_ui(ui, |ui| {
                    for participant in &choreo.participants {
                        if ui
                            .selectable_value(
                                &mut self.collaboration.choreography_first,
                                Some(participant.id.selector()),
                                participant.name.as_deref().unwrap_or("Unnamed"),
                            )
                            .changed()
                        {
                            self.collaboration.choreography_dirty = true;
                            self.collaboration.document = Some(doc.id);
                        }
                    }
                });
            egui::ComboBox::from_id_salt("choreography_second")
                .selected_text(
                    self.collaboration
                        .choreography_second
                        .as_ref()
                        .and_then(|id| choreo.participants.iter().find(|p| &p.id == id))
                        .map_or("Other participant", |p| {
                            p.name.as_deref().unwrap_or("Unnamed")
                        }),
                )
                .show_ui(ui, |ui| {
                    for participant in &choreo.participants {
                        if ui
                            .selectable_value(
                                &mut self.collaboration.choreography_second,
                                Some(participant.id.selector()),
                                participant.name.as_deref().unwrap_or("Unnamed"),
                            )
                            .changed()
                        {
                            self.collaboration.choreography_dirty = true;
                            self.collaboration.document = Some(doc.id);
                        }
                    }
                });
            ui.label(
                "Select a diagram for this choreography to place the task and participant bands.",
            );
            ui.horizontal(|ui| {
                if ui.button("Create choreography task").clicked() {
                    let (Some(first), Some(second)) = (
                        self.collaboration.choreography_first.clone(),
                        self.collaboration.choreography_second.clone(),
                    ) else {
                        self.ctl.error = "Select two participants.".into();
                        return;
                    };
                    if first == second {
                        self.ctl.error = "Select two different participants.".into();
                        return;
                    }
                    let node_id = controller::id("ChoreographyTask");
                    let activity = processcraft_core::bpmn_collaboration::ChoreographyActivity {
                            loop_type: None,
                            details: None,
                            correlation_keys: Vec::new(),
                            flow_metadata: None,
                            base: None,
                            id: node_id.clone().into(),
                            name: Some(self.collaboration.choreography_name.clone()),
                            participant_refs: vec![first.clone().into(), second.clone().into()],
                            initiating_participant: Some(first.clone().into()),
                            message_flow_refs: vec![],
                        };
                    let mut mutations = vec![if let Some(graph) = &choreo.graph {
                        Mutation::CreateChoreographyFlowElement { document_id: doc.id, choreography_id: choreo.id.selector(), scope_id: choreo.id.selector(), index: graph.flow_elements.len(), element: processcraft_core::bpmn_collaboration::ChoreographyFlowElement::Activity(Box::new(activity)) }
                    } else { Mutation::CreateChoreographyActivity { document_id: doc.id, choreography_id: choreo.id.selector(), activity } }];
                    let Some(diagram) = doc.diagrams.iter().find(|d| {
                        self.diagram_id.as_ref().is_none_or(|id| &d.id == id)
                            && d.plane == choreo.id
                    }) else {
                        self.ctl.error =
                            "Select a diagram for this choreography before creating a task.".into();
                        return;
                    };
                    for shape in choreography_task_shapes(
                        node_id.into(),
                        first,
                        second,
                        Bounds {
                            x: self.camera.0 + 120.0,
                            y: self.camera.1 + 80.0 + scope.as_ref().map_or(0, |scope| scope.activities.len()) as f64 * 110.0,
                            width: 160.0,
                            height: 100.0,
                        },
                    ) {
                        mutations.push(Mutation::CreateShape {
                            document_id: doc.id,
                            diagram_id: diagram.id.selector(),
                            shape,
                        });
                    }
                    if self.commit_draft() && self.ctl.mutate_at_generation(generation, mutations) {
                        self.collaboration.choreography_name.clear();
                        self.collaboration.choreography_first = None;
                        self.collaboration.choreography_second = None;
                        self.collaboration.choreography_dirty = false;
                    }
                }
                if self.collaboration.choreography_dirty
                    && ui.button("Discard task draft").clicked()
                {
                    self.collaboration.choreography_name.clear();
                    self.collaboration.choreography_first = None;
                    self.collaboration.choreography_second = None;
                    self.collaboration.choreography_id = None;
                    self.collaboration.choreography_dirty = false;
                }
            });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_choreography_graph_survives_native_round_trip_and_history() {
        use processcraft_core::bpmn_collaboration::ChoreographyFlowElement;
        let mut app = ProcessCraft::default();
        let before = app.ctl.session.snapshot();
        let document_id = before.project.documents[0].id;
        let choreography = fresh_choreography(
            [
                "Root".parse().unwrap(),
                "Task".parse().unwrap(),
                "Buyer".parse().unwrap(),
                "Seller".parse().unwrap(),
            ],
            [
                "Orders".into(),
                "Submit".into(),
                "Buyer".into(),
                "Seller".into(),
            ],
        );
        assert!(choreography.activities.is_empty() && choreography.sequence_flows.is_empty());
        assert!(
            matches!(&choreography.graph.as_ref().unwrap().flow_elements[..], [ChoreographyFlowElement::Activity(activity)] if activity.name.as_deref() == Some("Submit"))
        );
        let diagram = Diagram {
            id: "Diagram".parse().unwrap(),
            plane_id: "Plane".parse().unwrap(),
            plane: "Root".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: choreography_task_shapes(
                "Task".parse().unwrap(),
                "Buyer".parse().unwrap(),
                "Seller".parse().unwrap(),
                Bounds {
                    x: 80.0,
                    y: 80.0,
                    width: 180.0,
                    height: 100.0,
                },
            ),
            edges: vec![],
        };
        assert!(app.ctl.mutate_at_generation(
            before.generation,
            vec![
                Mutation::CreateChoreography {
                    document_id,
                    choreography
                },
                Mutation::CreateDiagram {
                    document_id,
                    diagram
                }
            ]
        ));
        let created = app.ctl.session.snapshot();
        let native = processcraft_io::native::write_bytes(&created.project).unwrap();
        let loaded = processcraft_io::native::load_bytes(&native).unwrap();
        assert_eq!(
            loaded.project.content_identity().unwrap(),
            created.project.content_identity().unwrap()
        );
        assert_eq!(
            loaded.project.documents[0].choreographies[0]
                .graph
                .as_ref()
                .unwrap()
                .flow_elements
                .len(),
            1
        );
        app.history(Request::Undo);
        assert_eq!(app.ctl.session.snapshot().content_id, before.content_id);
        app.history(Request::Redo);
        assert_eq!(app.ctl.session.snapshot().content_id, created.content_id);
    }
    #[test]
    fn created_choreography_bands_round_trip_and_delete_by_owner() {
        use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let participant = |id: &str| Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: id.parse().unwrap(),
            name: Some(id.into()),
            base: None,
            process: None,
            multiplicity: None,
        };
        let message = Flow {
            id: "MessageFlow".parse().unwrap(),
            kind: FlowKind::Message,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Buyer".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Seller".parse().unwrap(),
            )),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            flow_metadata: None,
            condition: None,
            association_direction: None,
            message_ref: None,
        };
        let choreography = Choreography {
            kind: None,
            graph: None,
            extras: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            id: "Choreography".parse().unwrap(),
            name: Some("Orders".into()),
            base: None,
            participants: vec![participant("Buyer"), participant("Seller")],
            message_flows: vec![message],
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
                    "MessageFlow".parse().unwrap(),
                )],
            }],
            sequence_flows: vec![],
        };
        let mut shapes = choreography_task_shapes(
            "Task".parse().unwrap(),
            "Buyer".parse().unwrap(),
            "Seller".parse().unwrap(),
            Bounds {
                x: 10.0,
                y: 10.0,
                width: 160.0,
                height: 100.0,
            },
        );
        let first_owner = shapes[0].id.selector();
        let first_band = shapes[1].id.selector();
        let mut second = choreography_task_shapes(
            "Task".parse().unwrap(),
            "Buyer".parse().unwrap(),
            "Seller".parse().unwrap(),
            Bounds {
                x: 250.0,
                y: 10.0,
                width: 160.0,
                height: 100.0,
            },
        );
        let second_owner = second[0].id.selector();
        shapes.append(&mut second);
        let order = shapes.iter().map(|shape| shape.id.selector()).collect();
        let diagram = Diagram {
            id: "ChoreographyDiagram".parse().unwrap(),
            plane_id: "ChoreographyPlane".parse().unwrap(),
            plane: "Choreography".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: Some(order),
            shapes,
            edges: vec![],
        };
        let before = app.ctl.session.snapshot().content_id;
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
        let authored = app.ctl.session.snapshot().content_id;
        let snap = app.ctl.session.snapshot();
        let exported = processcraft_io::interchange::export_bpmn(&snap.project, document_id, true)
            .expect("export authored bands");
        let xml = String::from_utf8(exported.bytes).unwrap();
        assert_eq!(
            xml.matches("participantBandKind=\"top_initiating\"")
                .count(),
            2
        );
        assert_eq!(
            xml.matches("participantBandKind=\"bottom_non_initiating\"")
                .count(),
            2
        );
        let imported = processcraft_io::interchange::prepare_import(xml.as_bytes(), "authored")
            .expect("reimport authored bands");
        assert_eq!(
            imported
                .document
                .diagrams
                .iter()
                .flat_map(|diagram| &diagram.shapes)
                .filter(|shape| shape.participant_band_kind.is_some())
                .count(),
            4
        );
        app.history(Request::Undo);
        assert_eq!(app.ctl.session.snapshot().content_id, before);
        app.history(Request::Redo);
        assert_eq!(app.ctl.session.snapshot().content_id, authored);
        assert!(app.ctl.mutate(vec![Mutation::DeleteShape {
            document_id,
            diagram_id: "ChoreographyDiagram".parse().unwrap(),
            shape_id: first_owner
        }]));
        let snap = app.ctl.session.snapshot();
        let diagram = snap.project.documents[0]
            .diagrams
            .iter()
            .find(|d| {
                d.id == "ChoreographyDiagram"
                    .parse::<processcraft_core::ids::ElementSelector>()
                    .unwrap()
            })
            .unwrap();
        assert_eq!(diagram.shapes.len(), 3);
        assert!(
            diagram
                .shapes
                .iter()
                .any(|shape| shape.id.selector() == second_owner)
        );
        assert!(!diagram.plane_order.as_ref().unwrap().contains(&first_band));
        app.history(Request::Undo);
        assert_eq!(app.ctl.session.snapshot().content_id, authored);
        assert!(app.ctl.mutate(vec![Mutation::DeleteChoreographyActivity {
            document_id,
            choreography_id: "Choreography".parse().unwrap(),
            activity_id: "Task".parse().unwrap()
        }]));
        let snap = app.ctl.session.snapshot();
        assert_eq!(
            snap.project.documents[0].choreographies[0]
                .participants
                .len(),
            2
        );
        let diagram = snap.project.documents[0]
            .diagrams
            .iter()
            .find(|d| {
                d.id == "ChoreographyDiagram"
                    .parse::<processcraft_core::ids::ElementSelector>()
                    .unwrap()
            })
            .unwrap();
        assert!(diagram.shapes.is_empty());
        assert!(diagram.plane_order.as_ref().unwrap().is_empty());
        app.history(Request::Undo);
        assert_eq!(app.ctl.session.snapshot().content_id, authored);
    }
    #[test]
    fn drafts_survive_closing_and_reset_on_session_change() {
        let mut app = ProcessCraft::default();
        let ctx = egui::Context::default();
        app.open_collaboration();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            app.collaboration_windows(&ctx)
        });
        output.textures_delta.clear();
        assert!(app.collaboration.open);
        app.collaboration.name = "In progress".into();
        app.collaboration.dirty = true;
        app.collaboration.link_dirty = true;
        app.collaboration.link_participant = Some("participant".parse().unwrap());
        app.collaboration.open = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            app.collaboration_windows(&ctx)
        });
        output.textures_delta.clear();
        assert!(app.collaboration.has_draft());
        assert_eq!(app.collaboration.name, "In progress");
        app.collaboration.clear_node();
        assert!(app.collaboration.has_draft());
        assert!(app.collaboration.link_participant.is_some());
        app.collaboration.link_dirty = false;
        app.collaboration.reference_dirty = true;
        assert!(app.collaboration.has_draft());
        app.collaboration.reference_dirty = false;
        app.collaboration.io_dirty = true;
        assert!(app.collaboration.has_draft());
        app.collaboration.io_dirty = false;
        app.collaboration.group_dirty = true;
        assert!(app.collaboration.has_draft());
        app.ctl.session = Controller::default().session;
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            app.collaboration_windows(&ctx)
        });
        output.textures_delta.clear();
        assert!(!app.collaboration.has_draft());
    }
}

#[cfg(test)]
mod collaboration_scope_tests {
    use super::*;
    use processcraft_core::bpmn_collaboration::{
        Choreography, CollaborationExtras, CollaborationRootKind,
    };
    #[test]
    fn inherited_choreography_update_preserves_own_graph_and_history() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let choreography = Choreography {
            kind: None,
            graph: None,
            id: "Choreography".parse().unwrap(),
            name: None,
            base: None,
            extras: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            participants: vec![],
            message_flows: vec![],
            activities: vec![],
            sequence_flows: vec![],
        };
        assert!(app.ctl.mutate(vec![Mutation::CreateChoreography {
            document_id,
            choreography
        }]));
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let mut root = choreography_view(&document.choreographies[0]);
        root.name = Some("Distribution".into());
        root.extras = Some(CollaborationExtras {
            is_closed: Some(false),
            ..Default::default()
        });
        assert!(app.ctl.mutate(vec![update_root(document, root)]));
        let snapshot = app.ctl.session.snapshot();
        let stored = &snapshot.project.documents[0].choreographies[0];
        assert_eq!(stored.name.as_deref(), Some("Distribution"));
        assert_eq!(stored.extras.as_ref().unwrap().is_closed, Some(false));
        assert!(stored.activities.is_empty());
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].choreographies[0]
                .extras
                .is_none()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].choreographies[0]
                .extras
                .as_ref()
                .unwrap()
                .is_closed,
            Some(false)
        );
    }
    #[test]
    fn global_collaboration_updates_the_selected_root_only() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let make = |id: &str, kind| Collaboration {
            kind,
            extras: None,
            correlation_keys: vec![],
            base: None,
            name: None,
            id: id.parse().unwrap(),
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::SetCollaborationRoot {
                document_id,
                collaboration: make("First", None)
            },
            Mutation::SetCollaborationRoot {
                document_id,
                collaboration: make("Second", Some(CollaborationRootKind::GlobalConversation))
            }
        ]));
        let snapshot = app.ctl.session.snapshot();
        let mut second = snapshot.project.documents[0].additional_collaborations[0].clone();
        second.name = Some("Conversations".into());
        assert!(app.ctl.mutate(vec![Mutation::SetCollaborationRoot {
            document_id,
            collaboration: second
        }]));
        let snapshot = app.ctl.session.snapshot();
        assert!(
            snapshot.project.documents[0]
                .collaboration
                .as_ref()
                .unwrap()
                .name
                .is_none()
        );
        assert_eq!(
            snapshot.project.documents[0].additional_collaborations[0]
                .name
                .as_deref(),
            Some("Conversations")
        );
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].additional_collaborations[0]
                .name
                .is_none()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].additional_collaborations[0]
                .name
                .as_deref(),
            Some("Conversations")
        );
    }
}
