//! Parser for process containers and their flow nodes.
use crate::interchange::import::elements::{
    activity_io, activity_parts, activity_properties, event_reference, gateway_details,
    item_documentation, loop_characteristics, parse_audit_monitor, parse_correlation_subscription,
    parse_event, parse_io_binding, parse_io_specification, parse_item_aware, parse_lane_set,
    parse_property, parse_resource_role,
};
use crate::interchange::import::metadata::{
    anonymous_documentation, anonymous_extras, flow_parts, take_anonymous_base, take_flow_metadata,
};
use crate::interchange::import::tree::{
    Elem, attrs, documentation, element_identity, mixed_body, optional_bool, optional_id, selector,
    structured_documentation,
};
use crate::interchange::refusal::at;
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{EventReference, NodeDetails};
use processcraft_core::model::*;
pub(super) fn parse_container(e: &Elem, depth: usize, target_namespace: &str) -> Option<Process> {
    if depth > 32 {
        return None;
    }
    let mut p = Process {
        activity_metadata: None,
        lane_set_base: None,
        lane_set_name: None,
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
        at(n);
        if e.name == "process" && n.ns == BPMN {
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
        ) && n.ns == BPMN
            && matches!(
                n.name.as_str(),
                "standardLoopCharacteristics" | "multiInstanceLoopCharacteristics"
            )
        {
            continue;
        }
        if e.name == "adHocSubProcess" && n.ns == BPMN && n.name == "completionCondition" {
            continue;
        }
        if n.ns == BPMN && n.name == "documentation" {
            documentation_nodes.push(structured_documentation(n)?);
            continue;
        }
        if n.ns != BPMN {
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
            p.lane_set_name = n.attrs.get("name").cloned();
            p.lane_set_base = take_anonymous_base(n);
            if p.lane_set_base.is_none()
                && p.lane_set_id.is_none()
                && p.lane_set_name.is_none()
                && n.children.is_empty()
            {
                p.lane_set_base = Some(Default::default());
            }
            parse_lane_set(n, None, &mut p.lanes, 0, target_namespace)?;
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
                if c.ns != BPMN {
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
                        lane_set_name: child.lane_set_name,
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
