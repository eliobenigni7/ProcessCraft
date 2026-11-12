use super::callable::{
    BindingDraft, interface_choices, port_choices, reference_list_ui, selector_ui,
};
use super::catalog::{optional_bool, optional_text};
use super::event::{
    PropertyDraft, ReferenceDraft, build_ref, local_choice_label, local_choices, ref_ui,
};
use super::inspector::{ExpressionDefault, optional_expression_ui_with_default};
use super::roles::RolesDraft;
use super::*;
use processcraft_core::bpmn_activity::{
    CorrelationPropertyBinding, CorrelationSubscription, IoBinding, ProcessActivityMetadata,
    ProcessType,
};
use processcraft_core::bpmn_model::EventProperty;

struct SubscriptionDraft {
    value: CorrelationSubscription,
    key: Option<ReferenceDraft>,
    bindings: Vec<CorrelationBindingDraft>,
}
struct CorrelationBindingDraft {
    value: CorrelationPropertyBinding,
    property: Option<ReferenceDraft>,
}
pub(super) struct ProcessActivityDraft {
    target: ElementSelector,
    present: bool,
    value: ProcessActivityMetadata,
    roles: RolesDraft,
    properties: Vec<PropertyDraft>,
    collaboration: Option<ReferenceDraft>,
    supports: Vec<ReferenceDraft>,
    interfaces: Vec<ReferenceDraft>,
    bindings: Vec<BindingDraft>,
    subscriptions: Vec<SubscriptionDraft>,
    error: String,
}
fn correlation_keys(document: &BpmnDocument) -> Vec<(ElementSelector, String)> {
    let mut keys = Vec::new();
    let mut add = |key: &processcraft_core::bpmn_activity::CorrelationKey| {
        if let Some(id) = &key.id {
            keys.push((
                id.selector(),
                local_choice_label(key.name.as_deref(), "Correlation key", id, keys.len() + 1),
            ));
        }
    };
    if let Some(c) = &document.collaboration {
        for key in &c.correlation_keys {
            add(key);
        }
        for node in &c.conversation_nodes {
            for key in &node.correlation_keys {
                add(key);
            }
        }
    }
    for scope in processcraft_core::queries::choreography_scopes(document) {
        for activity in scope.activities {
            for key in &activity.correlation_keys {
                add(key);
            }
        }
    }
    keys
}
pub(super) fn correlation_properties(document: &BpmnDocument) -> Vec<(ElementSelector, String)> {
    document
        .correlation_properties
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            p.id.as_ref().map(|id| {
                (
                    id.selector(),
                    local_choice_label(p.name.as_deref(), "Correlation property", id, i + 1),
                )
            })
        })
        .collect()
}
impl ProcessActivityDraft {
    fn new(process: &processcraft_core::model::Process) -> Self {
        let present = process.activity_metadata.is_some();
        let value = process.activity_metadata.clone().unwrap_or_default();
        Self {
            target: process.id.selector(),
            present,
            roles: RolesDraft::new(&value.resource_roles),
            properties: value.properties.iter().map(PropertyDraft::new).collect(),
            collaboration: value
                .definitional_collaboration_ref
                .as_ref()
                .map(ReferenceDraft::new),
            supports: value.supports.iter().map(ReferenceDraft::new).collect(),
            interfaces: value
                .callable
                .supported_interface_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            bindings: value
                .callable
                .io_bindings
                .iter()
                .map(|b| BindingDraft {
                    value: b.clone(),
                    operation: b.operation_ref.as_ref().map(ReferenceDraft::new),
                })
                .collect(),
            subscriptions: value
                .correlation_subscriptions
                .iter()
                .map(|s| SubscriptionDraft {
                    value: s.clone(),
                    key: s.key_ref.as_ref().map(ReferenceDraft::new),
                    bindings: s
                        .bindings
                        .iter()
                        .map(|b| CorrelationBindingDraft {
                            value: b.clone(),
                            property: b.property_ref.as_ref().map(ReferenceDraft::new),
                        })
                        .collect(),
                })
                .collect(),
            value,
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Option<ProcessActivityMetadata>, String> {
        if !self.present {
            return Ok(None);
        }
        let mut v = self.value.clone();
        v.resource_roles = self.roles.build()?;
        v.properties = self
            .properties
            .iter()
            .map(PropertyDraft::build)
            .collect::<Result<_, _>>()?;
        v.definitional_collaboration_ref = build_ref(&self.collaboration)?;
        v.supports = self
            .supports
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        v.callable.supported_interface_refs = self
            .interfaces
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        v.callable.io_bindings = self
            .bindings
            .iter()
            .map(|b| {
                let mut v = b.value.clone();
                v.operation_ref = build_ref(&b.operation)?;
                Ok(v)
            })
            .collect::<Result<_, String>>()?;
        v.correlation_subscriptions = self
            .subscriptions
            .iter()
            .map(|s| {
                let mut v = s.value.clone();
                v.key_ref = build_ref(&s.key)?;
                v.bindings = s
                    .bindings
                    .iter()
                    .map(|b| {
                        let mut v = b.value.clone();
                        v.property_ref = build_ref(&b.property)?;
                        Ok(v)
                    })
                    .collect::<Result<_, String>>()?;
                Ok(v)
            })
            .collect::<Result<_, String>>()?;
        Ok(Some(v))
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        process: &processcraft_core::model::Process,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
    ) {
        ui.add_enabled(
            expression_draft.is_none(),
            egui::Checkbox::new(&mut self.present, "Process metadata present"),
        );
        if !self.present {
            return;
        }
        egui::ComboBox::from_label("Process type")
            .selected_text(match self.value.process_type {
                None => "Unspecified",
                Some(ProcessType::None) => "None",
                Some(ProcessType::Public) => "Public",
                Some(ProcessType::Private) => "Private",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.value.process_type, None, "Unspecified");
                for (v, label) in [
                    (ProcessType::None, "None"),
                    (ProcessType::Public, "Public"),
                    (ProcessType::Private, "Private"),
                ] {
                    ui.selectable_value(&mut self.value.process_type, Some(v), label);
                }
            });
        optional_bool(ui, "Closed", &mut self.value.is_closed);
        let collaborations = document.collaboration.as_ref().map_or_else(Vec::new, |c| {
            vec![(
                c.id.selector(),
                local_choice_label(c.name.as_deref(), "Collaboration", &c.id, 1),
            )]
        });
        ref_ui(
            ui,
            "Definitional collaboration",
            &mut self.collaboration,
            &collaborations,
        );
        let processes: Vec<_> = document
            .processes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id.selector(),
                    local_choice_label(p.name.as_deref(), "Process", &p.id, i + 1),
                )
            })
            .collect();
        ui.collapsing("Supported processes", |ui| {
            reference_list_ui(ui, "Process", &mut self.supports, &processes)
        });
        ui.collapsing("Roles", |ui| {
            self.roles.ui(ui, document, expression_draft, "Process")
        });
        ui.collapsing("Properties", |ui| {
            let mut remove = None;
            let mut up = None;
            for (i, p) in self.properties.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.label(format!("Property {}", i + 1));
                    optional_text(ui, "Name", &mut p.name);
                    super::data::item_ui(ui, &mut p.item);
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move property up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove property").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.properties.remove(i);
            } else if let Some(i) = up {
                self.properties.swap(i, i - 1);
            }
            if ui
                .add_enabled(
                    self.properties.len() < 256,
                    egui::Button::new("Add property"),
                )
                .clicked()
            {
                self.properties.push(PropertyDraft::new(&EventProperty {
                    id: None,
                    base: None,
                    name: None,
                    item_aware: None,
                }));
            }
        });
        ui.collapsing("Callable interfaces and bindings", |ui| {
            reference_list_ui(
                ui,
                "Interface",
                &mut self.interfaces,
                &interface_choices(document),
            );
            let operations = local_choices(document, "Operation", None);
            let inputs = port_choices(&process.io_specification, true);
            let outputs = port_choices(&process.io_specification, false);
            let mut remove = None;
            let mut up = None;
            for (i, b) in self.bindings.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.label(format!("Binding {}", i + 1));
                    ref_ui(ui, "Operation", &mut b.operation, &operations);
                    selector_ui(ui, "Input", &mut b.value.input_data_ref, &inputs);
                    selector_ui(ui, "Output", &mut b.value.output_data_ref, &outputs);
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move binding up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove binding").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.bindings.remove(i);
            } else if let Some(i) = up {
                self.bindings.swap(i, i - 1);
            }
            if ui
                .add_enabled(self.bindings.len() < 256, egui::Button::new("Add binding"))
                .clicked()
            {
                self.bindings.push(BindingDraft {
                    value: IoBinding {
                        id: None,
                        base: None,
                        operation_ref: None,
                        input_data_ref: None,
                        output_data_ref: None,
                    },
                    operation: None,
                });
            }
        });
        ui.collapsing("Correlation subscriptions", |ui| {
            let keys = correlation_keys(document);
            let properties = correlation_properties(document);
            let mut remove = None;
            let mut up = None;
            for (i, s) in self.subscriptions.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.label(format!("Subscription {}", i + 1));
                    ref_ui(ui, "Correlation key", &mut s.key, &keys);
                    let mut delete = None;
                    let mut binding_up = None;
                    for (j, b) in s.bindings.iter_mut().enumerate() {
                        ui.push_id(j, |ui| {
                            ui.label(format!("Property binding {}", j + 1));
                            ref_ui(ui, "Correlation property", &mut b.property, &properties);
                            optional_expression_ui_with_default(
                                ui,
                                &format!("Subscription {} binding {} data path", i + 1, j + 1),
                                &mut b.value.data_path,
                                true,
                                expression_draft,
                                ExpressionDefault::ImplicitFormal,
                            );
                            if ui
                                .add_enabled(
                                    expression_draft.is_none() && j > 0,
                                    egui::Button::new("Move property binding up"),
                                )
                                .clicked()
                            {
                                binding_up = Some(j);
                            }
                            if ui
                                .add_enabled(
                                    expression_draft.is_none(),
                                    egui::Button::new("Remove property binding"),
                                )
                                .clicked()
                            {
                                delete = Some(j);
                            }
                        });
                    }
                    if let Some(j) = delete {
                        s.bindings.remove(j);
                    } else if let Some(j) = binding_up {
                        s.bindings.swap(j, j - 1);
                    }
                    if ui
                        .add_enabled(
                            expression_draft.is_none() && s.bindings.len() < 256,
                            egui::Button::new("Add property binding"),
                        )
                        .clicked()
                    {
                        s.bindings.push(CorrelationBindingDraft {
                            value: CorrelationPropertyBinding {
                                id: None,
                                base: None,
                                property_ref: None,
                                data_path: None,
                            },
                            property: None,
                        });
                    }
                    if ui
                        .add_enabled(
                            expression_draft.is_none() && i > 0,
                            egui::Button::new("Move subscription up"),
                        )
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui
                        .add_enabled(
                            expression_draft.is_none(),
                            egui::Button::new("Remove subscription"),
                        )
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.subscriptions.remove(i);
            } else if let Some(i) = up {
                self.subscriptions.swap(i, i - 1);
            }
            if ui
                .add_enabled(
                    expression_draft.is_none() && self.subscriptions.len() < 256,
                    egui::Button::new("Add subscription"),
                )
                .clicked()
            {
                self.subscriptions.push(SubscriptionDraft {
                    value: CorrelationSubscription {
                        id: None,
                        base: None,
                        key_ref: None,
                        bindings: vec![],
                    },
                    key: None,
                    bindings: vec![],
                });
            }
        });
        for (label, value) in [
            ("Auditing", &mut self.value.auditing),
            ("Monitoring", &mut self.value.monitoring),
        ] {
            let mut present = value.is_some();
            ui.checkbox(&mut present, label);
            if present {
                value.get_or_insert_with(Default::default);
            } else {
                *value = None;
            }
        }
    }
}
impl ProcessCraft {
    pub(super) fn process_activity_ui(
        &mut self,
        ui: &mut egui::Ui,
        process: &processcraft_core::model::Process,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.process_activity_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit process behavior…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.process_activity_draft = Some(ProcessActivityDraft::new(process));
            }
            return;
        }
        let draft = self.process_activity_draft.as_mut().unwrap();
        if draft.target != process.id.selector() {
            return;
        }
        ui.strong("Process behavior");
        draft.ui(ui, document, process, &mut self.expression_draft);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply process behavior"),
            )
            .clicked()
        {
            match draft.build() {
                Ok(metadata) => {
                    if self.ctl.mutate(vec![Mutation::SetProcessActivityMetadata {
                        document_id: document.id,
                        process_id: draft.target.clone(),
                        metadata,
                    }]) {
                        self.process_activity_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel process behavior"),
            )
            .clicked()
        {
            self.process_activity_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_draft_preserves_omission_and_nested_owner_metadata() {
        let app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let mut process = snapshot.project.documents[0].processes[0].clone();
        process.activity_metadata = Some(ProcessActivityMetadata {
            is_closed: Some(false),
            properties: vec![EventProperty {
                id: None,
                base: Some(Default::default()),
                name: Some(String::new()),
                item_aware: None,
            }],
            ..Default::default()
        });
        let draft = ProcessActivityDraft::new(&process);
        assert_eq!(draft.build().unwrap(), process.activity_metadata);
    }
}
