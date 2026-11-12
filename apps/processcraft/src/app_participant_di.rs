use super::catalog::optional_text;
use super::event::{ReferenceDraft, build_ref, local_choice_label, local_choices, ref_ui};
use super::*;
pub(super) struct MultiplicityDraft {
    target: ElementSelector,
    value: Option<ParticipantMultiplicity>,
    minimum: Option<String>,
    maximum: Option<String>,
    error: String,
}
pub(super) struct MessageRefDraft {
    target: ElementSelector,
    value: Option<ReferenceDraft>,
    error: String,
}
impl MultiplicityDraft {
    fn new(participant: &Participant) -> Self {
        Self {
            target: participant.id.selector(),
            minimum: participant
                .multiplicity
                .as_ref()
                .and_then(|m| m.minimum.map(|v| v.to_string())),
            maximum: participant
                .multiplicity
                .as_ref()
                .and_then(|m| m.maximum.map(|v| v.to_string())),
            value: participant.multiplicity.clone(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Option<ParticipantMultiplicity>, String> {
        let Some(mut value) = self.value.clone() else {
            return Ok(None);
        };
        let parse = |value: &Option<String>, label: &str| {
            value
                .as_ref()
                .map(|s| {
                    s.parse::<i32>().map_err(|_| {
                        format!("{label} must be a whole number within the 32-bit range.")
                    })
                })
                .transpose()
        };
        value.minimum = parse(&self.minimum, "Minimum")?;
        value.maximum = parse(&self.maximum, "Maximum")?;
        Ok(Some(value))
    }
}
impl ProcessCraft {
    pub(super) fn participant_multiplicity_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.multiplicity_draft.is_none() {
            ui.menu_button("Edit participant multiplicity…", |ui| {
                let participants = document
                    .collaboration
                    .iter()
                    .flat_map(|c| &c.participants)
                    .chain(document.choreographies.iter().flat_map(|c| &c.participants));
                for (i, p) in participants.enumerate() {
                    let label = local_choice_label(p.name.as_deref(), "Participant", &p.id, i + 1);
                    if ui
                        .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                        .clicked()
                        && self.commit_draft()
                    {
                        self.multiplicity_draft = Some(MultiplicityDraft::new(p));
                        ui.close();
                    }
                }
            });
            return;
        }
        let draft = self.multiplicity_draft.as_mut().unwrap();
        ui.strong("Participant multiplicity");
        let mut present = draft.value.is_some();
        ui.checkbox(&mut present, "Multiplicity present");
        if present {
            draft.value.get_or_insert_with(Default::default);
            optional_text(ui, "Minimum (default 0)", &mut draft.minimum);
            optional_text(ui, "Maximum (default 1)", &mut draft.maximum);
        } else {
            draft.value = None;
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply multiplicity").clicked() {
            match draft.build() {
                Ok(multiplicity) => {
                    if self.ctl.mutate(vec![Mutation::SetParticipantMultiplicity {
                        document_id: document.id,
                        participant_id: draft.target.clone(),
                        multiplicity,
                    }]) {
                        self.multiplicity_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel multiplicity").clicked() {
            self.multiplicity_draft = None;
        }
    }
    pub(super) fn message_reference_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.message_ref_draft.is_none() {
            ui.menu_button("Edit message flow reference…", |ui| {
                let flows = document
                    .collaboration
                    .iter()
                    .flat_map(|c| &c.flows)
                    .filter(|f| f.kind == FlowKind::Message)
                    .chain(
                        document
                            .choreographies
                            .iter()
                            .flat_map(|c| &c.message_flows),
                    );
                for (i, flow) in flows.enumerate() {
                    let label =
                        local_choice_label(flow.name.as_deref(), "Message flow", &flow.id, i + 1);
                    if ui
                        .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                        .clicked()
                        && self.commit_draft()
                    {
                        self.message_ref_draft = Some(MessageRefDraft {
                            target: flow.id.selector(),
                            value: flow.message_ref.as_ref().map(ReferenceDraft::new),
                            error: String::new(),
                        });
                        ui.close();
                    }
                }
            });
            return;
        }
        let draft = self.message_ref_draft.as_mut().unwrap();
        ui.strong("Message flow reference");
        ref_ui(
            ui,
            "Message",
            &mut draft.value,
            &local_choices(document, "Message", None),
        );
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply message reference").clicked() {
            match build_ref(&draft.value) {
                Ok(message_ref) => {
                    if self.ctl.mutate(vec![Mutation::SetMessageFlowReference {
                        document_id: document.id,
                        flow_id: draft.target.clone(),
                        message_ref,
                    }]) {
                        self.message_ref_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel message reference").clicked() {
            self.message_ref_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiplicity_draft_keeps_optional_defaults_identity_and_invalid_text() {
        let participant = Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: "Participant".parse().unwrap(),
            base: None,
            name: None,
            process: None,
            multiplicity: Some(ParticipantMultiplicity {
                id: Some("Multiplicity".parse().unwrap()),
                base: Some(Default::default()),
                minimum: None,
                maximum: Some(1),
            }),
        };
        let mut draft = MultiplicityDraft::new(&participant);
        assert_eq!(draft.build().unwrap(), participant.multiplicity);
        draft.minimum = Some("1x".into());
        assert!(draft.build().is_err());
        assert_eq!(draft.minimum.as_deref(), Some("1x"));
    }
}
