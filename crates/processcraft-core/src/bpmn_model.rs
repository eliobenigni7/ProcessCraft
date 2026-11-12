//! Typed BPMN metadata beyond the basic node projection.
use crate::ids::XmlId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A plain legacy expression or a text expression with its BPMN metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Expression {
    Legacy(String),
    Structured(ExpressionData),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionData {
    pub id: Option<crate::ids::ElementIdentity>,
    pub kind: ExpressionKind,
    pub text: ExpressionBody,
    /// Effective namespace bindings; the empty prefix denotes the default namespace.
    pub namespaces: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_attributes: Vec<crate::bpmn_xml::XmlAttribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum ExpressionBody {
    Text(String),
    Mixed { parts: Vec<ExpressionPart> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ExpressionPart {
    Text {
        text: String,
    },
    Documentation {
        documentation: crate::bpmn_metadata::Documentation,
    },
    ExtensionElements {
        extension_elements: crate::bpmn_metadata::ExtensionElements,
    },
    Comment {
        text: String,
    },
    ProcessingInstruction {
        target: String,
        data: Option<String>,
    },
}
impl From<String> for ExpressionBody {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for ExpressionBody {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ExpressionKind {
    Base {
        explicit_type: bool,
    },
    Formal {
        explicit_type: bool,
        language: Option<String>,
        evaluates_to_type_ref: Option<QNameRef>,
    },
}
impl Expression {
    pub fn plain_text(&self) -> Option<&str> {
        match self {
            Self::Legacy(s) => Some(s),
            Self::Structured(ExpressionData {
                text: ExpressionBody::Text(s),
                ..
            }) => Some(s),
            Self::Structured(_) => None,
        }
    }
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Legacy(s) => Some(s),
            Self::Structured(ExpressionData {
                text: ExpressionBody::Text(s),
                ..
            }) => Some(s),
            Self::Structured(_) => None,
        }
    }
    pub fn set_text(&mut self, text: String) -> Result<(), &'static str> {
        *self
            .text_mut()
            .ok_or("Cannot replace mixed expression with plain text")? = text;
        Ok(())
    }
}
impl From<String> for Expression {
    fn from(value: String) -> Self {
        Self::Legacy(value)
    }
}
impl From<&str> for Expression {
    fn from(value: &str) -> Self {
        Self::Legacy(value.into())
    }
}

/// Event definitions are modeled explicitly; unsupported variants stay preserved source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum EventDefinition {
    Message {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        message_ref: Option<EventReference>,
        operation_ref: Option<EventReference>,
    },
    Signal {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        signal_ref: Option<EventReference>,
    },
    Error {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        error_ref: Option<EventReference>,
    },
    Escalation {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        escalation_ref: Option<EventReference>,
    },
    Conditional {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        expression: Expression,
    },
    Link {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        name: String,
        sources: Vec<EventReference>,
        target: Option<EventReference>,
    },
    Cancel {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
    },
    Compensate {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        wait_for_completion: Option<bool>,
        activity_ref: Option<EventReference>,
    },
    Terminate {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
    },
    Timer {
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        kind: Option<TimerKind>,
        expression: Option<Expression>,
    },
}
impl EventDefinition {
    pub fn base(&self) -> Option<&crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Message { base, .. }
            | Self::Signal { base, .. }
            | Self::Error { base, .. }
            | Self::Escalation { base, .. }
            | Self::Conditional { base, .. }
            | Self::Link { base, .. }
            | Self::Cancel { base, .. }
            | Self::Compensate { base, .. }
            | Self::Terminate { base, .. }
            | Self::Timer { base, .. } => base.as_ref(),
        }
    }
    pub fn base_mut(&mut self) -> &mut Option<crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Message { base, .. }
            | Self::Signal { base, .. }
            | Self::Error { base, .. }
            | Self::Escalation { base, .. }
            | Self::Conditional { base, .. }
            | Self::Link { base, .. }
            | Self::Cancel { base, .. }
            | Self::Compensate { base, .. }
            | Self::Terminate { base, .. }
            | Self::Timer { base, .. } => base,
        }
    }
    pub fn id(&self) -> Option<&crate::ids::ElementIdentity> {
        match self {
            Self::Message { id, .. }
            | Self::Signal { id, .. }
            | Self::Error { id, .. }
            | Self::Escalation { id, .. }
            | Self::Conditional { id, .. }
            | Self::Link { id, .. }
            | Self::Cancel { id, .. }
            | Self::Compensate { id, .. }
            | Self::Terminate { id, .. }
            | Self::Timer { id, .. } => id.as_ref(),
        }
    }
    pub fn has_native_references(&self) -> bool {
        match self {
            Self::Message {
                message_ref,
                operation_ref,
                ..
            } => {
                message_ref.as_ref().is_some_and(EventReference::is_native)
                    || operation_ref
                        .as_ref()
                        .is_some_and(EventReference::is_native)
            }
            Self::Signal { signal_ref, .. } => {
                signal_ref.as_ref().is_some_and(EventReference::is_native)
            }
            Self::Error { error_ref, .. } => {
                error_ref.as_ref().is_some_and(EventReference::is_native)
            }
            Self::Escalation { escalation_ref, .. } => escalation_ref
                .as_ref()
                .is_some_and(EventReference::is_native),
            Self::Link {
                sources, target, ..
            } => {
                sources.iter().any(EventReference::is_native)
                    || target.as_ref().is_some_and(EventReference::is_native)
            }
            Self::Compensate { activity_ref, .. } => {
                activity_ref.as_ref().is_some_and(EventReference::is_native)
            }
            _ => false,
        }
    }
    pub fn has_qualified_references(&self) -> bool {
        let qualified =
            |r: &Option<EventReference>| matches!(r, Some(EventReference::Qualified(_)));
        match self {
            Self::Message {
                message_ref,
                operation_ref,
                ..
            } => qualified(message_ref) || qualified(operation_ref),
            Self::Signal { signal_ref, .. } => qualified(signal_ref),
            Self::Error { error_ref, .. } => qualified(error_ref),
            Self::Escalation { escalation_ref, .. } => qualified(escalation_ref),
            Self::Compensate { activity_ref, .. } => qualified(activity_ref),
            Self::Link {
                sources, target, ..
            } => {
                sources
                    .iter()
                    .any(|r| matches!(r, EventReference::Qualified(_)))
                    || qualified(target)
            }
            _ => false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum EventReference {
    Local(crate::ids::ElementSelector),
    Qualified(QNameRef),
}
impl From<crate::ids::ElementSelector> for EventReference {
    fn from(value: crate::ids::ElementSelector) -> Self {
        Self::Local(value)
    }
}
impl EventReference {
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Local(value) if value.is_native())
    }
    pub fn matches_identity(
        &self,
        identity: &crate::ids::ElementIdentity,
        target_namespace: Option<&str>,
    ) -> bool {
        match self {
            Self::Local(selector) => identity == selector,
            Self::Qualified(name) => {
                target_namespace == Some(name.namespace.as_str())
                    && identity.xml_id() == Some(&name.local)
            }
        }
    }
    pub fn is_external(&self, target_namespace: Option<&str>) -> bool {
        matches!(self, Self::Qualified(name) if target_namespace != Some(name.namespace.as_str()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventProperty {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: Option<crate::ids::ElementIdentity>,
    pub name: Option<String>,
    pub item_aware: Option<crate::bpmn_data::ItemAwareMetadata>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum EventIo {
    Catch {
        outputs: Vec<crate::bpmn_data::DataPort>,
        output_set: Option<crate::bpmn_data::IoSet>,
    },
    Throw {
        inputs: Vec<crate::bpmn_data::DataPort>,
        input_set: Option<crate::bpmn_data::IoSet>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TimerKind {
    Date,
    Duration,
    Cycle,
}

// Keep the existing public and serialized event fields directly addressable.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum NodeDetails {
    TextAnnotation {
        text_format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<crate::bpmn_xml::MixedXml>,
    },
    Gateway {
        default_flow: Option<crate::ids::ElementSelector>,
        direction: Option<GatewayDirection>,
        instantiate: Option<bool>,
        event_gateway_type: Option<EventGatewayType>,
        activation_condition: Option<Expression>,
    },
    DataObjectReference {
        data_object_ref: Option<crate::ids::ElementSelector>,
    },
    DataStoreReference {
        data_store_ref: Option<crate::bpmn_data::DataStoreRef>,
    },
    SubProcess {
        content: Box<crate::model::FlowContainer>,
    },
    CallActivity {
        called_element: Option<QNameRef>,
    },
    BoundaryEvent {
        attached_to: crate::bpmn_activity::ModelReference,
        cancel_activity: bool,
    },
    Event {
        definitions: Vec<EventDefinition>,
        attached_to: Option<crate::bpmn_activity::ModelReference>,
        cancel_activity: Option<bool>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        definition_refs: Vec<EventReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parallel_multiple: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_interrupting: Option<bool>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        properties: Vec<EventProperty>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        io: Option<EventIo>,
    },
}

impl NodeDetails {
    /// Event or gateway details that carry no field. BPMN XML cannot represent them
    /// apart from absent details, so the canonical form is `None`.
    pub fn is_vacuous(&self) -> bool {
        match self {
            Self::Event {
                definitions,
                attached_to,
                cancel_activity,
                definition_refs,
                parallel_multiple,
                is_interrupting,
                properties,
                io,
            } => {
                definitions.is_empty()
                    && attached_to.is_none()
                    && cancel_activity.is_none()
                    && definition_refs.is_empty()
                    && parallel_multiple.is_none()
                    && is_interrupting.is_none()
                    && properties.is_empty()
                    && io.is_none()
            }
            Self::Gateway {
                default_flow,
                direction,
                instantiate,
                event_gateway_type,
                activation_condition,
            } => {
                default_flow.is_none()
                    && direction.is_none()
                    && instantiate.is_none()
                    && event_gateway_type.is_none()
                    && activation_condition.is_none()
            }
            _ => false,
        }
    }
}

/// Typed, document-owned BPMN root declarations referenced by events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum EventResource {
    Signal {
        id: crate::ids::ElementIdentity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
    Message {
        id: crate::ids::ElementIdentity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_ref: Option<QNameRef>,
    },
    Error {
        id: crate::ids::ElementIdentity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        name: Option<String>,
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
    Escalation {
        id: crate::ids::ElementIdentity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        name: Option<String>,
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
}
impl EventResource {
    pub fn base(&self) -> Option<&crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Signal { base, .. }
            | Self::Message { base, .. }
            | Self::Error { base, .. }
            | Self::Escalation { base, .. } => base.as_ref(),
        }
    }
    pub fn base_mut(&mut self) -> &mut Option<crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Signal { base, .. }
            | Self::Message { base, .. }
            | Self::Error { base, .. }
            | Self::Escalation { base, .. } => base,
        }
    }
}

/// Expanded XML QName; prefix spelling is not semantic identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QNameRef {
    pub namespace: String,
    pub local: XmlId,
}

/// Qualified simple-text loop characteristics. Unsupported BPMN loop fields remain retained source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)] // Retains the legacy unboxed field API and wire representation.
pub enum LoopCharacteristics {
    Standard {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        test_before: Option<bool>,
        condition: Option<Expression>,
        maximum: Option<String>,
    },
    MultiInstance {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<crate::ids::ElementIdentity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<crate::bpmn_metadata::BaseMetadata>,
        is_sequential: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        behavior: Option<crate::bpmn_activity::MultiInstanceFlowCondition>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        loop_data_input_ref: Option<crate::bpmn_activity::ModelReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        loop_data_output_ref: Option<crate::bpmn_activity::ModelReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_data_item: Option<crate::bpmn_data::DataPort>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_data_item: Option<crate::bpmn_data::DataPort>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        complex_behavior_definitions: Vec<crate::bpmn_activity::ComplexBehaviorDefinition>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        one_behavior_event_ref: Option<crate::bpmn_activity::ModelReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        none_behavior_event_ref: Option<crate::bpmn_activity::ModelReference>,
        cardinality: Option<Expression>,
        completion_condition: Option<Expression>,
    },
}
impl LoopCharacteristics {
    pub fn base(&self) -> Option<&crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Standard { base, .. } | Self::MultiInstance { base, .. } => base.as_ref(),
        }
    }
    pub fn base_mut(&mut self) -> &mut Option<crate::bpmn_metadata::BaseMetadata> {
        match self {
            Self::Standard { base, .. } | Self::MultiInstance { base, .. } => base,
        }
    }
    pub fn id(&self) -> Option<&crate::ids::ElementIdentity> {
        match self {
            Self::Standard { id, .. } | Self::MultiInstance { id, .. } => id.as_ref(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GatewayDirection {
    Unspecified,
    Converging,
    Diverging,
    Mixed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EventGatewayType {
    Exclusive,
    Parallel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AssociationDirection {
    None,
    One,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Category {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub values: Vec<CategoryValue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub value: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub category_value_ref: Option<crate::bpmn_activity::ModelReference>,
}
