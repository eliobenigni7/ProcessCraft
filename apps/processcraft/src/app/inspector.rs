//! Properties inspector: the side panel that edits the selected element, flow or document.
use super::{ProcessCraft, node_kind_label};
use crate::widgets;
use processcraft_core::bpmn_metadata::DocumentationSet;
use processcraft_core::{bpmn_model, queries};

/// Human label of a gateway direction (never Rust debug output).
fn gateway_direction_label(direction: bpmn_model::GatewayDirection) -> &'static str {
    use bpmn_model::GatewayDirection as G;
    match direction {
        G::Unspecified => "Unspecified",
        G::Converging => "Converging",
        G::Diverging => "Diverging",
        G::Mixed => "Mixed",
    }
}

fn graph_node<'a>(
    document: &'a processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> Option<&'a processcraft_core::model::Node> {
    queries::choreography_scopes(document)
        .into_iter()
        .find_map(|scope| {
            scope
                .events
                .into_iter()
                .chain(scope.gateways)
                .find(|node| node.id == *id)
        })
}
fn graph_flow<'a>(
    document: &'a processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> Option<&'a processcraft_core::model::Flow> {
    queries::choreography_scopes(document)
        .into_iter()
        .find_map(|scope| scope.flows.into_iter().find(|flow| flow.id == *id))
}
fn graph_artifact<'a>(
    document: &'a processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> Option<&'a processcraft_core::bpmn_collaboration::CollaborationArtifact> {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    queries::choreography_scopes(document)
        .into_iter()
        .flat_map(|scope| scope.artifacts)
        .find(|artifact| match artifact {
            CollaborationArtifact::Group(value) => value.id == *id,
            CollaborationArtifact::TextAnnotation(value) => value.id == *id,
            CollaborationArtifact::Association(value) => value.id == *id,
        })
}
fn graph_data_association<'a>(
    document: &'a processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> Option<&'a processcraft_core::bpmn_data::DataAssociation> {
    queries::choreography_scopes(document)
        .into_iter()
        .flat_map(|scope| scope.data_associations)
        .find(|association| {
            association
                .id
                .as_ref()
                .is_some_and(|identity| identity == id)
        })
}

fn is_group(
    document: &processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> bool {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    let in_scope_groups = queries::scopes(document)
        .iter()
        .any(|scope| scope.groups.iter().any(|group| group.id == *id));
    let root_extras = document
        .collaborations()
        .filter_map(|root| root.extras.as_ref())
        .chain(
            document
                .choreographies
                .iter()
                .filter_map(|root| root.extras.as_ref()),
        );
    let in_root_artifacts = root_extras
        .flat_map(|extras| &extras.artifacts)
        .any(|artifact| matches!(artifact, CollaborationArtifact::Group(group) if group.id == *id));
    in_scope_groups
        || in_root_artifacts
        || matches!(
            graph_artifact(document, id),
            Some(CollaborationArtifact::Group(_))
        )
}
pub(super) fn selection_has_name(
    document: &processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> bool {
    !document
        .endpoints
        .iter()
        .any(|endpoint| endpoint.id.as_ref().is_some_and(|identity| identity == id))
        && !is_group(document, id)
        && graph_artifact(document, id).is_none()
        && graph_data_association(document, id).is_none()
        && !queries::find_flow(document, id)
            .is_some_and(|flow| flow.kind == processcraft_core::model::FlowKind::Association)
}
pub(super) fn selection_kind(
    document: &processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> Option<&'static str> {
    if document
        .partner_entities
        .iter()
        .any(|value| value.id.as_ref().is_some_and(|identity| identity == id))
    {
        Some("Partner entity")
    } else if document
        .partner_roles
        .iter()
        .any(|value| value.id.as_ref().is_some_and(|identity| identity == id))
    {
        Some("Partner role")
    } else if document
        .endpoints
        .iter()
        .any(|value| value.id.as_ref().is_some_and(|identity| identity == id))
    {
        Some("Endpoint")
    } else if let Some(root) = document.collaborations().find(|root| root.id == *id) {
        Some(if root.kind.is_some() {
            "Global conversation"
        } else {
            "Collaboration"
        })
    } else if let Some(root) = document.choreographies.iter().find(|root| root.id == *id) {
        Some(if root.kind.is_some() {
            "Global choreography"
        } else {
            "Choreography"
        })
    } else if graph_data_association(document, id).is_some() {
        Some("Event data association")
    } else if let Some(artifact) = graph_artifact(document, id) {
        Some(match artifact {
            processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(_) => "Group",
            processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(_) => {
                "Text annotation"
            }
            processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(_) => {
                "Association"
            }
        })
    } else if is_group(document, id) {
        Some("Group")
    } else if queries::find_flow(document, id)
        .is_some_and(|flow| flow.kind == processcraft_core::model::FlowKind::Association)
    {
        Some("Association")
    } else {
        None
    }
}

fn flow_label(
    document: &processcraft_core::model::BpmnDocument,
    id: &processcraft_core::ids::ElementSelector,
) -> String {
    queries::element_name(document, id)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            queries::element_identity(document, id)
                .and_then(|identity| identity.xml_id())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "Unnamed flow".into())
}

#[derive(PartialEq, Eq)]
enum XmlIdTarget {
    Existing(processcraft_core::ids::ElementSelector),
    Anonymous(String),
}
pub(super) struct XmlIdDraft {
    target: XmlIdTarget,
    value: Option<String>,
    error: String,
}

impl ProcessCraft {
    /// Apply an open XML ID draft (selection change, history): same mutation as its button.
    pub(super) fn apply_xml_id_draft(&mut self) -> Result<(), String> {
        let Some(current) = self.drafts.xml_id_draft.as_mut() else {
            return Ok(());
        };
        let xml_id = current
            .value
            .as_ref()
            .map(|text| {
                text.parse::<processcraft_core::ids::XmlId>()
                    .map_err(|_| "Enter a valid XML ID.".to_owned())
            })
            .transpose()
            .inspect_err(|e| current.error = e.clone())?;
        let document_id = self.ctl.session.snapshot().project.documents[self.doc].id;
        let mutation = match &current.target {
            XmlIdTarget::Existing(target) => {
                processcraft_core::commands::Mutation::SetElementXmlId {
                    document_id,
                    element_id: target.clone(),
                    xml_id,
                }
            }
            XmlIdTarget::Anonymous(path) => {
                processcraft_core::commands::Mutation::PromoteElementIdentity {
                    document_id,
                    model_path: path.clone(),
                    xml_id,
                }
            }
        };
        let changes = self
            .ctl
            .mutate_changes(vec![mutation])
            .ok_or_else(|| self.ctl.error.clone())?;
        self.drafts.xml_id_draft = None;
        self.remap_context(&changes.selector_mappings);
        Ok(())
    }
    /// Apply open message/operation references into the activity properties draft.
    pub(super) fn apply_binding_draft(&mut self) -> Result<(), String> {
        let Some(draft) = self.drafts.binding_draft.as_mut() else {
            return Ok(());
        };
        let (message, operation) = match (
            super::catalog::q_build(&draft.message),
            super::catalog::q_build(&draft.operation),
        ) {
            (Ok(message), Ok(operation)) => (message, operation),
            (Err(error), _) | (_, Err(error)) => {
                draft.error = error.clone();
                return Err(error);
            }
        };
        let original = self.bpmn_activity.clone().unwrap_or_default();
        let mut value = original.clone();
        value.message_ref = message;
        value.operation_ref = operation;
        if value != original {
            self.bpmn_activity = Some(value);
            self.draft_dirty = true;
        }
        self.drafts.binding_draft = None;
        Ok(())
    }
}

fn xml_id_editor(
    ui: &mut egui::Ui,
    draft: &mut Option<XmlIdDraft>,
    controller: &mut crate::controller::Controller,
    document_id: processcraft_core::ids::DocumentId,
    target: &processcraft_core::ids::ElementSelector,
    xml_id: Option<&processcraft_core::ids::XmlId>,
    enabled: bool,
) -> Option<Vec<processcraft_core::commands::SelectorMapping>> {
    if draft.is_none() {
        if ui
            .add_enabled(enabled, egui::Button::new("Edit XML ID…"))
            .clicked()
        {
            *draft = Some(XmlIdDraft {
                target: XmlIdTarget::Existing(target.clone()),
                value: xml_id.map(ToString::to_string),
                error: String::new(),
            });
        }
    } else if draft
        .as_ref()
        .is_some_and(|draft| draft.target == XmlIdTarget::Existing(target.clone()))
    {
        let current = draft.as_mut().expect("draft exists");
        optional_metadata_text(ui, "XML ID", &mut current.value);
        if !current.error.is_empty() {
            ui.label(&current.error);
        }
        let mut clear = false;
        let mut mappings = None;
        if ui.button("Apply XML ID").clicked() {
            match current
                .value
                .as_ref()
                .map(|text| {
                    text.parse::<processcraft_core::ids::XmlId>()
                        .map_err(|_| "Enter a valid XML ID.".to_owned())
                })
                .transpose()
            {
                Ok(xml_id) => {
                    if let Some(changes) = controller.mutate_changes(vec![
                        processcraft_core::commands::Mutation::SetElementXmlId {
                            document_id,
                            element_id: target.clone(),
                            xml_id,
                        },
                    ]) {
                        mappings = Some(changes.selector_mappings);
                        clear = true;
                    }
                }
                Err(error) => current.error = error,
            }
        }
        if ui.button("Cancel XML ID editing").clicked() {
            clear = true;
        }
        if clear {
            *draft = None;
        }
        return mappings;
    }
    None
}

fn anonymous_xml_id_editor(
    ui: &mut egui::Ui,
    draft: &mut Option<XmlIdDraft>,
    controller: &mut crate::controller::Controller,
    document_id: processcraft_core::ids::DocumentId,
    path: &str,
    enabled: bool,
) -> Option<Vec<processcraft_core::commands::SelectorMapping>> {
    if draft.is_none() {
        if ui
            .add_enabled(enabled, egui::Button::new("Assign XML ID…"))
            .clicked()
        {
            *draft = Some(XmlIdDraft {
                target: XmlIdTarget::Anonymous(path.into()),
                value: Some(String::new()),
                error: String::new(),
            });
        }
    } else if draft
        .as_ref()
        .is_some_and(|draft| draft.target == XmlIdTarget::Anonymous(path.into()))
    {
        let current = draft.as_mut().expect("draft exists");
        optional_metadata_text(ui, "XML ID", &mut current.value);
        if !current.error.is_empty() {
            ui.label(&current.error);
        }
        let mut clear = false;
        let mut mappings = None;
        if ui.button("Apply XML ID").clicked() {
            match current
                .value
                .as_ref()
                .map(|text| {
                    text.parse::<processcraft_core::ids::XmlId>()
                        .map_err(|_| "Enter a valid XML ID.".to_owned())
                })
                .transpose()
            {
                Ok(xml_id) => {
                    if let Some(changes) = controller.mutate_changes(vec![
                        processcraft_core::commands::Mutation::PromoteElementIdentity {
                            document_id,
                            model_path: path.into(),
                            xml_id,
                        },
                    ]) {
                        mappings = Some(changes.selector_mappings);
                        clear = true;
                    }
                }
                Err(error) => current.error = error,
            }
        }
        if ui.button("Cancel XML ID editing").clicked() {
            clear = true;
        }
        if clear {
            *draft = None;
        }
        return mappings;
    }
    None
}

fn optional_identity_rows(
    document: &processcraft_core::model::BpmnDocument,
) -> Vec<(
    String,
    String,
    Option<processcraft_core::ids::ElementIdentity>,
)> {
    fn visit(
        value: &serde_json::Value,
        path: &str,
        rows: &mut Vec<(
            String,
            String,
            Option<processcraft_core::ids::ElementIdentity>,
        )>,
    ) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (base_key, id_key, label) in [
            ("lane_set_base", "lane_set_id", "Lane set"),
            ("child_lane_set_base", "child_lane_set_id", "Child lane set"),
        ] {
            if object.get(base_key).is_some_and(|value| !value.is_null()) {
                let identity = object
                    .get(id_key)
                    .and_then(|value| serde_json::from_value(value.clone()).ok());
                rows.push((label.into(), format!("{path}/{id_key}"), identity));
            }
        }
        for (key, child) in object {
            let child_path = format!("{path}/{key}");
            if let Some(array) = child.as_array() {
                let label = match key.as_str() {
                    "data_stores" => Some("Data store"),
                    "data_objects" => Some("Data object"),
                    "data_associations" => Some("Data association"),
                    "inputs" => Some("Data input"),
                    "outputs" => Some("Data output"),
                    "input_sets" => Some("Input set"),
                    "output_sets" => Some("Output set"),
                    "assignments" => Some("Assignment"),
                    "root_event_definitions" | "definitions" => Some("Event definition"),
                    "documentation" => Some("Documentation"),
                    "label_styles" => Some("Label style"),
                    "relationships" => Some("Relationship"),
                    "global_tasks" => Some("Global task"),
                    "correlation_properties" => Some("Correlation property"),
                    "retrieval_expressions" => Some("Correlation retrieval"),
                    "correlation_keys" => Some("Correlation key"),
                    "correlation_subscriptions" => Some("Correlation subscription"),
                    "bindings" | "io_bindings" => Some("Binding"),
                    "resource_roles" => Some("Resource role"),
                    "renderings" => Some("Rendering"),
                    "complex_behavior_definitions" => Some("Complex behavior"),
                    "properties" => Some("Event property"),
                    "item_definitions" => Some("Item definition"),
                    "interfaces" => Some("Interface"),
                    "operations" => Some("Operation"),
                    "resources" => Some("Resource"),
                    "parameters" => Some("Resource parameter"),
                    "partner_entities" => Some("Partner entity"),
                    "partner_roles" => Some("Partner role"),
                    "endpoints" => Some("Endpoint"),
                    "conversation_associations" => Some("Conversation association"),
                    "participant_associations" => Some("Participant association"),
                    "message_flow_associations" => Some("Message flow association"),
                    _ => None,
                };
                for (index, item) in array.iter().enumerate() {
                    let item_path = format!("{child_path}/{index}");
                    if let (Some(label), Some(entry)) = (label, item.as_object()) {
                        let identity = entry
                            .get("id")
                            .and_then(|id| serde_json::from_value(id.clone()).ok());
                        let name = entry
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .filter(|name| !name.is_empty());
                        rows.push((
                            format!(
                                "{label} {}{}",
                                index + 1,
                                name.map_or(String::new(), |name| format!(" · {name}"))
                            ),
                            format!("{item_path}/id"),
                            identity,
                        ));
                    }
                    visit(item, &item_path, rows);
                }
            } else if child.is_object() {
                if matches!(
                    key.as_str(),
                    "label"
                        | "event"
                        | "auditing"
                        | "monitoring"
                        | "message_path"
                        | "data_path"
                        | "assignment"
                        | "input_data_item"
                        | "output_data_item"
                        | "multiplicity"
                        | "loop_characteristics"
                        | "data_state"
                        | "io_specification"
                        | "input_set"
                        | "output_set"
                        | "from"
                        | "to"
                        | "transformation"
                        | "condition"
                        | "expression"
                        | "cardinality"
                        | "completion_condition"
                        | "activation_condition"
                ) && (key != "assignment" || child.get("id").is_some())
                {
                    let identity = child
                        .get("id")
                        .and_then(|id| serde_json::from_value(id.clone()).ok());
                    let label = match key.as_str() {
                        "label" => "Diagram label",
                        "event" => "Implicit event",
                        "auditing" => "Auditing",
                        "monitoring" => "Monitoring",
                        "message_path" => "Message path",
                        "data_path" => "Data path",
                        "assignment" => "Resource assignment",
                        "input_data_item" => "Input data item",
                        "output_data_item" => "Output data item",
                        "loop_characteristics" => "Loop",
                        "multiplicity" => "Participant multiplicity",
                        "data_state" => "Data state",
                        "io_specification" => "I/O specification",
                        "from" => "Assignment source expression",
                        "to" => "Assignment target expression",
                        "input_set" => "Input set",
                        "output_set" => "Output set",
                        "transformation" => "Transformation expression",
                        _ => "Expression",
                    };
                    rows.push((label.into(), format!("{child_path}/id"), identity));
                }
                visit(child, &child_path, rows);
            }
        }
    }
    let mut rows = Vec::new();
    if let Ok(value) = serde_json::to_value(document) {
        visit(&value, "", &mut rows);
    }
    rows
}

