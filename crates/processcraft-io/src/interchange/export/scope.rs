//! Writer for one process or sub-process scope (`write_scope`).
use crate::interchange::export::ExportContext;
use crate::interchange::export::collaboration::{
    collaboration_base_scopes, collaboration_flow_scopes, event_reference_qname,
    write_collaboration_qname_owner, write_event_qname_text,
};
use crate::interchange::export::elements::{
    anonymous_base_attributes, anonymous_base_children, anonymous_extras_attributes,
    finish_anonymous_base_empty, local_flow_xml_reference, write_data_association,
    write_event_definition, write_event_port, write_event_property, write_event_set,
    write_flow_metadata, write_implicit_throw, write_io_specification, write_item_attributes,
    write_item_state, write_lanes, write_rendering, write_resource_role, write_xml_id,
    write_xml_name, xml_reference,
};
use crate::interchange::export::xml::{
    esc, metadata_context, write_catalog_qname_attribute, write_context_qname_attribute, write_doc,
    write_expression, write_loop_expression, write_mixed_element,
};
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{
    EventDefinition, EventIo, EventReference, Expression, LoopCharacteristics, NodeDetails,
    QNameRef,
};
use processcraft_core::model::*;
// Recursive scope serialization shares the same document namespace and ID allocation.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_scope(
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
            xml.push_str(&format!(" xmlns:pc_data=\"{BPMN}\" xmlns=\"\""));
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
            || matches!(&n.details, Some(NodeDetails::Event { attached_to: Some(EventReference::Qualified(q)), .. }) if q.namespace.is_empty())
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
                            && uri == BPMN
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
            xml.push_str(&format!(" xmlns:{owner_prefix}=\"{BPMN}\" xmlns=\"\""));
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
            let q = event_reference_qname(id, namespace, document);
            write_catalog_qname_attribute(xml, "attachedToRef", &q, prefix);
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
                            format!(" xmlns=\"{BPMN}\"")
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
                    let owner_prefix = empty_default.then(|| prefix_for(BPMN));
                    let mut written_refs = Vec::new();
                    for (name, q) in &refs {
                        let value =
                            if q.namespace.is_empty() || (!empty_default && q.namespace == BPMN) {
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
                        xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
                            xml.push_str(&format!(" xmlns=\"{BPMN}\""));
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
                || content.lane_set_name.is_some()
                || !content.lanes.is_empty()
            {
                xml.push_str("<laneSet");
                if let Some(id) = &content.lane_set_id {
                    write_xml_id(xml, id);
                }
                write_xml_name(xml, content.lane_set_name.as_deref());
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
                write_lanes(xml, &content.lanes, None, document, namespace);
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
