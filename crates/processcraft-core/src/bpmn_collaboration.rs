//! Typed BPMN conversation structures within a collaboration.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ConversationKind {
    Conversation,
    SubConversation,
    CallConversation,
}

/// A conversation's participant references resolve in its collaboration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationNode {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_flow_refs: Vec<crate::bpmn_activity::ModelReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<CallConversationData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correlation_keys: Vec<crate::bpmn_activity::CorrelationKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub kind: ConversationKind,
    pub participant_refs: Vec<crate::bpmn_activity::ModelReference>,
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
    pub source: Option<crate::bpmn_activity::ModelReference>,
    pub target: Option<crate::bpmn_activity::ModelReference>,
}

/// A choreography owns its participant and message-flow namespace.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choreography {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<CollaborationExtras>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_nodes: Vec<ConversationNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_links: Vec<ConversationLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correlation_keys: Vec<crate::bpmn_activity::CorrelationKey>,
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
    pub flow_metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correlation_keys: Vec<crate::bpmn_activity::CorrelationKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub participant_refs: Vec<crate::ids::ElementSelector>,
    pub initiating_participant: crate::ids::ElementSelector,
    pub message_flow_refs: Vec<crate::ids::ElementSelector>,
}

use crate::{bpmn_activity::ModelReference, bpmn_metadata::BaseMetadata, ids::ElementIdentity};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum CollaborationRootKind {
    GlobalConversation,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CollaborationExtras {
    pub is_closed: Option<bool>,
    pub artifacts: Vec<CollaborationArtifact>,
    pub conversation_associations: Vec<ConversationAssociation>,
    pub participant_associations: Vec<ParticipantAssociation>,
    pub message_flow_associations: Vec<MessageFlowAssociation>,
    pub choreography_refs: Vec<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum CollaborationArtifact {
    Group(crate::bpmn_model::Group),
    TextAnnotation(Box<crate::model::Node>),
    Association(Box<crate::model::Flow>),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CallConversationData {
    pub called_collaboration_ref: Option<ModelReference>,
    pub participant_associations: Vec<ParticipantAssociation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationAssociation {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub inner: Option<ModelReference>,
    pub outer: Option<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParticipantAssociation {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub inner: Option<ModelReference>,
    pub outer: Option<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageFlowAssociation {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub inner: Option<ModelReference>,
    pub outer: Option<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartnerEntity {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub participant_refs: Vec<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartnerRole {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
    pub name: Option<String>,
    pub participant_refs: Vec<ModelReference>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndPoint {
    pub id: Option<ElementIdentity>,
    pub base: Option<BaseMetadata>,
}