pub(super) struct BindingDraft {
    message: Option<super::catalog::QNameDraft>,
    operation: Option<super::catalog::QNameDraft>,
    error: String,
}
impl BindingDraft {
    fn new(value: &processcraft_core::bpmn_activity::ActivityProperties) -> Self {
        Self {
            message: value
                .message_ref
                .as_ref()
                .map(super::catalog::QNameDraft::from),
            operation: value
                .operation_ref
                .as_ref()
                .map(super::catalog::QNameDraft::from),
            error: String::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ExpressionDefault {
    Base,
    SequenceFlow,
    ImplicitFormal,
}

pub(super) struct ExpressionDraft {
    slot: String,
    default: ExpressionDefault,
    id: super::catalog::IdentityDraft,
    formal: bool,
    explicit_type: bool,
    language: Option<String>,
    type_reference: Option<(String, String)>,
    text: super::xml::ExpressionBodyDraft,
    foreign_attributes: Vec<super::xml::AttributeDraft>,
    namespaces: Vec<(String, String)>,
    error: String,
}

impl ExpressionDraft {
    fn new(slot: &str, value: &bpmn_model::Expression, default: ExpressionDefault) -> Self {
        use bpmn_model::{Expression, ExpressionKind};
        let mut draft = Self {
            slot: slot.into(),
            default,
            id: Default::default(),
            formal: false,
            explicit_type: false,
            language: None,
            type_reference: None,
            text: super::xml::ExpressionBodyDraft::new(&match value {
                Expression::Legacy(text) => text.clone().into(),
                Expression::Structured(data) => data.text.clone(),
            }),
            foreign_attributes: Vec::new(),
            namespaces: Vec::new(),
            error: String::new(),
        };
        if matches!(value, Expression::Legacy(_)) {
            draft.namespaces.push((
                String::new(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            ));
            if matches!(default, ExpressionDefault::ImplicitFormal) {
                draft.formal = true;
            }
            if matches!(default, ExpressionDefault::SequenceFlow) {
                draft.formal = true;
                draft.explicit_type = true;
                draft.namespaces.push((
                    "xsi".into(),
                    "http://www.w3.org/2001/XMLSchema-instance".into(),
                ));
            }
        }
        if let Expression::Structured(data) = value {
            draft.id = super::catalog::id_string(&data.id);
            draft.foreign_attributes = data
                .foreign_attributes
                .iter()
                .map(super::xml::AttributeDraft::new)
                .collect();
            draft.namespaces = data.namespaces.clone();
            match &data.kind {
                ExpressionKind::Base { explicit_type } => draft.explicit_type = *explicit_type,
                ExpressionKind::Formal {
                    explicit_type,
                    language,
                    evaluates_to_type_ref,
                } => {
                    draft.formal = true;
                    draft.explicit_type = *explicit_type;
                    draft.language = language.clone();
                    draft.type_reference = evaluates_to_type_ref
                        .as_ref()
                        .map(|q| (q.namespace.clone(), q.local.to_string()));
                }
            }
        }
        draft
    }
    fn build(&self) -> Result<bpmn_model::ExpressionData, String> {
        use bpmn_model::{ExpressionData, ExpressionKind, QNameRef};
        let id = super::catalog::id_build(&self.id)?;
        let type_ref = if self.formal {
            self.type_reference
                .as_ref()
                .map(|(namespace, local)| {
                    Ok::<QNameRef, String>(QNameRef {
                        namespace: namespace.clone(),
                        local: local.parse().map_err(|_| {
                            "Type reference local name must be a valid XML ID.".to_owned()
                        })?,
                    })
                })
                .transpose()?
        } else {
            None
        };
        if matches!(self.default, ExpressionDefault::ImplicitFormal) && !self.formal {
            return Err("This expression requires a formal expression.".into());
        }
        if self.formal
            && !self.explicit_type
            && !matches!(self.default, ExpressionDefault::ImplicitFormal)
        {
            return Err("Formal expressions require an explicit type in this context.".into());
        }
        let kind = if self.formal {
            ExpressionKind::Formal {
                explicit_type: self.explicit_type,
                language: self.language.clone(),
                evaluates_to_type_ref: type_ref,
            }
        } else {
            ExpressionKind::Base {
                explicit_type: self.explicit_type,
            }
        };
        Ok(ExpressionData {
            id,
            kind,
            text: self.text.build()?,
            namespaces: self.namespaces.clone(),
            foreign_attributes: self
                .foreign_attributes
                .iter()
                .map(super::xml::AttributeDraft::build)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// A name field without a presence checkbox: typing sets the name, clearing removes it.
fn name_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    muted: egui::Color32,
) -> bool {
    let label = ui.label(egui::RichText::new(label).small().color(muted));
    let mut text = value.clone().unwrap_or_default();
    let changed = ui
        .text_edit_singleline(&mut text)
        .labelled_by(label.id)
        .changed();
    if changed {
        *value = (!text.is_empty()).then_some(text);
    }
    changed
}
pub(super) fn association_direction_label(
    direction: Option<bpmn_model::AssociationDirection>,
) -> &'static str {
    use bpmn_model::AssociationDirection as A;
    match direction {
        None => "Default (no arrow)",
        Some(A::None) => "No arrow",
        Some(A::One) => "Arrow to target",
        Some(A::Both) => "Arrows at both ends",
    }
}
fn optional_metadata_text(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut present = value.is_some();
    let checkbox = ui.checkbox(&mut present, label);
    if present {
        ui.text_edit_singleline(value.get_or_insert_with(String::new))
            .labelled_by(checkbox.id);
    } else {
        *value = None;
    }
}

pub(super) fn expression_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut bpmn_model::Expression,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
    default: ExpressionDefault,
) -> bool {
    use bpmn_model::Expression;
    let label_response = ui.label(label);
    let mut changed = false;
    ui.add_enabled_ui(draft.is_none(), |ui| {
        if let Some(text) = value.text_mut() {
            let response = if multiline {
                ui.text_edit_multiline(text)
            } else {
                ui.text_edit_singleline(text)
            };
            changed |= response.labelled_by(label_response.id).changed();
        } else {
            ui.label("Structured expression content");
        }
        if ui.button("Edit expression metadata…").clicked() {
            *draft = Some(ExpressionDraft::new(label, value, default));
        }
    });
    if draft.as_ref().is_some_and(|d| d.slot == label) {
        let current = draft.as_mut().expect("draft exists");
        ui.collapsing("Expression content", |ui| current.text.ui(ui));
        ui.collapsing("Foreign attributes", |ui| {
            let mut remove = None;
            for (i, attribute) in current.foreign_attributes.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    attribute.ui(ui);
                    if ui.button("Remove attribute").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                current.foreign_attributes.remove(i);
            }
            if ui
                .add_enabled(
                    current.foreign_attributes.len() < 256,
                    egui::Button::new("Add attribute"),
                )
                .clicked()
            {
                current.foreign_attributes.push(super::xml::AttributeDraft {
                    namespace: String::new(),
                    local: String::new(),
                    prefix: None,
                    value: String::new(),
                });
            }
        });
        optional_metadata_text(ui, "Expression ID", &mut current.id.value);
        let was_formal = current.formal;
        egui::ComboBox::from_label("Expression kind")
            .selected_text(if current.formal { "Formal" } else { "Base" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut current.formal, false, "Base");
                ui.selectable_value(&mut current.formal, true, "Formal");
            });
        if current.formal && !was_formal {
            current.explicit_type = !matches!(current.default, ExpressionDefault::ImplicitFormal);
            if current.explicit_type
                && !current.namespaces.iter().any(|(prefix, uri)| {
                    !prefix.is_empty() && uri == "http://www.w3.org/2001/XMLSchema-instance"
                })
            {
                let mut prefix = "xsi".to_owned();
                while current
                    .namespaces
                    .iter()
                    .any(|(existing, _)| existing == &prefix)
                {
                    prefix.push('_');
                }
                current
                    .namespaces
                    .push((prefix, "http://www.w3.org/2001/XMLSchema-instance".into()));
            }
        }
        if ui
            .checkbox(&mut current.explicit_type, "Explicit expression type")
            .changed()
            && current.explicit_type
            && !current.namespaces.iter().any(|(prefix, uri)| {
                !prefix.is_empty() && uri == "http://www.w3.org/2001/XMLSchema-instance"
            })
        {
            let mut prefix = "xsi".to_owned();
            while current
                .namespaces
                .iter()
                .any(|(existing, _)| existing == &prefix)
            {
                prefix.push('_');
            }
            current
                .namespaces
                .push((prefix, "http://www.w3.org/2001/XMLSchema-instance".into()));
        }
        if current.formal {
            optional_metadata_text(ui, "Language", &mut current.language);
            let mut present = current.type_reference.is_some();
            ui.checkbox(&mut present, "Evaluates to type");
            if present {
                let (namespace, local) = current
                    .type_reference
                    .get_or_insert_with(|| (String::new(), String::new()));
                let label = ui.label("Type namespace");
                ui.text_edit_singleline(namespace).labelled_by(label.id);
                let label = ui.label("Type local name");
                ui.text_edit_singleline(local).labelled_by(label.id);
            } else {
                current.type_reference = None;
            }
        }
        ui.label("Namespace bindings");
        let mut remove = None;
        for (index, (prefix, namespace)) in current.namespaces.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                let label = ui.label("Prefix (empty for default)");
                ui.text_edit_singleline(prefix).labelled_by(label.id);
                let label = ui.label("Namespace URI");
                ui.text_edit_singleline(namespace).labelled_by(label.id);
                if ui.button("Remove namespace").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            current.namespaces.remove(index);
        }
        if ui
            .add_enabled(
                current.namespaces.len() < 256,
                egui::Button::new("Add namespace"),
            )
            .clicked()
        {
            current.namespaces.push((String::new(), String::new()));
        }
        if !current.error.is_empty() {
            ui.label(&current.error);
        }
        let mut clear = false;
        if ui.button("Apply expression metadata").clicked() {
            match current.build() {
                Ok(data) => {
                    *value = Expression::Structured(data);
                    changed = true;
                    clear = true;
                }
                Err(error) => current.error = error,
            }
        }
        if ui.button("Cancel expression metadata").clicked() {
            clear = true;
        }
        if clear {
            *draft = None;
        }
    }
    changed
}

pub(super) fn optional_expression_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<bpmn_model::Expression>,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
) -> bool {
    optional_expression_ui_with_default(ui, label, value, multiline, draft, ExpressionDefault::Base)
}

pub(super) fn optional_expression_ui_with_default(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<bpmn_model::Expression>,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
    default: ExpressionDefault,
) -> bool {
    let mut present = value.is_some();
    let checkbox = ui.add_enabled(draft.is_none(), egui::Checkbox::new(&mut present, label));
    let mut changed = checkbox.changed();
    if present {
        let expression = value.get_or_insert_with(|| String::new().into());
        changed |= expression_ui(ui, label, expression, multiline, draft, default);
    } else if value.take().is_some() {
        changed = true;
    }
    changed
}

/// True when a multi-instance loop carries data, behavior or event details beyond its type.
fn is_extended_loop(value: &Option<bpmn_model::LoopCharacteristics>) -> bool {
    use bpmn_model::LoopCharacteristics as L;
    let Some(L::MultiInstance {
        behavior,
        loop_data_input_ref,
        loop_data_output_ref,
        input_data_item,
        output_data_item,
        one_behavior_event_ref,
        none_behavior_event_ref,
        complex_behavior_definitions,
        ..
    }) = value
    else {
        return false;
    };
    behavior.is_some()
        || loop_data_input_ref.is_some()
        || loop_data_output_ref.is_some()
        || input_data_item.is_some()
        || output_data_item.is_some()
        || one_behavior_event_ref.is_some()
        || none_behavior_event_ref.is_some()
        || !complex_behavior_definitions.is_empty()
}

/// True when a loop carries an XML ID or base metadata that a type change would drop.
fn has_loop_metadata(value: &Option<bpmn_model::LoopCharacteristics>) -> bool {
    use bpmn_model::LoopCharacteristics as L;
    matches!(
        value,
        Some(
            L::Standard { id: Some(_), .. }
                | L::Standard { base: Some(_), .. }
                | L::MultiInstance { id: Some(_), .. }
                | L::MultiInstance { base: Some(_), .. }
        )
    )
}

/// "XML ID · <id or Not assigned>" label line.
fn xml_id_text(xml_id: Option<&processcraft_core::ids::XmlId>) -> String {
    let id = xml_id.map_or("Not assigned".into(), ToString::to_string);
    format!("XML ID · {id}")
}

/// "<kind> XML ID · <id or Not assigned>" label line.
fn xml_id_label(kind: &str, xml_id: Option<&processcraft_core::ids::XmlId>) -> String {
    format!("{kind} {}", xml_id_text(xml_id))
}

/// Small muted helper text used by the property labels.
fn small_muted(text: impl Into<String>, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text).small().color(color)
}

