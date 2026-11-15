//! Editor for the kind, participant/message-flow references and call mappings of a conversation node.
use super::event::{ReferenceDraft, build_ref, local_choice_label, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{
    ConversationKind, ConversationNode, ParticipantAssociation,
};

/// Upper bound on the entries of each reference list in a draft.
const MAX_ENTRIES: usize = 256;

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

fn kind_label(kind: ConversationKind) -> &'static str {
    match kind {
        ConversationKind::Conversation => "Conversation",
        ConversationKind::SubConversation => "Sub-conversation",
        ConversationKind::CallConversation => "Call conversation",
    }
}

/// An editable list of references with per-item "Remove" and a bounded "Add" button.
fn reference_list_ui(
    ui: &mut egui::Ui,
    (id_salt, item_label, remove_label, add_label): (&str, &str, &str, &str),
    references: &mut Vec<Option<ReferenceDraft>>,
    choices: &[(ElementSelector, String)],
) {
    let mut remove = None;
    for (index, reference) in references.iter_mut().enumerate() {
        ui.push_id((id_salt, index), |ui| {
            ref_ui(
                ui,
                &format!("{item_label} {}", index + 1),
                reference,
                choices,
            );
            if ui.button(remove_label).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        references.remove(index);
    }
    let has_room = references.len() < MAX_ENTRIES;
    if ui
        .add_enabled(has_room, egui::Button::new(add_label))
        .clicked()
    {
        references.push(Some(ReferenceDraft::Local(None)));
    }
}

/// Participants of the collaboration or choreography `called` points at.
fn called_participant_choices(
    document: &BpmnDocument,
    called: &Option<ReferenceDraft>,
) -> Vec<(ElementSelector, String)> {
    let Some(ReferenceDraft::Local(Some(id))) = called else {
        return Vec::new();
    };
    let collaboration = document
        .collaboration
        .iter()
        .chain(document.additional_collaborations.iter())
        .find(|candidate| candidate.id == *id)
        .map(|candidate| &candidate.participants);
    let participants = collaboration.or_else(|| {
        document
            .choreographies
            .iter()
            .find(|candidate| candidate.id == *id)
            .map(|candidate| &candidate.participants)
    });
    participants
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, value)| {
            let label = local_choice_label(value.name.as_deref(), "Participant", &value.id, i + 1);
            (value.id.selector(), label)
        })
        .collect()
}

impl ProcessCraft {
    /// Applies the draft to `root`. Returns whether the editor should close.
    fn apply_conversation_details(
        &mut self,
        document: &BpmnDocument,
        root: &Collaboration,
        draft: &ConversationDetailsDraft,
    ) -> bool {
        if draft.generation != self.ctl.session.snapshot().generation
            || draft.root != root.id.selector()
        {
            self.ctl.error =
                "The model changed while this draft was open. Discard it and reopen the editor."
                    .into();
            return false;
        }
        let node = match draft.build() {
            Ok(node) => node,
            Err(error) => {
                self.ctl.error = error;
                return false;
            }
        };
        let mut updated = root.clone();
        let Some(current) = updated
            .conversation_nodes
            .iter_mut()
            .find(|v| v.id == node.id)
        else {
            self.ctl.error = "The conversation no longer exists.".into();
            return false;
        };
        *current = node;
        let update = super::collaboration::update_root(document, updated);
        if !self.ctl.mutate(vec![update]) {
            return false;
        }
        self.collaboration.clear_node();
        true
    }

    pub(super) fn conversation_details_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        node: &ConversationNode,
        generation: u64,
    ) {
        if self.collaboration.conversation_details.is_none() {
            let edit = egui::Button::new("Edit conversation details…");
            if ui
                .add_enabled(!self.collaboration.has_draft(), edit)
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
                let label = local_choice_label(p.name.as_deref(), "Participant", &p.id, i + 1);
                (p.id.selector(), label)
            })
            .collect();
        let message_choices: Vec<_> = root
            .flows
            .iter()
            .enumerate()
            .map(|(i, flow)| {
                let label =
                    local_choice_label(flow.name.as_deref(), "Message flow", &flow.id, i + 1);
                (flow.id.selector(), label)
            })
            .collect();
        let root_choices: Vec<_> = document
            .collaboration
            .iter()
            .chain(document.additional_collaborations.iter())
            .enumerate()
            .map(|(i, c)| {
                let label = local_choice_label(c.name.as_deref(), "Collaboration", &c.id, i + 1);
                (c.id.selector(), label)
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
            egui::ComboBox::from_label("Kind")
                .selected_text(kind_label(draft.node.kind))
                .show_ui(ui, |ui| {
                    for kind in [
                        ConversationKind::Conversation,
                        ConversationKind::SubConversation,
                        ConversationKind::CallConversation,
                    ] {
                        ui.selectable_value(&mut draft.node.kind, kind, kind_label(kind));
                    }
                });
            ui.label("Participant references");
            reference_list_ui(
                ui,
                (
                    "participant",
                    "Participant",
                    "Remove participant reference",
                    "Add participant reference",
                ),
                &mut draft.participants,
                &participant_choices,
            );
            let is_call = draft.node.kind == ConversationKind::CallConversation;
            if is_call {
                ui.label(
                    "Call conversations cannot contain message-flow references in BPMN export.",
                );
            }
            ui.label("Message flows");
            reference_list_ui(
                ui,
                (
                    "message",
                    "Message flow",
                    "Remove message flow",
                    "Add message flow",
                ),
                &mut draft.messages,
                &message_choices,
            );
            if is_call {
                ref_ui(ui, "Called collaboration", &mut draft.called, &root_choices);
                let inner_choices = called_participant_choices(document, &draft.called);
                ui.strong("Participant mappings");
                let mut remove = None;
                for (index, mapping) in draft.mappings.iter_mut().enumerate() {
                    ui.push_id(("mapping", index), |ui| {
                        ref_ui(ui, "Inner participant", &mut mapping.inner, &inner_choices);
                        ref_ui(
                            ui,
                            "Outer participant",
                            &mut mapping.outer,
                            &participant_choices,
                        );
                        if ui.button("Remove mapping").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    draft.mappings.remove(index);
                }
                let has_room = draft.mappings.len() < MAX_ENTRIES;
                if ui
                    .add_enabled(has_room, egui::Button::new("Add mapping"))
                    .clicked()
                {
                    draft.mappings.push(MappingDraft {
                        value: ParticipantAssociation {
                            id: None,
                            base: None,
                            inner: None,
                            outer: None,
                        },
                        inner: None,
                        outer: None,
                    });
                }
            } else if draft.node.call.is_some() && ui.button("Remove call details").clicked() {
                draft.node.call = None;
                draft.called = None;
                draft.mappings.clear();
            }
            ui.horizontal(|ui| {
                if ui.button("Apply conversation details").clicked()
                    && self.apply_conversation_details(document, root, &draft)
                {
                    close = true;
                }
                if ui.button("Discard conversation details").clicked() {
                    close = true;
                }
            });
            if !close {
                self.collaboration.conversation_details = Some(draft);
            }
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
