//! Parsers for choreography tasks, graphs and flows.
use crate::interchange::import::collaboration::{
    collaboration_child_phase, parse_collaboration_artifact, parse_collaboration_extras_child,
    parse_conversation, parse_participant, parse_participant_association,
};
use crate::interchange::import::elements::{
    event_reference, gateway_details, parse_correlation_key, parse_event,
};
use crate::interchange::import::metadata::{
    anonymous_documentation, anonymous_extras, flow_parts, take_anonymous_base, take_flow_metadata,
};
use crate::interchange::import::tree::{
    Elem, attrs, documentation, element_identity, optional_bool, selector,
};
use crate::interchange::shared::MAX_CHOREOGRAPHY_DEPTH;
use crate::ns::BPMN;
use processcraft_core::bpmn_model::EventReference;
use processcraft_core::model::*;
fn parse_choreography_flow(
    e: &Elem,
    namespace: &str,
    depth: usize,
    data_associations: &mut Vec<processcraft_core::bpmn_data::DataAssociation>,
) -> Option<processcraft_core::bpmn_collaboration::ChoreographyFlowElement> {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivity, ChoreographyActivityDetails as Details,
        ChoreographyFlowElement as Element, ChoreographyGraph, ChoreographyLoopType,
    };
    if depth > MAX_CHOREOGRAPHY_DEPTH || e.ns != BPMN {
        return None;
    }
    if matches!(
        e.name.as_str(),
        "choreographyTask" | "subChoreography" | "callChoreography"
    ) {
        if !attrs(
            e,
            &[
                "id",
                "name",
                "initiatingParticipantRef",
                "loopType",
                "calledChoreographyRef",
            ],
        ) || (e.name != "callChoreography" && e.attrs.contains_key("calledChoreographyRef"))
        {
            return None;
        }
        let mut participants = Vec::new();
        let mut messages = Vec::new();
        let mut keys = Vec::new();
        let mut graph = ChoreographyGraph::default();
        let mut artifacts = Vec::new();
        let mut associations = Vec::new();
        let mut phase = 0;
        for child in &e.children {
            if child.ns != BPMN {
                return None;
            }
            match child.name.as_str() {
                "participantRef"
                    if phase == 0 && attrs(child, &[]) && child.children.is_empty() =>
                {
                    participants.push(event_reference(
                        child.catalog_text_qname.as_ref()?,
                        namespace,
                    ))
                }
                "correlationKey" if phase <= 1 => {
                    phase = 1;
                    keys.push(parse_correlation_key(child, namespace)?);
                }
                "messageFlowRef"
                    if e.name == "choreographyTask"
                        && phase <= 2
                        && attrs(child, &[])
                        && child.children.is_empty() =>
                {
                    phase = 2;
                    messages.push(event_reference(
                        child.catalog_text_qname.as_ref()?,
                        namespace,
                    ));
                }
                "participantAssociation" if e.name == "callChoreography" && phase <= 2 => {
                    phase = 2;
                    associations.push(parse_participant_association(child, namespace)?);
                }
                "group" | "textAnnotation" | "association" if e.name == "subChoreography" => {
                    phase = 3;
                    artifacts.push(parse_collaboration_artifact(child, namespace)?);
                }
                _ if e.name == "subChoreography" && phase <= 2 => {
                    phase = 2;
                    let element = parse_choreography_flow(
                        child,
                        namespace,
                        depth + 1,
                        &mut graph.data_associations,
                    )?;
                    graph.flow_elements.push(element);
                }
                _ => return None,
            }
        }
        if participants.len() < 2
            || (e.name == "choreographyTask" && !(1..=2).contains(&messages.len()))
        {
            return None;
        }
        let details = match e.name.as_str() {
            "subChoreography" => Some(Details::Sub {
                graph: Box::new(graph),
                artifacts,
            }),
            "callChoreography" => Some(Details::Call {
                participant_associations: associations,
                called_choreography_ref: e
                    .catalog_qnames
                    .get("calledChoreographyRef")
                    .map(|q| event_reference(q, namespace)),
            }),
            _ => None,
        };
        let loop_type = match e.attrs.get("loopType").map(String::as_str) {
            None => None,
            Some("None") => Some(ChoreographyLoopType::None),
            Some("Standard") => Some(ChoreographyLoopType::Standard),
            Some("MultiInstanceSequential") => Some(ChoreographyLoopType::MultiInstanceSequential),
            Some("MultiInstanceParallel") => Some(ChoreographyLoopType::MultiInstanceParallel),
            _ => return None,
        };
        return Some(Element::Activity(Box::new(ChoreographyActivity {
            loop_type,
            details,
            flow_metadata: take_flow_metadata(e),
            correlation_keys: keys,
            base: take_anonymous_base(e),
            id: element_identity(e)?,
            name: e.attrs.get("name").cloned(),
            initiating_participant: Some(event_reference(
                e.catalog_qnames.get("initiatingParticipantRef")?,
                namespace,
            )),
            participant_refs: participants,
            message_flow_refs: messages,
        })));
    }
    if e.name == "sequenceFlow"
        && attrs(e, &["id", "name", "sourceRef", "targetRef", "isImmediate"])
    {
        let (doc, condition) = flow_parts(e)?;
        return Some(Element::SequenceFlow(Box::new(Flow {
            message_ref: None,
            flow_metadata: take_flow_metadata(e),
            base_extras: anonymous_extras(e),
            association_direction: None,
            id: element_identity(e)?,
            kind: FlowKind::Sequence,
            source: Some(EventReference::Local(selector(e, "sourceRef")?)),
            target: Some(EventReference::Local(selector(e, "targetRef")?)),
            name: e.attrs.get("name").cloned(),
            documentation: anonymous_documentation(e, doc),
            condition,
        })));
    }
    let kind = match e.name.as_str() {
        "startEvent" => NodeKind::StartEvent,
        "endEvent" => NodeKind::EndEvent,
        "intermediateThrowEvent" => NodeKind::IntermediateThrowEvent,
        "intermediateCatchEvent" => NodeKind::IntermediateCatchEvent,
        "boundaryEvent" => NodeKind::BoundaryEvent,
        "exclusiveGateway" => NodeKind::ExclusiveGateway,
        "parallelGateway" => NodeKind::ParallelGateway,
        "inclusiveGateway" => NodeKind::InclusiveGateway,
        "complexGateway" => NodeKind::ComplexGateway,
        "eventBasedGateway" => NodeKind::EventBasedGateway,
        _ => return None,
    };
    let gateway = matches!(
        kind,
        NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway
            | NodeKind::InclusiveGateway
            | NodeKind::ComplexGateway
            | NodeKind::EventBasedGateway
    );
    if !attrs(
        e,
        if gateway {
            &[
                "id",
                "name",
                "default",
                "gatewayDirection",
                "instantiate",
                "eventGatewayType",
            ]
        } else if kind == NodeKind::BoundaryEvent {
            &[
                "id",
                "name",
                "attachedToRef",
                "cancelActivity",
                "parallelMultiple",
            ]
        } else if kind == NodeKind::StartEvent {
            &["id", "name", "parallelMultiple", "isInterrupting"]
        } else if kind == NodeKind::IntermediateCatchEvent {
            &["id", "name", "parallelMultiple"]
        } else {
            &["id", "name"]
        },
    ) {
        return None;
    }
    let id = element_identity(e)?;
    let (doc, details) = if gateway {
        gateway_details(e)?
    } else {
        let (doc, details, associations) = parse_event(e, kind, &id.selector(), namespace)?;
        data_associations.extend(associations);
        (doc, details)
    };
    let node = Box::new(Node {
        flow_metadata: take_flow_metadata(e),
        base_extras: anonymous_extras(e),
        item_aware: None,
        id,
        name: e.attrs.get("name").cloned(),
        documentation: anonymous_documentation(e, doc),
        kind,
        details,
        owner: String::new(),
        contact: String::new(),
        activity: None,
        io_specification: None,
        loop_characteristics: None,
    });
    Some(if gateway {
        Element::Gateway(node)
    } else {
        Element::Event(node)
    })
}
pub(super) fn parse_choreography(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_collaboration::Choreography> {
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyFlowElement, ChoreographyGraph, ChoreographyRootKind,
    };
    if !e.text.trim().is_empty()
        || !attrs(e, &["id", "name", "isClosed", "initiatingParticipantRef"])
        || (e.name != "globalChoreographyTask" && e.attrs.contains_key("initiatingParticipantRef"))
    {
        return None;
    }
    let mut extras = processcraft_core::bpmn_collaboration::CollaborationExtras {
        is_closed: optional_bool(e.attrs.get("isClosed"))?,
        ..Default::default()
    };
    let mut c = Choreography {
        kind: if e.name == "globalChoreographyTask" {
            Some(ChoreographyRootKind::Global {
                initiating_participant_ref: e
                    .catalog_qnames
                    .get("initiatingParticipantRef")
                    .map(|q| event_reference(q, namespace)),
            })
        } else {
            None
        },
        graph: None,
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
    let mut graph = ChoreographyGraph::default();
    let mut needs_graph = e.name == "globalChoreographyTask";
    let mut seen_sequence = false;
    for child in &e.children {
        let next = collaboration_child_phase(&child.name).unwrap_or(10);
        if next < phase {
            return None;
        }
        phase = next;
        if child.ns != BPMN {
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
            "choreographyTask"
            | "subChoreography"
            | "callChoreography"
            | "startEvent"
            | "endEvent"
            | "intermediateThrowEvent"
            | "intermediateCatchEvent"
            | "boundaryEvent"
            | "exclusiveGateway"
            | "parallelGateway"
            | "inclusiveGateway"
            | "complexGateway"
            | "eventBasedGateway"
            | "sequenceFlow" => {
                let element =
                    parse_choreography_flow(child, namespace, 0, &mut graph.data_associations)?;
                if !matches!(&element, ChoreographyFlowElement::Activity(a) if a.details.is_none())
                    && !matches!(&element, ChoreographyFlowElement::SequenceFlow(_))
                {
                    needs_graph = true;
                }
                if matches!(&element, ChoreographyFlowElement::Activity(_)) && seen_sequence {
                    needs_graph = true;
                }
                if matches!(&element, ChoreographyFlowElement::SequenceFlow(_)) {
                    seen_sequence = true;
                }
                graph.flow_elements.push(element);
            }
            _ if parse_collaboration_extras_child(child, namespace, &mut extras)? => {}
            _ => return None,
        }
    }
    if e.name == "globalChoreographyTask" && !graph.flow_elements.is_empty() {
        return None;
    }
    if needs_graph {
        c.graph = Some(graph);
    } else {
        for element in graph.flow_elements {
            match element {
                ChoreographyFlowElement::Activity(a) => c.activities.push(*a),
                ChoreographyFlowElement::SequenceFlow(f) => c.sequence_flows.push(*f),
                _ => unreachable!(),
            }
        }
    }
    if extras != Default::default() {
        c.extras = Some(extras);
    }
    Some(c)
}
