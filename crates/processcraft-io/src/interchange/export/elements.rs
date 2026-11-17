//! Writers for event definitions, activity parts, data, I/O, lanes and shared owner attributes.
use crate::interchange::export::ExportContext;
use crate::interchange::export::collaboration::{
    event_reference_qname, local_qname_value, write_event_qname_text, write_local_qname_open,
};
use crate::interchange::export::xml::{
    esc, metadata_context, write_catalog_qname_attribute, write_context_qname_attribute,
    write_expression, write_extension_elements, write_mixed_element,
    write_structured_documentation,
};
use crate::interchange::shared::ANONYMOUS_CONTEXT_MARKER;
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{
    EventDefinition, EventProperty, EventReference, Expression, TimerKind,
};
use processcraft_core::ids::{ElementIdentity, ElementSelector, XmlId};
use processcraft_core::model::*;
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
        .then(|| find_prefix(BPMN).unwrap_or_else(|| "pc_corr_owner".into()));
    let name = owner_prefix
        .as_ref()
        .map(|prefix| format!("{prefix}:{tag}"))
        .unwrap_or_else(|| tag.to_owned());
    xml.push('<');
    xml.push_str(&name);
    if let Some(prefix) = &owner_prefix {
        xml.push_str(&format!(" xmlns:{prefix}=\"{BPMN}\" xmlns=\"\""));
    }
    if q.namespace.is_empty() || q.namespace == BPMN {
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
pub(super) fn write_audit_monitor(
    xml: &mut String,
    tag: &str,
    value: &processcraft_core::bpmn_activity::AuditMonitor,
) {
    xml.push_str(&format!("<{tag} xmlns=\"{BPMN}\""));
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    xml.push_str(&format!("</{tag}>"));
}
pub(super) fn write_flow_metadata(
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
pub(super) fn write_correlation_key(
    xml: &mut String,
    value: &processcraft_core::bpmn_activity::CorrelationKey,
    namespace: &str,
    context: &ExportContext,
) {
    xml.push_str(&format!("<correlationKey xmlns=\"{BPMN}\""));
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
pub(super) fn write_correlation_property(
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
pub(super) fn write_correlation_subscription(
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
pub(super) fn write_io_binding(
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
pub(super) fn write_rendering(
    xml: &mut String,
    rendering: &processcraft_core::bpmn_activity::BaseOwner,
) {
    xml.push_str(&format!("<rendering xmlns=\"{BPMN}\""));
    if let Some(id) = &rendering.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, rendering.id.as_ref(), rendering.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, rendering.id.as_ref(), rendering.base.as_ref());
    xml.push_str("</rendering>");
}
pub(super) fn write_resource_role(
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
    xml.push_str(&format!("<{tag} xmlns=\"{BPMN}\""));
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
pub(super) fn write_global_task(
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
        xml.push_str(&format!(" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"{BPMN}\" xsi:type=\"bpmn:tCallableElement\""));
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
pub(super) fn write_event_definition(
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
            xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_xml_id(xml: &mut String, identity: &ElementIdentity) {
    if let Some(id) = identity.xml_id() {
        xml.push_str(&format!(" id=\"{id}\""));
    }
}
pub(super) fn anonymous_base_attributes(
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
pub(super) fn anonymous_base_children(
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
pub(super) fn finish_anonymous_base_empty(
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
pub(super) fn anonymous_extras_attributes(
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
pub(super) fn write_xml_name(xml: &mut String, name: Option<&str>) {
    if let Some(name) = name {
        xml.push_str(&format!(" name=\"{}\"", esc(name)));
    }
}
pub(super) fn local_flow_xml_reference<'a>(
    document: &'a ExportContext<'a>,
    reference: Option<&EventReference>,
) -> String {
    match reference {
        Some(EventReference::Local(selector)) => xml_reference(document, selector).to_string(),
        _ => String::new(),
    }
}
pub(super) fn xml_reference<'a>(
    document: &'a ExportContext<'a>,
    selector: &ElementSelector,
) -> &'a XmlId {
    document
        .xml_ids
        .get(selector)
        .copied()
        .expect("reference preflight")
}
pub(super) fn write_lanes(
    xml: &mut String,
    lanes: &[Lane],
    parent: Option<&ElementSelector>,
    document: &ExportContext,
    namespace: &str,
) {
    for lane in lanes.iter().filter(|lane| lane.parent.as_ref() == parent) {
        xml.push_str("<lane");
        write_xml_id(xml, &lane.id);
        write_xml_name(xml, lane.name.as_deref());
        if let Some(reference) = &lane.partition_element_ref {
            let q = event_reference_qname(reference, namespace, document);
            match lane.base.as_ref().filter(|b| !b.namespaces.is_empty()) {
                Some(base) => {
                    write_context_qname_attribute(xml, "partitionElementRef", &q, &base.namespaces)
                }
                None => write_catalog_qname_attribute(xml, "partitionElementRef", &q, "pc_part"),
            }
        }
        anonymous_base_attributes(xml, Some(&lane.id), lane.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&lane.id), lane.base.as_ref());
        if let Some(partition) = &lane.partition_element {
            xml.push_str(&format!(
                "<partitionElement xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:pc_bpmn=\"{BPMN}\" xsi:type=\"pc_bpmn:tCategoryValue\""
            ));
            write_xml_id(xml, &partition.id);
            if let Some(value) = &partition.value {
                xml.push_str(&format!(" value=\"{}\"", esc(value)));
            }
            xml.push_str("/>");
        }
        for member in &lane.members {
            xml.push_str(&format!(
                "<flowNodeRef>{}</flowNodeRef>",
                xml_reference(document, member)
            ));
        }
        let lane_selector = lane.id.selector();
        if lane.child_lane_set_id.is_some()
            || lane.child_lane_set_base.is_some()
            || lane.child_lane_set_name.is_some()
            || lanes
                .iter()
                .any(|child| child.parent.as_ref() == Some(&lane_selector))
        {
            xml.push_str("<childLaneSet");
            if let Some(id) = &lane.child_lane_set_id {
                write_xml_id(xml, id);
            }
            write_xml_name(xml, lane.child_lane_set_name.as_deref());
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
            write_lanes(xml, lanes, Some(&lane_selector), document, namespace);
            xml.push_str("</childLaneSet>");
        }
        xml.push_str("</lane>");
    }
}
pub(super) fn write_item_attributes(
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
pub(super) fn write_item_state(
    xml: &mut String,
    metadata: Option<&processcraft_core::bpmn_data::ItemAwareMetadata>,
    restore_default: bool,
) {
    if let Some(state) = metadata.and_then(|m| m.data_state.as_ref()) {
        xml.push_str("<dataState");
        if restore_default {
            xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_io_specification(
    xml: &mut String,
    io: &processcraft_core::bpmn_data::IoSpecification,
    unqualified: bool,
    document: &ExportContext,
) {
    xml.push_str("<ioSpecification");
    if unqualified {
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
                xml.push_str(&format!(" xmlns:pc_data=\"{BPMN}\" xmlns=\"\""));
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
pub(super) fn write_event_port(
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
        xml.push_str(&format!(" xmlns:pc_data=\"{BPMN}\" xmlns=\"\""));
    } else if restore_default {
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_event_set(
    xml: &mut String,
    set: &processcraft_core::bpmn_data::IoSet,
    input: bool,
    restore_default: bool,
    context: &ExportContext,
) {
    let tag = if input { "inputSet" } else { "outputSet" };
    xml.push_str(&format!("<{tag}"));
    if restore_default {
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_event_property(
    xml: &mut String,
    property: &EventProperty,
    restore_default: bool,
) {
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
        xml.push_str(&format!(" xmlns:pc_data=\"{BPMN}\" xmlns=\"\""));
    } else if restore_default {
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_data_association(
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
        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
pub(super) fn write_implicit_throw(
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
