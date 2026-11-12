use super::catalog::{QNameDraft, optional_qname, q_build};
use super::*;
use processcraft_core::bpmn_model::{NodeDetails, QNameRef};
pub(super) struct CallDraft {
    target: ElementSelector,
    called: Option<QNameDraft>,
    error: String,
}
impl ProcessCraft {
    pub(super) fn call_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.call_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit called element…"),
                )
                .clicked()
                && self.commit_draft()
            {
                let called = match &node.details {
                    Some(NodeDetails::CallActivity { called_element }) => {
                        called_element.as_ref().map(QNameDraft::from)
                    }
                    _ => None,
                };
                self.call_draft = Some(CallDraft {
                    target: node.id.selector(),
                    called,
                    error: String::new(),
                });
            }
            return;
        }
        let draft = self.call_draft.as_mut().unwrap();
        if draft.target != node.id.selector() {
            return;
        }
        let namespace = document
            .definitions
            .as_ref()
            .map(|m| m.target_namespace.as_str())
            .or_else(|| {
                document
                    .provenance
                    .is_none()
                    .then_some(processcraft_core::model::GENERATED_TARGET_NAMESPACE)
            });
        if let Some(namespace) = namespace {
            egui::ComboBox::from_label("Choose local callable")
                .selected_text("Select a process or global task")
                .show_ui(ui, |ui| {
                    for (i, process) in document.processes.iter().enumerate() {
                        if let Some(id) = process.id.xml_id() {
                            let label = super::event::local_choice_label(
                                process.name.as_deref(),
                                "Process",
                                &process.id,
                                i + 1,
                            );
                            if ui.button(label).clicked() {
                                draft.called = Some(QNameDraft::from(&QNameRef {
                                    namespace: namespace.into(),
                                    local: id.clone(),
                                }));
                                ui.close();
                            }
                        }
                    }
                    for (i, task) in document.global_tasks.iter().enumerate() {
                        if let Some(identity) = &task.id
                            && let Some(id) = identity.xml_id()
                        {
                            let label = super::event::local_choice_label(
                                task.name.as_deref(),
                                "Global task",
                                identity,
                                i + 1,
                            );
                            if ui.button(label).clicked() {
                                draft.called = Some(QNameDraft::from(&QNameRef {
                                    namespace: namespace.into(),
                                    local: id.clone(),
                                }));
                                ui.close();
                            }
                        }
                    }
                });
        }
        ui.label("Local callables need an XML ID before they can be selected.");
        optional_qname(ui, "Called element", &mut draft.called);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply called element").clicked() {
            match q_build(&draft.called) {
                Ok(called_element) => {
                    if self.ctl.mutate(vec![Mutation::SetNodeDetails {
                        document_id: document.id,
                        element_id: draft.target.clone(),
                        details: Some(NodeDetails::CallActivity { called_element }),
                    }]) {
                        self.call_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel called element").clicked() {
            self.call_draft = None;
        }
    }
}
