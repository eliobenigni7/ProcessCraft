use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{
    CollaborationExtras, ConversationAssociation, MessageFlowAssociation, ParticipantAssociation,
};

struct AssociationDraft {
    id: Option<processcraft_core::ids::ElementIdentity>,
    base: Option<processcraft_core::bpmn_metadata::BaseMetadata>,
    inner: Option<ReferenceDraft>,
    outer: Option<ReferenceDraft>,
}
impl AssociationDraft {
    fn new(
        id: &Option<processcraft_core::ids::ElementIdentity>,
        base: &Option<processcraft_core::bpmn_metadata::BaseMetadata>,
        inner: &Option<processcraft_core::bpmn_activity::ModelReference>,
        outer: &Option<processcraft_core::bpmn_activity::ModelReference>,
    ) -> Self {
        Self {
            id: id.clone(),
            base: base.clone(),
            inner: inner.as_ref().map(ReferenceDraft::new),
            outer: outer.as_ref().map(ReferenceDraft::new),
        }
    }
    fn empty() -> Self {
        Self {
            id: None,
            base: None,
            inner: None,
            outer: None,
        }
    }
}
pub(super) struct AssociationsDraft {
    generation: u64,
    root: ElementSelector,
    extras: CollaborationExtras,
    conversations: Vec<AssociationDraft>,
    participants: Vec<AssociationDraft>,
    messages: Vec<AssociationDraft>,
    choreography_refs: Vec<Option<ReferenceDraft>>,
}
impl AssociationsDraft {
    fn new(root: &Collaboration, generation: u64) -> Self {
        let extras = root.extras.clone().unwrap_or_default();
        Self {
            generation,
            root: root.id.selector(),
            conversations: extras
                .conversation_associations
                .iter()
                .map(|v| AssociationDraft::new(&v.id, &v.base, &v.inner, &v.outer))
                .collect(),
            participants: extras
                .participant_associations
                .iter()
                .map(|v| AssociationDraft::new(&v.id, &v.base, &v.inner, &v.outer))
                .collect(),
            messages: extras
                .message_flow_associations
                .iter()
                .map(|v| AssociationDraft::new(&v.id, &v.base, &v.inner, &v.outer))
                .collect(),
            choreography_refs: extras
                .choreography_refs
                .iter()
                .map(|v| Some(ReferenceDraft::new(v)))
                .collect(),
            extras,
        }
    }
    fn build(&self) -> Result<CollaborationExtras, String> {
        let mut value = self.extras.clone();
        value.conversation_associations = self
            .conversations
            .iter()
            .map(|d| {
                Ok(ConversationAssociation {
                    id: d.id.clone(),
                    base: d.base.clone(),
                    inner: build_ref(&d.inner)?,
                    outer: build_ref(&d.outer)?,
                })
            })
            .collect::<Result<_, String>>()?;
        value.participant_associations = self
            .participants
            .iter()
            .map(|d| {
                Ok(ParticipantAssociation {
                    id: d.id.clone(),
                    base: d.base.clone(),
                    inner: build_ref(&d.inner)?,
                    outer: build_ref(&d.outer)?,
                })
            })
            .collect::<Result<_, String>>()?;
        value.message_flow_associations = self
            .messages
            .iter()
            .map(|d| {
                Ok(MessageFlowAssociation {
                    id: d.id.clone(),
                    base: d.base.clone(),
                    inner: build_ref(&d.inner)?,
                    outer: build_ref(&d.outer)?,
                })
            })
            .collect::<Result<_, String>>()?;
        value.choreography_refs = self
            .choreography_refs
            .iter()
            .map(|v| {
                v.as_ref()
                    .ok_or_else(|| "Select a choreography or remove the reference.".to_owned())?
                    .build()
            })
            .collect::<Result<_, _>>()?;
        Ok(value)
    }
}
fn association_ui(
    ui: &mut egui::Ui,
    name: &str,
    values: &mut Vec<AssociationDraft>,
    inner_choices: &[(ElementSelector, String)],
    outer_choices: &[(ElementSelector, String)],
) {
    ui.strong(name);
    let mut remove = None;
    for (index, value) in values.iter_mut().enumerate() {
        ui.push_id((name, index), |ui| {
            ui.collapsing(format!("{name} {}", index + 1), |ui| {
                ref_ui(ui, "Inner", &mut value.inner, inner_choices);
                ref_ui(ui, "Outer", &mut value.outer, outer_choices);
                if ui.button("Remove association").clicked() {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        values.remove(index);
    }
    if ui
        .add_enabled(values.len() < 256, egui::Button::new(format!("Add {name}")))
        .clicked()
    {
        values.push(AssociationDraft::empty());
    }
}
impl ProcessCraft {
    pub(super) fn collaboration_associations_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
    ) {
        ui.collapsing("Collaboration associations and settings", |ui| {
            if self.collaboration.associations.is_none() {
                if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Edit associations and settings…")).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id);
                    self.collaboration.associations = Some(AssociationsDraft::new(root, generation)); }
                return;
            }
            let participants: Vec<_> = root.participants.iter().enumerate().map(|(i, v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1))).collect();
            let conversations: Vec<_> = root.conversation_nodes.iter().enumerate().map(|(i, v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1))).collect();
            let messages: Vec<_> = root.flows.iter().enumerate().map(|(i, v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Message flow", &v.id, i + 1))).collect();
            let all_roots: Vec<_> = document.collaboration.iter().chain(document.additional_collaborations.iter()).cloned().chain(document.choreographies.iter().map(super::collaboration::choreography_view)).collect();
            let inner_participants: Vec<_> = all_roots.iter().flat_map(|r| &r.participants).enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1))).collect();
            let inner_conversations: Vec<_> = all_roots.iter().flat_map(|r| &r.conversation_nodes).enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1))).collect();
            let inner_messages: Vec<_> = all_roots.iter().flat_map(|r| &r.flows).enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Message flow", &v.id, i + 1))).collect();
            let choreographies: Vec<_> = document.choreographies.iter().enumerate().map(|(i, v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Choreography", &v.id, i + 1))).collect();
            let mut draft = self.collaboration.associations.take().expect("open");
            let mut close = false;
            super::catalog::optional_bool(ui, "Closed collaboration", &mut draft.extras.is_closed);
            association_ui(ui, "Conversation association", &mut draft.conversations, &inner_conversations, &conversations);
            association_ui(ui, "Participant association", &mut draft.participants, &inner_participants, &participants);
            association_ui(ui, "Message flow association", &mut draft.messages, &inner_messages, &messages);
            ui.strong("Referenced choreographies");
            let mut remove = None;
            for (index, reference) in draft.choreography_refs.iter_mut().enumerate() { ui.push_id(index, |ui| { ref_ui(ui, &format!("Choreography {}", index + 1), reference, &choreographies); if ui.button("Remove choreography reference").clicked() { remove = Some(index); } }); }
            if let Some(index) = remove { draft.choreography_refs.remove(index); }
            if ui.add_enabled(draft.choreography_refs.len() < 256, egui::Button::new("Add choreography reference")).clicked() { draft.choreography_refs.push(Some(ReferenceDraft::Local(None))); }
            ui.horizontal(|ui| {
                if ui.button("Apply associations and settings").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { match draft.build() { Ok(extras) => { let mut updated = root.clone(); updated.extras = if root.extras.is_none() && extras == CollaborationExtras::default() { None } else { Some(extras) }; if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }, Err(error) => self.ctl.error = error } }
                }
                if ui.button("Discard associations draft").clicked() { close = true; }
            });
            if !close { self.collaboration.associations = Some(draft); }
        });
    }
}
