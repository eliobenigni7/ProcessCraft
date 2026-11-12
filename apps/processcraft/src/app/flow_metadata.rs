use super::callable::reference_list_ui;
use super::event::{ReferenceDraft, local_choice_label};
use super::*;
use processcraft_core::bpmn_activity::FlowElementMetadata;
pub(super) struct FlowDraft {
    target: ElementSelector,
    node: bool,
    present: bool,
    value: FlowElementMetadata,
    categories: Vec<ReferenceDraft>,
    incoming: Vec<ReferenceDraft>,
    outgoing: Vec<ReferenceDraft>,
    error: String,
}
impl FlowDraft {
    fn new(target: ElementSelector, node: bool, value: Option<FlowElementMetadata>) -> Self {
        let present = value.is_some();
        let value = value.unwrap_or_default();
        let categories = value
            .category_value_refs
            .iter()
            .map(ReferenceDraft::new)
            .collect();
        let incoming = value.incoming.iter().map(ReferenceDraft::new).collect();
        let outgoing = value.outgoing.iter().map(ReferenceDraft::new).collect();
        Self {
            target,
            node,
            present,
            value,
            categories,
            incoming,
            outgoing,
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Option<FlowElementMetadata>, String> {
        if !self.present {
            return Ok(None);
        }
        let mut value = self.value.clone();
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
        value.category_value_refs = self
            .categories
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        Ok(Some(value))
    }
}
impl ProcessCraft {
    pub(super) fn flow_metadata_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        target: ElementSelector,
        node: bool,
        value: Option<FlowElementMetadata>,
        editable: bool,
    ) {
        if self.flow_metadata_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit flow details…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.flow_metadata_draft = Some(FlowDraft::new(target, node, value));
            }
            return;
        }
        let draft = self.flow_metadata_draft.as_mut().unwrap();
        if draft.target != target {
            return;
        }
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
            if !draft.node {
                super::catalog::optional_bool(ui, "Immediate", &mut draft.value.is_immediate);
            }
            if draft.node
                && processcraft_core::queries::find_node(document, &target).is_some_and(|node| {
                    !matches!(
                        node.kind,
                        NodeKind::DataObjectReference | NodeKind::DataStoreReference
                    )
                })
            {
                let mut incoming = Vec::new();
                let mut outgoing = Vec::new();
                for scope in processcraft_core::queries::scopes(document) {
                    for flow in scope
                        .flows
                        .iter()
                        .filter(|flow| flow.kind == FlowKind::Sequence)
                    {
                        let label = local_choice_label(
                            flow.name.as_deref(),
                            "Sequence flow",
                            &flow.id,
                            incoming.len() + outgoing.len() + 1,
                        );
                        if matches!(&flow.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == target)
                        {
                            incoming.push((flow.id.selector(), label.clone()));
                        }
                        if matches!(&flow.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == target)
                        {
                            outgoing.push((flow.id.selector(), label));
                        }
                    }
                }
                ui.collapsing("Declared incoming flows", |ui| {
                    reference_list_ui(ui, "Incoming flow", &mut draft.incoming, &incoming)
                });
                ui.collapsing("Declared outgoing flows", |ui| {
                    reference_list_ui(ui, "Outgoing flow", &mut draft.outgoing, &outgoing)
                });
            }
            let choices: Vec<_> = document
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
                .collect();
            reference_list_ui(ui, "Category value", &mut draft.categories, &choices);
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply flow details").clicked() && self.apply_flow_metadata_draft().is_ok() {
            self.restore_selection();
        }
        if ui.button("Cancel flow details").clicked() {
            self.flow_metadata_draft = None;
        }
    }
    pub(super) fn apply_flow_metadata_draft(&mut self) -> Result<(), String> {
        self.apply_typed_draft(
            |app| &mut app.flow_metadata_draft,
            |draft, document| {
                let metadata = draft.build().inspect_err(|e| draft.error = e.clone())?;
                Ok(vec![if draft.node {
                    Mutation::SetNodeFlowMetadata {
                        document_id: document.id,
                        element_id: draft.target.clone(),
                        metadata,
                    }
                } else {
                    Mutation::SetFlowMetadata {
                        document_id: document.id,
                        element_id: draft.target.clone(),
                        metadata,
                    }
                }])
            },
        )
    }
}