/// Kind label for a selection that is not a node (partner, participant, activity, connection).
fn selection_fallback_kind(
    document: Option<&processcraft_core::model::BpmnDocument>,
    scopes: &[queries::ChoreographyScope<'_>],
    selected: &processcraft_core::ids::ElementSelector,
    is_flow: bool,
) -> &'static str {
    if let Some(kind) = document.and_then(|document| selection_kind(document, selected)) {
        kind
    } else if document.is_some_and(|document| {
        document.choreographies.iter().any(|choreography| {
            choreography
                .participants
                .iter()
                .any(|participant| participant.id == *selected)
        })
    }) {
        "Participant"
    } else if scopes.iter().any(|scope| {
        scope
            .activities
            .iter()
            .any(|activity| activity.id == *selected)
    }) {
        "Choreography activity"
    } else if is_flow {
        "Connection"
    } else {
        "Selection"
    }
}

impl ProcessCraft {
    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui, show_heading: bool) {
        let snap = self.ctl.session.snapshot();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        let process = snap.project.documents.get(self.doc).and_then(|d| {
            queries::scopes(d)
                .into_iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        });
        let palette = self.theme.palette();
        // XML ID editors are available only while no other property edit is pending.
        let id_edit_ok = editable
            && !self.draft_dirty
            && self.drafts.expression_draft.is_none()
            && self.drafts.binding_draft.is_none()
            && (self.drafts.xml_id_draft.is_some() || !self.ordinary_property_locked());
        // Built once per frame and shared by every choreography lookup below.
        let scopes = snap
            .project
            .documents
            .get(self.doc)
            .map(|document| queries::choreography_scopes(document))
            .unwrap_or_default();
        let _ = show_heading;
        let current_doc = snap.project.documents.get(self.doc);
        // Selector remaps produced by an XML ID edit this frame, applied after the panel.
        let mut selector_mappings = None;
        egui::ScrollArea::vertical()
            .id_salt("inspector_scroll")
            .show(ui, |side| {
                side.spacing_mut().text_edit_width = (side.available_width() - 8.0).max(40.0);
                if let Some(selected_owned) = self.selected.clone() {
                    let selected = &selected_owned;
                    let ordinary_enabled = editable && !self.ordinary_property_locked();
                    let mut advanced_body = None;
                    let selected_node = current_doc.and_then(|d| {
                        queries::find_node(d, selected).or_else(|| graph_node(d, selected))
                    });
                    let kind = match selected_node {
                        Some(n) => node_kind_label(n.kind),
                        None => selection_fallback_kind(
                            current_doc.map(|d| &**d),
                            &scopes,
                            selected,
                            self.is_flow,
                        ),
                    };
                    let node_kind = selected_node.map(|n| n.kind);
                    let replaceable = editable
                        && current_doc.is_some_and(|d| {
                            !processcraft_core::replace::replace_options(d, selected).is_empty()
                        });
                    let comment_count = snap
                        .project
                        .comments
                        .iter()
                        .filter(|c| c.target.xml_id == *selected)
                        .count();
                    let title = if self.draft.is_empty() {
                        "Untitled".to_owned()
                    } else {
                        self.draft.clone()
                    };
                    let header = header_card(
                        side,
                        kind,
                        &title,
                        node_kind,
                        self.is_flow,
                        replaceable,
                        comment_count,
                    );
                    match header {
                        Some(HeaderAction::Replace) => self.open_replace(selected.clone()),
                        Some(HeaderAction::Comments) => self.open_comments(),
                        None => {}
                    }
                    let annotation = current_doc
                        .and_then(|d| queries::find_node(d, selected))
                        .is_some_and(|n| {
                            n.kind == processcraft_core::model::NodeKind::TextAnnotation
                        });
                    let documentation = if self.is_node || self.is_flow {
                        current_doc
                            .and_then(|document| {
                                let node = queries::find_node(document, selected)
                                    .or_else(|| graph_node(document, selected))
                                    .map(|node| &node.documentation);
                                node.or_else(|| {
                                    queries::find_flow(document, selected)
                                        .or_else(|| graph_flow(document, selected))
                                        .map(|flow| &flow.documentation)
                                })
                            })
                            .cloned()
                    } else {
                        None
                    };
                    let selected_flow = current_doc.and_then(|d| {
                        queries::find_flow(d, selected).or_else(|| graph_flow(d, selected))
                    });
                    widgets::section(side, "General", None, true, |side| {
                    let association = current_doc
                        .and_then(|d| queries::find_flow(d, selected))
                        .is_some_and(|flow| flow.kind == processcraft_core::model::FlowKind::Association);
                    let data_object_reference = current_doc
                        .and_then(|d| queries::find_node(d, selected))
                        .is_some_and(|n| n.kind == processcraft_core::model::NodeKind::DataObjectReference);
                    if annotation {
                        let label = side.label(small_muted("Annotation text", palette.muted));
                        let body = current_doc
                            .and_then(|d| queries::find_node(d, selected))
                            .and_then(|n| match &n.details {
                                Some(bpmn_model::NodeDetails::TextAnnotation {
                                    text: Some(text),
                                    ..
                                }) => Some(text.clone()),
                                _ => n.name.clone().map(processcraft_core::bpmn_xml::MixedXml::Text),
                            });
                        if body.as_ref().is_none_or(|b| b.plain_text().is_some()) {
                            let editor = egui::TextEdit::multiline(&mut self.draft).desired_rows(4);
                            if side
                                .add_enabled(ordinary_enabled, editor)
                                .labelled_by(label.id)
                                .changed()
                            {
                                self.draft_dirty = true;
                            }
                        } else {
                            side.label("Structured annotation content");
                        }
                        advanced_body = Some(body);
                    } else if !association
                        && current_doc.is_some_and(|document| selection_has_name(document, selected))
                    {
                        let name_label = if data_object_reference { "Reference name" } else { "Name" };
                        let label = side.label(small_muted(name_label, palette.muted));
                        // Typing a name sets it; clearing the field removes it.
                        let editor = egui::TextEdit::singleline(&mut self.draft).hint_text("Untitled");
                        if side
                            .add_enabled(ordinary_enabled, editor)
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_name_present = !self.draft.is_empty();
                            self.draft_dirty = true;
                        }
                    }
                    if data_object_reference {
                        side.label(small_muted(
                            "The diagram label uses the referenced data object's name and this reference's state.",
                            palette.muted,
                        ));
                    }
                    if let Some(documentation) = &documentation {
                        if documentation.plain_text().is_some() {
                            let label = side.label(small_muted("Documentation", palette.muted));
                            let docs_changed = side
                                .add_enabled_ui(ordinary_enabled, |side| {
                                    let editor = egui::TextEdit::multiline(&mut self.docs)
                                        .hint_text("Describe this element")
                                        .desired_rows(4);
                                    side.add(editor).labelled_by(label.id)
                                })
                                .inner
                                .changed();
                            if docs_changed {
                                self.docs_dirty = true;
                                self.draft_dirty = true;
                            }
                        } else if let DocumentationSet::Structured(entries) = documentation {
                            side.label(format!(
                                "Documentation · {} entries (edit under Advanced)",
                                entries.len()
                            ));
                        }
                    }
                    if self.is_flow
                        && selected_flow
                            .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Sequence)
                    {
                        self.draft_dirty |= side
                            .add_enabled_ui(ordinary_enabled, |side| {
                                optional_expression_ui_with_default(
                                    side,
                                    "Condition",
                                    &mut self.condition,
                                    true,
                                    &mut self.drafts.expression_draft,
                                    ExpressionDefault::SequenceFlow,
                                )
                            })
                            .inner;
                    }
                    if selected_flow
                        .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Association)
                    {
                        use bpmn_model::AssociationDirection as A;
                        let before = self.association_direction;
                        side.add_enabled_ui(ordinary_enabled, |side| egui::ComboBox::from_label("Association direction")
                            .selected_text(association_direction_label(self.association_direction))
                            .show_ui(side, |ui| {
                                for direction in [None, Some(A::None), Some(A::One), Some(A::Both)] {
                                    ui.selectable_value(
                                        &mut self.association_direction,
                                        direction,
                                        association_direction_label(direction),
                                    );
                                }
                            }));
                        self.draft_dirty |= before != self.association_direction;
                    }
                    if let Some(document) = current_doc
                        && self.graph_selection.is_some()
                    {
                        let owned_scope = scopes
                            .iter()
                            .find(|scope| scope.parent.is_some() && scope.owner == *selected);
                        if let Some(scope) = owned_scope {
                            self.scope_diagram_ui(
                                side,
                                document,
                                &scope.owner,
                                "Work inside sub-choreography",
                                snap.generation,
                            );
                        }
                        self.choreography_flow_ui(side, document, editable);
                    }
                    if self.is_node {
                        self.bpmn_properties_ui(side, editable);
                    }
                    });
                    if self.is_node {
                    widgets::section(side, "Responsibility & systems", None, true, |side| {
                    if self.graph_selection.is_none() {
                        let label =
                            side.label(egui::RichText::new("Owner").small().color(palette.muted));
                        if side
                            .add_enabled(ordinary_enabled,
                                egui::TextEdit::singleline(&mut self.owner)
                                    .hint_text("Optional owner"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                        let label =
                            side.label(egui::RichText::new("Contact").small().color(palette.muted));
                        if side
                            .add_enabled(ordinary_enabled,
                                egui::TextEdit::singleline(&mut self.contact)
                                    .hint_text("Optional contact details"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    }
                    self.dictionary_links_ui(side);
                    });
                    }
                    if self.attributes_section_visible(selected) {
                        widgets::section(side, "Attributes", None, false, |side| {
                            self.attributes_inspector_ui(side, selected, editable)
                        });
                    }
                    widgets::section(side, "Advanced BPMN", None, false, |side| {
                        let xml_id = current_doc
                            .and_then(|document| queries::element_identity(document, selected))
                            .and_then(|identity| identity.xml_id());
                        let xml_id_text = xml_id.map_or_else(
                            || "XML ID · Not assigned".to_owned(),
                            |id| format!("XML ID · {id}"),
                        );
                        side.label(small_muted(xml_id_text, palette.muted));
                        let document_id = snap.project.documents[self.doc].id;
                        let edited = xml_id_editor(
                            side,
                            &mut self.drafts.xml_id_draft,
                            &mut self.ctl,
                            document_id,
                            selected,
                            xml_id,
                            id_edit_ok,
                        );
                        selector_mappings = edited.or_else(|| selector_mappings.take());
                        if let Some(body) = advanced_body.take() {
                            let owner = super::mixed_body::BodyOwner::Annotation(selected.clone());
                            self.mixed_body_ui(side, owner, body, editable);
                        }
                        if let Some(documentation) = &documentation {
                            self.documentation_ui(side, selected.clone(), documentation, editable);
                        }
                        if let Some(metadata) =
                            current_doc.and_then(|document| document.metadata_for_selector(selected))
                        {
                            self.base_metadata_ui(side, selected.clone(), metadata, editable);
                        }
                        if let Some(document) = current_doc {
                            use processcraft_core::bpmn_collaboration::CollaborationArtifact as Artifact;
                            let data_scope = scopes.iter().find(|scope| {
                                scope.data_associations.iter().any(|association| {
                                    association.id.as_ref().is_some_and(|identity| identity == selected)
                                })
                            });
                            if let Some(scope) = data_scope {
                                self.graph_data_ui(side, document, scope, editable, snap.generation);
                            }
                            let artifact_scope = scopes.iter().find(|scope| {
                                scope.root.graph.is_some()
                                    && scope.artifacts.iter().any(|artifact| match artifact {
                                        Artifact::Group(value) => value.id == *selected,
                                        Artifact::TextAnnotation(value) => value.id == *selected,
                                        Artifact::Association(value) => value.id == *selected,
                                    })
                            });
                            if let Some(scope) = artifact_scope {
                                let view = super::collaboration::choreography_view(scope.root);
                                self.collaboration_artifacts_ui(
                                    side,
                                    document,
                                    &view,
                                    snap.generation,
                                    Some(scope),
                                );
                            }
                            if self.graph_selection.is_some() {
                                if let Some((root, owner, _)) = self.graph_selection.clone()
                                    && let Some(scope) = scopes
                                        .iter()
                                        .find(|scope| scope.root.id == root && scope.owner == owner)
                                {
                                    self.graph_data_ui(side, document, scope, editable, snap.generation);
                                }
                                self.correlation_keys_ui(side, document, editable);
                            }
                            if let Some(node) = queries::find_node(document, selected)
                                && node.kind != processcraft_core::model::NodeKind::TextAnnotation
                            {
                                let metadata = node.flow_metadata.clone();
                                self.flow_metadata_ui(side, document, selected.clone(), true, metadata, editable);
                            } else if let Some(flow) = queries::find_flow(document, selected)
                                && flow.kind == processcraft_core::model::FlowKind::Sequence
                            {
                                let metadata = flow.flow_metadata.clone();
                                self.flow_metadata_ui(side, document, selected.clone(), false, metadata, editable);
                            }
                        }
                        self.di_ui(side, editable);
                        if self.is_flow && side.button("Insert activity on selected flow").clicked()
                        {
                            self.splice_selected();
                        }
                        if side
                            .add_enabled(editable, egui::Button::new("Reroute selected connection"))
                            .clicked()
                        {
                            self.reroute_selected();
                        }
                        side.label("Move selected");
                        side.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(editable, egui::Button::new("Left"))
                                .clicked()
                            {
                                self.move_selected(-10.0, 0.0);
                            }
                            if ui.add_enabled(editable, egui::Button::new("Up")).clicked() {
                                self.move_selected(0.0, -10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Down"))
                                .clicked()
                            {
                                self.move_selected(0.0, 10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Right"))
                                .clicked()
                            {
                                self.move_selected(10.0, 0.0);
                            }
                        });
                        if side
                            .add_enabled(editable, egui::Button::new("Delete selected"))
                            .clicked()
                        {
                            self.delete();
                        }
                    });
                } else {
                    let title = current_doc
                        .map_or_else(|| snap.project.name.clone(), |d| d.name.clone());
                    let _ = header_card(side, "Project and document", &title, None, false, false, 0);
                    side.label(small_muted(
                        "Select an element on the canvas to edit its properties.",
                        palette.muted,
                    ));
                    side.add_space(4.0);
                    widgets::section(side, "General", None, true, |side| {
                    let name = side.label(
                        egui::RichText::new("Project name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.project_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    let name = side.label(
                        egui::RichText::new("Document name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.document_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    if let Some(document) = current_doc {
                        let current_process = document
                            .processes
                            .iter()
                            .find(|process| self.process_id.as_ref().is_none_or(|id| process.id == *id));
                        if current_process.is_some() {
                            self.draft_dirty |= name_field(side, "Process name", &mut self.process_name, palette.muted);
                        }
                        if document.collaboration.is_some() {
                            let name = &mut self.collaboration_name;
                            self.draft_dirty |= name_field(side, "Collaboration name", name, palette.muted);
                        }
                        let has_diagram = document
                            .diagrams
                            .iter()
                            .any(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id));
                        if has_diagram {
                            self.draft_dirty |= name_field(side, "Diagram name", &mut self.diagram_name, palette.muted);
                        }
                        if let Some(process) = current_process {
                            let mut executable = process.is_executable;
                            let selected_text = match executable {
                                None => "Unspecified",
                                Some(false) => "No",
                                Some(true) => "Yes",
                            };
                            egui::ComboBox::from_label("Executable")
                                .selected_text(selected_text)
                                .show_ui(side, |ui| {
                                    ui.selectable_value(&mut executable, None, "Unspecified");
                                    ui.selectable_value(&mut executable, Some(false), "No");
                                    ui.selectable_value(&mut executable, Some(true), "Yes");
                                });
                            if executable != process.is_executable && editable && self.commit_draft() {
                                self.ctl.mutate(vec![processcraft_core::commands::Mutation::SetProcessExecutable {
                                    document_id: document.id,
                                    process_id: process.id.selector(),
                                    is_executable: executable,
                                }]);
                            }
                        }
                    }
                    });
                    let attributes_scope = process
                        .as_ref()
                        .filter(|scope| self.attributes_section_visible(&scope.id.selector()));
                    if let Some(scope) = attributes_scope {
                        widgets::section(side, "Attributes", None, false, |side| {
                            self.attributes_inspector_ui(side, &scope.id.selector(), editable)
                        });
                    }
                    if self.take_collaboration_request() {
                        widgets::open_section(side.ctx(), "Data & collaboration");
                    }
                    widgets::section(side, "Data & collaboration", None, false, |side| {
                        self.collaboration_contents(side)
                    });
                }
                // Everything below is interchange detail: XML IDs, namespaces, raw XML and catalogs.
                let catalogs_title = if self.selected.is_some() { "Catalogs" } else { "Advanced BPMN" };
                widgets::section(side, catalogs_title, None, false, |side| {
                if self.selected.is_none() && let Some(document) = current_doc {
                    let anonymous = egui::Checkbox::new(
                        &mut self.create_anonymous,
                        "New canvas elements without XML ID",
                    );
                    side.add_enabled(editable, anonymous)
                        .on_hover_text("Assign an XML ID before exporting referenced elements to BPMN.");
                    let process = document
                        .processes
                        .iter()
                        .find(|process| self.process_id.as_ref().is_none_or(|id| process.id == *id));
                    if let Some(process) = process {
                        side.label(xml_id_label("Process", process.id.xml_id()));
                        let target = process.id.selector();
                        let edited = self.xml_id_row(side, document.id, &target, process.id.xml_id(), id_edit_ok);
                        selector_mappings = edited.or_else(|| selector_mappings.take());
                    }
                    let diagram = document
                        .diagrams
                        .iter()
                        .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id));
                    if let Some(diagram) = diagram {
                        side.label(xml_id_label("Diagram", diagram.id.xml_id()));
                        let target = diagram.id.selector();
                        let edited = self.xml_id_row(side, document.id, &target, diagram.id.xml_id(), id_edit_ok);
                        selector_mappings = edited.or_else(|| selector_mappings.take());
                    }
                    if let Some(collaboration) = &document.collaboration {
                        side.label(xml_id_label("Collaboration", collaboration.id.xml_id()));
                        let target = collaboration.id.selector();
                        let xml_id = collaboration.id.xml_id();
                        let edited = self.xml_id_row(side, document.id, &target, xml_id, id_edit_ok);
                        selector_mappings = edited.or_else(|| selector_mappings.take());
                    }
                    self.definitions_extra_ui(side, document, editable);
                    self.correlation_keys_ui(side, document, editable);
                    self.choreography_flow_ui(side, document, editable);
                    self.participant_multiplicity_ui(side, document, editable);
                    self.message_reference_ui(side, document, editable);
                    side.separator();
                    widgets::section_label(side, "BPMN definitions");
                    side.label(format!("ID · {}", document.definitions_id));
                    let legacy_import = document.definitions.is_none() && document.provenance.is_some();
                    if legacy_import {
                        side.label("Metadata editing is unavailable until the original target namespace is recovered.");
                    } else if document.definitions.is_none() {
                        side.label("The generated target namespace is used until changed.");
                    }
                    side.add_enabled_ui(editable && !legacy_import, |side| {
                        let label = side.label("Target namespace");
                        if side.text_edit_singleline(&mut self.definitions_draft.target_namespace)
                            .labelled_by(label.id).changed() {
                            self.definitions_dirty = true;
                        }
                        fn optional_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
                            let mut present = value.is_some();
                            let checkbox = ui.checkbox(&mut present, label);
                            let mut changed = checkbox.changed();
                            if present {
                                let text = value.get_or_insert_with(String::new);
                                changed |= ui.text_edit_singleline(text).labelled_by(checkbox.id).changed();
                            } else if value.take().is_some() {
                                changed = true;
                            }
                            changed
                        }
                        let draft = &mut self.definitions_draft;
                        self.definitions_dirty |= optional_field(side, "Name", &mut draft.name);
                        self.definitions_dirty |=
                            optional_field(side, "Expression language", &mut draft.expression_language);
                        self.definitions_dirty |=
                            optional_field(side, "Type language", &mut draft.type_language);
                        self.definitions_dirty |= optional_field(side, "Exporter", &mut draft.exporter);
                        self.definitions_dirty |=
                            optional_field(side, "Exporter version", &mut draft.exporter_version);
                        egui::CollapsingHeader::new("Imports").show(side, |side| {
                            side.label("Import declarations are stored as model data.");
                            let mut remove = None;
                            for (index, import) in self.definitions_draft.imports.iter_mut().enumerate() {
                                side.push_id(index, |side| {
                                    side.separator();
                                    side.label(format!("Import {}", index + 1));
                                    let label = side.label("Namespace");
                                    let edit = side.text_edit_singleline(&mut import.namespace);
                                    self.definitions_dirty |= edit.labelled_by(label.id).changed();
                                    let label = side.label("Location");
                                    let edit = side.text_edit_singleline(&mut import.location);
                                    self.definitions_dirty |= edit.labelled_by(label.id).changed();
                                    let label = side.label("Import type");
                                    let edit = side.text_edit_singleline(&mut import.import_type);
                                    self.definitions_dirty |= edit.labelled_by(label.id).changed();
                                    if side.button("Remove import").clicked() {
                                        remove = Some(index);
                                    }
                                });
                            }
                            if let Some(index) = remove {
                                self.definitions_draft.imports.remove(index);
                                self.definitions_dirty = true;
                            }
                            let has_room = self.definitions_draft.imports.len() < 256;
                            if side.add_enabled(has_room, egui::Button::new("Add import")).clicked() {
                                self.definitions_draft.imports.push(processcraft_core::model::DefinitionsImport {
                                    namespace: String::new(),
                                    location: String::new(),
                                    import_type: String::new(),
                                });
                                self.definitions_dirty = true;
                            }
                        });
                    });
                    self.draft_dirty |= self.definitions_dirty;
                    let diagram = document
                        .diagrams
                        .iter()
                        .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id));
                    if let Some(diagram) = diagram {
                        self.diagram_styles_ui(side, diagram, editable);
                        if !document.choreographies.is_empty() {
                            self.band_create_ui(side, document, diagram, editable);
                        }
                    }
                    let process = document
                        .processes
                        .iter()
                        .find(|process| self.process_id.as_ref().is_none_or(|id| process.id == *id));
                    if let Some(process) = process {
                        self.process_activity_ui(side, process, document, editable);
                        let process_documentation = process.documentation.clone();
                        let process_selector = process.id.selector();
                        self.documentation_ui(side, process_selector.clone(), &process_documentation, editable);
                        if let Some(metadata) = document.metadata_for_selector(&process_selector) {
                            self.base_metadata_ui(side, process_selector, metadata, editable);
                        }
                    }
                    side.collapsing("Catalog and data XML IDs", |ui| {
                        for (label, path, identity) in optional_identity_rows(document) {
                            ui.push_id(&path, |ui| {
                                ui.label(label);
                                let enabled = id_edit_ok;
                                let mappings = if let Some(identity) = &identity {
                                    ui.label(xml_id_text(identity.xml_id()));
                                    self.xml_id_row(ui, document.id, &identity.selector(), identity.xml_id(), enabled)
                                } else {
                                    ui.label("XML ID · Not assigned");
                                    let draft = &mut self.drafts.xml_id_draft;
                                    anonymous_xml_id_editor(ui, draft, &mut self.ctl, document.id, &path, enabled)
                                };
                                selector_mappings = mappings.or_else(|| selector_mappings.take());
                                if let Some(identity) = &identity
                                    && let Some(metadata) = document.metadata_for_selector(&identity.selector())
                                {
                                    self.base_metadata_ui(ui, identity.selector(), metadata, editable);
                                }
                            });
                        }
                    });
                    // Built only when expanded: the full identity list is costly on large models.
                    side.collapsing("All element XML IDs", |ui| {
                        let Some(rows) = queries::identified_elements(&snap.project, document.id) else {
                            return;
                        };
                        let pages = rows.len().div_ceil(50).max(1);
                        self.identity_page = self.identity_page.min(pages - 1);
                        ui.horizontal(|ui| {
                            let no_draft = self.drafts.xml_id_draft.is_none();
                            let previous = egui::Button::new("Previous IDs");
                            if ui.add_enabled(no_draft && self.identity_page > 0, previous).clicked() {
                                self.identity_page -= 1;
                            }
                            ui.label(format!("Page {} of {}", self.identity_page + 1, pages));
                            let next = egui::Button::new("Next IDs");
                            if ui.add_enabled(no_draft && self.identity_page + 1 < pages, next).clicked() {
                                self.identity_page += 1;
                            }
                        });
                        for (index, row) in rows.iter().enumerate().skip(self.identity_page * 50).take(50) {
                            ui.push_id(("all_ids", index), |ui| {
                                let name = queries::element_name(document, &row.selector)
                                    .filter(|name| !name.is_empty());
                                ui.label(name.map_or_else(|| format!("Element {}", index + 1), str::to_owned));
                                ui.label(xml_id_text(row.xml_id.as_ref()));
                                let xml_id = row.xml_id.as_ref();
                                let edited = self.xml_id_row(ui, document.id, &row.selector, xml_id, id_edit_ok);
                                selector_mappings = edited.or_else(|| selector_mappings.take());
                            });
                        }
                    });
                }
                self.catalog_ui(side, editable);
                self.data_ui(side, editable);
                self.event_catalog_ui(side, editable);
                self.correlation_catalog_ui(side, editable);
                self.callable_catalog_ui(side, editable);
                });
                if let Some(p) = process {
                    widgets::section(side, "Connections and lanes", None, false, |side| {
                        let lane_names: Vec<_> = p
                            .lanes
                            .iter()
                            .map(|l| l.name.as_deref().unwrap_or("Unnamed lane"))
                            .collect();
                        egui::ComboBox::from_label("Assign selected node to lane")
                            .selected_text(
                                self.lane
                                    .and_then(|i| lane_names.get(i).copied())
                                    .unwrap_or("No lane"),
                            )
                            .show_ui(side, |ui| {
                                ui.selectable_value(&mut self.lane, None, "No lane");
                                for (i, n) in lane_names.iter().enumerate() {
                                    ui.selectable_value(&mut self.lane, Some(i), *n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Apply lane"))
                            .clicked()
                        {
                            self.assign_lane();
                        }
                        side.strong("Connect nodes");
                        side.label("The connection type follows the BPMN rules for the two elements.");
                        let names: Vec<_> = p
                            .nodes
                            .iter()
                            .map(|n| {
                                let name = n.name.as_deref().unwrap_or("Unnamed element");
                                (Some(n.id.selector()), name)
                            })
                            .collect();
                        let chosen = |id: &Option<processcraft_core::ids::ElementSelector>| {
                            names
                                .iter()
                                .find(|(n, _)| n == id)
                                .map_or("Choose", |(_, name)| *name)
                        };
                        egui::ComboBox::from_label("Source")
                            .selected_text(chosen(&self.source))
                            .show_ui(side, |ui| {
                                for (id, n) in &names {
                                    ui.selectable_value(&mut self.source, id.clone(), *n);
                                }
                            });
                        egui::ComboBox::from_label("Target")
                            .selected_text(chosen(&self.target))
                            .show_ui(side, |ui| {
                                for (id, n) in &names {
                                    ui.selectable_value(&mut self.target, id.clone(), *n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Create connection"))
                            .clicked()
                        {
                            self.connect();
                        }
                    });
                }
            });
        // Simple fields apply as soon as editing ends (Enter, Tab away, click elsewhere) or a
        // choice changes; there is no separate Apply step.
        if self.draft_dirty && !ui.ctx().text_edit_focused() {
            self.auto_commit(ui.ctx());
        }
        if let Some(mappings) = selector_mappings {
            self.remap_context(&mappings);
            self.restore_selection();
        }
    }
    /// Shows the XML ID editor for `target`; returns the selector remaps of a committed edit.
    fn xml_id_row(
        &mut self,
        ui: &mut egui::Ui,
        document_id: processcraft_core::ids::DocumentId,
        target: &processcraft_core::ids::ElementSelector,
        xml_id: Option<&processcraft_core::ids::XmlId>,
        enabled: bool,
    ) -> Option<Vec<processcraft_core::commands::SelectorMapping>> {
        let draft = &mut self.drafts.xml_id_draft;
        xml_id_editor(
            ui,
            draft,
            &mut self.ctl,
            document_id,
            target,
            xml_id,
            enabled,
        )
    }
    /// Applies the pending simple-field draft once; a draft that was refused (its message is
    /// shown) is not retried until it changes.
    fn auto_commit(&mut self, ctx: &egui::Context) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!(
            "{:?}",
            (
                (
                    &self.selected,
                    &self.draft,
                    self.draft_name_present,
                    &self.docs
                ),
                (
                    &self.owner,
                    &self.contact,
                    &self.condition,
                    &self.association_direction
                ),
                (&self.bpmn_details, &self.bpmn_loop, &self.bpmn_activity),
                (&self.project_name, &self.document_name, &self.process_name),
                (
                    &self.collaboration_name,
                    &self.diagram_name,
                    &self.definitions_draft
                ),
                self.ctl.session.snapshot().generation,
            )
        )
        .hash(&mut hasher);
        let signature = hasher.finish();
        let id = egui::Id::unique("inspector_refused_draft");
        if ctx.data(|data| data.get_temp::<u64>(id)) == Some(signature) {
            return;
        }
        if !self.commit_draft() {
            ctx.data_mut(|data| data.insert_temp(id, signature));
        }
    }
}

impl ProcessCraft {
    fn bpmn_properties_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        use bpmn_model::NodeDetails as D;
        use processcraft_core::{model::NodeKind as K, queries};
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let Some(node) = self
            .selected
            .as_ref()
            .and_then(|id| queries::find_node(document, id).or_else(|| graph_node(document, id)))
        else {
            return;
        };
        if let Some(D::SubProcess { .. }) = &node.details {
            ui.separator();
            if ui.button("Work inside subprocess").clicked() && self.prepare_navigation() {
                self.process_id = Some(node.id.selector());
                self.select(None);
            }
        }
        let event = matches!(
            node.kind,
            K::StartEvent
                | K::EndEvent
                | K::IntermediateCatchEvent
                | K::IntermediateThrowEvent
                | K::BoundaryEvent
        );
        if node.kind.is_activity() {
            self.activity_properties_ui(ui, node, document, editable);
            ui.add_enabled_ui(editable && !self.ordinary_property_locked(), |ui| {
                ui.collapsing("Loop characteristics", |ui| {
                    use bpmn_model::LoopCharacteristics as L;
                    let mut value = self.bpmn_loop.clone();
                    let mut tag = match &value {
                        None => "None",
                        Some(L::Standard { .. }) => "Standard loop",
                        Some(L::MultiInstance {
                            is_sequential: Some(true),
                            ..
                        }) => "Sequential instances",
                        Some(L::MultiInstance { .. }) => "Parallel instances",
                    };
                    let old_tag = tag;
                    ui.add_enabled_ui(self.drafts.expression_draft.is_none(), |ui| {
                        egui::ComboBox::from_label("Repetition")
                            .selected_text(tag)
                            .show_ui(ui, |ui| {
                                for option in [
                                    "None",
                                    "Standard loop",
                                    "Sequential instances",
                                    "Parallel instances",
                                ] {
                                    ui.selectable_value(&mut tag, option, option);
                                }
                            });
                    });
                    let extended_loop = is_extended_loop(&value);
                    if tag != old_tag && extended_loop && matches!(tag, "None" | "Standard loop") {
                        ui.label("Clear multi-instance details before changing the repetition type.");
                    } else if tag != old_tag && tag == "None" && has_loop_metadata(&value) {
                        ui.label(
                            "This loop has additional metadata. Remove that metadata before changing the loop type.",
                        );
                    } else if tag != old_tag {
                        let (old_id, old_base) = match &value {
                            Some(L::Standard { id, base, .. } | L::MultiInstance { id, base, .. }) => {
                                (id.clone(), base.clone())
                            }
                            None => (None, None),
                        };
                        value = match tag {
                            "None" => None,
                            "Standard loop" => Some(L::Standard {
                                id: old_id.clone(),
                                base: old_base.clone(),
                                test_before: None,
                                condition: None,
                                maximum: None,
                            }),
                            _ if matches!(value, Some(L::MultiInstance { .. })) => {
                                if let Some(L::MultiInstance { is_sequential, .. }) = &mut value {
                                    *is_sequential = Some(tag == "Sequential instances");
                                }
                                value
                            }
                            _ => Some(L::MultiInstance {
                                id: old_id,
                                base: old_base,
                                is_sequential: Some(tag == "Sequential instances"),
                                behavior: None,
                                loop_data_input_ref: None,
                                loop_data_output_ref: None,
                                input_data_item: None,
                                output_data_item: None,
                                complex_behavior_definitions: Vec::new(),
                                one_behavior_event_ref: None,
                                none_behavior_event_ref: None,
                                cardinality: None,
                                completion_condition: None,
                            }),
                        };
                    }
                    let text = |ui: &mut egui::Ui, label: &str, value: &mut Option<String>| {
                        let mut current = value.clone().unwrap_or_default();
                        let label = ui.label(label);
                        if ui
                            .text_edit_singleline(&mut current)
                            .labelled_by(label.id)
                            .changed()
                        {
                            *value = (!current.is_empty()).then_some(current);
                        }
                    };
                    match &mut value {
                        Some(L::Standard {
                            test_before,
                            condition,
                            maximum,
                            ..
                        }) => {
                            super::catalog::optional_bool(ui, "Test before first iteration", test_before);
                            optional_expression_ui(
                                ui,
                                "Loop condition",
                                condition,
                                false,
                                &mut self.drafts.expression_draft,
                            );
                            text(ui, "Maximum iterations", maximum);
                        }
                        Some(L::MultiInstance {
                            is_sequential,
                            cardinality,
                            completion_condition,
                            ..
                        }) => {
                            super::catalog::optional_bool(ui, "Sequential instances", is_sequential);
                            optional_expression_ui(
                                ui,
                                "Cardinality expression",
                                cardinality,
                                false,
                                &mut self.drafts.expression_draft,
                            );
                            optional_expression_ui(
                                ui,
                                "Completion condition",
                                completion_condition,
                                false,
                                &mut self.drafts.expression_draft,
                            );
                        }
                        None => {}
                    }
                    if value != self.bpmn_loop {
                        self.bpmn_loop = value;
                        self.draft_dirty = true;
                    }
                });
            });
        }
        if matches!(
            node.kind,
            K::ExclusiveGateway
                | K::InclusiveGateway
                | K::ParallelGateway
                | K::ComplexGateway
                | K::EventBasedGateway
        ) {
            ui.add_enabled_ui(editable && !self.ordinary_property_locked(), |ui| {
                use bpmn_model::{EventGatewayType as T, GatewayDirection as G};
                let mut value = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if let D::Gateway {
                    default_flow,
                    direction,
                    instantiate,
                    event_gateway_type,
                    activation_condition,
                } = &mut value
                {
                    ui.separator();
                    ui.strong("Gateway behavior");
                    egui::ComboBox::from_label("Direction")
                        .selected_text(direction.map_or("Unspecified (omitted)", gateway_direction_label))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(direction, None, "Unspecified (omitted)");
                            for v in [G::Unspecified, G::Converging, G::Diverging, G::Mixed] {
                                ui.selectable_value(direction, Some(v), gateway_direction_label(v));
                            }
                        });
                    if matches!(
                        node.kind,
                        K::ExclusiveGateway | K::InclusiveGateway | K::ComplexGateway
                    ) {
                        let choreography_flows = queries::choreography_scopes(document)
                            .into_iter()
                            .filter(|scope| scope.gateways.iter().any(|gateway| gateway.id == node.id))
                            .flat_map(|scope| scope.flows);
                        let outgoing: Vec<_> = queries::scopes(document)
                            .into_iter()
                            .flat_map(|s| s.flows)
                            .chain(choreography_flows)
                            .filter(|f| {
                                let from_node = matches!(
                                    &f.source,
                                    Some(processcraft_core::bpmn_activity::ModelReference::Local(id))
                                        if node.id == *id
                                );
                                from_node && f.kind == processcraft_core::model::FlowKind::Sequence
                            })
                            .collect();
                        egui::ComboBox::from_label("Default outgoing flow")
                            .selected_text(
                                default_flow
                                    .as_ref()
                                    .map_or("None".into(), |id| flow_label(document, id)),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(default_flow, None, "None");
                                for flow in outgoing {
                                    ui.selectable_value(
                                        default_flow,
                                        Some(flow.id.selector()),
                                        flow_label(document, &flow.id.selector()),
                                    );
                                }
                            });
                    }
                    if node.kind == K::EventBasedGateway {
                        let mut starts = instantiate.unwrap_or(false);
                        if ui.checkbox(&mut starts, "Instantiate process").changed() {
                            *instantiate = Some(starts);
                        }
                        egui::ComboBox::from_label("Event gateway type")
                            .selected_text(
                                event_gateway_type.map_or("Exclusive (default)", |v| match v {
                                    T::Exclusive => "Exclusive",
                                    T::Parallel => "Parallel",
                                }),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    event_gateway_type,
                                    None,
                                    "Exclusive (default)",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Exclusive),
                                    "Exclusive",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Parallel),
                                    "Parallel",
                                );
                            });
                    }
                    if node.kind == K::ComplexGateway {
                        optional_expression_ui(
                            ui,
                            "Activation condition",
                            activation_condition,
                            true,
                            &mut self.drafts.expression_draft,
                        );
                    }
                }
                let original = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if value != original {
                    self.bpmn_details = Some(value);
                    self.draft_dirty = true;
                }
            });
        }
        if event {
            self.event_editor_ui(ui, editable);
        }
    }
}

