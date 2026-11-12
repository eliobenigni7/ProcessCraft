use super::catalog::{IdentityDraft, id_build, id_string, optional_bool, optional_text};
use super::data::{ItemDraft, PortDraft, build_item, item_ui};
use super::event::{
    DefinitionDraft, DefinitionOptions, ReferenceDraft, build_ref, definitions_ui,
    local_choice_label, local_choices, ref_ui,
};
use super::inspector::{ExpressionDefault, optional_expression_ui_with_default};
use super::*;
use processcraft_core::bpmn_activity::{
    ComplexBehaviorDefinition, ImplicitThrowEvent, MultiInstanceFlowCondition,
};
use processcraft_core::bpmn_data::DataPort;
use processcraft_core::bpmn_model::LoopCharacteristics;

struct ComplexDraft {
    value: ComplexBehaviorDefinition,
    definitions: Vec<DefinitionDraft>,
    new_kind: String,
    definition_refs: Vec<ReferenceDraft>,
    properties: Vec<super::event::PropertyDraft>,
    io: Option<super::event::EventIoDraft>,
    associations: Vec<ImplicitAssociationDraft>,
    incoming: Vec<ReferenceDraft>,
    outgoing: Vec<ReferenceDraft>,
    event_categories: Vec<ReferenceDraft>,
}
struct ImplicitAssociationDraft {
    value: processcraft_core::bpmn_activity::ImplicitInputAssociation,
    sources: Vec<Option<ElementSelector>>,
}
impl ImplicitAssociationDraft {
    fn new(value: &processcraft_core::bpmn_activity::ImplicitInputAssociation) -> Self {
        Self {
            value: value.clone(),
            sources: value.sources.iter().cloned().map(Some).collect(),
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_activity::ImplicitInputAssociation, String> {
        let mut value = self.value.clone();
        value.sources = self
            .sources
            .iter()
            .map(|v| {
                v.clone()
                    .ok_or_else(|| "Select an input source.".to_owned())
            })
            .collect::<Result<_, _>>()?;
        Ok(value)
    }
}
pub(super) struct LoopAdvancedDraft {
    target: ElementSelector,
    value: LoopCharacteristics,
    input_ref: Option<ReferenceDraft>,
    output_ref: Option<ReferenceDraft>,
    one_ref: Option<ReferenceDraft>,
    none_ref: Option<ReferenceDraft>,
    input: Option<PortDraft>,
    output: Option<PortDraft>,
    complex: Vec<ComplexDraft>,
    error: String,
}
fn port_new(v: &DataPort) -> PortDraft {
    PortDraft {
        value: v.clone(),
        id: id_string(&v.id),
        item: v.item_aware.as_ref().map(ItemDraft::from_item),
    }
}
fn port_build(v: &Option<PortDraft>) -> Result<Option<DataPort>, String> {
    v.as_ref()
        .map(|p| {
            let mut v = p.value.clone();
            v.id = id_build(&p.id)?;
            v.item_aware = build_item(&p.item)?;
            Ok(v)
        })
        .transpose()
}
fn port_ui(ui: &mut egui::Ui, label: &str, v: &mut Option<PortDraft>) {
    let mut present = v.is_some();
    ui.checkbox(&mut present, label);
    if present {
        let p = v.get_or_insert_with(|| PortDraft {
            value: DataPort {
                id: None,
                base: None,
                name: None,
                item_aware: None,
                is_collection: None,
            },
            id: IdentityDraft::default(),
            item: None,
        });
        optional_text(ui, "ID", &mut p.id.value);
        optional_text(ui, "Name", &mut p.value.name);
        optional_bool(ui, "Collection", &mut p.value.is_collection);
        item_ui(ui, &mut p.item);
    } else {
        *v = None;
    }
}
impl LoopAdvancedDraft {
    fn new(target: ElementSelector, value: &LoopCharacteristics) -> Self {
        let (input_ref, output_ref, one_ref, none_ref, input, output, complex) = match value {
            LoopCharacteristics::MultiInstance {
                loop_data_input_ref,
                loop_data_output_ref,
                one_behavior_event_ref,
                none_behavior_event_ref,
                input_data_item,
                output_data_item,
                complex_behavior_definitions,
                ..
            } => (
                loop_data_input_ref.as_ref().map(ReferenceDraft::new),
                loop_data_output_ref.as_ref().map(ReferenceDraft::new),
                one_behavior_event_ref.as_ref().map(ReferenceDraft::new),
                none_behavior_event_ref.as_ref().map(ReferenceDraft::new),
                input_data_item.as_ref().map(port_new),
                output_data_item.as_ref().map(port_new),
                complex_behavior_definitions
                    .iter()
                    .map(|v| ComplexDraft {
                        value: v.clone(),
                        definitions: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.definitions.iter().map(DefinitionDraft::new).collect()
                        }),
                        new_kind: "Message".into(),
                        event_categories: v
                            .event
                            .as_ref()
                            .and_then(|e| e.flow.as_ref())
                            .map_or_else(Vec::new, |f| {
                                f.category_value_refs
                                    .iter()
                                    .map(ReferenceDraft::new)
                                    .collect()
                            }),
                        incoming: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.incoming.iter().map(ReferenceDraft::new).collect()
                        }),
                        outgoing: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.outgoing.iter().map(ReferenceDraft::new).collect()
                        }),
                        associations: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.input_associations
                                .iter()
                                .map(ImplicitAssociationDraft::new)
                                .collect()
                        }),
                        properties: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.properties
                                .iter()
                                .map(super::event::PropertyDraft::new)
                                .collect()
                        }),
                        io: v.event.as_ref().map(|e| {
                            super::event::EventIoDraft::new(
                                &processcraft_core::bpmn_model::EventIo::Throw {
                                    inputs: e.inputs.clone(),
                                    input_set: e.input_set.clone(),
                                },
                            )
                        }),
                        definition_refs: v.event.as_ref().map_or_else(Vec::new, |e| {
                            e.definition_refs.iter().map(ReferenceDraft::new).collect()
                        }),
                    })
                    .collect(),
            ),
            _ => unreachable!(),
        };
        Self {
            target,
            value: value.clone(),
            input_ref,
            output_ref,
            one_ref,
            none_ref,
            input,
            output,
            complex,
            error: String::new(),
        }
    }
    fn build(&self) -> Result<LoopCharacteristics, String> {
        let mut value = self.value.clone();
        if let LoopCharacteristics::MultiInstance {
            loop_data_input_ref,
            loop_data_output_ref,
            one_behavior_event_ref,
            none_behavior_event_ref,
            input_data_item,
            output_data_item,
            complex_behavior_definitions,
            ..
        } = &mut value
        {
            *loop_data_input_ref = build_ref(&self.input_ref)?;
            *loop_data_output_ref = build_ref(&self.output_ref)?;
            *one_behavior_event_ref = build_ref(&self.one_ref)?;
            *none_behavior_event_ref = build_ref(&self.none_ref)?;
            *input_data_item = port_build(&self.input)?;
            *output_data_item = port_build(&self.output)?;
            *complex_behavior_definitions = self
                .complex
                .iter()
                .map(|d| {
                    let mut v = d.value.clone();
                    if let Some(event) = &mut v.event {
                        event.definitions = d
                            .definitions
                            .iter()
                            .map(DefinitionDraft::build)
                            .collect::<Result<_, _>>()?;
                        if let Some(flow) = &mut event.flow {
                            flow.category_value_refs = d
                                .event_categories
                                .iter()
                                .map(ReferenceDraft::build)
                                .collect::<Result<_, _>>()?;
                        }
                        event.incoming = d
                            .incoming
                            .iter()
                            .map(ReferenceDraft::build)
                            .collect::<Result<_, _>>()?;
                        event.outgoing = d
                            .outgoing
                            .iter()
                            .map(ReferenceDraft::build)
                            .collect::<Result<_, _>>()?;
                        event.input_associations = d
                            .associations
                            .iter()
                            .map(ImplicitAssociationDraft::build)
                            .collect::<Result<_, _>>()?;
                        event.properties = d
                            .properties
                            .iter()
                            .map(super::event::PropertyDraft::build)
                            .collect::<Result<_, _>>()?;
                        if let Some(io) = d.io.as_ref() {
                            if let processcraft_core::bpmn_model::EventIo::Throw {
                                inputs,
                                input_set,
                            } = io.build()?
                            {
                                event.inputs = inputs;
                                event.input_set = input_set;
                            }
                        } else {
                            event.inputs.clear();
                            event.input_set = None;
                        }
                        event.definition_refs = d
                            .definition_refs
                            .iter()
                            .map(ReferenceDraft::build)
                            .collect::<Result<_, _>>()?;
                    }
                    Ok(v)
                })
                .collect::<Result<_, String>>()?;
        }
        Ok(value)
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
    ) {
        let LoopCharacteristics::MultiInstance { behavior, .. } = &mut self.value else {
            return;
        };
        egui::ComboBox::from_label("Behavior")
            .selected_text(match behavior {
                None => "Unspecified",
                Some(MultiInstanceFlowCondition::None) => "None",
                Some(MultiInstanceFlowCondition::One) => "One",
                Some(MultiInstanceFlowCondition::All) => "All",
                Some(MultiInstanceFlowCondition::Complex) => "Complex",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(behavior, None, "Unspecified");
                for (v, label) in [
                    (MultiInstanceFlowCondition::None, "None"),
                    (MultiInstanceFlowCondition::One, "One"),
                    (MultiInstanceFlowCondition::All, "All"),
                    (MultiInstanceFlowCondition::Complex, "Complex"),
                ] {
                    ui.selectable_value(behavior, Some(v), label);
                }
            });
        let scopes = processcraft_core::queries::scopes(document);
        let mut visible_scopes = Vec::new();
        let mut child = self.target.clone();
        while let Some(scope) = scopes
            .iter()
            .find(|scope| scope.nodes.iter().any(|node| node.id == child))
        {
            let scope_id = scope.id.selector();
            if visible_scopes.contains(&scope_id) {
                break;
            }
            visible_scopes.push(scope_id.clone());
            child = scope_id;
        }
        let mut data = Vec::new();
        let mut visible_objects = Vec::new();
        for store in &document.data_stores {
            if let Some(id) = &store.id {
                data.push((
                    id.selector(),
                    local_choice_label(store.name.as_deref(), "Data store", id, data.len() + 1),
                ));
            }
        }
        for process in &document.processes {
            if visible_scopes.contains(&process.id.selector())
                && let Some(io) = &process.io_specification
            {
                for port in &io.inputs {
                    if let Some(id) = &port.id {
                        data.push((
                            id.selector(),
                            local_choice_label(
                                port.name.as_deref(),
                                "Data port",
                                id,
                                data.len() + 1,
                            ),
                        ));
                    }
                }
            }
        }
        for scope in scopes
            .into_iter()
            .filter(|scope| visible_scopes.contains(&scope.id.selector()))
        {
            for object in scope.data_objects {
                if let Some(id) = &object.id {
                    visible_objects.push((
                        id.selector(),
                        local_choice_label(
                            object.name.as_deref(),
                            "Data object",
                            id,
                            visible_objects.len() + 1,
                        ),
                    ));
                    data.push((
                        id.selector(),
                        local_choice_label(
                            object.name.as_deref(),
                            "Data object",
                            id,
                            data.len() + 1,
                        ),
                    ));
                }
            }
            for node in scope.nodes {
                if (node.id.selector() == self.target
                    || visible_scopes.contains(&node.id.selector()))
                    && let Some(activity) = &node.activity
                {
                    for property in &activity.properties {
                        if let Some(id) = &property.id {
                            data.push((
                                id.selector(),
                                local_choice_label(
                                    property.name.as_deref(),
                                    "Property",
                                    id,
                                    data.len() + 1,
                                ),
                            ));
                        }
                    }
                }
                if node.id.selector() == self.target
                    && let Some(io) = &node.io_specification
                {
                    for port in io.inputs.iter().chain(&io.outputs) {
                        if let Some(id) = &port.id {
                            data.push((
                                id.selector(),
                                local_choice_label(
                                    port.name.as_deref(),
                                    "Data port",
                                    id,
                                    data.len() + 1,
                                ),
                            ));
                        }
                    }
                }
                if matches!(
                    node.kind,
                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                ) {
                    data.push((
                        node.id.selector(),
                        local_choice_label(
                            node.name.as_deref(),
                            "Data reference",
                            &node.id,
                            data.len() + 1,
                        ),
                    ));
                }
            }
        }
        let owner = processcraft_core::queries::find_node(document, &self.target);
        let subprocess = owner.is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
            )
        });
        let port_options = |input| {
            owner
                .and_then(|node| node.io_specification.as_ref())
                .map_or_else(Vec::new, |io| {
                    if input { &io.inputs } else { &io.outputs }
                        .iter()
                        .enumerate()
                        .filter_map(|(i, port)| {
                            port.id.as_ref().map(|id| {
                                (
                                    id.selector(),
                                    local_choice_label(
                                        port.name.as_deref(),
                                        "Data port",
                                        id,
                                        i + 1,
                                    ),
                                )
                            })
                        })
                        .collect()
                })
        };
        let input_options = if subprocess {
            visible_objects.clone()
        } else {
            port_options(true)
        };
        let output_options = if subprocess {
            visible_objects
        } else {
            port_options(false)
        };
        ref_ui(ui, "Loop data input", &mut self.input_ref, &input_options);
        ref_ui(
            ui,
            "Loop data output",
            &mut self.output_ref,
            &output_options,
        );
        let events = local_choices(document, "Definition", None);
        ref_ui(ui, "One behavior event", &mut self.one_ref, &events);
        ref_ui(ui, "None behavior event", &mut self.none_ref, &events);
        ui.collapsing("Input data item", |ui| {
            port_ui(ui, "Present", &mut self.input)
        });
        ui.collapsing("Output data item", |ui| {
            port_ui(ui, "Present", &mut self.output)
        });
        let mut remove = None;
        let mut up = None;
        for (i, d) in self.complex.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.strong(format!("Complex behavior {}", i + 1));
                optional_expression_ui_with_default(
                    ui,
                    &format!("Behavior {} condition", i + 1),
                    &mut d.value.condition,
                    true,
                    expression_draft,
                    ExpressionDefault::ImplicitFormal,
                );
                let mut present = d.value.event.is_some();
                ui.add_enabled(
                    expression_draft.is_none(),
                    egui::Checkbox::new(&mut present, "Throw event present"),
                );
                if present {
                    let event = d.value.event.get_or_insert_with(|| ImplicitThrowEvent {
                        id: None,
                        name: None,
                        base: None,
                        flow: None,
                        properties: vec![],
                        incoming: vec![],
                        outgoing: vec![],
                        inputs: vec![],
                        input_associations: vec![],
                        input_set: None,
                        definitions: vec![],
                        definition_refs: vec![],
                    });
                    optional_text(ui, "Event name", &mut event.name);
                    ui.collapsing("Event flow details", |ui| {
                        let mut present = event.flow.is_some();
                        ui.checkbox(&mut present, "Flow details present");
                        if present {
                            let flow = event.flow.get_or_insert_with(Default::default);
                            for (label, value) in [
                                ("Auditing", &mut flow.auditing),
                                ("Monitoring", &mut flow.monitoring),
                            ] {
                                let mut present = value.is_some();
                                ui.checkbox(&mut present, label);
                                if present {
                                    value.get_or_insert_with(Default::default);
                                } else {
                                    *value = None;
                                }
                            }
                            let choices = document
                                .categories
                                .iter()
                                .flat_map(|c| &c.values)
                                .enumerate()
                                .map(|(j, v)| {
                                    (
                                        v.id.selector(),
                                        local_choice_label(
                                            v.value.as_deref(),
                                            "Category value",
                                            &v.id,
                                            j + 1,
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>();
                            super::callable::reference_list_ui(
                                ui,
                                "Category value",
                                &mut d.event_categories,
                                &choices,
                            );
                        } else {
                            event.flow = None;
                        }
                    });
                    let event_id = event.id.as_ref().map(|id| id.selector());
                    let flows = processcraft_core::queries::scopes(document)
                        .into_iter()
                        .find(|scope| visible_scopes.first().is_some_and(|id| scope.id == id))
                        .map_or_else(Vec::new, |scope| {
                            scope
                                .flows
                                .iter()
                                .filter(|flow| flow.kind == FlowKind::Sequence)
                                .collect::<Vec<_>>()
                        });
                    let incoming = flows
                        .iter()
                        .filter(|flow| event_id.as_ref().is_some_and(|id| flow.target == *id))
                        .enumerate()
                        .map(|(j, flow)| {
                            (
                                flow.id.selector(),
                                local_choice_label(
                                    flow.name.as_deref(),
                                    "Sequence flow",
                                    &flow.id,
                                    j + 1,
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    let outgoing = flows
                        .iter()
                        .filter(|flow| event_id.as_ref().is_some_and(|id| flow.source == *id))
                        .enumerate()
                        .map(|(j, flow)| {
                            (
                                flow.id.selector(),
                                local_choice_label(
                                    flow.name.as_deref(),
                                    "Sequence flow",
                                    &flow.id,
                                    j + 1,
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    ui.collapsing("Declared incoming flows", |ui| {
                        super::callable::reference_list_ui(
                            ui,
                            "Incoming flow",
                            &mut d.incoming,
                            &incoming,
                        )
                    });
                    ui.collapsing("Declared outgoing flows", |ui| {
                        super::callable::reference_list_ui(
                            ui,
                            "Outgoing flow",
                            &mut d.outgoing,
                            &outgoing,
                        )
                    });
                    ui.collapsing("Event properties", |ui| {
                        let mut remove = None;
                        let mut up = None;
                        for (j, p) in d.properties.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                ui.label(format!("Property {}", j + 1));
                                optional_text(ui, "Name", &mut p.name);
                                super::data::item_ui(ui, &mut p.item);
                                if ui
                                    .add_enabled(j > 0, egui::Button::new("Move property up"))
                                    .clicked()
                                {
                                    up = Some(j);
                                }
                                if ui.button("Remove property").clicked() {
                                    remove = Some(j);
                                }
                            });
                        }
                        if let Some(j) = remove {
                            d.properties.remove(j);
                        } else if let Some(j) = up {
                            d.properties.swap(j, j - 1);
                        }
                        if ui
                            .add_enabled(
                                d.properties.len() < 256,
                                egui::Button::new("Add property"),
                            )
                            .clicked()
                        {
                            d.properties.push(super::event::PropertyDraft::new(
                                &processcraft_core::bpmn_model::EventProperty {
                                    id: None,
                                    base: None,
                                    name: None,
                                    item_aware: None,
                                },
                            ));
                        }
                    });
                    ui.collapsing("Event inputs", |ui| {
                        super::event::event_io_ui(ui, &mut d.io, false)
                    });
                    ui.collapsing("Input associations", |ui| {
                        let targets = d
                            .io
                            .as_ref()
                            .and_then(|io| io.build().ok())
                            .and_then(|io| match io {
                                processcraft_core::bpmn_model::EventIo::Throw {
                                    inputs, ..
                                } => Some(inputs),
                                _ => None,
                            })
                            .unwrap_or_default()
                            .iter()
                            .enumerate()
                            .filter_map(|(j, p)| {
                                p.id.as_ref().map(|id| {
                                    (
                                        id.selector(),
                                        local_choice_label(p.name.as_deref(), "Input", id, j + 1),
                                    )
                                })
                            })
                            .collect::<Vec<_>>();
                        let mut sources = data.clone();
                        for port in [&self.input, &self.output].into_iter().flatten() {
                            if let Some(id) = super::catalog::id_selector(&port.id) {
                                sources.push((
                                    id,
                                    port.value
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| "Loop data item".into()),
                                ));
                            }
                        }
                        for (j, property) in d.properties.iter().enumerate() {
                            if let Some(id) = super::catalog::id_selector(&property.id) {
                                sources.push((
                                    id,
                                    property
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| format!("Property {}", j + 1)),
                                ));
                            }
                        }
                        let mut remove = None;
                        let mut up = None;
                        for (j, association) in d.associations.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                ui.separator();
                                ui.label(format!("Association {}", j + 1));
                                let mut remove_source = None;
                                let mut source_up = None;
                                for (k, source) in association.sources.iter_mut().enumerate() {
                                    ui.push_id(k, |ui| {
                                        super::callable::selector_ui(
                                            ui, "Source", source, &sources,
                                        );
                                        if ui
                                            .add_enabled(k > 0, egui::Button::new("Move source up"))
                                            .clicked()
                                        {
                                            source_up = Some(k);
                                        }
                                        if ui.button("Remove source").clicked() {
                                            remove_source = Some(k);
                                        }
                                    });
                                }
                                if let Some(k) = remove_source {
                                    association.sources.remove(k);
                                } else if let Some(k) = source_up {
                                    association.sources.swap(k, k - 1);
                                }
                                if ui
                                    .add_enabled(
                                        association.sources.len() < 256,
                                        egui::Button::new("Add source"),
                                    )
                                    .clicked()
                                {
                                    association.sources.push(None);
                                }
                                super::callable::selector_ui(
                                    ui,
                                    "Target input",
                                    &mut association.value.target,
                                    &targets,
                                );
                                optional_expression_ui_with_default(
                                    ui,
                                    &format!(
                                        "Behavior {} association {} transformation",
                                        i + 1,
                                        j + 1
                                    ),
                                    &mut association.value.transformation,
                                    true,
                                    expression_draft,
                                    ExpressionDefault::ImplicitFormal,
                                );
                                let mut remove_assignment = None;
                                let mut assignment_up = None;
                                for (k, assignment) in
                                    association.value.assignments.iter_mut().enumerate()
                                {
                                    ui.push_id(k, |ui| {
                                        super::inspector::expression_ui(
                                            ui,
                                            &format!(
                                                "Behavior {} association {} assignment {} source",
                                                i + 1,
                                                j + 1,
                                                k + 1
                                            ),
                                            &mut assignment.from,
                                            true,
                                            expression_draft,
                                            ExpressionDefault::Base,
                                        );
                                        super::inspector::expression_ui(
                                            ui,
                                            &format!(
                                                "Behavior {} association {} assignment {} target",
                                                i + 1,
                                                j + 1,
                                                k + 1
                                            ),
                                            &mut assignment.to,
                                            true,
                                            expression_draft,
                                            ExpressionDefault::Base,
                                        );
                                        if ui
                                            .add_enabled(
                                                expression_draft.is_none() && k > 0,
                                                egui::Button::new("Move assignment up"),
                                            )
                                            .clicked()
                                        {
                                            assignment_up = Some(k);
                                        }
                                        if ui
                                            .add_enabled(
                                                expression_draft.is_none(),
                                                egui::Button::new("Remove assignment"),
                                            )
                                            .clicked()
                                        {
                                            remove_assignment = Some(k);
                                        }
                                    });
                                }
                                if let Some(k) = remove_assignment {
                                    association.value.assignments.remove(k);
                                } else if let Some(k) = assignment_up {
                                    association.value.assignments.swap(k, k - 1);
                                }
                                if ui
                                    .add_enabled(
                                        expression_draft.is_none()
                                            && association.value.assignments.len() < 256,
                                        egui::Button::new("Add assignment"),
                                    )
                                    .clicked()
                                {
                                    association.value.assignments.push(
                                        processcraft_core::bpmn_data::DataAssignment {
                                            id: None,
                                            base: None,
                                            from: String::new().into(),
                                            to: String::new().into(),
                                        },
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        expression_draft.is_none() && j > 0,
                                        egui::Button::new("Move association up"),
                                    )
                                    .clicked()
                                {
                                    up = Some(j);
                                }
                                if ui
                                    .add_enabled(
                                        expression_draft.is_none(),
                                        egui::Button::new("Remove association"),
                                    )
                                    .clicked()
                                {
                                    remove = Some(j);
                                }
                            });
                        }
                        if let Some(j) = remove {
                            d.associations.remove(j);
                        } else if let Some(j) = up {
                            d.associations.swap(j, j - 1);
                        }
                        if ui
                            .add_enabled(
                                expression_draft.is_none() && d.associations.len() < 256,
                                egui::Button::new("Add association"),
                            )
                            .clicked()
                        {
                            d.associations.push(ImplicitAssociationDraft::new(
                                &processcraft_core::bpmn_activity::ImplicitInputAssociation {
                                    id: None,
                                    base: None,
                                    sources: vec![],
                                    target: None,
                                    transformation: None,
                                    assignments: vec![],
                                },
                            ));
                        }
                    });
                    ui.collapsing("Event definitions", |ui| {
                        let slot = format!("Behavior {} event", i + 1);
                        definitions_ui(
                            ui,
                            &mut d.definitions,
                            &mut d.new_kind,
                            document,
                            None,
                            expression_draft,
                            DefinitionOptions {
                                label: &slot,
                                allowed: &["Message", "Signal", "Escalation", "Compensate"],
                            },
                        )
                    });
                    ui.collapsing("Referenced definitions", |ui| {
                        super::callable::reference_list_ui(
                            ui,
                            "Definition",
                            &mut d.definition_refs,
                            &events,
                        )
                    });
                } else {
                    d.value.event = None;
                }
                if ui
                    .add_enabled(
                        expression_draft.is_none() && i > 0,
                        egui::Button::new("Move behavior up"),
                    )
                    .clicked()
                {
                    up = Some(i);
                }
                if ui
                    .add_enabled(
                        expression_draft.is_none(),
                        egui::Button::new("Remove behavior"),
                    )
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.complex.remove(i);
        } else if let Some(i) = up {
            self.complex.swap(i, i - 1);
        }
        if ui
            .add_enabled(
                expression_draft.is_none() && self.complex.len() < 256,
                egui::Button::new("Add complex behavior"),
            )
            .clicked()
        {
            self.complex.push(ComplexDraft {
                value: ComplexBehaviorDefinition {
                    id: None,
                    base: None,
                    condition: None,
                    event: None,
                },
                definitions: vec![],
                new_kind: "Message".into(),
                definition_refs: vec![],
                properties: vec![],
                io: None,
                associations: vec![],
                incoming: vec![],
                outgoing: vec![],
                event_categories: vec![],
            });
        }
    }
}
impl ProcessCraft {
    pub(super) fn loop_advanced_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.loop_advanced_draft.is_none() {
            if let Some(value @ LoopCharacteristics::MultiInstance { .. }) =
                &node.loop_characteristics
                && ui
                    .add_enabled(
                        editable && !self.draft_dirty,
                        egui::Button::new("Edit multi-instance details…"),
                    )
                    .clicked()
                && self.commit_draft()
            {
                self.loop_advanced_draft = Some(LoopAdvancedDraft::new(node.id.selector(), value));
            }
            return;
        }
        let draft = self.loop_advanced_draft.as_mut().unwrap();
        if draft.target != node.id.selector() {
            return;
        }
        draft.ui(ui, document, &mut self.expression_draft);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply multi-instance details"),
            )
            .clicked()
        {
            match draft.build() {
                Ok(value) => {
                    if self.ctl.mutate(vec![Mutation::SetNodeLoop {
                        document_id: document.id,
                        element_id: draft.target.clone(),
                        loop_characteristics: Some(value),
                    }]) {
                        self.loop_advanced_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel multi-instance details"),
            )
            .clicked()
        {
            self.loop_advanced_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_instance_draft_preserves_implicit_event_and_metadata() {
        let value = LoopCharacteristics::MultiInstance {
            id: None,
            base: Some(Default::default()),
            is_sequential: None,
            behavior: Some(MultiInstanceFlowCondition::Complex),
            loop_data_input_ref: None,
            loop_data_output_ref: None,
            input_data_item: None,
            output_data_item: None,
            one_behavior_event_ref: None,
            none_behavior_event_ref: None,
            cardinality: Some("count".into()),
            completion_condition: None,
            complex_behavior_definitions: vec![ComplexBehaviorDefinition {
                id: None,
                base: Some(Default::default()),
                condition: Some("x".into()),
                event: Some(ImplicitThrowEvent {
                    id: None,
                    name: Some(String::new()),
                    base: Some(Default::default()),
                    flow: None,
                    properties: vec![],
                    incoming: vec![],
                    outgoing: vec![],
                    inputs: vec![],
                    input_associations: vec![],
                    input_set: None,
                    definitions: vec![],
                    definition_refs: vec![],
                }),
            }],
        };
        let draft = LoopAdvancedDraft::new("Activity".parse().unwrap(), &value);
        assert_eq!(draft.build().unwrap(), value);
    }
}
