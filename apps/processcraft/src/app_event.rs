use super::catalog::{QNameDraft, optional_bool, optional_qname, optional_text, q_build};
use super::inspector::{ExpressionDefault, expression_ui};
use super::*;
use processcraft_core::bpmn_model::{
    EventDefinition as D, EventReference as R, NodeDetails, TimerKind,
};

#[derive(Clone)]
enum ReferenceDraft {
    Local(Option<ElementSelector>),
    Qualified(QNameDraft),
}
impl ReferenceDraft {
    fn new(value: &R) -> Self {
        match value {
            R::Local(value) => Self::Local(Some(value.clone())),
            R::Qualified(value) => Self::Qualified(QNameDraft::from(value)),
        }
    }
    fn build(&self) -> Result<R, String> {
        match self {
            Self::Local(value) => Ok(R::Local(value.clone().ok_or("Select a local reference.")?)),
            Self::Qualified(value) => Ok(R::Qualified(
                q_build(&Some(value.clone()))?.expect("present"),
            )),
        }
    }
}
fn unavailable_target_label(id: &ElementSelector) -> String {
    match id {
        ElementSelector::Xml(id) => format!("Unavailable target · {id}"),
        ElementSelector::Native { .. } => "Unavailable target".into(),
    }
}
fn local_choice_label(
    name: Option<&str>,
    kind: &str,
    identity: &processcraft_core::ids::ElementIdentity,
    ordinal: usize,
) -> String {
    let name = name.filter(|name| !name.is_empty()).unwrap_or(kind);
    identity.xml_id().map_or_else(
        || format!("{name} · {ordinal}"),
        |id| format!("{name} · {id}"),
    )
}
fn ref_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<ReferenceDraft>,
    choices: &[(ElementSelector, String)],
) {
    let mut present = value.is_some();
    ui.checkbox(&mut present, label);
    if !present {
        *value = None;
        return;
    }
    let value = value.get_or_insert_with(|| ReferenceDraft::Local(None));
    let mut qualified = matches!(value, ReferenceDraft::Qualified(_));
    if ui
        .checkbox(&mut qualified, "External or qualified reference")
        .changed()
    {
        *value = if qualified {
            ReferenceDraft::Qualified(QNameDraft::default())
        } else {
            ReferenceDraft::Local(None)
        };
    }
    match value {
        ReferenceDraft::Local(target) => {
            egui::ComboBox::from_label("Local target")
                .selected_text(target.as_ref().map_or_else(
                    || "Select target".to_owned(),
                    |id| {
                        choices
                            .iter()
                            .find(|(candidate, _)| candidate == id)
                            .map_or_else(|| unavailable_target_label(id), |(_, name)| name.clone())
                    },
                ))
                .show_ui(ui, |ui| {
                    for (id, name) in choices {
                        ui.selectable_value(target, Some(id.clone()), name);
                    }
                });
        }
        ReferenceDraft::Qualified(qname) => {
            let label = ui.label("Namespace");
            ui.text_edit_singleline(&mut qname.namespace)
                .labelled_by(label.id);
            let label = ui.label("Local name");
            ui.text_edit_singleline(&mut qname.local)
                .labelled_by(label.id);
        }
    }
}
fn build_ref(value: &Option<ReferenceDraft>) -> Result<Option<R>, String> {
    value.as_ref().map(ReferenceDraft::build).transpose()
}
#[derive(Clone)]
struct DefinitionDraft {
    value: D,
    first: Option<ReferenceDraft>,
    second: Option<ReferenceDraft>,
    sources: Vec<ReferenceDraft>,
}
impl DefinitionDraft {
    fn new(value: &D) -> Self {
        let (first, second, sources) = match value {
            D::Message {
                message_ref,
                operation_ref,
                ..
            } => (message_ref.as_ref(), operation_ref.as_ref(), &[][..]),
            D::Signal { signal_ref, .. } => (signal_ref.as_ref(), None, &[][..]),
            D::Error { error_ref, .. } => (error_ref.as_ref(), None, &[][..]),
            D::Escalation { escalation_ref, .. } => (escalation_ref.as_ref(), None, &[][..]),
            D::Compensate { activity_ref, .. } => (activity_ref.as_ref(), None, &[][..]),
            D::Link {
                target, sources, ..
            } => (target.as_ref(), None, sources.as_slice()),
            _ => (None, None, &[][..]),
        };
        Self {
            value: value.clone(),
            first: first.map(ReferenceDraft::new),
            second: second.map(ReferenceDraft::new),
            sources: sources.iter().map(ReferenceDraft::new).collect(),
        }
    }
    fn build(&self) -> Result<D, String> {
        let mut value = self.value.clone();
        match &mut value {
            D::Message {
                message_ref,
                operation_ref,
                ..
            } => {
                *message_ref = build_ref(&self.first)?;
                *operation_ref = build_ref(&self.second)?;
            }
            D::Signal { signal_ref, .. } => *signal_ref = build_ref(&self.first)?,
            D::Error { error_ref, .. } => *error_ref = build_ref(&self.first)?,
            D::Escalation { escalation_ref, .. } => *escalation_ref = build_ref(&self.first)?,
            D::Compensate { activity_ref, .. } => *activity_ref = build_ref(&self.first)?,
            D::Link {
                target, sources, ..
            } => {
                *target = build_ref(&self.first)?;
                *sources = self
                    .sources
                    .iter()
                    .map(ReferenceDraft::build)
                    .collect::<Result<_, _>>()?;
            }
            _ => {}
        }
        Ok(value)
    }
}
fn definition_kind(value: &D) -> &'static str {
    match value {
        D::Message { .. } => "Message",
        D::Signal { .. } => "Signal",
        D::Error { .. } => "Error",
        D::Escalation { .. } => "Escalation",
        D::Conditional { .. } => "Conditional",
        D::Link { .. } => "Link",
        D::Cancel { .. } => "Cancel",
        D::Compensate { .. } => "Compensate",
        D::Terminate { .. } => "Terminate",
        D::Timer { .. } => "Timer",
    }
}
fn new_definition(kind: &str) -> D {
    match kind {
        "Message" => D::Message {
            id: None,
            message_ref: None,
            operation_ref: None,
        },
        "Signal" => D::Signal {
            id: None,
            signal_ref: None,
        },
        "Error" => D::Error {
            id: None,
            error_ref: None,
        },
        "Escalation" => D::Escalation {
            id: None,
            escalation_ref: None,
        },
        "Conditional" => D::Conditional {
            id: None,
            expression: String::new().into(),
        },
        "Link" => D::Link {
            id: None,
            name: String::new(),
            sources: Vec::new(),
            target: None,
        },
        "Cancel" => D::Cancel { id: None },
        "Compensate" => D::Compensate {
            id: None,
            wait_for_completion: None,
            activity_ref: None,
        },
        "Terminate" => D::Terminate { id: None },
        _ => D::Timer {
            id: None,
            kind: None,
            expression: None,
        },
    }
}
#[derive(Clone)]
struct PropertyDraft {
    id: super::catalog::IdentityDraft,
    name: Option<String>,
    item: Option<super::data::ItemDraft>,
}
impl PropertyDraft {
    fn new(value: &processcraft_core::bpmn_model::EventProperty) -> Self {
        Self {
            id: super::catalog::id_string(&value.id),
            name: value.name.clone(),
            item: value
                .item_aware
                .as_ref()
                .map(super::data::ItemDraft::from_item),
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_model::EventProperty, String> {
        Ok(processcraft_core::bpmn_model::EventProperty {
            id: super::catalog::id_build(&self.id)?,
            name: self.name.clone(),
            item_aware: super::data::build_item(&self.item)?,
        })
    }
}
#[derive(Clone)]
struct EventPortDraft {
    value: processcraft_core::bpmn_data::DataPort,
    id: super::catalog::IdentityDraft,
    item: Option<super::data::ItemDraft>,
}
impl EventPortDraft {
    fn new(value: &processcraft_core::bpmn_data::DataPort) -> Self {
        Self {
            value: value.clone(),
            id: super::catalog::id_string(&value.id),
            item: value
                .item_aware
                .as_ref()
                .map(super::data::ItemDraft::from_item),
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_data::DataPort, String> {
        let mut value = self.value.clone();
        value.id = super::catalog::id_build(&self.id)?;
        value.item_aware = super::data::build_item(&self.item)?;
        Ok(value)
    }
}
#[derive(Clone)]
struct EventSetDraft {
    value: processcraft_core::bpmn_data::IoSet,
    id: super::catalog::IdentityDraft,
}
impl EventSetDraft {
    fn new(value: &processcraft_core::bpmn_data::IoSet) -> Self {
        Self {
            value: value.clone(),
            id: super::catalog::id_string(&value.id),
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_data::IoSet, String> {
        let mut value = self.value.clone();
        value.id = super::catalog::id_build(&self.id)?;
        Ok(value)
    }
}
#[derive(Clone)]
struct EventIoDraft {
    catch: bool,
    ports: Vec<EventPortDraft>,
    set: Option<EventSetDraft>,
}
impl EventIoDraft {
    fn new(value: &processcraft_core::bpmn_model::EventIo) -> Self {
        use processcraft_core::bpmn_model::EventIo;
        match value {
            EventIo::Catch {
                outputs,
                output_set,
            } => Self {
                catch: true,
                ports: outputs.iter().map(EventPortDraft::new).collect(),
                set: output_set.as_ref().map(EventSetDraft::new),
            },
            EventIo::Throw { inputs, input_set } => Self {
                catch: false,
                ports: inputs.iter().map(EventPortDraft::new).collect(),
                set: input_set.as_ref().map(EventSetDraft::new),
            },
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_model::EventIo, String> {
        let ports = self
            .ports
            .iter()
            .map(EventPortDraft::build)
            .collect::<Result<_, _>>()?;
        let set = self.set.as_ref().map(EventSetDraft::build).transpose()?;
        Ok(if self.catch {
            processcraft_core::bpmn_model::EventIo::Catch {
                outputs: ports,
                output_set: set,
            }
        } else {
            processcraft_core::bpmn_model::EventIo::Throw {
                inputs: ports,
                input_set: set,
            }
        })
    }
}
pub(super) struct EventDraft {
    element: ElementSelector,
    base: NodeDetails,
    definitions: Vec<DefinitionDraft>,
    references: Vec<ReferenceDraft>,
    properties: Vec<PropertyDraft>,
    io: Option<EventIoDraft>,
    new_kind: String,
    error: String,
}
impl EventDraft {
    fn new(element: ElementSelector, details: &NodeDetails) -> Option<Self> {
        if let NodeDetails::Event {
            definitions,
            definition_refs,
            properties,
            io,
            ..
        } = details
        {
            Some(Self {
                element,
                base: details.clone(),
                definitions: definitions.iter().map(DefinitionDraft::new).collect(),
                references: definition_refs.iter().map(ReferenceDraft::new).collect(),
                properties: properties.iter().map(PropertyDraft::new).collect(),
                io: io.as_ref().map(EventIoDraft::new),
                new_kind: "Timer".into(),
                error: String::new(),
            })
        } else {
            None
        }
    }
    fn build(&self) -> Result<NodeDetails, String> {
        let mut result = self.base.clone();
        if let NodeDetails::Event {
            definitions,
            definition_refs,
            properties,
            io,
            ..
        } = &mut result
        {
            *definitions = self
                .definitions
                .iter()
                .map(DefinitionDraft::build)
                .collect::<Result<_, _>>()?;
            *definition_refs = self
                .references
                .iter()
                .map(ReferenceDraft::build)
                .collect::<Result<_, _>>()?;
            *io = self.io.as_ref().map(EventIoDraft::build).transpose()?;
            *properties = self
                .properties
                .iter()
                .map(PropertyDraft::build)
                .collect::<Result<_, _>>()?;
        }
        Ok(result)
    }
}

fn local_choices(
    document: &BpmnDocument,
    kind: &str,
    scope: Option<&ElementSelector>,
) -> Vec<(ElementSelector, String)> {
    use processcraft_core::bpmn_model::EventResource;
    let mut out = Vec::new();
    for resource in &document.event_resources {
        let (id, name, tag) = match resource {
            EventResource::Message { id, name, .. } => (id, name, "Message"),
            EventResource::Signal { id, name, .. } => (id, name, "Signal"),
            EventResource::Error { id, name, .. } => (id, name, "Error"),
            EventResource::Escalation { id, name, .. } => (id, name, "Escalation"),
        };
        if kind == tag {
            out.push((
                id.selector(),
                local_choice_label(name.as_deref(), tag, id, out.len() + 1),
            ));
        }
    }
    if kind == "Operation" {
        for interface in &document.catalog.interfaces {
            for operation in &interface.operations {
                if let Some(id) = &operation.id {
                    out.push((
                        id.selector(),
                        local_choice_label(Some(&operation.name), "Operation", id, out.len() + 1),
                    ));
                }
            }
        }
    }
    if kind == "Definition" || kind == "Link" {
        for definition in &document.root_event_definitions {
            if (kind == "Definition" || matches!(definition, D::Link { .. }))
                && let Some(id) = definition_id(definition)
            {
                out.push((
                    id.selector(),
                    local_choice_label(None, definition_kind(definition), id, out.len() + 1),
                ));
            }
        }
    }
    if kind == "Activity" || kind == "Link" {
        for local_scope in processcraft_core::queries::scopes(document)
            .into_iter()
            .filter(|s| scope.is_none_or(|id| s.id == id))
        {
            for node in local_scope.nodes {
                if kind == "Activity" && node.kind.is_activity() {
                    out.push((
                        node.id.selector(),
                        local_choice_label(
                            node.name.as_deref(),
                            "Activity",
                            &node.id,
                            out.len() + 1,
                        ),
                    ));
                }
                if kind == "Link"
                    && let Some(NodeDetails::Event { definitions, .. }) = &node.details
                {
                    for definition in definitions {
                        if matches!(definition, D::Link { .. })
                            && let Some(id) = definition_id(definition)
                        {
                            out.push((
                                id.selector(),
                                local_choice_label(None, "Link", id, out.len() + 1),
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}
fn definition_id(value: &D) -> Option<&processcraft_core::ids::ElementIdentity> {
    match value {
        D::Message { id, .. }
        | D::Signal { id, .. }
        | D::Error { id, .. }
        | D::Escalation { id, .. }
        | D::Conditional { id, .. }
        | D::Link { id, .. }
        | D::Cancel { id }
        | D::Compensate { id, .. }
        | D::Terminate { id }
        | D::Timer { id, .. } => id.as_ref(),
    }
}
struct DefinitionOptions<'a> {
    label: &'a str,
    allowed: &'a [&'a str],
}
fn definitions_ui(
    ui: &mut egui::Ui,
    definitions: &mut Vec<DefinitionDraft>,
    new_kind: &mut String,
    document: &BpmnDocument,
    scope: Option<&ElementSelector>,
    expression_draft: &mut Option<super::inspector::ExpressionDraft>,
    options: DefinitionOptions<'_>,
) {
    let DefinitionOptions {
        label: slot,
        allowed,
    } = options;
    let mut remove = None;
    for (index, draft) in definitions.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.separator();
            ui.strong(format!(
                "{} definition {}",
                definition_kind(&draft.value),
                index + 1
            ));
            match &mut draft.value {
                D::Timer {
                    kind, expression, ..
                } => {
                    let before = *kind;
                    ui.add_enabled_ui(expression_draft.is_none(), |ui| {
                        egui::ComboBox::from_label("Timer choice")
                            .selected_text(match kind {
                                None => "Unspecified",
                                Some(TimerKind::Date) => "Date",
                                Some(TimerKind::Duration) => "Duration",
                                Some(TimerKind::Cycle) => "Cycle",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(kind, None, "Unspecified");
                                ui.selectable_value(kind, Some(TimerKind::Date), "Date");
                                ui.selectable_value(kind, Some(TimerKind::Duration), "Duration");
                                ui.selectable_value(kind, Some(TimerKind::Cycle), "Cycle");
                            });
                    });
                    if *kind != before {
                        if kind.is_some() && expression.is_none() {
                            *expression = Some(String::new().into());
                        }
                        if kind.is_none() {
                            *expression = None;
                        }
                    }
                    if let Some(expression) = expression {
                        expression_ui(
                            ui,
                            &format!("{slot} timer expression {}", index + 1),
                            expression,
                            false,
                            expression_draft,
                            ExpressionDefault::Base,
                        );
                    }
                }
                D::Conditional { expression, .. } => {
                    expression_ui(
                        ui,
                        &format!("{slot} condition expression {}", index + 1),
                        expression,
                        true,
                        expression_draft,
                        ExpressionDefault::Base,
                    );
                }
                D::Message { .. } => {
                    ref_ui(
                        ui,
                        "Message reference",
                        &mut draft.first,
                        &local_choices(document, "Message", scope),
                    );
                    ref_ui(
                        ui,
                        "Operation reference",
                        &mut draft.second,
                        &local_choices(document, "Operation", scope),
                    );
                }
                D::Signal { .. } => ref_ui(
                    ui,
                    "Signal reference",
                    &mut draft.first,
                    &local_choices(document, "Signal", scope),
                ),
                D::Error { .. } => ref_ui(
                    ui,
                    "Error reference",
                    &mut draft.first,
                    &local_choices(document, "Error", scope),
                ),
                D::Escalation { .. } => ref_ui(
                    ui,
                    "Escalation reference",
                    &mut draft.first,
                    &local_choices(document, "Escalation", scope),
                ),
                D::Compensate {
                    wait_for_completion,
                    ..
                } => {
                    optional_bool(ui, "Wait for completion", wait_for_completion);
                    ref_ui(
                        ui,
                        "Activity reference",
                        &mut draft.first,
                        &local_choices(document, "Activity", scope),
                    );
                }
                D::Link { name, .. } => {
                    let label = ui.label("Link name");
                    ui.text_edit_singleline(name).labelled_by(label.id);
                    ref_ui(
                        ui,
                        "Target link",
                        &mut draft.first,
                        &local_choices(document, "Link", scope),
                    );
                    ui.label("Source links");
                    let mut remove_source = None;
                    for (j, source) in draft.sources.iter_mut().enumerate() {
                        ui.push_id(j, |ui| {
                            let mut current = Some(source.clone());
                            ref_ui(
                                ui,
                                "Source link",
                                &mut current,
                                &local_choices(document, "Link", scope),
                            );
                            if let Some(value) = current {
                                *source = value;
                            } else {
                                remove_source = Some(j);
                            }
                        });
                    }
                    if let Some(j) = remove_source {
                        draft.sources.remove(j);
                    }
                    if ui.button("Add source link").clicked() {
                        draft.sources.push(ReferenceDraft::Local(None));
                    }
                }
                _ => {}
            }
            if ui
                .add_enabled(
                    expression_draft.is_none(),
                    egui::Button::new("Remove definition"),
                )
                .clicked()
            {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        definitions.remove(index);
    }
    egui::ComboBox::from_label("New definition type")
        .selected_text(new_kind.as_str())
        .show_ui(ui, |ui| {
            for kind in [
                "Timer",
                "Conditional",
                "Message",
                "Signal",
                "Error",
                "Escalation",
                "Link",
                "Cancel",
                "Compensate",
                "Terminate",
            ] {
                if allowed.contains(&kind) {
                    ui.selectable_value(new_kind, kind.to_owned(), kind);
                }
            }
        });
    if ui
        .add_enabled(
            expression_draft.is_none()
                && definitions.len() < 256
                && allowed.contains(&new_kind.as_str()),
            egui::Button::new("Add definition"),
        )
        .clicked()
    {
        definitions.push(DefinitionDraft::new(&new_definition(new_kind)));
    }
}
#[derive(Clone)]
struct ResourceDraft {
    value: processcraft_core::bpmn_model::EventResource,
    reference: Option<QNameDraft>,
}
impl ResourceDraft {
    fn new(value: &processcraft_core::bpmn_model::EventResource) -> Self {
        use processcraft_core::bpmn_model::EventResource as E;
        let reference = match value {
            E::Message { item_ref, .. } => item_ref,
            E::Signal { structure_ref, .. }
            | E::Error { structure_ref, .. }
            | E::Escalation { structure_ref, .. } => structure_ref,
        };
        Self {
            value: value.clone(),
            reference: reference.as_ref().map(QNameDraft::from),
        }
    }
    fn build(&self) -> Result<processcraft_core::bpmn_model::EventResource, String> {
        use processcraft_core::bpmn_model::EventResource as E;
        let mut value = self.value.clone();
        let reference = q_build(&self.reference)?;
        match &mut value {
            E::Message { item_ref, .. } => *item_ref = reference,
            E::Signal { structure_ref, .. }
            | E::Error { structure_ref, .. }
            | E::Escalation { structure_ref, .. } => *structure_ref = reference,
        };
        Ok(value)
    }
}
pub(super) struct EventCatalogDraft {
    definitions: Vec<DefinitionDraft>,
    resources: Vec<ResourceDraft>,
    new_kind: String,
    new_resource: String,
    error: String,
}
impl EventCatalogDraft {
    fn new(document: &BpmnDocument) -> Self {
        Self {
            definitions: document
                .root_event_definitions
                .iter()
                .map(DefinitionDraft::new)
                .collect(),
            resources: document
                .event_resources
                .iter()
                .map(ResourceDraft::new)
                .collect(),
            new_kind: "Timer".into(),
            new_resource: "Message".into(),
            error: String::new(),
        }
    }
}
fn new_resource(kind: &str) -> processcraft_core::bpmn_model::EventResource {
    use processcraft_core::bpmn_model::EventResource as E;
    let id = processcraft_core::ids::ElementIdentity::anonymous();
    match kind {
        "Signal" => E::Signal {
            id,
            name: None,
            structure_ref: None,
        },
        "Error" => E::Error {
            id,
            name: None,
            code: None,
            structure_ref: None,
        },
        "Escalation" => E::Escalation {
            id,
            name: None,
            code: None,
            structure_ref: None,
        },
        _ => E::Message {
            id,
            name: None,
            item_ref: None,
        },
    }
}
fn resources_ui(ui: &mut egui::Ui, resources: &mut Vec<ResourceDraft>, new_kind: &mut String) {
    use processcraft_core::bpmn_model::EventResource as E;
    let mut remove = None;
    for (index, draft) in resources.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            let (tag, name, code) = match &mut draft.value {
                E::Message { name, .. } => ("Message", name, None),
                E::Signal { name, .. } => ("Signal", name, None),
                E::Error { name, code, .. } => ("Error", name, Some(code)),
                E::Escalation { name, code, .. } => ("Escalation", name, Some(code)),
            };
            ui.separator();
            ui.strong(format!("{tag} {}", index + 1));
            optional_text(ui, "Name", name);
            if let Some(code) = code {
                optional_text(ui, "Code", code);
            }
            optional_qname(
                ui,
                if tag == "Message" {
                    "Item reference"
                } else {
                    "Structure reference"
                },
                &mut draft.reference,
            );
            if ui.button("Remove declaration").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        resources.remove(index);
    }
    egui::ComboBox::from_label("New declaration type")
        .selected_text(new_kind.as_str())
        .show_ui(ui, |ui| {
            for kind in ["Message", "Signal", "Error", "Escalation"] {
                ui.selectable_value(new_kind, kind.to_owned(), kind);
            }
        });
    if ui
        .add_enabled(resources.len() < 256, egui::Button::new("Add declaration"))
        .clicked()
    {
        resources.push(ResourceDraft::new(&new_resource(new_kind)));
    }
}
fn set_refs_ui(
    ui: &mut egui::Ui,
    label: &str,
    refs: &mut Vec<ElementSelector>,
    choices: &[ElementSelector],
) {
    ui.label(label);
    let mut values = choices.to_vec();
    for reference in refs.iter() {
        if !values.contains(reference) {
            values.push(reference.clone());
        }
    }
    for id in values {
        let mut selected = refs.contains(&id);
        if ui.checkbox(&mut selected, id.to_string()).changed() {
            if selected {
                refs.push(id);
            } else {
                refs.retain(|value| value != &id);
            }
        }
    }
}
fn event_io_ui(ui: &mut egui::Ui, io: &mut Option<EventIoDraft>, catch: bool) {
    let mut present = io.is_some();
    ui.checkbox(&mut present, "Event input/output");
    if !present {
        *io = None;
        return;
    }
    let io = io.get_or_insert_with(|| EventIoDraft {
        catch,
        ports: Vec::new(),
        set: None,
    });
    ui.label(if io.catch {
        "Data outputs"
    } else {
        "Data inputs"
    });
    let mut remove = None;
    for (index, port) in io.ports.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.separator();
            ui.label(format!("Port {}", index + 1));
            optional_text(ui, "Name", &mut port.value.name);
            optional_bool(ui, "Collection", &mut port.value.is_collection);
            super::data::item_ui(ui, &mut port.item);
            if ui.button("Remove port").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        io.ports.remove(index);
    }
    if ui
        .add_enabled(io.ports.len() < 256, egui::Button::new("Add port"))
        .clicked()
    {
        let value = processcraft_core::bpmn_data::DataPort {
            id: Some(processcraft_core::ids::ElementIdentity::anonymous()),
            name: None,
            is_collection: None,
            item_aware: None,
        };
        io.ports.push(EventPortDraft::new(&value));
    }
    let mut present = io.set.is_some();
    ui.checkbox(
        &mut present,
        if io.catch { "Output set" } else { "Input set" },
    );
    if present {
        let set = io
            .set
            .get_or_insert_with(|| EventSetDraft::new(&Default::default()));
        optional_text(ui, "Set name", &mut set.value.name);
        let ports = io
            .ports
            .iter()
            .filter_map(|port| super::catalog::id_selector(&port.id))
            .collect::<Vec<_>>();
        set_refs_ui(ui, "Ports", &mut set.value.data_refs, &ports);
        set_refs_ui(
            ui,
            "Optional ports",
            &mut set.value.optional_refs,
            &set.value.data_refs,
        );
        set_refs_ui(
            ui,
            "While executing",
            &mut set.value.while_executing_refs,
            &set.value.data_refs,
        );
    } else {
        io.set = None;
    }
}
impl ProcessCraft {
    pub(super) fn event_editor_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let Some(node) = processcraft_core::queries::find_node(document, &selected) else {
            return;
        };
        if !matches!(
            node.kind,
            NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ) {
            return;
        }
        if self.event_draft.is_none() {
            if ui
                .add_enabled(editable, egui::Button::new("Edit event definitions…"))
                .clicked()
                && self.commit_draft()
            {
                let base = match &node.details {
                    Some(value @ NodeDetails::Event { .. }) => value.clone(),
                    Some(NodeDetails::BoundaryEvent {
                        attached_to,
                        cancel_activity,
                    }) => NodeDetails::Event {
                        definitions: Vec::new(),
                        attached_to: Some(attached_to.clone()),
                        cancel_activity: Some(*cancel_activity),
                        definition_refs: Vec::new(),
                        parallel_multiple: None,
                        is_interrupting: None,
                        properties: Vec::new(),
                        io: None,
                    },
                    _ => NodeDetails::Event {
                        definitions: Vec::new(),
                        attached_to: None,
                        cancel_activity: None,
                        definition_refs: Vec::new(),
                        parallel_multiple: None,
                        is_interrupting: None,
                        properties: Vec::new(),
                        io: None,
                    },
                };
                self.event_draft = EventDraft::new(selected, &base);
            }
            return;
        }
        let scope = processcraft_core::queries::scopes(document)
            .into_iter()
            .find(|scope| scope.nodes.iter().any(|candidate| candidate.id == selected))
            .map(|scope| scope.id.selector());
        let parent = scope
            .as_ref()
            .and_then(|id| processcraft_core::queries::find_node(document, id));
        let event_sub_start = node.kind == NodeKind::StartEvent
            && parent.is_some_and(|parent| {
                parent.kind == NodeKind::SubProcess
                    && parent
                        .activity
                        .as_ref()
                        .is_some_and(|activity| activity.triggered_by_event == Some(true))
            });
        let transaction_scope = parent.is_some_and(|parent| parent.kind == NodeKind::Transaction);
        let attached = match &node.details {
            Some(NodeDetails::Event { attached_to, .. }) => attached_to.as_ref(),
            Some(NodeDetails::BoundaryEvent { attached_to, .. }) => Some(attached_to),
            _ => None,
        };
        let transaction_attachment = attached
            .and_then(|id| processcraft_core::queries::find_node(document, id))
            .is_some_and(|activity| activity.kind == NodeKind::Transaction);
        let mut allowed = vec!["Message", "Signal"];
        if matches!(
            node.kind,
            NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
        ) {
            allowed.extend(["Timer", "Conditional"]);
        }
        if matches!(
            node.kind,
            NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
        ) {
            allowed.push("Link");
        }
        if matches!(node.kind, NodeKind::EndEvent | NodeKind::BoundaryEvent) || event_sub_start {
            allowed.push("Error");
        }
        if matches!(
            node.kind,
            NodeKind::EndEvent | NodeKind::BoundaryEvent | NodeKind::IntermediateThrowEvent
        ) || event_sub_start
        {
            allowed.extend(["Escalation", "Compensate"]);
        }
        if node.kind == NodeKind::EndEvent {
            allowed.push("Terminate");
        }
        if (node.kind == NodeKind::BoundaryEvent && transaction_attachment)
            || (node.kind == NodeKind::EndEvent && transaction_scope)
        {
            allowed.push("Cancel");
        }
        let draft = self.event_draft.as_mut().unwrap();
        ui.strong("Event properties");
        if let NodeDetails::Event {
            attached_to,
            cancel_activity,
            parallel_multiple,
            is_interrupting,
            ..
        } = &mut draft.base
        {
            if node.kind == NodeKind::BoundaryEvent {
                egui::ComboBox::from_label("Attached activity")
                    .selected_text(
                        attached_to
                            .as_ref()
                            .map_or("Select activity".into(), ToString::to_string),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in local_choices(document, "Activity", scope.as_ref()) {
                            ui.selectable_value(attached_to, Some(id), name);
                        }
                    });
                optional_bool(ui, "Interrupt activity", cancel_activity);
            }
            if matches!(
                node.kind,
                NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
            ) {
                optional_bool(ui, "Parallel multiple", parallel_multiple);
            }
            if node.kind == NodeKind::StartEvent {
                optional_bool(ui, "Interrupting start", is_interrupting);
                if !event_sub_start {
                    ui.label("This setting applies to event-subprocess starts.");
                }
            }
        }
        ui.collapsing("Inline definitions", |ui| {
            definitions_ui(
                ui,
                &mut draft.definitions,
                &mut draft.new_kind,
                document,
                scope.as_ref(),
                &mut self.expression_draft,
                DefinitionOptions {
                    label: "Inline",
                    allowed: &allowed,
                },
            )
        });
        ui.collapsing("Referenced definitions", |ui| {
            let mut remove = None;
            for (index, reference) in draft.references.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    let mut value = Some(reference.clone());
                    ref_ui(
                        ui,
                        "Definition reference",
                        &mut value,
                        &local_choices(document, "Definition", scope.as_ref()),
                    );
                    if let Some(value) = value {
                        *reference = value;
                    } else {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                draft.references.remove(index);
            }
            if ui
                .add_enabled(
                    draft.references.len() < 256,
                    egui::Button::new("Add definition reference"),
                )
                .clicked()
            {
                draft.references.push(ReferenceDraft::Local(None));
            }
        });
        ui.collapsing("Event properties", |ui| {
            let mut remove = None;
            for (index, property) in draft.properties.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.label(format!("Property {}", index + 1));
                    optional_text(ui, "Name", &mut property.name);
                    super::data::item_ui(ui, &mut property.item);
                    if ui.button("Remove property").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                draft.properties.remove(index);
            }
            if ui
                .add_enabled(
                    draft.properties.len() < 256,
                    egui::Button::new("Add property"),
                )
                .clicked()
            {
                draft.properties.push(PropertyDraft {
                    id: Default::default(),
                    name: None,
                    item: None,
                });
            }
        });
        ui.collapsing("Event input/output", |ui| {
            event_io_ui(
                ui,
                &mut draft.io,
                matches!(
                    node.kind,
                    NodeKind::StartEvent
                        | NodeKind::IntermediateCatchEvent
                        | NodeKind::BoundaryEvent
                ),
            )
        });
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply event properties"),
            )
            .clicked()
        {
            self.apply_event_draft();
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel event editing"),
            )
            .clicked()
        {
            self.event_draft = None;
        }
    }
    fn apply_event_draft(&mut self) -> bool {
        let Some(document_id) = self
            .ctl
            .session
            .snapshot()
            .project
            .documents
            .get(self.doc)
            .map(|document| document.id)
        else {
            return false;
        };
        let Some(draft) = &mut self.event_draft else {
            return false;
        };
        let details = match draft.build() {
            Ok(details) => details,
            Err(error) => {
                draft.error = error;
                return false;
            }
        };
        if !self.ctl.mutate(vec![Mutation::SetNodeDetails {
            document_id,
            element_id: draft.element.clone(),
            details: Some(details.clone()),
        }]) {
            return false;
        }
        self.bpmn_details = Some(details);
        self.event_draft = None;
        self.restore_selection();
        true
    }
    pub(super) fn event_catalog_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        ui.separator();
        if self.event_catalog_draft.is_none() {
            if ui
                .add_enabled(editable, egui::Button::new("Edit event catalog…"))
                .clicked()
                && self.commit_draft()
            {
                self.event_catalog_draft = Some(EventCatalogDraft::new(document));
            }
            return;
        }
        let draft = self.event_catalog_draft.as_mut().unwrap();
        ui.strong("Event catalog");
        ui.collapsing("Root definitions", |ui| {
            definitions_ui(
                ui,
                &mut draft.definitions,
                &mut draft.new_kind,
                document,
                None,
                &mut self.expression_draft,
                DefinitionOptions {
                    label: "Root",
                    allowed: &[
                        "Timer",
                        "Conditional",
                        "Message",
                        "Signal",
                        "Error",
                        "Escalation",
                        "Link",
                        "Cancel",
                        "Compensate",
                        "Terminate",
                    ],
                },
            )
        });
        ui.collapsing("Messages, signals, errors and escalations", |ui| {
            resources_ui(ui, &mut draft.resources, &mut draft.new_resource)
        });
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply event catalog"),
            )
            .clicked()
        {
            let definitions = draft
                .definitions
                .iter()
                .map(DefinitionDraft::build)
                .collect::<Result<Vec<_>, _>>();
            let resources = draft
                .resources
                .iter()
                .map(ResourceDraft::build)
                .collect::<Result<Vec<_>, _>>();
            match (definitions, resources) {
                (Ok(definitions), Ok(resources)) => {
                    if self.ctl.mutate(vec![
                        Mutation::SetRootEventDefinitions {
                            document_id: document.id,
                            definitions,
                        },
                        Mutation::SetEventResources {
                            document_id: document.id,
                            resources,
                        },
                    ]) {
                        self.event_catalog_draft = None;
                    }
                }
                (Err(error), _) | (_, Err(error)) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel event catalog editing"),
            )
            .clicked()
        {
            self.event_catalog_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_model::{EventIo, EventProperty, QNameRef};
    #[test]
    fn local_reference_choices_show_xml_ids_without_native_handles() {
        use processcraft_core::ids::{ElementIdentity, NativeElementId};
        let with_id = ElementIdentity::Native {
            native_id: NativeElementId::new(),
            xml_id: Some("SharedTimer".parse().unwrap()),
        };
        let anonymous = ElementIdentity::anonymous();
        let mut document = crate::controller::blank_document();
        document.root_event_definitions.push(D::Timer {
            id: Some(with_id.clone()),
            kind: None,
            expression: None,
        });
        document.root_event_definitions.push(D::Timer {
            id: Some(anonymous.clone()),
            kind: None,
            expression: None,
        });
        let choices = local_choices(&document, "Definition", None);
        assert_eq!(
            choices[0],
            (with_id.selector(), "Timer · SharedTimer".into())
        );
        assert_eq!(choices[1], (anonymous.selector(), "Timer · 2".into()));
        assert_eq!(
            unavailable_target_label(&anonymous.selector()),
            "Unavailable target"
        );
        assert_eq!(
            unavailable_target_label(&"Missing".parse().unwrap()),
            "Unavailable target · Missing"
        );
        assert!(choices.iter().all(|(_, label)| !label.contains("native:")));
        assert_eq!(
            local_choice_label(Some("Review"), "Activity", &with_id, 1),
            "Review · SharedTimer"
        );
    }

    #[test]
    fn event_draft_blocks_navigation_history_and_transition() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let node = processcraft_core::queries::find_node(document, &selected).unwrap();
        app.event_draft = EventDraft::new(selected.clone(), node.details.as_ref().unwrap());
        assert!(app.has_drafts());
        assert!(!app.commit_draft());
        app.select(None);
        app.history(Request::Undo);
        assert_eq!(app.selected, Some(selected));
        assert!(app.event_draft.is_some());
    }

    #[test]
    fn applying_event_keeps_pending_name_without_reverting_definitions() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        let mut draft = EventDraft::new(selected.clone(), node.details.as_ref().unwrap()).unwrap();
        draft.definitions.push(DefinitionDraft::new(&D::Signal {
            id: None,
            signal_ref: None,
        }));
        app.event_draft = Some(draft);
        app.draft = "Renamed".into();
        app.draft_name_present = true;
        app.draft_dirty = true;
        assert!(app.apply_event_draft(), "{}", app.ctl.error);
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        assert_eq!(node.name.as_deref(), Some("Renamed"));
        assert!(
            matches!(&node.details, Some(NodeDetails::Event { definitions, .. }) if definitions.len() == 2)
        );
    }

    #[test]
    fn event_draft_preserves_multiple_definitions_references_and_io() {
        let root_id = processcraft_core::ids::ElementIdentity::anonymous();
        let root = D::Timer {
            id: Some(root_id.clone()),
            kind: None,
            expression: None,
        };
        let mut project = crate::controller::blank_project();
        std::sync::Arc::make_mut(&mut project.documents[0])
            .root_event_definitions
            .push(root);
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap();
        let mut draft = EventDraft::new(selected.clone(), node.details.as_ref().unwrap()).unwrap();
        draft.definitions.push(DefinitionDraft::new(&D::Signal {
            id: None,
            signal_ref: None,
        }));
        draft
            .references
            .push(ReferenceDraft::Local(Some(root_id.selector())));
        draft.references.push(ReferenceDraft::Qualified(QNameDraft {
            namespace: "urn:external".into(),
            local: "Shared".into(),
        }));
        draft.properties.push(PropertyDraft::new(&EventProperty {
            id: None,
            name: Some(String::new()),
            item_aware: None,
        }));
        draft.io = Some(EventIoDraft::new(&EventIo::Catch {
            outputs: vec![processcraft_core::bpmn_data::DataPort {
                id: Some(processcraft_core::ids::ElementIdentity::anonymous()),
                name: None,
                item_aware: None,
                is_collection: Some(false),
            }],
            output_set: None,
        }));
        let details = draft.build().unwrap();
        let document_id = snapshot.project.documents[0].id;
        assert!(
            app.ctl.mutate(vec![Mutation::SetNodeDetails {
                document_id,
                element_id: selected.clone(),
                details: Some(details.clone())
            }]),
            "{}",
            app.ctl.error
        );
        app.history(Request::Undo);
        app.history(Request::Redo);
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(
            processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected)
                .unwrap()
                .details,
            Some(details)
        );
        let mut invalid = ReferenceDraft::Qualified(QNameDraft {
            namespace: "urn:external".into(),
            local: "not valid".into(),
        });
        assert!(invalid.build().is_err());
        if let ReferenceDraft::Qualified(value) = &mut invalid {
            assert_eq!(value.local, "not valid");
        }
        let _ = QNameRef {
            namespace: String::new(),
            local: "Valid".parse().unwrap(),
        };
    }
}
