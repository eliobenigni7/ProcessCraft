use super::*;
use processcraft_core::bpmn_catalog::{
    BpmnResource, Interface, ItemDefinition, ItemKind, Operation, ResourceParameter,
    SupportingCatalog,
};
use processcraft_core::bpmn_model::QNameRef;

#[derive(Clone, Default)]
pub(super) struct QNameDraft {
    pub(super) namespace: String,
    pub(super) local: String,
}
impl From<&QNameRef> for QNameDraft {
    fn from(value: &QNameRef) -> Self {
        Self {
            namespace: value.namespace.clone(),
            local: value.local.to_string(),
        }
    }
}
impl QNameDraft {
    fn build(&self) -> Result<QNameRef, String> {
        Ok(QNameRef {
            namespace: self.namespace.clone(),
            local: self
                .local
                .parse()
                .map_err(|_| "Reference local name must be a valid XML ID.".to_owned())?,
        })
    }
}
fn id_string(id: &Option<XmlId>) -> Option<String> {
    id.as_ref().map(ToString::to_string)
}
fn id_build(id: &Option<String>) -> Result<Option<XmlId>, String> {
    id.as_ref()
        .map(|s| {
            s.parse()
                .map_err(|_| "ID must be a valid XML ID.".to_owned())
        })
        .transpose()
}
pub(super) fn q_build(q: &Option<QNameDraft>) -> Result<Option<QNameRef>, String> {
    q.as_ref().map(QNameDraft::build).transpose()
}
#[derive(Clone, Default)]
struct ItemDraft {
    id: Option<String>,
    structure_ref: Option<QNameDraft>,
    is_collection: Option<bool>,
    item_kind: Option<ItemKind>,
}
#[derive(Clone, Default)]
struct OperationDraft {
    id: Option<String>,
    name: String,
    implementation_ref: Option<QNameDraft>,
    in_message_ref: QNameDraft,
    out_message_ref: Option<QNameDraft>,
    error_refs: Vec<QNameDraft>,
}
#[derive(Clone, Default)]
struct InterfaceDraft {
    id: Option<String>,
    name: String,
    implementation_ref: Option<QNameDraft>,
    operations: Vec<OperationDraft>,
}
#[derive(Clone, Default)]
struct ParameterDraft {
    id: Option<String>,
    name: Option<String>,
    type_ref: Option<QNameDraft>,
    is_required: Option<bool>,
}
#[derive(Clone, Default)]
struct ResourceDraft {
    id: Option<String>,
    name: String,
    parameters: Vec<ParameterDraft>,
}
pub(super) struct CatalogDraft {
    items: Vec<ItemDraft>,
    interfaces: Vec<InterfaceDraft>,
    resources: Vec<ResourceDraft>,
    error: String,
}
impl CatalogDraft {
    fn new(catalog: &SupportingCatalog) -> Self {
        Self {
            items: catalog
                .item_definitions
                .iter()
                .map(|x| ItemDraft {
                    id: id_string(&x.id),
                    structure_ref: x.structure_ref.as_ref().map(QNameDraft::from),
                    is_collection: x.is_collection,
                    item_kind: x.item_kind,
                })
                .collect(),
            interfaces: catalog
                .interfaces
                .iter()
                .map(|x| InterfaceDraft {
                    id: id_string(&x.id),
                    name: x.name.clone(),
                    implementation_ref: x.implementation_ref.as_ref().map(QNameDraft::from),
                    operations: x
                        .operations
                        .iter()
                        .map(|o| OperationDraft {
                            id: id_string(&o.id),
                            name: o.name.clone(),
                            implementation_ref: o.implementation_ref.as_ref().map(QNameDraft::from),
                            in_message_ref: QNameDraft::from(&o.in_message_ref),
                            out_message_ref: o.out_message_ref.as_ref().map(QNameDraft::from),
                            error_refs: o.error_refs.iter().map(QNameDraft::from).collect(),
                        })
                        .collect(),
                })
                .collect(),
            resources: catalog
                .resources
                .iter()
                .map(|x| ResourceDraft {
                    id: id_string(&x.id),
                    name: x.name.clone(),
                    parameters: x
                        .parameters
                        .iter()
                        .map(|p| ParameterDraft {
                            id: id_string(&p.id),
                            name: p.name.clone(),
                            type_ref: p.type_ref.as_ref().map(QNameDraft::from),
                            is_required: p.is_required,
                        })
                        .collect(),
                })
                .collect(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<SupportingCatalog, String> {
        Ok(SupportingCatalog {
            item_definitions: self
                .items
                .iter()
                .map(|x| {
                    Ok(ItemDefinition {
                        id: id_build(&x.id)?,
                        structure_ref: q_build(&x.structure_ref)?,
                        is_collection: x.is_collection,
                        item_kind: x.item_kind,
                    })
                })
                .collect::<Result<_, String>>()?,
            interfaces: self
                .interfaces
                .iter()
                .map(|x| {
                    Ok(Interface {
                        id: id_build(&x.id)?,
                        name: x.name.clone(),
                        implementation_ref: q_build(&x.implementation_ref)?,
                        operations: x
                            .operations
                            .iter()
                            .map(|o| {
                                Ok(Operation {
                                    id: id_build(&o.id)?,
                                    name: o.name.clone(),
                                    implementation_ref: q_build(&o.implementation_ref)?,
                                    in_message_ref: o.in_message_ref.build()?,
                                    out_message_ref: q_build(&o.out_message_ref)?,
                                    error_refs: o
                                        .error_refs
                                        .iter()
                                        .map(QNameDraft::build)
                                        .collect::<Result<_, _>>()?,
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    })
                })
                .collect::<Result<_, String>>()?,
            resources: self
                .resources
                .iter()
                .map(|x| {
                    Ok(BpmnResource {
                        id: id_build(&x.id)?,
                        name: x.name.clone(),
                        parameters: x
                            .parameters
                            .iter()
                            .map(|p| {
                                Ok(ResourceParameter {
                                    id: id_build(&p.id)?,
                                    name: p.name.clone(),
                                    type_ref: q_build(&p.type_ref)?,
                                    is_required: p.is_required,
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}
fn optional_text(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut present = value.is_some();
    let checkbox = ui.checkbox(&mut present, label);
    if present {
        ui.text_edit_singleline(value.get_or_insert_with(String::new))
            .labelled_by(checkbox.id);
    } else {
        *value = None;
    }
}
fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let l = ui.label(label);
    ui.text_edit_singleline(value).labelled_by(l.id);
}
fn qname(ui: &mut egui::Ui, label: &str, value: &mut QNameDraft) {
    ui.label(label);
    text(ui, "Namespace", &mut value.namespace);
    text(ui, "Local name", &mut value.local);
}
pub(super) fn optional_qname(ui: &mut egui::Ui, label: &str, value: &mut Option<QNameDraft>) {
    let mut present = value.is_some();
    ui.checkbox(&mut present, label);
    if present {
        qname(ui, label, value.get_or_insert_with(QNameDraft::default));
    } else {
        *value = None;
    }
}
fn optional_bool(ui: &mut egui::Ui, label: &str, value: &mut Option<bool>) {
    egui::ComboBox::from_label(label)
        .selected_text(match value {
            None => "Default",
            Some(true) => "Yes",
            Some(false) => "No",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Default");
            ui.selectable_value(value, Some(true), "Yes");
            ui.selectable_value(value, Some(false), "No");
        });
}
impl CatalogDraft {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Item definitions", |ui| {
            let mut remove = None;
            for (i, item) in self.items.iter_mut().enumerate() {
                ui.push_id(("item", i), |ui| {
                    ui.separator();
                    ui.label(format!("Item definition {}", i + 1));
                    optional_text(ui, "ID", &mut item.id);
                    optional_qname(ui, "Structure reference", &mut item.structure_ref);
                    optional_bool(ui, "Collection", &mut item.is_collection);
                    egui::ComboBox::from_label("Item kind")
                        .selected_text(match item.item_kind {
                            None => "Default",
                            Some(ItemKind::Information) => "Information",
                            Some(ItemKind::Physical) => "Physical",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut item.item_kind, None, "Default");
                            ui.selectable_value(
                                &mut item.item_kind,
                                Some(ItemKind::Information),
                                "Information",
                            );
                            ui.selectable_value(
                                &mut item.item_kind,
                                Some(ItemKind::Physical),
                                "Physical",
                            );
                        });
                    if ui.button("Remove item definition").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.items.remove(i);
            }
            if ui
                .add_enabled(
                    self.items.len() < 256,
                    egui::Button::new("Add item definition"),
                )
                .clicked()
            {
                self.items.push(ItemDraft::default());
            }
        });
        ui.collapsing("Interfaces", |ui| {
            let mut remove = None;
            for (i, interface) in self.interfaces.iter_mut().enumerate() {
                ui.push_id(("interface", i), |ui| {
                    ui.separator();
                    ui.label(format!("Interface {}", i + 1));
                    optional_text(ui, "ID", &mut interface.id);
                    text(ui, "Name", &mut interface.name);
                    optional_qname(
                        ui,
                        "Implementation reference",
                        &mut interface.implementation_ref,
                    );
                    ui.label("Operations");
                    let mut remove_operation = None;
                    for (j, operation) in interface.operations.iter_mut().enumerate() {
                        ui.push_id(("operation", j), |ui| {
                            ui.separator();
                            ui.label(format!("Operation {}", j + 1));
                            optional_text(ui, "ID", &mut operation.id);
                            text(ui, "Name", &mut operation.name);
                            optional_qname(
                                ui,
                                "Implementation reference",
                                &mut operation.implementation_ref,
                            );
                            qname(ui, "Input message reference", &mut operation.in_message_ref);
                            optional_qname(
                                ui,
                                "Output message reference",
                                &mut operation.out_message_ref,
                            );
                            ui.label("Error references");
                            let mut remove_error = None;
                            for (k, error) in operation.error_refs.iter_mut().enumerate() {
                                ui.push_id(("error", k), |ui| {
                                    qname(ui, "Error reference", error);
                                    if ui.button("Remove error reference").clicked() {
                                        remove_error = Some(k);
                                    }
                                });
                            }
                            if let Some(k) = remove_error {
                                operation.error_refs.remove(k);
                            }
                            if ui
                                .add_enabled(
                                    operation.error_refs.len() < 256,
                                    egui::Button::new("Add error reference"),
                                )
                                .clicked()
                            {
                                operation.error_refs.push(QNameDraft::default());
                            }
                            if ui.button("Remove operation").clicked() {
                                remove_operation = Some(j);
                            }
                        });
                    }
                    if let Some(j) = remove_operation {
                        interface.operations.remove(j);
                    }
                    if ui
                        .add_enabled(
                            interface.operations.len() < 256,
                            egui::Button::new("Add operation"),
                        )
                        .clicked()
                    {
                        interface.operations.push(OperationDraft::default());
                    }
                    if ui.button("Remove interface").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.interfaces.remove(i);
            }
            if ui
                .add_enabled(
                    self.interfaces.len() < 256,
                    egui::Button::new("Add interface"),
                )
                .clicked()
            {
                self.interfaces.push(InterfaceDraft {
                    operations: vec![OperationDraft::default()],
                    ..Default::default()
                });
            }
        });
        ui.collapsing("BPMN resources", |ui| {
            let mut remove = None;
            for (i, resource) in self.resources.iter_mut().enumerate() {
                ui.push_id(("resource", i), |ui| {
                    ui.separator();
                    ui.label(format!("Resource {}", i + 1));
                    optional_text(ui, "ID", &mut resource.id);
                    text(ui, "Name", &mut resource.name);
                    let mut remove_parameter = None;
                    for (j, parameter) in resource.parameters.iter_mut().enumerate() {
                        ui.push_id(("parameter", j), |ui| {
                            ui.separator();
                            ui.label(format!("Parameter {}", j + 1));
                            optional_text(ui, "ID", &mut parameter.id);
                            optional_text(ui, "Name", &mut parameter.name);
                            optional_qname(ui, "Type reference", &mut parameter.type_ref);
                            optional_bool(ui, "Required", &mut parameter.is_required);
                            if ui.button("Remove parameter").clicked() {
                                remove_parameter = Some(j);
                            }
                        });
                    }
                    if let Some(j) = remove_parameter {
                        resource.parameters.remove(j);
                    }
                    if ui
                        .add_enabled(
                            resource.parameters.len() < 256,
                            egui::Button::new("Add parameter"),
                        )
                        .clicked()
                    {
                        resource.parameters.push(ParameterDraft::default());
                    }
                    if ui.button("Remove resource").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.resources.remove(i);
            }
            if ui
                .add_enabled(
                    self.resources.len() < 256,
                    egui::Button::new("Add resource"),
                )
                .clicked()
            {
                self.resources.push(ResourceDraft::default());
            }
        });
    }
}

impl ProcessCraft {
    pub(super) fn catalog_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let Some(document) = self
            .ctl
            .session
            .snapshot()
            .project
            .documents
            .get(self.doc)
            .cloned()
        else {
            return;
        };
        ui.separator();
        if self.catalog_draft.is_none() {
            if ui
                .add_enabled(editable, egui::Button::new("Edit supporting catalog…"))
                .clicked()
                && self.commit_draft()
            {
                self.catalog_draft = Some(CatalogDraft::new(&document.catalog));
                self.draft_dirty = true;
            }
            return;
        }
        ui.strong("Supporting catalog");
        let draft = self.catalog_draft.as_mut().unwrap();
        draft.ui(ui);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply catalog").clicked() {
            match draft.build() {
                Ok(catalog) => {
                    if self.ctl.mutate(vec![Mutation::SetSupportingCatalog {
                        document_id: document.id,
                        catalog,
                    }]) {
                        self.catalog_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel catalog editing").clicked() {
            self.catalog_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_draft_round_trips_and_keeps_invalid_reference() {
        let catalog = SupportingCatalog {
            item_definitions: vec![ItemDefinition {
                id: Some("Item".parse().unwrap()),
                structure_ref: Some(QNameRef {
                    namespace: "urn:types".into(),
                    local: "Order".parse().unwrap(),
                }),
                is_collection: Some(false),
                item_kind: Some(ItemKind::Information),
            }],
            interfaces: vec![Interface {
                id: None,
                name: "Service".into(),
                implementation_ref: None,
                operations: vec![Operation {
                    id: None,
                    name: "Submit".into(),
                    implementation_ref: None,
                    in_message_ref: QNameRef {
                        namespace: "urn:messages".into(),
                        local: "Request".parse().unwrap(),
                    },
                    out_message_ref: None,
                    error_refs: vec![],
                }],
            }],
            resources: vec![BpmnResource {
                id: None,
                name: "Reviewer".into(),
                parameters: vec![ResourceParameter {
                    id: None,
                    name: Some(String::new()),
                    type_ref: None,
                    is_required: Some(false),
                }],
            }],
        };
        let mut draft = CatalogDraft::new(&catalog);
        assert_eq!(draft.build().unwrap(), catalog);
        draft.interfaces[0].operations[0].in_message_ref.local = "invalid name".into();
        assert!(draft.build().is_err());
        assert_eq!(
            draft.interfaces[0].operations[0].in_message_ref.local,
            "invalid name"
        );
    }
    #[test]
    fn catalog_draft_blocks_navigation_and_uses_history() {
        let mut app = ProcessCraft::default();
        app.select(None);
        let mut draft = CatalogDraft::new(&SupportingCatalog::default());
        draft.items.push(ItemDraft {
            id: Some("Item".into()),
            ..Default::default()
        });
        app.catalog_draft = Some(draft);
        app.draft_dirty = true;
        assert!(!app.commit_draft());
        app.select(Some("not_selected".parse().unwrap()));
        assert!(app.selected.is_none());
        assert!(app.catalog_draft.is_some());
        let catalog = app.catalog_draft.as_ref().unwrap().build().unwrap();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        assert!(app.ctl.mutate(vec![Mutation::SetSupportingCatalog {
            document_id,
            catalog
        }]));
        app.catalog_draft = None;
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0]
                .catalog
                .is_empty()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0]
                .catalog
                .item_definitions
                .len(),
            1
        );
    }
}
