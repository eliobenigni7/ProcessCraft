//! Low-level XML writers: escaping, namespaces, extension nodes, catalog, expressions, DI and labels.
use crate::interchange::export::ExportContext;
use crate::interchange::export::elements::{
    anonymous_base_attributes, anonymous_base_children, write_xml_id, xml_reference,
};
use crate::ns::{BPMN, BPMNDI, DC, DI, XSI};
use processcraft_core::bpmn_model::{Expression, ExpressionKind, QNameRef};
use processcraft_core::ids::{ElementIdentity, XmlId};
use processcraft_core::model::*;
pub(super) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}
pub(super) fn write_namespace_context(
    xml: &mut String,
    namespaces: &[processcraft_core::bpmn_xml::NamespaceBinding],
) {
    for binding in namespaces {
        if binding.prefix == "xml" {
            continue;
        }
        if binding.prefix.is_empty() {
            xml.push_str(&format!(" xmlns=\"{}\"", esc(&binding.uri)));
        } else {
            xml.push_str(&format!(
                " xmlns:{}=\"{}\"",
                binding.prefix,
                esc(&binding.uri)
            ));
        }
    }
}
fn write_xml_nodes(xml: &mut String, nodes: &[processcraft_core::bpmn_xml::XmlNode]) {
    use processcraft_core::bpmn_xml::XmlNode;
    for node in nodes {
        match node {
            XmlNode::Text { text } => xml.push_str(&esc(text)),
            XmlNode::Comment { text } => xml.push_str(&format!("<!--{text}-->")),
            XmlNode::ProcessingInstruction { target, data } => {
                xml.push_str(&format!("<?{target}"));
                if let Some(data) = data {
                    xml.push(' ');
                    xml.push_str(data);
                }
                xml.push_str("?>");
            }
            XmlNode::Element { element } => {
                let name = element
                    .prefix
                    .as_ref()
                    .map(|p| format!("{p}:{}", element.local))
                    .unwrap_or_else(|| element.local.to_string());
                xml.push('<');
                xml.push_str(&name);
                write_namespace_context(xml, &element.namespaces);
                for attribute in &element.attributes {
                    xml.push(' ');
                    if let Some(prefix) = &attribute.prefix {
                        xml.push_str(&prefix.to_string());
                        xml.push(':');
                    }
                    xml.push_str(&attribute.local.to_string());
                    xml.push_str(&format!("=\"{}\"", esc(&attribute.value)));
                }
                xml.push('>');
                write_xml_nodes(xml, &element.children);
                xml.push_str(&format!("</{name}>"));
            }
        }
    }
}
pub(super) fn write_structured_documentation(
    xml: &mut String,
    doc: &processcraft_core::bpmn_metadata::Documentation,
) {
    use processcraft_core::bpmn_xml::MixedXml;
    let (name, namespaces) = match &doc.content {
        MixedXml::Text(_) => ("documentation".to_owned(), None),
        MixedXml::Structured(content) => {
            let name = if content
                .namespaces
                .iter()
                .any(|n| n.prefix.is_empty() && n.uri == BPMN)
            {
                "documentation".to_owned()
            } else {
                let prefix = content
                    .namespaces
                    .iter()
                    .find(|n| !n.prefix.is_empty() && n.uri == BPMN)
                    .map(|n| n.prefix.as_str())
                    .unwrap_or("!");
                format!("{prefix}:documentation")
            };
            (name, Some(content.namespaces.as_slice()))
        }
    };
    xml.push('<');
    xml.push_str(&name);
    if let Some(namespaces) = namespaces {
        write_namespace_context(xml, namespaces);
    } else {
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
    }
    if let Some(id) = &doc.id {
        write_xml_id(xml, id);
    }
    if let Some(format) = &doc.text_format {
        xml.push_str(&format!(" textFormat=\"{}\"", esc(format)));
    }
    xml.push('>');
    match &doc.content {
        MixedXml::Text(text) => xml.push_str(&esc(text)),
        MixedXml::Structured(content) => write_xml_nodes(xml, &content.nodes),
    }
    xml.push_str(&format!("</{name}>"));
}
pub(super) fn write_mixed_element(
    xml: &mut String,
    tag: &str,
    body: &processcraft_core::bpmn_xml::MixedXml,
) {
    use processcraft_core::bpmn_xml::MixedXml;
    match body {
        MixedXml::Text(text) => xml.push_str(&format!("<{tag}>{}</{tag}>", esc(text))),
        MixedXml::Structured(content) => {
            let name = if content
                .namespaces
                .iter()
                .any(|n| n.prefix.is_empty() && n.uri == BPMN)
            {
                tag.to_owned()
            } else {
                format!(
                    "{}:{tag}",
                    content
                        .namespaces
                        .iter()
                        .find(|n| !n.prefix.is_empty() && n.uri == BPMN)
                        .map(|n| n.prefix.as_str())
                        .unwrap_or("!")
                )
            };
            xml.push('<');
            xml.push_str(&name);
            write_namespace_context(xml, &content.namespaces);
            xml.push('>');
            write_xml_nodes(xml, &content.nodes);
            xml.push_str(&format!("</{name}>"));
        }
    }
}
pub(super) fn write_extension_elements(
    xml: &mut String,
    extension: &processcraft_core::bpmn_metadata::ExtensionElements,
) {
    let context = &extension.content.namespaces;
    let tag = if context.iter().any(|n| n.prefix.is_empty() && n.uri == BPMN) {
        "extensionElements".to_owned()
    } else {
        format!(
            "{}:extensionElements",
            context
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == BPMN)
                .map(|n| n.prefix.as_str())
                .unwrap_or("!")
        )
    };
    xml.push('<');
    xml.push_str(&tag);
    write_namespace_context(xml, context);
    xml.push('>');
    write_xml_nodes(xml, &extension.content.nodes);
    xml.push_str(&format!("</{tag}>"));
}

