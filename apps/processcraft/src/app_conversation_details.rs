use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{
    ConversationKind, ConversationNode, ParticipantAssociation,
};

struct MappingDraft {
    value: ParticipantAssociation,
    inner: Option<ReferenceDraft>,
    outer: Option<ReferenceDraft>,
}
pub(super) struct ConversationDetailsDraft {
    generation: u64,
    root: ElementSelector,
    node: ConversationNode,
    participants: Vec<Option<ReferenceDraft>>,
    messages: Vec<Option<ReferenceDraft>>,
    called: Option<ReferenceDraft>,
    mappings: Vec<MappingDraft>,
}
impl ConversationDetailsDraft {
    fn new(root: ElementSelector, node: &ConversationNode, generation: u64) -> Self {
        let call = node.call.as_ref();
        Self {
            generation,
            root,
            node: node.clone(),
            participants: node
                .participant_refs
                .iter()
                .map(|r| Some(ReferenceDraft::new(r)))
                .collect(),
            messages: node
                .message_flow_refs
                .iter()
                .map(|r| Some(ReferenceDraft::new(r)))
                .collect(),
            called: call
                .and_then(|c| c.called_collaboration_ref.as_ref())
                .map(ReferenceDraft::new),
            mappings: call.map_or_else(Vec::new, |c| {
                c.participant_associations
                    .iter()
                    .map(|value| MappingDraft {
                        value: value.clone(),
                        inner: value.inner.as_ref().map(ReferenceDraft::new),
                        outer: value.outer.as_ref().map(ReferenceDraft::new),
                    })
                    .collect()
            }),
        }
    }
    fn build(&self) -> Result<ConversationNode, String> {
        let mut node = self.node.clone();
        node.participant_refs = self
            .participants
            .iter()
            .map(|v| {
                v.as_ref()
                    .ok_or_else(|| "Select a participant or remove the reference.".to_owned())?
                    .build()
            })
            .collect::<Result<_, _>>()?;
        node.message_flow_refs = self
            .messages
            .iter()
            .map(|v| {
                v.as_ref()
                    .ok_or_else(|| "Select a message flow or remove the reference.".to_owned())?
                    .build()
            })
            .collect::<Result<_, _>>()?;
        if node.kind == ConversationKind::CallConversation {
            let mut call = node.call.clone().unwrap_or_default();
            call.called_collaboration_ref = build_ref(&self.called)?;
            call.participant_associations = self
                .mappings
                .iter()
                .map(|d| {
                    let mut value = d.value.clone();
                    value.inner = build_ref(&d.inner)?;
                    value.outer = build_ref(&d.outer)?;
                    Ok(value)
                })
                .collect::<Result<_, String>>()?;
            if node.call.is_some()
                || call.called_collaboration_ref.is_some()
                || !call.participant_associations.is_empty()
            {
                node.call = Some(call);
            }
        } else if node.call.is_some() {
            return Err("Remove the call details before changing conversation kind.".into());
        }
        Ok(node)
    }
}
impl ProcessCraft {
    pub(super) fn conversation_details_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        node: &ConversationNode,
        generation: u64,
    ) {
        if self.collaboration.conversation_details.is_none() {
            if ui
                .add_enabled(
                    !self.collaboration.has_draft(),
                    egui::Button::new("Edit conversation details…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.collaboration.document = Some(document.id);
                self.collaboration.conversation_details = Some(ConversationDetailsDraft::new(
                    root.id.selector(),
                    node,
                    generation,
                ));
            }
            return;
        }
        let participant_choices: Vec<_> = root
            .participants
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id.selector(),
                    super::event::local_choice_label(
                        p.name.as_deref(),
                        "Participant",
                        &p.id,
                        i + 1,
                    ),
                )
            })
            .collect();
        let message_choices: Vec<_> = root
            .flows
            .iter()
            .enumerate()
            .map(|(i, flow)| {
                (
                    flow.id.selector(),
                    super::event::local_choice_label(
                        flow.name.as_deref(),
                        "Message flow",
                        &flow.id,
                        i + 1,
                    ),
                )
            })
            .collect();
        let root_choices: Vec<_> = document
            .collaboration
            .iter()
            .chain(document.additional_collaborations.iter())
            .enumerate()
            .map(|(i, c)| {
                (
                    c.id.selector(),
                    super::event::local_choice_label(
                        c.name.as_deref(),
                        "Collaboration",
                        &c.id,
                        i + 1,
                    ),
                )
            })
            .collect();
        let mut draft = self
            .collaboration
            .conversation_details
            .take()
            .expect("open");
        let mut close = false;
        ui.group(|ui| {
            ui.strong("Conversation details");
            egui::ComboBox::from_label("Kind").selected_text(match draft.node.kind { ConversationKind::Conversation => "Conversation", ConversationKind::SubConversation => "Sub-conversation", ConversationKind::CallConversation => "Call conversation" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut draft.node.kind, ConversationKind::Conversation, "Conversation");
                ui.selectable_value(&mut draft.node.kind, ConversationKind::SubConversation, "Sub-conversation");
                ui.selectable_value(&mut draft.node.kind, ConversationKind::CallConversation, "Call conversation");
            });
            ui.label("Participant references");
            let mut remove = None;
            for (index, reference) in draft.participants.iter_mut().enumerate() { ui.push_id(("participant", index), |ui| { ref_ui(ui, &format!("Participant {}", index + 1), reference, &participant_choices); if ui.button("Remove participant reference").clicked() { remove = Some(index); } }); }
            if let Some(index) = remove { draft.participants.remove(index); }
            if ui.add_enabled(draft.participants.len() < 256, egui::Button::new("Add participant reference")).clicked() { draft.participants.push(Some(ReferenceDraft::Local(None))); }
            if draft.node.kind == ConversationKind::CallConversation { ui.label("Call conversations cannot contain message-flow references in BPMN export."); }
            ui.label("Message flows");
            let mut remove = None;
            for (index, reference) in draft.messages.iter_mut().enumerate() { ui.push_id(("message", index), |ui| { ref_ui(ui, &format!("Message flow {}", index + 1), reference, &message_choices); if ui.button("Remove message flow").clicked() { remove = Some(index); } }); }
            if let Some(index) = remove { draft.messages.remove(index); }
            if ui.add_enabled(draft.messages.len() < 256, egui::Button::new("Add message flow")).clicked() { draft.messages.push(Some(ReferenceDraft::Local(None))); }
            if draft.node.kind == ConversationKind::CallConversation {
                ref_ui(ui, "Called collaboration", &mut draft.called, &root_choices);
                let inner_choices: Vec<_> = match &draft.called {
                    Some(ReferenceDraft::Local(Some(id))) => document.collaboration.iter().chain(document.additional_collaborations.iter()).find(|candidate| candidate.id == *id).map(|candidate| &candidate.participants).or_else(|| document.choreographies.iter().find(|candidate| candidate.id == *id).map(|candidate| &candidate.participants)).into_iter().flatten().enumerate().map(|(i, value)| (value.id.selector(), super::event::local_choice_label(value.name.as_deref(), "Participant", &value.id, i + 1))).collect(),
                    _ => Vec::new(),
                };
                ui.strong("Participant mappings");
                let mut remove = None;
                for (index, mapping) in draft.mappings.iter_mut().enumerate() { ui.push_id(("mapping", index), |ui| {
                    ref_ui(ui, "Inner participant", &mut mapping.inner, &inner_choices);
                    ref_ui(ui, "Outer participant", &mut mapping.outer, &participant_choices);
                    if ui.button("Remove mapping").clicked() { remove = Some(index); }
                }); }
                if let Some(index) = remove { draft.mappings.remove(index); }
                if ui.add_enabled(draft.mappings.len() < 256, egui::Button::new("Add mapping")).clicked() { draft.mappings.push(MappingDraft { value: ParticipantAssociation { id: None, base: None, inner: None, outer: None }, inner: None, outer: None }); }
            } else if draft.node.call.is_some() && ui.button("Remove call details").clicked() { draft.node.call = None; draft.called = None; draft.mappings.clear(); }
            ui.horizontal(|ui| {
                if ui.button("Apply conversation details").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { match draft.build() { Ok(node) => { let mut updated = root.clone(); if let Some(current) = updated.conversation_nodes.iter_mut().find(|v| v.id == node.id) { *current = node; if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { self.collaboration.clear_node(); close = true; } } else { self.ctl.error = "The conversation no longer exists.".into(); } }, Err(error) => self.ctl.error = error } }
                }
                if ui.button("Discard conversation details").clicked() { close = true; }
            });
            if !close { self.collaboration.conversation_details = Some(draft); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_model::QNameRef;
    #[test]
    fn call_draft_keeps_external_references_and_absent_payload() {
        let mut node = ConversationNode {
            id: "Conversation".parse().unwrap(),
            name: None,
            kind: ConversationKind::CallConversation,
            parent: None,
            base: None,
            correlation_keys: vec![],
            message_flow_refs: vec![],
            call: None,
            participant_refs: vec![ModelReference::Qualified(QNameRef {
                namespace: "urn:external".into(),
                local: "Actor".parse().unwrap(),
            })],
        };
        let root: ElementSelector = "Root".parse().unwrap();
        let draft = ConversationDetailsDraft::new(root.clone(), &node, 1);
        let built = draft.build().unwrap();
        assert!(built.call.is_none());
        assert_eq!(built.participant_refs, node.participant_refs);
        node.call = Some(Default::default());
        let built = ConversationDetailsDraft::new(root, &node, 1)
            .build()
            .unwrap();
        assert!(built.call.is_some());
    }
}
