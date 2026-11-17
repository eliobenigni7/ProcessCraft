//! Owner metadata passes that operate on already serialized XML text, not on the model:
//! collecting pending base metadata, restoring owner defaults and inserting owner metadata.
use crate::interchange::export::xml::{
    esc, write_extension_elements, write_namespace_context, write_structured_documentation,
};
use crate::interchange::shared::{ANONYMOUS_CONTEXT_MARKER, PendingBase};
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{Expression, LoopCharacteristics, NodeDetails};
use processcraft_core::ids::ElementIdentity;
use processcraft_core::model::*;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::name::{QName, ResolveResult};
use quick_xml::reader::NsReader;
use std::collections::BTreeMap;
// Process documentation is fully represented; reject all other common metadata until each
// owner has a qualified serializer, rather than silently discarding native fields.
pub(in crate::interchange) fn collect_owner_metadata(
    document: &BpmnDocument,
) -> Result<BTreeMap<String, PendingBase>, ()> {
    let mut remaining = document.clone();
    let mut pending = BTreeMap::new();
    let mut invalid = false;
    invalid |= remaining
        .extensions
        .iter()
        .any(|e| e.must_understand == Some(true));
    invalid |= remaining
        .relationships
        .iter()
        .any(|r| r.sources.is_empty() || r.targets.is_empty());
    remaining.extensions.clear();
    if let Some(definitions) = &mut remaining.definitions {
        definitions.namespaces.clear();
        definitions.foreign_attributes.clear();
    }
    processcraft_core::metadata_visit::visit_base_metadata_mut(&mut remaining, |owner, base| {
        if let Some(base) = base.take()
            && let Some(id) = owner.and_then(ElementIdentity::xml_id)
        {
            let value = PendingBase {
                docs: base.documentation,
                extras: processcraft_core::bpmn_metadata::BaseExtras {
                    namespaces: base.namespaces,
                    foreign_attributes: base.foreign_attributes,
                    extension_elements: base.extension_elements,
                },
            };
            invalid |= pending.insert(id.to_string(), value).is_some();
        }
    });
    processcraft_core::metadata_visit::visit_legacy_metadata_mut(
        &mut remaining,
        |owner, docs, extras| {
            *docs = processcraft_core::bpmn_metadata::DocumentationSet::Legacy(String::new());
            if let Some(mut extras) = extras.take() {
                // Legacy owners write the extension after their own documentation.
                extras.extension_elements = None;
                if extras.namespaces.is_empty() && extras.foreign_attributes.is_empty() {
                    return;
                }
                if let Some(id) = owner.xml_id() {
                    invalid |= pending
                        .insert(
                            id.to_string(),
                            PendingBase {
                                docs: Vec::new(),
                                extras,
                            },
                        )
                        .is_some();
                }
            }
        },
    );
    processcraft_core::metadata_visit::visit_expressions_mut(&mut remaining, |expression| {
        if let Expression::Structured(data) = expression {
            data.text = processcraft_core::bpmn_model::ExpressionBody::Text(String::new());
        }
    });
    remaining.relationships.clear();
    fn clear_mixed(nodes: &mut [Node]) {
        use processcraft_core::bpmn_xml::MixedXml;
        for node in nodes {
            if let Some(loop_characteristics) = &mut node.loop_characteristics {
                match loop_characteristics {
                    LoopCharacteristics::Standard { id, .. }
                    | LoopCharacteristics::MultiInstance { id, .. } => *id = None,
                }
            }
            if let Some(body @ MixedXml::Structured(_)) =
                node.activity.as_mut().and_then(|a| a.script.as_mut())
            {
                *body = MixedXml::Text(String::new());
            }
            if let Some(NodeDetails::TextAnnotation {
                text: Some(body @ MixedXml::Structured(_)),
                ..
            }) = &mut node.details
            {
                *body = MixedXml::Text(String::new());
            }
            if let Some(NodeDetails::SubProcess { content }) = &mut node.details {
                clear_mixed(&mut content.nodes);
            }
        }
    }
    for process in &mut remaining.processes {
        clear_mixed(&mut process.nodes);
    }
    for extras in remaining
        .collaboration
        .iter_mut()
        .chain(&mut remaining.additional_collaborations)
        .filter_map(|c| c.extras.as_mut())
        .chain(
            remaining
                .choreographies
                .iter_mut()
                .filter_map(|c| c.extras.as_mut()),
        )
    {
        for artifact in &mut extras.artifacts {
            if let processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(
                node,
            ) = artifact
            {
                clear_mixed(std::slice::from_mut(node.as_mut()));
            }
        }
    }
    for task in &mut remaining.global_tasks {
        if let processcraft_core::bpmn_activity::GlobalTaskKind::Script {
            script: Some(body @ processcraft_core::bpmn_xml::MixedXml::Structured(_)),
            ..
        } = &mut task.kind
        {
            *body = processcraft_core::bpmn_xml::MixedXml::Text(String::new());
        }
    }
    if invalid || remaining.has_semantic_common_metadata_features() {
        Err(())
    } else {
        Ok(pending)
    }
}
pub(super) fn restore_owner_defaults(
    xml: &str,
    pending: &BTreeMap<String, PendingBase>,
) -> Option<String> {
    struct Frame {
        name: Option<String>,
        changed: bool,
        opaque: bool,
    }
    let mut reader = NsReader::from_reader(xml.as_bytes());
    let mut stack: Vec<Frame> = Vec::new();
    let mut edits = Vec::new();
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event().ok()?;
        let end = usize::try_from(reader.buffer_position()).ok()?;
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let empty = xml.get(start..end)?.ends_with("/>");
                let (ns, local) = reader.resolver().resolve_element(element.name());
                let is_bpmn = matches!(ns, ResolveResult::Bound(uri) if uri.0 == BPMN);
                let local_name = local.as_ref().to_owned();
                let parent_changed = stack.last().is_some_and(|frame| frame.changed);
                let inside_opaque = stack.last().is_some_and(|frame| frame.opaque);
                let opaque = inside_opaque
                    || (is_bpmn
                        && matches!(
                            local_name.as_str(),
                            "documentation" | "extensionElements" | "script" | "text"
                        ));
                let mut id = None;
                let mut has_default = false;
                let mut ordinary = Vec::new();
                let mut anonymous_context = None;
                for attribute in element.attributes() {
                    let attribute = attribute.ok()?;
                    let key = attribute.key.0;
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned();
                    if key == "id" {
                        id = Some(value.clone());
                    }
                    if key == ANONYMOUS_CONTEXT_MARKER && is_bpmn && !inside_opaque {
                        if value.len() % 2 != 0 {
                            return None;
                        }
                        let bytes = value
                            .as_bytes()
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|pair| {
                                std::str::from_utf8(pair)
                                    .ok()
                                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                            })
                            .collect::<Option<Vec<_>>>()?;
                        anonymous_context = Some(
                            serde_json::from_slice::<
                                Vec<processcraft_core::bpmn_xml::NamespaceBinding>,
                            >(&bytes)
                            .ok()?,
                        );
                        continue;
                    }
                    if key == "xmlns" {
                        has_default = true;
                    }
                    if key != "xmlns" && !key.starts_with("xmlns:") {
                        ordinary.push((key.to_owned(), value));
                    }
                }
                let metadata = id.as_ref().and_then(|id| pending.get(id));
                let context = anonymous_context.as_deref().or_else(|| {
                    metadata
                        .map(|m| m.extras.namespaces.as_slice())
                        .filter(|c| !c.is_empty())
                });
                let original_default = match reader.resolver().resolve_element(QName("scope")).0 {
                    ResolveResult::Bound(uri) => uri.0.to_owned(),
                    _ => String::new(),
                };
                let desired_default = context
                    .and_then(|c| c.iter().find(|n| n.prefix.is_empty()))
                    .map(|n| n.uri.as_str())
                    .unwrap_or(&original_default);
                let rewrite = is_bpmn
                    && context.is_some()
                    && (anonymous_context.is_some()
                        || desired_default != original_default
                        || (parent_changed && desired_default != BPMN));
                let mut renamed = None;
                let mut changed = false;
                if rewrite {
                    let context = context?;
                    let tag = if desired_default == BPMN {
                        local_name.clone()
                    } else {
                        let prefix = context
                            .iter()
                            .find(|n| !n.prefix.is_empty() && n.uri == BPMN)?
                            .prefix
                            .as_str();
                        format!("{prefix}:{local_name}")
                    };
                    let mut replacement = format!("<{tag}");
                    write_namespace_context(&mut replacement, context);
                    for (key, mut value) in ordinary {
                        let qname = matches!(
                            key.as_str(),
                            "structureRef"
                                | "implementationRef"
                                | "itemRef"
                                | "itemSubjectRef"
                                | "messageRef"
                                | "signalRef"
                                | "errorRef"
                                | "escalationRef"
                                | "operationRef"
                                | "dataStoreRef"
                                | "activityRef"
                                | "categoryValueRef"
                                | "calledElement"
                                | "processRef"
                                | "initiatingParticipantRef"
                                | "attachedToRef"
                        ) || (key == "type" && local_name == "resourceParameter")
                            || (matches!(key.as_str(), "sourceRef" | "targetRef")
                                && local_name != "sequenceFlow");
                        if qname {
                            let (resolved, name) =
                                reader.resolver().resolve_element(QName(value.trim()));
                            let uri = match resolved {
                                ResolveResult::Bound(uri) => uri.0,
                                ResolveResult::Unbound => "",
                                ResolveResult::Unknown(_) => return None,
                            };
                            value = if uri == desired_default {
                                name.as_ref().to_owned()
                            } else {
                                let prefix = context
                                    .iter()
                                    .find(|n| !n.prefix.is_empty() && n.uri == uri)?
                                    .prefix
                                    .as_str();
                                format!("{prefix}:{}", name.as_ref())
                            };
                        }
                        replacement.push_str(&format!(" {key}=\"{}\"", esc(&value)));
                    }
                    replacement.push_str(if empty { "/>" } else { ">" });
                    edits.push((start, end, replacement));
                    renamed = Some(tag);
                    changed = desired_default != original_default;
                } else if parent_changed
                    && is_bpmn
                    && !has_default
                    && !matches!(local_name.as_str(), "documentation" | "extensionElements")
                {
                    let tag = xml.get(start..end)?;
                    let ending = if empty { "/>" } else { ">" };
                    edits.push((
                        start,
                        end,
                        format!(
                            "{} xmlns=\"{}\"{ending}",
                            tag.strip_suffix(ending)?,
                            esc(&original_default)
                        ),
                    ));
                }
                if !empty {
                    stack.push(Frame {
                        name: renamed,
                        changed,
                        opaque,
                    });
                }
            }
            Event::End(_) => {
                let frame = stack.pop()?;
                if let Some(name) = frame.name {
                    edits.push((start, end, format!("</{name}>")));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut result = xml.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    Some(result)
}
pub(super) fn insert_owner_metadata(
    xml: &str,
    mut pending: BTreeMap<String, PendingBase>,
) -> Option<String> {
    if pending.is_empty() {
        return Some(xml.to_owned());
    }
    let mut reader = NsReader::from_reader(xml.as_bytes());
    let mut edits = Vec::new();
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event().ok()?;
        let end = usize::try_from(reader.buffer_position()).ok()?;
        let (element, empty) = match event {
            Event::Start(e) => (e, false),
            Event::Empty(e) => (e, true),
            Event::Eof => break,
            _ => continue,
        };
        let (namespace, _) = reader.resolver().resolve_element(element.name());
        if !matches!(namespace, ResolveResult::Bound(uri) if uri.0 == BPMN) {
            continue;
        }
        let mut owner_id = None;
        let mut declared = BTreeMap::new();
        let mut ordinary = Vec::new();
        for attribute in element.attributes() {
            let attribute = attribute.ok()?;
            let key = attribute.key.0;
            if key == "id" {
                owner_id = Some(
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned(),
                );
            }
            if let Some(prefix) = key.strip_prefix("xmlns:") {
                declared.insert(
                    prefix.to_owned(),
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned(),
                );
            } else if key == "xmlns" {
                declared.insert(
                    String::new(),
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned(),
                );
            } else {
                ordinary.push((
                    key.to_owned(),
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned(),
                ));
            }
        }
        let Some(metadata) = owner_id.as_ref().and_then(|id| pending.remove(id)) else {
            continue;
        };
        if metadata.docs.is_empty()
            && metadata.extras.namespaces.is_empty()
            && metadata.extras.foreign_attributes.is_empty()
            && metadata.extras.extension_elements.is_none()
        {
            return None;
        }
        let mut attributes = String::new();
        let owner_name = element.name().0.to_owned();
        let owner_prefix = owner_name.split_once(':').map(|(p, _)| p);
        let needs_rewrite = empty
            && !metadata.extras.namespaces.is_empty()
            && metadata.extras.namespaces.iter().any(|binding| {
                let probe = if binding.prefix.is_empty() {
                    "scope".to_owned()
                } else {
                    format!("{}:scope", binding.prefix)
                };
                let actual = match reader.resolver().resolve_element(QName(&probe)).0 {
                    ResolveResult::Bound(uri) => uri.0,
                    _ => "",
                };
                actual != binding.uri
            });
        if needs_rewrite {
            let context = &metadata.extras.namespaces;
            let default = context
                .iter()
                .find(|n| n.prefix.is_empty())
                .map(|n| n.uri.as_str())
                .unwrap_or("");
            let local_name = owner_name.rsplit(':').next()?;
            let tag = if default == BPMN {
                local_name.to_owned()
            } else {
                let prefix = context
                    .iter()
                    .find(|n| !n.prefix.is_empty() && n.uri == BPMN)?
                    .prefix
                    .as_str();
                format!("{prefix}:{local_name}")
            };
            let mut replacement = format!("<{tag}");
            write_namespace_context(&mut replacement, context);
            for (key, mut value) in ordinary {
                let qname_attribute = matches!(
                    key.as_str(),
                    "structureRef"
                        | "implementationRef"
                        | "itemRef"
                        | "itemSubjectRef"
                        | "messageRef"
                        | "signalRef"
                        | "errorRef"
                        | "escalationRef"
                        | "operationRef"
                        | "dataStoreRef"
                        | "activityRef"
                        | "categoryValueRef"
                        | "calledElement"
                        | "processRef"
                        | "initiatingParticipantRef"
                        | "attachedToRef"
                ) || (key == "type" && local_name == "resourceParameter")
                    || (matches!(key.as_str(), "sourceRef" | "targetRef")
                        && local_name != "sequenceFlow");
                if qname_attribute {
                    let (resolved, local) = reader.resolver().resolve_element(QName(value.trim()));
                    let uri = match resolved {
                        ResolveResult::Bound(uri) => uri.0,
                        ResolveResult::Unbound => "",
                        ResolveResult::Unknown(_) => return None,
                    };
                    value = if uri == default {
                        local.as_ref().to_owned()
                    } else {
                        let prefix = context
                            .iter()
                            .find(|n| !n.prefix.is_empty() && n.uri == uri)?
                            .prefix
                            .as_str();
                        format!("{prefix}:{}", local.as_ref())
                    };
                }
                replacement.push_str(&format!(" {key}=\"{}\"", esc(&value)));
            }
            for attribute in &metadata.extras.foreign_attributes {
                let prefix = attribute.prefix.as_ref()?;
                replacement.push_str(&format!(
                    " {prefix}:{}=\"{}\"",
                    attribute.local,
                    esc(&attribute.value)
                ));
            }
            replacement.push('>');
            for doc in &metadata.docs {
                write_structured_documentation(&mut replacement, doc);
            }
            if let Some(extension) = &metadata.extras.extension_elements {
                write_extension_elements(&mut replacement, extension);
            }
            replacement.push_str(&format!("</{tag}>"));
            edits.push((start, end, replacement));
            continue;
        }
        for binding in &metadata.extras.namespaces {
            if binding.prefix == "xml" {
                continue;
            }
            let probe = if binding.prefix.is_empty() {
                "scope".to_owned()
            } else {
                format!("{}:scope", binding.prefix)
            };
            let actual = match reader.resolver().resolve_element(QName(&probe)).0 {
                ResolveResult::Bound(uri) => uri.0,
                ResolveResult::Unbound => "",
                ResolveResult::Unknown(_) => "",
            };
            if actual == binding.uri {
                continue;
            }
            if binding.prefix.is_empty()
                || owner_prefix == Some(binding.prefix.as_str())
                || declared.contains_key(&binding.prefix)
            {
                return None;
            }
            attributes.push_str(&format!(
                " xmlns:{}=\"{}\"",
                binding.prefix,
                esc(&binding.uri)
            ));
        }
        for attribute in &metadata.extras.foreign_attributes {
            let prefix = attribute.prefix.as_ref()?;
            attributes.push_str(&format!(
                " {prefix}:{}=\"{}\"",
                attribute.local,
                esc(&attribute.value)
            ));
        }
        let mut body = String::new();
        for doc in &metadata.docs {
            write_structured_documentation(&mut body, doc);
        }
        if let Some(extension) = &metadata.extras.extension_elements {
            write_extension_elements(&mut body, extension);
        }
        let tag = xml.get(start..end)?;
        let ending = if empty { "/>" } else { ">" };
        let opening = tag.strip_suffix(ending)?;
        let replacement = if empty {
            format!("{opening}{attributes}>{body}</{owner_name}>")
        } else {
            format!("{opening}{attributes}>{body}")
        };
        edits.push((start, end, replacement));
    }
    if !pending.is_empty() {
        return None;
    }
    let mut result = xml.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    Some(result)
}
