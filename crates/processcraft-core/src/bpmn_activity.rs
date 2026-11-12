//! Typed BPMN activity attributes. Values are model data, never executed.
use crate::bpmn_model::QNameRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum AdHocOrdering {
    Parallel,
    Sequential,
}

/// Optional values preserve the distinction between an omitted attribute and an explicit default.
/// Quantities retain the full lexical range of xs:integer; expressions and scripts are plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActivityProperties {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<crate::bpmn_model::EventProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_roles: Vec<ResourceRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renderings: Vec<BaseOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_for_compensation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_flow: Option<crate::ids::ElementSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_by_event: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ad_hoc_ordering: Option<AdHocOrdering>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_remaining_instances: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_condition: Option<crate::bpmn_model::Expression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<QNameRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_ref: Option<QNameRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instantiate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<crate::bpmn_xml::MixedXml>,
}

// Activity, callable and correlation data are inert model values.
pub type ModelReference = crate::bpmn_model::EventReference;
use crate::{
    bpmn_data::{DataPort, IoSpecification},
    bpmn_metadata::BaseMetadata,
    bpmn_model::{EventDefinition, Expression},
    bpmn_xml::MixedXml,
    ids::{ElementIdentity, ElementSelector},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaseOwner {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
}
pub type AuditMonitor = BaseOwner;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelationProperty {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub item_type: Option<QNameRef>,
    pub retrieval_expressions: Vec<CorrelationRetrieval>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelationRetrieval {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub message_ref: Option<ModelReference>,
    pub message_path: Option<Expression>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelationKey {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub property_refs: Vec<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelationSubscription {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub key_ref: Option<ModelReference>,
    pub bindings: Vec<CorrelationPropertyBinding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelationPropertyBinding {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub property_ref: Option<ModelReference>,
    pub data_path: Option<Expression>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowElementMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incoming: Vec<ModelReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outgoing: Vec<ModelReference>,
    pub is_immediate: Option<bool>,
    pub auditing: Option<AuditMonitor>,
    pub monitoring: Option<AuditMonitor>,
    pub category_value_refs: Vec<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IoBinding {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub operation_ref: Option<ModelReference>,
    pub input_data_ref: Option<ElementSelector>,
    pub output_data_ref: Option<ElementSelector>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallableMetadata {
    pub supported_interface_refs: Vec<ModelReference>,
    pub io_bindings: Vec<IoBinding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ProcessType {
    None,
    Public,
    Private,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessActivityMetadata {
    pub callable: CallableMetadata,
    pub process_type: Option<ProcessType>,
    pub is_closed: Option<bool>,
    pub definitional_collaboration_ref: Option<ModelReference>,
    pub supports: Vec<ModelReference>,
    pub auditing: Option<AuditMonitor>,
    pub monitoring: Option<AuditMonitor>,
    pub properties: Vec<crate::bpmn_model::EventProperty>,
    pub resource_roles: Vec<ResourceRole>,
    pub correlation_subscriptions: Vec<CorrelationSubscription>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ResourceRoleKind {
    ResourceRole,
    Performer,
    HumanPerformer,
    PotentialOwner,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)] // Keeps assignment editing direct and bounded by owner limits.
pub enum ResourceRoleAssignment {
    None,
    Resource {
        reference: ModelReference,
        bindings: Vec<ResourceParameterBinding>,
    },
    Expression {
        assignment: ResourceAssignmentExpression,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceRole {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub kind: ResourceRoleKind,
    pub assignment: ResourceRoleAssignment,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceParameterBinding {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub parameter_ref: Option<ModelReference>,
    pub expression: Option<Expression>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceAssignmentExpression {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub expression: Option<Expression>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlobalTask {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub callable: CallableMetadata,
    pub io_specification: Option<IoSpecification>,
    pub resource_roles: Vec<ResourceRole>,
    pub kind: GlobalTaskKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum GlobalTaskKind {
    CallableElement,
    Task,
    Manual,
    BusinessRule {
        implementation: Option<String>,
    },
    Script {
        script_language: Option<String>,
        script: Option<MixedXml>,
    },
    User {
        implementation: Option<String>,
        renderings: Vec<BaseOwner>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum MultiInstanceFlowCondition {
    None,
    One,
    All,
    Complex,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComplexBehaviorDefinition {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub condition: Option<Expression>,
    pub event: Option<ImplicitThrowEvent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImplicitThrowEvent {
    pub id: Option<ElementIdentity>,
    pub name: Option<String>,
    pub base: Option<BaseMetadata>,
    pub flow: Option<FlowElementMetadata>,
    pub properties: Vec<crate::bpmn_model::EventProperty>,
    pub incoming: Vec<ModelReference>,
    pub outgoing: Vec<ModelReference>,
    pub inputs: Vec<DataPort>,
    pub input_associations: Vec<ImplicitInputAssociation>,
    pub input_set: Option<crate::bpmn_data::IoSet>,
    pub definitions: Vec<EventDefinition>,
    pub definition_refs: Vec<ModelReference>,
}

/// Input association nested in an implicit event; its owner is its containing event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImplicitInputAssociation {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub sources: Vec<ElementSelector>,
    pub target: Option<ElementSelector>,
    pub transformation: Option<Expression>,
    pub assignments: Vec<crate::bpmn_data::DataAssignment>,
}
