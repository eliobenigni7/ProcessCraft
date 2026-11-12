//! Inert, ordered XML content for BPMN mixed bodies and preserved extensions.
use crate::ids::XmlId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum MixedXml {
    Text(String),
    Structured(XmlContent),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XmlContent {
    pub namespaces: Vec<NamespaceBinding>,
    pub nodes: Vec<XmlNode>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum XmlNode {
    Text {
        text: String,
    },
    Element {
        element: XmlElement,
    },
    Comment {
        text: String,
    },
    ProcessingInstruction {
        target: String,
        data: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XmlElement {
    pub namespace: String,
    pub local: XmlId,
    pub prefix: Option<XmlId>,
    pub namespaces: Vec<NamespaceBinding>,
    pub attributes: Vec<XmlAttribute>,
    pub children: Vec<XmlNode>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamespaceBinding {
    pub prefix: String,
    pub uri: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XmlAttribute {
    pub namespace: String,
    pub local: XmlId,
    pub prefix: Option<XmlId>,
    pub value: String,
}
impl MixedXml {
    pub fn plain_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::Structured(_) => None,
        }
    }
}
impl From<String> for MixedXml {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for MixedXml {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

const MAX_NODES: usize = 10_000;
const MAX_DEPTH: usize = 32;
const MAX_BYTES: usize = 1024 * 1024;
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const BPMN_NS: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";

fn xml_text(value: &str) -> Result<(), String> {
    if value.len() > MAX_BYTES || value.chars().any(|c| !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
        return Err("Invalid or oversized XML text".into());
    }
    Ok(())
}
/// Validate a processing instruction without interpreting it. XML PITarget is an
/// NCName in namespace-conformant XML, excluding the reserved name `xml`.
pub fn validate_processing_instruction(target: &str, data: Option<&str>) -> Result<(), String> {
    if target.eq_ignore_ascii_case("xml") || target.parse::<XmlId>().is_err() {
        return Err("Invalid XML processing instruction target".into());
    }
    if let Some(data) = data {
        xml_text(data)?;
        if data.contains("?>") {
            return Err("Invalid XML processing instruction".into());
        }
    }
    Ok(())
}
fn bindings(
    bindings: &[NamespaceBinding],
    parent: &std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    if bindings.len() > 256 {
        return Err("XML namespace limit exceeded".into());
    }
    let mut result = parent.clone();
    let mut seen = std::collections::HashSet::new();
    for binding in bindings {
        xml_text(&binding.prefix)?;
        xml_text(&binding.uri)?;
        if !seen.insert(&binding.prefix)
            || binding.prefix == "xmlns"
            || binding.uri == XMLNS_NS
            || (!binding.prefix.is_empty()
                && (binding.prefix.parse::<XmlId>().is_err() || binding.uri.is_empty()))
            || (binding.prefix == "xml") != (binding.uri == XML_NS)
            || binding.uri.chars().any(char::is_whitespace)
        {
            return Err("Invalid XML namespace binding".into());
        }
        result.insert(binding.prefix.clone(), binding.uri.clone());
    }
    Ok(result)
}
fn name_binding(
    namespace: &str,
    prefix: Option<&XmlId>,
    default_applies: bool,
    scope: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    xml_text(namespace)?;
    let actual = match prefix {
        Some(prefix) => scope
            .get(&prefix.to_string())
            .map(String::as_str)
            .ok_or("Unbound XML prefix")?,
        None if default_applies => scope.get("").map(String::as_str).unwrap_or(""),
        None => "",
    };
    if actual != namespace {
        return Err("XML name namespace does not match prefix binding".into());
    }
    Ok(())
}
fn attrs(
    attributes: &[XmlAttribute],
    scope: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    if attributes.len() > 256 {
        return Err("XML attribute limit exceeded".into());
    }
    let mut seen = std::collections::HashSet::new();
    for attribute in attributes {
        name_binding(
            &attribute.namespace,
            attribute.prefix.as_ref(),
            false,
            scope,
        )?;
        xml_text(&attribute.value)?;
        if !seen.insert((&attribute.namespace, &attribute.local)) {
            return Err("Duplicate XML attribute".into());
        }
    }
    Ok(())
}
fn validate_nodes(
    nodes: &[XmlNode],
    scope: &std::collections::HashMap<String, String>,
    depth: usize,
    count: &mut usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("XML nesting limit exceeded".into());
    }
    for node in nodes {
        *count = count.checked_add(1).ok_or("XML node limit exceeded")?;
        if *count > MAX_NODES {
            return Err("XML node limit exceeded".into());
        }
        match node {
            XmlNode::Text { text } => xml_text(text)?,
            XmlNode::Comment { text } => {
                xml_text(text)?;
                if text.contains("--") || text.ends_with('-') {
                    return Err("Invalid XML comment".into());
                }
            }
            XmlNode::ProcessingInstruction { target, data } => {
                validate_processing_instruction(target, data.as_deref())?
            }
            XmlNode::Element { element } => {
                let scope = bindings(&element.namespaces, scope)?;
                name_binding(&element.namespace, element.prefix.as_ref(), true, &scope)?;
                attrs(&element.attributes, &scope)?;
                validate_nodes(&element.children, &scope, depth + 1, count)?;
            }
        }
    }
    Ok(())
}
/// Validate a documentation/script/text body against the XSD's at-most-one child element.
pub fn validate_mixed(body: &MixedXml) -> Result<(), String> {
    match body {
        MixedXml::Text(s) => xml_text(s),
        MixedXml::Structured(content) => {
            if content
                .nodes
                .iter()
                .filter(|n| matches!(n, XmlNode::Element { .. }))
                .count()
                > 1
            {
                return Err("Mixed XML body has more than one child element".into());
            }
            let mut scope = std::collections::HashMap::new();
            scope.insert("xml".into(), XML_NS.into());
            let scope = bindings(&content.namespaces, &scope)?;
            validate_nodes(&content.nodes, &scope, 0, &mut 0)
        }
    }
}
/// Validate a BPMN extensionElements body without interpreting foreign semantics.
pub fn validate_extension(content: &XmlContent) -> Result<(), String> {
    for node in &content.nodes {
        match node {
            XmlNode::Element { element }
                if element.namespace.is_empty() || element.namespace == BPMN_NS =>
            {
                return Err("Extension child must have a foreign namespace".into());
            }
            XmlNode::Text { text } if !text.trim_matches([' ', '\t', '\r', '\n']).is_empty() => {
                return Err("Extension wrapper cannot contain character data".into());
            }
            _ => {}
        }
    }
    let mut scope = std::collections::HashMap::new();
    scope.insert("xml".into(), XML_NS.into());
    let scope = bindings(&content.namespaces, &scope)?;
    validate_nodes(&content.nodes, &scope, 0, &mut 0)
}
/// Validate foreign attributes in their captured namespace context.
pub fn validate_foreign_attributes(
    attributes: &[XmlAttribute],
    namespaces: &[NamespaceBinding],
) -> Result<(), String> {
    if attributes
        .iter()
        .any(|a| a.namespace.is_empty() || a.namespace == BPMN_NS)
    {
        return Err("Attribute must have a foreign namespace".into());
    }
    let mut scope = std::collections::HashMap::new();
    scope.insert("xml".into(), XML_NS.into());
    let scope = bindings(namespaces, &scope)?;
    attrs(attributes, &scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_content_limits_children_and_checks_comment_syntax() {
        let mut content = XmlContent {
            namespaces: vec![],
            nodes: vec![XmlNode::Comment { text: "ok".into() }],
        };
        assert!(validate_mixed(&MixedXml::Structured(content.clone())).is_ok());
        content.nodes.push(XmlNode::Comment {
            text: "bad--comment".into(),
        });
        assert!(validate_mixed(&MixedXml::Structured(content)).is_err());
    }

    #[test]
    fn processing_instruction_uses_namespace_conformant_ncname() {
        assert!(validate_processing_instruction("vendor_action", Some("safe")).is_ok());
        assert!(validate_processing_instruction("vendor:action", None).is_err());
        assert!(validate_processing_instruction(":action", None).is_err());
        assert!(validate_processing_instruction("xMl", None).is_err());
        assert!(validate_processing_instruction("bad target", None).is_err());
        assert!(validate_processing_instruction("ok", Some("bad?>data")).is_err());
    }

    #[test]
    fn extension_accepts_only_xml_whitespace() {
        let content = |value: &str| XmlContent {
            namespaces: vec![],
            nodes: vec![XmlNode::Text { text: value.into() }],
        };
        assert!(validate_extension(&content(" \t\r\n")).is_ok());
        assert!(validate_extension(&content("\u{a0}")).is_err());
        assert!(validate_mixed(&MixedXml::Text("\u{a0}".into())).is_ok());
    }

    #[test]
    fn extension_elements_reject_bpmn_children() {
        let content = XmlContent {
            namespaces: vec![NamespaceBinding {
                prefix: "b".into(),
                uri: BPMN_NS.into(),
            }],
            nodes: vec![XmlNode::Element {
                element: XmlElement {
                    namespace: BPMN_NS.into(),
                    local: "task".parse().unwrap(),
                    prefix: Some("b".parse().unwrap()),
                    namespaces: vec![],
                    attributes: vec![],
                    children: vec![],
                },
            }],
        };
        assert!(validate_extension(&content).is_err());
    }
}
