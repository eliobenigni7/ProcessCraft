//! BPMN supporting root declarations; metadata is never executed or fetched.
use crate::bpmn_model::QNameRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportingCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_definitions: Vec<ItemDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<Interface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<BpmnResource>,
}
impl SupportingCatalog {
    pub fn is_empty(&self) -> bool {
        self.item_definitions.is_empty() && self.interfaces.is_empty() && self.resources.is_empty()
    }
    pub fn has_qname_refs(&self) -> bool {
        self.item_definitions
            .iter()
            .any(|i| i.structure_ref.is_some())
            || self
                .interfaces
                .iter()
                .any(|i| i.implementation_ref.is_some() || !i.operations.is_empty())
            || self
                .resources
                .iter()
                .flat_map(|r| &r.parameters)
                .any(|p| p.type_ref.is_some())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ItemKind {
    Information,
    Physical,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ItemDefinition {
    pub id: Option<crate::ids::ElementIdentity>,
    pub structure_ref: Option<QNameRef>,
    pub is_collection: Option<bool>,
    pub item_kind: Option<ItemKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    pub id: Option<crate::ids::ElementIdentity>,
    pub name: String,
    pub implementation_ref: Option<QNameRef>,
    pub operations: Vec<Operation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: Option<crate::ids::ElementIdentity>,
    pub name: String,
    pub implementation_ref: Option<QNameRef>,
    pub in_message_ref: QNameRef,
    pub out_message_ref: Option<QNameRef>,
    pub error_refs: Vec<QNameRef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BpmnResource {
    pub id: Option<crate::ids::ElementIdentity>,
    pub name: String,
    pub parameters: Vec<ResourceParameter>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceParameter {
    pub id: Option<crate::ids::ElementIdentity>,
    pub name: Option<String>,
    pub type_ref: Option<QNameRef>,
    pub is_required: Option<bool>,
}
