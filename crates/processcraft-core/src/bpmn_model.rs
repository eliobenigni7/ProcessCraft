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
    pub text: String,
    /// Effective namespace bindings; the empty prefix denotes the default namespace.
    pub namespaces: Vec<(String, String)>,
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
    pub fn text(&self) -> &str {
        match self {
            Self::Legacy(s) => s,
            Self::Structured(s) => &s.text,
        }
    }
    pub fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Legacy(s) => s,
            Self::Structured(s) => &mut s.text,
        }
    }
    pub fn set_text(&mut self, text: String) {
        *self.text_mut() = text;
    }
}
impl std::ops::Deref for Expression {
    type Target = str;
    fn deref(&self) -> &str {
        self.text()
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
        message_ref: Option<EventReference>,
        operation_ref: Option<EventReference>,
    },
    Signal {
        id: Option<crate::ids::ElementIdentity>,
        signal_ref: Option<EventReference>,
    },
    Error {
        id: Option<crate::ids::ElementIdentity>,
        error_ref: Option<EventReference>,
    },
    Escalation {
        id: Option<crate::ids::ElementIdentity>,
        escalation_ref: Option<EventReference>,
    },
    Conditional {
        id: Option<crate::ids::ElementIdentity>,
        expression: Expression,
    },
    Link {
        id: Option<crate::ids::ElementIdentity>,
        name: String,
        sources: Vec<EventReference>,
        target: Option<EventReference>,
    },
    Cancel {
        id: Option<crate::ids::ElementIdentity>,
    },
    Compensate {
        id: Option<crate::ids::ElementIdentity>,
        wait_for_completion: Option<bool>,
        activity_ref: Option<EventReference>,
    },
    Terminate {
        id: Option<crate::ids::ElementIdentity>,
    },
    Timer {
        id: Option<crate::ids::ElementIdentity>,
        kind: Option<TimerKind>,
        expression: Option<Expression>,
    },
}
impl EventDefinition {
    pub fn id(&self) -> Option<&crate::ids::ElementIdentity> {
        match self {
            Self::Message { id, .. }
            | Self::Signal { id, .. }
            | Self::Error { id, .. }
            | Self::Escalation { id, .. }
            | Self::Conditional { id, .. }
            | Self::Link { id, .. }
            | Self::Cancel { id }
            | Self::Compensate { id, .. }
            | Self::Terminate { id }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum NodeDetails {
    TextAnnotation {
        text_format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
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
        called_element: QNameRef,
    },
    BoundaryEvent {
        attached_to: crate::ids::ElementSelector,
        cancel_activity: bool,
    },
    Event {
        definitions: Vec<EventDefinition>,
        attached_to: Option<crate::ids::ElementSelector>,
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

/// Typed, document-owned BPMN root declarations referenced by events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum EventResource {
    Signal {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
    Message {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_ref: Option<QNameRef>,
    },
    Error {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
    Escalation {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure_ref: Option<QNameRef>,
    },
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
pub enum LoopCharacteristics {
    Standard {
        test_before: bool,
        condition: Option<Expression>,
        maximum: Option<String>,
    },
    MultiInstance {
        is_sequential: bool,
        cardinality: Option<Expression>,
        completion_condition: Option<Expression>,
    },
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
    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub values: Vec<CategoryValue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryValue {
    pub id: crate::ids::ElementIdentity,
    pub value: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: crate::ids::ElementIdentity,
    pub category_value_ref: Option<crate::ids::ElementSelector>,
}