pub(super) fn write_catalog_qname_attribute(
    xml: &mut String,
    attribute: &str,
    q: &QNameRef,
    prefix: &str,
) {
    if q.namespace.is_empty() {
        xml.push_str(&format!(" {attribute}=\"{}\"", q.local));
    } else {
        xml.push_str(&format!(
            " xmlns:{prefix}=\"{}\" {attribute}=\"{prefix}:{}\"",
            esc(&q.namespace),
            q.local
        ));
    }
}
pub(super) fn write_context_qname_attribute(
    xml: &mut String,
    name: &str,
    q: &QNameRef,
    context: &[processcraft_core::bpmn_xml::NamespaceBinding],
) {
    let default = context
        .iter()
        .find(|n| n.prefix.is_empty())
        .map(|n| n.uri.as_str())
        .unwrap_or("");
    let value = if q.namespace == default {
        q.local.to_string()
    } else {
        let prefix = context
            .iter()
            .find(|n| !n.prefix.is_empty() && n.uri == q.namespace)
            .map(|n| n.prefix.as_str())
            .unwrap_or("!");
        format!("{prefix}:{}", q.local)
    };
    xml.push_str(&format!(" {name}=\"{value}\""));
}
fn write_catalog_qname_text(xml: &mut String, tag: &str, q: &QNameRef) {
    if q.namespace.is_empty() {
        xml.push_str(&format!(
            "<bpmn:{tag} xmlns:bpmn=\"{BPMN}\" xmlns=\"\">{}</bpmn:{tag}>",
            q.local
        ));
    } else {
        xml.push_str(&format!(
            "<bpmn:{tag} xmlns:bpmn=\"{BPMN}\" xmlns:q=\"{}\">q:{}</bpmn:{tag}>",
            esc(&q.namespace),
            q.local
        ));
    }
}
pub(super) fn metadata_context(
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
) -> Option<&[processcraft_core::bpmn_xml::NamespaceBinding]> {
    let base = base?;
    if !base.namespaces.is_empty() {
        return Some(&base.namespaces);
    }
    for doc in &base.documentation {
        if let processcraft_core::bpmn_xml::MixedXml::Structured(content) = &doc.content {
            return Some(&content.namespaces);
        }
    }
    base.extension_elements
        .as_ref()
        .map(|extension| extension.content.namespaces.as_slice())
}
fn catalog_open(
    xml: &mut String,
    local: &str,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
) -> String {
    if let Some(context) = metadata_context(base) {
        let tag = if context.iter().any(|n| n.prefix.is_empty() && n.uri == BPMN) {
            local.to_owned()
        } else {
            let prefix = context
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == BPMN)
                .map(|n| n.prefix.as_str())
                .unwrap_or("!");
            format!("{prefix}:{local}")
        };
        xml.push('<');
        xml.push_str(&tag);
        write_namespace_context(xml, context);
        tag
    } else {
        let tag = format!("bpmn:{local}");
        xml.push('<');
        xml.push_str(&tag);
        xml.push_str(&format!(" xmlns:bpmn=\"{BPMN}\" xmlns=\"\""));
        tag
    }
}
fn catalog_attribute(
    xml: &mut String,
    name: &str,
    q: &QNameRef,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
    generated_prefix: &str,
) {
    if let Some(context) = metadata_context(base) {
        let default = context
            .iter()
            .find(|n| n.prefix.is_empty())
            .map(|n| n.uri.as_str())
            .unwrap_or("");
        let value = if q.namespace == default {
            q.local.to_string()
        } else {
            let prefix = context
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == q.namespace)
                .map(|n| n.prefix.as_str())
                .unwrap_or("!");
            format!("{prefix}:{}", q.local)
        };
        xml.push_str(&format!(" {name}=\"{value}\""));
    } else {
        write_catalog_qname_attribute(xml, name, q, generated_prefix);
    }
}
pub(super) fn write_catalog(
    xml: &mut String,
    catalog: &processcraft_core::bpmn_catalog::SupportingCatalog,
    spans: &mut crate::root_order_export::RootSpans,
) -> Result<(), String> {
    for item in &catalog.item_definitions {
        let start = xml.len();
        let tag = catalog_open(xml, "itemDefinition", item.base.as_ref());
        if let Some(id) = &item.id {
            write_xml_id(xml, id);
        }
        if let Some(q) = &item.structure_ref {
            catalog_attribute(xml, "structureRef", q, item.base.as_ref(), "q");
        }
        if let Some(value) = item.is_collection {
            xml.push_str(&format!(" isCollection=\"{value}\""));
        }
        if let Some(kind) = item.item_kind {
            xml.push_str(&format!(" itemKind=\"{kind:?}\""));
        }
        anonymous_base_attributes(xml, item.id.as_ref(), item.base.as_ref());
        if item.id.as_ref().and_then(ElementIdentity::xml_id).is_none() && item.base.is_some() {
            xml.push('>');
            anonymous_base_children(xml, item.id.as_ref(), item.base.as_ref());
            xml.push_str(&format!("</{tag}>"));
        } else {
            xml.push_str("/>");
        }
        spans.record(item.id.as_ref(), start, xml.len())?;
    }
    for interface in &catalog.interfaces {
        let start = xml.len();
        let tag = catalog_open(xml, "interface", interface.base.as_ref());
        xml.push_str(&format!(" name=\"{}\"", esc(&interface.name)));
        if let Some(id) = &interface.id {
            write_xml_id(xml, id);
        }
        if let Some(q) = &interface.implementation_ref {
            catalog_attribute(xml, "implementationRef", q, interface.base.as_ref(), "q");
        }
        anonymous_base_attributes(xml, interface.id.as_ref(), interface.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, interface.id.as_ref(), interface.base.as_ref());
        for op in &interface.operations {
            let op_tag = catalog_open(xml, "operation", op.base.as_ref());
            xml.push_str(&format!(" name=\"{}\"", esc(&op.name)));
            if let Some(id) = &op.id {
                write_xml_id(xml, id);
            }
            if let Some(q) = &op.implementation_ref {
                catalog_attribute(xml, "implementationRef", q, op.base.as_ref(), "r");
            }
            anonymous_base_attributes(xml, op.id.as_ref(), op.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, op.id.as_ref(), op.base.as_ref());
            write_catalog_qname_text(xml, "inMessageRef", &op.in_message_ref);
            if let Some(q) = &op.out_message_ref {
                write_catalog_qname_text(xml, "outMessageRef", q);
            }
            for q in &op.error_refs {
                write_catalog_qname_text(xml, "errorRef", q);
            }
            xml.push_str(&format!("</{op_tag}>"));
        }
        xml.push_str(&format!("</{tag}>"));
        spans.record(interface.id.as_ref(), start, xml.len())?;
    }
    for resource in &catalog.resources {
        let start = xml.len();
        let tag = catalog_open(xml, "resource", resource.base.as_ref());
        xml.push_str(&format!(" name=\"{}\"", esc(&resource.name)));
        if let Some(id) = &resource.id {
            write_xml_id(xml, id);
        }
        anonymous_base_attributes(xml, resource.id.as_ref(), resource.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, resource.id.as_ref(), resource.base.as_ref());
        for parameter in &resource.parameters {
            let tag = catalog_open(xml, "resourceParameter", parameter.base.as_ref());
            if let Some(id) = &parameter.id {
                write_xml_id(xml, id);
            }
            if let Some(name) = &parameter.name {
                xml.push_str(&format!(" name=\"{}\"", esc(name)));
            }
            if let Some(q) = &parameter.type_ref {
                catalog_attribute(xml, "type", q, parameter.base.as_ref(), "q");
            }
            if let Some(value) = parameter.is_required {
                xml.push_str(&format!(" isRequired=\"{value}\""));
            }
            anonymous_base_attributes(xml, parameter.id.as_ref(), parameter.base.as_ref());
            if parameter
                .id
                .as_ref()
                .and_then(ElementIdentity::xml_id)
                .is_none()
                && parameter.base.is_some()
            {
                xml.push('>');
                anonymous_base_children(xml, parameter.id.as_ref(), parameter.base.as_ref());
                xml.push_str(&format!("</{tag}>"));
            } else {
                xml.push_str("/>");
            }
        }
        xml.push_str(&format!("</{tag}>"));
        spans.record(resource.id.as_ref(), start, xml.len())?;
    }
    Ok(())
}
pub(super) fn write_loop_expression(
    xml: &mut String,
    tag: &str,
    expression: &Expression,
    restore_default: bool,
) {
    if restore_default && matches!(expression, Expression::Legacy(_)) {
        let mut child = String::new();
        write_expression(&mut child, tag, expression, false);
        if let Some(index) = child.find(|c: char| c == '>' || c.is_whitespace()) {
            child.insert_str(index, &format!(" xmlns=\"{BPMN}\""));
        }
        xml.push_str(&child);
    } else {
        write_expression(xml, tag, expression, false);
    }
}
pub(super) fn write_expression(
    xml: &mut String,
    tag: &str,
    expression: &Expression,
    legacy_formal: bool,
) {
    let Expression::Structured(data) = expression else {
        let xsi = if legacy_formal {
            " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" xsi:type=\"bpmn:tFormalExpression\""
        } else {
            ""
        };
        xml.push_str(&format!(
            "<{tag}{xsi}>{}</{tag}>",
            esc(expression.plain_text().unwrap_or(""))
        ));
        return;
    };
    let existing = |uri: &str| {
        data.namespaces
            .iter()
            .find(|(p, u)| !p.is_empty() && p != "xml" && u == uri)
            .map(|(p, _)| p.clone())
    };
    let default = data
        .namespaces
        .iter()
        .find(|(p, _)| p.is_empty())
        .map(|(_, u)| u.as_str())
        .unwrap_or("");
    let tag_name = if default == BPMN {
        tag.to_string()
    } else if let Some(prefix) = existing(BPMN) {
        format!("{prefix}:{tag}")
    } else {
        "!".to_string()
    };
    let xp = existing(XSI);
    xml.push_str(&format!("<{tag_name}"));
    if !data.namespaces.iter().any(|(prefix, _)| prefix.is_empty()) {
        xml.push_str(" xmlns=\"\"");
    }
    for (prefix, uri) in &data.namespaces {
        if prefix.is_empty() {
            xml.push_str(&format!(" xmlns=\"{}\"", esc(uri)));
        } else {
            xml.push_str(&format!(" xmlns:{prefix}=\"{}\"", esc(uri)));
        }
    }
    let type_value = |local: &str| {
        if default == BPMN {
            local.to_owned()
        } else if let Some(prefix) = existing(BPMN) {
            format!("{prefix}:{local}")
        } else {
            "!".to_string()
        }
    };
    if let Some(id) = &data.id {
        write_xml_id(xml, id);
    }
    match &data.kind {
        ExpressionKind::Base { explicit_type } => {
            if *explicit_type {
                xml.push_str(&format!(
                    " {}:type=\"{}\"",
                    xp.as_deref().unwrap_or("!"),
                    type_value("tExpression")
                ));
            }
        }
        ExpressionKind::Formal {
            explicit_type,
            language,
            evaluates_to_type_ref,
        } => {
            if *explicit_type {
                xml.push_str(&format!(
                    " {}:type=\"{}\"",
                    xp.as_deref().unwrap_or("!"),
                    type_value("tFormalExpression")
                ));
            }
            if let Some(language) = language {
                xml.push_str(&format!(" language=\"{}\"", esc(language)));
            }
            if let Some(reference) = evaluates_to_type_ref {
                if reference.namespace.is_empty() {
                    if data
                        .namespaces
                        .iter()
                        .any(|(p, n)| p.is_empty() && !n.is_empty())
                    {
                        xml.push_str(" evaluatesToTypeRef=\"!\"");
                    } else {
                        xml.push_str(&format!(" evaluatesToTypeRef=\"{}\"", reference.local));
                    }
                } else if reference.namespace == default {
                    xml.push_str(&format!(" evaluatesToTypeRef=\"{}\"", reference.local));
                } else if let Some(prefix) = existing(&reference.namespace) {
                    xml.push_str(&format!(
                        " evaluatesToTypeRef=\"{prefix}:{}\"",
                        reference.local
                    ));
                } else {
                    xml.push_str(" evaluatesToTypeRef=\"!\"");
                }
            }
        }
    }
    xml.push('>');
    match &data.text {
        processcraft_core::bpmn_model::ExpressionBody::Text(text) => xml.push_str(&esc(text)),
        processcraft_core::bpmn_model::ExpressionBody::Mixed { parts } => {
            use processcraft_core::bpmn_model::ExpressionPart;
            for part in parts {
                match part {
                    ExpressionPart::Text { text } => xml.push_str(&esc(text)),
                    ExpressionPart::Documentation { documentation } => {
                        write_structured_documentation(xml, documentation)
                    }
                    ExpressionPart::ExtensionElements { extension_elements } => {
                        write_extension_elements(xml, extension_elements);
                    }
                    ExpressionPart::Comment { text } => xml.push_str(&format!("<!--{text}-->")),
                    ExpressionPart::ProcessingInstruction { target, data } => {
                        xml.push_str(&format!("<?{target}"));
                        if let Some(data) = data {
                            xml.push(' ');
                            xml.push_str(data);
                        }
                        xml.push_str("?>");
                    }
                }
            }
        }
    }
    xml.push_str(&format!("</{tag_name}>"));
}
pub(super) fn di_owner_prefix<'a>(
    metadata: Option<&'a processcraft_core::model::DiElementMetadata>,
    fallback: &'a str,
) -> &'a str {
    metadata
        .and_then(|m| {
            m.namespaces
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == BPMNDI)
                .map(|n| n.prefix.as_str())
        })
        .unwrap_or(fallback)
}
pub(super) fn write_di_attributes(
    xml: &mut String,
    metadata: Option<&processcraft_core::model::DiElementMetadata>,
) {
    if let Some(metadata) = metadata {
        write_namespace_context(xml, &metadata.namespaces);
        for attr in &metadata.foreign_attributes {
            xml.push(' ');
            if let Some(prefix) = &attr.prefix {
                xml.push_str(&prefix.to_string());
                xml.push(':');
            }
            xml.push_str(&attr.local.to_string());
            xml.push_str(&format!("=\"{}\"", esc(&attr.value)));
        }
    }
}
pub(super) fn write_di_extension(
    xml: &mut String,
    metadata: Option<&processcraft_core::model::DiElementMetadata>,
) {
    if let Some(content) = metadata.and_then(|m| m.extension.as_ref()) {
        let tag = if content
            .namespaces
            .iter()
            .any(|n| n.prefix.is_empty() && n.uri == DI)
        {
            "extension".to_owned()
        } else {
            format!(
                "{}:extension",
                content
                    .namespaces
                    .iter()
                    .find(|n| !n.prefix.is_empty() && n.uri == DI)
                    .map(|n| n.prefix.as_str())
                    .unwrap_or("!")
            )
        };
        xml.push('<');
        xml.push_str(&tag);
        write_namespace_context(xml, &content.namespaces);
        xml.push('>');
        write_xml_nodes(xml, &content.nodes);
        xml.push_str(&format!("</{tag}>"));
    }
}
pub(super) fn write_di_qname(
    xml: &mut String,
    name: &str,
    uri: &str,
    local: &XmlId,
    metadata: Option<&processcraft_core::model::DiElementMetadata>,
) {
    if let Some(metadata) = metadata {
        write_context_qname_attribute(
            xml,
            name,
            &QNameRef {
                namespace: uri.to_owned(),
                local: local.clone(),
            },
            &metadata.namespaces,
        );
    } else {
        xml.push_str(&format!(" {name}=\"{local}\""));
    }
}
pub(super) fn write_label(
    xml: &mut String,
    label: &Label,
    namespace: &str,
    document: &ExportContext,
    di_prefix: &str,
) {
    let di_prefix = di_owner_prefix(label.di_metadata.as_ref(), di_prefix);
    xml.push_str(&format!("<{di_prefix}:BPMNLabel"));
    write_di_attributes(xml, label.di_metadata.as_ref());
    if let Some(id) = &label.id {
        write_xml_id(xml, id);
    }
    if let Some(reference) = &label.style_ref {
        let (uri, local) = match reference {
            DiStyleRef::Local(selector) => {
                (namespace, xml_reference(document, selector).to_string())
            }
            DiStyleRef::Qualified(q) => (q.namespace.as_str(), q.local.to_string()),
        };
        if let Some(metadata) = &label.di_metadata {
            if let Ok(local) = local.parse() {
                write_di_qname(xml, "labelStyle", uri, &local, Some(metadata));
            }
        } else {
            xml.push_str(&format!(
                " xmlns=\"{}\" labelStyle=\"{}\"",
                esc(uri),
                esc(&local)
            ));
        }
    }
    xml.push('>');
    write_di_extension(xml, label.di_metadata.as_ref());
    if let Some(b) = &label.bounds {
        xml.push_str(&format!(
            "<dc:Bounds xmlns:dc=\"{DC}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            b.x, b.y, b.width, b.height
        ));
    }
    xml.push_str(&format!("</{di_prefix}:BPMNLabel>"));
}

pub(super) fn write_doc(
    xml: &mut String,
    docs: &processcraft_core::bpmn_metadata::DocumentationSet,
    extras: Option<&processcraft_core::bpmn_metadata::BaseExtras>,
) {
    match docs {
        processcraft_core::bpmn_metadata::DocumentationSet::Legacy(text) if !text.is_empty() => {
            xml.push_str(&format!(
                "<documentation xmlns=\"{BPMN}\">{}</documentation>",
                esc(text)
            ));
        }
        processcraft_core::bpmn_metadata::DocumentationSet::Structured(docs) => {
            for doc in docs {
                write_structured_documentation(xml, doc);
            }
        }
        _ => {}
    }
    if let Some(extension) = extras.and_then(|extra| extra.extension_elements.as_ref()) {
        write_extension_elements(xml, extension);
    }
}
