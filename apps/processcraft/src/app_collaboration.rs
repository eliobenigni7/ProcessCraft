use super::*;
use processcraft_core::bpmn_collaboration::{ConversationKind, ConversationLink, ConversationNode};

#[derive(Default)]
pub(super) struct State {
    session: String,
    open: bool,
    document: Option<processcraft_core::ids::DocumentId>,
    node_id: Option<XmlId>,
    name: String,
    is_subconversation: bool,
    parent: Option<XmlId>,
    participants: Vec<XmlId>,
    link_participant: Option<XmlId>,
    link_conversation: Option<XmlId>,
    dirty: bool,
    link_dirty: bool,
    data_name: String,
    data_store: bool,
    data_dirty: bool,
    reference_name: String,
    reference_target: Option<XmlId>,
    reference_store: bool,
    reference_dirty: bool,
    io_name: String,
    io_activity: Option<XmlId>,
    io_reference: Option<XmlId>,
    io_output: bool,
    io_dirty: bool,
    choreography_id: Option<XmlId>,
    choreography_name: String,
    choreography_first: Option<XmlId>,
    choreography_second: Option<XmlId>,
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
    pub(super) fn has_draft(&self) -> bool {
        self.dirty
            || self.link_dirty
            || self.data_dirty
            || self.reference_dirty
            || self.io_dirty
            || self.choreography_dirty
            || self.root_dirty
            || self.group_dirty
    }
    fn clear_node(&mut self) {
        self.node_id = None;
        self.name.clear();
        self.is_subconversation = false;
        self.parent = None;
        self.participants.clear();
        self.dirty = false;
    }
    fn clear(&mut self) {
        self.clear_node();
        self.link_participant = None;
        self.link_conversation = None;
        self.link_dirty = false;
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
        self.collaboration_data(ui, doc);
        self.collaboration_groups(ui, doc);
        self.collaboration_choreography(ui, doc);
        ui.separator();
        let Some(collaboration) = &doc.collaboration else {
            ui.label(
                "Create a collaboration, then add participants using the model's pool controls.",
            );
            if ui.button("Create collaboration").clicked() && self.commit_draft() {
                self.ctl.mutate(vec![Mutation::CreateCollaboration {
                    document_id: id,
                    collaboration: Collaboration {
                        id: controller::id("Collaboration"),
                        participants: vec![],
                        flows: vec![],
                        conversation_nodes: vec![],
                        conversation_links: vec![],
                    },
                }]);
            }
            return;
        };
        ui.label("Conversations");
        if collaboration.participants.is_empty() {
            ui.label("Add a pool to create a participant.");
        }
        egui::ScrollArea::vertical()
            .id_salt("conversation_list")
            .max_height(130.0)
            .show(ui, |ui| {
                for node in &collaboration.conversation_nodes {
                    let label = if node.name.is_empty() {
                        "Unnamed conversation"
                    } else {
                        &node.name
                    };
                    if ui
                        .selectable_label(
                            self.collaboration.node_id.as_ref() == Some(&node.id),
                            label,
                        )
                        .on_hover_text(node.id.to_string())
                        .clicked()
                        && !self.collaboration.dirty
                    {
                        self.collaboration.document = Some(id);
                        self.collaboration.node_id = Some(node.id.clone());
                        self.collaboration.name = node.name.clone();
                        self.collaboration.is_subconversation =
                            matches!(node.kind, ConversationKind::SubConversation);
                        self.collaboration.parent = node.parent.clone();
                        self.collaboration.participants = node.participant_refs.clone();
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
        let label = ui.label("Name");
        if ui
            .text_edit_singleline(&mut self.collaboration.name)
            .labelled_by(label.id)
            .changed()
        {
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
            .map_or("None", |n| n.name.as_str());
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
                        && self.collaboration.node_id.as_ref() != Some(&n.id)
                }) {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.parent,
                            Some(parent.id.clone()),
                            &parent.name,
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
            let mut selected = self.collaboration.participants.contains(&participant.id);
            if ui.checkbox(&mut selected, &participant.name).changed() {
                if selected {
                    self.collaboration.participants.push(participant.id.clone());
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
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save conversation").clicked() {
                let node_id = self
                    .collaboration
                    .node_id
                    .clone()
                    .unwrap_or_else(|| controller::id("Conversation"));
                let node = ConversationNode {
                    id: node_id.clone(),
                    name: self.collaboration.name.clone(),
                    kind: if self.collaboration.is_subconversation {
                        ConversationKind::SubConversation
                    } else {
                        ConversationKind::Conversation
                    },
                    participant_refs: self.collaboration.participants.clone(),
                    parent: self.collaboration.parent.clone(),
                };
                let mut mutations = vec![if self.collaboration.node_id.is_some() {
                    Mutation::UpdateConversationNode {
                        document_id: id,
                        node,
                    }
                } else {
                    Mutation::CreateConversationNode {
                        document_id: id,
                        node,
                    }
                }];
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
                        mutations.push(Mutation::SetDiagramPlane {
                            document_id: id,
                            diagram_id: diagram.id.clone(),
                            plane: collaboration.id.clone(),
                        });
                    }
                    let count = collaboration.conversation_nodes.len() as f64;
                    mutations.push(Mutation::CreateShape {
                        document_id: id,
                        diagram_id: diagram.id.clone(),
                        shape: Shape {
                            id: controller::id("ConversationShape"),
                            element: node_id.clone(),
                            is_expanded: None,
                            bounds: Bounds {
                                x: self.camera.0 + 120.0,
                                y: self.camera.1 + 80.0 + count * 90.0,
                                width: 120.0,
                                height: 70.0,
                            },
                            label: None,
                            style: None,
                        },
                    });
                }
                if self.commit_draft() && self.ctl.mutate(mutations) {
                    self.collaboration.node_id = Some(node_id);
                    self.collaboration.document = Some(id);
                    self.collaboration.dirty = false;
                }
            }
            if ui.button("Discard draft").clicked() {
                self.collaboration.clear_node();
                self.collaboration.document = Some(id);
            }
            if let Some(node_id) = &self.collaboration.node_id
                && !self.collaboration.has_draft()
                && ui.button("Delete conversation").clicked()
                && self.ctl.mutate(vec![Mutation::DeleteConversationNode {
                    document_id: id,
                    node_id: node_id.clone(),
                }])
            {
                self.collaboration.clear_node();
            }
        });
        ui.separator();
        ui.strong("Conversation links");
        ui.checkbox(&mut self.collaboration.draw, "Add links to current diagram");
        for link in &collaboration.conversation_links {
            let participant = collaboration
                .participants
                .iter()
                .find(|p| p.id == link.source || p.id == link.target)
                .map_or("Unknown participant", |p| p.name.as_str());
            let node = collaboration
                .conversation_nodes
                .iter()
                .find(|n| n.id == link.source || n.id == link.target)
                .map_or("Unknown conversation", |n| n.name.as_str());
            ui.horizontal(|ui| {
                ui.label(format!("{participant} / {node}"));
                if !self.collaboration.has_draft() && ui.button("Delete link").clicked() {
                    self.ctl.mutate(vec![Mutation::DeleteConversationLink {
                        document_id: id,
                        link_id: link.id.clone(),
                    }]);
                }
            });
        }
        egui::ComboBox::from_id_salt("link_participant")
            .selected_text(
                self.collaboration
                    .link_participant
                    .as_ref()
                    .and_then(|id| collaboration.participants.iter().find(|p| &p.id == id))
                    .map_or("Select participant", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for participant in &collaboration.participants {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.link_participant,
                            Some(participant.id.clone()),
                            &participant.name,
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
                    .map_or("Select conversation", |n| n.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for node in &collaboration.conversation_nodes {
                    if ui
                        .selectable_value(
                            &mut self.collaboration.link_conversation,
                            Some(node.id.clone()),
                            &node.name,
                        )
                        .changed()
                    {
                        self.collaboration.link_dirty = true;
                        self.collaboration.document = Some(id);
                    }
                }
            });
        if self.collaboration.link_dirty && ui.button("Discard link draft").clicked() {
            self.collaboration.link_participant = None;
            self.collaboration.link_conversation = None;
            self.collaboration.link_dirty = false;
        }
        if ui.button("Create link").clicked() && !self.collaboration.dirty {
            let (Some(participant), Some(node)) = (
                self.collaboration.link_participant.clone(),
                self.collaboration.link_conversation.clone(),
            ) else {
                self.ctl.error = "Select a participant and conversation.".into();
                return;
            };
            let link_id = controller::id("ConversationLink");
            let mut mutations = vec![Mutation::CreateConversationLink {
                document_id: id,
                link: ConversationLink {
                    id: link_id.clone(),
                    name: String::new(),
                    source: participant.clone(),
                    target: node.clone(),
                },
            }];
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
                    mutations.push(Mutation::SetDiagramPlane {
                        document_id: id,
                        diagram_id: diagram.id.clone(),
                        plane: collaboration.id.clone(),
                    });
                }
                mutations.push(Mutation::CreateEdge {
                    document_id: id,
                    diagram_id: diagram.id.clone(),
                    edge: Edge {
                        id: controller::id("ConversationEdge"),
                        element: link_id,
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
                        label: None,
                        style: None,
                    },
                });
            }
            if self.commit_draft() && self.ctl.mutate(mutations) {
                self.collaboration.link_participant = None;
                self.collaboration.link_conversation = None;
                self.collaboration.link_dirty = false;
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
                ui.label(format!("Store: {}", store.name))
                    .on_hover_text(store.id.to_string());
            }
            if let Some(scope) = scope {
                for object in scope.data_objects {
                    ui.label(format!("Object: {}", object.name))
                        .on_hover_text(object.id.to_string());
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
                            data_store: processcraft_core::bpmn_data::DataStore {
                                id: controller::id("DataStore"),
                                name: self.collaboration.data_name.clone(),
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
                            scope_id: scope.id.clone(),
                            data_object: processcraft_core::bpmn_data::DataObject {
                                id: controller::id("DataObject"),
                                name: self.collaboration.data_name.clone(),
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
                    .map(|v| (&v.id, v.name.as_str()))
                    .collect()
            } else {
                scope
                    .map(|s| {
                        s.data_objects
                            .iter()
                            .map(|v| (&v.id, v.name.as_str()))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            egui::ComboBox::from_id_salt("data_reference_target")
                .selected_text(
                    self.collaboration
                        .reference_target
                        .as_ref()
                        .and_then(|id| targets.iter().find(|(v, _)| *v == id))
                        .map_or("Select definition", |(_, name)| *name),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &targets {
                        if ui
                            .selectable_value(
                                &mut self.collaboration.reference_target,
                                Some((*id).clone()),
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
                                data_store_ref: Some(target),
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
                        process_id: scope.id.clone(),
                        element: Node {
                            id: node_id.clone(),
                            name: self.collaboration.reference_name.clone(),
                            documentation: String::new(),
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
                            diagram_id: diagram.id.clone(),
                            shape: Shape {
                                id: controller::id("DataShape"),
                                element: node_id,
                                is_expanded: None,
                                bounds: Bounds {
                                    x: self.camera.0 + 100.0,
                                    y: self.camera.1 + 100.0,
                                    width: 50.0,
                                    height: 70.0,
                                },
                                label: None,
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
                            .map_or("Select activity", |n| n.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for node in scope.nodes.iter().filter(|n| n.kind.is_activity()) {
                            let selected =
                                self.collaboration.io_activity.as_ref() == Some(&node.id);
                            if ui
                                .add_enabled(
                                    !self.collaboration.io_dirty || selected,
                                    egui::Button::selectable(selected, &node.name),
                                )
                                .clicked()
                                && !selected
                            {
                                self.collaboration.io_activity = Some(node.id.clone());
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
                egui::ComboBox::from_id_salt("io_reference")
                    .selected_text(
                        self.collaboration
                            .io_reference
                            .as_ref()
                            .and_then(|id| scope.nodes.iter().find(|n| &n.id == id))
                            .map_or("No data association", |n| n.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut self.collaboration.io_reference,
                                None,
                                "No data association",
                            )
                            .changed()
                        {
                            self.collaboration.io_dirty = true;
                            self.collaboration.document = Some(doc.id);
                        }
                        for node in scope.nodes.iter().filter(|n| {
                            matches!(
                                n.kind,
                                NodeKind::DataObjectReference | NodeKind::DataStoreReference
                            )
                        }) {
                            if ui
                                .selectable_value(
                                    &mut self.collaboration.io_reference,
                                    Some(node.id.clone()),
                                    &node.name,
                                )
                                .changed()
                            {
                                self.collaboration.io_dirty = true;
                                self.collaboration.document = Some(doc.id);
                            }
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
                        let port = processcraft_core::bpmn_data::DataPort {
                            id: controller::id("DataPort"),
                            name: self.collaboration.io_name.clone(),
                        };
                        let port_id = port.id.clone();
                        if self.collaboration.io_output {
                            io.output_sets[0].data_refs.push(port_id.clone());
                            io.outputs.push(port);
                        } else {
                            io.input_sets[0].data_refs.push(port_id.clone());
                            io.inputs.push(port);
                        }
                        let mut mutations = vec![Mutation::SetNodeIoSpecification {
                            document_id: doc.id,
                            element_id: node.id.clone(),
                            io_specification: Some(io),
                        }];
                        if let Some(reference) = &self.collaboration.io_reference {
                            let assoc_id = controller::id("DataAssociation");
                            let (kind, sources, target) = if self.collaboration.io_output {
                                (
                                    processcraft_core::bpmn_data::DataAssociationKind::Output,
                                    vec![port_id],
                                    reference.clone(),
                                )
                            } else {
                                (
                                    processcraft_core::bpmn_data::DataAssociationKind::Input,
                                    vec![reference.clone()],
                                    port_id,
                                )
                            };
                            mutations.push(Mutation::CreateDataAssociation {
                                document_id: doc.id,
                                scope_id: scope.id.clone(),
                                association: processcraft_core::bpmn_data::DataAssociation {
                                    id: assoc_id.clone(),
                                    owner: node.id.clone(),
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
                                    diagram_id: diagram.id.clone(),
                                    edge: Edge {
                                        id: controller::id("DataAssociationEdge"),
                                        element: assoc_id,
                                        waypoints,
                                        label: None,
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
                            doc.categories
                                .iter()
                                .flat_map(|c| &c.values)
                                .find(|v| &v.id == id)
                        })
                        .and_then(|v| v.value.as_deref())
                        .unwrap_or("Uncategorised group");
                    ui.horizontal(|ui| {
                        ui.label(value).on_hover_text(group.id.to_string());
                        if !self.collaboration.has_draft() && ui.button("Delete group").clicked() {
                            self.ctl.mutate(vec![Mutation::DeleteGroup {
                                document_id: doc.id,
                                scope_id: scope.id.clone(),
                                group_id: group.id.clone(),
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
                            id: category_id,
                            name: Some(self.collaboration.category_name.clone()),
                            values: vec![processcraft_core::bpmn_model::CategoryValue {
                                id: value_id.clone(),
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
                                scope_id: scope.id.clone(),
                                group: processcraft_core::bpmn_model::Group {
                                    id: group_id.clone(),
                                    category_value_ref: Some(value_id),
                                },
                            },
                        ];
                        if let Some(diagram) = doc.diagrams.iter().find(|d| {
                            self.diagram_id.as_ref().is_none_or(|id| &d.id == id)
                                && &d.plane == scope.id
                        }) {
                            mutations.push(Mutation::CreateShape {
                                document_id: doc.id,
                                diagram_id: diagram.id.clone(),
                                shape: Shape {
                                    id: controller::id("GroupShape"),
                                    element: group_id,
                                    is_expanded: None,
                                    bounds: Bounds {
                                        x: self.camera.0 + 80.0,
                                        y: self.camera.1 + 80.0,
                                        width: 260.0,
                                        height: 160.0,
                                    },
                                    label: None,
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
    fn collaboration_choreography(&mut self, ui: &mut egui::Ui, doc: &BpmnDocument) {
        ui.collapsing("Choreography tasks", |ui| {
            ui.collapsing("New choreography", |ui| {
                let mut changed = false;
                for (label, text) in [("Choreography name", &mut self.collaboration.root_name), ("Initiating participant", &mut self.collaboration.root_first), ("Other participant", &mut self.collaboration.root_second), ("First task name", &mut self.collaboration.root_task)] {
                    let label = ui.label(label); changed |= ui.text_edit_singleline(text).labelled_by(label.id).changed();
                }
                if changed { self.collaboration.root_dirty = true; self.collaboration.document = Some(doc.id); }
                ui.horizontal(|ui| {
                    if ui.button("Create choreography and diagram").clicked() {
                        if self.collaboration.root_name.trim().is_empty() || self.collaboration.root_first.trim().is_empty() || self.collaboration.root_second.trim().is_empty() || self.collaboration.root_task.trim().is_empty() { self.ctl.error = "Enter a choreography, two participants, and a first task.".into(); return; }
                        let root = controller::id("Choreography"); let diagram = controller::id("ChoreographyDiagram"); let task = controller::id("ChoreographyTask"); let first = controller::id("Participant"); let second = controller::id("Participant");
                        let choreography = processcraft_core::bpmn_collaboration::Choreography { id:root.clone(), name:self.collaboration.root_name.clone(), participants:vec![Participant {id:first.clone(),name:self.collaboration.root_first.clone(),process:None},Participant {id:second.clone(),name:self.collaboration.root_second.clone(),process:None}],message_flows:vec![],activities:vec![processcraft_core::bpmn_collaboration::ChoreographyActivity{id:task.clone(),name:self.collaboration.root_task.clone(),participant_refs:vec![first.clone(),second],initiating_participant:first,message_flow_refs:vec![]}],sequence_flows:vec![] };
                        let diagram_value = Diagram { id:diagram.clone(),plane_id:controller::id("ChoreographyPlane"),plane:root.clone(),shapes:vec![Shape{id:controller::id("ChoreographyShape"),element:task,is_expanded:None,bounds:Bounds{x:80.0,y:80.0,width:180.0,height:100.0},label:None,style:None}],edges:vec![] };
                        if self.commit_draft() && self.ctl.mutate(vec![Mutation::CreateChoreography {document_id:doc.id,choreography},Mutation::CreateDiagram {document_id:doc.id,diagram:diagram_value}]) {
                            self.diagram_id = Some(diagram); self.process_id = None;
                            self.collaboration.root_name.clear(); self.collaboration.root_first.clear(); self.collaboration.root_second.clear(); self.collaboration.root_task.clear(); self.collaboration.root_dirty = false;
                            self.collaboration.choreography_id = Some(root);
                        }
                    }
                    if self.collaboration.root_dirty && ui.button("Discard choreography draft").clicked() { self.collaboration.root_name.clear(); self.collaboration.root_first.clear(); self.collaboration.root_second.clear(); self.collaboration.root_task.clear(); self.collaboration.root_dirty = false; }
                });
            });
            if doc.choreographies.is_empty() { ui.label("No choreography in this model."); return; }
            egui::ComboBox::from_id_salt("choreography_choice").selected_text(self.collaboration.choreography_id.as_ref().and_then(|id| doc.choreographies.iter().find(|c| &c.id == id)).map_or("Select choreography", |c| c.name.as_str())).show_ui(ui, |ui| {
                for c in &doc.choreographies {
                    let selected = self.collaboration.choreography_id.as_ref() == Some(&c.id);
                    if ui.add_enabled(!self.collaboration.choreography_dirty || selected, egui::Button::selectable(selected, &c.name)).clicked() && !selected {
                        self.collaboration.choreography_id = Some(c.id.clone()); self.collaboration.choreography_first = None; self.collaboration.choreography_second = None; self.collaboration.choreography_dirty = true; self.collaboration.document = Some(doc.id);
                    }
                }
            });
            let Some(choreo) = self.collaboration.choreography_id.as_ref().and_then(|id| doc.choreographies.iter().find(|c| &c.id == id)) else { return; };
            for activity in &choreo.activities { ui.label(&activity.name).on_hover_text(activity.id.to_string()); }
            let name_label = ui.label("Task name");
            if ui.text_edit_singleline(&mut self.collaboration.choreography_name).labelled_by(name_label.id).changed() { self.collaboration.choreography_dirty = true; self.collaboration.document = Some(doc.id); }
            egui::ComboBox::from_id_salt("choreography_first").selected_text(self.collaboration.choreography_first.as_ref().and_then(|id| choreo.participants.iter().find(|p| &p.id == id)).map_or("Initiating participant", |p| p.name.as_str())).show_ui(ui, |ui| {
                for participant in &choreo.participants { if ui.selectable_value(&mut self.collaboration.choreography_first, Some(participant.id.clone()), &participant.name).changed() { self.collaboration.choreography_dirty = true; self.collaboration.document = Some(doc.id); } }
            });
            egui::ComboBox::from_id_salt("choreography_second").selected_text(self.collaboration.choreography_second.as_ref().and_then(|id| choreo.participants.iter().find(|p| &p.id == id)).map_or("Other participant", |p| p.name.as_str())).show_ui(ui, |ui| {
                for participant in &choreo.participants { if ui.selectable_value(&mut self.collaboration.choreography_second, Some(participant.id.clone()), &participant.name).changed() { self.collaboration.choreography_dirty = true; self.collaboration.document = Some(doc.id); } }
            });
            ui.label("The task is added to the model; a shape is added when the current diagram shows this choreography.");
            ui.horizontal(|ui| {
                if ui.button("Create choreography task").clicked() {
                    let (Some(first),Some(second)) = (self.collaboration.choreography_first.clone(),self.collaboration.choreography_second.clone()) else { self.ctl.error = "Select two participants.".into(); return; };
                    if first == second { self.ctl.error = "Select two different participants.".into(); return; }
                    let node_id = controller::id("ChoreographyTask");
                    let mut mutations = vec![Mutation::CreateChoreographyActivity {document_id:doc.id,choreography_id:choreo.id.clone(),activity:processcraft_core::bpmn_collaboration::ChoreographyActivity {id:node_id.clone(),name:self.collaboration.choreography_name.clone(),participant_refs:vec![first.clone(),second],initiating_participant:first,message_flow_refs:vec![]}}];
                    if let Some(diagram) = doc.diagrams.iter().find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id) && d.plane == choreo.id) {
                        mutations.push(Mutation::CreateShape {document_id:doc.id,diagram_id:diagram.id.clone(),shape:Shape{id:controller::id("ChoreographyShape"),element:node_id,is_expanded:None,bounds:Bounds{x:self.camera.0+120.0,y:self.camera.1+80.0+choreo.activities.len() as f64*90.0,width:160.0,height:100.0},label:None,style:None}});
                    }
                    if self.commit_draft() && self.ctl.mutate(mutations) { self.collaboration.choreography_name.clear(); self.collaboration.choreography_first = None; self.collaboration.choreography_second = None; self.collaboration.choreography_dirty = false; }
                }
                if self.collaboration.choreography_dirty && ui.button("Discard task draft").clicked() { self.collaboration.choreography_name.clear(); self.collaboration.choreography_first = None; self.collaboration.choreography_second = None; self.collaboration.choreography_id = None; self.collaboration.choreography_dirty = false; }
            });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
