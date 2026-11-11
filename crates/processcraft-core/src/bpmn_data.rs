//! Typed BPMN data declarations and scoped data associations.
use crate::ids::XmlId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataStore {
    pub id: XmlId,
    pub name: String,
    pub capacity: Option<String>,
    pub is_unlimited: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataObject {
    pub id: XmlId,
    pub name: String,
    pub is_collection: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DataAssociationKind {
    Input,
    Output,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataAssociation {
    pub id: XmlId,
    pub owner: XmlId,
    pub kind: DataAssociationKind,
    pub sources: Vec<XmlId>,
    pub target: XmlId,
}

/// Activity-local input/output ports. Binding sets and unsupported expressions remain retained source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataPort {
    pub id: XmlId,
    pub name: String,
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
