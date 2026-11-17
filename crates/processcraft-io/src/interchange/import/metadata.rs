//! Base, flow-element, expression and label metadata extraction for the import parser.
use crate::interchange::import::elements::event_reference;
use crate::interchange::import::tree::{
    Elem, ElemContent, attrs, bounds, di_attrs, extension_elements, optional_bool, optional_id,
    structured_documentation, xml_content, xml_namespaces,
};
use crate::interchange::shared::PendingBase;
use crate::ns::{BPMN, BPMNDI, DI};
use processcraft_core::bpmn_model::{Expression, ExpressionData, ExpressionKind};
use processcraft_core::model::*;
use std::collections::BTreeMap;
pub(super) fn documentation_set(
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
            | "subChoreography"
            | "callChoreography"
            | "globalChoreographyTask"
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
pub(super) fn extract_owner_metadata(
    e: &mut Elem,
    pending: &mut BTreeMap<String, PendingBase>,
) -> Option<()> {
    if e.opaque {
        return Some(());
    }
    for child in &mut e.children {
        extract_owner_metadata(child, pending)?;
    }
    if e.ns != BPMN || !supported_base_owner(&e.name) {
        return Some(());
    }
    let mut docs = Vec::new();
    let mut extension = None;
    let mut rest = Vec::new();
    let mut phase = 0;
    for child in std::mem::take(&mut e.children) {
        if child.ns == BPMN && child.name == "documentation" && phase == 0 {
            docs.push(structured_documentation(&child)?);
        } else if child.ns == BPMN
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
pub(super) fn take_anonymous_base(
    e: &Elem,
) -> Option<processcraft_core::bpmn_metadata::BaseMetadata> {
    let pending = e.anonymous_base.as_ref()?;
    e.anonymous_base_consumed.set(true);
    Some(processcraft_core::bpmn_metadata::BaseMetadata {
        namespaces: pending.extras.namespaces.clone(),
        documentation: pending.docs.clone(),
        foreign_attributes: pending.extras.foreign_attributes.clone(),
        extension_elements: pending.extras.extension_elements.clone(),
    })
}
pub(super) fn anonymous_documentation(
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
pub(super) fn anonymous_extras(e: &Elem) -> Option<processcraft_core::bpmn_metadata::BaseExtras> {
    let pending = e.anonymous_base.as_ref()?;
    e.anonymous_base_consumed.set(true);
    let extras = pending.extras.clone();
    (!extras.namespaces.is_empty()
        || !extras.foreign_attributes.is_empty()
        || extras.extension_elements.is_some())
    .then_some(extras)
}
pub(super) fn has_unconsumed_anonymous_base(e: &Elem) -> bool {
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
            | "subChoreography"
            | "callChoreography"
            | "event"
    )
}
pub(super) fn take_flow_metadata(
    e: &Elem,
) -> Option<processcraft_core::bpmn_activity::FlowElementMetadata> {
    let metadata = e.flow_metadata.clone()?;
    e.flow_metadata_consumed.set(true);
    Some(metadata)
}
pub(super) fn has_unconsumed_flow_metadata(e: &Elem) -> bool {
    (e.flow_metadata.is_some() && !e.flow_metadata_consumed.get())
        || e.children.iter().any(has_unconsumed_flow_metadata)
}
pub(super) fn extract_flow_metadata(e: &mut Elem, target_namespace: &str) -> Option<()> {
    if e.opaque {
        return Some(());
    }
    for child in &mut e.children {
        extract_flow_metadata(child, target_namespace)?;
    }
    if e.ns != BPMN || !is_flow_element(&e.name) {
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
            (BPMN, "auditing") if phase == 0 && result.auditing.is_none() => {
                if !attrs(&child, &["id"]) || !child.children.is_empty() {
                    return None;
                }
                result.auditing = Some(processcraft_core::bpmn_activity::BaseOwner {
                    id: optional_id(&child)?,
                    base: take_anonymous_base(&child),
                });
                phase = 1;
            }
            (BPMN, "monitoring") if phase <= 1 && result.monitoring.is_none() => {
                if !attrs(&child, &["id"]) || !child.children.is_empty() {
                    return None;
                }
                result.monitoring = Some(processcraft_core::bpmn_activity::BaseOwner {
                    id: optional_id(&child)?,
                    base: take_anonymous_base(&child),
                });
                phase = 2;
            }
            (BPMN, "categoryValueRef") if phase <= 2 => {
                if !attrs(&child, &[]) || !child.children.is_empty() {
                    return None;
                }
                result.category_value_refs.push(event_reference(
                    child.catalog_text_qname.as_ref()?,
                    target_namespace,
                ));
                phase = 2;
            }
            (BPMN, "incoming")
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
            (BPMN, "outgoing")
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
            (BPMN, "auditing" | "monitoring" | "categoryValueRef" | "incoming" | "outgoing") => {
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
pub(super) fn is_expression(name: &str) -> bool {
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
pub(super) fn parse_di_metadata(
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
pub(super) fn parse_label(e: &Elem, target_namespace: &str) -> Option<Label> {
    if e.ns != BPMNDI || e.name != "BPMNLabel" || !di_attrs(e, &["id", "labelStyle"]) {
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
pub(super) fn parse_expression(e: &Elem, implicit_formal: bool) -> Option<Expression> {
    if e.ns != BPMN
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
                if child.ns == BPMN && child.name == "documentation" && !seen_extension {
                    ExpressionPart::Documentation {
                        documentation: structured_documentation(child)?,
                    }
                } else if child.ns == BPMN && child.name == "extensionElements" && !seen_extension {
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
pub(super) fn flow_parts(e: &Elem) -> Option<(String, Option<Expression>)> {
    let mut documentation = String::new();
    let mut condition = None;
    let mut seen_doc = false;
    for c in &e.children {
        if c.ns != BPMN {
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
