//! Parsers for event definitions, activity parts, data and I/O, and lane sets.
use crate::interchange::import::metadata::{
    parse_expression, take_anonymous_base, take_flow_metadata,
};
use crate::interchange::import::tree::{
    Elem, attrs, element_identity, mixed_body, optional_bool, optional_id, selector,
};
use crate::interchange::refusal::refused;
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{
    EventDefinition, EventGatewayType, EventIo, EventProperty, EventReference, GatewayDirection,
    LoopCharacteristics, NodeDetails, QNameRef, TimerKind,
};
use processcraft_core::ids::ElementSelector;
use processcraft_core::model::*;
pub(super) fn event_reference(qname: &QNameRef, target_namespace: &str) -> EventReference {
    if qname.namespace == target_namespace {
        EventReference::Local(qname.local.clone().into())
    } else {
        EventReference::Qualified(qname.clone())
    }
}
pub(super) fn parse_correlation_key(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationKey> {
    use processcraft_core::bpmn_activity::CorrelationKey;
    if e.ns != BPMN || e.name != "correlationKey" || !attrs(e, &["id", "name"]) {
        return None;
    }
    let mut refs = Vec::new();
    for child in &e.children {
        if child.ns != BPMN
            || child.name != "correlationPropertyRef"
            || !attrs(child, &[])
            || !child.children.is_empty()
        {
            return refused(|| {
                "<correlationKey> child other than a plain <correlationPropertyRef>".into()
            });
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
pub(super) fn parse_correlation_property(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationProperty> {
    use processcraft_core::bpmn_activity::{CorrelationProperty, CorrelationRetrieval};
    if e.ns != BPMN
        || e.name != "correlationProperty"
        || !attrs(e, &["id", "name", "type"])
        || e.children.is_empty()
    {
        return refused(|| {
            "<correlationProperty> with unsupported attributes or without retrieval expressions"
                .into()
        });
    }
    let mut retrieval_expressions = Vec::new();
    for child in &e.children {
        if child.ns != BPMN
            || child.name != "correlationPropertyRetrievalExpression"
            || !attrs(child, &["id", "messageRef"])
            || child.children.len() != 1
            || child.children[0].name != "messagePath"
        {
            return refused(|| {
                "<correlationPropertyRetrievalExpression> without exactly one <messagePath>".into()
            });
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
pub(super) fn parse_correlation_subscription(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::CorrelationSubscription> {
    use processcraft_core::bpmn_activity::{CorrelationPropertyBinding, CorrelationSubscription};
    if e.ns != BPMN
        || e.name != "correlationSubscription"
        || !attrs(e, &["id", "correlationKeyRef"])
    {
        return refused(|| "<correlationSubscription> with unsupported attributes".into());
    }
    let mut bindings = Vec::new();
    for child in &e.children {
        if child.ns != BPMN
            || child.name != "correlationPropertyBinding"
            || !attrs(child, &["id", "correlationPropertyRef"])
            || child.children.len() != 1
            || child.children[0].name != "dataPath"
        {
            return refused(|| {
                "<correlationPropertyBinding> without exactly one <dataPath>".into()
            });
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
pub(super) fn parse_audit_monitor(
    e: &Elem,
) -> Option<processcraft_core::bpmn_activity::AuditMonitor> {
    if e.ns != BPMN
        || !matches!(e.name.as_str(), "auditing" | "monitoring")
        || !attrs(e, &["id"])
        || !e.children.is_empty()
    {
        return refused(|| {
            "<auditing>/<monitoring> with unsupported attributes or children".into()
        });
    }
    Some(processcraft_core::bpmn_activity::BaseOwner {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
    })
}
pub(super) fn parse_property(e: &Elem) -> Option<processcraft_core::bpmn_model::EventProperty> {
    if e.ns != BPMN || e.name != "property" || !attrs(e, &["id", "name", "itemSubjectRef"]) {
        return refused(|| "<property> with unsupported attributes".into());
    }
    Some(processcraft_core::bpmn_model::EventProperty {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
        name: e.attrs.get("name").cloned(),
        item_aware: parse_item_aware(e)?,
    })
}
pub(super) fn parse_io_binding(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::IoBinding> {
    if e.ns != BPMN
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
    if e.ns != BPMN || e.name != "rendering" || !attrs(e, &["id"]) || !e.children.is_empty() {
        return None;
    }
    Some(processcraft_core::bpmn_activity::BaseOwner {
        id: optional_id(e)?,
        base: take_anonymous_base(e),
    })
}
pub(super) fn parse_resource_role(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::ResourceRole> {
    use processcraft_core::bpmn_activity::{
        ResourceAssignmentExpression, ResourceParameterBinding, ResourceRole,
        ResourceRoleAssignment, ResourceRoleKind,
    };
    if e.ns != BPMN || !attrs(e, &["id", "name"]) {
        return refused(|| "resource role with unsupported attributes".into());
    }
    let kind = match e.name.as_str() {
        "resourceRole" => ResourceRoleKind::ResourceRole,
        "performer" => ResourceRoleKind::Performer,
        "humanPerformer" => ResourceRoleKind::HumanPerformer,
        "potentialOwner" => ResourceRoleKind::PotentialOwner,
        _ => return refused(|| "unsupported resource role element".into()),
    };
    let assignment = if e.children.is_empty() {
        ResourceRoleAssignment::None
    } else if e.children[0].ns == BPMN
        && e.children[0].name == "resourceRef"
        && attrs(&e.children[0], &[])
        && e.children[0].children.is_empty()
    {
        let reference = event_reference(e.children[0].catalog_text_qname.as_ref()?, namespace);
        let mut bindings = Vec::new();
        for child in &e.children[1..] {
            if child.ns != BPMN
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
        && e.children[0].ns == BPMN
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
pub(super) fn parse_global_task(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_activity::GlobalTask> {
    use processcraft_core::bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind};
    if e.ns != BPMN
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
        if child.ns != BPMN {
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
pub(super) fn parse_event_definition(e: &Elem, target_namespace: &str) -> Option<EventDefinition> {
    if e.ns != BPMN {
        return refused(|| "event definition outside the BPMN namespace".into());
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
                if child.ns != BPMN
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
                    _ => {
                        return refused(|| {
                            "<timerEventDefinition> child other than timeDate, timeDuration or timeCycle".into()
                        });
                    }
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
                if child.ns != BPMN || !child.attrs.is_empty() || !child.children.is_empty() {
                    return None;
                }
                let reference =
                    event_reference(child.catalog_text_qname.as_ref()?, target_namespace);
                match child.name.as_str() {
                    "source" if target.is_none() => sources.push(reference),
                    "target" if target.is_none() => target = Some(reference),
                    _ => {
                        return refused(|| {
                            "<linkEventDefinition> child other than <source>, or <source> after <target>".into()
                        });
                    }
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
pub(super) fn gateway_details(e: &Elem) -> Option<(String, Option<NodeDetails>)> {
    let direction = match e.attrs.get("gatewayDirection").map(String::as_str) {
        None => None,
        Some("Unspecified") => Some(GatewayDirection::Unspecified),
        Some("Converging") => Some(GatewayDirection::Converging),
        Some("Diverging") => Some(GatewayDirection::Diverging),
        Some("Mixed") => Some(GatewayDirection::Mixed),
        _ => return refused(|| "unsupported gatewayDirection value".into()),
    };
    let event_gateway_type = match e.attrs.get("eventGatewayType").map(String::as_str) {
        None => None,
        Some("Exclusive") => Some(EventGatewayType::Exclusive),
        Some("Parallel") => Some(EventGatewayType::Parallel),
        _ => return refused(|| "unsupported eventGatewayType value".into()),
    };
    let default_flow = e.attrs.get("default").map(|v| v.parse()).transpose().ok()?;
    let instantiate = optional_bool(e.attrs.get("instantiate"))?;
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut activation_condition = None;
    for child in &e.children {
        if child.ns != BPMN {
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
    if e.ns != BPMN || e.name != "dataInputAssociation" || !attrs(e, &["id"]) {
        return None;
    }
    let mut sources = Vec::new();
    let mut target = None;
    let mut transformation = None;
    let mut assignments = Vec::new();
    for child in &e.children {
        if child.ns != BPMN {
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
    if e.ns != BPMN || e.name != "event" || !attrs(e, &["id", "name"]) {
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
        if child.ns != BPMN {
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
pub(super) fn loop_characteristics(e: &Elem, namespace: &str) -> Option<LoopCharacteristics> {
    use processcraft_core::bpmn_activity::{ComplexBehaviorDefinition, MultiInstanceFlowCondition};
    if e.name == "standardLoopCharacteristics" {
        if !attrs(e, &["id", "testBefore", "loopMaximum"]) || e.children.len() > 1 {
            return refused(|| {
                "<standardLoopCharacteristics> with unsupported attributes or more than one child"
                    .into()
            });
        }
        let condition = if let Some(child) = e.children.first() {
            if child.ns != BPMN || child.name != "loopCondition" {
                return refused(|| {
                    "<standardLoopCharacteristics> child other than <loopCondition>".into()
                });
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
        _ => return refused(|| "unsupported multi-instance behavior value".into()),
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
        if child.ns != BPMN {
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
                    && child.children[0].ns == BPMN
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
pub(super) fn parse_item_aware(
    e: &Elem,
) -> Option<Option<processcraft_core::bpmn_data::ItemAwareMetadata>> {
    use processcraft_core::bpmn_data::{DataState, ItemAwareMetadata};
    let mut state = None;
    for child in &e.children {
        if child.ns == BPMN && child.name == "dataState" {
            if state.is_some() || !attrs(child, &["id", "name"]) || !child.children.is_empty() {
                return None;
            }
            state = Some(DataState {
                base: take_anonymous_base(child),
                id: optional_id(child)?,
                name: child.attrs.get("name").cloned(),
            });
        } else if child.ns != BPMN || child.name != "documentation" {
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
pub(super) fn item_documentation(e: &Elem) -> Option<String> {
    let mut doc = None;
    for child in &e.children {
        if child.ns == BPMN && child.name == "dataState" {
            continue;
        }
        if child.ns != BPMN
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
    if e.ns != BPMN
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
    if child.ns != BPMN || !attrs(child, &["id", "name"]) {
        return None;
    }
    let mut set = processcraft_core::bpmn_data::IoSet {
        base: take_anonymous_base(child),
        id: optional_id(child)?,
        name: child.attrs.get("name").cloned(),
        ..Default::default()
    };
    for reference in &child.children {
        if reference.ns != BPMN || !reference.attrs.is_empty() || !reference.children.is_empty() {
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
pub(super) fn parse_io_specification(
    e: &Elem,
) -> Option<processcraft_core::bpmn_data::IoSpecification> {
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
        if child.ns != BPMN {
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
        if child.ns != BPMN {
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
pub(super) fn activity_io(
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
    // bpmn.io never writes an ioSpecification; its associations target the activity's
    // `__targetRef_placeholder` property or carry no source at all.
    Some((io, associations))
}
pub(super) fn parse_event(
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
        if child.ns != BPMN {
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
        Some(event_reference(
            e.catalog_qnames.get("attachedToRef")?,
            target_namespace,
        ))
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
pub(super) fn activity_properties(
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
            if child.ns != BPMN || result.completion_condition.is_some() {
                return None;
            }
            result.completion_condition = Some(parse_expression(child, false)?);
        }
        if child.name == "script" {
            if e.name != "scriptTask" || child.ns != BPMN || result.script.is_some() {
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
pub(super) fn activity_parts(
    e: &Elem,
    namespace: &str,
) -> Option<(String, Option<LoopCharacteristics>)> {
    let mut doc = String::new();
    let mut seen_doc = false;
    let mut lp = None;
    for child in &e.children {
        if child.ns != BPMN {
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
pub(in crate::interchange) fn supported_event_details(node: &Node) -> bool {
    matches!(node.details, Some(NodeDetails::Event { .. }))
}
pub(super) fn parse_lane_set(
    set: &Elem,
    parent: Option<ElementSelector>,
    lanes: &mut Vec<Lane>,
    depth: usize,
    target_namespace: &str,
) -> Option<()> {
    if depth > 32
        || set.ns != BPMN
        || !matches!(set.name.as_str(), "laneSet" | "childLaneSet")
        || !attrs(set, &["id", "name"])
    {
        return None;
    }
    for lane in &set.children {
        if lane.ns != BPMN
            || lane.name != "lane"
            || !attrs(lane, &["id", "name", "partitionElementRef"])
        {
            return None;
        }
        let lane_id = element_identity(lane)?;
        let mut members = Vec::new();
        let mut child_set = None;
        let mut child_set_base = None;
        let mut child_set_name = None;
        let mut partition_element = None;
        let mut seen_child_set = false;
        for child in &lane.children {
            if child.ns != BPMN {
                return None;
            }
            match child.name.as_str() {
                // tLane sequence: partitionElement? precedes flowNodeRef* and childLaneSet?.
                "partitionElement"
                    if partition_element.is_none()
                        && members.is_empty()
                        && !seen_child_set
                        && attrs(child, &["id", "xsi:type", "value"])
                        && child.attrs.get("xsi:type").map(String::as_str)
                            == Some("tCategoryValue")
                        && child.children.is_empty() =>
                {
                    partition_element = Some(processcraft_core::model::LanePartitionElement {
                        id: element_identity(child)?,
                        value: child.attrs.get("value").cloned(),
                    })
                }
                "flowNodeRef"
                    if child.attrs.is_empty() && child.children.is_empty() && !seen_child_set =>
                {
                    members.push(child.text.parse().ok()?)
                }
                "childLaneSet" if !seen_child_set => {
                    seen_child_set = true;
                    child_set = optional_id(child)?;
                    child_set_name = child.attrs.get("name").cloned();
                    child_set_base = take_anonymous_base(child);
                    if child_set_base.is_none()
                        && child_set.is_none()
                        && child_set_name.is_none()
                        && child.children.is_empty()
                    {
                        child_set_base = Some(Default::default());
                    }
                    parse_lane_set(
                        child,
                        Some(lane_id.selector()),
                        lanes,
                        depth + 1,
                        target_namespace,
                    )?;
                }
                _ => return None,
            }
        }
        lanes.push(Lane {
            child_lane_set_base: child_set_base,
            child_lane_set_name: child_set_name,
            partition_element,
            partition_element_ref: lane
                .catalog_qnames
                .get("partitionElementRef")
                .map(|q| event_reference(q, target_namespace)),
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
