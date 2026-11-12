use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::ConversationLink;

#[derive(Clone)]
enum Connection {
    Message(Box<Flow>),
    Conversation(Box<ConversationLink>),
}
pub(super) struct ConnectionDraft {
    generation: u64,
    root: ElementSelector,
    value: Connection,
    source: Option<ReferenceDraft>,
    target: Option<ReferenceDraft>,
    message: Option<ReferenceDraft>,
    is_new: bool,
}
impl ConnectionDraft {
    fn new(root: &Collaboration, value: Connection, generation: u64, is_new: bool) -> Self {
        let (source, target) = match &value {
            Connection::Message(value) => (&value.source, &value.target),
            Connection::Conversation(value) => (&value.source, &value.target),
        };
        let message = match &value {
            Connection::Message(value) => value.message_ref.as_ref().map(ReferenceDraft::new),
            Connection::Conversation(_) => None,
        };
        Self {
            message,
            generation,
            root: root.id.selector(),
            source: source.as_ref().map(ReferenceDraft::new),
            target: target.as_ref().map(ReferenceDraft::new),
            value,
            is_new,
        }
    }
    fn build(&self) -> Result<Connection, String> {
        let mut value = self.value.clone();
        let source = build_ref(&self.source)?;
        let target = build_ref(&self.target)?;
        match &mut value {
            Connection::Message(v) => {
                v.source = source;
                v.target = target;
                v.message_ref = build_ref(&self.message)?;
            }
            Connection::Conversation(v) => {
                v.source = source;
                v.target = target;
            }
        }
        Ok(value)
    }
}
impl ProcessCraft {
    pub(super) fn collaboration_connections_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
    ) {
        ui.collapsing("Message flows and conversation links", |ui| {
            if self.collaboration.connection.is_none() {
                for (index, value) in root.flows.iter().enumerate() {
                    let name = super::event::local_choice_label(value.name.as_deref(), "Message flow", &value.id, index + 1);
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {name} properties"))).clicked() && self.commit_draft() { self.select(Some(value.id.selector())); }
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Edit {name}…"))).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id);
                    self.collaboration.connection = Some(ConnectionDraft::new(root, Connection::Message(Box::new(value.clone())), generation, false)); }
                }
                for (index, value) in root.conversation_links.iter().enumerate() {
                    let name = super::event::local_choice_label(value.name.as_deref(), "Conversation link", &value.id, index + 1);
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {name} properties"))).clicked() && self.commit_draft() { self.select(Some(value.id.selector())); }
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Edit {name}…"))).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id);
                    self.collaboration.connection = Some(ConnectionDraft::new(root, Connection::Conversation(Box::new(value.clone())), generation, false)); }
                }
                if ui.add_enabled(!self.collaboration.has_draft() && root.flows.len() < 256, egui::Button::new("Add message flow…")).clicked() && self.commit_draft() {
                    self.collaboration.document = Some(document.id);
                    self.collaboration.connection = Some(ConnectionDraft::new(root, Connection::Message(Box::new(Flow { id: processcraft_core::ids::ElementIdentity::anonymous(), kind: FlowKind::Message, source: None, target: None, name: None, documentation: String::new().into(), base_extras: None, condition: None, association_direction: None, flow_metadata: None, message_ref: None })), generation, true));
                }
                return;
            }
            let mut draft = self.collaboration.connection.take().expect("open");
            let mut choices: Vec<_> = root.participants.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1))).collect();
            for scope in processcraft_core::queries::scopes(document) { for node in scope.nodes.iter().filter(|node| node.kind.is_activity() || matches!(node.kind, NodeKind::StartEvent | NodeKind::EndEvent | NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent | NodeKind::BoundaryEvent)) { choices.push((node.id.selector(), super::event::local_choice_label(node.name.as_deref(), "Activity or event", &node.id, choices.len() + 1))); } }
            if matches!(&draft.value, Connection::Conversation(_)) { choices.extend(root.conversation_nodes.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1)))); }
            let mut close = false;
            let name = match &mut draft.value { Connection::Message(v) => &mut v.name, Connection::Conversation(v) => &mut v.name };
            super::catalog::optional_text(ui, "Name", name);
            ref_ui(ui, "Source", &mut draft.source, &choices);
            ref_ui(ui, "Target", &mut draft.target, &choices);
            if matches!(&draft.value, Connection::Message(_)) { ref_ui(ui, "Message", &mut draft.message, &super::event::local_choices(document, "Message", None)); }
            ui.horizontal(|ui| {
                if ui.button("Apply connection").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { match draft.build() { Ok(value) => { let mut updated = root.clone(); match value { Connection::Message(value) => { if draft.is_new { updated.flows.push(*value); } else if let Some(current) = updated.flows.iter_mut().find(|v| v.id == value.id) { *current = *value; } else { self.ctl.error = "The message flow no longer exists.".into(); return; } }, Connection::Conversation(value) => { if draft.is_new { updated.conversation_links.push(*value); } else if let Some(current) = updated.conversation_links.iter_mut().find(|v| v.id == value.id) { *current = *value; } else { self.ctl.error = "The conversation link no longer exists.".into(); return; } } } if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }, Err(error) => self.ctl.error = error } }
                }
                if !draft.is_new && ui.button("Delete connection").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { let mut updated = root.clone(); match &draft.value { Connection::Message(value) => updated.flows.retain(|candidate| candidate.id != value.id), Connection::Conversation(value) => updated.conversation_links.retain(|candidate| candidate.id != value.id) }; if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }
                }
                if ui.button("Discard connection draft").clicked() { close = true; }
            });
            if !close { self.collaboration.connection = Some(draft); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_model::QNameRef;
    #[test]
    fn connection_draft_retains_missing_and_qualified_endpoints() {
        let root = Collaboration {
            kind: None,
            extras: None,
            correlation_keys: vec![],
            base: None,
            name: None,
            id: "Root".parse().unwrap(),
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        let flow = Flow {
            id: "Flow".parse().unwrap(),
            kind: FlowKind::Message,
            source: None,
            target: Some(ModelReference::Qualified(QNameRef {
                namespace: "urn:remote".into(),
                local: "Target".parse().unwrap(),
            })),
            name: None,
            documentation: "docs".into(),
            base_extras: None,
            condition: None,
            association_direction: None,
            flow_metadata: None,
            message_ref: None,
        };
        let draft =
            ConnectionDraft::new(&root, Connection::Message(Box::new(flow.clone())), 1, false);
        let Connection::Message(built) = draft.build().unwrap() else {
            panic!("message flow expected")
        };
        assert_eq!(built.source, None);
        assert_eq!(built.target, flow.target);
        assert_eq!(built.documentation, flow.documentation);
    }
}
