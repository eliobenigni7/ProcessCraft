//! BPMN BaseElement metadata, kept separate from DI and Definitions rules.
use crate::{
    bpmn_xml::{MixedXml, XmlContent},
    ids::ElementIdentity,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum DocumentationSet {
    Legacy(String),
    Structured(Vec<Documentation>),
}
impl From<String> for DocumentationSet {
    fn from(value: String) -> Self {
        Self::Legacy(value)
    }
}
impl From<&str> for DocumentationSet {
    fn from(value: &str) -> Self {
        Self::Legacy(value.into())
    }
}
impl DocumentationSet {
    pub fn plain_text(&self) -> Option<&str> {
        match self {
            Self::Legacy(s) => Some(s),
            Self::Structured(_) => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Documentation {
    pub id: Option<ElementIdentity>,
    pub text_format: Option<String>,
    pub content: MixedXml,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaseExtras {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<crate::bpmn_xml::NamespaceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_attributes: Vec<crate::bpmn_xml::XmlAttribute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_elements: Option<ExtensionElements>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaseMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<crate::bpmn_xml::NamespaceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<Documentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_attributes: Vec<crate::bpmn_xml::XmlAttribute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_elements: Option<ExtensionElements>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionElements {
    pub content: XmlContent,
}

pub fn validate_documentation(documentation: &Documentation) -> Result<(), String> {
    if let Some(format) = &documentation.text_format
        && (format.len() > 16_384 || format.chars().any(|c| !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) ) { return Err("Invalid documentation text format".into()); }
    crate::bpmn_xml::validate_mixed(&documentation.content)
}
pub fn validate_extras(base: &BaseExtras) -> Result<(), String> {
    crate::bpmn_xml::validate_foreign_attributes(&base.foreign_attributes, &base.namespaces)?;
    if let Some(extension) = &base.extension_elements {
        crate::bpmn_xml::validate_extension(&extension.content)?;
    }
    Ok(())
}
pub fn validate_base(base: &BaseMetadata) -> Result<(), String> {
    if base.documentation.len() > 256 {
        return Err("Documentation limit exceeded".into());
    }
    for documentation in &base.documentation {
        validate_documentation(documentation)?;
    }
    crate::bpmn_xml::validate_foreign_attributes(&base.foreign_attributes, &base.namespaces)?;
    if let Some(extension) = &base.extension_elements {
        crate::bpmn_xml::validate_extension(&extension.content)?;
    }
    Ok(())
}
pub fn validate_set(set: &DocumentationSet) -> Result<(), String> {
    match set {
        DocumentationSet::Legacy(value) => {
            if value.len() > 16_384 {
                return Err("Documentation exceeds 16 KiB".into());
            }
            Ok(())
        }
        DocumentationSet::Structured(docs) => {
            if docs.len() > 256 {
                return Err("Documentation limit exceeded".into());
            }
            for doc in docs {
                validate_documentation(doc)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_documentation_keeps_string_representation() {
        let doc = DocumentationSet::Legacy("legacy".into());
        assert_eq!(serde_json::to_string(&doc).unwrap(), "\"legacy\"");
        assert_eq!(doc.plain_text(), Some("legacy"));
    }

    #[test]
    fn documentation_rejects_invalid_xml_text() {
        let doc = Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Text("bad\u{0}".into()),
        };
        assert!(validate_documentation(&doc).is_err());
    }
}
