//! Typed BPMN activity attributes. Values are model data, never executed.
use crate::{bpmn_model::QNameRef, ids::XmlId};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_for_compensation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_quantity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_flow: Option<XmlId>,
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
    pub script: Option<String>,
}
