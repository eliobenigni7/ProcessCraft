//! Conservative interchange: validate first, then qualify a small lossless profile.
use crate::bpmn::{ImportError, import_bpmn};
use processcraft_core::{
    bpmn_catalog::{
        BpmnResource, Interface, ItemDefinition, ItemKind, Operation, ResourceParameter,
    },
    bpmn_model::{
        EventDefinition, EventGatewayType, EventIo, EventProperty, EventReference, EventResource,
        Expression, ExpressionData, ExpressionKind, GatewayDirection, LoopCharacteristics,
        NodeDetails, QNameRef, TimerKind,
    },
    ids::{ElementIdentity, ElementSelector, XmlId},
    model::*,
};
use quick_xml::{
    XmlVersion,
    events::Event,
    name::{QName, ResolveResult},
    reader::NsReader,
};
use std::{collections::BTreeMap, sync::Arc};
const B: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";
const BD: &str = "http://www.omg.org/spec/BPMN/20100524/DI";
const DC: &str = "http://www.omg.org/spec/DD/20100524/DC";
const DI: &str = "http://www.omg.org/spec/DD/20100524/DI";
const ANONYMOUS_CONTEXT_MARKER: &str = "data-processcraft-internal-context";
#[derive(Debug)]
pub struct ImportReport {
    pub editable: bool,
    pub limitations: Vec<String>,
}
#[derive(Debug)]
pub struct PreparedImport {
    pub document: BpmnDocument,
    pub resources: Vec<Resource>,
    pub report: ImportReport,
}
/// No mutation or filesystem access occurs until the caller commits this bundle.
pub fn prepare_import(bytes: &[u8], source_name: &str) -> Result<PreparedImport, ImportError> {
    import_bpmn(bytes)?;
    let rid: XmlId = format!("source_{}", uuid::Uuid::new_v4().simple())
        .parse()
        .expect("generated ID");
    let resource = Resource::new(rid.clone(), "application/xml".into(), Arc::from(bytes));
    let initial = parse_profile(bytes);
    let projected = initial.and_then(|document| {
        let check = (document.has_semantic_common_metadata_features()
            || document.has_di_choreography_features()
            || document.has_expanded_collaboration())
        .then(|| export_document(&document, std::slice::from_ref(&resource), true));
        if let Some(Err(_)) = check {
            None
        } else {
            Some(document)
        }
    });
    let mut doc = projected.unwrap_or_else(|| {
        let mut d = BpmnDocument::new(
            source_name.into(),
            "preserved_definitions".parse().expect("constant"),
        );
        d.capability = Capability::PreservedSource {
            source_resource_id: rid.clone(),
        };
        d
    });
    doc.name = source_name.into();
    doc.provenance = Some(Provenance {
        source_resource_id: rid,
        source_name: source_name.into(),
    });
    let editable = matches!(doc.capability, Capability::Editable);
    Ok(PreparedImport {
        document: doc,
        resources: vec![resource],
        report: ImportReport {
            editable,
            limitations: if editable {
                vec!["Qualified BPMN subset; original source retained".into()]
            } else {
                vec!["Unsupported or ambiguous content retained byte-for-byte; semantic editing disabled".into()]
            },
        },
    })
}
#[derive(Default)]
struct Elem {
    ns: String,
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Elem>,
    text: String,
    namespaces: BTreeMap<String, String>,
    type_ref: Option<QNameRef>,
    catalog_qnames: BTreeMap<String, QNameRef>,
    catalog_text_qname: Option<QNameRef>,
    content: Vec<ElemContent>,
    opaque: bool,
    prefix: Option<String>,
    xml_attributes: Vec<processcraft_core::bpmn_xml::XmlAttribute>,
    foreign_attributes: Vec<processcraft_core::bpmn_xml::XmlAttribute>,
    anonymous_base: Option<PendingBase>,
    anonymous_base_consumed: std::cell::Cell<bool>,
    flow_metadata: Option<processcraft_core::bpmn_activity::FlowElementMetadata>,
    flow_metadata_consumed: std::cell::Cell<bool>,
}
enum ElemContent {
    Text(String),
    Child(usize),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: Option<String>,
    },
}
fn xml_namespaces(e: &Elem) -> Vec<processcraft_core::bpmn_xml::NamespaceBinding> {
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
fn xml_content(e: &Elem) -> Option<processcraft_core::bpmn_xml::XmlContent> {
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
fn structured_documentation(e: &Elem) -> Option<processcraft_core::bpmn_metadata::Documentation> {
    use processcraft_core::{bpmn_metadata::Documentation, bpmn_xml::MixedXml};
    if e.ns != B
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
fn mixed_body(e: &Elem, preserve_context: bool) -> Option<processcraft_core::bpmn_xml::MixedXml> {
    use processcraft_core::bpmn_xml::{MixedXml, validate_mixed};
    if !attrs(e, &[]) || e.children.len() > 1 {
        return None;
    }
    let body = if (!preserve_context
        || e.namespaces.len() == 1 && e.namespaces.get("").is_some_and(|v| v == B))
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
fn extension_elements(e: &Elem) -> Option<processcraft_core::bpmn_metadata::ExtensionElements> {
    if e.ns != B || e.name != "extensionElements" || !attrs(e, &[]) {
        return None;
    }
    let content = xml_content(e)?;
    processcraft_core::bpmn_xml::validate_extension(&content).ok()?;
    Some(processcraft_core::bpmn_metadata::ExtensionElements { content })
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingBase {
    docs: Vec<processcraft_core::bpmn_metadata::Documentation>,
    extras: processcraft_core::bpmn_metadata::BaseExtras,
}
fn documentation_set(
    docs: Vec<processcraft_core::bpmn_metadata::Documentation>,
) -> processcraft_core::bpmn_metadata::DocumentationSet {
    use processcraft_core::{
        bpmn_metadata::DocumentationSet,
        bpmn_xml::{MixedXml, XmlNode},
    };
    if docs.is_empty() {
        return DocumentationSet::Legacy(String::new());
    }
    if docs.len() == 1
        && docs[0].id.is_none()
        && docs[0].text_format.is_none()
        && let MixedXml::Structured(content) = &docs[0].content
        && let [XmlNode::Text { text }] = content.nodes.as_slice()
        && !text.is_empty()
    {
        return DocumentationSet::Legacy(text.clone());
    }
    DocumentationSet::Structured(docs)
}
fn supported_base_owner(name: &str) -> bool {
    matches!(
        name,
        "process"
            | "subProcess"
            | "adHocSubProcess"
            | "transaction"
            | "task"
            | "userTask"
            | "manualTask"
            | "serviceTask"
            | "sendTask"
            | "receiveTask"
            | "scriptTask"
            | "businessRuleTask"
            | "callActivity"
            | "startEvent"
            | "endEvent"
            | "intermediateCatchEvent"
            | "intermediateThrowEvent"
            | "boundaryEvent"
            | "exclusiveGateway"
            | "inclusiveGateway"
            | "parallelGateway"
            | "complexGateway"
            | "eventBasedGateway"
            | "sequenceFlow"
            | "messageFlow"
            | "association"
            | "textAnnotation"
            | "group"
            | "lane"
            | "laneSet"
            | "childLaneSet"
            | "dataObject"
            | "dataObjectReference"
            | "dataStore"
            | "dataStoreReference"
            | "dataInput"
            | "dataOutput"
            | "dataState"
            | "dataInputAssociation"
            | "dataOutputAssociation"
            | "assignment"
            | "ioSpecification"
            | "inputSet"
            | "outputSet"
            | "property"
            | "category"
            | "categoryValue"
            | "message"
            | "signal"
            | "error"
            | "escalation"
            | "itemDefinition"
            | "interface"
            | "operation"
            | "resource"
            | "resourceParameter"
            | "collaboration"
            | "participant"
            | "participantMultiplicity"
            | "globalConversation"
            | "conversationAssociation"
            | "participantAssociation"
            | "messageFlowAssociation"
            | "partnerEntity"
            | "partnerRole"
            | "endPoint"
            | "callConversation"
            | "conversation"
            | "subConversation"
            | "conversationLink"
            | "choreography"
            | "choreographyTask"
            | "messageEventDefinition"
            | "signalEventDefinition"
            | "errorEventDefinition"
            | "escalationEventDefinition"
            | "cancelEventDefinition"
            | "compensateEventDefinition"
            | "terminateEventDefinition"
            | "conditionalEventDefinition"
            | "timerEventDefinition"
            | "linkEventDefinition"
            | "standardLoopCharacteristics"
            | "multiInstanceLoopCharacteristics"
            | "relationship"
            | "callableElement"
            | "rootElement"
            | "globalTask"
            | "globalManualTask"
            | "globalBusinessRuleTask"
            | "globalScriptTask"
            | "globalUserTask"
            | "ioBinding"
            | "auditing"
            | "monitoring"
            | "resourceRole"
            | "performer"
            | "humanPerformer"
            | "potentialOwner"
            | "resourceParameterBinding"
            | "resourceAssignmentExpression"
            | "rendering"
            | "correlationProperty"
            | "correlationPropertyRetrievalExpression"
            | "correlationKey"
            | "correlationSubscription"
            | "correlationPropertyBinding"
            | "inputDataItem"
            | "outputDataItem"
            | "complexBehaviorDefinition"
            | "event"
    )
}
fn extract_owner_metadata(e: &mut Elem, pending: &mut BTreeMap<String, PendingBase>) -> Option<()> {
    if e.opaque {
        return Some(());
    }
    for child in &mut e.children {
        extract_owner_metadata(child, pending)?;
    }
    if e.ns != B || !supported_base_owner(&e.name) {
        return Some(());
    }
    let mut docs = Vec::new();
    let mut extension = None;
    let mut rest = Vec::new();
    let mut phase = 0;
    for child in std::mem::take(&mut e.children) {
        if child.ns == B && child.name == "documentation" && phase == 0 {
            docs.push(structured_documentation(&child)?);
        } else if child.ns == B
            && child.name == "extensionElements"
            && phase == 0
            && extension.is_none()
        {
            extension = Some(extension_elements(&child)?);
            phase = 1;
        } else {
            phase = 2;
            rest.push(child);
        }
    }
    e.children = rest;
    if docs.is_empty() && extension.is_none() && e.foreign_attributes.is_empty() {
        return Some(());
    }
    let attributes = std::mem::take(&mut e.foreign_attributes);
    let namespaces = if attributes.is_empty() {
        Vec::new()
    } else {
        xml_namespaces(e)
    };
    if !attributes.is_empty() {
        processcraft_core::bpmn_xml::validate_foreign_attributes(&attributes, &namespaces).ok()?;
    }
    let extras = processcraft_core::bpmn_metadata::BaseExtras {
        namespaces,
        foreign_attributes: attributes,
        extension_elements: extension,
    };
    let value = PendingBase { docs, extras };
    if let Some(identity) = e.attrs.get("id") {
        if pending.insert(identity.clone(), value).is_some() {
            return None;
        }
    } else {
        e.anonymous_base = Some(value);
    }
    Some(())
}
fn take_anonymous_base(e: &Elem) -> Option<processcraft_core::bpmn_metadata::BaseMetadata> {
    let pending = e.anonymous_base.as_ref()?;
    e.anonymous_base_consumed.set(true);
    Some(processcraft_core::bpmn_metadata::BaseMetadata {
        namespaces: pending.extras.namespaces.clone(),
        documentation: pending.docs.clone(),
        foreign_attributes: pending.extras.foreign_attributes.clone(),
        extension_elements: pending.extras.extension_elements.clone(),
    })
}
fn anonymous_documentation(
    e: &Elem,
    fallback: String,
) -> processcraft_core::bpmn_metadata::DocumentationSet {
    if let Some(pending) = &e.anonymous_base {
        e.anonymous_base_consumed.set(true);
        documentation_set(pending.docs.clone())
    } else {
        fallback.into()
    }
}
fn anonymous_extras(e: &Elem) -> Option<processcraft_core::bpmn_metadata::BaseExtras> {
    let pending = e.anonymous_base.as_ref()?;
    e.anonymous_base_consumed.set(true);
    let extras = pending.extras.clone();
    (!extras.namespaces.is_empty()
        || !extras.foreign_attributes.is_empty()
        || extras.extension_elements.is_some())
    .then_some(extras)
}
fn has_unconsumed_anonymous_base(e: &Elem) -> bool {
    (e.anonymous_base.is_some() && !e.anonymous_base_consumed.get())
        || e.children.iter().any(has_unconsumed_anonymous_base)
}
fn is_flow_element(name: &str) -> bool {
    matches!(
        name,
        "task"
            | "userTask"
            | "manualTask"
            | "serviceTask"
            | "sendTask"
            | "receiveTask"
            | "scriptTask"
            | "businessRuleTask"
            | "callActivity"
            | "subProcess"
            | "adHocSubProcess"
            | "transaction"
            | "startEvent"
            | "endEvent"
            | "intermediateCatchEvent"
            | "intermediateThrowEvent"
            | "boundaryEvent"
            | "exclusiveGateway"
            | "inclusiveGateway"
            | "parallelGateway"
            | "complexGateway"
            | "eventBasedGateway"
            | "sequenceFlow"
            | "dataObject"
            | "dataObjectReference"
            | "dataStoreReference"
            | "choreographyTask"
            | "event"
    )
}
fn take_flow_metadata(e: &Elem) -> Option<processcraft_core::bpmn_activity::FlowElementMetadata> {
    let metadata = e.flow_metadata.clone()?;
    e.flow_metadata_consumed.set(true);
    Some(metadata)
}
fn has_unconsumed_flow_metadata(e: &Elem) -> bool {
    (e.flow_metadata.is_some() && !e.flow_metadata_consumed.get())
        || e.children.iter().any(has_unconsumed_flow_metadata)
}
fn extract_flow_metadata(e: &mut Elem, target_namespace: &str) -> Option<()> {
    if e.opaque {
        return Some(());
    }
    for child in &mut e.children {
        extract_flow_metadata(child, target_namespace)?;
    }
    if e.ns != B || !is_flow_element(&e.name) {
        return Some(());
    }
    let mut result = processcraft_core::bpmn_activity::FlowElementMetadata::default();
    if e.name == "sequenceFlow" {
        result.is_immediate = optional_bool(e.attrs.get("isImmediate"))?;
    }
    let mut phase = 0;
    let mut rest = Vec::new();
    for child in std::mem::take(&mut e.children) {
        match (child.ns.as_str(), child.name.as_str()) {
            (B, "auditing") if phase == 0 && result.auditing.is_none() => {
                if !attrs(&child, &["id"]) || !child.children.is_empty() {
                    return None;
                }
                result.auditing = Some(processcraft_core::bpmn_activity::BaseOwner {
                    id: optional_id(&child)?,
                    base: take_anonymous_base(&child),
                });
                phase = 1;
            }
            (B, "monitoring") if phase <= 1 && result.monitoring.is_none() => {
                if !attrs(&child, &["id"]) || !child.children.is_empty() {
                    return None;
                }
                result.monitoring = Some(processcraft_core::bpmn_activity::BaseOwner {
                    id: optional_id(&child)?,
                    base: take_anonymous_base(&child),
                });
                phase = 2;
            }
            (B, "categoryValueRef") if phase <= 2 => {
                if !attrs(&child, &[]) || !child.children.is_empty() {
                    return None;
                }
                result.category_value_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
                phase = 2;
            }
            (B, "incoming")
                if phase <= 3
                    && !matches!(
                        e.name.as_str(),
                        "sequenceFlow"
                            | "dataObject"
                            | "dataObjectReference"
                            | "dataStoreReference"
                    ) =>
            {
                if !attrs(&child, &[]) || !child.children.is_empty() {
                    return None;
                }
                result.incoming.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
                phase = 3;
            }
            (B, "outgoing")
                if phase <= 4
                    && !matches!(
                        e.name.as_str(),
                        "sequenceFlow"
                            | "dataObject"
                            | "dataObjectReference"
                            | "dataStoreReference"
                    ) =>
            {
                if !attrs(&child, &[]) || !child.children.is_empty() {
                    return None;
                }
                result.outgoing.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
                phase = 4;
            }
            (B, "auditing" | "monitoring" | "categoryValueRef" | "incoming" | "outgoing") => {
                return None;
            }
            _ => {
                phase = 5;
                rest.push(child);
            }
        }
    }
    e.children = rest;
    if result != processcraft_core::bpmn_activity::FlowElementMetadata::default() {
        e.flow_metadata = Some(result);
    }
    Some(())
}
fn is_expression(name: &str) -> bool {
    matches!(
        name,
        "conditionExpression"
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
    )
}
fn tree(bytes: &[u8]) -> Option<Elem> {
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
                        || (parent.ns == B
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
                        if item.ns == B
                            && item.name == "rootElement"
                            && uri.0 == "http://www.w3.org/2001/XMLSchema-instance"
                            && key == "type"
                        {
                            let value = a
                                .normalized_value(XmlVersion::Implicit1_0)
                                .ok()?
                                .into_owned();
                            let (resolved, name) = r.resolver().resolve_element(QName(&value));
                            if !matches!(resolved, ResolveResult::Bound(uri) if uri.0 == B)
                                || name.as_ref() != "tCallableElement"
                            {
                                return None;
                            }
                            item.attrs
                                .insert("xsi:type".into(), "tCallableElement".into());
                            continue;
                        }
                        if item.ns != B
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
                        if !matches!(resolved, ResolveResult::Bound(uri) if uri.0==B)
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
                    if (item.ns == B
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
                                | (
                                    "conversationAssociation",
                                    "innerConversationNodeRef" | "outerConversationNodeRef"
                                )
                                | (
                                    "messageFlowAssociation",
                                    "innerMessageFlowRef" | "outerMessageFlowRef"
                                )
                                | ("process", "definitionalCollaborationRef")
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
                        || (item.ns == BD
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
                    if item.ns == B && is_expression(&item.name) && key == "evaluatesToTypeRef" {
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
                        (B, "sequenceFlow", "sourceRef" | "targetRef")
                            | (B, "boundaryEvent", "attachedToRef")
                            | (B, "choreographyTask", "initiatingParticipantRef")
                            | (BD, "BPMNPlane" | "BPMNShape" | "BPMNEdge", "bpmnElement")
                            | (BD, "BPMNEdge", "sourceElement" | "targetElement")
                    ) {
                        if item.name == "sequenceFlow" && !value.contains(':') {
                            item.attrs.insert(key.into(), value);
                            continue;
                        }
                        let (resolved, local) = r.resolver().resolve_element(QName(value.trim()));
                        let namespace = match resolved {
                            ResolveResult::Bound(uri) => uri.0,
                            ResolveResult::Unbound => "",
                            ResolveResult::Unknown(_) => return None,
                        };
                        if Some(namespace) != target_namespace.as_deref() {
                            return None;
                        }
                        value = local.as_ref().to_string();
                    }
                    item.attrs.insert(key.into(), value);
                }
                stack.push(item);
            }
            Event::End(_) => {
                let mut item = stack.pop()?;
                if item.ns == B && matches!(item.name.as_str(), "flowNodeRef") {
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
                if item.ns == B
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
                        || last.ns == B
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
                    && (last.ns != B
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
                    && (last.ns != B
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
                    && !(last.ns == B
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
fn attrs(e: &Elem, allowed: &[&str]) -> bool {
    e.foreign_attributes.is_empty() && e.attrs.keys().all(|k| allowed.contains(&k.as_str()))
}
fn id(e: &Elem, key: &str) -> Option<XmlId> {
    e.attrs.get(key)?.parse().ok()
}
fn number(e: &Elem, key: &str) -> Option<f64> {
    e.attrs.get(key)?.parse().ok()
}
fn bounds(e: &Elem) -> Option<Bounds> {
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
fn di_attrs(e: &Elem, allowed: &[&str]) -> bool {
    e.attrs.keys().all(|key| allowed.contains(&key.as_str()))
}
fn parse_di_metadata(
    e: &Elem,
) -> Option<(Option<processcraft_core::model::DiElementMetadata>, usize)> {
    let extension = if let Some(child) = e
        .children
        .first()
        .filter(|child| child.ns == DI && child.name == "extension")
    {
        if !attrs(child, &[]) || !child.foreign_attributes.is_empty() {
            return None;
        }
        let content = xml_content(child)?;
        processcraft_core::bpmn_xml::validate_di_extension(&content).ok()?;
        Some(content)
    } else {
        None
    };
    let offset = usize::from(extension.is_some());
    if e.foreign_attributes.is_empty() && extension.is_none() {
        return Some((None, offset));
    }
    let namespaces = xml_namespaces(e);
    processcraft_core::bpmn_xml::validate_di_foreign_attributes(&e.foreign_attributes, &namespaces)
        .ok()?;
    Some((
        Some(processcraft_core::model::DiElementMetadata {
            namespaces,
            foreign_attributes: e.foreign_attributes.clone(),
            extension,
        }),
        offset,
    ))
}
fn parse_label(e: &Elem, target_namespace: &str) -> Option<Label> {
    if e.ns != BD || e.name != "BPMNLabel" || !di_attrs(e, &["id", "labelStyle"]) {
        return None;
    }
    let (di_metadata, offset) = parse_di_metadata(e)?;
    if e.children.len() > offset + 1 {
        return None;
    }
    Some(Label {
        di_metadata,
        id: optional_id(e)?,
        bounds: if let Some(child) = e.children.get(offset) {
            Some(bounds(child)?)
        } else {
            None
        },
        style: None,
        style_ref: e.catalog_qnames.get("labelStyle").map(|q| {
            if q.namespace == target_namespace {
                DiStyleRef::Local(q.local.clone().into())
            } else {
                DiStyleRef::Qualified(q.clone())
            }
        }),
    })
}
fn parse_expression(e: &Elem, implicit_formal: bool) -> Option<Expression> {
    if e.ns != B
        || e.text.len() > 16_384
        || e.namespaces.len() > 256
        || e.namespaces
            .iter()
            .any(|(p, u)| p.len() > 16_384 || u.len() > 16_384)
        || e.attrs.get("language").is_some_and(|v| v.len() > 16_384)
        || !attrs(e, &["id", "xsi:type", "language", "evaluatesToTypeRef"])
    {
        return None;
    }
    let explicit = e.attrs.get("xsi:type").map(String::as_str);
    let formal = match explicit {
        None => implicit_formal,
        Some("tExpression") if !implicit_formal => false,
        Some("tFormalExpression") => true,
        _ => return None,
    };
    if !formal && (e.attrs.contains_key("language") || e.attrs.contains_key("evaluatesToTypeRef")) {
        return None;
    }
    let mut has_markup = false;
    let mut parts = Vec::new();
    let mut seen_extension = false;
    for part in &e.content {
        use processcraft_core::bpmn_model::ExpressionPart;
        parts.push(match part {
            ElemContent::Text(text) => ExpressionPart::Text { text: text.clone() },
            ElemContent::Comment(text) => {
                has_markup = true;
                ExpressionPart::Comment { text: text.clone() }
            }
            ElemContent::ProcessingInstruction { target, data } => {
                has_markup = true;
                ExpressionPart::ProcessingInstruction {
                    target: target.parse().ok()?,
                    data: data.clone(),
                }
            }
            ElemContent::Child(index) => {
                has_markup = true;
                let child = e.children.get(*index)?;
                if child.ns == B && child.name == "documentation" && !seen_extension {
                    ExpressionPart::Documentation {
                        documentation: structured_documentation(child)?,
                    }
                } else if child.ns == B && child.name == "extensionElements" && !seen_extension {
                    seen_extension = true;
                    ExpressionPart::ExtensionElements {
                        extension_elements: extension_elements(child)?,
                    }
                } else {
                    return None;
                }
            }
        });
    }
    Some(Expression::Structured(ExpressionData {
        foreign_attributes: Vec::new(),
        id: optional_id(e)?,
        kind: if formal {
            ExpressionKind::Formal {
                explicit_type: explicit.is_some(),
                language: e.attrs.get("language").cloned(),
                evaluates_to_type_ref: e.type_ref.clone(),
            }
        } else {
            ExpressionKind::Base {
                explicit_type: explicit.is_some(),
            }
        },
        text: if has_markup {
            processcraft_core::bpmn_model::ExpressionBody::Mixed { parts }
        } else {
            e.text.clone().into()
        },
        namespaces: e
            .namespaces
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }))
}
fn event_reference(qname: &QNameRef, target_namespace: &str) -> EventReference {
    if qname.namespace == target_namespace {
        EventReference::Local(qname.local.clone().into())
    } else {
        EventReference::Qualified(qname.clone())
    }
}
fn parse_correlation_key(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationKey> {
    use processcraft_core::bpmn_activity::CorrelationKey;
    if e.ns != B || e.name != "correlationKey" || !attrs(e, &["id", "name"]) {
        return None;
    }
    let mut refs = Vec::new();
    for child in &e.children {
        if child.ns != B
            || child.name != "correlationPropertyRef"
            || !attrs(child, &[])
            || !child.children.is_empty()
        {
            return None;
        }
        refs.push(event_reference(
            child.catalog_text_qname.as_ref()?,
            namespace,
        ));
    }
    Some(CorrelationKey {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        property_refs: refs,
    })
}
fn parse_correlation_property(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationProperty> {
    use processcraft_core::bpmn_activity::{CorrelationProperty, CorrelationRetrieval};
    if e.ns != B
        || e.name != "correlationProperty"
        || !attrs(e, &["id", "name", "type"])
        || e.children.is_empty()
    {
        return None;
    }
    let mut retrieval_expressions = Vec::new();
    for child in &e.children {
        if child.ns != B
            || child.name != "correlationPropertyRetrievalExpression"
            || !attrs(child, &["id", "messageRef"])
            || child.children.len() != 1
            || child.children[0].name != "messagePath"
        {
            return None;
        }
        retrieval_expressions.push(CorrelationRetrieval {
            id: optional_id(child)?,
            base: take_anonymous_base(child),
            message_ref: Some(event_reference(
                child.catalog_qnames.get("messageRef")?,
                namespace,
            )),
            message_path: Some(parse_expression(&child.children[0], true)?),
        });
    }
    Some(CorrelationProperty {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        item_type: e.catalog_qnames.get("type").cloned(),
        retrieval_expressions,
    })
}
fn parse_correlation_subscription(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationSubscription> {
    use processcraft_core::bpmn_activity::{CorrelationPropertyBinding, CorrelationSubscription};
    if e.ns != B || e.name != "correlationSubscription" || !attrs(e, &["id", "correlationKeyRef"]) {
        return None;
    }
    let mut bindings = Vec::new();
    for child in &e.children {
        if child.ns != B
            || child.name != "correlationPropertyBinding"
            || !attrs(child, &["id", "correlationPropertyRef"])
            || child.children.len() != 1
            || child.children[0].name != "dataPath"
        {
            return None;
        }
        bindings.push(CorrelationPropertyBinding {
            id: optional_id(child)?,
            base: take_anonymous_base(child),
            property_ref: Some(event_reference(
                child.catalog_qnames.get("correlationPropertyRef")?,
                namespace,
            )),
            data_path: Some(parse_expression(&child.children[0], true)?),
        });
    }
    Some(CorrelationSubscription {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        key_ref: Some(event_reference(
            e.catalog_qnames.get("correlationKeyRef")?,
            namespace,
        )),
        bindings,
    })
}
fn parse_audit_monitor(e: &Elem) -> Option<processcraft_core::bpmn_activity::AuditMonitor> {
    if e.ns != B
        || !matches!(e.name.as_str(), "auditing" | "monitoring")
        || !attrs(e, &["id"])
        || !e.children.is_empty()
    {
        return None;
    }
    Some(processcraft_core::bpmn_activity::BaseOwner {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
    })
}
fn parse_property(e: &Elem) -> Option<processcraft_core::bpmn_model::EventProperty> {
    if e.ns != B || e.name != "property" || !attrs(e, &["id", "name", "itemSubjectRef"]) {
        return None;
    }
    Some(processcraft_core::bpmn_model::EventProperty {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        item_aware: parse_item_aware(e)?,
    })
}
fn parse_io_binding(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::IoBinding> {
    if e.ns != B
        || e.name != "ioBinding"
        || !attrs(e, &["id", "operationRef", "inputDataRef", "outputDataRef"])
        || !e.children.is_empty()
    {
        return None;
    }
    Some(processcraft_core::bpmn_activity::IoBinding {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        operation_ref: Some(event_reference(
            e.catalog_qnames.get("operationRef")?,
            namespace,
        )),
        input_data_ref: Some(selector(e, "inputDataRef")?),
        output_data_ref: Some(selector(e, "outputDataRef")?),
    })
}
fn parse_rendering(e: &Elem) -> Option<processcraft_core::bpmn_activity::BaseOwner> {
    if e.ns != B || e.name != "rendering" || !attrs(e, &["id"]) || !e.children.is_empty() {
        return None;
    }
    Some(processcraft_core::bpmn_activity::BaseOwner {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
    })
}
fn parse_resource_role(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::ResourceRole> {
    use processcraft_core::bpmn_activity::{
        ResourceAssignmentExpression, ResourceParameterBinding, ResourceRole,
        ResourceRoleAssignment, ResourceRoleKind,
    };
    if e.ns != B || !attrs(e, &["id", "name"]) {
        return None;
    }
    let kind = match e.name.as_str() {
        "resourceRole" => ResourceRoleKind::ResourceRole,
        "performer" => ResourceRoleKind::Performer,
        "humanPerformer" => ResourceRoleKind::HumanPerformer,
        "potentialOwner" => ResourceRoleKind::PotentialOwner,
        _ => return None,
    };
    let assignment = if e.children.is_empty() {
        ResourceRoleAssignment::None
    } else if e.children[0].ns == B
        && e.children[0].name == "resourceRef"
        && attrs(&e.children[0], &[])
        && e.children[0].children.is_empty()
    {
        let reference = event_reference(e.children[0].catalog_text_qname.as_ref()?, namespace);
        let mut bindings = Vec::new();
        for child in &e.children[1..] {
            if child.ns != B
                || child.name != "resourceParameterBinding"
                || !attrs(child, &["id", "parameterRef"])
                || child.children.len() != 1
                || child.children[0].name != "expression"
            {
                return None;
            }
            bindings.push(ResourceParameterBinding {
                id: optional_id(child)?,
                base: take_anonymous_base(child),
                parameter_ref: Some(event_reference(
                    child.catalog_qnames.get("parameterRef")?,
                    namespace,
                )),
                expression: Some(parse_expression(&child.children[0], false)?),
            });
        }
        ResourceRoleAssignment::Resource {
            reference,
            bindings,
        }
    } else if e.children.len() == 1
        && e.children[0].ns == B
        && e.children[0].name == "resourceAssignmentExpression"
    {
        let child = &e.children[0];
        if !attrs(child, &["id"])
            || child.children.len() != 1
            || child.children[0].name != "expression"
        {
            return None;
        }
        ResourceRoleAssignment::Expression {
            assignment: ResourceAssignmentExpression {
                id: optional_id(child)?,
                base: take_anonymous_base(child),
                expression: Some(parse_expression(&child.children[0], false)?),
            },
        }
    } else {
        return None;
    };
    Some(ResourceRole {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        kind,
        assignment,
    })
}
fn parse_global_task(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::GlobalTask> {
    use processcraft_core::bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind};
    if e.ns != B
        || !attrs(
            e,
            &["id", "name", "implementation", "scriptLanguage", "xsi:type"],
        )
    {
        return None;
    }
    let mut callable = CallableMetadata::default();
    let mut io_specification = None;
    let mut resource_roles = Vec::new();
    let mut script = None;
    let mut renderings = Vec::new();
    let mut phase = 0u8;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "supportedInterfaceRef"
                if phase == 0 && attrs(child, &[]) && child.children.is_empty() =>
            {
                callable.supported_interface_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ))
            }
            "ioSpecification" if phase <= 1 && io_specification.is_none() => {
                phase = 1;
                io_specification = Some(parse_io_specification(child)?);
            }
            "ioBinding" if phase <= 2 => {
                phase = 2;
                callable
                    .io_bindings
                    .push(parse_io_binding(child, namespace)?);
            }
            "resourceRole" | "performer" | "humanPerformer" | "potentialOwner"
                if phase <= 3 && e.name != "rootElement" =>
            {
                phase = 3;
                resource_roles.push(parse_resource_role(child, namespace)?);
            }
            "script" if e.name == "globalScriptTask" && phase <= 4 && script.is_none() => {
                phase = 4;
                script = Some(mixed_body(child, true)?);
            }
            "rendering" if e.name == "globalUserTask" && phase <= 4 => {
                phase = 4;
                renderings.push(parse_rendering(child)?);
            }
            _ => return None,
        }
    }
    let implementation = e.attrs.get("implementation").cloned();
    let kind = match e.name.as_str() {
        "rootElement"
            if e.attrs.get("xsi:type").map(String::as_str) == Some("tCallableElement")
                && implementation.is_none()
                && !e.attrs.contains_key("scriptLanguage") =>
        {
            GlobalTaskKind::CallableElement
        }
        "globalTask" if implementation.is_none() && !e.attrs.contains_key("scriptLanguage") => {
            GlobalTaskKind::Task
        }
        "globalManualTask"
            if implementation.is_none() && !e.attrs.contains_key("scriptLanguage") =>
        {
            GlobalTaskKind::Manual
        }
        "globalBusinessRuleTask" if !e.attrs.contains_key("scriptLanguage") => {
            GlobalTaskKind::BusinessRule { implementation }
        }
        "globalScriptTask" if implementation.is_none() => GlobalTaskKind::Script {
            script_language: e.attrs.get("scriptLanguage").cloned(),
            script,
        },
        "globalUserTask" if !e.attrs.contains_key("scriptLanguage") => GlobalTaskKind::User {
            implementation,
            renderings,
        },
        _ => return None,
    };
    Some(GlobalTask {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        callable,
        io_specification,
        resource_roles,
        kind,
    })
}
fn parse_event_definition(e: &Elem, target_namespace: &str) -> Option<EventDefinition> {
    if e.ns != B {
        return None;
    }
    let id = optional_id(e)?;
    let reference = |key: &str| {
        e.catalog_qnames
            .get(key)
            .map(|q| event_reference(q, target_namespace))
    };
    Some(match e.name.as_str() {
        "messageEventDefinition" if attrs(e, &["id", "messageRef"]) => {
            let mut operation_ref = None;
            for child in &e.children {
                if child.ns != B
                    || child.name != "operationRef"
                    || !child.attrs.is_empty()
                    || !child.children.is_empty()
                    || operation_ref.is_some()
                {
                    return None;
                }
                operation_ref = Some(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
            }
            EventDefinition::Message {
                base: take_anonymous_base(e),
                id,
                message_ref: reference("messageRef"),
                operation_ref,
            }
        }
        "signalEventDefinition" if attrs(e, &["id", "signalRef"]) && e.children.is_empty() => {
            EventDefinition::Signal {
                base: take_anonymous_base(e),
                id,
                signal_ref: reference("signalRef"),
            }
        }
        "errorEventDefinition" if attrs(e, &["id", "errorRef"]) && e.children.is_empty() => {
            EventDefinition::Error {
                base: take_anonymous_base(e),
                id,
                error_ref: reference("errorRef"),
            }
        }
        "escalationEventDefinition"
            if attrs(e, &["id", "escalationRef"]) && e.children.is_empty() =>
        {
            EventDefinition::Escalation {
                base: take_anonymous_base(e),
                id,
                escalation_ref: reference("escalationRef"),
            }
        }
        "cancelEventDefinition" if attrs(e, &["id"]) && e.children.is_empty() => {
            EventDefinition::Cancel {
                base: take_anonymous_base(e),
                id,
            }
        }
        "compensateEventDefinition"
            if attrs(e, &["id", "waitForCompletion", "activityRef"]) && e.children.is_empty() =>
        {
            EventDefinition::Compensate {
                base: take_anonymous_base(e),
                id,
                wait_for_completion: optional_bool(e.attrs.get("waitForCompletion"))?,
                activity_ref: reference("activityRef"),
            }
        }
        "terminateEventDefinition" if attrs(e, &["id"]) && e.children.is_empty() => {
            EventDefinition::Terminate {
                base: take_anonymous_base(e),
                id,
            }
        }
        "conditionalEventDefinition"
            if attrs(e, &["id"]) && e.children.len() == 1 && e.children[0].name == "condition" =>
        {
            EventDefinition::Conditional {
                base: take_anonymous_base(e),
                id,
                expression: parse_expression(&e.children[0], false)?,
            }
        }
        "timerEventDefinition" if attrs(e, &["id"]) && e.children.len() <= 1 => {
            let (kind, expression) = if let Some(child) = e.children.first() {
                let kind = match child.name.as_str() {
                    "timeDate" => TimerKind::Date,
                    "timeDuration" => TimerKind::Duration,
                    "timeCycle" => TimerKind::Cycle,
                    _ => return None,
                };
                (Some(kind), Some(parse_expression(child, false)?))
            } else {
                (None, None)
            };
            EventDefinition::Timer {
                base: take_anonymous_base(e),
                id,
                kind,
                expression,
            }
        }
        "linkEventDefinition" if attrs(e, &["id", "name"]) => {
            let mut sources = Vec::new();
            let mut target = None;
            for child in &e.children {
                if child.ns != B || !child.attrs.is_empty() || !child.children.is_empty() {
                    return None;
                }
                let reference =
                    event_reference(child.catalog_text_qname.as_ref()?, target_namespace);
                match child.name.as_str() {
                    "source" if target.is_none() => sources.push(reference),
                    "target" if target.is_none() => target = Some(reference),
                    _ => return None,
                }
            }
            EventDefinition::Link {
                base: take_anonymous_base(e),
                id,
                name: e.attrs.get("name")?.clone(),
                sources,
                target,
            }
        }
        _ => return None,
    })
}
fn optional_bool(value: Option<&String>) -> Option<Option<bool>> {
    match value.map(String::as_str) {
        None => Some(None),
        Some("true" | "1") => Some(Some(true)),
        Some("false" | "0") => Some(Some(false)),
        _ => None,
    }
}
fn gateway_details(e: &Elem) -> Option<(String, Option<NodeDetails>)> {
    let direction = match e.attrs.get("gatewayDirection").map(String::as_str) {
        None => None,
        Some("Unspecified") => Some(GatewayDirection::Unspecified),
        Some("Converging") => Some(GatewayDirection::Converging),
        Some("Diverging") => Some(GatewayDirection::Diverging),
        Some("Mixed") => Some(GatewayDirection::Mixed),
        _ => return None,
    };
    let event_gateway_type = match e.attrs.get("eventGatewayType").map(String::as_str) {
        None => None,
        Some("Exclusive") => Some(EventGatewayType::Exclusive),
        Some("Parallel") => Some(EventGatewayType::Parallel),
        _ => return None,
    };
    let default_flow = e.attrs.get("default").map(|v| v.parse()).transpose().ok()?;
    let instantiate = optional_bool(e.attrs.get("instantiate"))?;
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut activation_condition = None;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "documentation" if !seen_doc && child.attrs.is_empty() && child.children.is_empty() => {
                doc = child.text.clone();
                seen_doc = true;
            }
            "activationCondition" if activation_condition.is_none() => {
                activation_condition = Some(parse_expression(child, false)?)
            }
            _ => return None,
        }
    }
    let details = if default_flow.is_some()
        || direction.is_some()
        || instantiate.is_some()
        || event_gateway_type.is_some()
        || activation_condition.is_some()
    {
        Some(NodeDetails::Gateway {
            default_flow,
            direction,
            instantiate,
            event_gateway_type,
            activation_condition,
        })
    } else {
        None
    };
    Some((doc, details))
}
fn parse_implicit_input_association(
    e: &Elem,
) -> Option<processcraft_core::bpmn_activity::ImplicitInputAssociation> {
    use processcraft_core::bpmn_activity::ImplicitInputAssociation;
    use processcraft_core::bpmn_data::DataAssignment;
    if e.ns != B || e.name != "dataInputAssociation" || !attrs(e, &["id"]) {
        return None;
    }
    let mut sources = Vec::new();
    let mut target = None;
    let mut transformation = None;
    let mut assignments = Vec::new();
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "sourceRef" if target.is_none() && attrs(child, &[]) && child.children.is_empty() => {
                sources.push(child.text.trim().parse().ok()?)
            }
            "targetRef" if target.is_none() && attrs(child, &[]) && child.children.is_empty() => {
                target = Some(child.text.trim().parse().ok()?)
            }
            "transformation"
                if target.is_some() && transformation.is_none() && assignments.is_empty() =>
            {
                transformation = Some(parse_expression(child, true)?)
            }
            "assignment"
                if target.is_some()
                    && attrs(child, &["id"])
                    && child.children.len() == 2
                    && child.children[0].name == "from"
                    && child.children[1].name == "to" =>
            {
                assignments.push(DataAssignment {
                    id: optional_id(child)?,
                    base: take_anonymous_base(child),
                    from: parse_expression(&child.children[0], false)?,
                    to: parse_expression(&child.children[1], false)?,
                })
            }
            _ => return None,
        }
    }
    Some(ImplicitInputAssociation {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        sources,
        target: Some(target?),
        transformation,
        assignments,
    })
}
fn parse_implicit_throw(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::ImplicitThrowEvent> {
    use processcraft_core::bpmn_activity::ImplicitThrowEvent;
    if e.ns != B || e.name != "event" || !attrs(e, &["id", "name"]) {
        return None;
    }
    let mut flow = take_flow_metadata(e);
    let incoming = flow
        .as_mut()
        .map(|metadata| std::mem::take(&mut metadata.incoming))
        .unwrap_or_default();
    let outgoing = flow
        .as_mut()
        .map(|metadata| std::mem::take(&mut metadata.outgoing))
        .unwrap_or_default();
    if flow.as_ref().is_some_and(|metadata| {
        *metadata == processcraft_core::bpmn_activity::FlowElementMetadata::default()
    }) {
        flow = None;
    }
    let mut value = ImplicitThrowEvent {
        id: optional_id(e)?,
        name: e.attrs.get("name").cloned(),
        base: take_anonymous_base(e),
        flow,
        properties: Vec::new(),
        incoming,
        outgoing,
        inputs: Vec::new(),
        input_associations: Vec::new(),
        input_set: None,
        definitions: Vec::new(),
        definition_refs: Vec::new(),
    };
    let mut phase = 0;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "property" if phase == 0 => value.properties.push(parse_property(child)?),
            "dataInput" if phase <= 1 => {
                phase = 1;
                value.inputs.push(parse_data_port(child)?);
            }
            "dataInputAssociation" if phase <= 2 => {
                phase = 2;
                value
                    .input_associations
                    .push(parse_implicit_input_association(child)?);
            }
            "inputSet" if phase <= 3 && value.input_set.is_none() => {
                phase = 3;
                value.input_set = Some(parse_io_set(child, true)?);
            }
            "eventDefinitionRef"
                if phase <= 5 && attrs(child, &[]) && child.children.is_empty() =>
            {
                phase = 5;
                value.definition_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ));
            }
            name if name.ends_with("EventDefinition") && phase <= 4 => {
                phase = 4;
                value
                    .definitions
                    .push(parse_event_definition(child, namespace)?);
            }
            _ => return None,
        }
    }
    Some(value)
}
fn loop_characteristics(e: &Elem, namespace: &str) -> Option<LoopCharacteristics> {
    use processcraft_core::bpmn_activity::{ComplexBehaviorDefinition, MultiInstanceFlowCondition};
    if e.name == "standardLoopCharacteristics" {
        if !attrs(e, &["id", "testBefore", "loopMaximum"]) || e.children.len() > 1 {
            return None;
        }
        let condition = if let Some(child) = e.children.first() {
            if child.ns != B || child.name != "loopCondition" {
                return None;
            }
            Some(parse_expression(child, false)?)
        } else {
            None
        };
        let maximum = e.attrs.get("loopMaximum").cloned();
        if let Some(v) = &maximum {
            let digits = v.strip_prefix(['+', '-']).unwrap_or(v);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        return Some(LoopCharacteristics::Standard {
            base: take_anonymous_base(e),
            id: optional_id(e)?,
            test_before: optional_bool(e.attrs.get("testBefore"))?,
            condition,
            maximum,
        });
    }
    if e.name != "multiInstanceLoopCharacteristics"
        || !attrs(
            e,
            &[
                "id",
                "isSequential",
                "behavior",
                "oneBehaviorEventRef",
                "noneBehaviorEventRef",
            ],
        )
    {
        return None;
    }
    let behavior = match e.attrs.get("behavior").map(String::as_str) {
        None => None,
        Some("None") => Some(MultiInstanceFlowCondition::None),
        Some("One") => Some(MultiInstanceFlowCondition::One),
        Some("All") => Some(MultiInstanceFlowCondition::All),
        Some("Complex") => Some(MultiInstanceFlowCondition::Complex),
        _ => return None,
    };
    let reference = |name: &str| {
        e.catalog_qnames
            .get(name)
            .map(|q| event_reference(q, namespace))
    };
    let mut result = LoopCharacteristics::MultiInstance {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        is_sequential: optional_bool(e.attrs.get("isSequential"))?,
        behavior,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: Vec::new(),
        one_behavior_event_ref: reference("oneBehaviorEventRef"),
        none_behavior_event_ref: reference("noneBehaviorEventRef"),
        cardinality: None,
        completion_condition: None,
    };
    let LoopCharacteristics::MultiInstance {
        cardinality,
        loop_data_input_ref,
        loop_data_output_ref,
        input_data_item,
        output_data_item,
        complex_behavior_definitions,
        completion_condition,
        ..
    } = &mut result
    else {
        unreachable!()
    };
    let mut phase = 0;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "loopCardinality" if phase == 0 && cardinality.is_none() => {
                *cardinality = Some(parse_expression(child, false)?);
                phase = 1;
            }
            "loopDataInputRef"
                if phase <= 1
                    && loop_data_input_ref.is_none()
                    && attrs(child, &[])
                    && child.children.is_empty() =>
            {
                *loop_data_input_ref = Some(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ));
                phase = 2;
            }
            "loopDataOutputRef"
                if phase <= 2
                    && loop_data_output_ref.is_none()
                    && attrs(child, &[])
                    && child.children.is_empty() =>
            {
                *loop_data_output_ref = Some(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ));
                phase = 3;
            }
            "inputDataItem" if phase <= 3 && input_data_item.is_none() => {
                *input_data_item = Some(parse_data_port(child)?);
                phase = 4;
            }
            "outputDataItem" if phase <= 4 && output_data_item.is_none() => {
                *output_data_item = Some(parse_data_port(child)?);
                phase = 5;
            }
            "complexBehaviorDefinition"
                if phase <= 5
                    && attrs(child, &["id"])
                    && (1..=2).contains(&child.children.len())
                    && child.children[0].ns == B
                    && child.children[0].name == "condition" =>
            {
                let condition = Some(parse_expression(&child.children[0], true)?);
                let event = if child.children.len() == 2 {
                    Some(parse_implicit_throw(&child.children[1], namespace)?)
                } else {
                    None
                };
                complex_behavior_definitions.push(ComplexBehaviorDefinition {
                    id: optional_id(child)?,
                    base: take_anonymous_base(child),
                    condition,
                    event,
                });
                phase = 5;
            }
            "completionCondition" if phase <= 5 && completion_condition.is_none() => {
                *completion_condition = Some(parse_expression(child, false)?);
                phase = 6;
            }
            _ => return None,
        }
    }
    Some(result)
}
fn parse_item_aware(e: &Elem) -> Option<Option<processcraft_core::bpmn_data::ItemAwareMetadata>> {
    use processcraft_core::bpmn_data::{DataState, ItemAwareMetadata};
    let mut state = None;
    for child in &e.children {
        if child.ns == B && child.name == "dataState" {
            if state.is_some() || !attrs(child, &["id", "name"]) || !child.children.is_empty() {
                return None;
            }
            state = Some(DataState {
                base: take_anonymous_base(child),
                id: optional_id(child)?,
                name: child.attrs.get("name").cloned(),
            });
        } else if child.ns != B || child.name != "documentation" {
            return None;
        }
    }
    let item_subject_ref = e.catalog_qnames.get("itemSubjectRef").cloned();
    if item_subject_ref.is_none() && state.is_none() {
        Some(None)
    } else {
        Some(Some(ItemAwareMetadata {
            item_subject_ref,
            data_state: state,
        }))
    }
}
fn item_documentation(e: &Elem) -> Option<String> {
    let mut doc = None;
    for child in &e.children {
        if child.ns == B && child.name == "dataState" {
            continue;
        }
        if child.ns != B
            || child.name != "documentation"
            || !child.attrs.is_empty()
            || !child.children.is_empty()
            || doc.is_some()
        {
            return None;
        }
        doc = Some(child.text.clone());
    }
    Some(doc.unwrap_or_default())
}
fn parse_data_port(e: &Elem) -> Option<processcraft_core::bpmn_data::DataPort> {
    if e.ns != B
        || !matches!(
            e.name.as_str(),
            "dataInput" | "dataOutput" | "inputDataItem" | "outputDataItem"
        )
        || !attrs(e, &["id", "name", "itemSubjectRef", "isCollection"])
        || e.children.iter().any(|c| c.name == "documentation")
    {
        return None;
    }
    Some(processcraft_core::bpmn_data::DataPort {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        name: e.attrs.get("name").cloned(),
        item_aware: parse_item_aware(e)?,
        is_collection: optional_bool(e.attrs.get("isCollection"))?,
    })
}
fn parse_io_set(child: &Elem, input: bool) -> Option<processcraft_core::bpmn_data::IoSet> {
    if child.ns != B || !attrs(child, &["id", "name"]) {
        return None;
    }
    let mut set = processcraft_core::bpmn_data::IoSet {
        base: take_anonymous_base(child),
        id: optional_id(child)?,
        name: child.attrs.get("name").cloned(),
        ..Default::default()
    };
    for reference in &child.children {
        if reference.ns != B || !reference.attrs.is_empty() || !reference.children.is_empty() {
            return None;
        }
        let value = reference.text.trim().parse().ok()?;
        match (input, reference.name.as_str()) {
            (true, "dataInputRefs") | (false, "dataOutputRefs") => set.data_refs.push(value),
            (true, "optionalInputRefs") | (false, "optionalOutputRefs") => {
                set.optional_refs.push(value)
            }
            (true, "whileExecutingInputRefs") | (false, "whileExecutingOutputRefs") => {
                set.while_executing_refs.push(value)
            }
            (true, "outputSetRefs") | (false, "inputSetRefs") => set.counterpart_refs.push(value),
            _ => return None,
        }
    }
    Some(set)
}
fn parse_io_specification(e: &Elem) -> Option<processcraft_core::bpmn_data::IoSpecification> {
    use processcraft_core::bpmn_data::IoSpecification;
    if !attrs(e, &["id"]) {
        return None;
    }
    let mut io = IoSpecification {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        ..Default::default()
    };
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "dataInput" | "dataOutput"
                if attrs(child, &["id", "name", "itemSubjectRef", "isCollection"])
                    && !child.children.iter().any(|c| c.name == "documentation") =>
            {
                let port = parse_data_port(child)?;
                if child.name == "dataInput" {
                    io.inputs.push(port);
                } else {
                    io.outputs.push(port);
                }
            }
            "inputSet" | "outputSet" if attrs(child, &["id", "name"]) => {
                let input = child.name == "inputSet";
                let set = parse_io_set(child, input)?;
                if input {
                    io.input_sets.push(set);
                } else {
                    io.output_sets.push(set);
                }
            }
            _ => return None,
        }
    }
    if io.input_sets.is_empty() || io.output_sets.is_empty() {
        return None;
    }
    Some(io)
}
fn parse_data_association(
    e: &Elem,
    owner: &ElementSelector,
) -> Option<processcraft_core::bpmn_data::DataAssociation> {
    use processcraft_core::bpmn_data::{DataAssociation, DataAssociationKind};
    if !attrs(e, &["id"]) {
        return None;
    }
    let kind = match e.name.as_str() {
        "dataInputAssociation" => DataAssociationKind::Input,
        "dataOutputAssociation" => DataAssociationKind::Output,
        _ => return None,
    };
    let mut sources = Vec::new();
    let mut target = None;
    let mut transformation = None;
    let mut assignments = Vec::new();
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "sourceRef"
                if target.is_none() && child.attrs.is_empty() && child.children.is_empty() =>
            {
                sources.push(child.text.trim().parse().ok()?)
            }
            "targetRef"
                if target.is_none() && child.attrs.is_empty() && child.children.is_empty() =>
            {
                target = Some(child.text.trim().parse().ok()?)
            }
            "transformation"
                if target.is_some() && transformation.is_none() && assignments.is_empty() =>
            {
                transformation = Some(parse_expression(child, true)?)
            }
            "assignment"
                if target.is_some()
                    && attrs(child, &["id"])
                    && child.children.len() == 2
                    && assignments.len() < 256 =>
            {
                if child.children[0].name != "from" || child.children[1].name != "to" {
                    return None;
                }
                assignments.push(processcraft_core::bpmn_data::DataAssignment {
                    base: take_anonymous_base(child),
                    id: optional_id(child)?,
                    from: parse_expression(&child.children[0], false)?,
                    to: parse_expression(&child.children[1], false)?,
                });
            }
            _ => return None,
        }
    }
    Some(DataAssociation {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        owner: owner.clone(),
        kind,
        sources,
        target: target?,
        transformation,
        assignments,
    })
}
fn activity_io(
    e: &Elem,
    owner: &ElementSelector,
) -> Option<(
    Option<processcraft_core::bpmn_data::IoSpecification>,
    Vec<processcraft_core::bpmn_data::DataAssociation>,
)> {
    let mut io = None;
    let mut associations = Vec::new();
    for child in &e.children {
        match child.name.as_str() {
            "ioSpecification" if io.is_none() => io = Some(parse_io_specification(child)?),
            "dataInputAssociation" | "dataOutputAssociation" => {
                associations.push(parse_data_association(child, owner)?)
            }
            _ => {}
        }
    }
    if !associations.is_empty() && io.is_none() {
        return None;
    }
    Some((io, associations))
}
fn parse_event(
    e: &Elem,
    kind: NodeKind,
    owner: &ElementSelector,
    target_namespace: &str,
) -> Option<(
    String,
    Option<NodeDetails>,
    Vec<processcraft_core::bpmn_data::DataAssociation>,
)> {
    let catch = matches!(
        kind,
        NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
    );
    let mut doc = None;
    let mut properties = Vec::new();
    let mut ports = Vec::new();
    let mut associations = Vec::new();
    let mut set = None;
    let mut definitions = Vec::new();
    let mut definition_refs = Vec::new();
    let mut phase = 0u8;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "documentation"
                if phase == 0
                    && doc.is_none()
                    && child.attrs.is_empty()
                    && child.children.is_empty() =>
            {
                doc = Some(child.text.clone())
            }
            "property"
                if phase <= 1
                    && attrs(child, &["id", "name", "itemSubjectRef"])
                    && !child.children.iter().any(|c| c.name == "documentation") =>
            {
                phase = 1;
                properties.push(EventProperty {
                    base: take_anonymous_base(child),
                    id: optional_id(child)?,
                    name: child.attrs.get("name").cloned(),
                    item_aware: parse_item_aware(child)?,
                });
            }
            "dataOutput" if catch && phase <= 2 => {
                phase = 2;
                ports.push(parse_data_port(child)?);
            }
            "dataInput" if !catch && phase <= 2 => {
                phase = 2;
                ports.push(parse_data_port(child)?);
            }
            "dataOutputAssociation" if catch && phase <= 3 => {
                phase = 3;
                associations.push(parse_data_association(child, owner)?);
            }
            "dataInputAssociation" if !catch && phase <= 3 => {
                phase = 3;
                associations.push(parse_data_association(child, owner)?);
            }
            "outputSet" if catch && phase <= 4 && set.is_none() => {
                phase = 4;
                set = Some(parse_io_set(child, false)?);
            }
            "inputSet" if !catch && phase <= 4 && set.is_none() => {
                phase = 4;
                set = Some(parse_io_set(child, true)?);
            }
            "eventDefinitionRef"
                if phase <= 6 && child.attrs.is_empty() && child.children.is_empty() =>
            {
                phase = 6;
                definition_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
            }
            _ if phase <= 5 => {
                phase = 5;
                definitions.push(parse_event_definition(child, target_namespace)?);
            }
            _ => return None,
        }
    }
    let attached_to = if kind == NodeKind::BoundaryEvent {
        Some(selector(e, "attachedToRef")?)
    } else {
        None
    };
    let cancel_activity = optional_bool(e.attrs.get("cancelActivity"))?;
    let parallel_multiple = optional_bool(e.attrs.get("parallelMultiple"))?;
    let is_interrupting = optional_bool(e.attrs.get("isInterrupting"))?;
    let io = if ports.is_empty() && set.is_none() {
        None
    } else if catch {
        Some(EventIo::Catch {
            outputs: ports,
            output_set: set,
        })
    } else {
        Some(EventIo::Throw {
            inputs: ports,
            input_set: set,
        })
    };
    let details = if definitions.is_empty()
        && definition_refs.is_empty()
        && properties.is_empty()
        && io.is_none()
        && attached_to.is_none()
        && cancel_activity.is_none()
        && parallel_multiple.is_none()
        && is_interrupting.is_none()
    {
        None
    } else {
        Some(NodeDetails::Event {
            definitions,
            attached_to,
            cancel_activity,
            definition_refs,
            parallel_multiple,
            is_interrupting,
            properties,
            io,
        })
    };
    Some((doc.unwrap_or_default(), details, associations))
}
fn activity_properties(
    e: &Elem,
    namespace: &str,
) -> Option<Option<processcraft_core::bpmn_activity::ActivityProperties>> {
    use processcraft_core::bpmn_activity::ActivityProperties;
    if ["messageRef", "operationRef"]
        .iter()
        .any(|key| e.attrs.contains_key(*key) && !e.catalog_qnames.contains_key(*key))
    {
        return None;
    }
    let mut result = ActivityProperties {
        is_for_compensation: optional_bool(e.attrs.get("isForCompensation"))?,
        start_quantity: e.attrs.get("startQuantity").cloned(),
        completion_quantity: e.attrs.get("completionQuantity").cloned(),
        default_flow: e.attrs.get("default").map(|v| v.parse()).transpose().ok()?,
        triggered_by_event: optional_bool(e.attrs.get("triggeredByEvent"))?,
        ad_hoc_ordering: match e.attrs.get("ordering").map(String::as_str) {
            None => None,
            Some("Parallel") => Some(processcraft_core::bpmn_activity::AdHocOrdering::Parallel),
            Some("Sequential") => Some(processcraft_core::bpmn_activity::AdHocOrdering::Sequential),
            _ => return None,
        },
        cancel_remaining_instances: optional_bool(e.attrs.get("cancelRemainingInstances"))?,
        transaction_method: e.attrs.get("method").cloned(),
        implementation: e.attrs.get("implementation").cloned(),
        message_ref: e.catalog_qnames.get("messageRef").cloned(),
        operation_ref: e.catalog_qnames.get("operationRef").cloned(),
        instantiate: optional_bool(e.attrs.get("instantiate"))?,
        script_format: e.attrs.get("scriptFormat").cloned(),
        ..Default::default()
    };
    for child in &e.children {
        match child.name.as_str() {
            "property" => result.properties.push(parse_property(child)?),
            "resourceRole" | "performer" | "humanPerformer" | "potentialOwner" => result
                .resource_roles
                .push(parse_resource_role(child, namespace)?),
            "rendering" if e.name == "userTask" => result.renderings.push(parse_rendering(child)?),
            "rendering" => return None,
            _ => {}
        }
        if child.name == "completionCondition" && e.name == "adHocSubProcess" {
            if child.ns != B || result.completion_condition.is_some() {
                return None;
            }
            result.completion_condition = Some(parse_expression(child, false)?);
        }
        if child.name == "script" {
            if e.name != "scriptTask" || child.ns != B || result.script.is_some() {
                return None;
            }
            result.script = Some(mixed_body(child, true)?);
        }
    }
    if result == ActivityProperties::default() {
        Some(None)
    } else {
        Some(Some(result))
    }
}
fn activity_parts(e: &Elem, namespace: &str) -> Option<(String, Option<LoopCharacteristics>)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut lp = None;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        if child.name == "documentation"
            && !seen_doc
            && child.attrs.is_empty()
            && child.children.is_empty()
        {
            doc = child.text.clone();
            seen_doc = true;
        } else if matches!(
            child.name.as_str(),
            "standardLoopCharacteristics" | "multiInstanceLoopCharacteristics"
        ) && lp.is_none()
        {
            lp = Some(loop_characteristics(child, namespace)?);
        } else if (child.name == "script" && e.name == "scriptTask")
            || matches!(
                child.name.as_str(),
                "ioSpecification"
                    | "property"
                    | "dataInputAssociation"
                    | "dataOutputAssociation"
                    | "resourceRole"
                    | "performer"
                    | "humanPerformer"
                    | "potentialOwner"
                    | "rendering"
            )
        {
            continue;
        } else {
            return None;
        }
    }
    Some((doc, lp))
}
fn supported_event_details(node: &Node) -> bool {
    matches!(node.details, Some(NodeDetails::Event { .. }))
}
fn parse_lane_set(
    set: &Elem,
    parent: Option<ElementSelector>,
    lanes: &mut Vec<Lane>,
    depth: usize,
) -> Option<()> {
    if depth > 32
        || set.ns != B
        || !matches!(set.name.as_str(), "laneSet" | "childLaneSet")
        || !attrs(set, &["id"])
    {
        return None;
    }
    for lane in &set.children {
        if lane.ns != B || lane.name != "lane" || !attrs(lane, &["id", "name"]) {
            return None;
        }
        let lane_id = element_identity(lane)?;
        let mut members = Vec::new();
        let mut child_set = None;
        let mut child_set_base = None;
        let mut seen_child_set = false;
        for child in &lane.children {
            if child.ns != B {
                return None;
            }
            match child.name.as_str() {
                "flowNodeRef"
                    if child.attrs.is_empty() && child.children.is_empty() && !seen_child_set =>
                {
                    members.push(child.text.parse().ok()?)
                }
                "childLaneSet" if !seen_child_set => {
                    seen_child_set = true;
                    child_set = optional_id(child)?;
                    child_set_base = take_anonymous_base(child);
                    if child_set_base.is_none() && child_set.is_none() && child.children.is_empty()
                    {
                        child_set_base = Some(Default::default());
                    }
                    parse_lane_set(child, Some(lane_id.selector()), lanes, depth + 1)?;
                }
                _ => return None,
            }
        }
        lanes.push(Lane {
            child_lane_set_base: child_set_base,
            base: take_anonymous_base(lane),
            child_lane_set_id: child_set,
            id: lane_id,
            name: lane.attrs.get("name").cloned(),
            parent: parent.clone(),
            members,
        });
    }
    Some(())
}
fn parse_container(e: &Elem, depth: usize, target_namespace: &str) -> Option<Process> {
    if depth > 32 {
        return None;
    }
    let mut p = Process {
        activity_metadata: None,
        lane_set_base: None,
        base_extras: anonymous_extras(e),
        is_executable: optional_bool(e.attrs.get("isExecutable"))?,
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        documentation: anonymous_documentation(e, String::new()),
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    };
    let mut documentation_nodes = Vec::new();
    let mut seen_lane_set = false;
    let mut process_metadata = processcraft_core::bpmn_activity::ProcessActivityMetadata::default();
    if e.name == "process" {
        process_metadata.process_type = match e.attrs.get("processType").map(String::as_str) {
            None => None,
            Some("None") => Some(processcraft_core::bpmn_activity::ProcessType::None),
            Some("Public") => Some(processcraft_core::bpmn_activity::ProcessType::Public),
            Some("Private") => Some(processcraft_core::bpmn_activity::ProcessType::Private),
            _ => return None,
        };
        process_metadata.is_closed = optional_bool(e.attrs.get("isClosed"))?;
        process_metadata.definitional_collaboration_ref = e
            .catalog_qnames
            .get("definitionalCollaborationRef")
            .map(|q| event_reference(q, target_namespace));
    }
    for n in &e.children {
        if e.name == "process" && n.ns == B {
            match n.name.as_str() {
                "supportedInterfaceRef" if attrs(n, &[]) && n.children.is_empty() => {
                    process_metadata
                        .callable
                        .supported_interface_refs
                        .push(event_reference(
                            n.catalog_text_qname.as_ref()?,
                            target_namespace,
                        ));
                    continue;
                }
                "ioBinding" => {
                    process_metadata
                        .callable
                        .io_bindings
                        .push(parse_io_binding(n, target_namespace)?);
                    continue;
                }
                "auditing" if process_metadata.auditing.is_none() => {
                    process_metadata.auditing = Some(parse_audit_monitor(n)?);
                    continue;
                }
                "monitoring" if process_metadata.monitoring.is_none() => {
                    process_metadata.monitoring = Some(parse_audit_monitor(n)?);
                    continue;
                }
                "property" => {
                    process_metadata.properties.push(parse_property(n)?);
                    continue;
                }
                "resourceRole" | "performer" | "humanPerformer" | "potentialOwner" => {
                    process_metadata
                        .resource_roles
                        .push(parse_resource_role(n, target_namespace)?);
                    continue;
                }
                "correlationSubscription" => {
                    process_metadata
                        .correlation_subscriptions
                        .push(parse_correlation_subscription(n, target_namespace)?);
                    continue;
                }
                "supports" if attrs(n, &[]) && n.children.is_empty() => {
                    process_metadata.supports.push(event_reference(
                        n.catalog_text_qname.as_ref()?,
                        target_namespace,
                    ));
                    continue;
                }
                _ => {}
            }
        }
        if matches!(
            e.name.as_str(),
            "subProcess" | "adHocSubProcess" | "transaction"
        ) && n.ns == B
            && matches!(
                n.name.as_str(),
                "standardLoopCharacteristics" | "multiInstanceLoopCharacteristics"
            )
        {
            continue;
        }
        if e.name == "adHocSubProcess" && n.ns == B && n.name == "completionCondition" {
            continue;
        }
        if n.ns == B && n.name == "documentation" {
            documentation_nodes.push(structured_documentation(n)?);
            continue;
        }
        if n.ns != B {
            return None;
        }
        if n.name == "ioSpecification" {
            if e.name == "process" {
                if p.io_specification.is_some() {
                    return None;
                }
                p.io_specification = Some(parse_io_specification(n)?);
            }
            continue;
        }
        if matches!(
            e.name.as_str(),
            "subProcess" | "adHocSubProcess" | "transaction"
        ) && matches!(
            n.name.as_str(),
            "property"
                | "dataInputAssociation"
                | "dataOutputAssociation"
                | "resourceRole"
                | "performer"
                | "humanPerformer"
                | "potentialOwner"
        ) {
            continue;
        }
        if n.name == "laneSet" {
            if seen_lane_set {
                return None;
            }
            seen_lane_set = true;
            p.lane_set_id = optional_id(n)?;
            p.lane_set_base = take_anonymous_base(n);
            if p.lane_set_base.is_none() && p.lane_set_id.is_none() && n.children.is_empty() {
                p.lane_set_base = Some(Default::default());
            }
            parse_lane_set(n, None, &mut p.lanes, 0)?;
            continue;
        }
        if n.name == "group" {
            if !attrs(n, &["id", "categoryValueRef"]) || !n.children.is_empty() {
                return None;
            }
            p.groups.push(processcraft_core::bpmn_model::Group {
                base: take_anonymous_base(n),
                id: element_identity(n)?,
                category_value_ref: n
                    .catalog_qnames
                    .get("categoryValueRef")
                    .map(|q| event_reference(q, target_namespace)),
            });
            continue;
        }
        if n.name == "textAnnotation" {
            if !attrs(n, &["id", "textFormat"]) {
                return None;
            }
            let mut text = None;
            let mut doc = String::new();
            let mut seen_text = false;
            let mut seen_doc = false;
            for c in &n.children {
                if c.ns != B {
                    return None;
                }
                match c.name.as_str() {
                    "text" if !seen_text => {
                        text = Some(mixed_body(
                            c,
                            n.attrs
                                .get("textFormat")
                                .is_some_and(|format| format != "text/plain"),
                        )?);
                        seen_text = true;
                    }
                    "documentation" if !seen_doc => {
                        if !c.attrs.is_empty() || !c.children.is_empty() {
                            return None;
                        }
                        doc = c.text.clone();
                        seen_doc = true;
                    }
                    _ => return None,
                }
            }
            p.nodes.push(Node {
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                item_aware: None,
                id: element_identity(n)?,
                name: None,
                documentation: anonymous_documentation(n, doc),
                kind: NodeKind::TextAnnotation,
                details: Some(NodeDetails::TextAnnotation {
                    text_format: n.attrs.get("textFormat").cloned(),
                    text,
                }),
                owner: String::new(),
                contact: String::new(),
                activity: None,
                io_specification: None,
                loop_characteristics: None,
            });
            continue;
        }
        if n.name == "association" {
            if !attrs(n, &["id", "sourceRef", "targetRef", "associationDirection"]) {
                return None;
            }
            let direction = match n.attrs.get("associationDirection").map(String::as_str) {
                None => None,
                Some("None") => Some(processcraft_core::bpmn_model::AssociationDirection::None),
                Some("One") => Some(processcraft_core::bpmn_model::AssociationDirection::One),
                Some("Both") => Some(processcraft_core::bpmn_model::AssociationDirection::Both),
                _ => return None,
            };
            p.flows.push(Flow {
                message_ref: None,
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                id: element_identity(n)?,
                name: None,
                kind: FlowKind::Association,
                source: Some(event_reference(
                    n.catalog_qnames.get("sourceRef")?,
                    target_namespace,
                )),
                target: Some(event_reference(
                    n.catalog_qnames.get("targetRef")?,
                    target_namespace,
                )),
                documentation: anonymous_documentation(n, documentation(n)?),
                condition: None,
                association_direction: direction,
            });
            continue;
        }
        if n.name == "dataObject" {
            if !attrs(n, &["id", "name", "isCollection", "itemSubjectRef"])
                || n.children.iter().any(|c| c.name != "dataState")
            {
                return None;
            }
            p.data_objects
                .push(processcraft_core::bpmn_data::DataObject {
                    flow_metadata: take_flow_metadata(n),
                    base: take_anonymous_base(n),
                    id: optional_id(n)?,
                    name: n.attrs.get("name").cloned(),
                    is_collection: optional_bool(n.attrs.get("isCollection"))?,
                    item_aware: parse_item_aware(n)?,
                });
            continue;
        }
        if matches!(
            n.name.as_str(),
            "dataObjectReference" | "dataStoreReference"
        ) {
            let object = n.name == "dataObjectReference";
            if !attrs(
                n,
                if object {
                    &["id", "name", "dataObjectRef", "itemSubjectRef"]
                } else {
                    &["id", "name", "dataStoreRef", "itemSubjectRef"]
                },
            ) {
                return None;
            }
            let object_reference = if object {
                n.attrs
                    .get("dataObjectRef")
                    .map(|v| v.parse())
                    .transpose()
                    .ok()?
            } else {
                None
            };
            let store_reference = if object {
                None
            } else {
                n.catalog_qnames
                    .get("dataStoreRef")
                    .cloned()
                    .map(processcraft_core::bpmn_data::DataStoreRef::Qualified)
            };
            p.nodes.push(Node {
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                item_aware: parse_item_aware(n)?,
                id: element_identity(n)?,
                name: n.attrs.get("name").cloned(),
                documentation: anonymous_documentation(n, item_documentation(n)?),
                kind: if object {
                    NodeKind::DataObjectReference
                } else {
                    NodeKind::DataStoreReference
                },
                details: Some(if object {
                    NodeDetails::DataObjectReference {
                        data_object_ref: object_reference,
                    }
                } else {
                    NodeDetails::DataStoreReference {
                        data_store_ref: store_reference,
                    }
                }),
                owner: String::new(),
                contact: String::new(),
                activity: None,
                io_specification: None,
                loop_characteristics: None,
            });
            continue;
        }
        if n.name == "callActivity" {
            if !attrs(
                n,
                &[
                    "id",
                    "name",
                    "calledElement",
                    "isForCompensation",
                    "startQuantity",
                    "completionQuantity",
                    "default",
                ],
            ) {
                return None;
            }
            let (doc, lp) = activity_parts(n, target_namespace)?;
            let node_id = element_identity(n)?;
            let (io, associations) = activity_io(n, &node_id.selector())?;
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                item_aware: None,
                id: node_id,
                name: n.attrs.get("name").cloned(),
                documentation: anonymous_documentation(n, doc),
                kind: NodeKind::CallActivity,
                details: Some(NodeDetails::CallActivity {
                    called_element: n.catalog_qnames.get("calledElement").cloned(),
                }),
                owner: String::new(),
                contact: String::new(),
                activity: activity_properties(n, target_namespace)?,
                io_specification: io,
                loop_characteristics: lp,
            });
            continue;
        }
        if matches!(
            n.name.as_str(),
            "subProcess" | "adHocSubProcess" | "transaction"
        ) {
            if !attrs(
                n,
                &[
                    "id",
                    "name",
                    "isForCompensation",
                    "startQuantity",
                    "completionQuantity",
                    "default",
                    "triggeredByEvent",
                    "ordering",
                    "cancelRemainingInstances",
                    "method",
                ],
            ) {
                return None;
            }
            let loops: Vec<_> = n
                .children
                .iter()
                .filter(|c| {
                    matches!(
                        c.name.as_str(),
                        "standardLoopCharacteristics" | "multiInstanceLoopCharacteristics"
                    )
                })
                .collect();
            if loops.len() > 1 {
                return None;
            }
            let lp = match loops.first() {
                Some(lp) => Some(loop_characteristics(lp, target_namespace)?),
                None => None,
            };
            let child = parse_container(n, depth + 1, target_namespace)?;
            let (io, associations) = activity_io(n, &child.id.selector())?;
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                item_aware: None,
                id: child.id,
                name: child.name,
                documentation: child.documentation,
                kind: match n.name.as_str() {
                    "adHocSubProcess" => NodeKind::AdHocSubProcess,
                    "transaction" => NodeKind::Transaction,
                    _ => NodeKind::SubProcess,
                },
                details: Some(NodeDetails::SubProcess {
                    content: Box::new(FlowContainer {
                        lane_set_base: child.lane_set_base,
                        lane_set_id: child.lane_set_id,
                        groups: child.groups,
                        data_objects: child.data_objects,
                        data_associations: child.data_associations,
                        nodes: child.nodes,
                        lanes: child.lanes,
                        flows: child.flows,
                    }),
                }),
                owner: String::new(),
                contact: String::new(),
                activity: activity_properties(n, target_namespace)?,
                io_specification: io,
                loop_characteristics: lp,
            });
            continue;
        }
        let kind = match n.name.as_str() {
            "task" => Some(NodeKind::Task),
            "userTask" => Some(NodeKind::UserTask),
            "manualTask" => Some(NodeKind::ManualTask),
            "serviceTask" => Some(NodeKind::ServiceTask),
            "sendTask" => Some(NodeKind::SendTask),
            "receiveTask" => Some(NodeKind::ReceiveTask),
            "scriptTask" => Some(NodeKind::ScriptTask),
            "businessRuleTask" => Some(NodeKind::BusinessRuleTask),
            "inclusiveGateway" => Some(NodeKind::InclusiveGateway),
            "complexGateway" => Some(NodeKind::ComplexGateway),
            "eventBasedGateway" => Some(NodeKind::EventBasedGateway),
            "intermediateThrowEvent" => Some(NodeKind::IntermediateThrowEvent),
            "intermediateCatchEvent" => Some(NodeKind::IntermediateCatchEvent),
            "boundaryEvent" => Some(NodeKind::BoundaryEvent),
            "startEvent" => Some(NodeKind::StartEvent),
            "endEvent" => Some(NodeKind::EndEvent),
            "exclusiveGateway" => Some(NodeKind::ExclusiveGateway),
            "parallelGateway" => Some(NodeKind::ParallelGateway),
            _ => None,
        };
        if let Some(kind) = kind {
            if !attrs(
                n,
                if matches!(kind, NodeKind::BoundaryEvent) {
                    &[
                        "id",
                        "name",
                        "attachedToRef",
                        "cancelActivity",
                        "parallelMultiple",
                    ]
                } else if matches!(kind, NodeKind::StartEvent) {
                    &["id", "name", "parallelMultiple", "isInterrupting"]
                } else if matches!(kind, NodeKind::IntermediateCatchEvent) {
                    &["id", "name", "parallelMultiple"]
                } else if matches!(
                    kind,
                    NodeKind::ExclusiveGateway
                        | NodeKind::InclusiveGateway
                        | NodeKind::ParallelGateway
                        | NodeKind::ComplexGateway
                        | NodeKind::EventBasedGateway
                ) {
                    &[
                        "id",
                        "name",
                        "default",
                        "gatewayDirection",
                        "instantiate",
                        "eventGatewayType",
                    ]
                } else if kind.is_activity() {
                    &[
                        "id",
                        "name",
                        "isForCompensation",
                        "startQuantity",
                        "completionQuantity",
                        "default",
                        "implementation",
                        "instantiate",
                        "scriptFormat",
                        "messageRef",
                        "operationRef",
                    ]
                } else {
                    &["id", "name"]
                },
            ) {
                return None;
            }
            let node_id = element_identity(n)?;
            let mut event_associations = Vec::new();
            let (documentation, details) = if matches!(
                kind,
                NodeKind::StartEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent
                    | NodeKind::EndEvent
            ) {
                let (doc, details, associations) =
                    parse_event(n, kind, &node_id.selector(), target_namespace)?;
                event_associations = associations;
                (doc, details)
            } else if matches!(
                kind,
                NodeKind::ExclusiveGateway
                    | NodeKind::InclusiveGateway
                    | NodeKind::ParallelGateway
                    | NodeKind::ComplexGateway
                    | NodeKind::EventBasedGateway
            ) {
                gateway_details(n)?
            } else if kind.is_activity() {
                (activity_parts(n, target_namespace)?.0, None)
            } else {
                (documentation(n)?, None)
            };
            let lp = if kind.is_activity() {
                activity_parts(n, target_namespace)?.1
            } else {
                None
            };
            let (io, associations) = if kind.is_activity() {
                activity_io(n, &node_id.selector())?
            } else {
                (None, Vec::new())
            };
            p.data_associations.extend(associations);
            p.data_associations.extend(event_associations);
            p.nodes.push(Node {
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                item_aware: None,
                id: node_id,
                name: n.attrs.get("name").cloned(),
                documentation: anonymous_documentation(n, documentation),
                kind,
                owner: String::new(),
                contact: String::new(),
                activity: if kind.is_activity() {
                    activity_properties(n, target_namespace)?
                } else {
                    None
                },
                io_specification: io,
                loop_characteristics: lp,
                details,
            });
        } else if n.name == "sequenceFlow"
            && attrs(n, &["id", "name", "sourceRef", "targetRef", "isImmediate"])
        {
            p.flows.push(Flow {
                message_ref: None,
                flow_metadata: take_flow_metadata(n),
                base_extras: anonymous_extras(n),
                association_direction: None,
                id: element_identity(n)?,
                kind: FlowKind::Sequence,
                source: Some(EventReference::Local(selector(n, "sourceRef")?)),
                target: Some(EventReference::Local(selector(n, "targetRef")?)),
                name: n.attrs.get("name").cloned(),
                documentation: anonymous_documentation(n, flow_parts(n)?.0),
                condition: flow_parts(n)?.1,
            });
        } else {
            return None;
        }
    }
    if documentation_nodes.len() == 1
        && documentation_nodes[0].id.is_none()
        && documentation_nodes[0].text_format.is_none()
        && matches!(&documentation_nodes[0].content, processcraft_core::bpmn_xml::MixedXml::Structured(content) if content.nodes.len() == 1 && matches!(&content.nodes[0], processcraft_core::bpmn_xml::XmlNode::Text { text } if !text.is_empty()))
    {
        let processcraft_core::bpmn_xml::MixedXml::Structured(content) =
            &documentation_nodes[0].content
        else {
            unreachable!()
        };
        let processcraft_core::bpmn_xml::XmlNode::Text { text } = &content.nodes[0] else {
            unreachable!()
        };
        p.documentation = text.clone().into();
    } else if !documentation_nodes.is_empty() {
        p.documentation =
            processcraft_core::bpmn_metadata::DocumentationSet::Structured(documentation_nodes);
    }
    if process_metadata != processcraft_core::bpmn_activity::ProcessActivityMetadata::default() {
        p.activity_metadata = Some(process_metadata);
    }
    Some(p)
}
struct Scope<'a> {
    id: &'a ElementIdentity,
    nodes: &'a [Node],
    lanes: &'a [Lane],
    flows: &'a [Flow],
    groups: &'a [processcraft_core::bpmn_model::Group],
    data_objects: &'a [processcraft_core::bpmn_data::DataObject],
    data_associations: &'a [processcraft_core::bpmn_data::DataAssociation],
}
fn scopes(d: &BpmnDocument) -> Option<Vec<Scope<'_>>> {
    let mut pending: Vec<(Scope<'_>, usize)> = d
        .processes
        .iter()
        .map(|p| {
            (
                Scope {
                    id: &p.id,
                    nodes: &p.nodes,
                    lanes: &p.lanes,
                    flows: &p.flows,
                    groups: &p.groups,
                    data_objects: &p.data_objects,
                    data_associations: &p.data_associations,
                },
                0,
            )
        })
        .collect();
    let mut result = Vec::new();
    let mut count = 0usize;
    while let Some((scope, depth)) = pending.pop() {
        if depth > 32 {
            return None;
        }
        count = count.checked_add(scope.nodes.len() + scope.lanes.len() + scope.flows.len())?;
        if count > 10_000 {
            return None;
        }
        for n in scope.nodes {
            if let Some(NodeDetails::SubProcess { content }) = &n.details {
                if !content.data_associations.is_empty() {
                    return None;
                }
                pending.push((
                    Scope {
                        id: &n.id,
                        nodes: &content.nodes,
                        lanes: &content.lanes,
                        flows: &content.flows,
                        groups: &content.groups,
                        data_objects: &content.data_objects,
                        data_associations: &content.data_associations,
                    },
                    depth + 1,
                ));
            }
        }
        result.push(scope);
    }
    Some(result)
}
fn parse_participant_association(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_collaboration::ParticipantAssociation> {
    if e.ns != B
        || e.name != "participantAssociation"
        || !attrs(e, &["id"])
        || e.children.len() != 2
    {
        return None;
    }
    let reference = |child: &Elem, name: &str| -> Option<EventReference> {
        if child.ns != B || child.name != name || !attrs(child, &[]) || !child.children.is_empty() {
            return None;
        }
        Some(event_reference(
            child.catalog_text_qname.as_ref()?,
            namespace,
        ))
    };
    Some(
        processcraft_core::bpmn_collaboration::ParticipantAssociation {
            id: optional_id(e)?,
            base: take_anonymous_base(e),
            inner: Some(reference(&e.children[0], "innerParticipantRef")?),
            outer: Some(reference(&e.children[1], "outerParticipantRef")?),
        },
    )
}
fn parse_conversation(
    e: &Elem,
    parent: Option<ElementSelector>,
    out: &mut Vec<processcraft_core::bpmn_collaboration::ConversationNode>,
    depth: usize,
    namespace: &str,
) -> Option<()> {
    use processcraft_core::bpmn_collaboration::{ConversationKind, ConversationNode};
    if depth > 32
        || !e.text.trim().is_empty()
        || !attrs(e, &["id", "name", "calledCollaborationRef"])
    {
        return None;
    }
    let kind = match e.name.as_str() {
        "conversation" => ConversationKind::Conversation,
        "subConversation" => ConversationKind::SubConversation,
        "callConversation" => ConversationKind::CallConversation,
        _ => return None,
    };
    let node_id = element_identity(e)?;
    let mut refs = Vec::new();
    let mut message_flow_refs = Vec::new();
    let mut call = matches!(kind, ConversationKind::CallConversation).then(|| {
        processcraft_core::bpmn_collaboration::CallConversationData {
            called_collaboration_ref: e
                .catalog_qnames
                .get("calledCollaborationRef")
                .map(|q| event_reference(q, namespace)),
            participant_associations: Vec::new(),
        }
    });
    if call.is_none() && e.attrs.contains_key("calledCollaborationRef") {
        return None;
    }
    let mut correlation_keys = Vec::new();
    let mut phase = 0;
    for c in &e.children {
        if c.ns != B {
            return None;
        }
        match c.name.as_str() {
            "participantRef" if phase == 0 && c.attrs.is_empty() && c.children.is_empty() => {
                refs.push(event_reference(c.catalog_text_qname.as_ref()?, namespace))
            }
            "messageFlowRef" if phase <= 1 && c.attrs.is_empty() && c.children.is_empty() => {
                phase = 1;
                message_flow_refs.push(event_reference(c.catalog_text_qname.as_ref()?, namespace));
            }
            "correlationKey" if phase <= 2 => {
                phase = 2;
                correlation_keys.push(parse_correlation_key(c, namespace)?);
            }
            "participantAssociation" if phase <= 3 && call.is_some() => {
                phase = 3;
                call.as_mut()?
                    .participant_associations
                    .push(parse_participant_association(c, namespace)?);
            }
            "conversation" | "subConversation" | "callConversation"
                if phase <= 3 && matches!(kind, ConversationKind::SubConversation) =>
            {
                phase = 3;
                parse_conversation(c, Some(node_id.selector()), out, depth + 1, namespace)?
            }
            _ => return None,
        }
    }
    out.push(ConversationNode {
        message_flow_refs,
        call,
        correlation_keys,
        base: take_anonymous_base(e),
        id: node_id,
        name: e.attrs.get("name").cloned(),
        kind,
        participant_refs: refs,
        parent,
    });
    Some(())
}
fn parse_participant(e: &Elem, namespace: &str) -> Option<Participant> {
    if !e.text.trim().is_empty() || !attrs(e, &["id", "name", "processRef"]) {
        return None;
    }
    let mut interface_refs = Vec::new();
    let mut endpoint_refs = Vec::new();
    let mut multiplicity = None;
    let mut phase = 0;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "interfaceRef" if phase == 0 && attrs(child, &[]) && child.children.is_empty() => {
                interface_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ))
            }
            "endPointRef" if phase <= 1 && attrs(child, &[]) && child.children.is_empty() => {
                phase = 1;
                endpoint_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    namespace,
                ));
            }
            "participantMultiplicity"
                if phase <= 1
                    && multiplicity.is_none()
                    && attrs(child, &["id", "minimum", "maximum"])
                    && child.children.is_empty() =>
            {
                phase = 2;
                multiplicity = Some(processcraft_core::model::ParticipantMultiplicity {
                    id: optional_id(child)?,
                    base: take_anonymous_base(child),
                    minimum: child
                        .attrs
                        .get("minimum")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                    maximum: child
                        .attrs
                        .get("maximum")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                });
            }
            _ => return None,
        }
    }
    Some(Participant {
        interface_refs,
        endpoint_refs,
        multiplicity,
        base: take_anonymous_base(e),
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        process: e
            .catalog_qnames
            .get("processRef")
            .map(|q| event_reference(q, namespace)),
    })
}
fn parse_collaboration_artifact(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_collaboration::CollaborationArtifact> {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    match e.name.as_str() {
        "group" if attrs(e, &["id", "categoryValueRef"]) && e.children.is_empty() => Some(
            CollaborationArtifact::Group(processcraft_core::bpmn_model::Group {
                base: take_anonymous_base(e),
                id: element_identity(e)?,
                category_value_ref: e
                    .catalog_qnames
                    .get("categoryValueRef")
                    .map(|q| event_reference(q, namespace)),
            }),
        ),
        "textAnnotation" if attrs(e, &["id", "textFormat"]) => {
            let mut text = None;
            for child in &e.children {
                if child.ns != B || child.name != "text" || text.is_some() {
                    return None;
                }
                text = Some(mixed_body(child, true)?);
            }
            Some(CollaborationArtifact::TextAnnotation(Box::new(Node {
                flow_metadata: None,
                base_extras: anonymous_extras(e),
                item_aware: None,
                id: element_identity(e)?,
                name: None,
                documentation: anonymous_documentation(e, String::new()),
                kind: NodeKind::TextAnnotation,
                details: Some(NodeDetails::TextAnnotation {
                    text_format: e.attrs.get("textFormat").cloned(),
                    text,
                }),
                owner: String::new(),
                contact: String::new(),
                activity: None,
                io_specification: None,
                loop_characteristics: None,
            })))
        }
        "association" if attrs(e, &["id", "sourceRef", "targetRef", "associationDirection"]) => {
            let association_direction =
                match e.attrs.get("associationDirection").map(String::as_str) {
                    None => None,
                    Some("None") => Some(processcraft_core::bpmn_model::AssociationDirection::None),
                    Some("One") => Some(processcraft_core::bpmn_model::AssociationDirection::One),
                    Some("Both") => Some(processcraft_core::bpmn_model::AssociationDirection::Both),
                    _ => return None,
                };
            Some(CollaborationArtifact::Association(Box::new(Flow {
                message_ref: None,
                flow_metadata: None,
                base_extras: anonymous_extras(e),
                association_direction,
                id: element_identity(e)?,
                kind: FlowKind::Association,
                source: Some(event_reference(
                    e.catalog_qnames.get("sourceRef")?,
                    namespace,
                )),
                target: Some(event_reference(
                    e.catalog_qnames.get("targetRef")?,
                    namespace,
                )),
                name: None,
                documentation: anonymous_documentation(e, documentation(e)?),
                condition: None,
            })))
        }
        _ => None,
    }
}
fn write_collaboration_artifacts(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    if let Some(extras) = extras {
        for artifact in &extras.artifacts {
            match artifact {
                CollaborationArtifact::Group(group) => write_scope(
                    xml,
                    &[],
                    &[],
                    &[],
                    &[],
                    std::slice::from_ref(group),
                    context,
                    namespace,
                ),
                CollaborationArtifact::TextAnnotation(node) => write_scope(
                    xml,
                    std::slice::from_ref(node.as_ref()),
                    &[],
                    &[],
                    &[],
                    &[],
                    context,
                    namespace,
                ),
                CollaborationArtifact::Association(flow) => write_scope(
                    xml,
                    &[],
                    std::slice::from_ref(flow.as_ref()),
                    &[],
                    &[],
                    &[],
                    context,
                    namespace,
                ),
            }
        }
    }
}
fn parse_collaboration_extras_child(
    e: &Elem,
    namespace: &str,
    value: &mut processcraft_core::bpmn_collaboration::CollaborationExtras,
) -> Option<bool> {
    use processcraft_core::bpmn_collaboration::{ConversationAssociation, MessageFlowAssociation};
    match e.name.as_str() {
        "group" | "textAnnotation" | "association" => value
            .artifacts
            .push(parse_collaboration_artifact(e, namespace)?),
        "participantAssociation" => value
            .participant_associations
            .push(parse_participant_association(e, namespace)?),
        "conversationAssociation" | "messageFlowAssociation" => {
            let (inner, outer) = if e.name == "conversationAssociation" {
                ("innerConversationNodeRef", "outerConversationNodeRef")
            } else {
                ("innerMessageFlowRef", "outerMessageFlowRef")
            };
            if !attrs(e, &["id", inner, outer])
                || !e.text.trim().is_empty()
                || !e.children.is_empty()
            {
                return None;
            }
            let inner = Some(event_reference(e.catalog_qnames.get(inner)?, namespace));
            let outer = Some(event_reference(e.catalog_qnames.get(outer)?, namespace));
            if e.name == "conversationAssociation" {
                value
                    .conversation_associations
                    .push(ConversationAssociation {
                        id: optional_id(e)?,
                        base: take_anonymous_base(e),
                        inner,
                        outer,
                    });
            } else {
                value
                    .message_flow_associations
                    .push(MessageFlowAssociation {
                        id: optional_id(e)?,
                        base: take_anonymous_base(e),
                        inner,
                        outer,
                    });
            }
        }
        "choreographyRef" if attrs(e, &[]) && e.children.is_empty() => value
            .choreography_refs
            .push(event_reference(e.catalog_text_qname.as_ref()?, namespace)),
        _ => return Some(false),
    }
    Some(true)
}
fn parse_partner_refs(e: &Elem, namespace: &str) -> Option<Vec<EventReference>> {
    if !e.text.trim().is_empty() {
        return None;
    }
    let mut result = Vec::new();
    for child in &e.children {
        if child.ns != B
            || child.name != "participantRef"
            || !attrs(child, &[])
            || !child.children.is_empty()
        {
            return None;
        }
        result.push(event_reference(
            child.catalog_text_qname.as_ref()?,
            namespace,
        ));
    }
    Some(result)
}
fn collaboration_child_phase(name: &str) -> Option<u8> {
    Some(match name {
        "participant" => 0,
        "messageFlow" => 1,
        "group" | "textAnnotation" | "association" => 2,
        "conversation" | "subConversation" | "callConversation" => 3,
        "conversationAssociation" => 4,
        "participantAssociation" => 5,
        "messageFlowAssociation" => 6,
        "correlationKey" => 7,
        "choreographyRef" => 8,
        "conversationLink" => 9,
        _ => return None,
    })
}
fn parse_collaboration(e: &Elem, namespace: &str) -> Option<Collaboration> {
    use processcraft_core::bpmn_collaboration::ConversationLink;
    if !e.text.trim().is_empty() || !attrs(e, &["id", "name", "isClosed"]) {
        return None;
    }
    let mut extras = processcraft_core::bpmn_collaboration::CollaborationExtras {
        is_closed: optional_bool(e.attrs.get("isClosed"))?,
        ..Default::default()
    };
    let mut c = Collaboration {
        kind: (e.name == "globalConversation").then_some(
            processcraft_core::bpmn_collaboration::CollaborationRootKind::GlobalConversation,
        ),
        extras: None,
        correlation_keys: Vec::new(),
        base: take_anonymous_base(e),
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let mut phase = 0;
    for child in &e.children {
        let next = collaboration_child_phase(&child.name)?;
        if next < phase {
            return None;
        }
        phase = next;
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "participant" => c.participants.push(parse_participant(child, namespace)?),
            "messageFlow"
                if attrs(
                    child,
                    &["id", "name", "sourceRef", "targetRef", "messageRef"],
                ) =>
            {
                c.flows.push(Flow {
                    message_ref: child
                        .catalog_qnames
                        .get("messageRef")
                        .map(|q| event_reference(q, namespace)),
                    flow_metadata: take_flow_metadata(child),
                    base_extras: anonymous_extras(child),
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Message,
                    source: Some(event_reference(
                        child.catalog_qnames.get("sourceRef")?,
                        namespace,
                    )),
                    target: Some(event_reference(
                        child.catalog_qnames.get("targetRef")?,
                        namespace,
                    )),
                    documentation: anonymous_documentation(child, documentation(child)?),
                    condition: None,
                })
            }
            "conversation" | "subConversation" | "callConversation" => {
                parse_conversation(child, None, &mut c.conversation_nodes, 0, namespace)?
            }
            "correlationKey" => c
                .correlation_keys
                .push(parse_correlation_key(child, namespace)?),
            "conversationLink"
                if attrs(child, &["id", "name", "sourceRef", "targetRef"])
                    && child.children.is_empty() =>
            {
                c.conversation_links.push(ConversationLink {
                    base: take_anonymous_base(child),
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    source: Some(event_reference(
                        child.catalog_qnames.get("sourceRef")?,
                        namespace,
                    )),
                    target: Some(event_reference(
                        child.catalog_qnames.get("targetRef")?,
                        namespace,
                    )),
                })
            }
            _ if parse_collaboration_extras_child(child, namespace, &mut extras)? => {}
            _ => return None,
        }
    }
    if extras != Default::default() {
        c.extras = Some(extras);
    }
    Some(c)
}
fn parse_choreography(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_collaboration::Choreography> {
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    if !e.text.trim().is_empty() || !attrs(e, &["id", "name", "isClosed"]) {
        return None;
    }
    let mut extras = processcraft_core::bpmn_collaboration::CollaborationExtras {
        is_closed: optional_bool(e.attrs.get("isClosed"))?,
        ..Default::default()
    };
    let mut c = Choreography {
        extras: None,
        conversation_nodes: Vec::new(),
        conversation_links: Vec::new(),
        correlation_keys: Vec::new(),
        base: take_anonymous_base(e),
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    };
    let mut phase = 0;
    for child in &e.children {
        let next = collaboration_child_phase(&child.name).unwrap_or(10);
        if next < phase {
            return None;
        }
        phase = next;
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "participant" => c.participants.push(parse_participant(child, namespace)?),
            "messageFlow"
                if attrs(
                    child,
                    &["id", "name", "sourceRef", "targetRef", "messageRef"],
                ) =>
            {
                c.message_flows.push(Flow {
                    message_ref: child
                        .catalog_qnames
                        .get("messageRef")
                        .map(|q| event_reference(q, namespace)),
                    flow_metadata: take_flow_metadata(child),
                    base_extras: anonymous_extras(child),
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Message,
                    source: Some(event_reference(
                        child.catalog_qnames.get("sourceRef")?,
                        namespace,
                    )),
                    target: Some(event_reference(
                        child.catalog_qnames.get("targetRef")?,
                        namespace,
                    )),
                    documentation: anonymous_documentation(child, documentation(child)?),
                    condition: None,
                })
            }
            "conversation" | "subConversation" | "callConversation" => {
                parse_conversation(child, None, &mut c.conversation_nodes, 0, namespace)?
            }
            "correlationKey" => c
                .correlation_keys
                .push(parse_correlation_key(child, namespace)?),
            "conversationLink"
                if attrs(child, &["id", "name", "sourceRef", "targetRef"])
                    && child.children.is_empty() =>
            {
                c.conversation_links
                    .push(processcraft_core::bpmn_collaboration::ConversationLink {
                        base: take_anonymous_base(child),
                        id: element_identity(child)?,
                        name: child.attrs.get("name").cloned(),
                        source: Some(event_reference(
                            child.catalog_qnames.get("sourceRef")?,
                            namespace,
                        )),
                        target: Some(event_reference(
                            child.catalog_qnames.get("targetRef")?,
                            namespace,
                        )),
                    })
            }
            "choreographyTask" if attrs(child, &["id", "name", "initiatingParticipantRef"]) => {
                let mut participants = Vec::new();
                let mut messages = Vec::new();
                let mut correlation_keys = Vec::new();
                for reference in &child.children {
                    if reference.ns == B && reference.name == "correlationKey" {
                        correlation_keys.push(parse_correlation_key(reference, namespace)?);
                        continue;
                    }
                    if reference.ns != B
                        || !reference.attrs.is_empty()
                        || !reference.children.is_empty()
                    {
                        return None;
                    }
                    match reference.name.as_str() {
                        "participantRef" => {
                            match event_reference(reference.catalog_text_qname.as_ref()?, namespace)
                            {
                                EventReference::Local(id) => participants.push(id),
                                _ => return None,
                            }
                        }
                        "messageFlowRef" => {
                            match event_reference(reference.catalog_text_qname.as_ref()?, namespace)
                            {
                                EventReference::Local(id) => messages.push(id),
                                _ => return None,
                            }
                        }
                        _ => return None,
                    }
                }
                if participants.len() < 2 || !(1..=2).contains(&messages.len()) {
                    return None;
                }
                c.activities.push(ChoreographyActivity {
                    flow_metadata: take_flow_metadata(child),
                    correlation_keys,
                    base: take_anonymous_base(child),
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    initiating_participant: selector(child, "initiatingParticipantRef")?,
                    participant_refs: participants,
                    message_flow_refs: messages,
                });
            }
            "sequenceFlow"
                if attrs(
                    child,
                    &["id", "name", "sourceRef", "targetRef", "isImmediate"],
                ) =>
            {
                c.sequence_flows.push(Flow {
                    message_ref: None,
                    flow_metadata: take_flow_metadata(child),
                    base_extras: anonymous_extras(child),
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Sequence,
                    source: Some(EventReference::Local(selector(child, "sourceRef")?)),
                    target: Some(EventReference::Local(selector(child, "targetRef")?)),
                    documentation: anonymous_documentation(child, flow_parts(child)?.0),
                    condition: flow_parts(child)?.1,
                })
            }
            _ if parse_collaboration_extras_child(child, namespace, &mut extras)? => {}
            _ => return None,
        }
    }
    if extras != Default::default() {
        c.extras = Some(extras);
    }
    Some(c)
}
fn optional_id(e: &Elem) -> Option<Option<ElementIdentity>> {
    e.attrs
        .get("id")
        .map(|v| v.parse::<XmlId>().map(ElementIdentity::from))
        .transpose()
        .ok()
}
fn element_identity(e: &Elem) -> Option<ElementIdentity> {
    Some(optional_id(e)?.unwrap_or_else(ElementIdentity::anonymous))
}
fn selector(e: &Elem, key: &str) -> Option<ElementSelector> {
    Some(id(e, key)?.into())
}
fn catalog_item(e: &Elem) -> Option<ItemDefinition> {
    if !attrs(e, &["id", "structureRef", "isCollection", "itemKind"]) || !e.children.is_empty() {
        return None;
    }
    Some(ItemDefinition {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
        is_collection: optional_bool(e.attrs.get("isCollection"))?,
        item_kind: match e.attrs.get("itemKind").map(String::as_str) {
            None => None,
            Some("Information") => Some(ItemKind::Information),
            Some("Physical") => Some(ItemKind::Physical),
            _ => return None,
        },
    })
}
fn catalog_interface(e: &Elem) -> Option<Interface> {
    if !attrs(e, &["id", "name", "implementationRef"])
        || e.children.is_empty()
        || e.children.len() > 256
    {
        return None;
    }
    let mut operations = Vec::new();
    for op in &e.children {
        if op.ns != B || op.name != "operation" || !attrs(op, &["id", "name", "implementationRef"])
        {
            return None;
        }
        let mut input = None;
        let mut output = None;
        let mut errors = Vec::new();
        for child in &op.children {
            if child.ns != B || !child.attrs.is_empty() || !child.children.is_empty() {
                return None;
            }
            let q = child.catalog_text_qname.clone()?;
            match child.name.as_str() {
                "inMessageRef" if input.is_none() && output.is_none() && errors.is_empty() => {
                    input = Some(q)
                }
                "outMessageRef" if input.is_some() && output.is_none() && errors.is_empty() => {
                    output = Some(q)
                }
                "errorRef" if input.is_some() && errors.len() < 256 => errors.push(q),
                _ => return None,
            }
        }
        operations.push(Operation {
            base: take_anonymous_base(op),
            id: optional_id(op)?,
            name: op.attrs.get("name")?.clone(),
            implementation_ref: op.catalog_qnames.get("implementationRef").cloned(),
            in_message_ref: input?,
            out_message_ref: output,
            error_refs: errors,
        });
    }
    Some(Interface {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        name: e.attrs.get("name")?.clone(),
        implementation_ref: e.catalog_qnames.get("implementationRef").cloned(),
        operations,
    })
}
fn catalog_resource(e: &Elem) -> Option<BpmnResource> {
    if !attrs(e, &["id", "name"]) || e.children.len() > 256 {
        return None;
    }
    let mut parameters = Vec::new();
    for p in &e.children {
        if p.ns != B
            || p.name != "resourceParameter"
            || !attrs(p, &["id", "name", "type", "isRequired"])
            || !p.children.is_empty()
        {
            return None;
        }
        parameters.push(ResourceParameter {
            base: take_anonymous_base(p),
            id: optional_id(p)?,
            name: p.attrs.get("name").cloned(),
            type_ref: p.catalog_qnames.get("type").cloned(),
            is_required: optional_bool(p.attrs.get("isRequired"))?,
        });
    }
    Some(BpmnResource {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        name: e.attrs.get("name")?.clone(),
        parameters,
    })
}
fn activity_ports(document: &BpmnDocument) -> Option<std::collections::HashSet<ElementSelector>> {
    let mut ports = std::collections::HashSet::new();
    for node in scopes(document)?.iter().flat_map(|scope| scope.nodes) {
        if let Some(io) = &node.io_specification {
            ports.extend(
                io.inputs
                    .iter()
                    .chain(&io.outputs)
                    .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
            );
        }
        if let Some(NodeDetails::Event { io, .. }) = &node.details {
            match io {
                Some(EventIo::Catch { outputs, .. }) => ports.extend(
                    outputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
                ),
                Some(EventIo::Throw { inputs, .. }) => ports.extend(
                    inputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
                ),
                None => {}
            }
        }
    }
    Some(ports)
}
fn is_process_port(
    document: &BpmnDocument,
    plane: &ElementSelector,
    selector: &ElementSelector,
) -> bool {
    document
        .processes
        .iter()
        .filter(|p| p.id.selector() == *plane)
        .filter_map(|p| p.io_specification.as_ref())
        .flat_map(|io| io.inputs.iter().chain(&io.outputs))
        .any(|port| {
            port.id
                .as_ref()
                .is_some_and(|id| id.selector() == *selector)
        })
}
fn event_definition_identity(definition: &EventDefinition) -> Option<&ElementIdentity> {
    match definition {
        EventDefinition::Message { id, .. }
        | EventDefinition::Signal { id, .. }
        | EventDefinition::Error { id, .. }
        | EventDefinition::Escalation { id, .. }
        | EventDefinition::Conditional { id, .. }
        | EventDefinition::Link { id, .. }
        | EventDefinition::Cancel { id, .. }
        | EventDefinition::Compensate { id, .. }
        | EventDefinition::Terminate { id, .. }
        | EventDefinition::Timer { id, .. } => id.as_ref(),
    }
}
// Called only while importing a new BPMN source; native archives are never promoted on read.
fn imported_root_selector(document: &mut BpmnDocument, name: &str) -> Option<ElementSelector> {
    fn ensure(id: &mut Option<ElementIdentity>) -> ElementSelector {
        id.get_or_insert_with(ElementIdentity::anonymous).selector()
    }
    Some(match name {
        "rootElement"
        | "globalTask"
        | "globalManualTask"
        | "globalBusinessRuleTask"
        | "globalScriptTask"
        | "globalUserTask" => ensure(&mut document.global_tasks.last_mut()?.id),
        "correlationProperty" => ensure(&mut document.correlation_properties.last_mut()?.id),
        "itemDefinition" => ensure(&mut document.catalog.item_definitions.last_mut()?.id),
        "interface" => ensure(&mut document.catalog.interfaces.last_mut()?.id),
        "resource" => ensure(&mut document.catalog.resources.last_mut()?.id),
        "dataStore" => ensure(&mut document.data_stores.last_mut()?.id),
        "partnerEntity" => ensure(&mut document.partner_entities.last_mut()?.id),
        "partnerRole" => ensure(&mut document.partner_roles.last_mut()?.id),
        "endPoint" => ensure(&mut document.endpoints.last_mut()?.id),
        "category" => document.categories.last()?.id.selector(),
        "message" | "signal" | "error" | "escalation" => match document.event_resources.last()? {
            EventResource::Message { id, .. }
            | EventResource::Signal { id, .. }
            | EventResource::Error { id, .. }
            | EventResource::Escalation { id, .. } => id.selector(),
        },
        "choreography" => document.choreographies.last()?.id.selector(),
        "collaboration" | "globalConversation" => document
            .additional_collaborations
            .last()
            .or(document.collaboration.as_ref())?
            .id
            .selector(),
        "process" => document.processes.last()?.id.selector(),
        name if name.ends_with("EventDefinition") => {
            let id = match document.root_event_definitions.last_mut()? {
                EventDefinition::Message { id, .. }
                | EventDefinition::Signal { id, .. }
                | EventDefinition::Error { id, .. }
                | EventDefinition::Escalation { id, .. }
                | EventDefinition::Conditional { id, .. }
                | EventDefinition::Link { id, .. }
                | EventDefinition::Cancel { id, .. }
                | EventDefinition::Compensate { id, .. }
                | EventDefinition::Terminate { id, .. }
                | EventDefinition::Timer { id, .. } => id,
            };
            ensure(id)
        }
        _ => return None,
    })
}
fn event_definition_resources_valid(document: &BpmnDocument, definition: &EventDefinition) -> bool {
    fn local_or_external(
        reference: &EventReference,
        matches_local: impl FnOnce(&ElementSelector) -> bool,
    ) -> bool {
        match reference {
            EventReference::Local(selector) => matches_local(selector),
            EventReference::Qualified(_) => true,
        }
    }
    match definition {
        EventDefinition::Signal { signal_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Signal { id, .. } if id.selector() == *selector))),
        EventDefinition::Message { message_ref, operation_ref, .. } => message_ref.as_ref().is_none_or(|reference| local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Message { id, .. } if id.selector() == *selector)))) && operation_ref.as_ref().is_none_or(|reference| local_or_external(reference, |selector| document.catalog.interfaces.iter().flat_map(|i| &i.operations).any(|op| op.id.as_ref().is_some_and(|id| id.selector() == *selector)))),
        EventDefinition::Error { error_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Error { id, .. } if id.selector() == *selector))),
        EventDefinition::Escalation { escalation_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Escalation { id, .. } if id.selector() == *selector))),
        _ => true,
    }
}
fn parse_profile(bytes: &[u8]) -> Option<BpmnDocument> {
    let mut root = tree(bytes)?;
    let mut pending_metadata = BTreeMap::new();
    extract_owner_metadata(&mut root, &mut pending_metadata)?;
    let target_namespace = root.attrs.get("targetNamespace")?.clone();
    extract_flow_metadata(&mut root, &target_namespace)?;
    // Until every owner has a metadata writer, foreign attributes must remain source-preserved.
    fn has_foreign_attributes(e: &Elem) -> bool {
        if e.opaque || (e.ns == DI && e.name == "extension") {
            return false;
        }
        let supported_di = e.ns == BD
            && matches!(
                e.name.as_str(),
                "BPMNPlane" | "BPMNShape" | "BPMNEdge" | "BPMNLabel"
            );
        (!supported_di && !e.foreign_attributes.is_empty())
            || e.children.iter().any(has_foreign_attributes)
    }
    if root.children.iter().any(has_foreign_attributes) {
        return None;
    }
    if root.ns != B
        || root.name != "definitions"
        || !root.attrs.keys().all(|key| [
                "id",
                "targetNamespace",
                "name",
                "expressionLanguage",
                "typeLanguage",
                "exporter",
                "exporterVersion",
            ].contains(&key.as_str()))
        || !root.attrs.contains_key("targetNamespace")
        // The editable profile must fit the native metadata limits.
        || ["targetNamespace", "name", "expressionLanguage", "typeLanguage", "exporter", "exporterVersion"]
            .iter()
            .any(|name| root.attrs.get(*name).is_some_and(|value| value.len() > 16_384))
    {
        return None;
    }
    let mut d = BpmnDocument::new(String::new(), element_identity(&root)?);
    d.definitions = Some(DefinitionsMetadata {
        namespaces: if root.foreign_attributes.is_empty() {
            Vec::new()
        } else {
            xml_namespaces(&root)
        },
        foreign_attributes: root.foreign_attributes.clone(),
        target_namespace: root.attrs.get("targetNamespace")?.clone(),
        name: root.attrs.get("name").cloned(),
        expression_language: root.attrs.get("expressionLanguage").cloned(),
        type_language: root.attrs.get("typeLanguage").cloned(),
        exporter: root.attrs.get("exporter").cloned(),
        exporter_version: root.attrs.get("exporterVersion").cloned(),
        imports: Vec::new(),
    });
    let mut seen_non_import = false;
    let mut root_phase = 0;
    let mut imported_order = Vec::new();
    for e in &root.children {
        let phase = match (e.ns.as_str(), e.name.as_str()) {
            (B, "import") => 0,
            (B, "extension") => 1,
            (BD, "BPMNDiagram") => 3,
            (B, "relationship") => 4,
            _ => 2,
        };
        if phase < root_phase {
            return None;
        }
        root_phase = phase;
        if e.ns == B && e.name == "import" {
            if seen_non_import
                || !attrs(e, &["namespace", "location", "importType"])
                || !e.children.is_empty()
                || !e.text.trim().is_empty()
            {
                return None;
            }
            let imports = &mut d.definitions.as_mut()?.imports;
            if imports.len() >= 256 {
                return None;
            }
            let namespace = e.attrs.get("namespace")?.clone();
            let location = e.attrs.get("location")?.clone();
            let import_type = e.attrs.get("importType")?.clone();
            if [&namespace, &location, &import_type]
                .iter()
                .any(|value| value.len() > 16_384)
            {
                return None;
            }
            imports.push(DefinitionsImport {
                namespace,
                location,
                import_type,
            });
            continue;
        }
        seen_non_import = true;
        match (e.ns.as_str(), e.name.as_str()) {
            (B, "correlationProperty") => {
                d.correlation_properties
                    .push(parse_correlation_property(e, &target_namespace)?);
            }
            (
                B,
                "rootElement"
                | "globalTask"
                | "globalManualTask"
                | "globalBusinessRuleTask"
                | "globalScriptTask"
                | "globalUserTask",
            ) => {
                d.global_tasks
                    .push(parse_global_task(e, &target_namespace)?);
            }
            (B, "extension") => {
                if !attrs(e, &["definition", "mustUnderstand"]) || d.extensions.len() >= 256 {
                    return None;
                }
                let must_understand = optional_bool(e.attrs.get("mustUnderstand"))?;
                if must_understand == Some(true) {
                    return None;
                }
                let documentation = e
                    .children
                    .iter()
                    .map(structured_documentation)
                    .collect::<Option<Vec<_>>>()?;
                d.extensions.push(DefinitionsExtension {
                    documentation,
                    definition: e.catalog_qnames.get("definition").cloned(),
                    must_understand,
                });
            }
            (B, "relationship") => {
                if !attrs(e, &["id", "type", "direction"]) || d.relationships.len() >= 256 {
                    return None;
                }
                let direction = match e.attrs.get("direction").map(String::as_str) {
                    None => None,
                    Some("None") => Some(RelationshipDirection::None),
                    Some("Forward") => Some(RelationshipDirection::Forward),
                    Some("Backward") => Some(RelationshipDirection::Backward),
                    Some("Both") => Some(RelationshipDirection::Both),
                    _ => return None,
                };
                let mut sources = Vec::new();
                let mut targets = Vec::new();
                for child in &e.children {
                    if child.ns != B || !attrs(child, &[]) || !child.children.is_empty() {
                        return None;
                    }
                    match child.name.as_str() {
                        "source" if targets.is_empty() => {
                            sources.push(child.catalog_text_qname.clone()?)
                        }
                        "target" => targets.push(child.catalog_text_qname.clone()?),
                        _ => return None,
                    }
                }
                if sources.is_empty() || targets.is_empty() {
                    return None;
                }
                d.relationships.push(DefinitionsRelationship {
                    id: optional_id(e)?,
                    base: take_anonymous_base(e),
                    relationship_type: e.attrs.get("type")?.clone(),
                    direction,
                    sources,
                    targets,
                });
            }
            (
                B,
                "messageEventDefinition"
                | "signalEventDefinition"
                | "errorEventDefinition"
                | "escalationEventDefinition"
                | "conditionalEventDefinition"
                | "timerEventDefinition"
                | "linkEventDefinition"
                | "cancelEventDefinition"
                | "compensateEventDefinition"
                | "terminateEventDefinition",
            ) => d.root_event_definitions.push(parse_event_definition(
                e,
                root.attrs.get("targetNamespace")?,
            )?),
            (B, "itemDefinition") if d.catalog.item_definitions.len() < 256 => {
                d.catalog.item_definitions.push(catalog_item(e)?)
            }
            (B, "interface") if d.catalog.interfaces.len() < 256 => {
                d.catalog.interfaces.push(catalog_interface(e)?)
            }
            (B, "resource") if d.catalog.resources.len() < 256 => {
                d.catalog.resources.push(catalog_resource(e)?)
            }
            (B, "category") => {
                if !attrs(e, &["id", "name"]) {
                    return None;
                }
                let mut values = Vec::new();
                for child in &e.children {
                    if child.ns != B
                        || child.name != "categoryValue"
                        || !attrs(child, &["id", "value"])
                        || !child.children.is_empty()
                    {
                        return None;
                    }
                    values.push(processcraft_core::bpmn_model::CategoryValue {
                        base: take_anonymous_base(child),
                        id: element_identity(child)?,
                        value: child.attrs.get("value").cloned(),
                    });
                }
                d.categories.push(processcraft_core::bpmn_model::Category {
                    base: take_anonymous_base(e),
                    id: element_identity(e)?,
                    name: e.attrs.get("name").cloned(),
                    values,
                });
            }
            (B, "dataStore") => {
                if !attrs(
                    e,
                    &["id", "name", "capacity", "isUnlimited", "itemSubjectRef"],
                ) || e.children.iter().any(|c| c.name != "dataState")
                {
                    return None;
                }
                d.data_stores.push(processcraft_core::bpmn_data::DataStore {
                    base: take_anonymous_base(e),
                    id: optional_id(e)?,
                    name: e.attrs.get("name").cloned(),
                    capacity: e.attrs.get("capacity").cloned(),
                    is_unlimited: optional_bool(e.attrs.get("isUnlimited"))?,
                    item_aware: parse_item_aware(e)?,
                });
            }
            (B, "error" | "escalation") => {
                if !attrs(
                    e,
                    &["id", "name", "errorCode", "escalationCode", "structureRef"],
                ) || !e.children.is_empty()
                {
                    return None;
                }
                let root_id = element_identity(e)?;
                let name = e.attrs.get("name").cloned();
                let code = e
                    .attrs
                    .get(if e.name == "error" {
                        "errorCode"
                    } else {
                        "escalationCode"
                    })
                    .cloned();
                if e.attrs.contains_key(if e.name == "error" {
                    "escalationCode"
                } else {
                    "errorCode"
                }) {
                    return None;
                }
                d.event_resources.push(if e.name == "error" {
                    EventResource::Error {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        code,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                } else {
                    EventResource::Escalation {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        code,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                });
            }
            (B, "signal" | "message") => {
                if !attrs(
                    e,
                    if e.name == "signal" {
                        &["id", "name", "structureRef"]
                    } else {
                        &["id", "name", "itemRef"]
                    },
                ) || !e.children.is_empty()
                {
                    return None;
                }
                let root_id = element_identity(e)?;
                let name = e.attrs.get("name").cloned();
                d.event_resources.push(if e.name == "signal" {
                    EventResource::Signal {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                } else {
                    EventResource::Message {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        item_ref: e.catalog_qnames.get("itemRef").cloned(),
                    }
                });
            }
            (B, "partnerEntity" | "partnerRole") => {
                if !attrs(e, &["id", "name"]) {
                    return None;
                }
                let refs = parse_partner_refs(e, &target_namespace)?;
                if e.name == "partnerEntity" {
                    d.partner_entities
                        .push(processcraft_core::bpmn_collaboration::PartnerEntity {
                            id: optional_id(e)?,
                            base: take_anonymous_base(e),
                            name: e.attrs.get("name").cloned(),
                            participant_refs: refs,
                        });
                } else {
                    d.partner_roles
                        .push(processcraft_core::bpmn_collaboration::PartnerRole {
                            id: optional_id(e)?,
                            base: take_anonymous_base(e),
                            name: e.attrs.get("name").cloned(),
                            participant_refs: refs,
                        });
                }
            }
            (B, "endPoint") => {
                if !attrs(e, &["id"]) || !e.text.trim().is_empty() || !e.children.is_empty() {
                    return None;
                }
                d.endpoints
                    .push(processcraft_core::bpmn_collaboration::EndPoint {
                        id: optional_id(e)?,
                        base: take_anonymous_base(e),
                    });
            }
            (B, "choreography") => {
                d.choreographies
                    .push(parse_choreography(e, &target_namespace)?);
            }
            (B, "collaboration" | "globalConversation") => {
                let collaboration = parse_collaboration(e, &target_namespace)?;
                if d.collaboration.is_none() {
                    d.collaboration = Some(collaboration);
                } else {
                    d.additional_collaborations.push(collaboration);
                }
            }
            (B, "process") => {
                if !attrs(
                    e,
                    &[
                        "id",
                        "name",
                        "isExecutable",
                        "processType",
                        "isClosed",
                        "definitionalCollaborationRef",
                    ],
                ) || optional_bool(e.attrs.get("isExecutable")).is_none()
                {
                    return None;
                }
                let p = parse_container(e, 0, root.attrs.get("targetNamespace")?)?;
                d.processes.push(p);
            }
            (BD, "BPMNDiagram") => {
                if !attrs(e, &["id", "name", "documentation", "resolution"])
                    || e.children.is_empty()
                {
                    return None;
                }
                let plane = &e.children[0];
                if plane.ns != BD
                    || plane.name != "BPMNPlane"
                    || !di_attrs(plane, &["id", "bpmnElement"])
                {
                    return None;
                }
                let (plane_metadata, plane_offset) = parse_di_metadata(plane)?;
                let mut diagram = Diagram {
                    plane_metadata,
                    plane_order: Some(Vec::new()),
                    id: element_identity(e)?,
                    name: e.attrs.get("name").cloned(),
                    documentation: e.attrs.get("documentation").cloned(),
                    resolution: e
                        .attrs
                        .get("resolution")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                    label_styles: Vec::new(),
                    plane_id: element_identity(plane)?,
                    plane: selector(plane, "bpmnElement")?,
                    shapes: vec![],
                    edges: vec![],
                };
                for style in &e.children[1..] {
                    if style.ns != BD
                        || style.name != "BPMNLabelStyle"
                        || !attrs(style, &["id"])
                        || style.children.len() != 1
                    {
                        return None;
                    }
                    let font = &style.children[0];
                    if font.ns != DC
                        || font.name != "Font"
                        || !attrs(
                            font,
                            &[
                                "name",
                                "size",
                                "isBold",
                                "isItalic",
                                "isUnderline",
                                "isStrikeThrough",
                            ],
                        )
                        || !font.children.is_empty()
                    {
                        return None;
                    }
                    diagram.label_styles.push(LabelStyle {
                        id: optional_id(style)?,
                        font: Some(DiFont {
                            name: font.attrs.get("name").cloned(),
                            size: font.attrs.get("size").map(|v| v.parse()).transpose().ok()?,
                            is_bold: optional_bool(font.attrs.get("isBold"))?,
                            is_italic: optional_bool(font.attrs.get("isItalic"))?,
                            is_underline: optional_bool(font.attrs.get("isUnderline"))?,
                            is_strike_through: optional_bool(font.attrs.get("isStrikeThrough"))?,
                        }),
                    });
                }
                for item in &plane.children[plane_offset..] {
                    if item.ns != BD
                        || !di_attrs(
                            item,
                            if item.name == "BPMNShape" {
                                &[
                                    "id",
                                    "bpmnElement",
                                    "isExpanded",
                                    "isHorizontal",
                                    "isMarkerVisible",
                                    "isMessageVisible",
                                    "participantBandKind",
                                    "choreographyActivityShape",
                                ]
                            } else {
                                &[
                                    "id",
                                    "bpmnElement",
                                    "sourceElement",
                                    "targetElement",
                                    "messageVisibleKind",
                                ]
                            },
                        )
                    {
                        return None;
                    }
                    let (di_metadata, offset) = parse_di_metadata(item)?;
                    let identity = element_identity(item)?;
                    diagram.plane_order.as_mut()?.push(identity.selector());
                    match item.name.as_str() {
                        "BPMNShape" => {
                            if !(offset + 1..=offset + 2).contains(&item.children.len()) {
                                return None;
                            }
                            let label = if item.children.len() == offset + 2 {
                                Some(parse_label(
                                    &item.children[offset + 1],
                                    root.attrs.get("targetNamespace")?,
                                )?)
                            } else {
                                None
                            };
                            let is_expanded = match item.attrs.get("isExpanded").map(String::as_str)
                            {
                                None => None,
                                Some("true" | "1") => Some(true),
                                Some("false" | "0") => Some(false),
                                _ => return None,
                            };
                            let participant_band_kind =
                                match item.attrs.get("participantBandKind").map(String::as_str) {
                                    None => None,
                                    Some("top_initiating") => {
                                        Some(ParticipantBandKind::TopInitiating)
                                    }
                                    Some("middle_initiating") => {
                                        Some(ParticipantBandKind::MiddleInitiating)
                                    }
                                    Some("bottom_initiating") => {
                                        Some(ParticipantBandKind::BottomInitiating)
                                    }
                                    Some("top_non_initiating") => {
                                        Some(ParticipantBandKind::TopNonInitiating)
                                    }
                                    Some("middle_non_initiating") => {
                                        Some(ParticipantBandKind::MiddleNonInitiating)
                                    }
                                    Some("bottom_non_initiating") => {
                                        Some(ParticipantBandKind::BottomNonInitiating)
                                    }
                                    _ => return None,
                                };
                            diagram.shapes.push(Shape {
                                di_metadata,
                                is_message_visible: optional_bool(
                                    item.attrs.get("isMessageVisible"),
                                )?,
                                participant_band_kind,
                                choreography_activity_shape: item
                                    .catalog_qnames
                                    .get("choreographyActivityShape")
                                    .map(|q| {
                                        if q.namespace == target_namespace {
                                            DiStyleRef::Local(q.local.clone().into())
                                        } else {
                                            DiStyleRef::Qualified(q.clone())
                                        }
                                    }),
                                id: identity,
                                element: selector(item, "bpmnElement")?,
                                is_expanded,
                                is_horizontal: optional_bool(item.attrs.get("isHorizontal"))?,
                                is_marker_visible: optional_bool(
                                    item.attrs.get("isMarkerVisible"),
                                )?,
                                bounds: bounds(&item.children[offset])?,
                                label,
                                label_visibility: Some(DiLabelVisibility::Explicit),
                                style: None,
                            });
                        }
                        "BPMNEdge" => {
                            let mut points = vec![];
                            let mut label = None;
                            for pt in &item.children[offset..] {
                                if pt.ns == BD && pt.name == "BPMNLabel" && label.is_none() {
                                    label =
                                        Some(parse_label(pt, root.attrs.get("targetNamespace")?)?);
                                    continue;
                                }
                                if label.is_some() {
                                    return None;
                                }
                                if pt.ns != DI
                                    || pt.name != "waypoint"
                                    || !attrs(pt, &["x", "y"])
                                    || !pt.children.is_empty()
                                {
                                    return None;
                                }
                                points.push(Point {
                                    x: number(pt, "x")?,
                                    y: number(pt, "y")?,
                                });
                            }
                            if points.len() < 2 {
                                return None;
                            }
                            let message_visible_kind =
                                match item.attrs.get("messageVisibleKind").map(String::as_str) {
                                    None => None,
                                    Some("initiating") => Some(MessageVisibleKind::Initiating),
                                    Some("non_initiating") => {
                                        Some(MessageVisibleKind::NonInitiating)
                                    }
                                    _ => return None,
                                };
                            diagram.edges.push(Edge {
                                di_metadata,
                                message_visible_kind,
                                id: identity,
                                element: selector(item, "bpmnElement")?,
                                source_element: item
                                    .attrs
                                    .get("sourceElement")
                                    .map(|v| v.parse())
                                    .transpose()
                                    .ok()?,
                                target_element: item
                                    .attrs
                                    .get("targetElement")
                                    .map(|v| v.parse())
                                    .transpose()
                                    .ok()?,
                                waypoints: points,
                                label,
                                label_visibility: Some(DiLabelVisibility::Explicit),
                                style: None,
                            });
                        }
                        _ => return None,
                    }
                }
                d.diagrams.push(diagram);
            }
            _ => return None,
        }
        if phase == 2 {
            imported_order.push(imported_root_selector(&mut d, &e.name)?);
        }
    }
    d.root_order = Some(imported_order);
    if d.root_event_definitions
        .iter()
        .any(|definition| !event_definition_resources_valid(&d, definition))
    {
        return None;
    }
    // Every flow and lane reference is validated in its owning scope.
    let all_scopes = scopes(&d)?;
    for p in &all_scopes {
        for node in p.nodes {
            if node.activity.as_ref().is_some_and(|activity| {
                !processcraft_core::validation::activity_reference_issues(&d, activity).is_empty()
            }) {
                return None;
            }
            if let Some(NodeDetails::CallActivity {
                called_element: Some(called_element),
            }) = &node.details
                && called_element.namespace == target_namespace
                && !d
                    .processes
                    .iter()
                    .any(|process| process.id.xml_id() == Some(&called_element.local))
                && !d.global_tasks.iter().any(|task| {
                    task.id.as_ref().and_then(ElementIdentity::xml_id)
                        == Some(&called_element.local)
                })
            {
                return None;
            }
            if node.details.is_some()
                && !matches!(
                    node.details,
                    Some(
                        NodeDetails::SubProcess { .. }
                            | NodeDetails::CallActivity { .. }
                            | NodeDetails::DataObjectReference { .. }
                            | NodeDetails::DataStoreReference { .. }
                            | NodeDetails::Gateway { .. }
                            | NodeDetails::TextAnnotation { .. }
                    )
                )
                && !supported_event_details(node)
            {
                return None;
            }
            if let Some(NodeDetails::Event {
                definitions,
                definition_refs,
                ..
            }) = &node.details
            {
                if definitions
                    .iter()
                    .any(|definition| !event_definition_resources_valid(&d, definition))
                {
                    return None;
                }
                for reference in definition_refs {
                    if let EventReference::Local(selector) = reference
                        && !d.root_event_definitions.iter().any(|definition| {
                            event_definition_identity(definition)
                                .is_some_and(|id| id.selector() == *selector)
                        })
                    {
                        return None;
                    }
                }
            }
            if let Some(NodeDetails::Event {
                attached_to: Some(target),
                ..
            }) = &node.details
                && !p
                    .nodes
                    .iter()
                    .any(|n| n.id == *target && n.kind.is_activity())
            {
                return None;
            }
        }
        for lane in p.lanes {
            if lane
                .members
                .iter()
                .any(|id| !p.nodes.iter().any(|n| &n.id == id))
            {
                return None;
            }
        }
        for f in p.flows {
            for reference in [&f.source, &f.target] {
                match reference {
                    Some(EventReference::Local(selector)) => {
                        if !p.nodes.iter().any(|n| n.id == *selector)
                            && !(f.kind == FlowKind::Association
                                && (p.groups.iter().any(|g| g.id == *selector)
                                    || p.flows.iter().any(|flow| {
                                        flow.kind == FlowKind::Sequence && flow.id == *selector
                                    })))
                        {
                            return None;
                        }
                    }
                    Some(EventReference::Qualified(_)) if f.kind == FlowKind::Association => {}
                    _ => return None,
                }
            }
        }
    }
    let activity_ports = activity_ports(&d)?;
    for dia in &d.diagrams {
        if dia
            .shapes
            .iter()
            .any(|shape| activity_ports.contains(&shape.element))
        {
            return None;
        }
        if let Some(c) = d.choreographies.iter().find(|c| c.id == dia.plane) {
            if dia.shapes.iter().any(|s| {
                !c.participants.iter().any(|p| p.id == s.element)
                    && !c.activities.iter().any(|a| a.id == s.element)
            }) || dia.edges.iter().any(|e| {
                !c.message_flows.iter().any(|f| f.id == e.element)
                    && !c.sequence_flows.iter().any(|f| f.id == e.element)
            }) {
                return None;
            }
            continue;
        }
        if let Some(collaboration) = &d.collaboration
            && collaboration.id == dia.plane
        {
            if dia.shapes.iter().any(|s| {
                !collaboration.participants.iter().any(|p| p.id == s.element)
                    && !collaboration
                        .conversation_nodes
                        .iter()
                        .any(|n| n.id == s.element)
                    && !all_scopes.iter().any(|p| {
                        p.nodes.iter().any(|n| n.id == s.element)
                            || p.lanes.iter().any(|l| l.id == s.element)
                            || p.groups.iter().any(|g| g.id == s.element)
                    })
            }) || dia.edges.iter().any(|e| {
                !collaboration.flows.iter().any(|f| f.id == e.element)
                    && !collaboration
                        .conversation_links
                        .iter()
                        .any(|f| f.id == e.element)
                    && !all_scopes.iter().any(|p| {
                        p.flows.iter().any(|f| f.id == e.element)
                            || p.data_associations
                                .iter()
                                .any(|a| a.id.as_ref().is_some_and(|id| id.selector() == e.element))
                    })
            }) {
                return None;
            }
            continue;
        }
        let first = all_scopes.iter().find(|p| *p.id == dia.plane)?;
        let mut contained = Vec::new();
        let mut pending = vec![first];
        while let Some(scope) = pending.pop() {
            contained.push(scope);
            for node in scope.nodes {
                if matches!(node.details, Some(NodeDetails::SubProcess { .. })) {
                    pending.push(all_scopes.iter().find(|p| *p.id == node.id)?);
                }
            }
        }
        if dia.shapes.iter().any(|s| {
            !is_process_port(&d, &dia.plane, &s.element)
                && !contained.iter().any(|p| {
                    p.nodes.iter().any(|n| n.id == s.element)
                        || p.lanes.iter().any(|l| l.id == s.element)
                        || p.groups.iter().any(|g| g.id == s.element)
                })
        }) || dia.edges.iter().any(|e| {
            !contained.iter().any(|p| {
                p.flows.iter().any(|f| f.id == e.element)
                    || p.data_associations
                        .iter()
                        .any(|a| a.id.as_ref().is_some_and(|id| id.selector() == e.element))
            })
        }) {
            return None;
        }
    }
    let mut validation = ProjectData::new("validation".into());
    validation.documents.push(Arc::new(d.clone()));
    validation.validate().ok()?;
    processcraft_core::metadata_visit::visit_base_metadata_mut(&mut d, |owner, base| {
        if let Some(id) = owner.and_then(ElementIdentity::xml_id)
            && let Some(pending) = pending_metadata.remove(&id.to_string())
        {
            *base = Some(processcraft_core::bpmn_metadata::BaseMetadata {
                namespaces: pending.extras.namespaces,
                documentation: pending.docs,
                foreign_attributes: pending.extras.foreign_attributes,
                extension_elements: pending.extras.extension_elements,
            });
        }
    });
    processcraft_core::metadata_visit::visit_legacy_metadata_mut(&mut d, |owner, docs, extras| {
        if let Some(id) = owner.xml_id()
            && let Some(pending) = pending_metadata.remove(&id.to_string())
        {
            *docs = documentation_set(pending.docs);
            if !pending.extras.namespaces.is_empty()
                || !pending.extras.foreign_attributes.is_empty()
                || pending.extras.extension_elements.is_some()
            {
                *extras = Some(pending.extras);
            }
        }
    });
    if !pending_metadata.is_empty()
        || has_unconsumed_anonymous_base(&root)
        || has_unconsumed_flow_metadata(&root)
        || collect_owner_metadata(&d).is_err()
    {
        return None;
    }
    Some(d)
}
// Process documentation is fully represented; reject all other common metadata until each
// owner has a qualified serializer, rather than silently discarding native fields.
fn collect_owner_metadata(document: &BpmnDocument) -> Result<BTreeMap<String, PendingBase>, ()> {
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
fn restore_owner_defaults(xml: &str, pending: &BTreeMap<String, PendingBase>) -> Option<String> {
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
                let is_bpmn = matches!(ns, ResolveResult::Bound(uri) if uri.0 == B);
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
                        || (parent_changed && desired_default != B));
                let mut renamed = None;
                let mut changed = false;
                if rewrite {
                    let context = context?;
                    let tag = if desired_default == B {
                        local_name.clone()
                    } else {
                        let prefix = context
                            .iter()
                            .find(|n| !n.prefix.is_empty() && n.uri == B)?
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
fn insert_owner_metadata(xml: &str, mut pending: BTreeMap<String, PendingBase>) -> Option<String> {
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
        if !matches!(namespace, ResolveResult::Bound(uri) if uri.0 == B) {
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
            let tag = if default == B {
                local_name.to_owned()
            } else {
                let prefix = context
                    .iter()
                    .find(|n| !n.prefix.is_empty() && n.uri == B)?
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

#[derive(Debug, Default)]
pub struct ExportReport {
    pub retained: Vec<String>,
    pub transformed: Vec<String>,
    pub omitted: Vec<String>,
    pub blocked: Vec<String>,
}
#[derive(Debug)]
pub struct ExportedBpmn {
    pub bytes: Vec<u8>,
    pub report: ExportReport,
}
#[derive(Debug)]
pub struct ExportFailure {
    pub report: ExportReport,
}
/// Caller must additionally account for project-level dictionary and navigation links.
struct ExportContext<'a> {
    document: &'a BpmnDocument,
    xml_ids: std::collections::HashMap<ElementSelector, &'a XmlId>,
}
impl<'a> ExportContext<'a> {
    fn new(document: &'a BpmnDocument) -> Self {
        let xml_ids = document
            .element_identities()
            .into_iter()
            .filter_map(|identity| identity.xml_id().map(|id| (identity.selector(), id)))
            .collect();
        Self { document, xml_ids }
    }
}
impl std::ops::Deref for ExportContext<'_> {
    type Target = BpmnDocument;
    fn deref(&self) -> &Self::Target {
        self.document
    }
}
fn write_participant_association(
    xml: &mut String,
    value: &processcraft_core::bpmn_collaboration::ParticipantAssociation,
    namespace: &str,
    context: &ExportContext,
) {
    xml.push_str(&format!("<participantAssociation xmlns=\"{B}\""));
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    if let Some(inner) = &value.inner {
        write_event_qname_text(xml, "innerParticipantRef", inner, namespace, context);
    }
    if let Some(outer) = &value.outer {
        write_event_qname_text(xml, "outerParticipantRef", outer, namespace, context);
    }
    xml.push_str("</participantAssociation>");
}
fn write_collaboration_associations(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    let Some(extras) = extras else {
        return;
    };
    for value in &extras.conversation_associations {
        let refs: Vec<_> = [
            ("innerConversationNodeRef", value.inner.as_ref()),
            ("outerConversationNodeRef", value.outer.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag = write_collaboration_qname_owner(
            xml,
            "conversationAssociation",
            &refs,
            namespace,
            context,
            &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
                scopes
            },
        );
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        finish_anonymous_base_empty(xml, &tag, value.id.as_ref(), value.base.as_ref());
    }
    for value in &extras.participant_associations {
        write_participant_association(xml, value, namespace, context);
    }
    for value in &extras.message_flow_associations {
        let refs: Vec<_> = [
            ("innerMessageFlowRef", value.inner.as_ref()),
            ("outerMessageFlowRef", value.outer.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag = write_collaboration_qname_owner(
            xml,
            "messageFlowAssociation",
            &refs,
            namespace,
            context,
            &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
                scopes
            },
        );
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        finish_anonymous_base_empty(xml, &tag, value.id.as_ref(), value.base.as_ref());
    }
}
fn write_conversation_links(
    xml: &mut String,
    links: &[processcraft_core::bpmn_collaboration::ConversationLink],
    namespace: &str,
    context: &ExportContext,
) {
    for link in links {
        let refs: Vec<_> = [
            ("sourceRef", link.source.as_ref()),
            ("targetRef", link.target.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag =
            write_collaboration_qname_owner(xml, "conversationLink", &refs, namespace, context, &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(link.base.as_ref(), &mut scopes);
                scopes
            });
        write_xml_id(xml, &link.id);
        write_xml_name(xml, link.name.as_deref());
        finish_anonymous_base_empty(xml, &tag, Some(&link.id), link.base.as_ref());
    }
}
fn write_collaboration_refs(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    if let Some(extras) = extras {
        for reference in &extras.choreography_refs {
            write_event_qname_text(xml, "choreographyRef", reference, namespace, context);
        }
    }
}
fn write_conversations(
    xml: &mut String,
    nodes: &[processcraft_core::bpmn_collaboration::ConversationNode],
    parent: Option<&ElementSelector>,
    namespace: &str,
    document: &ExportContext,
) {
    for node in nodes.iter().filter(|n| n.parent.as_ref() == parent) {
        let tag = match node.kind {
            processcraft_core::bpmn_collaboration::ConversationKind::Conversation => "conversation",
            processcraft_core::bpmn_collaboration::ConversationKind::SubConversation => {
                "subConversation"
            }
            processcraft_core::bpmn_collaboration::ConversationKind::CallConversation => {
                "callConversation"
            }
        };
        let refs: Vec<_> = node
            .call
            .as_ref()
            .and_then(|c| c.called_collaboration_ref.as_ref())
            .map(|r| vec![("calledCollaborationRef", r)])
            .unwrap_or_default();
        let mut scopes = Vec::new();
        collaboration_base_scopes(node.base.as_ref(), &mut scopes);
        for key in &node.correlation_keys {
            collaboration_base_scopes(key.base.as_ref(), &mut scopes);
        }
        if let Some(call) = &node.call {
            for value in &call.participant_associations {
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
            }
        }
        let tag = write_collaboration_qname_owner(xml, tag, &refs, namespace, document, &scopes);
        write_xml_id(xml, &node.id);
        write_xml_name(xml, node.name.as_deref());
        anonymous_base_attributes(xml, Some(&node.id), node.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&node.id), node.base.as_ref());
        for participant in &node.participant_refs {
            write_event_qname_text(xml, "participantRef", participant, namespace, document);
        }
        for reference in &node.message_flow_refs {
            write_event_qname_text(xml, "messageFlowRef", reference, namespace, document);
        }
        for key in &node.correlation_keys {
            write_correlation_key(xml, key, namespace, document);
        }
        write_conversations(xml, nodes, Some(&node.id.selector()), namespace, document);
        if let Some(call) = &node.call {
            for association in &call.participant_associations {
                write_participant_association(xml, association, namespace, document);
            }
        }
        xml.push_str(&format!("</{tag}>"));
    }
}
fn write_local_qname_text(xml: &mut String, tag: &str, id: &XmlId, namespace: &str) {
    if namespace.is_empty() {
        xml.push_str(&format!(
            "<pc_q:{tag} xmlns:pc_q=\"{B}\" xmlns=\"\">{id}</pc_q:{tag}>"
        ));
    } else {
        xml.push_str(&format!(
            "<{tag} xmlns:tns=\"{}\">tns:{id}</{tag}>",
            esc(namespace)
        ));
    }
}
fn local_qname_value(id: &XmlId, namespace: &str) -> String {
    if namespace.is_empty() {
        id.to_string()
    } else {
        format!("tns:{id}")
    }
}
fn write_local_qname_open(xml: &mut String, tag: &str, namespace: &str) -> String {
    if namespace.is_empty() {
        xml.push_str(&format!("<pc_q:{tag} xmlns:pc_q=\"{B}\" xmlns=\"\""));
        format!("pc_q:{tag}")
    } else {
        xml.push_str(&format!("<{tag} xmlns:tns=\"{}\"", esc(namespace)));
        tag.to_owned()
    }
}
fn collaboration_base_scopes<'a>(
    base: Option<&'a processcraft_core::bpmn_metadata::BaseMetadata>,
    scopes: &mut Vec<&'a [processcraft_core::bpmn_xml::NamespaceBinding]>,
) {
    use processcraft_core::bpmn_xml::MixedXml;
    if let Some(base) = base {
        if !base.namespaces.is_empty() {
            scopes.push(&base.namespaces);
        }
        for doc in &base.documentation {
            if let MixedXml::Structured(content) = &doc.content {
                scopes.push(&content.namespaces);
            }
        }
        if let Some(ext) = &base.extension_elements {
            scopes.push(&ext.content.namespaces);
        }
    }
}
fn collaboration_flow_scopes(flow: &Flow) -> Vec<&[processcraft_core::bpmn_xml::NamespaceBinding]> {
    use processcraft_core::{bpmn_metadata::DocumentationSet, bpmn_xml::MixedXml};
    let mut scopes = Vec::new();
    if let DocumentationSet::Structured(docs) = &flow.documentation {
        for doc in docs {
            if let MixedXml::Structured(content) = &doc.content {
                scopes.push(content.namespaces.as_slice());
            }
        }
    }
    if let Some(extras) = &flow.base_extras {
        if !extras.namespaces.is_empty() {
            scopes.push(&extras.namespaces);
        }
        if let Some(ext) = &extras.extension_elements {
            scopes.push(&ext.content.namespaces);
        }
    }
    scopes
}
fn write_collaboration_qname_owner(
    xml: &mut String,
    tag: &str,
    refs: &[(&str, &EventReference)],
    namespace: &str,
    context: &ExportContext,
    scopes: &[&[processcraft_core::bpmn_xml::NamespaceBinding]],
) -> String {
    let values: Vec<_> = refs
        .iter()
        .map(|(name, reference)| (*name, event_reference_qname(reference, namespace, context)))
        .collect();
    let empty_default = values.iter().any(|(_, q)| q.namespace.is_empty());
    let mut prefixes = std::collections::BTreeMap::<String, String>::new();
    let mut next_prefix = |uri: &str| -> String {
        if let Some(prefix) = prefixes.get(uri) {
            return prefix.clone();
        }
        let prefix = scopes
            .first()
            .and_then(|first| {
                first
                    .iter()
                    .find(|binding| {
                        !binding.prefix.is_empty()
                            && binding.prefix != "xml"
                            && binding.uri == uri
                            && !prefixes.values().any(|used| used == &binding.prefix)
                            && scopes
                                .iter()
                                .all(|scope| scope.iter().any(|n| n.prefix == binding.prefix))
                    })
                    .or_else(|| {
                        first.iter().find(|binding| {
                            !binding.prefix.is_empty()
                                && binding.prefix != "xml"
                                && !prefixes.values().any(|used| used == &binding.prefix)
                                && scopes
                                    .iter()
                                    .all(|scope| scope.iter().any(|n| n.prefix == binding.prefix))
                        })
                    })
            })
            .map(|binding| binding.prefix.clone())
            .unwrap_or_else(|| {
                let mut i = prefixes.len();
                loop {
                    let p = format!("pc_cref{i}");
                    if !prefixes.values().any(|used| used == &p) {
                        break p;
                    }
                    i += 1;
                }
            });
        prefixes.insert(uri.to_owned(), prefix.clone());
        prefix
    };
    let owner = if empty_default {
        format!("{}:{tag}", next_prefix(B))
    } else {
        tag.to_owned()
    };
    let mut attrs = Vec::new();
    for (name, q) in values {
        let lexical = if q.namespace.is_empty() || (!empty_default && q.namespace == B) {
            q.local.to_string()
        } else {
            format!("{}:{}", next_prefix(&q.namespace), q.local)
        };
        attrs.push((name, lexical));
    }
    xml.push('<');
    xml.push_str(&owner);
    if empty_default {
        xml.push_str(" xmlns=\"\"");
    }
    for (uri, prefix) in prefixes {
        xml.push_str(&format!(" xmlns:{prefix}=\"{}\"", esc(&uri)));
    }
    for (name, value) in attrs {
        xml.push_str(&format!(" {name}=\"{}\"", esc(&value)));
    }
    owner
}
fn write_participant(
    xml: &mut String,
    participant: &Participant,
    namespace: &str,
    document: &ExportContext,
) {
    let refs: Vec<_> = participant
        .process
        .iter()
        .map(|r| ("processRef", r))
        .collect();
    let mut scopes = Vec::new();
    collaboration_base_scopes(participant.base.as_ref(), &mut scopes);
    if let Some(multiplicity) = &participant.multiplicity {
        collaboration_base_scopes(multiplicity.base.as_ref(), &mut scopes);
    }
    let tag =
        write_collaboration_qname_owner(xml, "participant", &refs, namespace, document, &scopes);
    write_xml_id(xml, &participant.id);
    write_xml_name(xml, participant.name.as_deref());
    anonymous_base_attributes(xml, Some(&participant.id), participant.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, Some(&participant.id), participant.base.as_ref());
    for reference in &participant.interface_refs {
        write_event_qname_text(xml, "interfaceRef", reference, namespace, document);
    }
    for reference in &participant.endpoint_refs {
        write_event_qname_text(xml, "endPointRef", reference, namespace, document);
    }
    if let Some(value) = &participant.multiplicity {
        xml.push_str(&format!("<participantMultiplicity xmlns=\"{B}\""));
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        if let Some(minimum) = value.minimum {
            xml.push_str(&format!(" minimum=\"{minimum}\""));
        }
        if let Some(maximum) = value.maximum {
            xml.push_str(&format!(" maximum=\"{maximum}\""));
        }
        finish_anonymous_base_empty(
            xml,
            "participantMultiplicity",
            value.id.as_ref(),
            value.base.as_ref(),
        );
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_message_flow(xml: &mut String, flow: &Flow, namespace: &str, document: &ExportContext) {
    let refs: Vec<_> = [
        ("sourceRef", flow.source.as_ref()),
        ("targetRef", flow.target.as_ref()),
        ("messageRef", flow.message_ref.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, r)| r.map(|r| (name, r)))
    .collect();
    let tag = write_collaboration_qname_owner(
        xml,
        "messageFlow",
        &refs,
        namespace,
        document,
        &collaboration_flow_scopes(flow),
    );
    write_xml_id(xml, &flow.id);
    write_xml_name(xml, flow.name.as_deref());
    anonymous_extras_attributes(xml, &flow.id, flow.base_extras.as_ref());
    xml.push('>');
    write_doc(xml, &flow.documentation, flow.base_extras.as_ref());
    xml.push_str(&format!("</{tag}>"));
}
fn event_reference_qname(
    reference: &EventReference,
    namespace: &str,
    context: &ExportContext,
) -> QNameRef {
    match reference {
        EventReference::Local(selector) => QNameRef {
            namespace: namespace.to_owned(),
            local: xml_reference(context, selector).clone(),
        },
        EventReference::Qualified(qname) => qname.clone(),
    }
}
fn write_event_qname_text(
    xml: &mut String,
    tag: &str,
    reference: &EventReference,
    namespace: &str,
    context: &ExportContext,
) {
    let qname = event_reference_qname(reference, namespace, context);
    xml.push_str(&format!(
        "<pc_event:{tag} xmlns:pc_event=\"{B}\" xmlns=\"{}\">{}</pc_event:{tag}>",
        esc(&qname.namespace),
        qname.local
    ));
}
#[allow(clippy::too_many_arguments)]
fn write_reference_owner_open(
    xml: &mut String,
    tag: &str,
    attribute: &str,
    reference: &EventReference,
    namespace: &str,
    context: &ExportContext,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
    expressions: &[&Expression],
) -> String {
    let q = event_reference_qname(reference, namespace, context);
    let candidates: Vec<_> = expressions
        .iter()
        .filter_map(|e| match e {
            Expression::Structured(data) => Some(data),
            _ => None,
        })
        .collect();
    let find_prefix = |uri: &str| -> Option<String> {
        if let Some(bindings) = metadata_context(base)
            && let Some(binding) = bindings.iter().find(|binding| {
                !binding.prefix.is_empty()
                    && binding.prefix != "xml"
                    && binding.uri == uri
                    && candidates.iter().all(|candidate| {
                        candidate
                            .namespaces
                            .iter()
                            .any(|(p, u)| p == &binding.prefix && u == uri)
                    })
            })
        {
            return Some(binding.prefix.clone());
        }
        candidates.first().and_then(|first| {
            first
                .namespaces
                .iter()
                .find(|(prefix, value)| {
                    !prefix.is_empty()
                        && prefix != "xml"
                        && value == uri
                        && candidates.iter().all(|candidate| {
                            candidate
                                .namespaces
                                .iter()
                                .any(|(p, u)| p == prefix && u == uri)
                        })
                })
                .map(|(prefix, _)| prefix.clone())
        })
    };
    let owner_prefix = q
        .namespace
        .is_empty()
        .then(|| find_prefix(B).unwrap_or_else(|| "pc_corr_owner".into()));
    let name = owner_prefix
        .as_ref()
        .map(|prefix| format!("{prefix}:{tag}"))
        .unwrap_or_else(|| tag.to_owned());
    xml.push('<');
    xml.push_str(&name);
    if let Some(prefix) = &owner_prefix {
        xml.push_str(&format!(" xmlns:{prefix}=\"{B}\" xmlns=\"\""));
    }
    if q.namespace.is_empty() || q.namespace == B {
        xml.push_str(&format!(" {attribute}=\"{}\"", q.local));
    } else {
        let prefix = find_prefix(&q.namespace).unwrap_or_else(|| "pc_corr_ref".into());
        xml.push_str(&format!(
            " xmlns:{prefix}=\"{}\" {attribute}=\"{prefix}:{}\"",
            esc(&q.namespace),
            q.local
        ));
    }
    name
}
fn write_audit_monitor(
    xml: &mut String,
    tag: &str,
    value: &processcraft_core::bpmn_activity::AuditMonitor,
) {
    xml.push_str(&format!("<{tag} xmlns=\"{B}\""));
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    xml.push_str(&format!("</{tag}>"));
}
fn write_flow_metadata(
    xml: &mut String,
    value: Option<&processcraft_core::bpmn_activity::FlowElementMetadata>,
    namespace: &str,
    context: &ExportContext,
) {
    if let Some(value) = value {
        if let Some(audit) = &value.auditing {
            write_audit_monitor(xml, "auditing", audit);
        }
        if let Some(monitor) = &value.monitoring {
            write_audit_monitor(xml, "monitoring", monitor);
        }
        for reference in &value.category_value_refs {
            write_event_qname_text(xml, "categoryValueRef", reference, namespace, context);
        }
        for reference in &value.incoming {
            write_event_qname_text(xml, "incoming", reference, namespace, context);
        }
        for reference in &value.outgoing {
            write_event_qname_text(xml, "outgoing", reference, namespace, context);
        }
    }
}
fn write_correlation_key(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::CorrelationKey,
    namespace: &str,
    context: &ExportContext,
) {
    xml.push_str(&format!("<correlationKey xmlns=\"{B}\""));
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, value.name.as_deref());
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    for reference in &value.property_refs {
        write_event_qname_text(xml, "correlationPropertyRef", reference, namespace, context);
    }
    xml.push_str("</correlationKey>");
}
fn write_correlation_property(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::CorrelationProperty,
    namespace: &str,
    context: &ExportContext,
) {
    let expressions: Vec<_> = value
        .retrieval_expressions
        .iter()
        .filter_map(|r| r.message_path.as_ref())
        .collect();
    let tag = if let Some(q) = &value.item_type {
        write_reference_owner_open(
            xml,
            "correlationProperty",
            "type",
            &EventReference::Qualified(q.clone()),
            namespace,
            context,
            value.base.as_ref(),
            &expressions,
        )
    } else {
        xml.push_str("<correlationProperty");
        "correlationProperty".to_owned()
    };
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, value.name.as_deref());
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    for retrieval in &value.retrieval_expressions {
        let expressions: Vec<_> = retrieval.message_path.iter().collect();
        let child = if let Some(reference) = &retrieval.message_ref {
            write_reference_owner_open(
                xml,
                "correlationPropertyRetrievalExpression",
                "messageRef",
                reference,
                namespace,
                context,
                retrieval.base.as_ref(),
                &expressions,
            )
        } else {
            xml.push_str("<correlationPropertyRetrievalExpression");
            "correlationPropertyRetrievalExpression".to_owned()
        };
        if let Some(id) = &retrieval.id {
            write_xml_id(xml, id);
        }
        anonymous_base_attributes(xml, retrieval.id.as_ref(), retrieval.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, retrieval.id.as_ref(), retrieval.base.as_ref());
        if let Some(path) = &retrieval.message_path {
            write_expression(xml, "messagePath", path, false);
        }
        xml.push_str(&format!("</{child}>"));
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_correlation_subscription(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::CorrelationSubscription,
    namespace: &str,
    context: &ExportContext,
) {
    let expressions: Vec<_> = value
        .bindings
        .iter()
        .filter_map(|binding| binding.data_path.as_ref())
        .collect();
    let tag = if let Some(reference) = &value.key_ref {
        write_reference_owner_open(
            xml,
            "correlationSubscription",
            "correlationKeyRef",
            reference,
            namespace,
            context,
            value.base.as_ref(),
            &expressions,
        )
    } else {
        xml.push_str("<correlationSubscription");
        "correlationSubscription".to_owned()
    };
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    for binding in &value.bindings {
        let expressions: Vec<_> = binding.data_path.iter().collect();
        let child = if let Some(reference) = &binding.property_ref {
            write_reference_owner_open(
                xml,
                "correlationPropertyBinding",
                "correlationPropertyRef",
                reference,
                namespace,
                context,
                binding.base.as_ref(),
                &expressions,
            )
        } else {
            xml.push_str("<correlationPropertyBinding");
            "correlationPropertyBinding".to_owned()
        };
        if let Some(id) = &binding.id {
            write_xml_id(xml, id);
        }
        anonymous_base_attributes(xml, binding.id.as_ref(), binding.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, binding.id.as_ref(), binding.base.as_ref());
        if let Some(path) = &binding.data_path {
            write_expression(xml, "dataPath", path, false);
        }
        xml.push_str(&format!("</{child}>"));
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_io_binding(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::IoBinding,
    namespace: &str,
    context: &ExportContext,
) {
    let tag = if let Some(reference) = &value.operation_ref {
        write_reference_owner_open(
            xml,
            "ioBinding",
            "operationRef",
            reference,
            namespace,
            context,
            value.base.as_ref(),
            &[],
        )
    } else {
        xml.push_str("<ioBinding");
        "ioBinding".to_owned()
    };
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    if let Some(reference) = &value.input_data_ref {
        xml.push_str(&format!(
            " inputDataRef=\"{}\"",
            xml_reference(context, reference)
        ));
    }
    if let Some(reference) = &value.output_data_ref {
        xml.push_str(&format!(
            " outputDataRef=\"{}\"",
            xml_reference(context, reference)
        ));
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    xml.push_str(&format!("</{tag}>"));
}
fn write_rendering(xml: &mut String, rendering: &processcraft_core::bpmn_activity::BaseOwner) {
    xml.push_str(&format!("<rendering xmlns=\"{B}\""));
    if let Some(id) = &rendering.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, rendering.id.as_ref(), rendering.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, rendering.id.as_ref(), rendering.base.as_ref());
    xml.push_str("</rendering>");
}
fn write_resource_role(
    xml: &mut String,
    role: &processcraft_core::bpmn_activity::ResourceRole,
    namespace: &str,
    context: &ExportContext,
) {
    use processcraft_core::bpmn_activity::{ResourceRoleAssignment, ResourceRoleKind};
    let tag = match role.kind {
        ResourceRoleKind::ResourceRole => "resourceRole",
        ResourceRoleKind::Performer => "performer",
        ResourceRoleKind::HumanPerformer => "humanPerformer",
        ResourceRoleKind::PotentialOwner => "potentialOwner",
    };
    xml.push_str(&format!("<{tag} xmlns=\"{B}\""));
    if let Some(id) = &role.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, role.name.as_deref());
    anonymous_base_attributes(xml, role.id.as_ref(), role.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, role.id.as_ref(), role.base.as_ref());
    match &role.assignment {
        ResourceRoleAssignment::None => {}
        ResourceRoleAssignment::Resource {
            reference,
            bindings,
        } => {
            write_event_qname_text(xml, "resourceRef", reference, namespace, context);
            for binding in bindings {
                let expressions: Vec<_> = binding.expression.iter().collect();
                let child = if let Some(reference) = &binding.parameter_ref {
                    write_reference_owner_open(
                        xml,
                        "resourceParameterBinding",
                        "parameterRef",
                        reference,
                        namespace,
                        context,
                        binding.base.as_ref(),
                        &expressions,
                    )
                } else {
                    xml.push_str("<resourceParameterBinding");
                    "resourceParameterBinding".to_owned()
                };
                if let Some(id) = &binding.id {
                    write_xml_id(xml, id);
                }
                anonymous_base_attributes(xml, binding.id.as_ref(), binding.base.as_ref());
                xml.push('>');
                anonymous_base_children(xml, binding.id.as_ref(), binding.base.as_ref());
                if let Some(expression) = &binding.expression {
                    write_expression(xml, "expression", expression, false);
                }
                xml.push_str(&format!("</{child}>"));
            }
        }
        ResourceRoleAssignment::Expression { assignment } => {
            xml.push_str("<resourceAssignmentExpression");
            if let Some(id) = &assignment.id {
                write_xml_id(xml, id);
            }
            anonymous_base_attributes(xml, assignment.id.as_ref(), assignment.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, assignment.id.as_ref(), assignment.base.as_ref());
            if let Some(expression) = &assignment.expression {
                write_expression(xml, "expression", expression, false);
            }
            xml.push_str("</resourceAssignmentExpression>");
        }
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_global_task(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::GlobalTask,
    namespace: &str,
    context: &ExportContext,
) {
    use processcraft_core::bpmn_activity::GlobalTaskKind;
    let tag = match &value.kind {
        GlobalTaskKind::CallableElement => "rootElement",
        GlobalTaskKind::Task => "globalTask",
        GlobalTaskKind::Manual => "globalManualTask",
        GlobalTaskKind::BusinessRule { .. } => "globalBusinessRuleTask",
        GlobalTaskKind::Script { .. } => "globalScriptTask",
        GlobalTaskKind::User { .. } => "globalUserTask",
    };
    xml.push_str(&format!("<{tag}"));
    if matches!(value.kind, GlobalTaskKind::CallableElement) {
        xml.push_str(&format!(" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"{B}\" xsi:type=\"bpmn:tCallableElement\""));
    }
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, value.name.as_deref());
    match &value.kind {
        GlobalTaskKind::BusinessRule { implementation }
        | GlobalTaskKind::User { implementation, .. } => {
            if let Some(value) = implementation {
                xml.push_str(&format!(" implementation=\"{}\"", esc(value)));
            }
        }
        GlobalTaskKind::Script {
            script_language: Some(value),
            ..
        } => {
            xml.push_str(&format!(" scriptLanguage=\"{}\"", esc(value)));
        }
        _ => {}
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    for reference in &value.callable.supported_interface_refs {
        write_event_qname_text(xml, "supportedInterfaceRef", reference, namespace, context);
    }
    if let Some(io) = &value.io_specification {
        write_io_specification(xml, io, false, context);
    }
    for binding in &value.callable.io_bindings {
        write_io_binding(xml, binding, namespace, context);
    }
    for role in &value.resource_roles {
        write_resource_role(xml, role, namespace, context);
    }
    match &value.kind {
        GlobalTaskKind::Script {
            script: Some(script),
            ..
        } => write_mixed_element(xml, "script", script),
        GlobalTaskKind::User { renderings, .. } => {
            for rendering in renderings {
                write_rendering(xml, rendering);
            }
        }
        _ => {}
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_event_definition(
    xml: &mut String,
    definition: &EventDefinition,
    restore_default: bool,
    namespace: &str,
    context: &ExportContext,
) {
    let (tag, id, attribute_reference) = match definition {
        EventDefinition::Message {
            id, message_ref, ..
        } => (
            "messageEventDefinition",
            id,
            message_ref.as_ref().map(|r| ("messageRef", r)),
        ),
        EventDefinition::Signal { id, signal_ref, .. } => (
            "signalEventDefinition",
            id,
            signal_ref.as_ref().map(|r| ("signalRef", r)),
        ),
        EventDefinition::Error { id, error_ref, .. } => (
            "errorEventDefinition",
            id,
            error_ref.as_ref().map(|r| ("errorRef", r)),
        ),
        EventDefinition::Escalation {
            id, escalation_ref, ..
        } => (
            "escalationEventDefinition",
            id,
            escalation_ref.as_ref().map(|r| ("escalationRef", r)),
        ),
        EventDefinition::Compensate {
            id, activity_ref, ..
        } => (
            "compensateEventDefinition",
            id,
            activity_ref.as_ref().map(|r| ("activityRef", r)),
        ),
        EventDefinition::Conditional { id, .. } => ("conditionalEventDefinition", id, None),
        EventDefinition::Timer { id, .. } => ("timerEventDefinition", id, None),
        EventDefinition::Link { id, .. } => ("linkEventDefinition", id, None),
        EventDefinition::Cancel { id, .. } => ("cancelEventDefinition", id, None),
        EventDefinition::Terminate { id, .. } => ("terminateEventDefinition", id, None),
    };
    let name = if let Some((_, reference)) = attribute_reference {
        let qname = event_reference_qname(reference, namespace, context);
        write_local_qname_open(xml, tag, &qname.namespace)
    } else {
        xml.push_str(&format!("<{tag}"));
        if restore_default {
            xml.push_str(&format!(" xmlns=\"{B}\""));
        }
        tag.to_owned()
    };
    if let Some(id) = id {
        write_xml_id(xml, id);
    }
    if let Some((attribute, reference)) = attribute_reference {
        let qname = event_reference_qname(reference, namespace, context);
        xml.push_str(&format!(
            " {attribute}=\"{}\"",
            local_qname_value(&qname.local, &qname.namespace)
        ));
    }
    match definition {
        EventDefinition::Compensate {
            wait_for_completion: Some(value),
            ..
        } => xml.push_str(&format!(" waitForCompletion=\"{value}\"")),
        EventDefinition::Link { name, .. } => xml.push_str(&format!(" name=\"{}\"", esc(name))),
        _ => {}
    }
    anonymous_base_attributes(xml, id.as_ref(), definition.base());
    xml.push('>');
    anonymous_base_children(xml, id.as_ref(), definition.base());
    match definition {
        EventDefinition::Message {
            operation_ref: Some(reference),
            ..
        } => write_event_qname_text(xml, "operationRef", reference, namespace, context),
        EventDefinition::Conditional { expression, .. } => {
            write_expression(xml, "condition", expression, false)
        }
        EventDefinition::Timer {
            kind: Some(kind),
            expression: Some(expression),
            ..
        } => {
            let child = match kind {
                TimerKind::Date => "timeDate",
                TimerKind::Duration => "timeDuration",
                TimerKind::Cycle => "timeCycle",
            };
            write_expression(xml, child, expression, false);
        }
        EventDefinition::Link {
            sources, target, ..
        } => {
            for source in sources {
                write_event_qname_text(xml, "source", source, namespace, context);
            }
            if let Some(target) = target {
                write_event_qname_text(xml, "target", target, namespace, context);
            }
        }
        _ => {}
    }
    xml.push_str(&format!("</{name}>"));
}
fn write_xml_id(xml: &mut String, identity: &ElementIdentity) {
    if let Some(id) = identity.xml_id() {
        xml.push_str(&format!(" id=\"{id}\""));
    }
}
fn anonymous_base_attributes(
    xml: &mut String,
    id: Option<&ElementIdentity>,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
) {
    if id.and_then(ElementIdentity::xml_id).is_some() {
        return;
    }
    let Some(base) = base else {
        return;
    };
    let start = xml.rfind('<').unwrap_or(xml.len());
    let opening = xml[start..].to_owned();
    if !base.namespaces.is_empty() {
        if let Ok(encoded) = serde_json::to_vec(&base.namespaces) {
            let hex: String = encoded.iter().map(|byte| format!("{byte:02x}")).collect();
            xml.push_str(&format!(" {ANONYMOUS_CONTEXT_MARKER}=\"{hex}\""));
        } else {
            xml.push_str(" !");
        }
    }
    for binding in &base.namespaces {
        if binding.prefix == "xml" || binding.prefix.is_empty() {
            continue;
        }
        let key = if binding.prefix.is_empty() {
            "xmlns".to_owned()
        } else {
            format!("xmlns:{}", binding.prefix)
        };
        let expected = format!(" {key}=\"{}\"", esc(&binding.uri));
        if !opening.contains(&format!(" {key}=")) {
            xml.push_str(&expected);
        }
    }
    for attr in &base.foreign_attributes {
        if let Some(prefix) = &attr.prefix {
            xml.push_str(&format!(
                " {prefix}:{}=\"{}\"",
                attr.local,
                esc(&attr.value)
            ));
        } else {
            xml.push_str(" !");
        }
    }
}
fn anonymous_base_children(
    xml: &mut String,
    id: Option<&ElementIdentity>,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
) {
    if id.and_then(ElementIdentity::xml_id).is_some() {
        return;
    }
    if let Some(base) = base {
        for doc in &base.documentation {
            write_structured_documentation(xml, doc);
        }
        if let Some(extension) = &base.extension_elements {
            write_extension_elements(xml, extension);
        }
    }
}
fn finish_anonymous_base_empty(
    xml: &mut String,
    tag: &str,
    id: Option<&ElementIdentity>,
    base: Option<&processcraft_core::bpmn_metadata::BaseMetadata>,
) {
    anonymous_base_attributes(xml, id, base);
    if id.and_then(ElementIdentity::xml_id).is_none() && base.is_some() {
        xml.push('>');
        anonymous_base_children(xml, id, base);
        xml.push_str(&format!("</{tag}>"));
    } else {
        xml.push_str("/>");
    }
}
fn anonymous_extras_attributes(
    xml: &mut String,
    id: &ElementIdentity,
    extras: Option<&processcraft_core::bpmn_metadata::BaseExtras>,
) {
    if let Some(extras) = extras {
        let base = processcraft_core::bpmn_metadata::BaseMetadata {
            documentation: Vec::new(),
            namespaces: extras.namespaces.clone(),
            foreign_attributes: extras.foreign_attributes.clone(),
            extension_elements: None,
        };
        anonymous_base_attributes(xml, Some(id), Some(&base));
    }
}
fn write_xml_name(xml: &mut String, name: Option<&str>) {
    if let Some(name) = name {
        xml.push_str(&format!(" name=\"{}\"", esc(name)));
    }
}
fn local_flow_xml_reference<'a>(
    document: &'a ExportContext<'a>,
    reference: Option<&EventReference>,
) -> String {
    match reference {
        Some(EventReference::Local(selector)) => xml_reference(document, selector).to_string(),
        _ => String::new(),
    }
}
fn xml_reference<'a>(document: &'a ExportContext<'a>, selector: &ElementSelector) -> &'a XmlId {
    document
        .xml_ids
        .get(selector)
        .copied()
        .expect("reference preflight")
}
fn write_lanes(
    xml: &mut String,
    lanes: &[Lane],
    parent: Option<&ElementSelector>,
    document: &ExportContext,
) {
    for lane in lanes.iter().filter(|lane| lane.parent.as_ref() == parent) {
        xml.push_str("<lane");
        write_xml_id(xml, &lane.id);
        write_xml_name(xml, lane.name.as_deref());
        anonymous_base_attributes(xml, Some(&lane.id), lane.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&lane.id), lane.base.as_ref());
        for member in &lane.members {
            xml.push_str(&format!(
                "<flowNodeRef>{}</flowNodeRef>",
                xml_reference(document, member)
            ));
        }
        let lane_selector = lane.id.selector();
        if lane.child_lane_set_id.is_some()
            || lane.child_lane_set_base.is_some()
            || lanes
                .iter()
                .any(|child| child.parent.as_ref() == Some(&lane_selector))
        {
            xml.push_str("<childLaneSet");
            if let Some(id) = &lane.child_lane_set_id {
                write_xml_id(xml, id);
            }
            anonymous_base_attributes(
                xml,
                lane.child_lane_set_id.as_ref(),
                lane.child_lane_set_base.as_ref(),
            );
            xml.push('>');
            anonymous_base_children(
                xml,
                lane.child_lane_set_id.as_ref(),
                lane.child_lane_set_base.as_ref(),
            );
            write_lanes(xml, lanes, Some(&lane_selector), document);
            xml.push_str("</childLaneSet>");
        }
        xml.push_str("</lane>");
    }
}
fn write_item_attributes(
    xml: &mut String,
    metadata: Option<&processcraft_core::bpmn_data::ItemAwareMetadata>,
    context: Option<&[processcraft_core::bpmn_xml::NamespaceBinding]>,
) {
    if let Some(q) = metadata.and_then(|m| m.item_subject_ref.as_ref()) {
        if let Some(context) = context {
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
            xml.push_str(&format!(" itemSubjectRef=\"{value}\""));
        } else {
            write_catalog_qname_attribute(xml, "itemSubjectRef", q, "pc_item");
        }
    }
}
fn write_item_state(
    xml: &mut String,
    metadata: Option<&processcraft_core::bpmn_data::ItemAwareMetadata>,
    restore_default: bool,
) {
    if let Some(state) = metadata.and_then(|m| m.data_state.as_ref()) {
        xml.push_str("<dataState");
        if restore_default {
            xml.push_str(&format!(" xmlns=\"{B}\""));
        }
        if let Some(id) = &state.id {
            write_xml_id(xml, id);
        }
        if let Some(name) = &state.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        anonymous_base_attributes(xml, state.id.as_ref(), state.base.as_ref());
        if state
            .id
            .as_ref()
            .and_then(ElementIdentity::xml_id)
            .is_none()
            && state.base.is_some()
        {
            xml.push('>');
            anonymous_base_children(xml, state.id.as_ref(), state.base.as_ref());
            xml.push_str("</dataState>");
        } else {
            xml.push_str("/>");
        }
    }
}
fn write_io_specification(
    xml: &mut String,
    io: &processcraft_core::bpmn_data::IoSpecification,
    unqualified: bool,
    document: &ExportContext,
) {
    xml.push_str("<ioSpecification");
    if unqualified {
        xml.push_str(&format!(" xmlns=\"{B}\""));
    }
    if let Some(id) = &io.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, io.id.as_ref(), io.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, io.id.as_ref(), io.base.as_ref());
    for (tag, ports) in [("dataInput", &io.inputs), ("dataOutput", &io.outputs)] {
        for port in ports {
            let unqualified_item = port
                .item_aware
                .as_ref()
                .and_then(|m| m.item_subject_ref.as_ref())
                .is_some_and(|q| q.namespace.is_empty());
            let element = if unqualified_item {
                format!("pc_data:{tag}")
            } else {
                tag.to_string()
            };
            xml.push_str(&format!("<{element}"));
            if unqualified_item {
                xml.push_str(&format!(" xmlns:pc_data=\"{B}\" xmlns=\"\""));
            }
            if let Some(id) = &port.id {
                write_xml_id(xml, id);
            }
            if let Some(name) = &port.name {
                xml.push_str(&format!(" name=\"{}\"", esc(name)));
            }
            if let Some(value) = port.is_collection {
                xml.push_str(&format!(" isCollection=\"{value}\""));
            }
            write_item_attributes(
                xml,
                port.item_aware.as_ref(),
                metadata_context(port.base.as_ref()),
            );
            anonymous_base_attributes(xml, port.id.as_ref(), port.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, port.id.as_ref(), port.base.as_ref());
            write_item_state(xml, port.item_aware.as_ref(), unqualified_item);
            xml.push_str(&format!("</{element}>"));
        }
    }
    for (input, sets) in [(true, &io.input_sets), (false, &io.output_sets)] {
        let tag = if input { "inputSet" } else { "outputSet" };
        for set in sets {
            xml.push_str(&format!("<{tag}"));
            if let Some(id) = &set.id {
                write_xml_id(xml, id);
            }
            if let Some(name) = &set.name {
                xml.push_str(&format!(" name=\"{}\"", esc(name)));
            }
            anonymous_base_attributes(xml, set.id.as_ref(), set.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, set.id.as_ref(), set.base.as_ref());
            for (name, values) in [
                (
                    if input {
                        "dataInputRefs"
                    } else {
                        "dataOutputRefs"
                    },
                    &set.data_refs,
                ),
                (
                    if input {
                        "optionalInputRefs"
                    } else {
                        "optionalOutputRefs"
                    },
                    &set.optional_refs,
                ),
                (
                    if input {
                        "whileExecutingInputRefs"
                    } else {
                        "whileExecutingOutputRefs"
                    },
                    &set.while_executing_refs,
                ),
                (
                    if input {
                        "outputSetRefs"
                    } else {
                        "inputSetRefs"
                    },
                    &set.counterpart_refs,
                ),
            ] {
                for value in values {
                    xml.push_str(&format!(
                        "<{name}>{}</{name}>",
                        xml_reference(document, value)
                    ));
                }
            }
            xml.push_str(&format!("</{tag}>"));
        }
    }
    xml.push_str("</ioSpecification>");
}
fn write_event_port(
    xml: &mut String,
    port: &processcraft_core::bpmn_data::DataPort,
    tag: &str,
    restore_default: bool,
) {
    let empty_qname = port
        .item_aware
        .as_ref()
        .and_then(|m| m.item_subject_ref.as_ref())
        .is_some_and(|q| q.namespace.is_empty());
    let element = if empty_qname {
        format!("pc_data:{tag}")
    } else {
        tag.to_owned()
    };
    xml.push_str(&format!("<{element}"));
    if empty_qname {
        xml.push_str(&format!(" xmlns:pc_data=\"{B}\" xmlns=\"\""));
    } else if restore_default {
        xml.push_str(&format!(" xmlns=\"{B}\""));
    }
    if let Some(id) = &port.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, port.name.as_deref());
    if let Some(collection) = port.is_collection {
        xml.push_str(&format!(" isCollection=\"{collection}\""));
    }
    write_item_attributes(
        xml,
        port.item_aware.as_ref(),
        metadata_context(port.base.as_ref()),
    );
    anonymous_base_attributes(xml, port.id.as_ref(), port.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, port.id.as_ref(), port.base.as_ref());
    write_item_state(xml, port.item_aware.as_ref(), empty_qname);
    xml.push_str(&format!("</{element}>"));
}
fn write_event_set(
    xml: &mut String,
    set: &processcraft_core::bpmn_data::IoSet,
    input: bool,
    restore_default: bool,
    context: &ExportContext,
) {
    let tag = if input { "inputSet" } else { "outputSet" };
    xml.push_str(&format!("<{tag}"));
    if restore_default {
        xml.push_str(&format!(" xmlns=\"{B}\""));
    }
    if let Some(id) = &set.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, set.name.as_deref());
    anonymous_base_attributes(xml, set.id.as_ref(), set.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, set.id.as_ref(), set.base.as_ref());
    for (child, refs) in [
        (
            if input {
                "dataInputRefs"
            } else {
                "dataOutputRefs"
            },
            &set.data_refs,
        ),
        (
            if input {
                "optionalInputRefs"
            } else {
                "optionalOutputRefs"
            },
            &set.optional_refs,
        ),
        (
            if input {
                "whileExecutingInputRefs"
            } else {
                "whileExecutingOutputRefs"
            },
            &set.while_executing_refs,
        ),
        (
            if input {
                "outputSetRefs"
            } else {
                "inputSetRefs"
            },
            &set.counterpart_refs,
        ),
    ] {
        for reference in refs {
            xml.push_str(&format!(
                "<{child}>{}</{child}>",
                xml_reference(context, reference)
            ));
        }
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_event_property(xml: &mut String, property: &EventProperty, restore_default: bool) {
    let empty_qname = property
        .item_aware
        .as_ref()
        .and_then(|m| m.item_subject_ref.as_ref())
        .is_some_and(|q| q.namespace.is_empty());
    let tag = if empty_qname {
        "pc_data:property"
    } else {
        "property"
    };
    xml.push_str(&format!("<{tag}"));
    if empty_qname {
        xml.push_str(&format!(" xmlns:pc_data=\"{B}\" xmlns=\"\""));
    } else if restore_default {
        xml.push_str(&format!(" xmlns=\"{B}\""));
    }
    if let Some(id) = &property.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, property.name.as_deref());
    write_item_attributes(
        xml,
        property.item_aware.as_ref(),
        metadata_context(property.base.as_ref()),
    );
    anonymous_base_attributes(xml, property.id.as_ref(), property.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, property.id.as_ref(), property.base.as_ref());
    write_item_state(xml, property.item_aware.as_ref(), empty_qname);
    xml.push_str(&format!("</{tag}>"));
}
fn write_data_association(
    xml: &mut String,
    a: &processcraft_core::bpmn_data::DataAssociation,
    restore_default: bool,
    document: &ExportContext,
) {
    let tag = match a.kind {
        processcraft_core::bpmn_data::DataAssociationKind::Input => "dataInputAssociation",
        processcraft_core::bpmn_data::DataAssociationKind::Output => "dataOutputAssociation",
    };
    xml.push_str(&format!("<{tag}"));
    if restore_default {
        xml.push_str(&format!(" xmlns=\"{B}\""));
    }
    if let Some(id) = &a.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, a.id.as_ref(), a.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, a.id.as_ref(), a.base.as_ref());
    for source in &a.sources {
        xml.push_str(&format!(
            "<sourceRef>{}</sourceRef>",
            xml_reference(document, source)
        ));
    }
    xml.push_str(&format!(
        "<targetRef>{}</targetRef>",
        xml_reference(document, &a.target)
    ));
    if let Some(expression) = &a.transformation {
        write_expression(xml, "transformation", expression, false);
    }
    for assignment in &a.assignments {
        xml.push_str("<assignment");
        if let Some(id) = &assignment.id {
            write_xml_id(xml, id);
        }
        anonymous_base_attributes(xml, assignment.id.as_ref(), assignment.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, assignment.id.as_ref(), assignment.base.as_ref());
        write_expression(xml, "from", &assignment.from, false);
        write_expression(xml, "to", &assignment.to, false);
        xml.push_str("</assignment>");
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_implicit_throw(
    xml: &mut String,
    event: &processcraft_core::bpmn_activity::ImplicitThrowEvent,
    namespace: &str,
    context: &ExportContext,
) {
    xml.push_str("<event");
    if let Some(id) = &event.id {
        write_xml_id(xml, id);
    }
    write_xml_name(xml, event.name.as_deref());
    anonymous_base_attributes(xml, event.id.as_ref(), event.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, event.id.as_ref(), event.base.as_ref());
    write_flow_metadata(xml, event.flow.as_ref(), namespace, context);
    for value in &event.incoming {
        write_event_qname_text(xml, "incoming", value, namespace, context);
    }
    for value in &event.outgoing {
        write_event_qname_text(xml, "outgoing", value, namespace, context);
    }
    for value in &event.properties {
        write_event_property(xml, value, false);
    }
    for value in &event.inputs {
        write_event_port(xml, value, "dataInput", false);
    }
    for value in &event.input_associations {
        xml.push_str("<dataInputAssociation");
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
        for source in &value.sources {
            xml.push_str(&format!(
                "<sourceRef>{}</sourceRef>",
                xml_reference(context, source)
            ));
        }
        if let Some(target) = &value.target {
            xml.push_str(&format!(
                "<targetRef>{}</targetRef>",
                xml_reference(context, target)
            ));
        }
        if let Some(expression) = &value.transformation {
            write_expression(xml, "transformation", expression, false);
        }
        for assignment in &value.assignments {
            xml.push_str("<assignment");
            if let Some(id) = &assignment.id {
                write_xml_id(xml, id);
            }
            anonymous_base_attributes(xml, assignment.id.as_ref(), assignment.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, assignment.id.as_ref(), assignment.base.as_ref());
            write_expression(xml, "from", &assignment.from, false);
            write_expression(xml, "to", &assignment.to, false);
            xml.push_str("</assignment>");
        }
        xml.push_str("</dataInputAssociation>");
    }
    if let Some(value) = &event.input_set {
        write_event_set(xml, value, true, false, context);
    }
    for value in &event.definitions {
        write_event_definition(xml, value, false, namespace, context);
    }
    for value in &event.definition_refs {
        write_event_qname_text(xml, "eventDefinitionRef", value, namespace, context);
    }
    xml.push_str("</event>");
}
// Recursive scope serialization shares the same document namespace and ID allocation.
#[allow(clippy::too_many_arguments)]
fn write_scope(
    xml: &mut String,
    nodes: &[Node],
    flows: &[Flow],
    objects: &[processcraft_core::bpmn_data::DataObject],
    associations: &[processcraft_core::bpmn_data::DataAssociation],
    groups: &[processcraft_core::bpmn_model::Group],
    document: &ExportContext,
    namespace: &str,
) {
    for o in objects {
        let unqualified = o
            .item_aware
            .as_ref()
            .and_then(|m| m.item_subject_ref.as_ref())
            .is_some_and(|q| q.namespace.is_empty());
        let tag = if unqualified {
            "pc_data:dataObject"
        } else {
            "dataObject"
        };
        xml.push_str(&format!("<{tag}"));
        if unqualified {
            xml.push_str(&format!(" xmlns:pc_data=\"{B}\" xmlns=\"\""));
        }
        if let Some(id) = &o.id {
            write_xml_id(xml, id);
        }
        if let Some(name) = &o.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        write_item_attributes(
            xml,
            o.item_aware.as_ref(),
            metadata_context(o.base.as_ref()),
        );
        if let Some(value) = o.is_collection {
            xml.push_str(&format!(" isCollection=\"{}\"", value));
        }
        anonymous_base_attributes(xml, o.id.as_ref(), o.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, o.id.as_ref(), o.base.as_ref());
        write_flow_metadata(xml, o.flow_metadata.as_ref(), namespace, document);
        write_item_state(xml, o.item_aware.as_ref(), unqualified);
        xml.push_str(&format!("</{tag}>"));
    }
    for n in nodes
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::TextAnnotation))
    {
        let tag = match n.kind {
            NodeKind::TextAnnotation => "textAnnotation",
            NodeKind::DataObjectReference => "dataObjectReference",
            NodeKind::DataStoreReference => "dataStoreReference",
            NodeKind::Task => "task",
            NodeKind::UserTask => "userTask",
            NodeKind::ManualTask => "manualTask",
            NodeKind::ServiceTask => "serviceTask",
            NodeKind::SendTask => "sendTask",
            NodeKind::ReceiveTask => "receiveTask",
            NodeKind::ScriptTask => "scriptTask",
            NodeKind::BusinessRuleTask => "businessRuleTask",
            NodeKind::InclusiveGateway => "inclusiveGateway",
            NodeKind::ComplexGateway => "complexGateway",
            NodeKind::EventBasedGateway => "eventBasedGateway",
            NodeKind::IntermediateThrowEvent => "intermediateThrowEvent",
            NodeKind::IntermediateCatchEvent => "intermediateCatchEvent",
            NodeKind::BoundaryEvent => "boundaryEvent",
            NodeKind::SubProcess => "subProcess",
            NodeKind::AdHocSubProcess => "adHocSubProcess",
            NodeKind::Transaction => "transaction",
            NodeKind::CallActivity => "callActivity",
            NodeKind::StartEvent => "startEvent",
            NodeKind::EndEvent => "endEvent",
            NodeKind::ExclusiveGateway => "exclusiveGateway",
            NodeKind::ParallelGateway => "parallelGateway",
        };
        let refs = n
            .activity
            .as_ref()
            .map(|a| [a.message_ref.as_ref(), a.operation_ref.as_ref()])
            .unwrap_or([None, None]);
        let store_ref = match &n.details {
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Qualified(q)),
            }) => Some(q),
            _ => None,
        };
        let local_owner_qname = namespace.is_empty()
            && matches!(
                &n.details,
                Some(NodeDetails::CallActivity {
                    called_element: Some(_)
                }) | Some(NodeDetails::Event {
                    attached_to: Some(_),
                    ..
                }) | Some(NodeDetails::DataStoreReference {
                    data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Legacy(_))
                })
            );
        let unqualified = local_owner_qname
            || refs.iter().flatten().any(|q| q.namespace.is_empty())
            || n.item_aware
                .as_ref()
                .and_then(|m| m.item_subject_ref.as_ref())
                .is_some_and(|q| q.namespace.is_empty())
            || store_ref.is_some_and(|q| q.namespace.is_empty())
            || matches!(&n.details, Some(NodeDetails::CallActivity { called_element: Some(q) }) if q.namespace.is_empty());
        let mut expressions = Vec::new();
        if let Some(lp) = &n.loop_characteristics {
            match lp {
                LoopCharacteristics::Standard { condition, .. } => {
                    expressions.extend(condition.iter())
                }
                LoopCharacteristics::MultiInstance {
                    cardinality,
                    completion_condition,
                    ..
                } => {
                    expressions.extend(cardinality.iter());
                    expressions.extend(completion_condition.iter());
                }
            }
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details {
            for definition in definitions {
                match definition {
                    EventDefinition::Timer {
                        expression: Some(expression),
                        ..
                    }
                    | EventDefinition::Conditional { expression, .. } => {
                        expressions.push(expression)
                    }
                    _ => {}
                }
            }
        }
        for association in associations.iter().filter(|a| a.owner == n.id) {
            expressions.extend(association.transformation.iter());
            for assignment in &association.assignments {
                expressions.push(&assignment.from);
                expressions.push(&assignment.to);
            }
        }
        let contexts: Vec<_> = expressions
            .iter()
            .filter_map(|e| match e {
                Expression::Structured(d) => Some(d),
                _ => None,
            })
            .collect();
        let mut prefixes: Vec<String> = contexts
            .first()
            .map(|first| {
                first
                    .namespaces
                    .iter()
                    .filter(|(prefix, _)| {
                        !prefix.is_empty()
                            && prefix != "xml"
                            && contexts
                                .iter()
                                .all(|d| d.namespaces.iter().any(|(p, _)| p == prefix))
                    })
                    .map(|(prefix, _)| prefix.clone())
                    .collect()
            })
            .unwrap_or_default();
        let owner_prefix = if unqualified {
            let candidate = contexts.first().and_then(|first| {
                first
                    .namespaces
                    .iter()
                    .find(|(prefix, uri)| {
                        !prefix.is_empty()
                            && prefix != "xml"
                            && uri == B
                            && contexts
                                .iter()
                                .all(|d| d.namespaces.iter().any(|(p, _)| p == prefix))
                    })
                    .map(|(prefix, _)| prefix.clone())
            });
            candidate.unwrap_or_else(|| "pc_owner".into())
        } else {
            String::new()
        };
        prefixes.retain(|prefix| prefix != &owner_prefix);
        let tag_name = if unqualified {
            format!("{owner_prefix}:{tag}")
        } else {
            tag.to_owned()
        };
        xml.push_str(&format!("<{tag_name}"));
        write_xml_id(xml, &n.id);
        if unqualified {
            xml.push_str(&format!(" xmlns:{owner_prefix}=\"{B}\" xmlns=\"\""));
        }
        for (index, (name, reference)) in [("messageRef", refs[0]), ("operationRef", refs[1])]
            .into_iter()
            .enumerate()
        {
            if let Some(reference) = reference {
                if let Some(context) = n
                    .base_extras
                    .as_ref()
                    .filter(|e| !e.namespaces.is_empty())
                    .map(|e| e.namespaces.as_slice())
                {
                    write_context_qname_attribute(xml, name, reference, context);
                } else if reference.namespace.is_empty() {
                    xml.push_str(&format!(" {name}=\"{}\"", reference.local));
                } else {
                    let prefix = if prefixes.is_empty() {
                        format!("pc_ref{index}")
                    } else {
                        prefixes.remove(0)
                    };
                    xml.push_str(&format!(
                        " xmlns:{prefix}=\"{}\" {name}=\"{prefix}:{}\"",
                        esc(&reference.namespace),
                        reference.local
                    ));
                }
            }
        }
        if !matches!(n.kind, NodeKind::TextAnnotation) {
            write_xml_name(xml, n.name.as_deref());
        }
        if let Some(NodeDetails::Event {
            attached_to: Some(id),
            cancel_activity,
            ..
        }) = &n.details
        {
            let prefix = match &n.details {
                Some(NodeDetails::Event { definitions, .. }) => definitions
                    .iter()
                    .find_map(|definition| {
                        let expression = match definition {
                            EventDefinition::Timer {
                                expression: Some(expression),
                                ..
                            }
                            | EventDefinition::Conditional { expression, .. } => expression,
                            _ => return None,
                        };
                        if let Expression::Structured(data) = expression {
                            data.namespaces
                                .iter()
                                .find(|(prefix, _)| !prefix.is_empty() && prefix != "xml")
                                .map(|(prefix, _)| prefix.as_str())
                        } else {
                            None
                        }
                    })
                    .unwrap_or("tns"),
                _ => "tns",
            };
            if namespace.is_empty() {
                xml.push_str(&format!(
                    " attachedToRef=\"{}\"",
                    xml_reference(document, id)
                ));
            } else {
                xml.push_str(&format!(
                    " xmlns:{prefix}=\"{}\" attachedToRef=\"{prefix}:{}\"",
                    esc(namespace),
                    xml_reference(document, id)
                ));
            }
            if let Some(cancel) = cancel_activity {
                xml.push_str(&format!(" cancelActivity=\"{}\"", cancel));
            }
        }
        write_item_attributes(
            xml,
            n.item_aware.as_ref(),
            n.base_extras.as_ref().and_then(|extra| {
                (!extra.namespaces.is_empty()).then_some(extra.namespaces.as_slice())
            }),
        );
        if let Some(NodeDetails::Event {
            parallel_multiple,
            is_interrupting,
            ..
        }) = &n.details
        {
            if let Some(value) = parallel_multiple {
                xml.push_str(&format!(" parallelMultiple=\"{value}\""));
            }
            if let Some(value) = is_interrupting {
                xml.push_str(&format!(" isInterrupting=\"{value}\""));
            }
        }
        if let Some(a) = &n.activity {
            if let Some(v) = a.is_for_compensation {
                xml.push_str(&format!(" isForCompensation=\"{}\"", v));
            }
            if let Some(v) = &a.start_quantity {
                xml.push_str(&format!(" startQuantity=\"{}\"", esc(v)));
            }
            if let Some(v) = &a.completion_quantity {
                xml.push_str(&format!(" completionQuantity=\"{}\"", esc(v)));
            }
            if let Some(v) = &a.default_flow {
                xml.push_str(&format!(" default=\"{}\"", xml_reference(document, v)));
            }
            if let Some(v) = a.ad_hoc_ordering {
                xml.push_str(&format!(" ordering=\"{:?}\"", v));
            }
            if let Some(v) = a.cancel_remaining_instances {
                xml.push_str(&format!(" cancelRemainingInstances=\"{}\"", v));
            }
            if let Some(v) = &a.transaction_method {
                xml.push_str(&format!(" method=\"{}\"", esc(v)));
            }
            if let Some(v) = a.triggered_by_event {
                xml.push_str(&format!(" triggeredByEvent=\"{}\"", v));
            }
            if let Some(v) = &a.implementation {
                xml.push_str(&format!(" implementation=\"{}\"", esc(v)));
            }
            if let Some(v) = a.instantiate {
                xml.push_str(&format!(" instantiate=\"{}\"", v));
            }
            if let Some(v) = &a.script_format {
                xml.push_str(&format!(" scriptFormat=\"{}\"", esc(v)));
            }
        }
        if let Some(NodeDetails::Gateway {
            default_flow,
            direction,
            instantiate,
            event_gateway_type,
            ..
        }) = &n.details
        {
            if let Some(id) = default_flow {
                xml.push_str(&format!(" default=\"{}\"", xml_reference(document, id)));
            }
            if let Some(value) = direction {
                xml.push_str(&format!(" gatewayDirection=\"{:?}\"", value));
            }
            if let Some(value) = instantiate {
                xml.push_str(&format!(" instantiate=\"{}\"", value));
            }
            if let Some(value) = event_gateway_type {
                xml.push_str(&format!(" eventGatewayType=\"{:?}\"", value));
            }
        }
        if let Some(NodeDetails::TextAnnotation {
            text_format: Some(format),
            ..
        }) = &n.details
        {
            xml.push_str(&format!(" textFormat=\"{}\"", esc(format)));
        }
        match &n.details {
            Some(NodeDetails::DataObjectReference {
                data_object_ref: Some(id),
            }) => xml.push_str(&format!(
                " dataObjectRef=\"{}\"",
                xml_reference(document, id)
            )),
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(reference),
            }) => match reference {
                processcraft_core::bpmn_data::DataStoreRef::Legacy(id) => {
                    if let Some(context) = n
                        .base_extras
                        .as_ref()
                        .filter(|e| !e.namespaces.is_empty())
                        .map(|e| e.namespaces.as_slice())
                    {
                        write_context_qname_attribute(
                            xml,
                            "dataStoreRef",
                            &QNameRef {
                                namespace: namespace.to_owned(),
                                local: xml_reference(document, id).clone(),
                            },
                            context,
                        );
                    } else if namespace.is_empty() {
                        xml.push_str(&format!(
                            " dataStoreRef=\"{}\"",
                            xml_reference(document, id)
                        ));
                    } else {
                        xml.push_str(&format!(
                            " xmlns:tns=\"{}\" dataStoreRef=\"tns:{}\"",
                            esc(namespace),
                            xml_reference(document, id)
                        ));
                    }
                }
                processcraft_core::bpmn_data::DataStoreRef::Qualified(q) => {
                    if let Some(context) = n
                        .base_extras
                        .as_ref()
                        .filter(|e| !e.namespaces.is_empty())
                        .map(|e| e.namespaces.as_slice())
                    {
                        write_context_qname_attribute(xml, "dataStoreRef", q, context);
                    } else {
                        write_catalog_qname_attribute(xml, "dataStoreRef", q, "pc_store")
                    }
                }
            },
            _ => {}
        }
        if let Some(NodeDetails::CallActivity {
            called_element: Some(called_element),
        }) = &n.details
        {
            if called_element.namespace.is_empty() {
                xml.push_str(&format!(" calledElement=\"{}\"", called_element.local));
            } else if let Some(metadata) =
                n.base_extras.as_ref().filter(|e| !e.namespaces.is_empty())
            {
                write_context_qname_attribute(
                    xml,
                    "calledElement",
                    called_element,
                    &metadata.namespaces,
                );
            } else {
                let prefix = contexts
                    .first()
                    .and_then(|first| {
                        first
                            .namespaces
                            .iter()
                            .find(|(prefix, uri)| {
                                !prefix.is_empty()
                                    && prefix != "xml"
                                    && uri == &called_element.namespace
                                    && contexts.iter().all(|context| {
                                        context
                                            .namespaces
                                            .iter()
                                            .any(|(p, u)| p == prefix && u == uri)
                                    })
                            })
                            .map(|(prefix, _)| prefix.as_str())
                    })
                    .unwrap_or("pc_call");
                xml.push_str(&format!(
                    " xmlns:{prefix}=\"{}\" calledElement=\"{prefix}:{}\"",
                    esc(&called_element.namespace),
                    called_element.local
                ));
            }
        }
        anonymous_extras_attributes(xml, &n.id, n.base_extras.as_ref());
        xml.push('>');
        write_doc(xml, &n.documentation, n.base_extras.as_ref());
        write_flow_metadata(xml, n.flow_metadata.as_ref(), namespace, document);
        write_item_state(xml, n.item_aware.as_ref(), unqualified);
        if let Some(NodeDetails::Event { properties, io, .. }) = &n.details {
            for property in properties {
                write_event_property(xml, property, unqualified);
            }
            match io {
                Some(EventIo::Catch { outputs, .. }) => {
                    for port in outputs {
                        write_event_port(xml, port, "dataOutput", unqualified);
                    }
                }
                Some(EventIo::Throw { inputs, .. }) => {
                    for port in inputs {
                        write_event_port(xml, port, "dataInput", unqualified);
                    }
                }
                None => {}
            }
        }
        if let Some(io) = &n.io_specification {
            write_io_specification(xml, io, unqualified, document);
        }
        if let Some(activity) = &n.activity {
            for property in &activity.properties {
                write_event_property(xml, property, unqualified);
            }
        }
        for association in associations.iter().filter(|a| {
            a.owner == n.id
                && matches!(
                    a.kind,
                    processcraft_core::bpmn_data::DataAssociationKind::Input
                )
        }) {
            write_data_association(xml, association, unqualified, document);
        }
        for association in associations.iter().filter(|a| {
            a.owner == n.id
                && matches!(
                    a.kind,
                    processcraft_core::bpmn_data::DataAssociationKind::Output
                )
        }) {
            write_data_association(xml, association, unqualified, document);
        }
        if let Some(activity) = &n.activity {
            for role in &activity.resource_roles {
                write_resource_role(xml, role, namespace, document);
            }
        }
        if let Some(NodeDetails::Event { io, .. }) = &n.details {
            match io {
                Some(EventIo::Catch {
                    output_set: Some(set),
                    ..
                }) => write_event_set(xml, set, false, unqualified, document),
                Some(EventIo::Throw {
                    input_set: Some(set),
                    ..
                }) => write_event_set(xml, set, true, unqualified, document),
                _ => {}
            }
        }
        if let Some(NodeDetails::Gateway {
            activation_condition: Some(condition),
            ..
        }) = &n.details
        {
            write_expression(xml, "activationCondition", condition, false);
        }
        if let Some(lp) = &n.loop_characteristics {
            match lp {
                LoopCharacteristics::Standard {
                    id,
                    base,
                    test_before,
                    condition,
                    maximum,
                    ..
                } => {
                    xml.push_str(&format!(
                        "<standardLoopCharacteristics{}",
                        if unqualified {
                            format!(" xmlns=\"{B}\"")
                        } else {
                            String::new()
                        }
                    ));
                    if let Some(value) = test_before {
                        xml.push_str(&format!(" testBefore=\"{value}\""));
                    }
                    if let Some(maximum) = maximum {
                        xml.push_str(&format!(" loopMaximum=\"{}\"", esc(maximum)));
                    }
                    if let Some(id) = id {
                        write_xml_id(xml, id);
                    }
                    anonymous_base_attributes(xml, id.as_ref(), base.as_ref());
                    xml.push('>');
                    anonymous_base_children(xml, id.as_ref(), base.as_ref());
                    if let Some(value) = condition {
                        write_expression(xml, "loopCondition", value, false);
                    }
                    xml.push_str("</standardLoopCharacteristics>");
                }
                LoopCharacteristics::MultiInstance {
                    id,
                    base,
                    is_sequential,
                    cardinality,
                    completion_condition,
                    behavior,
                    loop_data_input_ref,
                    loop_data_output_ref,
                    input_data_item,
                    output_data_item,
                    complex_behavior_definitions,
                    one_behavior_event_ref,
                    none_behavior_event_ref,
                } => {
                    let refs: Vec<_> = [
                        ("oneBehaviorEventRef", one_behavior_event_ref),
                        ("noneBehaviorEventRef", none_behavior_event_ref),
                    ]
                    .into_iter()
                    .filter_map(|(name, reference)| {
                        reference
                            .as_ref()
                            .map(|r| (name, event_reference_qname(r, namespace, document)))
                    })
                    .collect();
                    let empty_default = refs.iter().any(|(_, q)| q.namespace.is_empty());
                    let mut expression_contexts = Vec::new();
                    for expression in cardinality.iter().chain(completion_condition.iter()).chain(
                        complex_behavior_definitions
                            .iter()
                            .filter_map(|c| c.condition.as_ref()),
                    ) {
                        if let Expression::Structured(data) = expression {
                            expression_contexts.push(&data.namespaces);
                        }
                    }
                    for complex in complex_behavior_definitions {
                        if let Some(event) = &complex.event {
                            for association in &event.input_associations {
                                for expression in association.transformation.iter().chain(
                                    association
                                        .assignments
                                        .iter()
                                        .flat_map(|a| [&a.from, &a.to]),
                                ) {
                                    if let Expression::Structured(data) = expression {
                                        expression_contexts.push(&data.namespaces);
                                    }
                                }
                            }
                        }
                    }
                    let mut used = std::collections::BTreeMap::<String, String>::new();
                    let mut prefix_for = |uri: &str| -> String {
                        if let Some((prefix, _)) = used.iter().find(|(_, value)| *value == uri) {
                            return prefix.clone();
                        }
                        let candidates = expression_contexts.first();
                        let prefix = candidates
                            .and_then(|first| {
                                first
                                    .iter()
                                    .find(|(prefix, value)| {
                                        !prefix.is_empty()
                                            && prefix != "xml"
                                            && !used.contains_key(prefix)
                                            && value == uri
                                            && expression_contexts
                                                .iter()
                                                .all(|scope| scope.iter().any(|(p, _)| p == prefix))
                                    })
                                    .or_else(|| {
                                        first.iter().find(|(prefix, _)| {
                                            !prefix.is_empty()
                                                && prefix != "xml"
                                                && !used.contains_key(prefix)
                                                && expression_contexts.iter().all(|scope| {
                                                    scope.iter().any(|(p, _)| p == prefix)
                                                })
                                        })
                                    })
                            })
                            .map(|(prefix, _)| prefix.clone())
                            .unwrap_or_else(|| {
                                let mut i = used.len();
                                loop {
                                    let name = format!("pc_loop_{i}");
                                    if !used.contains_key(&name) {
                                        break name;
                                    }
                                    i += 1;
                                }
                            });
                        used.insert(prefix.clone(), uri.to_owned());
                        prefix
                    };
                    let owner_prefix = empty_default.then(|| prefix_for(B));
                    let mut written_refs = Vec::new();
                    for (name, q) in &refs {
                        let value =
                            if q.namespace.is_empty() || (!empty_default && q.namespace == B) {
                                q.local.to_string()
                            } else {
                                format!("{}:{}", prefix_for(&q.namespace), q.local)
                            };
                        written_refs.push((*name, value));
                    }
                    let loop_tag = owner_prefix
                        .as_ref()
                        .map(|p| format!("{p}:multiInstanceLoopCharacteristics"))
                        .unwrap_or_else(|| "multiInstanceLoopCharacteristics".to_owned());
                    xml.push_str(&format!("<{loop_tag}"));
                    if empty_default {
                        xml.push_str(" xmlns=\"\"");
                    } else if unqualified {
                        xml.push_str(&format!(" xmlns=\"{B}\""));
                    }
                    for (prefix, uri) in &used {
                        xml.push_str(&format!(" xmlns:{prefix}=\"{}\"", esc(uri)));
                    }
                    for (name, value) in written_refs {
                        xml.push_str(&format!(" {name}=\"{}\"", esc(&value)));
                    }
                    if let Some(value) = is_sequential {
                        xml.push_str(&format!(" isSequential=\"{value}\""));
                    }
                    if let Some(value) = behavior {
                        xml.push_str(&format!(" behavior=\"{value:?}\""));
                    }
                    if let Some(id) = id {
                        write_xml_id(xml, id);
                    }
                    anonymous_base_attributes(xml, id.as_ref(), base.as_ref());
                    xml.push('>');
                    anonymous_base_children(xml, id.as_ref(), base.as_ref());
                    if let Some(value) = cardinality {
                        write_loop_expression(xml, "loopCardinality", value, empty_default);
                    }
                    if let Some(value) = loop_data_input_ref {
                        write_event_qname_text(xml, "loopDataInputRef", value, namespace, document);
                    }
                    if let Some(value) = loop_data_output_ref {
                        write_event_qname_text(
                            xml,
                            "loopDataOutputRef",
                            value,
                            namespace,
                            document,
                        );
                    }
                    if let Some(value) = input_data_item {
                        write_event_port(xml, value, "inputDataItem", empty_default);
                    }
                    if let Some(value) = output_data_item {
                        write_event_port(xml, value, "outputDataItem", empty_default);
                    }
                    for value in complex_behavior_definitions {
                        xml.push_str("<complexBehaviorDefinition");
                        if empty_default {
                            xml.push_str(&format!(" xmlns=\"{B}\""));
                        }
                        if let Some(id) = &value.id {
                            write_xml_id(xml, id);
                        }
                        anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
                        xml.push('>');
                        anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
                        if let Some(condition) = &value.condition {
                            write_expression(xml, "condition", condition, false);
                        }
                        if let Some(event) = &value.event {
                            write_implicit_throw(xml, event, namespace, document);
                        }
                        xml.push_str("</complexBehaviorDefinition>");
                    }
                    if let Some(value) = completion_condition {
                        write_loop_expression(xml, "completionCondition", value, empty_default);
                    }
                    xml.push_str(&format!("</{loop_tag}>"));
                }
            }
        }
        if let Some(a) = &n.activity
            && let Some(script) = &a.script
        {
            write_mixed_element(xml, "script", script);
        }
        if let Some(activity) = &n.activity {
            for rendering in &activity.renderings {
                write_rendering(xml, rendering);
            }
        }
        if let Some(NodeDetails::SubProcess { content }) = &n.details {
            if content.lane_set_id.is_some()
                || content.lane_set_base.is_some()
                || !content.lanes.is_empty()
            {
                xml.push_str("<laneSet");
                if let Some(id) = &content.lane_set_id {
                    write_xml_id(xml, id);
                }
                anonymous_base_attributes(
                    xml,
                    content.lane_set_id.as_ref(),
                    content.lane_set_base.as_ref(),
                );
                xml.push('>');
                anonymous_base_children(
                    xml,
                    content.lane_set_id.as_ref(),
                    content.lane_set_base.as_ref(),
                );
                write_lanes(xml, &content.lanes, None, document);
                xml.push_str("</laneSet>");
            }
            write_scope(
                xml,
                &content.nodes,
                &content.flows,
                &content.data_objects,
                &content.data_associations,
                &content.groups,
                document,
                namespace,
            );
        }
        if let Some(NodeDetails::Event {
            definitions,
            definition_refs,
            ..
        }) = &n.details
        {
            for definition in definitions {
                write_event_definition(xml, definition, unqualified, namespace, document);
            }
            for reference in definition_refs {
                write_event_qname_text(xml, "eventDefinitionRef", reference, namespace, document);
            }
        }
        if let Some(a) = &n.activity
            && let Some(value) = &a.completion_condition
        {
            write_expression(xml, "completionCondition", value, false);
        }
        xml.push_str(&format!("</{tag_name}>"));
    }
    for f in flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
        xml.push_str("<sequenceFlow");
        write_xml_id(xml, &f.id);
        write_xml_name(xml, f.name.as_deref());
        xml.push_str(&format!(
            " sourceRef=\"{}\" targetRef=\"{}\"",
            local_flow_xml_reference(document, f.source.as_ref()),
            local_flow_xml_reference(document, f.target.as_ref())
        ));
        if let Some(value) = f.flow_metadata.as_ref().and_then(|m| m.is_immediate) {
            xml.push_str(&format!(" isImmediate=\"{value}\""));
        }
        anonymous_extras_attributes(xml, &f.id, f.base_extras.as_ref());
        xml.push('>');
        write_doc(xml, &f.documentation, f.base_extras.as_ref());
        write_flow_metadata(xml, f.flow_metadata.as_ref(), namespace, document);
        if let Some(condition) = &f.condition {
            write_expression(xml, "conditionExpression", condition, true);
        }
        xml.push_str("</sequenceFlow>");
    }
    for n in nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::TextAnnotation))
    {
        xml.push_str("<textAnnotation");
        write_xml_id(xml, &n.id);
        if let Some(NodeDetails::TextAnnotation {
            text_format: Some(format),
            ..
        }) = &n.details
        {
            xml.push_str(&format!(" textFormat=\"{}\"", esc(format)));
        }
        anonymous_extras_attributes(xml, &n.id, n.base_extras.as_ref());
        xml.push('>');
        write_doc(xml, &n.documentation, n.base_extras.as_ref());
        let body = match &n.details {
            Some(NodeDetails::TextAnnotation {
                text: Some(text), ..
            }) => Some(text.clone()),
            _ => n
                .name
                .as_ref()
                .map(|s| processcraft_core::bpmn_xml::MixedXml::Text(s.clone())),
        };
        if let Some(body) = body {
            write_mixed_element(xml, "text", &body);
        }
        xml.push_str("</textAnnotation>");
    }
    for group in groups {
        let refs: Vec<_> = group
            .category_value_ref
            .iter()
            .map(|r| ("categoryValueRef", r))
            .collect();
        let mut scopes = Vec::new();
        collaboration_base_scopes(group.base.as_ref(), &mut scopes);
        let tag =
            write_collaboration_qname_owner(xml, "group", &refs, namespace, document, &scopes);
        write_xml_id(xml, &group.id);
        finish_anonymous_base_empty(xml, &tag, Some(&group.id), group.base.as_ref());
    }
    for f in flows.iter().filter(|f| f.kind == FlowKind::Association) {
        let refs: Vec<_> = [
            ("sourceRef", f.source.as_ref()),
            ("targetRef", f.target.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag = write_collaboration_qname_owner(
            xml,
            "association",
            &refs,
            namespace,
            document,
            &collaboration_flow_scopes(f),
        );
        write_xml_id(xml, &f.id);
        if let Some(direction) = f.association_direction {
            xml.push_str(&format!(" associationDirection=\"{:?}\"", direction));
        }
        anonymous_extras_attributes(xml, &f.id, f.base_extras.as_ref());
        xml.push('>');
        write_doc(xml, &f.documentation, f.base_extras.as_ref());
        xml.push_str(&format!("</{tag}>"));
    }
}
pub fn export_bpmn(
    project: &ProjectData,
    document_id: processcraft_core::ids::DocumentId,
    acknowledge_native_fields: bool,
) -> Result<ExportedBpmn, ExportFailure> {
    let Some(document) = project.documents.iter().find(|d| d.id == document_id) else {
        return Err(ExportFailure {
            report: ExportReport {
                blocked: vec!["Document not found".into()],
                ..ExportReport::default()
            },
        });
    };
    let has_links = project
        .links
        .iter()
        .any(|l| l.source.document_id == document_id)
        || project
            .dictionary_links
            .iter()
            .any(|l| l.source.document_id == document_id);
    if has_links && !acknowledge_native_fields {
        return Err(ExportFailure {
            report: ExportReport {
                omitted: vec!["Project navigation or dictionary links are native-only".into()],
                blocked: vec!["Native-only link omission requires acknowledgement".into()],
                ..ExportReport::default()
            },
        });
    }
    if let Err(error) = project.validate() {
        return Err(ExportFailure {
            report: ExportReport {
                blocked: vec![format!("Invalid project: {error}")],
                ..ExportReport::default()
            },
        });
    }
    let mut result = export_document(document, &project.resources, acknowledge_native_fields)?;
    if has_links {
        result
            .report
            .omitted
            .push("Project navigation or dictionary links are native-only".into());
    }
    Ok(result)
}
fn event_local_selectors(definition: &EventDefinition) -> Vec<&ElementSelector> {
    let refs: Vec<&EventReference> = match definition {
        EventDefinition::Message {
            message_ref,
            operation_ref,
            ..
        } => message_ref.iter().chain(operation_ref.iter()).collect(),
        EventDefinition::Signal { signal_ref, .. } => signal_ref.iter().collect(),
        EventDefinition::Error { error_ref, .. } => error_ref.iter().collect(),
        EventDefinition::Escalation { escalation_ref, .. } => escalation_ref.iter().collect(),
        EventDefinition::Link {
            sources, target, ..
        } => sources.iter().chain(target.iter()).collect(),
        EventDefinition::Compensate { activity_ref, .. } => activity_ref.iter().collect(),
        _ => Vec::new(),
    };
    refs.into_iter()
        .filter_map(|reference| match reference {
            EventReference::Local(selector) => Some(selector),
            EventReference::Qualified(_) => None,
        })
        .collect()
}
fn missing_xml_reference(document: &ExportContext) -> Option<ElementSelector> {
    fn check(document: &ExportContext, selector: &ElementSelector) -> Option<ElementSelector> {
        (!document.xml_ids.contains_key(selector)).then(|| selector.clone())
    }
    fn check_ref(document: &ExportContext, reference: &EventReference) -> Option<ElementSelector> {
        match reference {
            EventReference::Local(selector) => check(document, selector),
            EventReference::Qualified(_) => None,
        }
    }
    fn check_key(
        document: &ExportContext,
        key: &processcraft_core::bpmn_activity::CorrelationKey,
    ) -> Option<ElementSelector> {
        key.property_refs
            .iter()
            .find_map(|reference| check_ref(document, reference))
    }
    for property in &document.correlation_properties {
        for retrieval in &property.retrieval_expressions {
            if let Some(missing) = retrieval
                .message_ref
                .as_ref()
                .and_then(|reference| check_ref(document, reference))
            {
                return Some(missing);
            }
        }
    }
    if let Some(collaboration) = &document.collaboration {
        for key in collaboration.correlation_keys.iter().chain(
            collaboration
                .conversation_nodes
                .iter()
                .flat_map(|node| &node.correlation_keys),
        ) {
            if let Some(missing) = check_key(document, key) {
                return Some(missing);
            }
        }
    }
    for choreography in &document.choreographies {
        for key in choreography
            .activities
            .iter()
            .flat_map(|activity| &activity.correlation_keys)
        {
            if let Some(missing) = check_key(document, key) {
                return Some(missing);
            }
        }
    }
    for process in &document.processes {
        if let Some(metadata) = &process.activity_metadata {
            for reference in metadata
                .callable
                .supported_interface_refs
                .iter()
                .chain(&metadata.supports)
                .chain(metadata.definitional_collaboration_ref.iter())
            {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
            for binding in &metadata.callable.io_bindings {
                if let Some(missing) = binding
                    .operation_ref
                    .as_ref()
                    .and_then(|reference| check_ref(document, reference))
                {
                    return Some(missing);
                }
                for reference in binding
                    .input_data_ref
                    .iter()
                    .chain(binding.output_data_ref.iter())
                {
                    if let Some(missing) = check(document, reference) {
                        return Some(missing);
                    }
                }
            }
            for subscription in &metadata.correlation_subscriptions {
                if let Some(missing) = subscription
                    .key_ref
                    .as_ref()
                    .and_then(|reference| check_ref(document, reference))
                {
                    return Some(missing);
                }
                for binding in &subscription.bindings {
                    if let Some(missing) = binding
                        .property_ref
                        .as_ref()
                        .and_then(|reference| check_ref(document, reference))
                    {
                        return Some(missing);
                    }
                }
            }
        }
    }
    fn io_refs(
        document: &ExportContext,
        io: &processcraft_core::bpmn_data::IoSpecification,
    ) -> Option<ElementSelector> {
        io.input_sets
            .iter()
            .chain(&io.output_sets)
            .flat_map(|set| {
                set.data_refs
                    .iter()
                    .chain(&set.optional_refs)
                    .chain(&set.while_executing_refs)
                    .chain(&set.counterpart_refs)
            })
            .find_map(|r| check(document, r))
    }
    for scope in scopes(document)? {
        for metadata in scope
            .nodes
            .iter()
            .filter_map(|node| node.flow_metadata.as_ref())
            .chain(
                scope
                    .flows
                    .iter()
                    .filter_map(|flow| flow.flow_metadata.as_ref()),
            )
            .chain(
                scope
                    .data_objects
                    .iter()
                    .filter_map(|object| object.flow_metadata.as_ref()),
            )
        {
            for reference in metadata
                .category_value_refs
                .iter()
                .chain(&metadata.incoming)
                .chain(&metadata.outgoing)
            {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
        }
        for lane in scope.lanes {
            for reference in &lane.members {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for flow in scope.flows {
            for reference in flow.source.iter().chain(flow.target.iter()) {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
        }
        for group in scope.groups {
            if let Some(reference) = &group.category_value_ref
                && let Some(missing) = check_ref(document, reference)
            {
                return Some(missing);
            }
        }
        for association in scope.data_associations {
            for reference in association
                .sources
                .iter()
                .chain(std::iter::once(&association.target))
            {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for node in scope.nodes {
            if let Some(io) = &node.io_specification
                && let Some(missing) = io_refs(document, io)
            {
                return Some(missing);
            }
            if let Some(default) = node.activity.as_ref().and_then(|a| a.default_flow.as_ref())
                && let Some(missing) = check(document, default)
            {
                return Some(missing);
            }
            let refs: Vec<&ElementSelector> = match &node.details {
                Some(NodeDetails::Gateway { default_flow, .. }) => default_flow.iter().collect(),
                Some(NodeDetails::DataObjectReference { data_object_ref }) => {
                    data_object_ref.iter().collect()
                }
                Some(NodeDetails::DataStoreReference {
                    data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Legacy(r)),
                }) => vec![r],
                Some(NodeDetails::BoundaryEvent { attached_to, .. }) => vec![attached_to],
                Some(NodeDetails::Event {
                    definitions,
                    definition_refs,
                    attached_to,
                    io,
                    ..
                }) => {
                    let mut refs: Vec<_> = attached_to.iter().collect();
                    for definition in definitions {
                        refs.extend(event_local_selectors(definition));
                    }
                    for reference in definition_refs {
                        if let EventReference::Local(selector) = reference {
                            refs.push(selector);
                        }
                    }
                    let event_set = match io {
                        Some(EventIo::Catch { output_set, .. }) => output_set.as_ref(),
                        Some(EventIo::Throw { input_set, .. }) => input_set.as_ref(),
                        None => None,
                    };
                    if let Some(set) = event_set {
                        refs.extend(set.data_refs.iter());
                        refs.extend(set.optional_refs.iter());
                        refs.extend(set.while_executing_refs.iter());
                        refs.extend(set.counterpart_refs.iter());
                    }
                    refs
                }
                _ => Vec::new(),
            };
            for reference in refs {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
    }
    for definition in &document.root_event_definitions {
        for reference in event_local_selectors(definition) {
            if let Some(missing) = check(document, reference) {
                return Some(missing);
            }
        }
    }
    for process in &document.processes {
        if let Some(io) = &process.io_specification
            && let Some(missing) = io_refs(document, io)
        {
            return Some(missing);
        }
    }
    if let Some(collab) = &document.collaboration {
        for p in &collab.participants {
            if let Some(r) = &p.process
                && let Some(missing) = check_ref(document, r)
            {
                return Some(missing);
            }
        }
        for f in &collab.flows {
            for r in f.source.iter().chain(f.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for n in &collab.conversation_nodes {
            for r in &n.participant_refs {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for l in &collab.conversation_links {
            for r in l.source.iter().chain(l.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
    }
    for choreography in &document.choreographies {
        for p in &choreography.participants {
            if let Some(r) = &p.process
                && let Some(missing) = check_ref(document, r)
            {
                return Some(missing);
            }
        }
        for f in choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
        {
            for r in f.source.iter().chain(f.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for a in &choreography.activities {
            for r in a
                .participant_refs
                .iter()
                .chain(&a.message_flow_refs)
                .chain(std::iter::once(&a.initiating_participant))
            {
                if let Some(missing) = check(document, r) {
                    return Some(missing);
                }
            }
        }
    }
    for flow in document.collaboration.iter().flat_map(|c| &c.flows).chain(
        document
            .choreographies
            .iter()
            .flat_map(|c| &c.message_flows),
    ) {
        if let Some(missing) = flow
            .message_ref
            .as_ref()
            .and_then(|reference| check_ref(document, reference))
        {
            return Some(missing);
        }
    }
    for diagram in &document.diagrams {
        for shape in &diagram.shapes {
            if let Some(DiStyleRef::Local(reference)) = &shape.choreography_activity_shape
                && let Some(missing) = check(document, reference)
            {
                return Some(missing);
            }
        }
        if let Some(missing) = check(document, &diagram.plane) {
            return Some(missing);
        }
        for edge in &diagram.edges {
            for reference in edge.source_element.iter().chain(edge.target_element.iter()) {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for label in diagram
            .shapes
            .iter()
            .filter_map(|s| s.label.as_ref())
            .chain(diagram.edges.iter().filter_map(|e| e.label.as_ref()))
        {
            if let Some(DiStyleRef::Local(reference)) = &label.style_ref
                && let Some(missing) = check(document, reference)
            {
                return Some(missing);
            }
        }
        for r in diagram
            .shapes
            .iter()
            .map(|s| &s.element)
            .chain(diagram.edges.iter().map(|e| &e.element))
        {
            if let Some(missing) = check(document, r) {
                return Some(missing);
            }
        }
    }
    None
}
fn export_document(
    document: &BpmnDocument,
    resources: &[Resource],
    acknowledge_native_fields: bool,
) -> Result<ExportedBpmn, ExportFailure> {
    let mut report = ExportReport::default();
    if document.folder.is_some() {
        report
            .omitted
            .push("Document folder is a native-only field".into());
    }
    if let Capability::PreservedSource { source_resource_id } = &document.capability {
        if document.folder.is_some() && !acknowledge_native_fields {
            report
                .blocked
                .push("Native-only field omission requires acknowledgement".into());
            return Err(ExportFailure { report });
        }
        if let Some(resource) = resources.iter().find(|r| &r.id == source_resource_id) {
            report.retained.push("Original source bytes".into());
            return Ok(ExportedBpmn {
                bytes: resource.bytes.to_vec(),
                report,
            });
        }
        report
            .blocked
            .push("Original source resource is missing".into());
        return Err(ExportFailure { report });
    }
    if document.diagrams.iter().any(|d| {
        d.plane_metadata
            .as_ref()
            .and_then(|m| m.extension.as_ref())
            .is_some()
            || d.shapes.iter().any(|s| {
                s.di_metadata
                    .as_ref()
                    .and_then(|m| m.extension.as_ref())
                    .is_some()
                    || s.label
                        .as_ref()
                        .and_then(|l| l.di_metadata.as_ref())
                        .and_then(|m| m.extension.as_ref())
                        .is_some()
            })
            || d.edges.iter().any(|e| {
                e.di_metadata
                    .as_ref()
                    .and_then(|m| m.extension.as_ref())
                    .is_some()
                    || e.label
                        .as_ref()
                        .and_then(|l| l.di_metadata.as_ref())
                        .and_then(|m| m.extension.as_ref())
                        .is_some()
            })
    }) {
        report.retained.push(
            "DI extension payloads retained; foreign-schema validation is not verified".into(),
        );
    }
    if document.diagrams.iter().any(|d| d.plane_order.is_none()) {
        report
            .transformed
            .push("Legacy diagram child grouping represented as explicit plane order".into());
    }
    if !matches!(document.capability, Capability::Editable) {
        report
            .blocked
            .push("Retained extensions cannot be safely reconstructed".into());
        return Err(ExportFailure { report });
    }
    if document
        .extensions
        .iter()
        .any(|extension| extension.must_understand == Some(true))
    {
        report.blocked.push(
            "Required extension semantics are not understood; BPMN export is protected".into(),
        );
        return Err(ExportFailure { report });
    }
    if document
        .relationships
        .iter()
        .any(|relationship| relationship.sources.is_empty() || relationship.targets.is_empty())
    {
        report.blocked.push(
            "A BPMN relationship requires at least one source and target before export".into(),
        );
        return Err(ExportFailure { report });
    }
    let Ok(owner_metadata) = collect_owner_metadata(document) else {
        report
            .blocked
            .push("Common BPMN metadata is not yet fully representable by this exporter".into());
        return Err(ExportFailure { report });
    };
    if document
        .diagrams
        .iter()
        .flat_map(|d| &d.label_styles)
        .any(|style| style.font.is_none())
    {
        report
            .blocked
            .push("BPMN label styles require a Font before export".into());
        return Err(ExportFailure { report });
    }
    let activity_ports = activity_ports(document).unwrap_or_default();
    if document
        .diagrams
        .iter()
        .flat_map(|diagram| &diagram.shapes)
        .any(|shape| activity_ports.contains(&shape.element))
    {
        report.blocked.push("Activity or event data input/output ports cannot be shown in BPMN diagrams; remove their shapes before export".into());
        return Err(ExportFailure { report });
    }
    if let Some(scopes) = scopes(document) {
        let associations: std::collections::HashMap<_, _> = scopes
            .iter()
            .flat_map(|scope| scope.data_associations)
            .filter_map(|association| {
                association
                    .id
                    .as_ref()
                    .map(|id| (id.selector(), association))
            })
            .collect();
        for edge in document.diagrams.iter().flat_map(|diagram| &diagram.edges) {
            if let Some(association) = associations.get(&edge.element) {
                if association
                    .sources
                    .iter()
                    .any(|source| activity_ports.contains(source))
                    && edge.source_element.is_none()
                {
                    report.blocked.push("Data association uses a non-process output port; select the visible source representation before export".into());
                    return Err(ExportFailure { report });
                }
                if activity_ports.contains(&association.target) && edge.target_element.is_none() {
                    report.blocked.push("Data association uses a non-process input port; select the visible target representation before export".into());
                    return Err(ExportFailure { report });
                }
            }
        }
    }
    let context = ExportContext::new(document);
    if let Some(selector) = missing_xml_reference(&context) {
        report.blocked.push(format!(
            "Assign an XML ID to referenced element {selector} before BPMN export"
        ));
        return Err(ExportFailure { report });
    }
    let Some(all_scopes) = scopes(document) else {
        report
            .blocked
            .push("Unsupported nested scope or depth".into());
        return Err(ExportFailure { report });
    };
    let native_nodes = all_scopes
        .iter()
        .flat_map(|p| p.nodes)
        .chain(
            document.collaboration.iter().chain(&document.additional_collaborations)
                .filter_map(|c| c.extras.as_ref())
                .chain(document.choreographies.iter().filter_map(|c| c.extras.as_ref()))
                .flat_map(|extras| &extras.artifacts)
                .filter_map(|artifact| match artifact {
                    processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) => Some(node.as_ref()),
                    _ => None,
                }),
        )
        .any(|n| !n.owner.is_empty() || !n.contact.is_empty());
    if native_nodes {
        report
            .omitted
            .push("Node owner and contact are native-only fields".into());
    }
    if (native_nodes || document.folder.is_some()) && !acknowledge_native_fields {
        report
            .blocked
            .push("Native-only field omission requires acknowledgement".into());
        return Err(ExportFailure { report });
    }
    let conflicting_annotation_name = all_scopes.iter().flat_map(|p| p.nodes).any(|node| {
        matches!((&node.details, &node.name), (Some(NodeDetails::TextAnnotation { text: Some(_), .. }), Some(name)) if !name.is_empty())
    });
    if conflicting_annotation_name {
        report
            .blocked
            .push("Text annotation has a separate native name that cannot be exported".into());
        return Err(ExportFailure { report });
    }
    let unsupported = all_scopes.iter().any(|p| {
        p.nodes.iter().any(|n| {
            n.io_specification
                .as_ref()
                .is_some_and(|io| io.input_sets.is_empty() || io.output_sets.is_empty())
        }) || p.nodes.iter().any(|n| {
            (n.details.is_some()
                || matches!(
                    n.kind,
                    NodeKind::IntermediateCatchEvent
                        | NodeKind::BoundaryEvent
                        | NodeKind::DataObjectReference
                        | NodeKind::DataStoreReference
                        | NodeKind::SubProcess
                        | NodeKind::CallActivity
                ))
                && !matches!(
                    (&n.kind, &n.details),
                    (
                        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction,
                        Some(NodeDetails::SubProcess { .. })
                    ) | (
                        NodeKind::DataObjectReference,
                        Some(NodeDetails::DataObjectReference { .. })
                    ) | (
                        NodeKind::DataStoreReference,
                        Some(NodeDetails::DataStoreReference { .. })
                    ) | (
                        NodeKind::CallActivity,
                        Some(NodeDetails::CallActivity { .. })
                    ) | (
                        NodeKind::ExclusiveGateway
                            | NodeKind::InclusiveGateway
                            | NodeKind::ParallelGateway
                            | NodeKind::ComplexGateway
                            | NodeKind::EventBasedGateway,
                        Some(NodeDetails::Gateway { .. })
                    )
                )
                && !matches!(
                    (&n.kind, &n.details),
                    (
                        NodeKind::TextAnnotation,
                        Some(NodeDetails::TextAnnotation { .. })
                    )
                )
                && !supported_event_details(n)
        }) || p.flows.iter().any(|f| {
            !matches!(f.kind, FlowKind::Sequence | FlowKind::Association)
                || (f.kind == FlowKind::Association
                    && (f.name.as_deref().is_some_and(|name| !name.is_empty())
                        || f.condition.is_some()))
                || f.condition
                    .as_ref()
                    .is_some_and(|c| matches!(c, Expression::Legacy(text) if text.contains(':')))
        })
    }) || document.diagrams.iter().any(|d| {
        d.shapes
            .iter()
            .any(|s| s.style.is_some() || s.label.as_ref().is_some_and(|l| l.style.is_some()))
            || d.edges
                .iter()
                .any(|e| e.style.is_some() || e.label.as_ref().is_some_and(|l| l.style.is_some()))
    });
    if unsupported {
        report.blocked.push(
            "Document contains semantics or diagram details outside the qualified export profile"
                .into(),
        );
        return Err(ExportFailure { report });
    }
    let namespace = document
        .provenance
        .as_ref()
        .and_then(|p| resources.iter().find(|r| r.id == p.source_resource_id))
        .and_then(|r| tree(&r.bytes))
        .and_then(|e| e.attrs.get("targetNamespace").cloned());
    let namespace = match (
        document
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.clone())
            .or(namespace),
        &document.provenance,
    ) {
        (Some(ns), _) => ns,
        (None, None) => GENERATED_TARGET_NAMESPACE.into(),
        _ => {
            report
                .blocked
                .push("Original target namespace is unavailable".into());
            return Err(ExportFailure { report });
        }
    };
    if let Err(reason) = crate::activity_export::validate(document, &namespace) {
        report.blocked.push(reason);
        return Err(ExportFailure { report });
    }
    if all_scopes
        .iter()
        .flat_map(|p| p.nodes)
        .any(|n| match &n.details {
            Some(NodeDetails::CallActivity {
                called_element: Some(called_element),
            }) => {
                called_element.namespace == namespace
                    && !document
                        .processes
                        .iter()
                        .any(|p| p.id.xml_id() == Some(&called_element.local))
                    && !document.global_tasks.iter().any(|task| {
                        task.id.as_ref().and_then(ElementIdentity::xml_id)
                            == Some(&called_element.local)
                    })
            }
            _ => false,
        })
    {
        report
            .blocked
            .push("Call activity target is not a local callable element".into());
        return Err(ExportFailure { report });
    }
    let mut xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><definitions xmlns=\"{B}\"");
    write_xml_id(&mut xml, &document.definitions_id);
    xml.push_str(&format!(" targetNamespace=\"{}\">", esc(&namespace)));
    if let Some(definitions) = &document.definitions {
        // Insert optional metadata before the closing bracket of the root tag.
        xml.pop();
        for (name, value) in [
            ("name", &definitions.name),
            ("expressionLanguage", &definitions.expression_language),
            ("typeLanguage", &definitions.type_language),
            ("exporter", &definitions.exporter),
            ("exporterVersion", &definitions.exporter_version),
        ] {
            if let Some(value) = value {
                xml.push_str(&format!(" {name}=\"{}\"", esc(value)));
            }
        }
        for binding in &definitions.namespaces {
            if binding.prefix.is_empty() {
                if binding.uri != B {
                    report.blocked.push(
                        "Definitions default namespace cannot preserve foreign attribute context"
                            .into(),
                    );
                    return Err(ExportFailure { report });
                }
            } else if binding.prefix != "xml" {
                xml.push_str(&format!(
                    " xmlns:{}=\"{}\"",
                    binding.prefix,
                    esc(&binding.uri)
                ));
            }
        }
        for attribute in &definitions.foreign_attributes {
            let Some(prefix) = &attribute.prefix else {
                report
                    .blocked
                    .push("Definitions foreign attribute lacks a prefix".into());
                return Err(ExportFailure { report });
            };
            xml.push_str(&format!(
                " {prefix}:{}=\"{}\"",
                attribute.local,
                esc(&attribute.value)
            ));
        }
        xml.push('>');
    }
    if let Some(definitions) = &document.definitions {
        for import in &definitions.imports {
            xml.push_str(&format!(
                "<import namespace=\"{}\" location=\"{}\" importType=\"{}\"/>",
                esc(&import.namespace),
                esc(&import.location),
                esc(&import.import_type)
            ));
        }
    }
    for extension in &document.extensions {
        let unqualified = extension
            .definition
            .as_ref()
            .is_some_and(|q| q.namespace.is_empty());
        let tag = if unqualified {
            "pc_definition:extension"
        } else {
            "extension"
        };
        xml.push_str(&format!("<{tag}"));
        if unqualified {
            xml.push_str(&format!(" xmlns:pc_definition=\"{B}\" xmlns=\"\""));
        }
        if let Some(q) = &extension.definition {
            write_catalog_qname_attribute(&mut xml, "definition", q, "pc_extension");
        }
        if let Some(value) = extension.must_understand {
            xml.push_str(&format!(" mustUnderstand=\"{value}\""));
        }
        xml.push('>');
        for documentation in &extension.documentation {
            write_structured_documentation(&mut xml, documentation);
        }
        xml.push_str(&format!("</{tag}>"));
    }
    let mut root_spans = crate::root_order_export::RootSpans::new(document.root_order.is_some());
    macro_rules! record_root {
        ($id:expr, $start:expr) => {
            if let Err(reason) = root_spans.record($id, $start, xml.len()) {
                report.blocked.push(reason);
                return Err(ExportFailure { report });
            }
        };
    }
    if let Err(reason) = write_catalog(&mut xml, &document.catalog, &mut root_spans) {
        report.blocked.push(reason);
        return Err(ExportFailure { report });
    }
    for task in &document.global_tasks {
        let start = xml.len();
        write_global_task(&mut xml, task, &namespace, &context);
        record_root!(task.id.as_ref(), start);
    }
    for property in &document.correlation_properties {
        let start = xml.len();
        write_correlation_property(&mut xml, property, &namespace, &context);
        record_root!(property.id.as_ref(), start);
    }
    for category in &document.categories {
        let start = xml.len();
        xml.push_str("<category");
        write_xml_id(&mut xml, &category.id);
        if let Some(name) = &category.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        anonymous_base_attributes(&mut xml, Some(&category.id), category.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, Some(&category.id), category.base.as_ref());
        for value in &category.values {
            xml.push_str("<categoryValue");
            write_xml_id(&mut xml, &value.id);
            if let Some(text) = &value.value {
                xml.push_str(&format!(" value=\"{}\"", esc(text)));
            }
            finish_anonymous_base_empty(
                &mut xml,
                "categoryValue",
                Some(&value.id),
                value.base.as_ref(),
            );
        }
        xml.push_str("</category>");
        record_root!(Some(&category.id), start);
    }
    for store in &document.data_stores {
        let start = xml.len();
        let unqualified = store
            .item_aware
            .as_ref()
            .and_then(|m| m.item_subject_ref.as_ref())
            .is_some_and(|q| q.namespace.is_empty());
        let tag = if unqualified {
            "pc_data:dataStore"
        } else {
            "dataStore"
        };
        xml.push_str(&format!("<{tag}"));
        if unqualified {
            xml.push_str(&format!(" xmlns:pc_data=\"{B}\" xmlns=\"\""));
        }
        if let Some(id) = &store.id {
            write_xml_id(&mut xml, id);
        }
        if let Some(name) = &store.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        write_item_attributes(
            &mut xml,
            store.item_aware.as_ref(),
            metadata_context(store.base.as_ref()),
        );
        if let Some(capacity) = &store.capacity {
            xml.push_str(&format!(" capacity=\"{}\"", esc(capacity)));
        }
        if let Some(unlimited) = store.is_unlimited {
            xml.push_str(&format!(" isUnlimited=\"{}\"", unlimited));
        }
        anonymous_base_attributes(&mut xml, store.id.as_ref(), store.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, store.id.as_ref(), store.base.as_ref());
        write_item_state(&mut xml, store.item_aware.as_ref(), unqualified);
        xml.push_str(&format!("</{tag}>"));
        record_root!(store.id.as_ref(), start);
    }
    for resource in &document.event_resources {
        let start = xml.len();
        let (tag, id, name, qname) = match resource {
            EventResource::Signal {
                id,
                name,
                structure_ref,
                ..
            } => (
                "signal",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
            EventResource::Message {
                id, name, item_ref, ..
            } => (
                "message",
                id,
                name,
                item_ref.as_ref().map(|q| ("itemRef", q)),
            ),
            EventResource::Error {
                id,
                name,
                structure_ref,
                ..
            } => (
                "error",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
            EventResource::Escalation {
                id,
                name,
                structure_ref,
                ..
            } => (
                "escalation",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
        };
        let owner_tag = if let Some((_, q)) = qname {
            write_local_qname_open(&mut xml, tag, &q.namespace)
        } else {
            xml.push_str(&format!("<{tag}"));
            tag.to_owned()
        };
        write_xml_id(&mut xml, id);
        write_xml_name(&mut xml, name.as_deref());
        if let Some((attribute, q)) = qname {
            xml.push_str(&format!(
                " {attribute}=\"{}\"",
                local_qname_value(&q.local, &q.namespace)
            ));
        }
        match resource {
            EventResource::Error {
                code: Some(code), ..
            } => xml.push_str(&format!(" errorCode=\"{}\"", esc(code))),
            EventResource::Escalation {
                code: Some(code), ..
            } => xml.push_str(&format!(" escalationCode=\"{}\"", esc(code))),
            _ => {}
        }
        finish_anonymous_base_empty(&mut xml, &owner_tag, Some(id), resource.base());
        record_root!(Some(id), start);
    }
    for definition in &document.root_event_definitions {
        let start = xml.len();
        write_event_definition(&mut xml, definition, false, &namespace, &context);
        record_root!(event_definition_identity(definition), start);
    }
    for value in &document.partner_entities {
        let start = xml.len();
        xml.push_str("<partnerEntity");
        if let Some(id) = &value.id {
            write_xml_id(&mut xml, id);
        }
        write_xml_name(&mut xml, value.name.as_deref());
        anonymous_base_attributes(&mut xml, value.id.as_ref(), value.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, value.id.as_ref(), value.base.as_ref());
        for reference in &value.participant_refs {
            write_event_qname_text(&mut xml, "participantRef", reference, &namespace, &context);
        }
        xml.push_str("</partnerEntity>");
        record_root!(value.id.as_ref(), start);
    }
    for value in &document.partner_roles {
        let start = xml.len();
        xml.push_str("<partnerRole");
        if let Some(id) = &value.id {
            write_xml_id(&mut xml, id);
        }
        write_xml_name(&mut xml, value.name.as_deref());
        anonymous_base_attributes(&mut xml, value.id.as_ref(), value.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, value.id.as_ref(), value.base.as_ref());
        for reference in &value.participant_refs {
            write_event_qname_text(&mut xml, "participantRef", reference, &namespace, &context);
        }
        xml.push_str("</partnerRole>");
        record_root!(value.id.as_ref(), start);
    }
    for value in &document.endpoints {
        let start = xml.len();
        xml.push_str("<endPoint");
        if let Some(id) = &value.id {
            write_xml_id(&mut xml, id);
        }
        finish_anonymous_base_empty(&mut xml, "endPoint", value.id.as_ref(), value.base.as_ref());
        record_root!(value.id.as_ref(), start);
    }
    for c in &document.choreographies {
        let start = xml.len();
        if c.activities
            .iter()
            .any(|a| a.participant_refs.len() < 2 || !(1..=2).contains(&a.message_flow_refs.len()))
        {
            report.blocked.push("Incomplete choreography task".into());
            return Err(ExportFailure { report });
        }
        xml.push_str("<choreography");
        write_xml_id(&mut xml, &c.id);
        write_xml_name(&mut xml, c.name.as_deref());
        if let Some(value) = c.extras.as_ref().and_then(|e| e.is_closed) {
            xml.push_str(&format!(" isClosed=\"{value}\""));
        }
        anonymous_base_attributes(&mut xml, Some(&c.id), c.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, Some(&c.id), c.base.as_ref());
        for p in &c.participants {
            write_participant(&mut xml, p, &namespace, &context);
        }
        for f in &c.message_flows {
            write_message_flow(&mut xml, f, &namespace, &context);
        }
        write_collaboration_artifacts(&mut xml, c.extras.as_ref(), &namespace, &context);
        write_conversations(&mut xml, &c.conversation_nodes, None, &namespace, &context);
        write_collaboration_associations(&mut xml, c.extras.as_ref(), &namespace, &context);
        for key in &c.correlation_keys {
            write_correlation_key(&mut xml, key, &namespace, &context);
        }
        write_collaboration_refs(&mut xml, c.extras.as_ref(), &namespace, &context);
        write_conversation_links(&mut xml, &c.conversation_links, &namespace, &context);
        for a in &c.activities {
            let tag = write_local_qname_open(&mut xml, "choreographyTask", &namespace);
            write_xml_id(&mut xml, &a.id);
            write_xml_name(&mut xml, a.name.as_deref());
            xml.push_str(&format!(
                " initiatingParticipantRef=\"{}\"",
                local_qname_value(
                    xml_reference(&context, &a.initiating_participant),
                    &namespace
                )
            ));
            anonymous_base_attributes(&mut xml, Some(&a.id), a.base.as_ref());
            xml.push('>');
            anonymous_base_children(&mut xml, Some(&a.id), a.base.as_ref());
            write_flow_metadata(&mut xml, a.flow_metadata.as_ref(), &namespace, &context);
            for participant in &a.participant_refs {
                write_local_qname_text(
                    &mut xml,
                    "participantRef",
                    xml_reference(&context, participant),
                    &namespace,
                );
            }
            for flow in &a.message_flow_refs {
                write_local_qname_text(
                    &mut xml,
                    "messageFlowRef",
                    xml_reference(&context, flow),
                    &namespace,
                );
            }
            for key in &a.correlation_keys {
                write_correlation_key(&mut xml, key, &namespace, &context);
            }
            xml.push_str(&format!("</{tag}>"));
        }
        for f in &c.sequence_flows {
            xml.push_str("<sequenceFlow");
            write_xml_id(&mut xml, &f.id);
            write_xml_name(&mut xml, f.name.as_deref());
            xml.push_str(&format!(
                " sourceRef=\"{}\" targetRef=\"{}\"",
                local_flow_xml_reference(&context, f.source.as_ref()),
                local_flow_xml_reference(&context, f.target.as_ref())
            ));
            if let Some(value) = f.flow_metadata.as_ref().and_then(|m| m.is_immediate) {
                xml.push_str(&format!(" isImmediate=\"{value}\""));
            }
            anonymous_extras_attributes(&mut xml, &f.id, f.base_extras.as_ref());
            xml.push('>');
            write_doc(&mut xml, &f.documentation, f.base_extras.as_ref());
            write_flow_metadata(&mut xml, f.flow_metadata.as_ref(), &namespace, &context);
            if let Some(condition) = &f.condition {
                write_expression(&mut xml, "conditionExpression", condition, true);
            }
            xml.push_str("</sequenceFlow>");
        }
        xml.push_str("</choreography>");
        record_root!(Some(&c.id), start);
    }
    for c in document
        .collaboration
        .iter()
        .chain(&document.additional_collaborations)
    {
        let start = xml.len();
        let tag = if c.kind.is_some() {
            "globalConversation"
        } else {
            "collaboration"
        };
        xml.push_str(&format!("<{tag}"));
        write_xml_id(&mut xml, &c.id);
        write_xml_name(&mut xml, c.name.as_deref());
        if let Some(value) = c.extras.as_ref().and_then(|e| e.is_closed) {
            xml.push_str(&format!(" isClosed=\"{value}\""));
        }
        anonymous_base_attributes(&mut xml, Some(&c.id), c.base.as_ref());
        xml.push('>');
        anonymous_base_children(&mut xml, Some(&c.id), c.base.as_ref());
        for p in &c.participants {
            write_participant(&mut xml, p, &namespace, &context);
        }
        for f in &c.flows {
            write_message_flow(&mut xml, f, &namespace, &context);
        }
        write_collaboration_artifacts(&mut xml, c.extras.as_ref(), &namespace, &context);
        write_conversations(&mut xml, &c.conversation_nodes, None, &namespace, &context);
        write_collaboration_associations(&mut xml, c.extras.as_ref(), &namespace, &context);
        for key in &c.correlation_keys {
            write_correlation_key(&mut xml, key, &namespace, &context);
        }
        write_collaboration_refs(&mut xml, c.extras.as_ref(), &namespace, &context);
        write_conversation_links(&mut xml, &c.conversation_links, &namespace, &context);
        xml.push_str(&format!("</{tag}>"));
        record_root!(Some(&c.id), start);
    }
    for p in &document.processes {
        let start = xml.len();
        xml.push_str("<process");
        write_xml_id(&mut xml, &p.id);
        write_xml_name(&mut xml, p.name.as_deref());
        if let Some(value) = p.is_executable {
            xml.push_str(&format!(" isExecutable=\"{value}\""));
        }
        if let Some(metadata) = &p.activity_metadata {
            if let Some(value) = metadata.process_type {
                xml.push_str(&format!(" processType=\"{value:?}\""));
            }
            if let Some(value) = metadata.is_closed {
                xml.push_str(&format!(" isClosed=\"{value}\""));
            }
            if let Some(reference) = &metadata.definitional_collaboration_ref {
                let q = event_reference_qname(reference, &namespace, &context);
                if let Some(extra) = p.base_extras.as_ref().filter(|e| !e.namespaces.is_empty()) {
                    write_context_qname_attribute(
                        &mut xml,
                        "definitionalCollaborationRef",
                        &q,
                        &extra.namespaces,
                    );
                } else {
                    write_catalog_qname_attribute(
                        &mut xml,
                        "definitionalCollaborationRef",
                        &q,
                        "pc_collab",
                    );
                }
            }
        }
        anonymous_extras_attributes(&mut xml, &p.id, p.base_extras.as_ref());
        xml.push('>');
        write_doc(&mut xml, &p.documentation, p.base_extras.as_ref());
        if let Some(metadata) = &p.activity_metadata {
            for reference in &metadata.callable.supported_interface_refs {
                write_event_qname_text(
                    &mut xml,
                    "supportedInterfaceRef",
                    reference,
                    &namespace,
                    &context,
                );
            }
        }
        if let Some(io) = &p.io_specification {
            write_io_specification(&mut xml, io, false, &context);
        }
        if let Some(metadata) = &p.activity_metadata {
            for binding in &metadata.callable.io_bindings {
                write_io_binding(&mut xml, binding, &namespace, &context);
            }
            if let Some(audit) = &metadata.auditing {
                write_audit_monitor(&mut xml, "auditing", audit);
            }
            if let Some(monitor) = &metadata.monitoring {
                write_audit_monitor(&mut xml, "monitoring", monitor);
            }
            for property in &metadata.properties {
                write_event_property(&mut xml, property, false);
            }
        }
        if p.lane_set_id.is_some() || p.lane_set_base.is_some() || !p.lanes.is_empty() {
            xml.push_str("<laneSet");
            if let Some(id) = &p.lane_set_id {
                write_xml_id(&mut xml, id);
            }
            anonymous_base_attributes(&mut xml, p.lane_set_id.as_ref(), p.lane_set_base.as_ref());
            xml.push('>');
            anonymous_base_children(&mut xml, p.lane_set_id.as_ref(), p.lane_set_base.as_ref());
            write_lanes(&mut xml, &p.lanes, None, &context);
            xml.push_str("</laneSet>");
        }
        write_scope(
            &mut xml,
            &p.nodes,
            &p.flows,
            &p.data_objects,
            &p.data_associations,
            &p.groups,
            &context,
            &namespace,
        );
        if let Some(metadata) = &p.activity_metadata {
            for role in &metadata.resource_roles {
                write_resource_role(&mut xml, role, &namespace, &context);
            }
            for subscription in &metadata.correlation_subscriptions {
                write_correlation_subscription(&mut xml, subscription, &namespace, &context);
            }
            for reference in &metadata.supports {
                write_event_qname_text(&mut xml, "supports", reference, &namespace, &context);
            }
        }
        xml.push_str("</process>");
        record_root!(Some(&p.id), start);
    }
    if let Err(reason) = root_spans.finish(&mut xml, document.root_order.as_deref()) {
        report.blocked.push(reason);
        return Err(ExportFailure { report });
    }
    for d in &document.diagrams {
        let ns_binding = format!(" xmlns=\"{}\"", esc(&namespace));
        let metadata_contexts: Vec<_> = d
            .plane_metadata
            .iter()
            .chain(d.shapes.iter().filter_map(|s| s.di_metadata.as_ref()))
            .chain(d.edges.iter().filter_map(|e| e.di_metadata.as_ref()))
            .chain(
                d.shapes
                    .iter()
                    .filter_map(|s| s.label.as_ref().and_then(|l| l.di_metadata.as_ref())),
            )
            .chain(
                d.edges
                    .iter()
                    .filter_map(|e| e.label.as_ref().and_then(|l| l.di_metadata.as_ref())),
            )
            .collect();
        let di_prefix = metadata_contexts
            .first()
            .and_then(|m| {
                m.namespaces.iter().find(|n| {
                    !n.prefix.is_empty()
                        && n.prefix != "xml"
                        && metadata_contexts.iter().all(|other| {
                            other
                                .namespaces
                                .iter()
                                .any(|candidate| candidate.prefix == n.prefix)
                        })
                })
            })
            .map(|n| n.prefix.as_str())
            .unwrap_or("bpmndi");
        xml.push_str(&format!(
            "<{di_prefix}:BPMNDiagram xmlns:{di_prefix}=\"{BD}\""
        ));
        write_xml_id(&mut xml, &d.id);
        write_xml_name(&mut xml, d.name.as_deref());
        if let Some(documentation) = &d.documentation {
            xml.push_str(&format!(" documentation=\"{}\"", esc(documentation)));
        }
        if let Some(resolution) = d.resolution {
            xml.push_str(&format!(" resolution=\"{resolution}\""));
        }
        let plane_prefix = di_owner_prefix(d.plane_metadata.as_ref(), di_prefix);
        xml.push_str(&format!("><{plane_prefix}:BPMNPlane"));
        if d.plane_metadata.is_some() {
            write_di_attributes(&mut xml, d.plane_metadata.as_ref());
        } else {
            xml.push_str(&ns_binding);
        }
        write_xml_id(&mut xml, &d.plane_id);
        write_di_qname(
            &mut xml,
            "bpmnElement",
            &namespace,
            xml_reference(&context, &d.plane),
            d.plane_metadata.as_ref(),
        );
        xml.push('>');
        write_di_extension(&mut xml, d.plane_metadata.as_ref());
        let order: Vec<_> = d.plane_order.clone().unwrap_or_else(|| {
            d.shapes
                .iter()
                .map(|s| s.id.selector())
                .chain(d.edges.iter().map(|e| e.id.selector()))
                .collect()
        });
        let shapes: std::collections::HashMap<_, _> = d
            .shapes
            .iter()
            .map(|shape| (shape.id.selector(), shape))
            .collect();
        let edges: std::collections::HashMap<_, _> = d
            .edges
            .iter()
            .map(|edge| (edge.id.selector(), edge))
            .collect();
        for selector in order {
            if let Some(s) = shapes.get(&selector) {
                let b = &s.bounds;
                let shape_prefix = di_owner_prefix(s.di_metadata.as_ref(), plane_prefix);
                xml.push_str(&format!("<{shape_prefix}:BPMNShape"));
                let empty_band_namespace = s.di_metadata.is_none()
                    && !namespace.is_empty()
                    && matches!(&s.choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace.is_empty());
                let semantic_prefix = s
                    .label
                    .as_ref()
                    .and_then(|label| label.di_metadata.as_ref())
                    .and_then(|metadata| {
                        metadata
                            .namespaces
                            .iter()
                            .find(|binding| {
                                !binding.prefix.is_empty()
                                    && binding.prefix != shape_prefix
                                    && binding.uri == namespace
                            })
                            .or_else(|| {
                                metadata.namespaces.iter().find(|binding| {
                                    !binding.prefix.is_empty()
                                        && binding.prefix != "xml"
                                        && binding.prefix != shape_prefix
                                })
                            })
                    })
                    .map(|binding| binding.prefix.as_str())
                    .unwrap_or("pc_semantic");
                if s.di_metadata.is_some() {
                    write_di_attributes(&mut xml, s.di_metadata.as_ref());
                } else if empty_band_namespace {
                    xml.push_str(&format!(
                        " xmlns=\"\" xmlns:{semantic_prefix}=\"{}\"",
                        esc(&namespace)
                    ));
                } else {
                    xml.push_str(&ns_binding);
                }
                write_xml_id(&mut xml, &s.id);
                if empty_band_namespace {
                    xml.push_str(&format!(
                        " bpmnElement=\"{semantic_prefix}:{}\"",
                        xml_reference(&context, &s.element)
                    ));
                } else {
                    write_di_qname(
                        &mut xml,
                        "bpmnElement",
                        &namespace,
                        xml_reference(&context, &s.element),
                        s.di_metadata.as_ref(),
                    );
                }
                if let Some(expanded) = s.is_expanded {
                    xml.push_str(&format!(" isExpanded=\"{}\"", expanded));
                }
                if let Some(horizontal) = s.is_horizontal {
                    xml.push_str(&format!(" isHorizontal=\"{}\"", horizontal));
                }
                if let Some(marker) = s.is_marker_visible {
                    xml.push_str(&format!(" isMarkerVisible=\"{}\"", marker));
                }
                if let Some(value) = s.is_message_visible {
                    xml.push_str(&format!(" isMessageVisible=\"{value}\""));
                }
                if let Some(value) = s.participant_band_kind {
                    let token = match value {
                        ParticipantBandKind::TopInitiating => "top_initiating",
                        ParticipantBandKind::MiddleInitiating => "middle_initiating",
                        ParticipantBandKind::BottomInitiating => "bottom_initiating",
                        ParticipantBandKind::TopNonInitiating => "top_non_initiating",
                        ParticipantBandKind::MiddleNonInitiating => "middle_non_initiating",
                        ParticipantBandKind::BottomNonInitiating => "bottom_non_initiating",
                    };
                    xml.push_str(&format!(" participantBandKind=\"{token}\""));
                }
                if let Some(reference) = &s.choreography_activity_shape {
                    let q = match reference {
                        DiStyleRef::Local(id) => QNameRef {
                            namespace: namespace.clone(),
                            local: xml_reference(&context, id).clone(),
                        },
                        DiStyleRef::Qualified(q) => q.clone(),
                    };
                    if s.di_metadata.is_some() {
                        write_di_qname(
                            &mut xml,
                            "choreographyActivityShape",
                            &q.namespace,
                            &q.local,
                            s.di_metadata.as_ref(),
                        );
                    } else if q.namespace == namespace
                        || (empty_band_namespace && q.namespace.is_empty())
                    {
                        xml.push_str(&format!(" choreographyActivityShape=\"{}\"", q.local));
                    } else {
                        xml.push_str(&format!(" xmlns:pc_band_ref=\"{}\" choreographyActivityShape=\"pc_band_ref:{}\"", esc(&q.namespace), q.local));
                    }
                }
                xml.push('>');
                write_di_extension(&mut xml, s.di_metadata.as_ref());
                xml.push_str(&format!(
                    "<dc:Bounds xmlns:dc=\"{DC}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                    b.x, b.y, b.width, b.height
                ));
                if let Some(label) = &s.label {
                    write_label(&mut xml, label, &namespace, &context, shape_prefix);
                } else if s.label_visibility.is_none() {
                    xml.push_str(&format!("<{shape_prefix}:BPMNLabel/>"));
                }
                xml.push_str(&format!("</{shape_prefix}:BPMNShape>"));
            } else if let Some(e) = edges.get(&selector) {
                let edge_prefix = di_owner_prefix(e.di_metadata.as_ref(), plane_prefix);
                xml.push_str(&format!("<{edge_prefix}:BPMNEdge"));
                if e.di_metadata.is_some() {
                    write_di_attributes(&mut xml, e.di_metadata.as_ref());
                } else {
                    xml.push_str(&ns_binding);
                }
                write_xml_id(&mut xml, &e.id);
                write_di_qname(
                    &mut xml,
                    "bpmnElement",
                    &namespace,
                    xml_reference(&context, &e.element),
                    e.di_metadata.as_ref(),
                );
                if let Some(source) = &e.source_element {
                    write_di_qname(
                        &mut xml,
                        "sourceElement",
                        &namespace,
                        xml_reference(&context, source),
                        e.di_metadata.as_ref(),
                    );
                }
                if let Some(target) = &e.target_element {
                    write_di_qname(
                        &mut xml,
                        "targetElement",
                        &namespace,
                        xml_reference(&context, target),
                        e.di_metadata.as_ref(),
                    );
                }
                if let Some(value) = e.message_visible_kind {
                    let token = match value {
                        MessageVisibleKind::Initiating => "initiating",
                        MessageVisibleKind::NonInitiating => "non_initiating",
                    };
                    xml.push_str(&format!(" messageVisibleKind=\"{token}\""));
                }
                xml.push('>');
                write_di_extension(&mut xml, e.di_metadata.as_ref());
                for p in &e.waypoints {
                    xml.push_str(&format!(
                        "<di:waypoint xmlns:di=\"{DI}\" x=\"{}\" y=\"{}\"/>",
                        p.x, p.y
                    ));
                }
                if let Some(label) = &e.label {
                    write_label(&mut xml, label, &namespace, &context, edge_prefix);
                } else if e.label_visibility.is_none() {
                    xml.push_str(&format!("<{edge_prefix}:BPMNLabel/>"));
                }
                xml.push_str(&format!("</{edge_prefix}:BPMNEdge>"));
            }
        }
        xml.push_str(&format!("</{plane_prefix}:BPMNPlane>"));
        for style in &d.label_styles {
            xml.push_str(&format!("<{di_prefix}:BPMNLabelStyle"));
            if let Some(id) = &style.id {
                write_xml_id(&mut xml, id);
            }
            xml.push('>');
            if let Some(font) = &style.font {
                xml.push_str(&format!("<dc:Font xmlns:dc=\"{DC}\""));
                if let Some(name) = &font.name {
                    xml.push_str(&format!(" name=\"{}\"", esc(name)));
                }
                if let Some(size) = font.size {
                    xml.push_str(&format!(" size=\"{size}\""));
                }
                for (key, value) in [
                    ("isBold", font.is_bold),
                    ("isItalic", font.is_italic),
                    ("isUnderline", font.is_underline),
                    ("isStrikeThrough", font.is_strike_through),
                ] {
                    if let Some(value) = value {
                        xml.push_str(&format!(" {key}=\"{value}\""));
                    }
                }
                xml.push_str("/>");
            }
            xml.push_str(&format!("</{di_prefix}:BPMNLabelStyle>"));
        }
        xml.push_str(&format!("</{di_prefix}:BPMNDiagram>"));
    }
    for relationship in &document.relationships {
        xml.push_str("<relationship");
        if let Some(id) = &relationship.id {
            write_xml_id(&mut xml, id);
        }
        xml.push_str(&format!(
            " type=\"{}\"",
            esc(&relationship.relationship_type)
        ));
        if let Some(direction) = relationship.direction {
            xml.push_str(&format!(" direction=\"{direction:?}\""));
        }
        anonymous_base_attributes(
            &mut xml,
            relationship.id.as_ref(),
            relationship.base.as_ref(),
        );
        xml.push('>');
        anonymous_base_children(
            &mut xml,
            relationship.id.as_ref(),
            relationship.base.as_ref(),
        );
        for (tag, values) in [
            ("source", &relationship.sources),
            ("target", &relationship.targets),
        ] {
            for q in values {
                xml.push_str(&format!("<pc_relationship:{tag} xmlns:pc_relationship=\"{B}\" xmlns=\"{}\">{}</pc_relationship:{tag}>", esc(&q.namespace), q.local));
            }
        }
        xml.push_str("</relationship>");
    }
    xml.push_str("</definitions>");
    let Some(xml) = restore_owner_defaults(&xml, &owner_metadata) else {
        report
            .blocked
            .push("BPMN owner namespace context cannot be restored safely".into());
        return Err(ExportFailure { report });
    };
    let Some(xml) = insert_owner_metadata(&xml, owner_metadata) else {
        report
            .blocked
            .push("BPMN metadata owner or namespace context cannot be represented safely".into());
        return Err(ExportFailure { report });
    };
    let reparsed = parse_profile(xml.as_bytes());
    if let Some(parsed) = &reparsed {
        match (
            crate::interchange_compare::xml_model_value_for_export(document, &namespace),
            crate::interchange_compare::xml_model_value_for_export(parsed, &namespace),
        ) {
            (Ok(expected), Ok(actual)) if expected == actual => {}
            _ => {
                report.blocked.push(
                    "BPMN typed owner paths or XML-representable fields cannot be preserved".into(),
                );
                return Err(ExportFailure { report });
            }
        }
    }
    if import_bpmn(xml.as_bytes()).is_err() || reparsed.is_none() {
        report
            .blocked
            .push("Document cannot be represented by the qualified profile".into());
        return Err(ExportFailure { report });
    }
    report
        .retained
        .push("Qualified semantics and diagram geometry".into());
    report
        .transformed
        .push("Canonical XML serialization".into());
    if document.diagrams.iter().any(|diagram| {
        diagram
            .shapes
            .iter()
            .any(|shape| shape.label_visibility.is_none())
            || diagram
                .edges
                .iter()
                .any(|edge| edge.label_visibility.is_none())
    }) {
        report
            .transformed
            .push("Legacy implicit diagram labels represented as explicit BPMN labels".into());
    }
    Ok(ExportedBpmn {
        bytes: xml.into_bytes(),
        report,
    })
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}
fn write_namespace_context(
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
fn write_structured_documentation(
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
                .any(|n| n.prefix.is_empty() && n.uri == B)
            {
                "documentation".to_owned()
            } else {
                let prefix = content
                    .namespaces
                    .iter()
                    .find(|n| !n.prefix.is_empty() && n.uri == B)
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
        xml.push_str(&format!(" xmlns=\"{B}\""));
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
fn write_mixed_element(xml: &mut String, tag: &str, body: &processcraft_core::bpmn_xml::MixedXml) {
    use processcraft_core::bpmn_xml::MixedXml;
    match body {
        MixedXml::Text(text) => xml.push_str(&format!("<{tag}>{}</{tag}>", esc(text))),
        MixedXml::Structured(content) => {
            let name = if content
                .namespaces
                .iter()
                .any(|n| n.prefix.is_empty() && n.uri == B)
            {
                tag.to_owned()
            } else {
                format!(
                    "{}:{tag}",
                    content
                        .namespaces
                        .iter()
                        .find(|n| !n.prefix.is_empty() && n.uri == B)
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
fn write_extension_elements(
    xml: &mut String,
    extension: &processcraft_core::bpmn_metadata::ExtensionElements,
) {
    let context = &extension.content.namespaces;
    let tag = if context.iter().any(|n| n.prefix.is_empty() && n.uri == B) {
        "extensionElements".to_owned()
    } else {
        format!(
            "{}:extensionElements",
            context
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == B)
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

fn write_catalog_qname_attribute(xml: &mut String, attribute: &str, q: &QNameRef, prefix: &str) {
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
fn write_context_qname_attribute(
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
            "<bpmn:{tag} xmlns:bpmn=\"{B}\" xmlns=\"\">{}</bpmn:{tag}>",
            q.local
        ));
    } else {
        xml.push_str(&format!(
            "<bpmn:{tag} xmlns:bpmn=\"{B}\" xmlns:q=\"{}\">q:{}</bpmn:{tag}>",
            esc(&q.namespace),
            q.local
        ));
    }
}
fn metadata_context(
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
        let tag = if context.iter().any(|n| n.prefix.is_empty() && n.uri == B) {
            local.to_owned()
        } else {
            let prefix = context
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == B)
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
        xml.push_str(&format!(" xmlns:bpmn=\"{B}\" xmlns=\"\""));
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
fn write_catalog(
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
fn write_loop_expression(
    xml: &mut String,
    tag: &str,
    expression: &Expression,
    restore_default: bool,
) {
    if restore_default && matches!(expression, Expression::Legacy(_)) {
        let mut child = String::new();
        write_expression(&mut child, tag, expression, false);
        if let Some(index) = child.find(|c: char| c == '>' || c.is_whitespace()) {
            child.insert_str(index, &format!(" xmlns=\"{B}\""));
        }
        xml.push_str(&child);
    } else {
        write_expression(xml, tag, expression, false);
    }
}
fn write_expression(xml: &mut String, tag: &str, expression: &Expression, legacy_formal: bool) {
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
    let tag_name = if default == B {
        tag.to_string()
    } else if let Some(prefix) = existing(B) {
        format!("{prefix}:{tag}")
    } else {
        "!".to_string()
    };
    let xp = existing("http://www.w3.org/2001/XMLSchema-instance");
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
        if default == B {
            local.to_owned()
        } else if let Some(prefix) = existing(B) {
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
fn di_owner_prefix<'a>(
    metadata: Option<&'a processcraft_core::model::DiElementMetadata>,
    fallback: &'a str,
) -> &'a str {
    metadata
        .and_then(|m| {
            m.namespaces
                .iter()
                .find(|n| !n.prefix.is_empty() && n.uri == BD)
                .map(|n| n.prefix.as_str())
        })
        .unwrap_or(fallback)
}
fn write_di_attributes(
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
fn write_di_extension(
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
fn write_di_qname(
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
fn write_label(
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

fn documentation(e: &Elem) -> Option<String> {
    if e.children.is_empty() {
        return Some(String::new());
    }
    if e.children.len() != 1 {
        return None;
    }
    let d = &e.children[0];
    if d.ns != B || d.name != "documentation" || !d.attrs.is_empty() || !d.children.is_empty() {
        return None;
    }
    Some(d.text.clone())
}
fn write_doc(
    xml: &mut String,
    docs: &processcraft_core::bpmn_metadata::DocumentationSet,
    extras: Option<&processcraft_core::bpmn_metadata::BaseExtras>,
) {
    match docs {
        processcraft_core::bpmn_metadata::DocumentationSet::Legacy(text) if !text.is_empty() => {
            xml.push_str(&format!(
                "<documentation xmlns=\"{B}\">{}</documentation>",
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

fn flow_parts(e: &Elem) -> Option<(String, Option<Expression>)> {
    let mut documentation = String::new();
    let mut condition = None;
    let mut seen_doc = false;
    for c in &e.children {
        if c.ns != B {
            return None;
        }
        match c.name.as_str() {
            "documentation" if !seen_doc && c.attrs.is_empty() && c.children.is_empty() => {
                documentation = c.text.clone();
                seen_doc = true;
            }
            "conditionExpression" if condition.is_none() => {
                condition = Some(parse_expression(c, false)?);
            }
            _ => return None,
        }
    }
    Some((documentation, condition))
}
