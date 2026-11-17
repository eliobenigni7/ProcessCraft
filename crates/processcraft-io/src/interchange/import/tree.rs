//! Owned XML element tree, its builder, and the attribute helpers used by every parser.
use crate::interchange::import::metadata::is_expression;
use crate::interchange::refusal::{at, refused};
use crate::interchange::shared::PendingBase;
use crate::ns::{BPMN, BPMNDI, DC, DI};
use processcraft_core::bpmn_model::QNameRef;
use processcraft_core::ids::{ElementIdentity, ElementSelector, XmlId};
use processcraft_core::model::*;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::name::{QName, ResolveResult};
use quick_xml::reader::NsReader;
use std::collections::BTreeMap;
#[derive(Default)]
pub(in crate::interchange) struct Elem {
    pub(in crate::interchange) ns: String,
    pub(in crate::interchange) name: String,
    pub(in crate::interchange) attrs: BTreeMap<String, String>,
    pub(in crate::interchange) children: Vec<Elem>,
    pub(in crate::interchange) text: String,
    pub(in crate::interchange) namespaces: BTreeMap<String, String>,
    pub(in crate::interchange) type_ref: Option<QNameRef>,
    pub(in crate::interchange) catalog_qnames: BTreeMap<String, QNameRef>,
    pub(in crate::interchange) catalog_text_qname: Option<QNameRef>,
    pub(in crate::interchange) content: Vec<ElemContent>,
    pub(in crate::interchange) opaque: bool,
    pub(in crate::interchange) prefix: Option<String>,
    pub(in crate::interchange) xml_attributes: Vec<processcraft_core::bpmn_xml::XmlAttribute>,
    pub(in crate::interchange) foreign_attributes: Vec<processcraft_core::bpmn_xml::XmlAttribute>,
    pub(in crate::interchange) anonymous_base: Option<PendingBase>,
    pub(in crate::interchange) anonymous_base_consumed: std::cell::Cell<bool>,
    pub(in crate::interchange) flow_metadata:
        Option<processcraft_core::bpmn_activity::FlowElementMetadata>,
    pub(in crate::interchange) flow_metadata_consumed: std::cell::Cell<bool>,
}
pub(in crate::interchange) enum ElemContent {
    Text(String),
    Child(usize),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: Option<String>,
    },
}
pub(super) fn xml_namespaces(e: &Elem) -> Vec<processcraft_core::bpmn_xml::NamespaceBinding> {
    e.namespaces
        .iter()
        .map(
            |(prefix, uri)| processcraft_core::bpmn_xml::NamespaceBinding {
                prefix: prefix.clone(),
                uri: uri.clone(),
            },
        )
        .collect()
}
pub(super) fn xml_content(e: &Elem) -> Option<processcraft_core::bpmn_xml::XmlContent> {
    use processcraft_core::bpmn_xml::{XmlContent, XmlElement, XmlNode};
    let mut nodes = Vec::new();
    for part in &e.content {
        let node = match part {
            ElemContent::Text(text) => XmlNode::Text { text: text.clone() },
            ElemContent::Comment(text) => XmlNode::Comment { text: text.clone() },
            ElemContent::ProcessingInstruction { target, data } => XmlNode::ProcessingInstruction {
                target: target.parse().ok()?,
                data: data.clone(),
            },
            ElemContent::Child(index) => {
                let child = e.children.get(*index)?;
                XmlNode::Element {
                    element: XmlElement {
                        namespace: child.ns.clone(),
                        local: child.name.parse().ok()?,
                        prefix: child.prefix.as_ref().map(|p| p.parse()).transpose().ok()?,
                        namespaces: xml_namespaces(child),
                        attributes: child.xml_attributes.clone(),
                        children: xml_content(child)?.nodes,
                    },
                }
            }
        };
        if let XmlNode::Text { text } = &node
            && let Some(XmlNode::Text { text: previous }) = nodes.last_mut()
        {
            previous.push_str(text);
        } else {
            nodes.push(node);
        }
    }
    Some(XmlContent {
        namespaces: xml_namespaces(e),
        nodes,
    })
}
pub(super) fn structured_documentation(
    e: &Elem,
) -> Option<processcraft_core::bpmn_metadata::Documentation> {
    use processcraft_core::{bpmn_metadata::Documentation, bpmn_xml::MixedXml};
    if e.ns != BPMN
        || e.name != "documentation"
        || !attrs(e, &["id", "textFormat"])
        || e.children.len() > 1
    {
        return None;
    }
    let content = MixedXml::Structured(xml_content(e)?);
    processcraft_core::bpmn_xml::validate_mixed(&content).ok()?;
    Some(Documentation {
        id: optional_id(e)?,
        text_format: e.attrs.get("textFormat").cloned(),
        content,
    })
}
pub(super) fn mixed_body(
    e: &Elem,
    preserve_context: bool,
) -> Option<processcraft_core::bpmn_xml::MixedXml> {
    use processcraft_core::bpmn_xml::{MixedXml, validate_mixed};
    if !attrs(e, &[]) || e.children.len() > 1 {
        return None;
    }
    let body = if (!preserve_context
        || e.namespaces.len() == 1 && e.namespaces.get("").is_some_and(|v| v == BPMN))
        && e.content
            .iter()
            .all(|part| matches!(part, ElemContent::Text(_)))
    {
        MixedXml::Text(e.text.clone())
    } else {
        MixedXml::Structured(xml_content(e)?)
    };
    validate_mixed(&body).ok()?;
    Some(body)
}
pub(super) fn extension_elements(
    e: &Elem,
) -> Option<processcraft_core::bpmn_metadata::ExtensionElements> {
    if e.ns != BPMN || e.name != "extensionElements" || !attrs(e, &[]) {
        return None;
    }
    let content = xml_content(e)?;
    processcraft_core::bpmn_xml::validate_extension(&content).ok()?;
    Some(processcraft_core::bpmn_metadata::ExtensionElements { content })
}
pub(in crate::interchange) fn tree(bytes: &[u8]) -> Option<Elem> {
    let mut r = NsReader::from_reader(bytes);
    r.config_mut().expand_empty_elements = true;
    let mut stack: Vec<Elem> = vec![];
    let mut root = None;
    let mut target_namespace: Option<String> = None;
    loop {
        match r.read_event().ok()? {
            Event::Start(e) => {
                let opaque = stack.last().is_some_and(|parent| {
                    parent.opaque
                        || (parent.ns == DI && parent.name == "extension")
                        || (parent.ns == BPMN
                            && matches!(
                                parent.name.as_str(),
                                "documentation" | "script" | "text" | "extensionElements"
                            ))
                });
                let (ns, local) = r.resolver().resolve_element(e.name());
                let ns = match ns {
                    ResolveResult::Bound(n) => n.0.to_string(),
                    ResolveResult::Unbound if opaque => String::new(),
                    _ => return None,
                };
                let mut item = Elem {
                    ns,
                    name: local.as_ref().into(),
                    opaque,
                    prefix: e
                        .name()
                        .0
                        .split_once(':')
                        .map(|(prefix, _)| prefix.to_owned()),
                    ..Elem::default()
                };
                at(&item);
                item.namespaces = stack
                    .last()
                    .map(|parent| parent.namespaces.clone())
                    .unwrap_or_default();
                item.namespaces.entry(String::new()).or_default();
                for a in e.attributes() {
                    let a = a.ok()?;
                    if a.key.as_namespace_binding().is_some() {
                        let raw = a.key.0;
                        let prefix = raw.strip_prefix("xmlns:").unwrap_or("");
                        let value = a
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()?
                            .into_owned();
                        item.namespaces.insert(prefix.into(), value);
                    }
                }
                for a in e.attributes() {
                    let a = a.ok()?;
                    if a.key.as_namespace_binding().is_some() {
                        continue;
                    }
                    let (ns, local) = r.resolver().resolve_attribute(a.key);
                    let key = local.as_ref();
                    if opaque {
                        let namespace = match ns {
                            ResolveResult::Bound(uri) => uri.0.to_owned(),
                            ResolveResult::Unbound => String::new(),
                            ResolveResult::Unknown(_) => return None,
                        };
                        item.xml_attributes
                            .push(processcraft_core::bpmn_xml::XmlAttribute {
                                namespace,
                                local: key.parse().ok()?,
                                prefix: a
                                    .key
                                    .0
                                    .split_once(':')
                                    .map(|(p, _)| p.parse())
                                    .transpose()
                                    .ok()?,
                                value: a
                                    .normalized_value(XmlVersion::Implicit1_0)
                                    .ok()?
                                    .into_owned(),
                            });
                        continue;
                    }
                    if let ResolveResult::Bound(uri) = ns {
                        if item.ns == BPMN
                            && item.name == "rootElement"
                            && uri.0 == "http://www.w3.org/2001/XMLSchema-instance"
                            && key == "type"
                        {
                            let value = a
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()?
                                .into_owned();
                            let (resolved, name) = r.resolver().resolve_element(QName(&value));
                            if !matches!(resolved, ResolveResult::Bound(uri) if uri.0 == BPMN)
                                || name.as_ref() != "tCallableElement"
                            {
                                return None;
                            }
                            item.attrs
                                .insert("xsi:type".into(), "tCallableElement".into());
                            continue;
                        }
                        if item.ns == BPMN
                            && item.name == "partitionElement"
                            && uri.0 == "http://www.w3.org/2001/XMLSchema-instance"
                            && key == "type"
                        {
                            let value = a
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()?
                                .into_owned();
                            let (resolved, name) = r.resolver().resolve_element(QName(&value));
                            if !matches!(resolved, ResolveResult::Bound(uri) if uri.0 == BPMN)
                                || name.as_ref() != "tCategoryValue"
                            {
                                return None;
                            }
                            item.attrs
                                .insert("xsi:type".into(), "tCategoryValue".into());
                            continue;
                        }
                        if item.ns != BPMN
                            || !is_expression(&item.name)
                            || uri.0 != "http://www.w3.org/2001/XMLSchema-instance"
                            || key != "type"
                        {
                            item.foreign_attributes.push(
                                processcraft_core::bpmn_xml::XmlAttribute {
                                    namespace: uri.0.to_owned(),
                                    local: key.parse().ok()?,
                                    prefix: a
                                        .key
                                        .0
                                        .split_once(':')
                                        .map(|(p, _)| p.parse())
                                        .transpose()
                                        .ok()?,
                                    value: a
                                        .normalized_value(XmlVersion::Implicit1_0)
                                        .ok()?
                                        .into_owned(),
                                },
                            );
                            continue;
                        }
                        let value = a
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()?
                            .into_owned();
                        let (resolved, name) = r.resolver().resolve_element(QName(&value));
                        if !matches!(resolved, ResolveResult::Bound(uri) if uri.0==BPMN)
                            || !matches!(name.as_ref(), "tExpression" | "tFormalExpression")
                        {
                            return None;
                        }
                        item.attrs.insert("xsi:type".into(), name.as_ref().into());
                        continue;
                    } else if !matches!(ns, ResolveResult::Unbound) {
                        return None;
                    }
                    let mut value = a
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned();
                    if (item.ns == BPMN
                        && matches!(
                            (item.name.as_str(), key),
                            ("itemDefinition", "structureRef")
                                | ("extension", "definition")
                                | ("interface" | "operation", "implementationRef")
                                | ("resourceParameter", "type")
                                | ("correlationProperty", "type")
                                | ("callActivity", "calledElement")
                                | ("participant", "processRef")
                                | ("group", "categoryValueRef")
                                | (
                                    "messageFlow" | "conversationLink" | "association",
                                    "sourceRef" | "targetRef"
                                )
                                | ("callConversation", "calledCollaborationRef")
                                | ("boundaryEvent", "attachedToRef")
                                | (
                                    "choreographyTask"
                                        | "subChoreography"
                                        | "callChoreography"
                                        | "globalChoreographyTask",
                                    "initiatingParticipantRef"
                                )
                                | ("callChoreography", "calledChoreographyRef")
                                | (
                                    "conversationAssociation",
                                    "innerConversationNodeRef" | "outerConversationNodeRef"
                                )
                                | (
                                    "messageFlowAssociation",
                                    "innerMessageFlowRef" | "outerMessageFlowRef"
                                )
                                | ("process", "definitionalCollaborationRef")
                                | ("lane", "partitionElementRef")
                                | ("ioBinding", "operationRef")
                                | ("resourceParameterBinding", "parameterRef")
                                | ("correlationSubscription", "correlationKeyRef")
                                | ("correlationPropertyBinding", "correlationPropertyRef")
                                | ("correlationPropertyRetrievalExpression", "messageRef")
                                | (
                                    "multiInstanceLoopCharacteristics",
                                    "oneBehaviorEventRef" | "noneBehaviorEventRef"
                                )
                                | ("serviceTask" | "sendTask" | "receiveTask", "operationRef")
                                | ("sendTask" | "receiveTask" | "messageFlow", "messageRef")
                                | ("dataStoreReference", "dataStoreRef")
                                | ("message", "itemRef")
                                | ("signal" | "error" | "escalation", "structureRef")
                                | ("messageEventDefinition", "messageRef")
                                | ("signalEventDefinition", "signalRef")
                                | ("errorEventDefinition", "errorRef")
                                | ("escalationEventDefinition", "escalationRef")
                                | ("compensateEventDefinition", "activityRef")
                                | (
                                    "dataObject"
                                        | "dataObjectReference"
                                        | "dataStore"
                                        | "dataStoreReference"
                                        | "dataInput"
                                        | "dataOutput"
                                        | "inputDataItem"
                                        | "outputDataItem"
                                        | "property",
                                    "itemSubjectRef"
                                )
                        ))
                        || (item.ns == BPMNDI
                            && ((item.name == "BPMNLabel" && key == "labelStyle")
                                || (item.name == "BPMNShape"
                                    && key == "choreographyActivityShape")))
                    {
                        let (resolved, local) = r.resolver().resolve_element(QName(value.trim()));
                        let namespace = match resolved {
                            ResolveResult::Bound(uri) => uri.0.to_string(),
                            ResolveResult::Unbound => String::new(),
                            ResolveResult::Unknown(_) => return None,
                        };
                        item.catalog_qnames.insert(
                            key.into(),
                            QNameRef {
                                namespace,
                                local: local.as_ref().parse().ok()?,
                            },
                        );
                    }
                    if item.ns == BPMN && is_expression(&item.name) && key == "evaluatesToTypeRef" {
                        let (resolved, local) = r.resolver().resolve_element(QName(&value));
                        let namespace = match resolved {
                            ResolveResult::Bound(uri) => uri.0.to_string(),
                            ResolveResult::Unbound => String::new(),
                            ResolveResult::Unknown(_) => return None,
                        };
                        item.type_ref = Some(QNameRef {
                            namespace,
                            local: local.as_ref().parse().ok()?,
                        });
                    }
                    if stack.is_empty() && key == "targetNamespace" {
                        target_namespace = Some(value.clone());
                    }
                    if matches!(
                        (item.ns.as_str(), item.name.as_str(), key),
                        (BPMN, "sequenceFlow", "sourceRef" | "targetRef")
                            | (
                                BPMNDI,
                                "BPMNPlane" | "BPMNShape" | "BPMNEdge",
                                "bpmnElement"
                            )
                            | (BPMNDI, "BPMNEdge", "sourceElement" | "targetElement")
                    ) {
                        if item.name == "sequenceFlow" {
                            if value.contains(':') {
                                return None;
                            }
                            item.attrs.insert(key.into(), value);
                            continue;
                        }
                        // An unprefixed reference names a local ID whatever the default
                        // namespace is (bpmn.io, Signavio and Bizagi all rely on this).
                        if value.trim().contains(':') {
                            let (resolved, _) = r.resolver().resolve_element(QName(value.trim()));
                            if !matches!(resolved, ResolveResult::Bound(uri)
                                if Some(uri.0) == target_namespace.as_deref())
                            {
                                return refused(|| {
                                    format!("{key} \"{value}\" is outside the targetNamespace")
                                });
                            }
                        }
                        value = value
                            .trim()
                            .rsplit_once(':')
                            .map_or(value.trim(), |(_, local)| local)
                            .to_string();
                    }
                    item.attrs.insert(key.into(), value);
                }
                stack.push(item);
            }
            Event::End(_) => {
                let mut item = stack.pop()?;
                if item.ns == BPMN && matches!(item.name.as_str(), "flowNodeRef") {
                    let q = item.text.trim();
                    if item.name == "flowNodeRef" && !q.contains(':') {
                        item.text = q.to_owned();
                    } else {
                        let (resolved, local) = r.resolver().resolve_element(QName(q));
                        let namespace = match resolved {
                            ResolveResult::Bound(uri) => uri.0,
                            ResolveResult::Unbound => "",
                            ResolveResult::Unknown(_) => return None,
                        };
                        if Some(namespace) != target_namespace.as_deref() {
                            return None;
                        }
                        item.text = local.as_ref().into();
                    }
                }
                if item.ns == BPMN
                    && matches!(
                        item.name.as_str(),
                        "participantRef"
                            | "messageFlowRef"
                            | "interfaceRef"
                            | "endPointRef"
                            | "innerParticipantRef"
                            | "outerParticipantRef"
                            | "choreographyRef"
                            | "inMessageRef"
                            | "outMessageRef"
                            | "errorRef"
                            | "eventDefinitionRef"
                            | "operationRef"
                            | "supportedInterfaceRef"
                            | "supports"
                            | "resourceRef"
                            | "correlationPropertyRef"
                            | "categoryValueRef"
                            | "loopDataInputRef"
                            | "loopDataOutputRef"
                            | "incoming"
                            | "outgoing"
                            | "source"
                            | "target"
                    )
                {
                    let (resolved, local) = r.resolver().resolve_element(QName(item.text.trim()));
                    let namespace = match resolved {
                        ResolveResult::Bound(uri) => uri.0.to_string(),
                        ResolveResult::Unbound => String::new(),
                        ResolveResult::Unknown(_) => return None,
                    };
                    item.catalog_text_qname = Some(QNameRef {
                        namespace,
                        local: local.as_ref().parse().ok()?,
                    });
                }
                if let Some(parent) = stack.last_mut() {
                    parent
                        .content
                        .push(ElemContent::Child(parent.children.len()));
                    parent.children.push(item);
                } else if root.replace(item).is_some() {
                    return None;
                }
            }
            Event::Text(t) => {
                if let Some(last) = stack.last_mut() {
                    if last.opaque
                        || (last.ns == DI && last.name == "extension")
                        || last.ns == BPMN
                            && matches!(
                                last.name.as_str(),
                                "documentation"
                                    | "flowNodeRef"
                                    | "participantRef"
                                    | "interfaceRef"
                                    | "endPointRef"
                                    | "innerParticipantRef"
                                    | "outerParticipantRef"
                                    | "choreographyRef"
                                    | "text"
                                    | "dataInputRefs"
                                    | "optionalInputRefs"
                                    | "whileExecutingInputRefs"
                                    | "outputSetRefs"
                                    | "dataOutputRefs"
                                    | "optionalOutputRefs"
                                    | "whileExecutingOutputRefs"
                                    | "inputSetRefs"
                                    | "sourceRef"
                                    | "targetRef"
                                    | "source"
                                    | "target"
                                    | "messageFlowRef"
                                    | "inMessageRef"
                                    | "outMessageRef"
                                    | "errorRef"
                                    | "eventDefinitionRef"
                                    | "operationRef"
                                    | "supportedInterfaceRef"
                                    | "supports"
                                    | "resourceRef"
                                    | "correlationPropertyRef"
                                    | "categoryValueRef"
                                    | "loopDataInputRef"
                                    | "loopDataOutputRef"
                                    | "incoming"
                                    | "outgoing"
                                    | "conditionExpression"
                                    | "condition"
                                    | "timeDate"
                                    | "timeDuration"
                                    | "timeCycle"
                                    | "loopCondition"
                                    | "loopCardinality"
                                    | "completionCondition"
                                    | "activationCondition"
                                    | "transformation"
                                    | "messagePath"
                                    | "dataPath"
                                    | "expression"
                                    | "from"
                                    | "to"
                                    | "script"
                            )
                    {
                        last.text.push_str(t.as_ref());
                        last.content.push(ElemContent::Text(t.as_ref().to_owned()));
                    } else if !t.trim().is_empty() {
                        return None;
                    }
                } else if !t.trim().is_empty() {
                    return None;
                }
            }
            Event::GeneralRef(value) => {
                let last = stack.last_mut()?;
                if !last.opaque
                    && !(last.ns == DI && last.name == "extension")
                    && (last.ns != BPMN
                        || !matches!(
                            last.name.as_str(),
                            "documentation"
                                | "extensionElements"
                                | "conditionExpression"
                                | "condition"
                                | "timeDate"
                                | "timeDuration"
                                | "timeCycle"
                                | "loopCondition"
                                | "loopCardinality"
                                | "completionCondition"
                                | "activationCondition"
                                | "transformation"
                                | "messagePath"
                                | "dataPath"
                                | "expression"
                                | "from"
                                | "to"
                                | "script"
                                | "text"
                        ))
                {
                    return None;
                }
                let c = if let Some(c) = value.resolve_char_ref().ok()? {
                    c
                } else {
                    match value.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => return None,
                    }
                };
                last.text.push(c);
                last.content.push(ElemContent::Text(c.to_string()));
            }
            Event::CData(t) => {
                let last = stack.last_mut()?;
                if !last.opaque
                    && !(last.ns == DI && last.name == "extension")
                    && (last.ns != BPMN
                        || !matches!(
                            last.name.as_str(),
                            "documentation"
                                | "conditionExpression"
                                | "condition"
                                | "timeDate"
                                | "timeDuration"
                                | "timeCycle"
                                | "loopCondition"
                                | "loopCardinality"
                                | "completionCondition"
                                | "activationCondition"
                                | "transformation"
                                | "messagePath"
                                | "dataPath"
                                | "expression"
                                | "from"
                                | "to"
                                | "script"
                                | "text"
                        ))
                {
                    return None;
                }
                last.text.push_str(t.as_ref());
                last.content.push(ElemContent::Text(t.as_ref().to_owned()));
            }
            Event::PI(pi) => {
                let last = stack.last_mut()?;
                if !last.opaque
                    && !(last.ns == DI && last.name == "extension")
                    && !(last.ns == BPMN
                        && (matches!(last.name.as_str(), "documentation" | "script" | "text")
                            || is_expression(&last.name)))
                {
                    return None;
                }
                let data = pi
                    .content()
                    .strip_prefix(char::is_whitespace)
                    .unwrap_or(pi.content());
                last.content.push(ElemContent::ProcessingInstruction {
                    target: pi.target().to_owned(),
                    data: (!data.is_empty()).then(|| data.to_owned()),
                });
            }
            Event::Comment(comment) => {
                if let Some(last) = stack.last_mut() {
                    last.content
                        .push(ElemContent::Comment(comment.as_ref().to_owned()));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    root
}
pub(super) fn attrs(e: &Elem, allowed: &[&str]) -> bool {
    e.foreign_attributes.is_empty() && e.attrs.keys().all(|k| allowed.contains(&k.as_str()))
}
fn id(e: &Elem, key: &str) -> Option<XmlId> {
    e.attrs.get(key)?.parse().ok()
}
pub(super) fn number(e: &Elem, key: &str) -> Option<f64> {
    e.attrs.get(key)?.parse().ok()
}
pub(super) fn bounds(e: &Elem) -> Option<Bounds> {
    if e.ns != DC
        || e.name != "Bounds"
        || !attrs(e, &["x", "y", "width", "height"])
        || !e.children.is_empty()
    {
        return None;
    }
    Some(Bounds {
        x: number(e, "x")?,
        y: number(e, "y")?,
        width: number(e, "width")?,
        height: number(e, "height")?,
    })
}
pub(super) fn di_attrs(e: &Elem, allowed: &[&str]) -> bool {
    e.attrs.keys().all(|key| allowed.contains(&key.as_str()))
}
pub(super) fn optional_bool(value: Option<&String>) -> Option<Option<bool>> {
    match value.map(String::as_str) {
        None => Some(None),
        Some("true" | "1") => Some(Some(true)),
        Some("false" | "0") => Some(Some(false)),
        _ => None,
    }
}
pub(super) fn optional_id(e: &Elem) -> Option<Option<ElementIdentity>> {
    e.attrs
        .get("id")
        .map(|v| v.parse::<XmlId>().map(ElementIdentity::from))
        .transpose()
        .ok()
}
pub(super) fn element_identity(e: &Elem) -> Option<ElementIdentity> {
    Some(optional_id(e)?.unwrap_or_else(ElementIdentity::anonymous))
}
pub(super) fn selector(e: &Elem, key: &str) -> Option<ElementSelector> {
    Some(id(e, key)?.into())
}
pub(super) fn documentation(e: &Elem) -> Option<String> {
    if e.children.is_empty() {
        return Some(String::new());
    }
    if e.children.len() != 1 {
        return None;
    }
    let d = &e.children[0];
    if d.ns != BPMN || d.name != "documentation" || !d.attrs.is_empty() || !d.children.is_empty() {
        return None;
    }
    Some(d.text.clone())
}