impl ProcessCraft {
    fn activity_properties_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &processcraft_core::model::BpmnDocument,
        editable: bool,
    ) {
        use super::catalog::{optional_bool as checkbox, optional_text as text};
        use processcraft_core::{
            bpmn_activity::AdHocOrdering,
            model::{FlowKind, NodeKind as K},
        };
        ui.add_enabled_ui(editable && !self.ordinary_property_locked(), |ui| {
            ui.collapsing("Activity behavior", |ui| {
                let mut value = self.bpmn_activity.clone().unwrap_or_default();
                checkbox(ui, "For compensation", &mut value.is_for_compensation);
                text(ui, "Start quantity (default 1)", &mut value.start_quantity);
                text(ui, "Completion quantity (default 1)", &mut value.completion_quantity);
                let outgoing: Vec<_> = queries::scopes(document)
                    .into_iter()
                    .flat_map(|s| s.flows)
                    .filter(|f| {
                        let from_node = matches!(
                            &f.source,
                            Some(processcraft_core::bpmn_activity::ModelReference::Local(id))
                                if node.id == *id
                        );
                        f.kind == FlowKind::Sequence && from_node
                    })
                    .collect();
                let default_flow = value
                    .default_flow
                    .as_ref()
                    .map_or("None".into(), |id| flow_label(document, id));
                egui::ComboBox::from_label("Activity default flow")
                    .selected_text(default_flow)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut value.default_flow, None, "None");
                        for flow in outgoing {
                            let id = flow.id.selector();
                            ui.selectable_value(&mut value.default_flow, Some(id.clone()), flow_label(document, &id));
                        }
                    });
                if node.kind == K::SubProcess {
                    checkbox(ui, "Triggered by event", &mut value.triggered_by_event);
                }
                if node.kind == K::AdHocSubProcess {
                    let ordering = value.ad_hoc_ordering.map_or("Parallel (default)", |v| match v {
                        AdHocOrdering::Parallel => "Parallel",
                        AdHocOrdering::Sequential => "Sequential",
                    });
                    egui::ComboBox::from_label("Ad-hoc ordering")
                        .selected_text(ordering)
                        .show_ui(ui, |ui| {
                            let slot = &mut value.ad_hoc_ordering;
                            ui.selectable_value(slot, None, "Parallel (default)");
                            ui.selectable_value(slot, Some(AdHocOrdering::Parallel), "Parallel");
                            ui.selectable_value(slot, Some(AdHocOrdering::Sequential), "Sequential");
                        });
                    checkbox(ui, "Cancel remaining instances", &mut value.cancel_remaining_instances);
                    optional_expression_ui(
                        ui,
                        "Ad-hoc completion condition",
                        &mut value.completion_condition,
                        true,
                        &mut self.drafts.expression_draft,
                    );
                }
                if node.kind == K::Transaction {
                    text(ui, "Transaction method (default ##Compensate)", &mut value.transaction_method);
                }
                if matches!(
                    node.kind,
                    K::ServiceTask | K::SendTask | K::ReceiveTask | K::UserTask | K::BusinessRuleTask
                ) {
                    text(ui, "Implementation", &mut value.implementation);
                }
                if matches!(node.kind, K::ServiceTask | K::SendTask | K::ReceiveTask) {
                    if self.drafts.binding_draft.is_none() {
                        let edit_label = if node.kind == K::ServiceTask {
                            "Edit operation reference…"
                        } else {
                            "Edit message and operation references…"
                        };
                        if ui.button(edit_label).clicked() {
                            self.drafts.binding_draft = Some(BindingDraft::new(&value));
                        }
                    } else if let Some(draft) = &mut self.drafts.binding_draft {
                        if matches!(node.kind, K::SendTask | K::ReceiveTask) {
                            super::catalog::optional_qname(ui, "Message reference", &mut draft.message);
                        }
                        super::catalog::optional_qname(ui, "Operation reference", &mut draft.operation);
                        if !draft.error.is_empty() {
                            ui.label(&draft.error);
                        }
                        let mut clear = false;
                        if ui.button("Apply activity references").clicked() {
                            let message = super::catalog::q_build(&draft.message);
                            let operation = super::catalog::q_build(&draft.operation);
                            match (message, operation) {
                                (Ok(message), Ok(operation)) => {
                                    value.message_ref = message;
                                    value.operation_ref = operation;
                                    clear = true;
                                }
                                (Err(error), _) | (_, Err(error)) => draft.error = error,
                            }
                        }
                        if ui.button("Cancel activity references").clicked() {
                            clear = true;
                        }
                        if clear {
                            self.drafts.binding_draft = None;
                        }
                    }
                }
                if node.kind == K::ReceiveTask {
                    checkbox(ui, "Instantiate process", &mut value.instantiate);
                }
                if node.kind == K::ScriptTask {
                    ui.label("Script source is stored as model data and is never executed by ProcessCraft.");
                    text(ui, "Script format", &mut value.script_format);
                    if let Some(script) = value.script.as_ref().and_then(|s| s.plain_text()) {
                        ui.label(script);
                    } else if value.script.is_some() {
                        ui.label("Structured script content");
                    }
                }
                let original = self.bpmn_activity.clone().unwrap_or_default();
                if value != original {
                    self.bpmn_activity = Some(value);
                    self.draft_dirty = true;
                }
            });
        });
        self.activity_roles_ui(ui, node, document, editable);
        self.loop_advanced_ui(ui, node, document, editable);
        if node.kind == K::CallActivity {
            self.call_ui(ui, node, document, editable);
        }
        if node.kind == K::ScriptTask {
            let body = node.activity.as_ref().and_then(|a| a.script.clone());
            self.mixed_body_ui(
                ui,
                super::mixed_body::BodyOwner::Script(node.id.selector()),
                body,
                editable,
            );
        }
    }
}

