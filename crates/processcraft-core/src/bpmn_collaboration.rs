//! Typed BPMN conversation structures within a collaboration.
use crate::ids::XmlId;
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
    pub id: XmlId,
    pub name: String,
    pub kind: ConversationKind,
    pub participant_refs: Vec<XmlId>,
    pub parent: Option<XmlId>,
}

/// A link joins a conversation node and a participant in the same collaboration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationLink {
    pub id: XmlId,
    pub name: String,
    pub source: XmlId,
    pub target: XmlId,
}

/// A choreography owns its participant and message-flow namespace.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choreography {
    pub id: XmlId,
    pub name: String,
    pub participants: Vec<crate::model::Participant>,
    pub message_flows: Vec<crate::model::Flow>,
    pub activities: Vec<ChoreographyActivity>,
    pub sequence_flows: Vec<crate::model::Flow>,
}
/// The qualified subset models choreography tasks, not gateways or events.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoreographyActivity {
    pub id: XmlId,
    pub name: String,
    pub participant_refs: Vec<XmlId>,
    pub initiating_participant: XmlId,
    pub message_flow_refs: Vec<XmlId>,
}
