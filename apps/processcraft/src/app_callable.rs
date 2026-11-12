use super::catalog::optional_text;
use super::event::{ReferenceDraft, build_ref, local_choice_label, local_choices, ref_ui};
use super::roles::RolesDraft;
use super::xml::MixedDraft;
use super::*;
use processcraft_core::bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind, IoBinding};
use processcraft_core::bpmn_xml::MixedXml;

pub(super) struct BindingDraft {
    pub(super) value: IoBinding,
    pub(super) operation: Option<ReferenceDraft>,
}
struct GlobalDraft {
    value: GlobalTask,
    roles: RolesDraft,
    interfaces: Vec<ReferenceDraft>,
    bindings: Vec<BindingDraft>,
    script: Option<MixedDraft>,
    io: super::callable_io::CallableIoDraft,
}
pub(super) struct CallableCatalogDraft {
    tasks: Vec<GlobalDraft>,
    error: String,
}
pub(super) fn interface_choices(document: &BpmnDocument) -> Vec<(ElementSelector, String)> {
    document
        .catalog
        .interfaces
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            v.id.as_ref().map(|id| {
                (
                    id.selector(),
                    local_choice_label(Some(&v.name), "Interface", id, i + 1),
                )
            })
        })
        .collect()
}
pub(super) fn reference_list_ui(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut Vec<ReferenceDraft>,
    choices: &[(ElementSelector, String)],
) {
    let mut remove = None;
    let mut up = None;
    for (i, value) in values.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let mut v = Some(value.clone());
            ref_ui(ui, &format!("{label} {}", i + 1), &mut v, choices);
            if let Some(v) = v {
                *value = v;
            } else {
                remove = Some(i);
            }
            if ui
                .add_enabled(i > 0, egui::Button::new("Move up"))
                .clicked()
            {
                up = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        values.remove(i);
    } else if let Some(i) = up {
        values.swap(i, i - 1);
    }
    if ui
        .add_enabled(
            values.len() < 256,
            egui::Button::new(format!("Add {label}")),
        )
        .clicked()
    {
        values.push(ReferenceDraft::Local(None));
    }
}
pub(super) fn port_choices(
    io: &Option<processcraft_core::bpmn_data::IoSpecification>,
    input: bool,
) -> Vec<(ElementSelector, String)> {
    io.as_ref()
        .into_iter()
        .flat_map(|io| if input { &io.inputs } else { &io.outputs })
        .enumerate()
        .filter_map(|(i, p)| {
            p.id.as_ref().map(|id| {
                (
                    id.selector(),
                    local_choice_label(
                        p.name.as_deref(),
                        if input { "Input" } else { "Output" },
                        id,
                        i + 1,
                    ),
                )
            })
        })
        .collect()
}
pub(super) fn selector_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<ElementSelector>,
    choices: &[(ElementSelector, String)],
) {
    egui::ComboBox::from_label(label)
        .selected_text(value.as_ref().map_or_else(
            || "Unspecified".into(),
            |id| {
                choices
                    .iter()
                    .find(|(c, _)| c == id)
                    .map_or_else(|| "Unavailable target".into(), |(_, name)| name.clone())
            },
        ))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Unspecified");
            for (id, name) in choices {
                ui.selectable_value(value, Some(id.clone()), name);
            }
        });
}
impl GlobalDraft {
    fn new(value: &GlobalTask) -> Self {
        Self {
            value: value.clone(),
            roles: RolesDraft::new(&value.resource_roles),
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
                .map(|v| BindingDraft {
                    value: v.clone(),
                    operation: v.operation_ref.as_ref().map(ReferenceDraft::new),
                })
                .collect(),
            io: super::callable_io::CallableIoDraft::new(&value.io_specification),
            script: match &value.kind {
                GlobalTaskKind::Script { script, .. } => script.as_ref().map(MixedDraft::new),
                _ => None,
            },
        }
    }
    fn build(&self) -> Result<GlobalTask, String> {
        let mut value = self.value.clone();
        value.resource_roles = self.roles.build()?;
        value.io_specification = self.io.build()?;
        value.callable = CallableMetadata {
            supported_interface_refs: self
                .interfaces
                .iter()
                .map(ReferenceDraft::build)
                .collect::<Result<_, _>>()?,
            io_bindings: self
                .bindings
                .iter()
                .map(|b| {
                    let mut v = b.value.clone();
                    v.operation_ref = build_ref(&b.operation)?;
                    Ok(v)
                })
                .collect::<Result<_, String>>()?,
        };
        if let GlobalTaskKind::Script { script, .. } = &mut value.kind {
            *script = self.script.as_ref().map(MixedDraft::build).transpose()?;
        }
        Ok(value)
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
        index: usize,
    ) {
        optional_text(ui, "Name", &mut self.value.name);
        let old = match &self.value.kind {
            GlobalTaskKind::Task => 0,
            GlobalTaskKind::Manual => 1,
            GlobalTaskKind::BusinessRule { .. } => 2,
            GlobalTaskKind::Script { .. } => 3,
            GlobalTaskKind::User { .. } => 4,
            GlobalTaskKind::CallableElement => 5,
        };
        let mut kind = old;
        egui::ComboBox::from_label("Task type")
            .selected_text(
                [
                    "Task",
                    "Manual",
                    "Business rule",
                    "Script",
                    "User",
                    "Callable element",
                ][old],
            )
            .show_ui(ui, |ui| {
                for (i, label) in [
                    "Task",
                    "Manual",
                    "Business rule",
                    "Script",
                    "User",
                    "Callable element",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut kind, i, *label);
                }
            });
        let has_details = match &self.value.kind {
            GlobalTaskKind::BusinessRule { implementation } => implementation.is_some(),
            GlobalTaskKind::User {
                implementation,
                renderings,
            } => implementation.is_some() || !renderings.is_empty(),
            GlobalTaskKind::Script {
                script_language, ..
            } => script_language.is_some() || self.script.is_some(),
            _ => false,
        };
        if kind != old && has_details {
            ui.label("Clear the task-specific details before changing its type.");
        }
        if kind != old && !has_details && expression_draft.is_none() {
            self.value.kind = match kind {
                1 => GlobalTaskKind::Manual,
                2 => GlobalTaskKind::BusinessRule {
                    implementation: None,
                },
                3 => GlobalTaskKind::Script {
                    script_language: None,
                    script: None,
                },
                4 => GlobalTaskKind::User {
                    implementation: None,
                    renderings: vec![],
                },
                5 => GlobalTaskKind::CallableElement,
                _ => GlobalTaskKind::Task,
            };
            self.script = None;
        }
        match &mut self.value.kind {
            GlobalTaskKind::BusinessRule { implementation } => {
                optional_text(ui, "Implementation", implementation)
            }
            GlobalTaskKind::User {
                implementation,
                renderings,
            } => {
                optional_text(ui, "Implementation", implementation);
                let mut remove = None;
                let mut up = None;
                for (i, rendering) in renderings.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.label(format!("Rendering {}", i + 1));
                        if let Some(id) = rendering.id.as_ref().and_then(|id| id.xml_id()) {
                            ui.label(format!("XML ID · {id}"));
                        }
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move rendering up"))
                            .clicked()
                        {
                            up = Some(i);
                        }
                        if ui.button("Remove rendering").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    renderings.remove(i);
                } else if let Some(i) = up {
                    renderings.swap(i, i - 1);
                }
                if ui
                    .add_enabled(renderings.len() < 256, egui::Button::new("Add rendering"))
                    .clicked()
                {
                    renderings.push(Default::default());
                }
            }
            GlobalTaskKind::Script {
                script_language, ..
            } => {
                optional_text(ui, "Script language", script_language);
                let mut present = self.script.is_some();
                ui.checkbox(&mut present, "Script present");
                if present {
                    self.script
                        .get_or_insert_with(|| MixedDraft::new(&MixedXml::Text(String::new())))
                        .ui(ui);
                } else {
                    self.script = None;
                }
            }
            _ => {}
        }
        ui.collapsing("Inputs and outputs", |ui| self.io.ui(ui));
        ui.collapsing("Roles", |ui| {
            self.roles.ui(
                ui,
                document,
                expression_draft,
                &format!("Global task {}", index + 1),
            )
        });
        ui.collapsing("Callable interfaces and bindings", |ui| {
            reference_list_ui(
                ui,
                "Interface",
                &mut self.interfaces,
                &interface_choices(document),
            );
            let operations = local_choices(document, "Operation", None);
            let current_io = self
                .io
                .build()
                .unwrap_or_else(|_| self.value.io_specification.clone());
            let inputs = port_choices(&current_io, true);
            let outputs = port_choices(&current_io, false);
            let mut remove = None;
            let mut up = None;
            for (i, b) in self.bindings.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
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
    }
}
impl CallableCatalogDraft {
    fn new(tasks: &[GlobalTask]) -> Self {
        Self {
            tasks: tasks.iter().map(GlobalDraft::new).collect(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Vec<GlobalTask>, String> {
        self.tasks.iter().map(GlobalDraft::build).collect()
    }
}
impl ProcessCraft {
    pub(super) fn callable_catalog_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snap = self.ctl.session.snapshot();
        let Some(document) = snap.project.documents.get(self.doc) else {
            return;
        };
        if self.callable_catalog_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit global tasks…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.callable_catalog_draft =
                    Some(CallableCatalogDraft::new(&document.global_tasks));
            }
            return;
        }
        let draft = self.callable_catalog_draft.as_mut().unwrap();
        let mut remove = None;
        let mut up = None;
        for (i, task) in draft.tasks.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.strong(format!("Global task {}", i + 1));
                task.ui(ui, document, &mut self.expression_draft, i);
                ui.add_enabled_ui(self.expression_draft.is_none(), |ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move task up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove task").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            draft.tasks.remove(i);
        } else if let Some(i) = up {
            draft.tasks.swap(i, i - 1);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none() && draft.tasks.len() < 256,
                egui::Button::new("Add global task"),
            )
            .clicked()
        {
            draft.tasks.push(GlobalDraft::new(&GlobalTask {
                id: None,
                base: None,
                name: None,
                callable: CallableMetadata::default(),
                io_specification: None,
                resource_roles: vec![],
                kind: GlobalTaskKind::Task,
            }));
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply global tasks"),
            )
            .clicked()
        {
            match draft.build() {
                Ok(global_tasks) => {
                    if self.ctl.mutate(vec![Mutation::SetGlobalTasks {
                        document_id: document.id,
                        global_tasks,
                    }]) {
                        self.callable_catalog_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel global tasks"),
            )
            .clicked()
        {
            self.callable_catalog_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_task_draft_preserves_absent_ids_script_and_metadata() {
        let value = GlobalTask {
            id: None,
            base: Some(Default::default()),
            name: Some(String::new()),
            callable: CallableMetadata::default(),
            io_specification: Some(Default::default()),
            resource_roles: vec![],
            kind: GlobalTaskKind::Script {
                script_language: Some(String::new()),
                script: Some(MixedXml::Text("script".into())),
            },
        };
        let draft = GlobalDraft::new(&value);
        assert_eq!(draft.build().unwrap(), value);
    }
    #[test]
    fn authored_manual_global_task_exports_and_reimports() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let task = GlobalTask {
            id: Some("GlobalReview".parse().unwrap()),
            base: None,
            name: Some("Review".into()),
            callable: CallableMetadata::default(),
            io_specification: None,
            resource_roles: vec![],
            kind: GlobalTaskKind::Manual,
        };
        assert!(app.ctl.mutate(vec![Mutation::SetGlobalTasks {
            document_id,
            global_tasks: vec![task]
        }]));
        let snapshot = app.ctl.session.snapshot();
        let exported =
            processcraft_io::interchange::export_bpmn(&snapshot.project, document_id, true)
                .expect("global task exports");
        let imported = processcraft_io::interchange::prepare_import(&exported.bytes, "global task")
            .expect("global task imports");
        assert!(
            matches!(&imported.document.global_tasks[..], [task] if task.name.as_deref()==Some("Review") && matches!(task.kind,GlobalTaskKind::Manual))
        );
    }
    #[test]
    fn global_tasks_use_shared_history() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let task = GlobalTask {
            id: None,
            base: None,
            name: Some("Review".into()),
            callable: CallableMetadata::default(),
            io_specification: None,
            resource_roles: vec![],
            kind: GlobalTaskKind::Manual,
        };
        assert!(app.ctl.mutate(vec![Mutation::SetGlobalTasks {
            document_id,
            global_tasks: vec![task]
        }]));
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0]
                .global_tasks
                .len(),
            1
        );
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0]
                .global_tasks
                .is_empty()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0]
                .global_tasks
                .len(),
            1
        );
    }
}