enum HeaderAction {
    Replace,
    Comments,
}

/// The inspector's header card: the element's icon, type and name, with Change type and
/// Comments actions.
fn header_card(
    ui: &mut egui::Ui,
    kind: &str,
    title: &str,
    node_kind: Option<processcraft_core::model::NodeKind>,
    flow: bool,
    replaceable: bool,
    comments: usize,
) -> Option<HeaderAction> {
    let p = crate::theme::current(ui);
    let mut action = None;
    egui::Frame::new()
        .fill(p.secondary)
        .corner_radius(crate::theme::RADIUS_CARD)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(36.0), egui::Sense::hover());
                ui.painter().rect_filled(icon, crate::theme::RADIUS, p.base);
                let glyph = egui::Rect::from_center_size(icon.center(), egui::Vec2::splat(22.0));
                if let Some(kind) = node_kind {
                    super::overlays::paint_entry_icon(ui.painter(), kind.into(), glyph, p.text);
                } else if flow {
                    ui.painter().arrow(
                        glyph.left_bottom() + egui::vec2(3.0, -3.0),
                        egui::vec2(15.0, -15.0),
                        egui::Stroke::new(1.4, p.text),
                    );
                } else {
                    widgets::paint_icon(ui, widgets::Icon::Workflow, glyph.shrink(2.0), p.text);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        egui::RichText::new(kind.to_uppercase())
                            .font(crate::theme::semibold(10.5))
                            .color(p.faint),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(title).font(crate::theme::semibold(16.0)),
                        )
                        .truncate(),
                    )
                    .on_hover_text(title);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if node_kind.is_some() || flow {
                        let label = if comments > 0 {
                            format!("Comments ({comments})")
                        } else {
                            "Comments".to_owned()
                        };
                        if widgets::icon_button_sized(
                            ui,
                            widgets::Icon::Comment,
                            &label,
                            comments > 0,
                            28.0,
                        )
                        .clicked()
                        {
                            action = Some(HeaderAction::Comments);
                        }
                    }
                    if replaceable
                        && widgets::icon_button_sized(
                            ui,
                            widgets::Icon::Redo,
                            "Change type… (R)",
                            false,
                            28.0,
                        )
                        .clicked()
                    {
                        action = Some(HeaderAction::Replace);
                    }
                });
            });
        });
    ui.add_space(6.0);
    action
}

