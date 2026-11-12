//! Typed BPMN conversation structures within a collaboration.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ConversationKind {
    Conversation,
    SubConversation,
}

/// A conversation's participant references resolve in its collaboration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub kind: ConversationKind,
    pub participant_refs: Vec<crate::ids::ElementSelector>,
    pub parent: Option<crate::ids::ElementSelector>,
}

/// A link joins a conversation node and a participant in the same collaboration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationLink {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub source: crate::ids::ElementSelector,
    pub target: crate::ids::ElementSelector,
}

/// A choreography owns its participant and message-flow namespace.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choreography {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub participants: Vec<crate::model::Participant>,
    pub message_flows: Vec<crate::model::Flow>,
    pub activities: Vec<ChoreographyActivity>,
    pub sequence_flows: Vec<crate::model::Flow>,
}
/// The qualified subset models choreography tasks, not gateways or events.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoreographyActivity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub participant_refs: Vec<crate::ids::ElementSelector>,
    pub initiating_participant: crate::ids::ElementSelector,
    pub message_flow_refs: Vec<crate::ids::ElementSelector>,
}
