use super::callable::reference_list_ui;
use super::event::{ReferenceDraft, local_choice_label};
use super::*;
use processcraft_core::bpmn_activity::FlowElementMetadata;
use processcraft_core::bpmn_collaboration::ChoreographyActivity;
pub(super) struct ChoreographyFlowDraft {
    choreography: ElementSelector,
    activity: Option<ChoreographyActivity>,
    sequence: Option<ElementSelector>,
    present: bool,
    value: FlowElementMetadata,
    categories: Vec<ReferenceDraft>,
    incoming: Vec<ReferenceDraft>,
    outgoing: Vec<ReferenceDraft>,
    error: String,
}
impl ChoreographyFlowDraft {
    fn new(choreography: ElementSelector, activity: ChoreographyActivity) -> Self {
        let present = activity.flow_metadata.is_some();
        let value = activity.flow_metadata.clone().unwrap_or_default();
        Self {
            choreography,
            activity: Some(activity),
            sequence: None,
            categories: value
                .category_value_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            incoming: value.incoming.iter().map(ReferenceDraft::new).collect(),
            outgoing: value.outgoing.iter().map(ReferenceDraft::new).collect(),
            present,
            value,
            error: String::new(),
        }
    }
    fn new_sequence(choreography: ElementSelector, flow: &Flow) -> Self {
        let value = flow.flow_metadata.clone().unwrap_or_default();
        Self {
            choreography,
            activity: None,
            sequence: Some(flow.id.selector()),
            present: flow.flow_metadata.is_some(),
            categories: value
                .category_value_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            incoming: value.incoming.iter().map(ReferenceDraft::new).collect(),
            outgoing: value.outgoing.iter().map(ReferenceDraft::new).collect(),
            value,
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Option<FlowElementMetadata>, String> {
        if !self.present {
            return Ok(None);
        }
        let mut value = self.value.clone();
        value.category_value_refs = self
            .categories
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        value.incoming = self
            .incoming
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        value.outgoing = self
            .outgoing
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        Ok(Some(value))
    }
}
impl ProcessCraft {
    pub(super) fn choreography_flow_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.choreography_flow_draft.is_none() {
            ui.menu_button("Edit choreography flow details…", |ui| {
                for choreography in &document.choreographies {
                    for (i, activity) in choreography.activities.iter().enumerate() {
                        let label = local_choice_label(
                            activity.name.as_deref(),
                            "Choreography activity",
                            &activity.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            self.choreography_flow_draft = Some(ChoreographyFlowDraft::new(
                                choreography.id.selector(),
                                activity.clone(),
                            ));
                            ui.close();
                        }
                    }
                    for (i, flow) in choreography.sequence_flows.iter().enumerate() {
                        let label = local_choice_label(
                            flow.name.as_deref(),
                            "Sequence flow",
                            &flow.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            self.choreography_flow_draft =
                                Some(ChoreographyFlowDraft::new_sequence(
                                    choreography.id.selector(),
                                    flow,
                                ));
                            ui.close();
                        }
                    }
                }
            });
            return;
        }
        let draft = self.choreography_flow_draft.as_mut().unwrap();
        ui.strong("Choreography flow details");
        ui.checkbox(&mut draft.present, "Flow details present");
        if draft.present {
            for (label, value) in [
                ("Auditing", &mut draft.value.auditing),
                ("Monitoring", &mut draft.value.monitoring),
            ] {
                let mut present = value.is_some();
                ui.checkbox(&mut present, label);
                if present {
                    value.get_or_insert_with(Default::default);
                } else {
                    *value = None;
                }
            }
            let categories = document
                .categories
                .iter()
                .flat_map(|c| &c.values)
                .enumerate()
                .map(|(i, v)| {
                    (
                        v.id.selector(),
                        local_choice_label(v.value.as_deref(), "Category value", &v.id, i + 1),
                    )
                })
                .collect::<Vec<_>>();
            reference_list_ui(ui, "Category value", &mut draft.categories, &categories);
            if draft.sequence.is_some() {
                super::catalog::optional_bool(ui, "Immediate", &mut draft.value.is_immediate);
            }
            if let Some(activity) = &draft.activity
                && let Some(choreography) = document
                    .choreographies
                    .iter()
                    .find(|c| c.id == draft.choreography)
            {
                let incoming = choreography
                    .sequence_flows
                    .iter()
                    .filter(|f| f.target == activity.id)
                    .enumerate()
                    .map(|(i, f)| {
                        (
                            f.id.selector(),
                            local_choice_label(f.name.as_deref(), "Sequence flow", &f.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                let outgoing = choreography
                    .sequence_flows
                    .iter()
                    .filter(|f| f.source == activity.id)
                    .enumerate()
                    .map(|(i, f)| {
                        (
                            f.id.selector(),
                            local_choice_label(f.name.as_deref(), "Sequence flow", &f.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                ui.collapsing("Declared incoming flows", |ui| {
                    reference_list_ui(ui, "Incoming flow", &mut draft.incoming, &incoming)
                });
                ui.collapsing("Declared outgoing flows", |ui| {
                    reference_list_ui(ui, "Outgoing flow", &mut draft.outgoing, &outgoing)
                });
            }
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply choreography details").clicked() {
            match draft.build() {
                Ok(metadata) => {
                    let mutation = if let Some(sequence) = &draft.sequence {
                        Mutation::SetFlowMetadata {
                            document_id: document.id,
                            element_id: sequence.clone(),
                            metadata,
                        }
                    } else {
                        let mut activity = draft.activity.clone().expect("activity owner");
                        activity.flow_metadata = metadata;
                        Mutation::UpdateChoreographyActivity {
                            document_id: document.id,
                            choreography_id: draft.choreography.clone(),
                            activity,
                        }
                    };
                    if self.ctl.mutate(vec![mutation]) {
                        self.choreography_flow_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel choreography details").clicked() {
            self.choreography_flow_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choreography_sequence_flow_draft_retains_metadata_and_optional_false() {
        let metadata = FlowElementMetadata {
            is_immediate: Some(false),
            auditing: Some(Default::default()),
            ..Default::default()
        };
        let flow = Flow {
            message_ref: None,
            id: "Flow".parse().unwrap(),
            kind: FlowKind::Sequence,
            source: "Source".parse().unwrap(),
            target: "Target".parse().unwrap(),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
            association_direction: None,
            flow_metadata: Some(metadata.clone()),
        };
        let mut draft = ChoreographyFlowDraft::new_sequence("Choreography".parse().unwrap(), &flow);
        assert_eq!(draft.sequence, Some(flow.id.selector()));
        assert_eq!(draft.build().unwrap(), Some(metadata.clone()));
        draft.value.is_immediate = None;
        let result = draft.build().unwrap().unwrap();
        assert_eq!(result.is_immediate, None);
        assert_eq!(result.auditing, metadata.auditing);
    }
}