#[cfg(test)]
mod expression_draft_tests {
    use super::*;
    use processcraft_core::model::NodeKind;

    #[test]
    fn legacy_promotion_preserves_flow_and_event_expression_defaults() {
        use bpmn_model::ExpressionKind;
        let flow = ExpressionDraft::new("Condition", &"x".into(), ExpressionDefault::SequenceFlow)
            .build()
            .unwrap();
        assert!(matches!(
            flow.kind,
            ExpressionKind::Formal {
                explicit_type: true,
                ..
            }
        ));
        assert!(
            flow.namespaces
                .iter()
                .any(|(p, n)| p.is_empty() && n == "http://www.omg.org/spec/BPMN/20100524/MODEL")
        );
        assert!(
            flow.namespaces
                .iter()
                .any(|(p, n)| p == "xsi" && n == "http://www.w3.org/2001/XMLSchema-instance")
        );
        let event =
            ExpressionDraft::new("Condition expression", &"x".into(), ExpressionDefault::Base)
                .build()
                .unwrap();
        assert!(matches!(
            event.kind,
            ExpressionKind::Base {
                explicit_type: false
            }
        ));
    }

    #[test]
    fn transformation_promotes_as_implicit_formal() {
        use bpmn_model::{Expression, ExpressionData, ExpressionKind};
        let draft = ExpressionDraft::new(
            "Transformation",
            &"body".into(),
            ExpressionDefault::ImplicitFormal,
        );
        assert!(matches!(
            draft.build().unwrap().kind,
            ExpressionKind::Formal {
                explicit_type: false,
                ..
            }
        ));
        let data = ExpressionData {
            id: None,
            kind: ExpressionKind::Formal {
                explicit_type: false,
                language: None,
                evaluates_to_type_ref: None,
            },
            text: "body".into(),
            namespaces: vec![(
                String::new(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            )],
            foreign_attributes: Vec::new(),
        };
        assert_eq!(
            ExpressionDraft::new(
                "Transformation",
                &Expression::Structured(data.clone()),
                ExpressionDefault::ImplicitFormal
            )
            .build()
            .unwrap(),
            data
        );
    }

    #[test]
    fn expression_metadata_draft_round_trips_and_validates() {
        use bpmn_model::{Expression, ExpressionData, ExpressionKind, QNameRef};
        let data = ExpressionData {
            id: Some("ExpressionId".parse().unwrap()),
            kind: ExpressionKind::Formal {
                explicit_type: true,
                language: Some(String::new()),
                evaluates_to_type_ref: Some(QNameRef {
                    namespace: "urn:type".into(),
                    local: "Thing".parse().unwrap(),
                }),
            },
            text: "body".into(),
            namespaces: vec![("t".into(), "urn:type".into())],
            foreign_attributes: Vec::new(),
        };
        let mut draft = ExpressionDraft::new(
            "Condition",
            &Expression::Structured(data.clone()),
            ExpressionDefault::Base,
        );
        assert_eq!(draft.build().unwrap(), data);
        draft.type_reference.as_mut().unwrap().1 = "invalid name".into();
        assert!(draft.build().is_err());
        draft.formal = false;
        assert!(draft.build().is_ok());
    }

    #[test]
    fn expression_id_edit_and_null_promotion_preserve_identity() {
        use bpmn_model::{EventDefinition, Expression, NodeDetails, TimerKind};
        use processcraft_core::commands::Mutation;
        let mut fresh = ExpressionDraft::new(
            "Timer",
            &Expression::Legacy("PT1M".into()),
            ExpressionDefault::Base,
        );
        fresh.id.value = Some("ExpressionId".into());
        let created = fresh.build().unwrap();
        assert_eq!(
            created.id.as_ref().unwrap().xml_id().unwrap().to_string(),
            "ExpressionId"
        );
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document_id = snapshot.project.documents[0].id;
        let mut data = created;
        data.id = None;
        let details = NodeDetails::Event {
            definitions: vec![EventDefinition::Timer {
                base: None,
                id: None,
                kind: Some(TimerKind::Duration),
                expression: Some(Expression::Structured(data)),
            }],
            attached_to: None,
            cancel_activity: None,
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
        };
        assert!(app.ctl.mutate(vec![Mutation::SetNodeDetails {
            document_id,
            element_id: selected.clone(),
            details: Some(details)
        }]));
        let snapshot = app.ctl.session.snapshot();
        let rows = optional_identity_rows(&snapshot.project.documents[0]);
        let path = rows
            .iter()
            .find(|(_, path, identity)| path.ends_with("/expression/id") && identity.is_none())
            .unwrap()
            .1
            .clone();
        let changes = app
            .ctl
            .mutate_changes(vec![Mutation::PromoteElementIdentity {
                document_id,
                model_path: path,
                xml_id: Some("PromotedExpression".parse().unwrap()),
            }])
            .unwrap();
        let selector = changes.promoted_identities[0].selector.clone();
        let snapshot = app.ctl.session.snapshot();
        let identity =
            queries::element_identity(&snapshot.project.documents[0], &selector).unwrap();
        assert_eq!(identity.xml_id().unwrap().to_string(), "PromotedExpression");
        app.history(processcraft_core::commands::Request::Undo);
        assert!(
            optional_identity_rows(&app.ctl.session.snapshot().project.documents[0])
                .iter()
                .any(|(_, path, identity)| path.ends_with("/expression/id") && identity.is_none())
        );
    }

    #[test]
    fn event_singleton_sets_are_listed_for_identity_promotion() {
        use bpmn_model::{EventIo, NodeDetails};
        use processcraft_core::bpmn_data::IoSet;
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone().unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document_id = snapshot.project.documents[0].id;
        let mut details = queries::find_node(&snapshot.project.documents[0], &selected)
            .unwrap()
            .details
            .clone()
            .unwrap();
        if let NodeDetails::Event { io, .. } = &mut details {
            *io = Some(EventIo::Catch {
                outputs: Vec::new(),
                output_set: Some(IoSet::default()),
            });
        }
        assert!(app.ctl.mutate(vec![
            processcraft_core::commands::Mutation::SetNodeDetails {
                document_id,
                element_id: selected,
                details: Some(details)
            }
        ]));
        let snapshot = app.ctl.session.snapshot();
        assert!(
            optional_identity_rows(&snapshot.project.documents[0])
                .iter()
                .any(|(_, path, identity)| path.ends_with("/output_set/id") && identity.is_none())
        );
    }

    #[test]
    fn anonymous_data_id_listing_is_read_only_until_explicit_promotion() {
        use processcraft_core::bpmn_data::DataStore;
        use processcraft_core::commands::Mutation;
        let mut project = crate::controller::blank_project();
        std::sync::Arc::make_mut(&mut project.documents[0])
            .data_stores
            .push(DataStore {
                base: None,
                id: None,
                name: Some("Stock".into()),
                capacity: None,
                is_unlimited: None,
                item_aware: None,
            });
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let row = optional_identity_rows(document)
            .into_iter()
            .find(|(_, path, _)| path == "/data_stores/0/id")
            .unwrap();
        assert!(row.2.is_none());
        assert_eq!(app.ctl.session.snapshot().generation, snapshot.generation);
        let changes = app
            .ctl
            .mutate_changes(vec![Mutation::PromoteElementIdentity {
                document_id: document.id,
                model_path: row.1,
                xml_id: Some("StockId".parse().unwrap()),
            }])
            .unwrap();
        assert_eq!(changes.promoted_identities.len(), 1);
        let snapshot = app.ctl.session.snapshot();
        let identity = snapshot.project.documents[0].data_stores[0]
            .id
            .as_ref()
            .unwrap();
        assert!(matches!(
            identity,
            processcraft_core::ids::ElementIdentity::Native { .. }
        ));
        assert_eq!(identity.xml_id().unwrap().to_string(), "StockId");
        app.history(processcraft_core::commands::Request::Undo);
        assert!(
            app.ctl.session.snapshot().project.documents[0].data_stores[0]
                .id
                .is_none()
        );
    }

    #[test]
    fn activity_reference_draft_retains_invalid_local_name() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::SendTask);
        let selected = app.selected.clone();
        let mut draft = BindingDraft::new(&Default::default());
        draft.message = Some(super::super::catalog::QNameDraft {
            namespace: "urn:messages".into(),
            local: "bad name".into(),
        });
        assert!(super::super::catalog::q_build(&draft.message).is_err());
        app.drafts.binding_draft = Some(draft);
        app.draft_dirty = true;
        app.select(None);
        app.history(processcraft_core::commands::Request::Undo);
        assert_eq!(app.selected, selected);
        assert_eq!(
            app.drafts
                .binding_draft
                .as_ref()
                .unwrap()
                .message
                .as_ref()
                .unwrap()
                .local,
            "bad name"
        );
    }

    #[test]
    fn invalid_expression_metadata_blocks_navigation_and_history() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone();
        let mut metadata =
            ExpressionDraft::new("Expression", &"PT1M".into(), ExpressionDefault::Base);
        metadata.id.value = Some("invalid id".into());
        assert!(metadata.build().is_err());
        app.drafts.expression_draft = Some(metadata);
        app.draft_dirty = true;
        assert!(!app.commit_draft());
        app.select(None);
        app.history(processcraft_core::commands::Request::Undo);
        assert_eq!(app.selected, selected);
        assert_eq!(
            app.drafts
                .expression_draft
                .as_ref()
                .unwrap()
                .id
                .value
                .as_deref(),
            Some("invalid id")
        );
        assert!(app.has_drafts());
    }
}
