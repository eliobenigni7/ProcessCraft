//! Conservative interchange: validate first, then qualify a small lossless profile.
use crate::bpmn::{ImportError, import_bpmn};
use processcraft_core::{
    bpmn_catalog::{
        BpmnResource, Interface, ItemDefinition, ItemKind, Operation, ResourceParameter,
    },
    bpmn_model::{
        EventDefinition, EventGatewayType, EventResource, Expression, ExpressionData,
        ExpressionKind, GatewayDirection, LoopCharacteristics, NodeDetails, QNameRef, TimerKind,
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
    let mut doc = parse_profile(bytes).unwrap_or_else(|| {
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
                let (ns, local) = r.resolver().resolve_element(e.name());
                let ns = match ns {
                    ResolveResult::Bound(n) => n.0.to_string(),
                    _ => return None,
                };
                let mut item = Elem {
                    ns,
                    name: local.as_ref().into(),
                    ..Elem::default()
                };
                if item.ns == B && matches!(item.name.as_str(), "script" | "text") {
                    let (default, _) = r.resolver().resolve_element(QName("context"));
                    if !matches!(default, ResolveResult::Bound(uri) if uri.0 == B) {
                        return None;
                    }
                }
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
                    if let ResolveResult::Bound(uri) = ns {
                        if item.ns != B
                            || !is_expression(&item.name)
                            || uri.0 != "http://www.w3.org/2001/XMLSchema-instance"
                            || key != "type"
                        {
                            return None;
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
                    if item.ns == B
                        && matches!(
                            (item.name.as_str(), key),
                            ("itemDefinition", "structureRef")
                                | ("interface" | "operation", "implementationRef")
                                | ("resourceParameter", "type")
                                | ("serviceTask" | "sendTask" | "receiveTask", "operationRef")
                                | ("sendTask" | "receiveTask", "messageRef")
                                | ("dataStoreReference", "dataStoreRef")
                                | (
                                    "dataObject"
                                        | "dataObjectReference"
                                        | "dataStore"
                                        | "dataStoreReference"
                                        | "dataInput"
                                        | "dataOutput",
                                    "itemSubjectRef"
                                )
                        )
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
                            | (B, "callActivity", "calledElement")
                            | (B, "participant", "processRef")
                            | (B, "group", "categoryValueRef")
                            | (B, "choreographyTask", "initiatingParticipantRef")
                            | (
                                B,
                                "messageFlow" | "conversationLink" | "association",
                                "sourceRef" | "targetRef"
                            )
                            | (B, "errorEventDefinition", "errorRef")
                            | (B, "escalationEventDefinition", "escalationRef")
                            | (B, "messageEventDefinition", "messageRef")
                            | (B, "signalEventDefinition", "signalRef")
                            | (B, "compensateEventDefinition", "activityRef")
                            | (BD, "BPMNPlane" | "BPMNShape" | "BPMNEdge", "bpmnElement")
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
                if item.ns == B
                    && matches!(
                        item.name.as_str(),
                        "flowNodeRef" | "participantRef" | "messageFlowRef" | "source" | "target"
                    )
                {
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
                        "inMessageRef" | "outMessageRef" | "errorRef"
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
                    parent.children.push(item);
                } else if root.replace(item).is_some() {
                    return None;
                }
            }
            Event::Text(t) => {
                if let Some(last) = stack.last_mut() {
                    if last.ns == B
                        && matches!(
                            last.name.as_str(),
                            "documentation"
                                | "flowNodeRef"
                                | "participantRef"
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
                                | "from"
                                | "to"
                                | "script"
                        )
                    {
                        last.text.push_str(t.as_ref());
                    } else if !t.trim().is_empty() {
                        return None;
                    }
                } else if !t.trim().is_empty() {
                    return None;
                }
            }
            Event::GeneralRef(value) => {
                let last = stack.last_mut()?;
                if last.ns != B
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
                            | "from"
                            | "to"
                            | "script"
                            | "text"
                    )
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
            }
            Event::CData(t) => {
                let last = stack.last_mut()?;
                if last.ns != B
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
                            | "from"
                            | "to"
                            | "script"
                            | "text"
                    )
                {
                    return None;
                }
                last.text.push_str(t.as_ref());
            }
            Event::PI(_) => return None,
            Event::Comment(_)
                if stack
                    .last()
                    .is_some_and(|e| e.ns == B && is_expression(&e.name)) =>
            {
                return None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    root
}
fn attrs(e: &Elem, allowed: &[&str]) -> bool {
    e.attrs.keys().all(|k| allowed.contains(&k.as_str()))
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
fn parse_label(e: &Elem) -> Option<Label> {
    if e.ns != BD || e.name != "BPMNLabel" || !e.attrs.is_empty() || e.children.len() != 1 {
        return None;
    }
    Some(Label {
        bounds: bounds(&e.children[0])?,
        style: None,
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
        || !e.children.is_empty()
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
    Some(Expression::Structured(ExpressionData {
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
        text: e.text.clone(),
        namespaces: e
            .namespaces
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }))
}
fn timer_details(e: &Elem) -> Option<(String, NodeDetails)> {
    let mut documentation = String::new();
    let mut seen_doc = false;
    let mut timer = None;
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        if child.name == "documentation"
            && !seen_doc
            && child.attrs.is_empty()
            && child.children.is_empty()
        {
            documentation = child.text.clone();
            seen_doc = true;
            continue;
        }
        if child.name != "timerEventDefinition"
            || timer.is_some()
            || !attrs(child, &["id"])
            || child.children.len() != 1
        {
            return None;
        }
        let expr = &child.children[0];
        let kind = match expr.name.as_str() {
            "timeDate" => TimerKind::Date,
            "timeDuration" => TimerKind::Duration,
            "timeCycle" => TimerKind::Cycle,
            _ => return None,
        };
        let id = optional_id(child)?;
        timer = Some(EventDefinition::Timer {
            id,
            kind,
            expression: parse_expression(expr, false)?,
        });
    }
    Some((
        documentation,
        NodeDetails::Event {
            definitions: vec![timer?],
            attached_to: None,
            cancel_activity: None,
        },
    ))
}
fn message_signal_details(e: &Elem) -> Option<(String, NodeDetails)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut definition = None;
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
            continue;
        }
        if definition.is_some() || !child.children.is_empty() {
            return None;
        }
        let def_id = optional_id(child)?;
        definition = Some(match child.name.as_str() {
            "errorEventDefinition" if attrs(child, &["id", "errorRef"]) => EventDefinition::Error {
                id: def_id,
                error_ref: Some(selector(child, "errorRef")?),
            },
            "escalationEventDefinition" if attrs(child, &["id", "escalationRef"]) => {
                EventDefinition::Escalation {
                    id: def_id,
                    escalation_ref: Some(selector(child, "escalationRef")?),
                }
            }
            "signalEventDefinition" if attrs(child, &["id", "signalRef"]) => {
                EventDefinition::Signal {
                    id: def_id,
                    signal_ref: Some(selector(child, "signalRef")?),
                }
            }
            "messageEventDefinition" if attrs(child, &["id", "messageRef"]) => {
                EventDefinition::Message {
                    id: def_id,
                    message_ref: Some(selector(child, "messageRef")?),
                    operation_ref: None,
                }
            }
            _ => return None,
        });
    }
    Some((
        doc,
        NodeDetails::Event {
            definitions: vec![definition?],
            attached_to: None,
            cancel_activity: None,
        },
    ))
}
fn link_cancel_compensate_details(e: &Elem) -> Option<(String, NodeDetails)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut definition = None;
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
            continue;
        }
        if definition.is_some() {
            return None;
        }
        let def_id = optional_id(child)?;
        definition = Some(match child.name.as_str() {
            "cancelEventDefinition" if attrs(child, &["id"]) && child.children.is_empty() => {
                EventDefinition::Cancel { id: def_id }
            }
            "compensateEventDefinition"
                if attrs(child, &["id", "waitForCompletion", "activityRef"])
                    && child.children.is_empty() =>
            {
                EventDefinition::Compensate {
                    id: def_id,
                    wait_for_completion: optional_bool(child.attrs.get("waitForCompletion"))?,
                    activity_ref: child
                        .attrs
                        .get("activityRef")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                }
            }
            "linkEventDefinition" if attrs(child, &["id", "name"]) => {
                let name = child.attrs.get("name")?.clone();
                let mut sources = Vec::new();
                let mut target = None;
                for reference in &child.children {
                    if reference.ns != B
                        || !reference.attrs.is_empty()
                        || !reference.children.is_empty()
                    {
                        return None;
                    }
                    match reference.name.as_str() {
                        "source" if target.is_none() => sources.push(reference.text.parse().ok()?),
                        "target" if target.is_none() => target = Some(reference.text.parse().ok()?),
                        _ => return None,
                    }
                }
                EventDefinition::Link {
                    id: def_id,
                    name,
                    sources,
                    target,
                }
            }
            _ => return None,
        });
    }
    Some((
        doc,
        NodeDetails::Event {
            definitions: vec![definition?],
            attached_to: None,
            cancel_activity: None,
        },
    ))
}
fn conditional_details(e: &Elem) -> Option<(String, NodeDetails)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut definition = None;
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
            continue;
        }
        if child.name != "conditionalEventDefinition"
            || definition.is_some()
            || !attrs(child, &["id"])
            || child.children.len() != 1
        {
            return None;
        }
        let expr = &child.children[0];
        if expr.ns != B || expr.name != "condition" {
            return None;
        }
        let id = optional_id(child)?;
        definition = Some(EventDefinition::Conditional {
            id,
            expression: parse_expression(expr, false)?,
        });
    }
    Some((
        doc,
        NodeDetails::Event {
            definitions: vec![definition?],
            attached_to: None,
            cancel_activity: None,
        },
    ))
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
fn loop_characteristics(e: &Elem) -> Option<LoopCharacteristics> {
    let boolean = |v: Option<&String>| -> Option<bool> {
        match v.map(String::as_str) {
            None | Some("false" | "0") => Some(false),
            Some("true" | "1") => Some(true),
            _ => None,
        }
    };
    let mut expressions = std::collections::BTreeMap::new();
    for child in &e.children {
        if child.ns != B
            || expressions
                .insert(child.name.as_str(), parse_expression(child, false)?)
                .is_some()
        {
            return None;
        }
    }
    match e.name.as_str() {
        "standardLoopCharacteristics"
            if attrs(e, &["testBefore", "loopMaximum"])
                && expressions.keys().all(|k| *k == "loopCondition") =>
        {
            let maximum = e.attrs.get("loopMaximum").cloned();
            if let Some(v) = &maximum {
                let digits = v.strip_prefix(['+', '-']).unwrap_or(v);
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
            }
            Some(LoopCharacteristics::Standard {
                test_before: boolean(e.attrs.get("testBefore"))?,
                condition: expressions.remove("loopCondition"),
                maximum,
            })
        }
        "multiInstanceLoopCharacteristics"
            if attrs(e, &["isSequential"])
                && expressions
                    .keys()
                    .all(|k| matches!(*k, "loopCardinality" | "completionCondition")) =>
        {
            Some(LoopCharacteristics::MultiInstance {
                is_sequential: boolean(e.attrs.get("isSequential"))?,
                cardinality: expressions.remove("loopCardinality"),
                completion_condition: expressions.remove("completionCondition"),
            })
        }
        _ => None,
    }
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
fn parse_io_specification(e: &Elem) -> Option<processcraft_core::bpmn_data::IoSpecification> {
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};
    if !attrs(e, &["id"]) {
        return None;
    }
    let mut io = IoSpecification {
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
                let port = DataPort {
                    id: optional_id(child)?,
                    name: child.attrs.get("name").cloned(),
                    item_aware: parse_item_aware(child)?,
                    is_collection: optional_bool(child.attrs.get("isCollection"))?,
                };
                if child.name == "dataInput" {
                    io.inputs.push(port);
                } else {
                    io.outputs.push(port);
                }
            }
            "inputSet" | "outputSet" if attrs(child, &["id", "name"]) => {
                let input = child.name == "inputSet";
                let mut set = IoSet {
                    id: optional_id(child)?,
                    name: child.attrs.get("name").cloned(),
                    ..Default::default()
                };
                for reference in &child.children {
                    if reference.ns != B
                        || !reference.attrs.is_empty()
                        || !reference.children.is_empty()
                    {
                        return None;
                    }
                    let value = reference.text.trim().parse().ok()?;
                    match (input, reference.name.as_str()) {
                        (true, "dataInputRefs") | (false, "dataOutputRefs") => {
                            set.data_refs.push(value)
                        }
                        (true, "optionalInputRefs") | (false, "optionalOutputRefs") => {
                            set.optional_refs.push(value)
                        }
                        (true, "whileExecutingInputRefs") | (false, "whileExecutingOutputRefs") => {
                            set.while_executing_refs.push(value)
                        }
                        (true, "outputSetRefs") | (false, "inputSetRefs") => {
                            set.counterpart_refs.push(value)
                        }
                        _ => return None,
                    }
                }
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
                    id: optional_id(child)?,
                    from: parse_expression(&child.children[0], false)?,
                    to: parse_expression(&child.children[1], false)?,
                });
            }
            _ => return None,
        }
    }
    Some(DataAssociation {
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
fn activity_properties(
    e: &Elem,
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
        if child.name == "completionCondition" && e.name == "adHocSubProcess" {
            if child.ns != B || result.completion_condition.is_some() {
                return None;
            }
            result.completion_condition = Some(parse_expression(child, false)?);
        }
        if child.name == "script" {
            if e.name != "scriptTask"
                || child.ns != B
                || !child.attrs.is_empty()
                || !child.children.is_empty()
                || result.script.is_some()
            {
                return None;
            }
            result.script = Some(child.text.clone());
        }
    }
    if result == ActivityProperties::default() {
        Some(None)
    } else {
        Some(Some(result))
    }
}
fn activity_parts(e: &Elem) -> Option<(String, Option<LoopCharacteristics>)> {
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
            lp = Some(loop_characteristics(child)?);
        } else if (child.name == "script" && e.name == "scriptTask")
            || matches!(
                child.name.as_str(),
                "ioSpecification" | "dataInputAssociation" | "dataOutputAssociation"
            )
        {
            continue;
        } else {
            return None;
        }
    }
    Some((doc, lp))
}
fn terminate_details(e: &Elem) -> Option<(String, NodeDetails)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut definition = None;
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
            continue;
        }
        if child.name != "terminateEventDefinition"
            || definition.is_some()
            || !attrs(child, &["id"])
            || !child.children.is_empty()
        {
            return None;
        }
        let id = optional_id(child)?;
        definition = Some(EventDefinition::Terminate { id });
    }
    Some((
        doc,
        NodeDetails::Event {
            definitions: vec![definition?],
            attached_to: None,
            cancel_activity: None,
        },
    ))
}
fn supported_event_details(n: &Node) -> bool {
    let Some(NodeDetails::Event {
        definitions,
        attached_to,
        cancel_activity,
    }) = &n.details
    else {
        return false;
    };
    if matches!(n.kind, NodeKind::BoundaryEvent) != attached_to.is_some() {
        return false;
    }
    if !matches!(n.kind, NodeKind::BoundaryEvent) && cancel_activity.is_some() {
        return false;
    }
    match definitions.as_slice() {
        [EventDefinition::Timer { .. }] => matches!(
            n.kind,
            NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
        ),
        [EventDefinition::Terminate { .. }] => matches!(n.kind, NodeKind::EndEvent),
        [EventDefinition::Link { .. }] => matches!(
            n.kind,
            NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
        ),
        [EventDefinition::Cancel { .. }] => {
            matches!(n.kind, NodeKind::BoundaryEvent | NodeKind::EndEvent)
        }
        [EventDefinition::Compensate { .. }] => matches!(
            n.kind,
            NodeKind::BoundaryEvent | NodeKind::IntermediateThrowEvent | NodeKind::EndEvent
        ),
        [
            EventDefinition::Error {
                error_ref: Some(_), ..
            },
        ] => matches!(n.kind, NodeKind::EndEvent | NodeKind::BoundaryEvent),
        [
            EventDefinition::Escalation {
                escalation_ref: Some(_),
                ..
            },
        ] => matches!(
            n.kind,
            NodeKind::EndEvent | NodeKind::IntermediateThrowEvent | NodeKind::BoundaryEvent
        ),
        [EventDefinition::Conditional { .. }] => matches!(
            n.kind,
            NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
        ),
        [
            EventDefinition::Signal {
                signal_ref: Some(_),
                ..
            }
            | EventDefinition::Message {
                message_ref: Some(_),
                operation_ref: None,
                ..
            },
        ] => matches!(
            n.kind,
            NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ),
        _ => false,
    }
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
        || (set.children.is_empty() && !set.attrs.contains_key("id"))
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
                    parse_lane_set(child, Some(lane_id.selector()), lanes, depth + 1)?;
                }
                _ => return None,
            }
        }
        lanes.push(Lane {
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
        is_executable: optional_bool(e.attrs.get("isExecutable"))?,
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        documentation: String::new(),
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    };
    let mut seen_documentation = false;
    let mut seen_lane_set = false;
    for n in &e.children {
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
        if n.ns == B
            && n.name == "documentation"
            && n.attrs.is_empty()
            && n.children.is_empty()
            && !seen_documentation
        {
            p.documentation = n.text.clone();
            seen_documentation = true;
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
            "dataInputAssociation" | "dataOutputAssociation"
        ) {
            continue;
        }
        if n.name == "laneSet" {
            if seen_lane_set {
                return None;
            }
            seen_lane_set = true;
            p.lane_set_id = optional_id(n)?;
            parse_lane_set(n, None, &mut p.lanes, 0)?;
            continue;
        }
        if n.name == "group" {
            if !attrs(n, &["id", "categoryValueRef"]) || !n.children.is_empty() {
                return None;
            }
            p.groups.push(processcraft_core::bpmn_model::Group {
                id: element_identity(n)?,
                category_value_ref: n
                    .attrs
                    .get("categoryValueRef")
                    .map(|v| v.parse())
                    .transpose()
                    .ok()?,
            });
            continue;
        }
        if n.name == "textAnnotation" {
            if !attrs(n, &["id", "textFormat"]) {
                return None;
            }
            let mut text = String::new();
            let mut doc = String::new();
            let mut seen_text = false;
            let mut seen_doc = false;
            for c in &n.children {
                if c.ns != B || !c.attrs.is_empty() || !c.children.is_empty() {
                    return None;
                }
                match c.name.as_str() {
                    "text" if !seen_text => {
                        text = c.text.clone();
                        seen_text = true;
                    }
                    "documentation" if !seen_doc => {
                        doc = c.text.clone();
                        seen_doc = true;
                    }
                    _ => return None,
                }
            }
            p.nodes.push(Node {
                item_aware: None,
                id: element_identity(n)?,
                name: None,
                documentation: doc,
                kind: NodeKind::TextAnnotation,
                details: Some(NodeDetails::TextAnnotation {
                    text_format: n.attrs.get("textFormat").cloned(),
                    text: seen_text.then_some(text),
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
                id: element_identity(n)?,
                name: None,
                kind: FlowKind::Association,
                source: selector(n, "sourceRef")?,
                target: selector(n, "targetRef")?,
                documentation: documentation(n)?,
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
                item_aware: parse_item_aware(n)?,
                id: element_identity(n)?,
                name: n.attrs.get("name").cloned(),
                documentation: item_documentation(n)?,
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
            let (doc, lp) = activity_parts(n)?;
            let node_id = element_identity(n)?;
            let (io, associations) = activity_io(n, &node_id.selector())?;
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                item_aware: None,
                id: node_id,
                name: n.attrs.get("name").cloned(),
                documentation: doc,
                kind: NodeKind::CallActivity,
                details: Some(NodeDetails::CallActivity {
                    called_element: processcraft_core::bpmn_model::QNameRef {
                        namespace: target_namespace.into(),
                        local: id(n, "calledElement")?,
                    },
                }),
                owner: String::new(),
                contact: String::new(),
                activity: activity_properties(n)?,
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
                Some(lp) => Some(loop_characteristics(lp)?),
                None => None,
            };
            let child = parse_container(n, depth + 1, target_namespace)?;
            let (io, associations) = activity_io(n, &child.id.selector())?;
            p.data_associations.extend(associations);
            p.nodes.push(Node {
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
                activity: activity_properties(n)?,
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
                    &["id", "name", "attachedToRef", "cancelActivity"]
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
            let (documentation, details) = if matches!(
                kind,
                NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
            ) || (matches!(kind, NodeKind::StartEvent)
                && n.children.iter().any(|c| {
                    matches!(
                        c.name.as_str(),
                        "timerEventDefinition" | "conditionalEventDefinition"
                    )
                })) {
                let (doc, mut details) = if n.children.iter().any(|c| {
                    matches!(
                        c.name.as_str(),
                        "linkEventDefinition"
                            | "cancelEventDefinition"
                            | "compensateEventDefinition"
                    )
                }) {
                    link_cancel_compensate_details(n)?
                } else if n.children.iter().any(|c| {
                    matches!(
                        c.name.as_str(),
                        "messageEventDefinition"
                            | "signalEventDefinition"
                            | "errorEventDefinition"
                            | "escalationEventDefinition"
                    )
                }) {
                    message_signal_details(n)?
                } else if n
                    .children
                    .iter()
                    .any(|c| c.name == "conditionalEventDefinition")
                {
                    conditional_details(n)?
                } else {
                    timer_details(n)?
                };
                if matches!(kind, NodeKind::BoundaryEvent) {
                    let NodeDetails::Event {
                        attached_to,
                        cancel_activity,
                        ..
                    } = &mut details
                    else {
                        return None;
                    };
                    *attached_to = Some(selector(n, "attachedToRef")?);
                    *cancel_activity = match n.attrs.get("cancelActivity").map(String::as_str) {
                        None => None,
                        Some("true" | "1") => Some(true),
                        Some("false" | "0") => Some(false),
                        _ => return None,
                    };
                }
                (doc, Some(details))
            } else if matches!(kind, NodeKind::EndEvent | NodeKind::IntermediateThrowEvent)
                && n.children.iter().any(|c| {
                    matches!(
                        c.name.as_str(),
                        "linkEventDefinition"
                            | "cancelEventDefinition"
                            | "compensateEventDefinition"
                    )
                })
            {
                let (doc, details) = link_cancel_compensate_details(n)?;
                (doc, Some(details))
            } else if matches!(
                kind,
                NodeKind::StartEvent | NodeKind::EndEvent | NodeKind::IntermediateThrowEvent
            ) && n.children.iter().any(|c| {
                matches!(
                    c.name.as_str(),
                    "messageEventDefinition"
                        | "signalEventDefinition"
                        | "errorEventDefinition"
                        | "escalationEventDefinition"
                )
            }) {
                let (doc, details) = message_signal_details(n)?;
                (doc, Some(details))
            } else if matches!(kind, NodeKind::EndEvent)
                && n.children
                    .iter()
                    .any(|c| c.name == "terminateEventDefinition")
            {
                let (doc, details) = terminate_details(n)?;
                (doc, Some(details))
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
                (activity_parts(n)?.0, None)
            } else {
                (documentation(n)?, None)
            };
            let lp = if kind.is_activity() {
                activity_parts(n)?.1
            } else {
                None
            };
            let node_id = element_identity(n)?;
            let (io, associations) = if kind.is_activity() {
                activity_io(n, &node_id.selector())?
            } else {
                (None, Vec::new())
            };
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                item_aware: None,
                id: node_id,
                name: n.attrs.get("name").cloned(),
                documentation,
                kind,
                owner: String::new(),
                contact: String::new(),
                activity: if kind.is_activity() {
                    activity_properties(n)?
                } else {
                    None
                },
                io_specification: io,
                loop_characteristics: lp,
                details,
            });
        } else if n.name == "sequenceFlow" && attrs(n, &["id", "name", "sourceRef", "targetRef"]) {
            p.flows.push(Flow {
                association_direction: None,
                id: element_identity(n)?,
                kind: FlowKind::Sequence,
                source: selector(n, "sourceRef")?,
                target: selector(n, "targetRef")?,
                name: n.attrs.get("name").cloned(),
                documentation: flow_parts(n)?.0,
                condition: flow_parts(n)?.1,
            });
        } else {
            return None;
        }
    }
    Some(p)
}
struct Scope<'a> {
    id: &'a ElementIdentity,
    nodes: &'a [Node],
    lanes: &'a [Lane],
    flows: &'a [Flow],
    groups: &'a [processcraft_core::bpmn_model::Group],
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
fn parse_conversation(
    e: &Elem,
    parent: Option<ElementSelector>,
    out: &mut Vec<processcraft_core::bpmn_collaboration::ConversationNode>,
    depth: usize,
) -> Option<()> {
    use processcraft_core::bpmn_collaboration::{ConversationKind, ConversationNode};
    if depth > 32 || !attrs(e, &["id", "name"]) {
        return None;
    }
    let kind = match e.name.as_str() {
        "conversation" => ConversationKind::Conversation,
        "subConversation" => ConversationKind::SubConversation,
        _ => return None,
    };
    let node_id = element_identity(e)?;
    let mut refs = Vec::new();
    for c in &e.children {
        if c.ns != B {
            return None;
        }
        match c.name.as_str() {
            "participantRef" if c.attrs.is_empty() && c.children.is_empty() => {
                refs.push(c.text.parse().ok()?)
            }
            "conversation" | "subConversation"
                if matches!(kind, ConversationKind::SubConversation) =>
            {
                parse_conversation(c, Some(node_id.selector()), out, depth + 1)?
            }
            _ => return None,
        }
    }
    out.push(ConversationNode {
        id: node_id,
        name: e.attrs.get("name").cloned(),
        kind,
        participant_refs: refs,
        parent,
    });
    Some(())
}
fn parse_collaboration(e: &Elem) -> Option<Collaboration> {
    use processcraft_core::bpmn_collaboration::ConversationLink;
    if !attrs(e, &["id", "name"]) {
        return None;
    }
    let mut c = Collaboration {
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "participant"
                if attrs(child, &["id", "name", "processRef"]) && child.children.is_empty() =>
            {
                c.participants.push(Participant {
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    process: child
                        .attrs
                        .get("processRef")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                })
            }
            "messageFlow" if attrs(child, &["id", "name", "sourceRef", "targetRef"]) => {
                c.flows.push(Flow {
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Message,
                    source: selector(child, "sourceRef")?,
                    target: selector(child, "targetRef")?,
                    documentation: documentation(child)?,
                    condition: None,
                })
            }
            "conversation" | "subConversation" => {
                parse_conversation(child, None, &mut c.conversation_nodes, 0)?
            }
            "conversationLink"
                if attrs(child, &["id", "name", "sourceRef", "targetRef"])
                    && child.children.is_empty() =>
            {
                c.conversation_links.push(ConversationLink {
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    source: selector(child, "sourceRef")?,
                    target: selector(child, "targetRef")?,
                })
            }
            _ => return None,
        }
    }
    Some(c)
}
fn parse_choreography(e: &Elem) -> Option<processcraft_core::bpmn_collaboration::Choreography> {
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    if !attrs(e, &["id", "name"]) {
        return None;
    }
    let mut c = Choreography {
        id: element_identity(e)?,
        name: e.attrs.get("name").cloned(),
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    };
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "participant"
                if attrs(child, &["id", "name", "processRef"]) && child.children.is_empty() =>
            {
                c.participants.push(Participant {
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    process: child
                        .attrs
                        .get("processRef")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                })
            }
            "messageFlow" if attrs(child, &["id", "name", "sourceRef", "targetRef"]) => {
                c.message_flows.push(Flow {
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Message,
                    source: selector(child, "sourceRef")?,
                    target: selector(child, "targetRef")?,
                    documentation: documentation(child)?,
                    condition: None,
                })
            }
            "choreographyTask" if attrs(child, &["id", "name", "initiatingParticipantRef"]) => {
                let mut participants = Vec::new();
                let mut messages = Vec::new();
                for reference in &child.children {
                    if reference.ns != B
                        || !reference.attrs.is_empty()
                        || !reference.children.is_empty()
                    {
                        return None;
                    }
                    match reference.name.as_str() {
                        "participantRef" => participants.push(reference.text.parse().ok()?),
                        "messageFlowRef" => messages.push(reference.text.parse().ok()?),
                        _ => return None,
                    }
                }
                if participants.len() < 2 || !(1..=2).contains(&messages.len()) {
                    return None;
                }
                c.activities.push(ChoreographyActivity {
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    initiating_participant: selector(child, "initiatingParticipantRef")?,
                    participant_refs: participants,
                    message_flow_refs: messages,
                });
            }
            "sequenceFlow" if attrs(child, &["id", "name", "sourceRef", "targetRef"]) => {
                c.sequence_flows.push(Flow {
                    association_direction: None,
                    id: element_identity(child)?,
                    name: child.attrs.get("name").cloned(),
                    kind: FlowKind::Sequence,
                    source: selector(child, "sourceRef")?,
                    target: selector(child, "targetRef")?,
                    documentation: flow_parts(child)?.0,
                    condition: flow_parts(child)?.1,
                })
            }
            _ => return None,
        }
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
            id: optional_id(op)?,
            name: op.attrs.get("name")?.clone(),
            implementation_ref: op.catalog_qnames.get("implementationRef").cloned(),
            in_message_ref: input?,
            out_message_ref: output,
            error_refs: errors,
        });
    }
    Some(Interface {
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
            id: optional_id(p)?,
            name: p.attrs.get("name").cloned(),
            type_ref: p.catalog_qnames.get("type").cloned(),
            is_required: optional_bool(p.attrs.get("isRequired"))?,
        });
    }
    Some(BpmnResource {
        id: optional_id(e)?,
        name: e.attrs.get("name")?.clone(),
        parameters,
    })
}
fn activity_ports(document: &BpmnDocument) -> Option<std::collections::HashSet<ElementSelector>> {
    Some(
        scopes(document)?
            .iter()
            .flat_map(|scope| scope.nodes)
            .filter_map(|node| node.io_specification.as_ref())
            .flat_map(|io| io.inputs.iter().chain(&io.outputs))
            .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector))
            .collect(),
    )
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
fn parse_profile(bytes: &[u8]) -> Option<BpmnDocument> {
    let root = tree(bytes)?;
    if root.ns != B
        || root.name != "definitions"
        || !attrs(
            &root,
            &[
                "id",
                "targetNamespace",
                "name",
                "expressionLanguage",
                "typeLanguage",
                "exporter",
                "exporterVersion",
            ],
        )
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
        target_namespace: root.attrs.get("targetNamespace")?.clone(),
        name: root.attrs.get("name").cloned(),
        expression_language: root.attrs.get("expressionLanguage").cloned(),
        type_language: root.attrs.get("typeLanguage").cloned(),
        exporter: root.attrs.get("exporter").cloned(),
        exporter_version: root.attrs.get("exporterVersion").cloned(),
        imports: Vec::new(),
    });
    let mut seen_non_import = false;
    for e in &root.children {
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
                        id: element_identity(child)?,
                        value: child.attrs.get("value").cloned(),
                    });
                }
                d.categories.push(processcraft_core::bpmn_model::Category {
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
                    id: optional_id(e)?,
                    name: e.attrs.get("name").cloned(),
                    capacity: e.attrs.get("capacity").cloned(),
                    is_unlimited: optional_bool(e.attrs.get("isUnlimited"))?,
                    item_aware: parse_item_aware(e)?,
                });
            }
            (B, "error" | "escalation") => {
                if !attrs(e, &["id", "name", "errorCode", "escalationCode"])
                    || !e.children.is_empty()
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
                        id: root_id,
                        name,
                        code,
                    }
                } else {
                    EventResource::Escalation {
                        id: root_id,
                        name,
                        code,
                    }
                });
            }
            (B, "signal" | "message") => {
                if !attrs(e, &["id", "name"]) || !e.children.is_empty() {
                    return None;
                }
                let root_id = element_identity(e)?;
                let name = e.attrs.get("name").cloned();
                d.event_resources.push(if e.name == "signal" {
                    EventResource::Signal { id: root_id, name }
                } else {
                    EventResource::Message { id: root_id, name }
                });
            }
            (B, "choreography") => {
                d.choreographies.push(parse_choreography(e)?);
            }
            (B, "collaboration") if d.collaboration.is_none() => {
                d.collaboration = Some(parse_collaboration(e)?);
            }
            (B, "process") => {
                if !attrs(e, &["id", "name", "isExecutable"])
                    || optional_bool(e.attrs.get("isExecutable")).is_none()
                {
                    return None;
                }
                let p = parse_container(e, 0, root.attrs.get("targetNamespace")?)?;
                d.processes.push(p);
            }
            (BD, "BPMNDiagram") => {
                if !attrs(e, &["id", "name"]) || e.children.len() != 1 {
                    return None;
                }
                let plane = &e.children[0];
                if plane.ns != BD
                    || plane.name != "BPMNPlane"
                    || !attrs(plane, &["id", "bpmnElement"])
                {
                    return None;
                }
                let mut diagram = Diagram {
                    id: element_identity(e)?,
                    name: e.attrs.get("name").cloned(),
                    plane_id: element_identity(plane)?,
                    plane: selector(plane, "bpmnElement")?,
                    shapes: vec![],
                    edges: vec![],
                };
                for item in &plane.children {
                    if item.ns != BD
                        || !attrs(
                            item,
                            if item.name == "BPMNShape" {
                                &["id", "bpmnElement", "isExpanded"]
                            } else {
                                &["id", "bpmnElement"]
                            },
                        )
                    {
                        return None;
                    }
                    match item.name.as_str() {
                        "BPMNShape" => {
                            if !(1..=2).contains(&item.children.len()) {
                                return None;
                            }
                            let label = if item.children.len() == 2 {
                                Some(parse_label(&item.children[1])?)
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
                            diagram.shapes.push(Shape {
                                id: element_identity(item)?,
                                element: selector(item, "bpmnElement")?,
                                is_expanded,
                                bounds: bounds(&item.children[0])?,
                                label,
                                style: None,
                            });
                        }
                        "BPMNEdge" => {
                            let mut points = vec![];
                            let mut label = None;
                            for pt in &item.children {
                                if pt.ns == BD && pt.name == "BPMNLabel" && label.is_none() {
                                    label = Some(parse_label(pt)?);
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
                            diagram.edges.push(Edge {
                                id: element_identity(item)?,
                                element: selector(item, "bpmnElement")?,
                                waypoints: points,
                                label,
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
            if let Some(NodeDetails::CallActivity { called_element }) = &node.details
                && (called_element.namespace != *root.attrs.get("targetNamespace")?
                    || !d
                        .processes
                        .iter()
                        .any(|process| process.id.xml_id() == Some(&called_element.local)))
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
            if let Some(NodeDetails::Event { definitions, .. }) = &node.details {
                for def in definitions {
                    let valid = match def {
                        EventDefinition::Signal { signal_ref: Some(id), .. } => d.event_resources.iter().any(|r| matches!(r, EventResource::Signal { id: other, .. } if id == other)),
                        EventDefinition::Message { message_ref: Some(id), .. } => d.event_resources.iter().any(|r| matches!(r, EventResource::Message { id: other, .. } if id == other)),
                        EventDefinition::Error { error_ref: Some(id), .. } => d.event_resources.iter().any(|r| matches!(r, EventResource::Error { id: other, .. } if id == other)),
                        EventDefinition::Escalation { escalation_ref: Some(id), .. } => d.event_resources.iter().any(|r| matches!(r, EventResource::Escalation { id: other, .. } if id == other)),
                        _ => true,
                    };
                    if !valid {
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
            if !(p.nodes.iter().any(|n| n.id == f.source)
                || (f.kind == FlowKind::Association && p.groups.iter().any(|g| g.id == f.source)))
                || !(p.nodes.iter().any(|n| n.id == f.target)
                    || (f.kind == FlowKind::Association
                        && p.groups.iter().any(|g| g.id == f.target)))
            {
                return None;
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
                    && !all_scopes
                        .iter()
                        .any(|p| p.flows.iter().any(|f| f.id == e.element))
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
            !contained
                .iter()
                .any(|p| p.flows.iter().any(|f| f.id == e.element))
        }) {
            return None;
        }
    }
    let mut validation = ProjectData::new("validation".into());
    validation.documents.push(Arc::new(d.clone()));
    validation.validate().ok()?;
    Some(d)
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
        };
        xml.push_str(&format!("<{tag}"));
        write_xml_id(xml, &node.id);
        write_xml_name(xml, node.name.as_deref());
        xml.push('>');
        for participant in &node.participant_refs {
            write_local_qname_text(
                xml,
                "participantRef",
                xml_reference(document, participant),
                namespace,
            );
        }
        write_conversations(xml, nodes, Some(&node.id.selector()), namespace, document);
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
fn write_participant(
    xml: &mut String,
    participant: &Participant,
    namespace: &str,
    document: &ExportContext,
) {
    if participant.process.is_some() {
        write_local_qname_open(xml, "participant", namespace);
    } else {
        xml.push_str("<participant");
    }
    write_xml_id(xml, &participant.id);
    write_xml_name(xml, participant.name.as_deref());
    if let Some(process) = &participant.process {
        xml.push_str(&format!(
            " processRef=\"{}\"",
            local_qname_value(xml_reference(document, process), namespace)
        ));
    }
    xml.push_str("/>");
}
fn write_message_flow(xml: &mut String, flow: &Flow, namespace: &str, document: &ExportContext) {
    let tag = write_local_qname_open(xml, "messageFlow", namespace);
    write_xml_id(xml, &flow.id);
    write_xml_name(xml, flow.name.as_deref());
    xml.push_str(&format!(
        " sourceRef=\"{}\" targetRef=\"{}\">",
        local_qname_value(xml_reference(document, &flow.source), namespace),
        local_qname_value(xml_reference(document, &flow.target), namespace)
    ));
    if namespace.is_empty() && !flow.documentation.is_empty() {
        xml.push_str(&format!(
            "<documentation xmlns=\"{B}\">{}</documentation>",
            esc(&flow.documentation)
        ));
    } else {
        write_doc(xml, &flow.documentation);
    }
    xml.push_str(&format!("</{tag}>"));
}
fn write_xml_id(xml: &mut String, identity: &ElementIdentity) {
    if let Some(id) = identity.xml_id() {
        xml.push_str(&format!(" id=\"{id}\""));
    }
}
fn write_xml_name(xml: &mut String, name: Option<&str>) {
    if let Some(name) = name {
        xml.push_str(&format!(" name=\"{}\"", esc(name)));
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
        xml.push('>');
        for member in &lane.members {
            xml.push_str(&format!(
                "<flowNodeRef>{}</flowNodeRef>",
                xml_reference(document, member)
            ));
        }
        let lane_selector = lane.id.selector();
        if lane.child_lane_set_id.is_some()
            || lanes
                .iter()
                .any(|child| child.parent.as_ref() == Some(&lane_selector))
        {
            xml.push_str("<childLaneSet");
            if let Some(id) = &lane.child_lane_set_id {
                write_xml_id(xml, id);
            }
            xml.push('>');
            write_lanes(xml, lanes, Some(&lane_selector), document);
            xml.push_str("</childLaneSet>");
        }
        xml.push_str("</lane>");
    }
}
fn write_item_attributes(
    xml: &mut String,
    metadata: Option<&processcraft_core::bpmn_data::ItemAwareMetadata>,
) {
    if let Some(q) = metadata.and_then(|m| m.item_subject_ref.as_ref()) {
        write_catalog_qname_attribute(xml, "itemSubjectRef", q, "pc_item");
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
        xml.push_str("/>");
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
    xml.push('>');
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
            write_item_attributes(xml, port.item_aware.as_ref());
            xml.push('>');
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
            xml.push('>');
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
    xml.push('>');
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
        xml.push('>');
        write_expression(xml, "from", &assignment.from, false);
        write_expression(xml, "to", &assignment.to, false);
        xml.push_str("</assignment>");
    }
    xml.push_str(&format!("</{tag}>"));
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
        write_item_attributes(xml, o.item_aware.as_ref());
        if let Some(value) = o.is_collection {
            xml.push_str(&format!(" isCollection=\"{}\"", value));
        }
        xml.push('>');
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
                Some(NodeDetails::CallActivity { .. })
                    | Some(NodeDetails::Event {
                        attached_to: Some(_),
                        ..
                    })
                    | Some(NodeDetails::DataStoreReference {
                        data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Legacy(_))
                    })
            );
        let unqualified = local_owner_qname
            || refs.iter().flatten().any(|q| q.namespace.is_empty())
            || n.item_aware
                .as_ref()
                .and_then(|m| m.item_subject_ref.as_ref())
                .is_some_and(|q| q.namespace.is_empty())
            || store_ref.is_some_and(|q| q.namespace.is_empty());
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
                    EventDefinition::Timer { expression, .. }
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
                if reference.namespace.is_empty() {
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
                            EventDefinition::Timer { expression, .. }
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
        write_item_attributes(xml, n.item_aware.as_ref());
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
                    if namespace.is_empty() {
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
                    write_catalog_qname_attribute(xml, "dataStoreRef", q, "pc_store")
                }
            },
            _ => {}
        }
        if let Some(NodeDetails::CallActivity { called_element }) = &n.details {
            if namespace.is_empty() {
                xml.push_str(&format!(" calledElement=\"{}\"", called_element.local));
            } else {
                xml.push_str(&format!(
                    " xmlns:tns=\"{}\" calledElement=\"tns:{}\"",
                    esc(namespace),
                    called_element.local
                ));
            }
        }
        xml.push('>');
        if unqualified && !n.documentation.is_empty() {
            xml.push_str(&format!(
                "<documentation xmlns=\"{B}\">{}</documentation>",
                esc(&n.documentation)
            ));
        } else {
            write_doc(xml, &n.documentation);
        }
        write_item_state(xml, n.item_aware.as_ref(), unqualified);
        if let Some(io) = &n.io_specification {
            write_io_specification(xml, io, unqualified, document);
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
                    test_before,
                    condition,
                    maximum,
                } => {
                    xml.push_str(&format!(
                        "<standardLoopCharacteristics{} testBefore=\"{}\"",
                        if unqualified {
                            format!(" xmlns=\"{B}\"")
                        } else {
                            String::new()
                        },
                        test_before
                    ));
                    if let Some(maximum) = maximum {
                        xml.push_str(&format!(" loopMaximum=\"{}\"", esc(maximum)));
                    }
                    xml.push('>');
                    if let Some(value) = condition {
                        write_expression(xml, "loopCondition", value, false);
                    }
                    xml.push_str("</standardLoopCharacteristics>");
                }
                LoopCharacteristics::MultiInstance {
                    is_sequential,
                    cardinality,
                    completion_condition,
                } => {
                    xml.push_str(&format!(
                        "<multiInstanceLoopCharacteristics{} isSequential=\"{}\">",
                        if unqualified {
                            format!(" xmlns=\"{B}\"")
                        } else {
                            String::new()
                        },
                        is_sequential
                    ));
                    if let Some(value) = cardinality {
                        write_expression(xml, "loopCardinality", value, false);
                    }
                    if let Some(value) = completion_condition {
                        write_expression(xml, "completionCondition", value, false);
                    }
                    xml.push_str("</multiInstanceLoopCharacteristics>");
                }
            }
        }
        if let Some(a) = &n.activity
            && let Some(script) = &a.script
        {
            xml.push_str(&format!("<script>{}</script>", esc(script)));
        }
        if let Some(NodeDetails::SubProcess { content }) = &n.details {
            if content.lane_set_id.is_some() || !content.lanes.is_empty() {
                xml.push_str("<laneSet");
                if let Some(id) = &content.lane_set_id {
                    write_xml_id(xml, id);
                }
                xml.push('>');
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
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details
            && let [
                EventDefinition::Timer {
                    id,
                    kind,
                    expression,
                },
            ] = definitions.as_slice()
        {
            xml.push_str("<timerEventDefinition");
            if unqualified {
                xml.push_str(&format!(" xmlns=\"{B}\""));
            }
            if let Some(id) = id {
                write_xml_id(xml, id);
            }
            let tag = match kind {
                TimerKind::Date => "timeDate",
                TimerKind::Duration => "timeDuration",
                TimerKind::Cycle => "timeCycle",
            };
            xml.push('>');
            write_expression(xml, tag, expression, false);
            xml.push_str("</timerEventDefinition>");
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details
            && let [EventDefinition::Terminate { id }] = definitions.as_slice()
        {
            xml.push_str("<terminateEventDefinition");
            if unqualified {
                xml.push_str(&format!(" xmlns=\"{B}\""));
            }
            if let Some(id) = id {
                write_xml_id(xml, id);
            }
            xml.push_str("/>");
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details {
            let referenced = match definitions.as_slice() {
                [
                    EventDefinition::Signal {
                        id,
                        signal_ref: Some(target),
                    },
                ] => Some(("signalEventDefinition", "signalRef", id, target)),
                [
                    EventDefinition::Message {
                        id,
                        message_ref: Some(target),
                        operation_ref: None,
                    },
                ] => Some(("messageEventDefinition", "messageRef", id, target)),
                [
                    EventDefinition::Error {
                        id,
                        error_ref: Some(target),
                    },
                ] => Some(("errorEventDefinition", "errorRef", id, target)),
                [
                    EventDefinition::Escalation {
                        id,
                        escalation_ref: Some(target),
                    },
                ] => Some(("escalationEventDefinition", "escalationRef", id, target)),
                _ => None,
            };
            if let Some((tag, attribute, id, target)) = referenced {
                write_local_qname_open(xml, tag, namespace);
                if let Some(id) = id {
                    write_xml_id(xml, id);
                }
                xml.push_str(&format!(
                    " {attribute}=\"{}\"/>",
                    local_qname_value(xml_reference(document, target), namespace)
                ));
            }
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details {
            match definitions.as_slice() {
                [EventDefinition::Cancel { id }] => {
                    xml.push_str("<cancelEventDefinition");
                    if unqualified {
                        xml.push_str(&format!(" xmlns=\"{B}\""));
                    }
                    if let Some(id) = id {
                        write_xml_id(xml, id);
                    }
                    xml.push_str("/>");
                }
                [
                    EventDefinition::Compensate {
                        id,
                        wait_for_completion,
                        activity_ref,
                    },
                ] => {
                    if activity_ref.is_some() {
                        write_local_qname_open(xml, "compensateEventDefinition", namespace);
                    } else {
                        xml.push_str("<compensateEventDefinition");
                        if unqualified {
                            xml.push_str(&format!(" xmlns=\"{B}\""));
                        }
                    }
                    if let Some(id) = id {
                        write_xml_id(xml, id);
                    }
                    if let Some(wait) = wait_for_completion {
                        xml.push_str(&format!(" waitForCompletion=\"{wait}\""));
                    }
                    if let Some(activity) = activity_ref {
                        xml.push_str(&format!(
                            " activityRef=\"{}\"",
                            local_qname_value(xml_reference(document, activity), namespace)
                        ));
                    }
                    xml.push_str("/>");
                }
                [
                    EventDefinition::Link {
                        id,
                        name,
                        sources,
                        target,
                    },
                ] => {
                    xml.push_str("<linkEventDefinition");
                    if unqualified {
                        xml.push_str(&format!(" xmlns=\"{B}\""));
                    }
                    xml.push_str(&format!(" name=\"{}\"", esc(name)));
                    if let Some(id) = id {
                        write_xml_id(xml, id);
                    }
                    xml.push('>');
                    for source in sources {
                        write_local_qname_text(
                            xml,
                            "source",
                            xml_reference(document, source),
                            namespace,
                        );
                    }
                    if let Some(target) = target {
                        write_local_qname_text(
                            xml,
                            "target",
                            xml_reference(document, target),
                            namespace,
                        );
                    }
                    xml.push_str("</linkEventDefinition>");
                }
                _ => {}
            }
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details
            && let [EventDefinition::Conditional { id, expression }] = definitions.as_slice()
        {
            xml.push_str("<conditionalEventDefinition");
            if unqualified {
                xml.push_str(&format!(" xmlns=\"{B}\""));
            }
            if let Some(id) = id {
                write_xml_id(xml, id);
            }
            xml.push('>');
            write_expression(xml, "condition", expression, false);
            xml.push_str("</conditionalEventDefinition>");
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
            " sourceRef=\"{}\" targetRef=\"{}\">",
            xml_reference(document, &f.source),
            xml_reference(document, &f.target)
        ));
        write_doc(xml, &f.documentation);
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
        xml.push('>');
        write_doc(xml, &n.documentation);
        let body = match &n.details {
            Some(NodeDetails::TextAnnotation {
                text: Some(text), ..
            }) => Some(text.as_str()),
            _ => n.name.as_deref(),
        };
        if let Some(body) = body {
            xml.push_str(&format!("<text>{}</text>", esc(body)));
        }
        xml.push_str("</textAnnotation>");
    }
    for group in groups {
        if group.category_value_ref.is_some() {
            write_local_qname_open(xml, "group", namespace);
        } else {
            xml.push_str("<group");
        }
        write_xml_id(xml, &group.id);
        if let Some(id) = &group.category_value_ref {
            xml.push_str(&format!(
                " categoryValueRef=\"{}\"",
                local_qname_value(xml_reference(document, id), namespace)
            ));
        }
        xml.push_str("/>");
    }
    for f in flows.iter().filter(|f| f.kind == FlowKind::Association) {
        let tag = write_local_qname_open(xml, "association", namespace);
        write_xml_id(xml, &f.id);
        xml.push_str(&format!(
            " sourceRef=\"{}\" targetRef=\"{}\"",
            local_qname_value(xml_reference(document, &f.source), namespace),
            local_qname_value(xml_reference(document, &f.target), namespace)
        ));
        if let Some(direction) = f.association_direction {
            xml.push_str(&format!(" associationDirection=\"{:?}\"", direction));
        }
        xml.push('>');
        if namespace.is_empty() && !f.documentation.is_empty() {
            xml.push_str(&format!(
                "<documentation xmlns=\"{B}\">{}</documentation>",
                esc(&f.documentation)
            ));
        } else {
            write_doc(xml, &f.documentation);
        }
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
fn missing_xml_reference(document: &ExportContext) -> Option<ElementSelector> {
    fn check(document: &ExportContext, selector: &ElementSelector) -> Option<ElementSelector> {
        (!document.xml_ids.contains_key(selector)).then(|| selector.clone())
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
        for lane in scope.lanes {
            for reference in &lane.members {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for flow in scope.flows {
            for reference in [&flow.source, &flow.target] {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for group in scope.groups {
            if let Some(reference) = &group.category_value_ref
                && let Some(missing) = check(document, reference)
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
                    attached_to,
                    ..
                }) => {
                    let mut refs: Vec<_> = attached_to.iter().collect();
                    for definition in definitions {
                        match definition {
                            EventDefinition::Message {
                                message_ref,
                                operation_ref,
                                ..
                            } => {
                                refs.extend(message_ref.iter());
                                refs.extend(operation_ref.iter());
                            }
                            EventDefinition::Signal { signal_ref, .. } => {
                                refs.extend(signal_ref.iter())
                            }
                            EventDefinition::Error { error_ref, .. } => {
                                refs.extend(error_ref.iter())
                            }
                            EventDefinition::Escalation { escalation_ref, .. } => {
                                refs.extend(escalation_ref.iter())
                            }
                            EventDefinition::Link {
                                sources, target, ..
                            } => {
                                refs.extend(sources);
                                refs.extend(target.iter());
                            }
                            EventDefinition::Compensate { activity_ref, .. } => {
                                refs.extend(activity_ref.iter())
                            }
                            _ => {}
                        }
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
                && let Some(missing) = check(document, r)
            {
                return Some(missing);
            }
        }
        for f in &collab.flows {
            for r in [&f.source, &f.target] {
                if let Some(missing) = check(document, r) {
                    return Some(missing);
                }
            }
        }
        for n in &collab.conversation_nodes {
            for r in &n.participant_refs {
                if let Some(missing) = check(document, r) {
                    return Some(missing);
                }
            }
        }
        for l in &collab.conversation_links {
            for r in [&l.source, &l.target] {
                if let Some(missing) = check(document, r) {
                    return Some(missing);
                }
            }
        }
    }
    for choreography in &document.choreographies {
        for p in &choreography.participants {
            if let Some(r) = &p.process
                && let Some(missing) = check(document, r)
            {
                return Some(missing);
            }
        }
        for f in choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
        {
            for r in [&f.source, &f.target] {
                if let Some(missing) = check(document, r) {
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
    for diagram in &document.diagrams {
        if let Some(missing) = check(document, &diagram.plane) {
            return Some(missing);
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
    if let Capability::PreservedSource { source_resource_id } = &document.capability {
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
    if !matches!(document.capability, Capability::Editable) {
        report
            .blocked
            .push("Retained extensions cannot be safely reconstructed".into());
        return Err(ExportFailure { report });
    }
    let activity_ports = activity_ports(document).unwrap_or_default();
    if document
        .diagrams
        .iter()
        .flat_map(|diagram| &diagram.shapes)
        .any(|shape| activity_ports.contains(&shape.element))
    {
        report.blocked.push("Activity data input/output ports cannot be shown in BPMN diagrams; remove their shapes before export".into());
        return Err(ExportFailure { report });
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
    let native = all_scopes
        .iter()
        .flat_map(|p| p.nodes)
        .any(|n| !n.owner.is_empty() || !n.contact.is_empty());
    if native {
        report
            .omitted
            .push("Node owner and contact are native-only fields".into());
        if !acknowledge_native_fields {
            report
                .blocked
                .push("Native-only field omission requires acknowledgement".into());
            return Err(ExportFailure { report });
        }
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
    if all_scopes
        .iter()
        .flat_map(|p| p.nodes)
        .any(|n| match &n.details {
            Some(NodeDetails::CallActivity { called_element }) => {
                called_element.namespace != namespace
                    || !document
                        .processes
                        .iter()
                        .any(|p| p.id.xml_id() == Some(&called_element.local))
            }
            _ => false,
        })
    {
        report
            .blocked
            .push("Call activity target is not a local process".into());
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
    write_catalog(&mut xml, &document.catalog);
    for category in &document.categories {
        xml.push_str("<category");
        write_xml_id(&mut xml, &category.id);
        if let Some(name) = &category.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        xml.push('>');
        for value in &category.values {
            xml.push_str("<categoryValue");
            write_xml_id(&mut xml, &value.id);
            if let Some(text) = &value.value {
                xml.push_str(&format!(" value=\"{}\"", esc(text)));
            }
            xml.push_str("/>");
        }
        xml.push_str("</category>");
    }
    for store in &document.data_stores {
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
        write_item_attributes(&mut xml, store.item_aware.as_ref());
        if let Some(capacity) = &store.capacity {
            xml.push_str(&format!(" capacity=\"{}\"", esc(capacity)));
        }
        if let Some(unlimited) = store.is_unlimited {
            xml.push_str(&format!(" isUnlimited=\"{}\"", unlimited));
        }
        xml.push('>');
        write_item_state(&mut xml, store.item_aware.as_ref(), unqualified);
        xml.push_str(&format!("</{tag}>"));
    }
    for resource in &document.event_resources {
        let (tag, id, name) = match resource {
            EventResource::Signal { id, name } => ("signal", id, name),
            EventResource::Message { id, name } => ("message", id, name),
            EventResource::Error { id, name, .. } => ("error", id, name),
            EventResource::Escalation { id, name, .. } => ("escalation", id, name),
        };
        xml.push_str(&format!("<{tag}"));
        write_xml_id(&mut xml, id);
        if let Some(name) = name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
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
        xml.push_str("/>");
    }
    for c in &document.choreographies {
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
        xml.push('>');
        for p in &c.participants {
            write_participant(&mut xml, p, &namespace, &context);
        }
        for f in &c.message_flows {
            write_message_flow(&mut xml, f, &namespace, &context);
        }
        for a in &c.activities {
            let tag = write_local_qname_open(&mut xml, "choreographyTask", &namespace);
            write_xml_id(&mut xml, &a.id);
            write_xml_name(&mut xml, a.name.as_deref());
            xml.push_str(&format!(
                " initiatingParticipantRef=\"{}\">",
                local_qname_value(
                    xml_reference(&context, &a.initiating_participant),
                    &namespace
                )
            ));
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
            xml.push_str(&format!("</{tag}>"));
        }
        for f in &c.sequence_flows {
            xml.push_str("<sequenceFlow");
            write_xml_id(&mut xml, &f.id);
            write_xml_name(&mut xml, f.name.as_deref());
            xml.push_str(&format!(
                " sourceRef=\"{}\" targetRef=\"{}\">",
                xml_reference(&context, &f.source),
                xml_reference(&context, &f.target)
            ));
            write_doc(&mut xml, &f.documentation);
            if let Some(condition) = &f.condition {
                write_expression(&mut xml, "conditionExpression", condition, true);
            }
            xml.push_str("</sequenceFlow>");
        }
        xml.push_str("</choreography>");
    }
    if let Some(c) = &document.collaboration {
        xml.push_str("<collaboration");
        write_xml_id(&mut xml, &c.id);
        write_xml_name(&mut xml, c.name.as_deref());
        xml.push('>');
        for p in &c.participants {
            write_participant(&mut xml, p, &namespace, &context);
        }
        for f in &c.flows {
            write_message_flow(&mut xml, f, &namespace, &context);
        }
        write_conversations(&mut xml, &c.conversation_nodes, None, &namespace, &context);
        for link in &c.conversation_links {
            let tag = write_local_qname_open(&mut xml, "conversationLink", &namespace);
            write_xml_id(&mut xml, &link.id);
            write_xml_name(&mut xml, link.name.as_deref());
            xml.push_str(&format!(
                " sourceRef=\"{}\" targetRef=\"{}\"/>",
                local_qname_value(xml_reference(&context, &link.source), &namespace),
                local_qname_value(xml_reference(&context, &link.target), &namespace)
            ));
            let _ = tag;
        }
        xml.push_str("</collaboration>");
    }
    for p in &document.processes {
        xml.push_str("<process");
        write_xml_id(&mut xml, &p.id);
        write_xml_name(&mut xml, p.name.as_deref());
        if let Some(value) = p.is_executable {
            xml.push_str(&format!(" isExecutable=\"{value}\""));
        }
        xml.push('>');
        write_doc(&mut xml, &p.documentation);
        if let Some(io) = &p.io_specification {
            write_io_specification(&mut xml, io, false, &context);
        }
        if p.lane_set_id.is_some() || !p.lanes.is_empty() {
            xml.push_str("<laneSet");
            if let Some(id) = &p.lane_set_id {
                write_xml_id(&mut xml, id);
            }
            xml.push('>');
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
        xml.push_str("</process>");
    }
    for d in &document.diagrams {
        let ns_binding = format!(" xmlns=\"{}\"", esc(&namespace));
        xml.push_str(&format!(
            "<bpmndi:BPMNDiagram xmlns:bpmndi=\"{BD}\" xmlns:dc=\"{DC}\" xmlns:di=\"{DI}\""
        ));
        write_xml_id(&mut xml, &d.id);
        write_xml_name(&mut xml, d.name.as_deref());
        xml.push_str("><bpmndi:BPMNPlane");
        xml.push_str(&ns_binding);
        write_xml_id(&mut xml, &d.plane_id);
        xml.push_str(&format!(
            " bpmnElement=\"{}\">",
            xml_reference(&context, &d.plane)
        ));
        for s in &d.shapes {
            let b = &s.bounds;
            xml.push_str("<bpmndi:BPMNShape");
            xml.push_str(&ns_binding);
            write_xml_id(&mut xml, &s.id);
            xml.push_str(&format!(
                " bpmnElement=\"{}\"",
                xml_reference(&context, &s.element)
            ));
            if let Some(expanded) = s.is_expanded {
                xml.push_str(&format!(" isExpanded=\"{}\"", expanded));
            }
            xml.push_str(&format!(
                "><dc:Bounds x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                b.x, b.y, b.width, b.height
            ));
            if let Some(label) = &s.label {
                write_label(&mut xml, label);
            }
            xml.push_str("</bpmndi:BPMNShape>");
        }
        for e in &d.edges {
            xml.push_str("<bpmndi:BPMNEdge");
            xml.push_str(&ns_binding);
            write_xml_id(&mut xml, &e.id);
            xml.push_str(&format!(
                " bpmnElement=\"{}\">",
                xml_reference(&context, &e.element)
            ));
            for p in &e.waypoints {
                xml.push_str(&format!("<di:waypoint x=\"{}\" y=\"{}\"/>", p.x, p.y));
            }
            if let Some(label) = &e.label {
                write_label(&mut xml, label);
            }
            xml.push_str("</bpmndi:BPMNEdge>");
        }
        xml.push_str("</bpmndi:BPMNPlane></bpmndi:BPMNDiagram>");
    }
    xml.push_str("</definitions>");
    let reparsed = parse_profile(xml.as_bytes());
    if let Some(parsed) = &reparsed {
        let expected = document.expressions();
        let actual = parsed.expressions();
        if expected.len() != actual.len()
            || expected
                .iter()
                .zip(actual.iter())
                .any(|(a, b)| match (a, b) {
                    (Expression::Structured(a), Expression::Structured(b)) => {
                        a.id.as_ref().and_then(ElementIdentity::xml_id)
                            != b.id.as_ref().and_then(ElementIdentity::xml_id)
                            || a.kind != b.kind
                            || a.text != b.text
                            || a.namespaces
                                .iter()
                                .map(|(k, v)| (k, v))
                                .collect::<BTreeMap<_, _>>()
                                != b.namespaces
                                    .iter()
                                    .map(|(k, v)| (k, v))
                                    .collect::<BTreeMap<_, _>>()
                    }
                    (Expression::Structured(_), _) => true,
                    _ => false,
                })
        {
            report.blocked.push(
                "Expression namespace context cannot represent owner QName or expression metadata"
                    .into(),
            );
            return Err(ExportFailure { report });
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
fn write_catalog_qname_text(xml: &mut String, tag: &str, q: &QNameRef) {
    if q.namespace.is_empty() {
        xml.push_str(&format!("<bpmn:{tag}>{}</bpmn:{tag}>", q.local));
    } else {
        xml.push_str(&format!(
            "<bpmn:{tag} xmlns:q=\"{}\">q:{}</bpmn:{tag}>",
            esc(&q.namespace),
            q.local
        ));
    }
}
fn write_catalog(xml: &mut String, catalog: &processcraft_core::bpmn_catalog::SupportingCatalog) {
    for item in &catalog.item_definitions {
        xml.push_str(&format!(
            "<bpmn:itemDefinition xmlns:bpmn=\"{B}\" xmlns=\"\""
        ));
        if let Some(id) = &item.id {
            write_xml_id(xml, id);
        }
        if let Some(q) = &item.structure_ref {
            write_catalog_qname_attribute(xml, "structureRef", q, "q");
        }
        if let Some(value) = item.is_collection {
            xml.push_str(&format!(" isCollection=\"{value}\""));
        }
        if let Some(kind) = item.item_kind {
            xml.push_str(&format!(" itemKind=\"{kind:?}\""));
        }
        xml.push_str("/>");
    }
    for interface in &catalog.interfaces {
        xml.push_str(&format!(
            "<bpmn:interface xmlns:bpmn=\"{B}\" xmlns=\"\" name=\"{}\"",
            esc(&interface.name)
        ));
        if let Some(id) = &interface.id {
            write_xml_id(xml, id);
        }
        if let Some(q) = &interface.implementation_ref {
            write_catalog_qname_attribute(xml, "implementationRef", q, "q");
        }
        xml.push('>');
        for op in &interface.operations {
            xml.push_str(&format!("<bpmn:operation name=\"{}\"", esc(&op.name)));
            if let Some(id) = &op.id {
                write_xml_id(xml, id);
            }
            if let Some(q) = &op.implementation_ref {
                write_catalog_qname_attribute(xml, "implementationRef", q, "r");
            }
            xml.push('>');
            write_catalog_qname_text(xml, "inMessageRef", &op.in_message_ref);
            if let Some(q) = &op.out_message_ref {
                write_catalog_qname_text(xml, "outMessageRef", q);
            }
            for q in &op.error_refs {
                write_catalog_qname_text(xml, "errorRef", q);
            }
            xml.push_str("</bpmn:operation>");
        }
        xml.push_str("</bpmn:interface>");
    }
    for resource in &catalog.resources {
        xml.push_str(&format!(
            "<bpmn:resource xmlns:bpmn=\"{B}\" xmlns=\"\" name=\"{}\"",
            esc(&resource.name)
        ));
        if let Some(id) = &resource.id {
            write_xml_id(xml, id);
        }
        xml.push('>');
        for parameter in &resource.parameters {
            xml.push_str("<bpmn:resourceParameter");
            if let Some(id) = &parameter.id {
                write_xml_id(xml, id);
            }
            if let Some(name) = &parameter.name {
                xml.push_str(&format!(" name=\"{}\"", esc(name)));
            }
            if let Some(q) = &parameter.type_ref {
                write_catalog_qname_attribute(xml, "type", q, "q");
            }
            if let Some(value) = parameter.is_required {
                xml.push_str(&format!(" isRequired=\"{value}\""));
            }
            xml.push_str("/>");
        }
        xml.push_str("</bpmn:resource>");
    }
}
fn write_expression(xml: &mut String, tag: &str, expression: &Expression, legacy_formal: bool) {
    let Expression::Structured(data) = expression else {
        let xsi = if legacy_formal {
            " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" xsi:type=\"bpmn:tFormalExpression\""
        } else {
            ""
        };
        xml.push_str(&format!("<{tag}{xsi}>{}</{tag}>", esc(expression.text())));
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
    xml.push_str(&format!(">{}</{tag_name}>", esc(&data.text)));
}
fn write_label(xml: &mut String, label: &Label) {
    let b = &label.bounds;
    xml.push_str(&format!("<bpmndi:BPMNLabel><dc:Bounds x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></bpmndi:BPMNLabel>", b.x,b.y,b.width,b.height));
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
fn write_doc(xml: &mut String, text: &str) {
    if !text.is_empty() {
        xml.push_str("<documentation>");
        xml.push_str(&esc(text));
        xml.push_str("</documentation>");
    }
}

fn flow_parts(e: &Elem) -> Option<(String, Option<Expression>)> {
    let mut documentation = String::new();
    let mut condition = None;
    let mut seen_doc = false;
    for c in &e.children {
        if c.ns != B || !c.children.is_empty() {
            return None;
        }
        match c.name.as_str() {
            "documentation" if !seen_doc && c.attrs.is_empty() => {
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
