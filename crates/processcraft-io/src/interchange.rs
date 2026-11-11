//! Conservative interchange: validate first, then qualify a small lossless profile.
use crate::bpmn::{ImportError, import_bpmn};
use processcraft_core::{
    bpmn_model::{
        EventDefinition, EventGatewayType, EventResource, GatewayDirection, LoopCharacteristics,
        NodeDetails, TimerKind,
    },
    ids::XmlId,
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
                // An expression can interpret unprefixed names using the default namespace.
                // Canonical export uses the BPMN namespace as default, so reject any
                // other expression context rather than silently changing its meaning.
                if item.ns == B
                    && matches!(
                        item.name.as_str(),
                        "conditionExpression"
                            | "condition"
                            | "timeDate"
                            | "timeDuration"
                            | "timeCycle"
                            | "loopCondition"
                            | "loopCardinality"
                            | "completionCondition"
                            | "activationCondition"
                            | "script"
                            | "text"
                    )
                {
                    let (default, _) = r.resolver().resolve_element(QName("context"));
                    if !matches!(default, ResolveResult::Bound(uri) if uri.0 == B) {
                        return None;
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
                            || item.name != "conditionExpression"
                            || uri.0 != "http://www.w3.org/2001/XMLSchema-instance"
                            || key != "type"
                        {
                            return None;
                        }
                        let value = a
                            .normalized_value(XmlVersion::Implicit1_0)
                            .ok()?
                            .into_owned();
                        if !value.contains(':') {
                            return None;
                        }
                        let (resolved, name) = r.resolver().resolve_element(QName(&value));
                        if !matches!(resolved, ResolveResult::Bound(uri) if uri.0==B)
                            || name.as_ref() != "tFormalExpression"
                        {
                            return None;
                        }
                        item.attrs
                            .insert("xsi:type".into(), "tFormalExpression".into());
                        continue;
                    } else if !matches!(ns, ResolveResult::Unbound) {
                        return None;
                    }
                    let mut value = a
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned();
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
                            | (B, "dataStoreReference", "dataStoreRef")
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
                            | (BD, "BPMNPlane" | "BPMNShape" | "BPMNEdge", "bpmnElement")
                    ) {
                        // Only a prefixed QName bound to the definitions target namespace is unambiguous.
                        if item.name == "sequenceFlow" && !value.contains(':') {
                            item.attrs.insert(key.into(), value);
                            continue;
                        }
                        if !value.contains(':') {
                            return None;
                        }
                        let (resolved, local) = r.resolver().resolve_element(QName(&value));
                        match resolved {
                            ResolveResult::Bound(n) if Some(n.0) == target_namespace.as_deref() => {
                                value = local.as_ref().to_string()
                            }
                            _ => return None,
                        }
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
                        "flowNodeRef" | "participantRef" | "messageFlowRef"
                    )
                {
                    let q = item.text.trim();
                    if q.contains(':') {
                        let (ns, local) = r.resolver().resolve_element(QName(q));
                        if !matches!(ns, ResolveResult::Bound(n) if Some(n.0)==target_namespace.as_deref())
                        {
                            return None;
                        }
                        item.text = local.as_ref().into();
                    } else if item.name == "flowNodeRef" {
                        item.text = q.to_owned();
                    } else {
                        return None;
                    }
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
                                | "messageFlowRef"
                                | "conditionExpression"
                                | "condition"
                                | "timeDate"
                                | "timeDuration"
                                | "timeCycle"
                                | "loopCondition"
                                | "loopCardinality"
                                | "completionCondition"
                                | "activationCondition"
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
                            | "script"
                            | "text"
                    )
                {
                    return None;
                }
                last.text.push_str(t.as_ref());
            }
            Event::PI(_) => return None,
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
fn value(e: &Elem, key: &str) -> String {
    e.attrs.get(key).cloned().unwrap_or_default()
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
        if expr.ns != B
            || !expr.attrs.is_empty()
            || !expr.children.is_empty()
            || expr.text.is_empty()
        {
            return None;
        }
        let kind = match expr.name.as_str() {
            "timeDate" => TimerKind::Date,
            "timeDuration" => TimerKind::Duration,
            "timeCycle" => TimerKind::Cycle,
            _ => return None,
        };
        let id = child.attrs.get("id").map(|v| v.parse()).transpose().ok()?;
        timer = Some(EventDefinition::Timer {
            id,
            kind,
            expression: expr.text.clone(),
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
        let def_id = child.attrs.get("id").map(|v| v.parse()).transpose().ok()?;
        definition = Some(match child.name.as_str() {
            "errorEventDefinition" if attrs(child, &["id", "errorRef"]) => EventDefinition::Error {
                id: def_id,
                error_ref: Some(id(child, "errorRef")?),
            },
            "escalationEventDefinition" if attrs(child, &["id", "escalationRef"]) => {
                EventDefinition::Escalation {
                    id: def_id,
                    escalation_ref: Some(id(child, "escalationRef")?),
                }
            }
            "signalEventDefinition" if attrs(child, &["id", "signalRef"]) => {
                EventDefinition::Signal {
                    id: def_id,
                    signal_ref: Some(id(child, "signalRef")?),
                }
            }
            "messageEventDefinition" if attrs(child, &["id", "messageRef"]) => {
                EventDefinition::Message {
                    id: def_id,
                    message_ref: Some(id(child, "messageRef")?),
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
        if expr.ns != B
            || expr.name != "condition"
            || !expr.attrs.is_empty()
            || !expr.children.is_empty()
            || expr.text.is_empty()
        {
            return None;
        }
        let id = child.attrs.get("id").map(|v| v.parse()).transpose().ok()?;
        definition = Some(EventDefinition::Conditional {
            id,
            expression: expr.text.clone(),
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
        if child.ns != B || !child.attrs.is_empty() || !child.children.is_empty() {
            return None;
        }
        match child.name.as_str() {
            "documentation" if !seen_doc => {
                doc = child.text.clone();
                seen_doc = true;
            }
            "activationCondition" if activation_condition.is_none() && !child.text.is_empty() => {
                activation_condition = Some(child.text.clone())
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
            || !child.attrs.is_empty()
            || !child.children.is_empty()
            || child.text.is_empty()
            || expressions
                .insert(child.name.as_str(), child.text.clone())
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
fn parse_io_specification(e: &Elem) -> Option<processcraft_core::bpmn_data::IoSpecification> {
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};
    if !attrs(e, &["id"]) {
        return None;
    }
    let mut io = IoSpecification {
        id: e.attrs.get("id").map(|v| v.parse()).transpose().ok()?,
        ..Default::default()
    };
    for child in &e.children {
        if child.ns != B {
            return None;
        }
        match child.name.as_str() {
            "dataInput" | "dataOutput"
                if attrs(child, &["id", "name"]) && child.children.is_empty() =>
            {
                let port = DataPort {
                    id: id(child, "id")?,
                    name: value(child, "name"),
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
                    id: child.attrs.get("id").map(|v| v.parse()).transpose().ok()?,
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
    owner: &XmlId,
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
    for child in &e.children {
        if child.ns != B || !child.attrs.is_empty() || !child.children.is_empty() {
            return None;
        }
        match child.name.as_str() {
            "sourceRef" if target.is_none() => sources.push(child.text.trim().parse().ok()?),
            "targetRef" if target.is_none() => target = Some(child.text.trim().parse().ok()?),
            _ => return None,
        }
    }
    Some(DataAssociation {
        id: id(e, "id")?,
        owner: owner.clone(),
        kind,
        sources,
        target: target?,
    })
}
fn activity_io(
    e: &Elem,
) -> Option<(
    Option<processcraft_core::bpmn_data::IoSpecification>,
    Vec<processcraft_core::bpmn_data::DataAssociation>,
)> {
    let mut io = None;
    let mut associations = Vec::new();
    let owner = id(e, "id")?;
    for child in &e.children {
        match child.name.as_str() {
            "ioSpecification" if io.is_none() => io = Some(parse_io_specification(child)?),
            "dataInputAssociation" | "dataOutputAssociation" => {
                associations.push(parse_data_association(child, &owner)?)
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
        instantiate: optional_bool(e.attrs.get("instantiate"))?,
        script_format: e.attrs.get("scriptFormat").cloned(),
        ..Default::default()
    };
    for child in &e.children {
        if child.name == "completionCondition" && e.name == "adHocSubProcess" {
            if child.ns != B
                || !child.attrs.is_empty()
                || !child.children.is_empty()
                || child.text.is_empty()
                || result.completion_condition.is_some()
            {
                return None;
            }
            result.completion_condition = Some(child.text.clone());
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
        let id = child.attrs.get("id").map(|v| v.parse()).transpose().ok()?;
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
    parent: Option<XmlId>,
    lanes: &mut Vec<Lane>,
    depth: usize,
) -> Option<()> {
    if depth > 32
        || set.ns != B
        || !matches!(set.name.as_str(), "laneSet" | "childLaneSet")
        || !attrs(set, &["id"])
        || id(set, "id").is_none()
    {
        return None;
    }
    for lane in &set.children {
        if lane.ns != B || lane.name != "lane" || !attrs(lane, &["id", "name"]) {
            return None;
        }
        let lane_id = id(lane, "id")?;
        let mut members = Vec::new();
        let mut child_set = None;
        for child in &lane.children {
            if child.ns != B {
                return None;
            }
            match child.name.as_str() {
                "flowNodeRef"
                    if child.attrs.is_empty()
                        && child.children.is_empty()
                        && child_set.is_none() =>
                {
                    members.push(child.text.parse().ok()?)
                }
                "childLaneSet" if child_set.is_none() => {
                    child_set = Some(id(child, "id")?);
                    parse_lane_set(child, Some(lane_id.clone()), lanes, depth + 1)?;
                }
                _ => return None,
            }
        }
        lanes.push(Lane {
            child_lane_set_id: child_set,
            id: lane_id,
            name: value(lane, "name"),
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
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id(e, "id")?,
        name: value(e, "name"),
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
        if n.name == "laneSet" {
            if seen_lane_set {
                return None;
            }
            seen_lane_set = true;
            p.lane_set_id = Some(id(n, "id")?);
            parse_lane_set(n, None, &mut p.lanes, 0)?;
            continue;
        }
        if n.name == "group" {
            if !attrs(n, &["id", "categoryValueRef"]) || !n.children.is_empty() {
                return None;
            }
            p.groups.push(processcraft_core::bpmn_model::Group {
                id: id(n, "id")?,
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
                id: id(n, "id")?,
                name: text,
                documentation: doc,
                kind: NodeKind::TextAnnotation,
                details: Some(NodeDetails::TextAnnotation {
                    text_format: n.attrs.get("textFormat").cloned(),
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
                id: id(n, "id")?,
                name: String::new(),
                kind: FlowKind::Association,
                source: id(n, "sourceRef")?,
                target: id(n, "targetRef")?,
                documentation: documentation(n)?,
                condition: None,
                association_direction: direction,
            });
            continue;
        }
        if n.name == "dataObject" {
            if !attrs(n, &["id", "name", "isCollection"]) || !n.children.is_empty() {
                return None;
            }
            p.data_objects
                .push(processcraft_core::bpmn_data::DataObject {
                    id: id(n, "id")?,
                    name: value(n, "name"),
                    is_collection: optional_bool(n.attrs.get("isCollection"))?,
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
                    &["id", "name", "dataObjectRef"]
                } else {
                    &["id", "name", "dataStoreRef"]
                },
            ) {
                return None;
            }
            let reference = n
                .attrs
                .get(if object {
                    "dataObjectRef"
                } else {
                    "dataStoreRef"
                })
                .map(|v| v.parse())
                .transpose()
                .ok()?;
            p.nodes.push(Node {
                id: id(n, "id")?,
                name: value(n, "name"),
                documentation: documentation(n)?,
                kind: if object {
                    NodeKind::DataObjectReference
                } else {
                    NodeKind::DataStoreReference
                },
                details: Some(if object {
                    NodeDetails::DataObjectReference {
                        data_object_ref: reference,
                    }
                } else {
                    NodeDetails::DataStoreReference {
                        data_store_ref: reference,
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
            let (io, associations) = activity_io(n)?;
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                id: id(n, "id")?,
                name: value(n, "name"),
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
            p.nodes.push(Node {
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
                io_specification: None,
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
                    *attached_to = Some(id(n, "attachedToRef")?);
                    *cancel_activity = match n.attrs.get("cancelActivity").map(String::as_str) {
                        None => None,
                        Some("true" | "1") => Some(true),
                        Some("false" | "0") => Some(false),
                        _ => return None,
                    };
                }
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
            let (io, associations) = if kind.is_activity() {
                activity_io(n)?
            } else {
                (None, Vec::new())
            };
            p.data_associations.extend(associations);
            p.nodes.push(Node {
                id: id(n, "id")?,
                name: value(n, "name"),
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
                id: id(n, "id")?,
                kind: FlowKind::Sequence,
                source: id(n, "sourceRef")?,
                target: id(n, "targetRef")?,
                name: value(n, "name"),
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
    id: &'a XmlId,
    nodes: &'a [Node],
    lanes: &'a [Lane],
    flows: &'a [Flow],
    groups: &'a [processcraft_core::bpmn_model::Group],
    lane_set_id: Option<&'a XmlId>,
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
                    lane_set_id: p.lane_set_id.as_ref(),
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
                        lane_set_id: content.lane_set_id.as_ref(),
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
    parent: Option<XmlId>,
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
    let node_id = id(e, "id")?;
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
                parse_conversation(c, Some(node_id.clone()), out, depth + 1)?
            }
            _ => return None,
        }
    }
    out.push(ConversationNode {
        id: node_id,
        name: value(e, "name"),
        kind,
        participant_refs: refs,
        parent,
    });
    Some(())
}
fn parse_collaboration(e: &Elem) -> Option<Collaboration> {
    use processcraft_core::bpmn_collaboration::ConversationLink;
    if !attrs(e, &["id"]) {
        return None;
    }
    let mut c = Collaboration {
        id: id(e, "id")?,
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
                    kind: FlowKind::Message,
                    source: id(child, "sourceRef")?,
                    target: id(child, "targetRef")?,
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
                    source: id(child, "sourceRef")?,
                    target: id(child, "targetRef")?,
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
        id: id(e, "id")?,
        name: value(e, "name"),
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
                    kind: FlowKind::Message,
                    source: id(child, "sourceRef")?,
                    target: id(child, "targetRef")?,
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
                    id: id(child, "id")?,
                    name: value(child, "name"),
                    initiating_participant: id(child, "initiatingParticipantRef")?,
                    participant_refs: participants,
                    message_flow_refs: messages,
                });
            }
            "sequenceFlow" if attrs(child, &["id", "name", "sourceRef", "targetRef"]) => {
                c.sequence_flows.push(Flow {
                    association_direction: None,
                    id: id(child, "id")?,
                    name: value(child, "name"),
                    kind: FlowKind::Sequence,
                    source: id(child, "sourceRef")?,
                    target: id(child, "targetRef")?,
                    documentation: flow_parts(child)?.0,
                    condition: flow_parts(child)?.1,
                })
            }
            _ => return None,
        }
    }
    Some(c)
}
fn parse_profile(bytes: &[u8]) -> Option<BpmnDocument> {
    let root = tree(bytes)?;
    if root.ns != B
        || root.name != "definitions"
        || !attrs(&root, &["id", "targetNamespace"])
        || !root.attrs.contains_key("targetNamespace")
    {
        return None;
    }
    let mut d = BpmnDocument::new(String::new(), id(&root, "id")?);
    for e in &root.children {
        match (e.ns.as_str(), e.name.as_str()) {
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
                        id: id(child, "id")?,
                        value: child.attrs.get("value").cloned(),
                    });
                }
                d.categories.push(processcraft_core::bpmn_model::Category {
                    id: id(e, "id")?,
                    name: e.attrs.get("name").cloned(),
                    values,
                });
            }
            (B, "dataStore") => {
                if !attrs(e, &["id", "name", "capacity", "isUnlimited"]) || !e.children.is_empty() {
                    return None;
                }
                d.data_stores.push(processcraft_core::bpmn_data::DataStore {
                    id: id(e, "id")?,
                    name: value(e, "name"),
                    capacity: e.attrs.get("capacity").cloned(),
                    is_unlimited: optional_bool(e.attrs.get("isUnlimited"))?,
                });
            }
            (B, "error" | "escalation") => {
                if !attrs(e, &["id", "name", "errorCode", "escalationCode"])
                    || !e.children.is_empty()
                {
                    return None;
                }
                let root_id = id(e, "id")?;
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
                let root_id = id(e, "id")?;
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
                    || e.attrs.get("isExecutable").is_some_and(|v| v != "false")
                {
                    return None;
                }
                let p = parse_container(e, 0, root.attrs.get("targetNamespace")?)?;
                d.processes.push(p);
            }
            (BD, "BPMNDiagram") => {
                if !attrs(e, &["id"]) || e.children.len() != 1 {
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
                    id: id(e, "id")?,
                    plane_id: id(plane, "id")?,
                    plane: id(plane, "bpmnElement")?,
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
                                id: id(item, "id")?,
                                element: id(item, "bpmnElement")?,
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
                                id: id(item, "id")?,
                                element: id(item, "bpmnElement")?,
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
    if d.processes.is_empty() && d.collaboration.is_none() && d.choreographies.is_empty() {
        return None;
    }
    // Every flow and lane reference is validated in its owning scope.
    let all_scopes = scopes(&d)?;
    for p in &all_scopes {
        for node in p.nodes {
            if let Some(NodeDetails::CallActivity { called_element }) = &node.details
                && (called_element.namespace != *root.attrs.get("targetNamespace")?
                    || !d
                        .processes
                        .iter()
                        .any(|process| process.id == called_element.local))
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
    for dia in &d.diagrams {
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
            !contained.iter().any(|p| {
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
fn write_conversations(
    xml: &mut String,
    nodes: &[processcraft_core::bpmn_collaboration::ConversationNode],
    parent: Option<&XmlId>,
) {
    for node in nodes.iter().filter(|n| n.parent.as_ref() == parent) {
        let tag = match node.kind {
            processcraft_core::bpmn_collaboration::ConversationKind::Conversation => "conversation",
            processcraft_core::bpmn_collaboration::ConversationKind::SubConversation => {
                "subConversation"
            }
        };
        xml.push_str(&format!(
            "<{tag} id=\"{}\" name=\"{}\">",
            node.id,
            esc(&node.name)
        ));
        for participant in &node.participant_refs {
            xml.push_str(&format!(
                "<participantRef>tns:{}</participantRef>",
                participant
            ));
        }
        write_conversations(xml, nodes, Some(&node.id));
        xml.push_str(&format!("</{tag}>"));
    }
}
fn allocate_lane_sets(
    document: &BpmnDocument,
    scopes: &[Scope<'_>],
    resources: &[Resource],
) -> Option<BTreeMap<XmlId, XmlId>> {
    let mut used = std::collections::BTreeSet::new();
    let mut values = vec![serde_json::to_value(document).ok()?];
    while let Some(value) = values.pop() {
        match value {
            serde_json::Value::String(text) => {
                used.insert(text);
            }
            serde_json::Value::Array(items) => values.extend(items),
            serde_json::Value::Object(items) => values.extend(items.into_values()),
            _ => {}
        }
    }
    let source = document
        .provenance
        .as_ref()
        .and_then(|p| resources.iter().find(|r| r.id == p.source_resource_id))
        .and_then(|r| tree(&r.bytes));
    let mut result = BTreeMap::new();
    let mut serial = 0usize;
    for scope in scopes {
        if scope.lanes.is_empty() && scope.lane_set_id.is_none() {
            continue;
        }
        let chosen = if let Some(id) = scope.lane_set_id {
            id.clone()
        } else {
            let legacy = source
                .as_ref()
                .and_then(|root| {
                    root.children.iter().find(|e| {
                        e.ns == B && e.name == "process" && id(e, "id").as_ref() == Some(scope.id)
                    })
                })
                .and_then(|e| e.children.iter().find(|e| e.ns == B && e.name == "laneSet"))
                .and_then(|e| id(e, "id"));
            if let Some(id) = legacy.filter(|id| !used.contains(&id.to_string())) {
                used.insert(id.to_string());
                id
            } else {
                loop {
                    if serial > 100_000 {
                        return None;
                    }
                    let candidate = format!("laneSetAuto{serial}");
                    serial += 1;
                    if used.insert(candidate.clone()) {
                        break candidate.parse().ok()?;
                    }
                }
            }
        };
        result.insert(scope.id.clone(), chosen);
        for lane in scope.lanes {
            if lane.child_lane_set_id.is_none()
                && !scope
                    .lanes
                    .iter()
                    .any(|child| child.parent.as_ref() == Some(&lane.id))
            {
                continue;
            }
            let chosen = if let Some(id) = &lane.child_lane_set_id {
                id.clone()
            } else {
                loop {
                    if serial > 100_000 {
                        return None;
                    }
                    let candidate = format!("laneSetAuto{serial}");
                    serial += 1;
                    if used.insert(candidate.clone()) {
                        break candidate.parse().ok()?;
                    }
                }
            };
            result.insert(lane.id.clone(), chosen);
        }
    }
    Some(result)
}
fn write_lanes(
    xml: &mut String,
    lanes: &[Lane],
    parent: Option<&XmlId>,
    lane_set_ids: &BTreeMap<XmlId, XmlId>,
) {
    for lane in lanes.iter().filter(|lane| lane.parent.as_ref() == parent) {
        xml.push_str(&format!(
            "<lane id=\"{}\" name=\"{}\">",
            lane.id,
            esc(&lane.name)
        ));
        for member in &lane.members {
            xml.push_str(&format!("<flowNodeRef>{}</flowNodeRef>", member));
        }
        if let Some(id) = lane_set_ids.get(&lane.id) {
            xml.push_str(&format!("<childLaneSet id=\"{}\">", id));
            write_lanes(xml, lanes, Some(&lane.id), lane_set_ids);
            xml.push_str("</childLaneSet>");
        }
        xml.push_str("</lane>");
    }
}
fn write_data_association(xml: &mut String, a: &processcraft_core::bpmn_data::DataAssociation) {
    let tag = match a.kind {
        processcraft_core::bpmn_data::DataAssociationKind::Input => "dataInputAssociation",
        processcraft_core::bpmn_data::DataAssociationKind::Output => "dataOutputAssociation",
    };
    xml.push_str(&format!("<{tag} id=\"{}\">", a.id));
    for source in &a.sources {
        xml.push_str(&format!("<sourceRef>{source}</sourceRef>"));
    }
    xml.push_str(&format!("<targetRef>{}</targetRef></{tag}>", a.target));
}
fn write_scope(
    xml: &mut String,
    nodes: &[Node],
    flows: &[Flow],
    objects: &[processcraft_core::bpmn_data::DataObject],
    associations: &[processcraft_core::bpmn_data::DataAssociation],
    groups: &[processcraft_core::bpmn_model::Group],
    lane_set_ids: &BTreeMap<XmlId, XmlId>,
) {
    for o in objects {
        xml.push_str(&format!(
            "<dataObject id=\"{}\" name=\"{}\"",
            o.id,
            esc(&o.name)
        ));
        if let Some(value) = o.is_collection {
            xml.push_str(&format!(" isCollection=\"{}\"", value));
        }
        xml.push_str("/>");
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
        xml.push_str(&format!("<{tag} id=\"{}\"", n.id));
        if !matches!(n.kind, NodeKind::TextAnnotation) {
            xml.push_str(&format!(" name=\"{}\"", esc(&n.name)));
        }
        if let Some(NodeDetails::Event {
            attached_to: Some(id),
            cancel_activity,
            ..
        }) = &n.details
        {
            xml.push_str(&format!(" attachedToRef=\"tns:{}\"", id));
            if let Some(cancel) = cancel_activity {
                xml.push_str(&format!(" cancelActivity=\"{}\"", cancel));
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
                xml.push_str(&format!(" default=\"{}\"", v));
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
                xml.push_str(&format!(" default=\"{}\"", id));
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
        }) = &n.details
        {
            xml.push_str(&format!(" textFormat=\"{}\"", esc(format)));
        }
        match &n.details {
            Some(NodeDetails::DataObjectReference {
                data_object_ref: Some(id),
            }) => xml.push_str(&format!(" dataObjectRef=\"{}\"", id)),
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(id),
            }) => xml.push_str(&format!(" dataStoreRef=\"tns:{}\"", id)),
            _ => {}
        }
        if let Some(NodeDetails::CallActivity { called_element }) = &n.details {
            xml.push_str(&format!(" calledElement=\"tns:{}\"", called_element.local));
        }
        xml.push('>');
        write_doc(xml, &n.documentation);
        if matches!(n.kind, NodeKind::TextAnnotation) {
            xml.push_str(&format!("<text>{}</text>", esc(&n.name)));
        }
        if let Some(io) = &n.io_specification {
            xml.push_str("<ioSpecification");
            if let Some(id) = &io.id {
                xml.push_str(&format!(" id=\"{}\"", id));
            }
            xml.push('>');
            for (tag, ports) in [("dataInput", &io.inputs), ("dataOutput", &io.outputs)] {
                for port in ports {
                    xml.push_str(&format!(
                        "<{tag} id=\"{}\" name=\"{}\"/>",
                        port.id,
                        esc(&port.name)
                    ));
                }
            }
            for (input, sets) in [(true, &io.input_sets), (false, &io.output_sets)] {
                let tag = if input { "inputSet" } else { "outputSet" };
                for set in sets {
                    xml.push_str(&format!("<{tag}"));
                    if let Some(id) = &set.id {
                        xml.push_str(&format!(" id=\"{}\"", id));
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
                            xml.push_str(&format!("<{name}>{value}</{name}>"));
                        }
                    }
                    xml.push_str(&format!("</{tag}>"));
                }
            }
            xml.push_str("</ioSpecification>");
        }
        for association in associations.iter().filter(|a| {
            a.owner == n.id
                && matches!(
                    a.kind,
                    processcraft_core::bpmn_data::DataAssociationKind::Input
                )
        }) {
            write_data_association(xml, association);
        }
        for association in associations.iter().filter(|a| {
            a.owner == n.id
                && matches!(
                    a.kind,
                    processcraft_core::bpmn_data::DataAssociationKind::Output
                )
        }) {
            write_data_association(xml, association);
        }
        if let Some(NodeDetails::Gateway {
            activation_condition: Some(condition),
            ..
        }) = &n.details
        {
            xml.push_str(&format!(
                "<activationCondition>{}</activationCondition>",
                esc(condition)
            ));
        }
        if let Some(lp) = &n.loop_characteristics {
            match lp {
                LoopCharacteristics::Standard {
                    test_before,
                    condition,
                    maximum,
                } => {
                    xml.push_str(&format!(
                        "<standardLoopCharacteristics testBefore=\"{}\"",
                        test_before
                    ));
                    if let Some(maximum) = maximum {
                        xml.push_str(&format!(" loopMaximum=\"{}\"", esc(maximum)));
                    }
                    xml.push('>');
                    if let Some(value) = condition {
                        xml.push_str(&format!("<loopCondition>{}</loopCondition>", esc(value)));
                    }
                    xml.push_str("</standardLoopCharacteristics>");
                }
                LoopCharacteristics::MultiInstance {
                    is_sequential,
                    cardinality,
                    completion_condition,
                } => {
                    xml.push_str(&format!(
                        "<multiInstanceLoopCharacteristics isSequential=\"{}\">",
                        is_sequential
                    ));
                    if let Some(value) = cardinality {
                        xml.push_str(&format!(
                            "<loopCardinality>{}</loopCardinality>",
                            esc(value)
                        ));
                    }
                    if let Some(value) = completion_condition {
                        xml.push_str(&format!(
                            "<completionCondition>{}</completionCondition>",
                            esc(value)
                        ));
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
            if let Some(id) = lane_set_ids.get(&n.id) {
                xml.push_str(&format!("<laneSet id=\"{}\">", id));
                write_lanes(xml, &content.lanes, None, lane_set_ids);
                xml.push_str("</laneSet>");
            }
            write_scope(
                xml,
                &content.nodes,
                &content.flows,
                &content.data_objects,
                &content.data_associations,
                &content.groups,
                lane_set_ids,
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
            if let Some(id) = id {
                xml.push_str(&format!(" id=\"{}\"", esc(&id.to_string())));
            }
            let tag = match kind {
                TimerKind::Date => "timeDate",
                TimerKind::Duration => "timeDuration",
                TimerKind::Cycle => "timeCycle",
            };
            xml.push_str(&format!(
                "><{tag}>{}</{tag}></timerEventDefinition>",
                esc(expression)
            ));
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details
            && let [EventDefinition::Terminate { id }] = definitions.as_slice()
        {
            xml.push_str("<terminateEventDefinition");
            if let Some(id) = id {
                xml.push_str(&format!(" id=\"{}\"", esc(&id.to_string())));
            }
            xml.push_str("/>");
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details {
            match definitions.as_slice() {
                [
                    EventDefinition::Signal {
                        id,
                        signal_ref: Some(target),
                    },
                ] => {
                    xml.push_str("<signalEventDefinition");
                    if let Some(id) = id {
                        xml.push_str(&format!(" id=\"{}\"", id));
                    }
                    xml.push_str(&format!(" signalRef=\"tns:{}\"/>", target));
                }
                [
                    EventDefinition::Message {
                        id,
                        message_ref: Some(target),
                        operation_ref: None,
                    },
                ] => {
                    xml.push_str("<messageEventDefinition");
                    if let Some(id) = id {
                        xml.push_str(&format!(" id=\"{}\"", id));
                    }
                    xml.push_str(&format!(" messageRef=\"tns:{}\"/>", target));
                }
                [
                    EventDefinition::Error {
                        id,
                        error_ref: Some(target),
                    },
                ] => {
                    xml.push_str("<errorEventDefinition");
                    if let Some(id) = id {
                        xml.push_str(&format!(" id=\"{}\"", id));
                    }
                    xml.push_str(&format!(" errorRef=\"tns:{}\"/>", target));
                }
                [
                    EventDefinition::Escalation {
                        id,
                        escalation_ref: Some(target),
                    },
                ] => {
                    xml.push_str("<escalationEventDefinition");
                    if let Some(id) = id {
                        xml.push_str(&format!(" id=\"{}\"", id));
                    }
                    xml.push_str(&format!(" escalationRef=\"tns:{}\"/>", target));
                }
                _ => {}
            }
        }
        if let Some(NodeDetails::Event { definitions, .. }) = &n.details
            && let [EventDefinition::Conditional { id, expression }] = definitions.as_slice()
        {
            xml.push_str("<conditionalEventDefinition");
            if let Some(id) = id {
                xml.push_str(&format!(" id=\"{}\"", id));
            }
            xml.push_str(&format!(
                "><condition>{}</condition></conditionalEventDefinition>",
                esc(expression)
            ));
        }
        if let Some(a) = &n.activity
            && let Some(value) = &a.completion_condition
        {
            xml.push_str(&format!(
                "<completionCondition>{}</completionCondition>",
                esc(value)
            ));
        }
        xml.push_str(&format!("</{tag}>"));
    }
    for f in flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
        xml.push_str(&format!(
            "<sequenceFlow id=\"{}\" name=\"{}\" sourceRef=\"{}\" targetRef=\"{}\">",
            f.id,
            esc(&f.name),
            f.source,
            f.target
        ));
        write_doc(xml, &f.documentation);
        if let Some(condition) = &f.condition {
            xml.push_str("<conditionExpression xsi:type=\"bpmn:tFormalExpression\">");
            xml.push_str(&esc(condition));
            xml.push_str("</conditionExpression>");
        }
        xml.push_str("</sequenceFlow>");
    }
    for n in nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::TextAnnotation))
    {
        xml.push_str(&format!("<textAnnotation id=\"{}\"", n.id));
        if let Some(NodeDetails::TextAnnotation {
            text_format: Some(format),
        }) = &n.details
        {
            xml.push_str(&format!(" textFormat=\"{}\"", esc(format)));
        }
        xml.push('>');
        write_doc(xml, &n.documentation);
        xml.push_str(&format!("<text>{}</text></textAnnotation>", esc(&n.name)));
    }
    for group in groups {
        xml.push_str(&format!("<group id=\"{}\"", group.id));
        if let Some(id) = &group.category_value_ref {
            xml.push_str(&format!(" categoryValueRef=\"tns:{}\"", id));
        }
        xml.push_str("/>");
    }
    for f in flows.iter().filter(|f| f.kind == FlowKind::Association) {
        xml.push_str(&format!(
            "<association id=\"{}\" sourceRef=\"tns:{}\" targetRef=\"tns:{}\"",
            f.id, f.source, f.target
        ));
        if let Some(direction) = f.association_direction {
            xml.push_str(&format!(" associationDirection=\"{:?}\"", direction));
        }
        xml.push('>');
        write_doc(xml, &f.documentation);
        xml.push_str("</association>");
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
    let unsupported = all_scopes.iter().any(|p| {
        p.nodes.iter().any(|n| {
            n.activity
                .as_ref()
                .is_some_and(|a| a.message_ref.is_some() || a.operation_ref.is_some())
        }) || p.nodes.iter().any(|n| {
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
                    && (!f.name.is_empty() || f.condition.is_some()))
                || f.condition.as_ref().is_some_and(|c| c.contains(':'))
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
    let namespace = match (namespace, &document.provenance) {
        (Some(ns), _) if !ns.is_empty() => ns,
        (None, None) => "urn:processcraft:generated".into(),
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
                        .any(|p| p.id == called_element.local)
            }
            _ => false,
        })
    {
        report
            .blocked
            .push("Call activity target is not a local process".into());
        return Err(ExportFailure { report });
    }
    let Some(lane_set_ids) = allocate_lane_sets(document, &all_scopes, resources) else {
        report
            .blocked
            .push("Unable to allocate unique lane set IDs".into());
        return Err(ExportFailure { report });
    };
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><definitions xmlns=\"{B}\" xmlns:bpmndi=\"{BD}\" xmlns:dc=\"{DC}\" xmlns:di=\"{DI}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"{B}\" xmlns:tns=\"{}\" id=\"{}\" targetNamespace=\"{}\">",
        esc(&namespace),
        document.definitions_id,
        esc(&namespace)
    );
    for category in &document.categories {
        xml.push_str(&format!("<category id=\"{}\"", category.id));
        if let Some(name) = &category.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        xml.push('>');
        for value in &category.values {
            xml.push_str(&format!("<categoryValue id=\"{}\"", value.id));
            if let Some(text) = &value.value {
                xml.push_str(&format!(" value=\"{}\"", esc(text)));
            }
            xml.push_str("/>");
        }
        xml.push_str("</category>");
    }
    for store in &document.data_stores {
        xml.push_str(&format!(
            "<dataStore id=\"{}\" name=\"{}\"",
            store.id,
            esc(&store.name)
        ));
        if let Some(capacity) = &store.capacity {
            xml.push_str(&format!(" capacity=\"{}\"", esc(capacity)));
        }
        if let Some(unlimited) = store.is_unlimited {
            xml.push_str(&format!(" isUnlimited=\"{}\"", unlimited));
        }
        xml.push_str("/>");
    }
    for resource in &document.event_resources {
        let (tag, id, name) = match resource {
            EventResource::Signal { id, name } => ("signal", id, name),
            EventResource::Message { id, name } => ("message", id, name),
            EventResource::Error { id, name, .. } => ("error", id, name),
            EventResource::Escalation { id, name, .. } => ("escalation", id, name),
        };
        xml.push_str(&format!("<{tag} id=\"{}\"", id));
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
        xml.push_str(&format!(
            "<choreography id=\"{}\" name=\"{}\">",
            c.id,
            esc(&c.name)
        ));
        for p in &c.participants {
            xml.push_str(&format!(
                "<participant id=\"{}\" name=\"{}\"",
                p.id,
                esc(&p.name)
            ));
            if let Some(process) = &p.process {
                xml.push_str(&format!(" processRef=\"tns:{}\"", process));
            }
            xml.push_str("/>");
        }
        for f in &c.message_flows {
            xml.push_str(&format!(
                "<messageFlow id=\"{}\" name=\"{}\" sourceRef=\"tns:{}\" targetRef=\"tns:{}\">",
                f.id,
                esc(&f.name),
                f.source,
                f.target
            ));
            write_doc(&mut xml, &f.documentation);
            xml.push_str("</messageFlow>");
        }
        for a in &c.activities {
            xml.push_str(&format!(
                "<choreographyTask id=\"{}\" name=\"{}\" initiatingParticipantRef=\"tns:{}\">",
                a.id,
                esc(&a.name),
                a.initiating_participant
            ));
            for participant in &a.participant_refs {
                xml.push_str(&format!(
                    "<participantRef>tns:{}</participantRef>",
                    participant
                ));
            }
            for flow in &a.message_flow_refs {
                xml.push_str(&format!("<messageFlowRef>tns:{}</messageFlowRef>", flow));
            }
            xml.push_str("</choreographyTask>");
        }
        for f in &c.sequence_flows {
            xml.push_str(&format!(
                "<sequenceFlow id=\"{}\" name=\"{}\" sourceRef=\"{}\" targetRef=\"{}\">",
                f.id,
                esc(&f.name),
                f.source,
                f.target
            ));
            write_doc(&mut xml, &f.documentation);
            if let Some(condition) = &f.condition {
                xml.push_str("<conditionExpression xsi:type=\"bpmn:tFormalExpression\">");
                xml.push_str(&esc(condition));
                xml.push_str("</conditionExpression>");
            }
            xml.push_str("</sequenceFlow>");
        }
        xml.push_str("</choreography>");
    }
    if let Some(c) = &document.collaboration {
        xml.push_str(&format!("<collaboration id=\"{}\">", c.id));
        for p in &c.participants {
            xml.push_str(&format!(
                "<participant id=\"{}\" name=\"{}\"",
                p.id,
                esc(&p.name)
            ));
            if let Some(process) = &p.process {
                xml.push_str(&format!(" processRef=\"tns:{}\"", process));
            }
            xml.push_str("/>");
        }
        for f in &c.flows {
            xml.push_str(&format!(
                "<messageFlow id=\"{}\" name=\"{}\" sourceRef=\"tns:{}\" targetRef=\"tns:{}\">",
                f.id,
                esc(&f.name),
                f.source,
                f.target
            ));
            write_doc(&mut xml, &f.documentation);
            xml.push_str("</messageFlow>");
        }
        write_conversations(&mut xml, &c.conversation_nodes, None);
        for l in &c.conversation_links {
            xml.push_str(&format!("<conversationLink id=\"{}\" name=\"{}\" sourceRef=\"tns:{}\" targetRef=\"tns:{}\"/>", l.id, esc(&l.name), l.source, l.target));
        }
        xml.push_str("</collaboration>");
    }
    for p in &document.processes {
        xml.push_str(&format!(
            "<process id=\"{}\" name=\"{}\" isExecutable=\"false\">",
            p.id,
            esc(&p.name)
        ));
        write_doc(&mut xml, &p.documentation);
        if let Some(lane_set_id) = lane_set_ids.get(&p.id) {
            xml.push_str(&format!("<laneSet id=\"{}\">", lane_set_id));
            write_lanes(&mut xml, &p.lanes, None, &lane_set_ids);
            xml.push_str("</laneSet>");
        }
        write_scope(
            &mut xml,
            &p.nodes,
            &p.flows,
            &p.data_objects,
            &p.data_associations,
            &p.groups,
            &lane_set_ids,
        );
        xml.push_str("</process>");
    }
    for d in &document.diagrams {
        xml.push_str(&format!(
            "<bpmndi:BPMNDiagram id=\"{}\"><bpmndi:BPMNPlane id=\"{}\" bpmnElement=\"tns:{}\">",
            d.id, d.plane_id, d.plane
        ));
        for s in &d.shapes {
            let b = &s.bounds;
            xml.push_str(&format!(
                "<bpmndi:BPMNShape id=\"{}\" bpmnElement=\"tns:{}\"",
                s.id, s.element
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
            xml.push_str(&format!(
                "<bpmndi:BPMNEdge id=\"{}\" bpmnElement=\"tns:{}\">",
                e.id, e.element
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
    if import_bpmn(xml.as_bytes()).is_err() || parse_profile(xml.as_bytes()).is_none() {
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

fn flow_parts(e: &Elem) -> Option<(String, Option<String>)> {
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
            "conditionExpression"
                if condition.is_none()
                    && c.attrs.len() == 1
                    && c.attrs
                        .get("xsi:type")
                        .is_some_and(|v| v == "tFormalExpression") =>
            {
                if c.text.contains(':') {
                    return None;
                }
                condition = Some(c.text.clone());
            }
            _ => return None,
        }
    }
    Some((documentation, condition))
}
