use super::catalog::{
    IdentityDraft, QNameDraft, id_build, id_selector, id_string, optional_bool, optional_qname,
    optional_text, q_build, text,
};
use super::inspector::{ExpressionDefault, expression_ui, optional_expression_ui_with_default};
use super::*;
use processcraft_core::bpmn_data::{
    DataAssignment, DataAssociation, DataObject, DataState, DataStore, ItemAwareMetadata,
};
type DataValues = (Vec<DataStore>, Vec<DataObject>, Vec<DataAssociation>);

#[derive(Clone, Default)]
pub(super) struct ItemDraft {
    subject: Option<QNameDraft>,
    state: Option<StateDraft>,
}
#[derive(Clone, Default)]
struct StateDraft {
    base: Option<processcraft_core::bpmn_metadata::BaseMetadata>,
    id: IdentityDraft,
    name: Option<String>,
}
impl ItemDraft {
    pub(super) fn from_item(item: &ItemAwareMetadata) -> Self {
        Self {
            subject: item.item_subject_ref.as_ref().map(QNameDraft::from),
            state: item.data_state.as_ref().map(|state| StateDraft {
                base: state.base.clone(),
                id: id_string(&state.id),
                name: state.name.clone(),
            }),
        }
    }
    fn build(&self) -> Result<ItemAwareMetadata, String> {
        Ok(ItemAwareMetadata {
            item_subject_ref: q_build(&self.subject)?,
            data_state: self
                .state
                .as_ref()
                .map(|state| {
                    Ok::<DataState, String>(DataState {
                        base: state.base.clone(),
                        id: id_build(&state.id)?,
                        name: state.name.clone(),
                    })
                })
                .transpose()?,
        })
    }
}
pub(super) fn item_ui(ui: &mut egui::Ui, item: &mut Option<ItemDraft>) {
    let mut present = item.is_some();
    ui.checkbox(&mut present, "Item-aware metadata");
    if present {
        let item = item.get_or_insert_with(ItemDraft::default);
        optional_qname(ui, "Item subject reference", &mut item.subject);
        let mut state_present = item.state.is_some();
        ui.checkbox(&mut state_present, "Data state");
        if state_present {
            let state = item.state.get_or_insert_with(StateDraft::default);
            optional_text(ui, "State ID", &mut state.id.value);
            optional_text(ui, "State name", &mut state.name);
        } else {
            item.state = None;
        }
    } else {
        *item = None;
    }
}
pub(super) fn build_item(item: &Option<ItemDraft>) -> Result<Option<ItemAwareMetadata>, String> {
    item.as_ref().map(ItemDraft::build).transpose()
}
#[derive(Clone)]
struct StoreDraft {
    value: DataStore,
    id: IdentityDraft,
    item: Option<ItemDraft>,
}
#[derive(Clone)]
struct ObjectDraft {
    value: DataObject,
    id: IdentityDraft,
    item: Option<ItemDraft>,
}
#[derive(Clone)]
struct AssignmentDraft {
    base: Option<processcraft_core::bpmn_metadata::BaseMetadata>,
    id: IdentityDraft,
    from: processcraft_core::bpmn_model::Expression,
    to: processcraft_core::bpmn_model::Expression,
}
#[derive(Clone)]
struct AssociationDraft {
    value: DataAssociation,
    id: IdentityDraft,
    assignments: Vec<AssignmentDraft>,
}
#[derive(Clone)]
struct PortDraft {
    value: processcraft_core::bpmn_data::DataPort,
    id: IdentityDraft,
    item: Option<ItemDraft>,
}
#[derive(Clone)]
struct SetDraft {
    value: processcraft_core::bpmn_data::IoSet,
    id: IdentityDraft,
}
fn set_drafts(sets: &[processcraft_core::bpmn_data::IoSet]) -> Vec<SetDraft> {
    sets.iter()
        .map(|value| SetDraft {
            value: value.clone(),
            id: id_string(&value.id),
        })
        .collect()
}
fn build_sets(sets: &[SetDraft]) -> Result<Vec<processcraft_core::bpmn_data::IoSet>, String> {
    sets.iter()
        .map(|draft| {
            let mut value = draft.value.clone();
            value.id = id_build(&draft.id)?;
            Ok(value)
        })
        .collect()
}
fn reference_checkboxes(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut Vec<ElementSelector>,
    candidates: &[ElementSelector],
) {
    ui.label(label);
    let mut all = candidates.to_vec();
    for value in values.iter() {
        if !all.contains(value) {
            all.push(value.clone());
        }
    }
    for id in all {
        let mut selected = values.contains(&id);
        if ui.checkbox(&mut selected, id.to_string()).changed() {
            if selected {
                values.push(id);
            } else {
                values.retain(|value| value != &id);
            }
        }
    }
}
fn sets_ui(
    ui: &mut egui::Ui,
    title: &str,
    sets: &mut Vec<SetDraft>,
    ports: &[ElementSelector],
    counterparts: &[ElementSelector],
) {
    ui.push_id(title, |ui| {
        ui.label(title);
        let mut remove = None;
        for (i, set) in sets.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.label(format!("Set {}", i + 1));
                optional_text(ui, "ID", &mut set.id.value);
                optional_text(ui, "Name", &mut set.value.name);
                reference_checkboxes(ui, "Ports", &mut set.value.data_refs, ports);
                reference_checkboxes(
                    ui,
                    "Optional ports",
                    &mut set.value.optional_refs,
                    &set.value.data_refs,
                );
                reference_checkboxes(
                    ui,
                    "While executing",
                    &mut set.value.while_executing_refs,
                    &set.value.data_refs,
                );
                reference_checkboxes(
                    ui,
                    "Counterpart sets",
                    &mut set.value.counterpart_refs,
                    counterparts,
                );
                if ui.button("Remove set").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            sets.remove(i);
        }
        if ui
            .add_enabled(sets.len() < 256, egui::Button::new("Add set"))
            .clicked()
        {
            sets.push(SetDraft {
                value: Default::default(),
                id: IdentityDraft::default(),
            });
        }
    });
}
#[derive(Clone)]
struct IoDraft {
    owner: ElementSelector,
    name: String,
    process: bool,
    original: Option<processcraft_core::bpmn_data::IoSpecification>,
    inputs: Vec<PortDraft>,
    outputs: Vec<PortDraft>,
    id: IdentityDraft,
    input_sets: Vec<SetDraft>,
    output_sets: Vec<SetDraft>,
    initialized: bool,
}
impl IoDraft {
    fn new(
        owner: ElementSelector,
        name: String,
        process: bool,
        io: Option<processcraft_core::bpmn_data::IoSpecification>,
    ) -> Self {
        let convert = |ports: &[processcraft_core::bpmn_data::DataPort]| {
            ports
                .iter()
                .map(|v| PortDraft {
                    value: v.clone(),
                    id: id_string(&v.id),
                    item: v.item_aware.as_ref().map(ItemDraft::from_item),
                })
                .collect()
        };
        let inputs = io.as_ref().map_or_else(Vec::new, |io| convert(&io.inputs));
        let outputs = io.as_ref().map_or_else(Vec::new, |io| convert(&io.outputs));
        let id = io
            .as_ref()
            .map_or_else(IdentityDraft::default, |io| id_string(&io.id));
        let input_sets = io
            .as_ref()
            .map_or_else(Vec::new, |io| set_drafts(&io.input_sets));
        let output_sets = io
            .as_ref()
            .map_or_else(Vec::new, |io| set_drafts(&io.output_sets));
        let initialized = io.is_some();
        Self {
            owner,
            name,
            process,
            original: io,
            inputs,
            outputs,
            id,
            input_sets,
            output_sets,
            initialized,
        }
    }
    fn build(&self) -> Result<Option<processcraft_core::bpmn_data::IoSpecification>, String> {
        if self.original.is_none()
            && self.inputs.is_empty()
            && self.outputs.is_empty()
            && self.input_sets.is_empty()
            && self.output_sets.is_empty()
            && self.id.value.is_none()
        {
            return Ok(None);
        }
        let mut io = self.original.clone().unwrap_or_default();
        let convert = |ports: &[PortDraft]| {
            ports
                .iter()
                .map(|draft| {
                    let mut value = draft.value.clone();
                    value.id = id_build(&draft.id)?;
                    value.item_aware = build_item(&draft.item)?;
                    Ok(value)
                })
                .collect::<Result<Vec<_>, String>>()
        };
        io.id = id_build(&self.id)?;
        io.inputs = convert(&self.inputs)?;
        io.outputs = convert(&self.outputs)?;
        io.input_sets = build_sets(&self.input_sets)?;
        io.output_sets = build_sets(&self.output_sets)?;
        Ok(Some(io))
    }
}
#[derive(Clone)]
enum StoreReferenceDraft {
    Legacy(Option<ElementSelector>),
    Qualified(QNameDraft),
}
#[derive(Clone)]
enum ReferenceTargetDraft {
    Object(Option<ElementSelector>),
    Store(Option<StoreReferenceDraft>),
}
#[derive(Clone)]
struct ReferenceDraft {
    id: ElementSelector,
    name: String,
    item: Option<ItemDraft>,
    original: Option<processcraft_core::bpmn_model::NodeDetails>,
    target: ReferenceTargetDraft,
}
pub(super) fn ancestor_data_candidates(
    document: &BpmnDocument,
    scope_id: &ElementSelector,
) -> (
    Vec<ElementSelector>,
    Vec<ElementSelector>,
    Vec<ElementSelector>,
) {
    let scopes = processcraft_core::queries::scopes(document);
    let mut current = scope_id.clone();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut objects = Vec::new();
    while let Some(parent) = scopes.iter().find(|scope| {
        scope.nodes.iter().any(|node| {
            node.id == current
                && matches!(
                    node.details,
                    Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { .. })
                )
        })
    }) {
        objects.extend(
            parent
                .data_objects
                .iter()
                .filter_map(|object| object.id.as_ref().map(|id| id.selector())),
        );
        let shared = parent
            .data_objects
            .iter()
            .filter_map(|object| object.id.as_ref().map(|id| id.selector()))
            .chain(
                parent
                    .nodes
                    .iter()
                    .filter(|node| {
                        matches!(
                            node.kind,
                            NodeKind::DataObjectReference | NodeKind::DataStoreReference
                        )
                    })
                    .map(|node| node.id.selector()),
            )
            .collect::<Vec<_>>();
        inputs.extend(shared.iter().cloned());
        outputs.extend(shared);
        if let Some(io) = document
            .processes
            .iter()
            .find(|process| &process.id == parent.id)
            .and_then(|process| process.io_specification.as_ref())
        {
            inputs.extend(
                io.inputs
                    .iter()
                    .filter_map(|port| port.id.as_ref().map(|id| id.selector())),
            );
            outputs.extend(
                io.outputs
                    .iter()
                    .filter_map(|port| port.id.as_ref().map(|id| id.selector())),
            );
        }
        current = parent.id.selector();
    }
    (inputs, outputs, objects)
}
type EventOwner = (
    ElementSelector,
    String,
    Vec<ElementSelector>,
    Vec<ElementSelector>,
    Vec<ElementSelector>,
);
pub(super) struct DataDraft {
    scope: ElementSelector,
    stores: Vec<StoreDraft>,
    objects: Vec<ObjectDraft>,
    associations: Vec<AssociationDraft>,
    ancestor_inputs: Vec<ElementSelector>,
    ancestor_outputs: Vec<ElementSelector>,
    ancestor_objects: Vec<ElementSelector>,
    new_owner: Option<ElementSelector>,
    new_input: bool,
    new_sources: Vec<ElementSelector>,
    new_target: Option<ElementSelector>,
    ios: Vec<IoDraft>,
    event_owners: Vec<EventOwner>,
    references: Vec<ReferenceDraft>,
    error: String,
}
impl DataDraft {
    fn new(document: &BpmnDocument, scope: processcraft_core::queries::ScopeRef<'_>) -> Self {
        let (ancestor_inputs, ancestor_outputs, ancestor_objects) =
            ancestor_data_candidates(document, &scope.id.selector());
        Self {
            scope: scope.id.selector(),
            stores: document
                .data_stores
                .iter()
                .map(|v| StoreDraft {
                    value: v.clone(),
                    id: id_string(&v.id),
                    item: v.item_aware.as_ref().map(ItemDraft::from_item),
                })
                .collect(),
            objects: scope
                .data_objects
                .iter()
                .map(|v| ObjectDraft {
                    value: v.clone(),
                    id: id_string(&v.id),
                    item: v.item_aware.as_ref().map(ItemDraft::from_item),
                })
                .collect(),
            associations: scope
                .data_associations
                .iter()
                .map(|v| AssociationDraft {
                    value: v.clone(),
                    id: id_string(&v.id),
                    assignments: v
                        .assignments
                        .iter()
                        .map(|a| AssignmentDraft {
                            base: a.base.clone(),
                            id: id_string(&a.id),
                            from: a.from.clone(),
                            to: a.to.clone(),
                        })
                        .collect(),
                })
                .collect(),
            ancestor_inputs,
            ancestor_outputs,
            ancestor_objects,
            new_owner: None,
            new_input: true,
            new_sources: Vec::new(),
            new_target: None,
            ios: document
                .processes
                .iter()
                .filter(|process| &process.id == scope.id)
                .map(|process| {
                    IoDraft::new(
                        process.id.selector(),
                        process.name.clone().unwrap_or_default(),
                        true,
                        process.io_specification.clone(),
                    )
                })
                .chain(
                    scope
                        .nodes
                        .iter()
                        .filter(|node| node.kind.is_activity())
                        .map(|node| {
                            IoDraft::new(
                                node.id.selector(),
                                node.name.clone().unwrap_or_default(),
                                false,
                                node.io_specification.clone(),
                            )
                        }),
                )
                .collect(),
            event_owners: scope
                .nodes
                .iter()
                .filter_map(|node| {
                    use processcraft_core::bpmn_model::{EventIo, NodeDetails};
                    let Some(NodeDetails::Event {
                        io: Some(io),
                        properties,
                        ..
                    }) = &node.details
                    else {
                        return None;
                    };
                    let (inputs, outputs) = match io {
                        EventIo::Catch { outputs, .. } => (
                            Vec::new(),
                            outputs
                                .iter()
                                .filter_map(|port| port.id.as_ref().map(|id| id.selector()))
                                .collect(),
                        ),
                        EventIo::Throw { inputs, .. } => (
                            inputs
                                .iter()
                                .filter_map(|port| port.id.as_ref().map(|id| id.selector()))
                                .collect(),
                            Vec::new(),
                        ),
                    };
                    Some((
                        node.id.selector(),
                        node.name.clone().unwrap_or_default(),
                        inputs,
                        outputs,
                        properties
                            .iter()
                            .filter_map(|property| property.id.as_ref().map(|id| id.selector()))
                            .collect(),
                    ))
                })
                .collect(),
            references: scope
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        NodeKind::DataObjectReference | NodeKind::DataStoreReference
                    )
                })
                .map(|node| ReferenceDraft {
                    id: node.id.selector(),
                    name: node.name.clone().unwrap_or_default(),
                    item: node.item_aware.as_ref().map(ItemDraft::from_item),
                    original: node.details.clone(),
                    target: match &node.details {
                        Some(processcraft_core::bpmn_model::NodeDetails::DataObjectReference {
                            data_object_ref,
                        }) => ReferenceTargetDraft::Object(data_object_ref.clone()),
                        Some(processcraft_core::bpmn_model::NodeDetails::DataStoreReference {
                            data_store_ref,
                        }) => {
                            ReferenceTargetDraft::Store(data_store_ref.as_ref().map(|reference| {
                                match reference {
                                    processcraft_core::bpmn_data::DataStoreRef::Legacy(id) => {
                                        StoreReferenceDraft::Legacy(Some(id.clone()))
                                    }
                                    processcraft_core::bpmn_data::DataStoreRef::Qualified(
                                        qname,
                                    ) => StoreReferenceDraft::Qualified(QNameDraft::from(qname)),
                                }
                            }))
                        }
                        _ => {
                            if node.kind == NodeKind::DataObjectReference {
                                ReferenceTargetDraft::Object(None)
                            } else {
                                ReferenceTargetDraft::Store(None)
                            }
                        }
                    },
                })
                .collect(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<DataValues, String> {
        let stores = self
            .stores
            .iter()
            .map(|d| {
                let mut v = d.value.clone();
                v.id = id_build(&d.id)?;
                v.item_aware = build_item(&d.item)?;
                Ok(v)
            })
            .collect::<Result<_, String>>()?;
        let objects = self
            .objects
            .iter()
            .map(|d| {
                let mut v = d.value.clone();
                v.id = id_build(&d.id)?;
                v.item_aware = build_item(&d.item)?;
                Ok(v)
            })
            .collect::<Result<_, String>>()?;
        let associations = self
            .associations
            .iter()
            .map(|d| {
                let mut v = d.value.clone();
                v.id = id_build(&d.id)?;
                v.assignments = d
                    .assignments
                    .iter()
                    .map(|a| {
                        Ok(DataAssignment {
                            base: a.base.clone(),
                            id: id_build(&a.id)?,
                            from: a.from.clone(),
                            to: a.to.clone(),
                        })
                    })
                    .collect::<Result<_, String>>()?;
                Ok(v)
            })
            .collect::<Result<_, String>>()?;
        Ok((stores, objects, associations))
    }
}
impl DataDraft {
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
    ) {
        ui.collapsing("Data stores", |ui| {
            let mut remove = None;
            for (i, draft) in self.stores.iter_mut().enumerate() {
                ui.push_id(("store", i), |ui| {
                    ui.separator();
                    ui.label(format!("Store {}", i + 1));
                    optional_text(ui, "ID", &mut draft.id.value);
                    optional_text(ui, "Name", &mut draft.value.name);
                    optional_text(ui, "Capacity", &mut draft.value.capacity);
                    optional_bool(ui, "Unlimited", &mut draft.value.is_unlimited);
                    item_ui(ui, &mut draft.item);
                    if ui.button("Remove store").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.stores.remove(i);
            }
            if ui
                .add_enabled(self.stores.len() < 256, egui::Button::new("Add store"))
                .clicked()
            {
                self.stores.push(StoreDraft {
                    value: DataStore {
                        base: None,
                        id: None,
                        name: None,
                        capacity: None,
                        is_unlimited: None,
                        item_aware: None,
                    },
                    id: IdentityDraft::default(),
                    item: None,
                });
            }
        });
        ui.collapsing("Data objects in scope", |ui| {
            let mut remove = None;
            for (i, draft) in self.objects.iter_mut().enumerate() {
                ui.push_id(("object", i), |ui| {
                    ui.separator();
                    ui.label(format!("Object {}", i + 1));
                    optional_text(ui, "ID", &mut draft.id.value);
                    optional_text(ui, "Name", &mut draft.value.name);
                    optional_bool(ui, "Collection", &mut draft.value.is_collection);
                    item_ui(ui, &mut draft.item);
                    if ui.button("Remove object").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.objects.remove(i);
            }
            if ui
                .add_enabled(self.objects.len() < 256, egui::Button::new("Add object"))
                .clicked()
            {
                self.objects.push(ObjectDraft {
                    value: DataObject {
                        base: None,
                        id: None,
                        name: None,
                        is_collection: None,
                        item_aware: None,
                    },
                    id: IdentityDraft::default(),
                    item: None,
                });
            }
        });
        let store_targets: Vec<_> = self
            .stores
            .iter()
            .filter_map(|store| id_selector(&store.id))
            .collect();
        let object_targets: Vec<_> = self
            .objects
            .iter()
            .filter_map(|object| id_selector(&object.id))
            .chain(self.ancestor_objects.iter().cloned())
            .collect();
        ui.collapsing("Data reference metadata", |ui| {
            for (i, reference) in self.references.iter_mut().enumerate() {
                ui.push_id(("reference", i), |ui| {
                    ui.label(format!("{} · {}", reference.name, reference.id));
                    item_ui(ui, &mut reference.item);
                    match &mut reference.target {
                        ReferenceTargetDraft::Object(target) => {
                            egui::ComboBox::from_label("Object definition")
                                .selected_text(
                                    target.as_ref().map_or("None".into(), ToString::to_string),
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(target, None, "None");
                                    for id in &object_targets {
                                        ui.selectable_value(
                                            target,
                                            Some(id.clone()),
                                            id.to_string(),
                                        );
                                    }
                                });
                        }
                        ReferenceTargetDraft::Store(target) => {
                            let mut present = target.is_some();
                            ui.checkbox(&mut present, "Store reference");
                            if present {
                                let value =
                                    target.get_or_insert_with(|| StoreReferenceDraft::Legacy(None));
                                let mut qualified =
                                    matches!(value, StoreReferenceDraft::Qualified(_));
                                if ui
                                    .checkbox(&mut qualified, "Qualified store reference")
                                    .changed()
                                {
                                    *value = if qualified {
                                        StoreReferenceDraft::Qualified(QNameDraft::default())
                                    } else {
                                        StoreReferenceDraft::Legacy(None)
                                    };
                                }
                                match value {
                                    StoreReferenceDraft::Legacy(local) => {
                                        egui::ComboBox::from_label("Choose local store")
                                            .selected_text(
                                                local.as_ref().map_or(
                                                    "Select store".into(),
                                                    ToString::to_string,
                                                ),
                                            )
                                            .show_ui(ui, |ui| {
                                                for id in &store_targets {
                                                    ui.selectable_value(
                                                        local,
                                                        Some(id.clone()),
                                                        id.to_string(),
                                                    );
                                                }
                                            });
                                    }
                                    StoreReferenceDraft::Qualified(qname) => {
                                        text(ui, "Store namespace", &mut qname.namespace);
                                        text(ui, "Store local name", &mut qname.local);
                                    }
                                }
                            } else {
                                *target = None;
                            }
                        }
                    }
                });
            }
        });
        ui.collapsing("Input and output ports", |ui| {
            ui.label("Removing a port requires removing its set and association references first.");
            for (i, io) in self.ios.iter_mut().enumerate() {
                ui.push_id(("io", i), |ui| {
                    ui.separator();
                    ui.label(format!("{} · {}", io.name, io.owner));
                    optional_text(ui, "I/O specification ID", &mut io.id.value);
                    for (title, ports) in [("Inputs", &mut io.inputs), ("Outputs", &mut io.outputs)]
                    {
                        ui.push_id(title, |ui| {
                            ui.label(title);
                            let mut remove = None;
                            for (j, port) in ports.iter_mut().enumerate() {
                                ui.push_id(j, |ui| {
                                    ui.label(format!("Port {}", j + 1));
                                    optional_text(ui, "ID", &mut port.id.value);
                                    optional_text(ui, "Name", &mut port.value.name);
                                    optional_bool(ui, "Collection", &mut port.value.is_collection);
                                    item_ui(ui, &mut port.item);
                                    if ui.button("Remove port").clicked() {
                                        remove = Some(j);
                                    }
                                });
                            }
                            if let Some(j) = remove {
                                ports.remove(j);
                            }
                            if ui
                                .add_enabled(ports.len() < 256, egui::Button::new("Add port"))
                                .clicked()
                            {
                                let id = crate::controller::id("DataPort");
                                ports.push(PortDraft {
                                    value: processcraft_core::bpmn_data::DataPort {
                                        base: None,
                                        id: Some(id.clone().into()),
                                        name: None,
                                        item_aware: None,
                                        is_collection: None,
                                    },
                                    id: IdentityDraft {
                                        value: Some(id.to_string()),
                                        ..Default::default()
                                    },
                                    item: None,
                                });
                            }
                        });
                    }
                    if !io.initialized && (!io.inputs.is_empty() || !io.outputs.is_empty()) {
                        if io.input_sets.is_empty() {
                            io.input_sets.push(SetDraft {
                                value: Default::default(),
                                id: IdentityDraft::default(),
                            });
                        }
                        if io.output_sets.is_empty() {
                            io.output_sets.push(SetDraft {
                                value: Default::default(),
                                id: IdentityDraft::default(),
                            });
                        }
                        io.initialized = true;
                    }
                    let input_ports = io
                        .inputs
                        .iter()
                        .filter_map(|port| id_selector(&port.id))
                        .collect::<Vec<_>>();
                    let output_ports = io
                        .outputs
                        .iter()
                        .filter_map(|port| id_selector(&port.id))
                        .collect::<Vec<_>>();
                    let input_ids = io
                        .input_sets
                        .iter()
                        .filter_map(|set| id_selector(&set.id))
                        .collect::<Vec<_>>();
                    let output_ids = io
                        .output_sets
                        .iter()
                        .filter_map(|set| id_selector(&set.id))
                        .collect::<Vec<_>>();
                    sets_ui(
                        ui,
                        "Input sets",
                        &mut io.input_sets,
                        &input_ports,
                        &output_ids,
                    );
                    sets_ui(
                        ui,
                        "Output sets",
                        &mut io.output_sets,
                        &output_ports,
                        &input_ids,
                    );
                });
            }
        });
        let base_candidates: Vec<ElementSelector> = self
            .objects
            .iter()
            .filter_map(|object| id_selector(&object.id))
            .chain(self.references.iter().map(|reference| reference.id.clone()))
            .chain(
                self.stores
                    .iter()
                    .filter_map(|store| id_selector(&store.id)),
            )
            .collect();
        let mut input_sources = base_candidates.clone();
        input_sources.extend(self.ancestor_inputs.iter().cloned());
        let mut output_targets = base_candidates;
        output_targets.extend(self.ancestor_outputs.iter().cloned());
        for io in self.ios.iter().filter(|io| io.process) {
            input_sources.extend(io.inputs.iter().filter_map(|port| id_selector(&port.id)));
            output_targets.extend(io.outputs.iter().filter_map(|port| id_selector(&port.id)));
        }
        let no_ports = Vec::new();
        let mut owner_choices: Vec<_> = self
            .ios
            .iter()
            .filter(|io| !io.process)
            .map(|io| {
                (
                    io.owner.clone(),
                    io.name.clone(),
                    io.inputs
                        .iter()
                        .filter_map(|port| id_selector(&port.id))
                        .collect::<Vec<_>>(),
                    io.outputs
                        .iter()
                        .filter_map(|port| id_selector(&port.id))
                        .collect::<Vec<_>>(),
                    Vec::<ElementSelector>::new(),
                )
            })
            .collect();
        owner_choices.extend(self.event_owners.iter().cloned());
        ui.collapsing("Data associations in scope", |ui| {
            let mut remove = None;
            for (i, draft) in self.associations.iter_mut().enumerate() {
                ui.push_id(("association", i), |ui| {
                    ui.separator();
                    ui.label(format!("Association {}", i + 1));
                    optional_text(ui, "ID", &mut draft.id.value);
                    use processcraft_core::bpmn_data::DataAssociationKind;
                    egui::ComboBox::from_label("Activity owner")
                        .selected_text(draft.value.owner.to_string())
                        .show_ui(ui, |ui| {
                            for (id, name, _, _, _) in &owner_choices {
                                ui.selectable_value(
                                    &mut draft.value.owner,
                                    id.clone(),
                                    format!("{name} · {id}"),
                                );
                            }
                        });
                    egui::ComboBox::from_label("Direction")
                        .selected_text(format!("{:?}", draft.value.kind))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut draft.value.kind,
                                DataAssociationKind::Input,
                                "Input",
                            );
                            ui.selectable_value(
                                &mut draft.value.kind,
                                DataAssociationKind::Output,
                                "Output",
                            );
                        });
                    let owner = owner_choices
                        .iter()
                        .find(|(id, _, _, _, _)| id == &draft.value.owner);
                    let ports = owner.map(|(_, _, inputs, outputs, _)| {
                        if draft.value.kind == DataAssociationKind::Input {
                            inputs
                        } else {
                            outputs
                        }
                    });
                    let mut event_local = input_sources.clone();
                    let mut event_target = output_targets.clone();
                    if let Some((_, _, _, _, properties)) = owner {
                        event_local.extend(properties.iter().cloned());
                        event_target.extend(properties.iter().cloned());
                    }
                    let source_candidates = if draft.value.kind == DataAssociationKind::Input {
                        &event_local
                    } else {
                        ports.unwrap_or(&no_ports)
                    };
                    let target_candidates = if draft.value.kind == DataAssociationKind::Output {
                        &event_target
                    } else {
                        ports.unwrap_or(&no_ports)
                    };
                    ui.label("Sources");
                    let mut candidates = source_candidates.clone();
                    for existing in &draft.value.sources {
                        if !candidates.contains(existing) {
                            candidates.push(existing.clone());
                        }
                    }
                    for candidate in candidates {
                        let mut included = draft.value.sources.contains(&candidate);
                        if ui.checkbox(&mut included, candidate.to_string()).changed() {
                            if included {
                                draft.value.sources.push(candidate);
                            } else {
                                draft.value.sources.retain(|id| id != &candidate);
                            }
                        }
                    }
                    egui::ComboBox::from_label("Target")
                        .selected_text(draft.value.target.to_string())
                        .show_ui(ui, |ui| {
                            for candidate in target_candidates {
                                ui.selectable_value(
                                    &mut draft.value.target,
                                    candidate.clone(),
                                    candidate.to_string(),
                                );
                            }
                        });
                    optional_expression_ui_with_default(
                        ui,
                        &format!("Transformation {i}"),
                        &mut draft.value.transformation,
                        true,
                        expression_draft,
                        ExpressionDefault::ImplicitFormal,
                    );
                    let mut remove_assignment = None;
                    for (j, assignment) in draft.assignments.iter_mut().enumerate() {
                        ui.push_id(("assignment", j), |ui| {
                            ui.separator();
                            ui.label(format!("Assignment {}", j + 1));
                            optional_text(ui, "Assignment ID", &mut assignment.id.value);
                            expression_ui(
                                ui,
                                &format!("From {i} {j}"),
                                &mut assignment.from,
                                true,
                                expression_draft,
                                ExpressionDefault::Base,
                            );
                            expression_ui(
                                ui,
                                &format!("To {i} {j}"),
                                &mut assignment.to,
                                true,
                                expression_draft,
                                ExpressionDefault::Base,
                            );
                            if ui
                                .add_enabled(
                                    expression_draft.is_none(),
                                    egui::Button::new("Remove assignment"),
                                )
                                .clicked()
                            {
                                remove_assignment = Some(j);
                            }
                        });
                    }
                    if let Some(j) = remove_assignment {
                        draft.assignments.remove(j);
                    }
                    if ui
                        .add_enabled(
                            expression_draft.is_none() && draft.assignments.len() < 256,
                            egui::Button::new("Add assignment"),
                        )
                        .clicked()
                    {
                        draft.assignments.push(AssignmentDraft {
                            base: None,
                            id: IdentityDraft::default(),
                            from: String::new().into(),
                            to: String::new().into(),
                        });
                    }
                    if ui
                        .add_enabled(
                            expression_draft.is_none(),
                            egui::Button::new("Remove association"),
                        )
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.associations.remove(i);
            }
            ui.separator();
            ui.label("New data association");
            egui::ComboBox::from_label("New activity owner")
                .selected_text(
                    self.new_owner
                        .as_ref()
                        .map_or("Select activity".into(), ToString::to_string),
                )
                .show_ui(ui, |ui| {
                    for (id, name, _, _, _) in &owner_choices {
                        ui.selectable_value(
                            &mut self.new_owner,
                            Some(id.clone()),
                            format!("{name} · {id}"),
                        );
                    }
                });
            ui.checkbox(&mut self.new_input, "Input association");
            let owner = self.new_owner.as_ref().and_then(|id| {
                owner_choices
                    .iter()
                    .find(|(candidate, _, _, _, _)| candidate == id)
            });
            let ports = owner
                .map(|(_, _, inputs, outputs, _)| if self.new_input { inputs } else { outputs });
            let mut event_local = input_sources.clone();
            let mut event_target = output_targets.clone();
            if let Some((_, _, _, _, properties)) = owner {
                event_local.extend(properties.iter().cloned());
                event_target.extend(properties.iter().cloned());
            }
            let sources = if self.new_input {
                &event_local
            } else {
                ports.unwrap_or(&no_ports)
            };
            let targets = if self.new_input {
                ports.unwrap_or(&no_ports)
            } else {
                &event_target
            };
            reference_checkboxes(
                ui,
                "New association sources",
                &mut self.new_sources,
                sources,
            );
            egui::ComboBox::from_label("New association target")
                .selected_text(
                    self.new_target
                        .as_ref()
                        .map_or("Select target".into(), ToString::to_string),
                )
                .show_ui(ui, |ui| {
                    for target in targets {
                        ui.selectable_value(
                            &mut self.new_target,
                            Some(target.clone()),
                            target.to_string(),
                        );
                    }
                });
            let ready = self.new_owner.is_some()
                && self
                    .new_target
                    .as_ref()
                    .is_some_and(|target| targets.contains(target))
                && !self.new_sources.is_empty()
                && self
                    .new_sources
                    .iter()
                    .all(|source| sources.contains(source));
            if ui
                .add_enabled(
                    ready && self.associations.len() < 256 && expression_draft.is_none(),
                    egui::Button::new("Add association"),
                )
                .clicked()
            {
                let value = DataAssociation {
                    base: None,
                    id: None,
                    owner: self.new_owner.clone().expect("selected"),
                    kind: if self.new_input {
                        processcraft_core::bpmn_data::DataAssociationKind::Input
                    } else {
                        processcraft_core::bpmn_data::DataAssociationKind::Output
                    },
                    sources: self.new_sources.clone(),
                    target: self.new_target.clone().expect("selected"),
                    transformation: None,
                    assignments: Vec::new(),
                };
                self.associations.push(AssociationDraft {
                    value,
                    id: IdentityDraft::default(),
                    assignments: Vec::new(),
                });
                self.new_sources.clear();
                self.new_target = None;
            }
        });
    }
}
impl ProcessCraft {
    pub(super) fn data_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .find(|s| self.process_id.as_ref().is_none_or(|id| s.id == id));
        let Some(scope) = scope else {
            return;
        };
        ui.separator();
        if self.data_draft.is_none() {
            ui.collapsing("Show ports on diagram", |ui| {
                let process_ports = document
                    .processes
                    .iter()
                    .filter(|process| &process.id == scope.id)
                    .filter_map(|process| process.io_specification.as_ref());
                if scope
                    .nodes
                    .iter()
                    .filter_map(|node| node.io_specification.as_ref())
                    .any(|io| !io.inputs.is_empty() || !io.outputs.is_empty())
                {
                    ui.label("BPMN diagrams can show process ports; activity ports are not shown.");
                }
                for (index, port) in process_ports
                    .flat_map(|io| io.inputs.iter().chain(&io.outputs))
                    .enumerate()
                {
                    ui.push_id(index, |ui| {
                        if let Some(id) = &port.id {
                            let diagram = document.diagrams.iter().find(|diagram| {
                                &diagram.plane == scope.id
                                    && self
                                        .diagram_id
                                        .as_ref()
                                        .is_none_or(|selected| selected == &diagram.id)
                            });
                            let already_shown = diagram.is_some_and(|diagram| {
                                diagram.shapes.iter().any(|shape| &shape.element == id)
                            });
                            if ui
                                .add_enabled(
                                    editable && diagram.is_some() && !already_shown,
                                    egui::Button::new(format!(
                                        "Show {} · {}",
                                        port.name.as_deref().unwrap_or("Unnamed port"),
                                        id
                                    )),
                                )
                                .clicked()
                            {
                                let diagram = diagram.expect("enabled only with matching diagram");
                                let shape = Shape {
                                    label_visibility: None,
                                    is_horizontal: None,
                                    is_marker_visible: None,
                                    id: crate::controller::id("DataPortShape").into(),
                                    element: id.selector(),
                                    is_expanded: None,
                                    bounds: Bounds {
                                        x: self.camera.0 + 120.0,
                                        y: self.camera.1 + 80.0 + index as f64 * 45.0,
                                        width: 36.0,
                                        height: 48.0,
                                    },
                                    label: None,
                                    style: None,
                                };
                                if self.commit_draft() {
                                    self.ctl.mutate(vec![Mutation::CreateShape {
                                        document_id: document.id,
                                        diagram_id: diagram.id.selector(),
                                        shape,
                                    }]);
                                }
                            }
                            if already_shown {
                                ui.label("Already shown");
                            }
                        } else {
                            ui.label(format!(
                                "{}: assign an ID in data declarations before showing this port.",
                                port.name.as_deref().unwrap_or("Unnamed port")
                            ));
                        }
                    });
                }
            });
            ui.collapsing("Show stores on diagram", |ui| {
                let diagram = document.diagrams.iter().find(|diagram| {
                    &diagram.plane == scope.id
                        && self
                            .diagram_id
                            .as_ref()
                            .is_none_or(|selected| selected == &diagram.id)
                });
                for (index, store) in document.data_stores.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        if let Some(id) = &store.id {
                            let shown = diagram.is_some_and(|diagram| {
                                diagram.shapes.iter().any(|shape| &shape.element == id)
                            });
                            if ui
                                .add_enabled(
                                    editable && diagram.is_some() && !shown,
                                    egui::Button::new(format!(
                                        "Show {} · {}",
                                        store.name.as_deref().unwrap_or("Unnamed store"),
                                        id
                                    )),
                                )
                                .clicked()
                            {
                                let diagram = diagram.expect("enabled only with matching diagram");
                                let shape = Shape {
                                    label_visibility: None,
                                    is_horizontal: None,
                                    is_marker_visible: None,
                                    id: crate::controller::id("DataStoreShape").into(),
                                    element: id.selector(),
                                    is_expanded: None,
                                    bounds: Bounds {
                                        x: self.camera.0 + 180.0,
                                        y: self.camera.1 + 80.0 + index as f64 * 80.0,
                                        width: 50.0,
                                        height: 65.0,
                                    },
                                    label: None,
                                    style: None,
                                };
                                if self.commit_draft() {
                                    self.ctl.mutate(vec![Mutation::CreateShape {
                                        document_id: document.id,
                                        diagram_id: diagram.id.selector(),
                                        shape,
                                    }]);
                                }
                            }
                            if shown {
                                ui.label("Already shown");
                            }
                        } else {
                            ui.label(format!(
                                "{}: assign an ID in data declarations before showing this store.",
                                store.name.as_deref().unwrap_or("Unnamed store")
                            ));
                        }
                    });
                }
            });
            if ui
                .add_enabled(editable, egui::Button::new("Edit data declarations…"))
                .clicked()
                && self.commit_draft()
            {
                self.data_draft = Some(DataDraft::new(document, scope));
                self.draft_dirty = true;
            }
            return;
        }
        let draft = self.data_draft.as_mut().unwrap();
        ui.strong("Data declarations");
        draft.ui(ui, &mut self.expression_draft);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply data declarations"),
            )
            .clicked()
        {
            match draft.build() {
                Ok((data_stores, data_objects, data_associations)) => {
                    let mut mutations = vec![
                        Mutation::SetDataStores {
                            document_id: document.id,
                            data_stores,
                        },
                        Mutation::SetScopeDataObjects {
                            document_id: document.id,
                            scope_id: draft.scope.clone(),
                            data_objects,
                        },
                        Mutation::SetScopeDataAssociations {
                            document_id: document.id,
                            scope_id: draft.scope.clone(),
                            data_associations,
                        },
                    ];
                    let extras = (|| -> Result<(), String> {
                        for io in &draft.ios {
                            let io_specification = io.build()?;
                            if io_specification != io.original {
                                mutations.push(if io.process {
                                    Mutation::SetProcessIoSpecification {
                                        document_id: document.id,
                                        process_id: io.owner.clone(),
                                        io_specification,
                                    }
                                } else {
                                    Mutation::SetNodeIoSpecification {
                                        document_id: document.id,
                                        element_id: io.owner.clone(),
                                        io_specification,
                                    }
                                });
                            }
                        }
                        for reference in &draft.references {
                            mutations.push(Mutation::SetNodeItemAware {
                                document_id: document.id,
                                element_id: reference.id.clone(),
                                item_aware: build_item(&reference.item)?,
                            });
                            let details = match &reference.target {
                                ReferenceTargetDraft::Object(data_object_ref) => processcraft_core::bpmn_model::NodeDetails::DataObjectReference { data_object_ref: data_object_ref.clone() },
                                ReferenceTargetDraft::Store(target) => processcraft_core::bpmn_model::NodeDetails::DataStoreReference { data_store_ref: target.as_ref().map(|target| -> Result<_, String> { match target {
                                    StoreReferenceDraft::Legacy(local) => Ok(processcraft_core::bpmn_data::DataStoreRef::Legacy(local.clone().ok_or_else(|| "Select a local store.".to_owned())?)),
                                    StoreReferenceDraft::Qualified(qname) => Ok(processcraft_core::bpmn_data::DataStoreRef::Qualified(q_build(&Some(qname.clone()))?.expect("reference is present"))),
                                }}).transpose()? },
                            };
                            if reference.original.as_ref() != Some(&details) {
                                mutations.push(Mutation::SetNodeDetails {
                                    document_id: document.id,
                                    element_id: reference.id.clone(),
                                    details: Some(details),
                                });
                            }
                        }
                        Ok(())
                    })();
                    match extras {
                        Ok(()) => {
                            if self.ctl.mutate(mutations) {
                                self.data_draft = None;
                                if let Some(selected) = &self.selected {
                                    let current = self.ctl.session.snapshot();
                                    self.bpmn_details = current
                                        .project
                                        .documents
                                        .get(self.doc)
                                        .and_then(|document| {
                                            processcraft_core::queries::find_node(
                                                document, selected,
                                            )
                                        })
                                        .and_then(|node| node.details.clone());
                                }
                            }
                        }
                        Err(error) => draft.error = error,
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel data editing"),
            )
            .clicked()
        {
            self.data_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};

    #[test]
    fn ancestor_candidates_include_parent_but_exclude_siblings() {
        use processcraft_core::bpmn_model::NodeDetails;
        let mut document = crate::controller::blank_document();
        let root_object: XmlId = "RootObject".parse().unwrap();
        let sibling_object: XmlId = "SiblingObject".parse().unwrap();
        document.processes[0].data_objects.push(DataObject {
            base: None,
            id: Some(root_object.clone().into()),
            name: None,
            is_collection: None,
            item_aware: None,
        });
        let make_subprocess = |id: &str, objects: Vec<DataObject>| Node {
            base_extras: None,
            id: id.parse().unwrap(),
            name: None,
            documentation: String::new().into(),
            kind: NodeKind::SubProcess,
            item_aware: None,
            details: Some(NodeDetails::SubProcess {
                content: Box::new(FlowContainer {
                    data_objects: objects,
                    ..Default::default()
                }),
            }),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        };
        document.processes[0]
            .nodes
            .push(make_subprocess("Child", vec![]));
        document.processes[0].nodes.push(make_subprocess(
            "Sibling",
            vec![DataObject {
                base: None,
                id: Some(sibling_object.clone().into()),
                name: None,
                is_collection: None,
                item_aware: None,
            }],
        ));
        let (inputs, outputs, objects) =
            ancestor_data_candidates(&document, &"Child".parse().unwrap());
        assert!(
            inputs.contains(&root_object.clone().into())
                && outputs.contains(&root_object.clone().into())
                && objects.contains(&root_object.clone().into())
        );
        assert!(
            !inputs.contains(&sibling_object.clone().into())
                && !outputs.contains(&sibling_object.clone().into())
                && !objects.contains(&sibling_object.clone().into())
        );
    }

    #[test]
    fn event_owned_output_association_uses_event_port_and_store() {
        use processcraft_core::bpmn_data::{DataAssociationKind, DataPort};
        use processcraft_core::bpmn_model::{EventIo, NodeDetails};
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document_id = snapshot.project.documents[0].id;
        let scope_id = snapshot.project.documents[0].processes[0].id.selector();
        let port_id = processcraft_core::ids::ElementIdentity::anonymous();
        let store_id = processcraft_core::ids::ElementIdentity::anonymous();
        let mut details =
            processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
                .unwrap()
                .details
                .clone()
                .unwrap();
        if let NodeDetails::Event { io, .. } = &mut details {
            *io = Some(EventIo::Catch {
                outputs: vec![DataPort {
                    base: None,
                    id: Some(port_id.clone()),
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                output_set: None,
            });
        }
        assert!(
            app.ctl.mutate(vec![
                Mutation::SetNodeDetails {
                    document_id,
                    element_id: selected.clone(),
                    details: Some(details)
                },
                Mutation::SetDataStores {
                    document_id,
                    data_stores: vec![DataStore {
                        base: None,
                        id: Some(store_id.clone()),
                        name: None,
                        capacity: None,
                        is_unlimited: None,
                        item_aware: None
                    }]
                }
            ]),
            "{}",
            app.ctl.error
        );
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .find(|scope| scope.id == &scope_id)
            .unwrap();
        let mut draft = DataDraft::new(document, scope);
        assert!(
            draft
                .event_owners
                .iter()
                .any(|(owner, _, _, outputs, _)| owner == &selected
                    && outputs.contains(&port_id.selector()))
        );
        let value = DataAssociation {
            base: None,
            id: None,
            owner: selected,
            kind: DataAssociationKind::Output,
            sources: vec![port_id.selector()],
            target: store_id.selector(),
            transformation: None,
            assignments: Vec::new(),
        };
        draft.associations.push(AssociationDraft {
            value,
            id: IdentityDraft::default(),
            assignments: Vec::new(),
        });
        let (_, _, data_associations) = draft.build().unwrap();
        assert!(
            app.ctl.mutate(vec![Mutation::SetScopeDataAssociations {
                document_id,
                scope_id,
                data_associations
            }]),
            "{}",
            app.ctl.error
        );
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].processes[0]
                .data_associations
                .is_empty()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].processes[0]
                .data_associations
                .len(),
            1
        );
    }

    #[test]
    fn data_draft_preserves_anonymous_names_and_invalid_state_ids() {
        let mut project = crate::controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        document.data_stores.push(DataStore {
            base: None,
            id: None,
            name: None,
            capacity: None,
            is_unlimited: Some(false),
            item_aware: None,
        });
        document.processes[0].data_objects.push(DataObject {
            base: None,
            id: None,
            name: Some(String::new()),
            is_collection: Some(false),
            item_aware: None,
        });
        document.processes[0].io_specification = Some(IoSpecification {
            base: None,
            id: None,
            inputs: vec![DataPort {
                base: None,
                id: None,
                name: None,
                item_aware: None,
                is_collection: None,
            }],
            outputs: vec![],
            input_sets: vec![IoSet::default()],
            output_sets: vec![IoSet::default()],
        });
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .next()
            .unwrap();
        let mut draft = DataDraft::new(document, scope);
        let (stores, objects, _) = draft.build().unwrap();
        assert_eq!(stores, document.data_stores);
        assert_eq!(objects, document.processes[0].data_objects);
        assert_eq!(
            draft.ios[0].build().unwrap(),
            document.processes[0].io_specification
        );
        draft.objects[0].item = Some(ItemDraft {
            subject: None,
            state: Some(StateDraft {
                base: None,
                id: IdentityDraft {
                    value: Some("invalid id".into()),
                    ..Default::default()
                },
                name: Some(String::new()),
            }),
        });
        assert!(draft.build().is_err());
        assert_eq!(
            draft.objects[0]
                .item
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .id
                .value
                .as_deref(),
            Some("invalid id")
        );
    }

    #[test]
    fn clean_session_data_draft_blocks_transition_and_is_cleared_only_after_discard() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let snapshot = app.ctl.session.snapshot();
        app.ctl.session.mark_saved(snapshot.content_id);
        let document = &snapshot.project.documents[0];
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .next()
            .unwrap();
        app.data_draft = Some(DataDraft::new(document, scope));
        app.draft_dirty = false;
        let selected = app.selected.clone();
        let session = app.ctl.session.id();
        app.select(None);
        assert_eq!(app.selected, selected);
        app.new_or_open("new");
        assert_eq!(app.confirm, Some("new"));
        assert_eq!(app.ctl.session.id(), session);
        assert!(app.data_draft.is_some());
        app.open_selected_path(None);
        assert!(app.data_draft.is_some());
        let action = app.discard_transition().unwrap();
        app.execute(action);
        assert_ne!(app.ctl.session.id(), session);
        assert!(app.data_draft.is_none());
        assert!(app.selected.is_none());
        assert!(!app.has_drafts());
    }

    #[test]
    fn data_draft_guards_navigation_and_history() {
        let mut app = ProcessCraft::default();
        app.select(None);
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .next()
            .unwrap();
        let mut draft = DataDraft::new(document, scope);
        draft.objects.push(ObjectDraft {
            value: DataObject {
                base: None,
                id: None,
                name: None,
                is_collection: None,
                item_aware: None,
            },
            id: IdentityDraft::default(),
            item: None,
        });
        app.data_draft = Some(draft);
        app.draft_dirty = true;
        assert!(!app.commit_draft());
        app.select(Some("other".parse().unwrap()));
        app.history(Request::Undo);
        assert!(app.selected.is_none());
        assert!(app.data_draft.is_some());
        let draft = app.data_draft.as_ref().unwrap();
        let (_, data_objects, _) = draft.build().unwrap();
        let document_id = document.id;
        let scope_id = draft.scope.clone();
        assert!(app.ctl.mutate(vec![Mutation::SetScopeDataObjects {
            document_id,
            scope_id,
            data_objects
        }]));
        app.data_draft = None;
        app.history(Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].processes[0]
                .data_objects
                .is_empty()
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].processes[0]
                .data_objects
                .len(),
            1
        );
    }
}
