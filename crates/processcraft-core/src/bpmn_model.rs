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
        message_ref: Option<crate::ids::ElementSelector>,
        operation_ref: Option<crate::ids::ElementSelector>,
    },
    Signal {
        id: Option<crate::ids::ElementIdentity>,
        signal_ref: Option<crate::ids::ElementSelector>,
    },
    Error {
        id: Option<crate::ids::ElementIdentity>,
        error_ref: Option<crate::ids::ElementSelector>,
    },
    Escalation {
        id: Option<crate::ids::ElementIdentity>,
        escalation_ref: Option<crate::ids::ElementSelector>,
    },
    Conditional {
        id: Option<crate::ids::ElementIdentity>,
        expression: Expression,
    },
    Link {
        id: Option<crate::ids::ElementIdentity>,
        name: String,
        sources: Vec<crate::ids::ElementSelector>,
        target: Option<crate::ids::ElementSelector>,
    },
    Cancel {
        id: Option<crate::ids::ElementIdentity>,
    },
    Compensate {
        id: Option<crate::ids::ElementIdentity>,
        wait_for_completion: Option<bool>,
        activity_ref: Option<crate::ids::ElementSelector>,
    },
    Terminate {
        id: Option<crate::ids::ElementIdentity>,
    },
    Timer {
        id: Option<crate::ids::ElementIdentity>,
        kind: TimerKind,
        expression: Expression,
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
    },
}

/// Typed, document-owned BPMN root declarations referenced by events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum EventResource {
    Signal {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
    },
    Message {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
    },
    Error {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        code: Option<String>,
    },
    Escalation {
        id: crate::ids::ElementIdentity,
        name: Option<String>,
        code: Option<String>,
    },
}

/// Expanded XML QName; prefix spelling is not semantic identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
