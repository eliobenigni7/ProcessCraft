//! Typed BPMN data declarations and scoped data associations.
use crate::{
    bpmn_model::{Expression, QNameRef},
    ids::XmlId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataStore {
    pub id: Option<XmlId>,
    pub name: Option<String>,
    pub capacity: Option<String>,
    pub is_unlimited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_aware: Option<ItemAwareMetadata>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataObject {
    pub id: Option<XmlId>,
    pub name: Option<String>,
    pub is_collection: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_aware: Option<ItemAwareMetadata>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ItemAwareMetadata {
    pub item_subject_ref: Option<QNameRef>,
    pub data_state: Option<DataState>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataState {
    pub id: Option<XmlId>,
    pub name: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DataAssociationKind {
    Input,
    Output,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataAssociation {
    pub id: Option<XmlId>,
    pub owner: XmlId,
    pub kind: DataAssociationKind,
    pub sources: Vec<XmlId>,
    pub target: XmlId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transformation: Option<Expression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<DataAssignment>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataAssignment {
    pub id: Option<XmlId>,
    pub from: Expression,
    pub to: Expression,
}

/// Activity-local input/output ports. Binding sets and unsupported expressions remain retained source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataPort {
    pub id: Option<XmlId>,
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_aware: Option<ItemAwareMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_collection: Option<bool>,
}
/// Legacy local references serialize as strings; qualified references retain namespace identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum DataStoreRef {
    Legacy(XmlId),
    Qualified(QNameRef),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IoSpecification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<XmlId>,
    pub inputs: Vec<DataPort>,
    pub outputs: Vec<DataPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_sets: Vec<IoSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_sets: Vec<IoSet>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IoSet {
    pub id: Option<XmlId>,
    pub name: Option<String>,
    pub data_refs: Vec<XmlId>,
    pub optional_refs: Vec<XmlId>,
    pub while_executing_refs: Vec<XmlId>,
    pub counterpart_refs: Vec<XmlId>,
}
