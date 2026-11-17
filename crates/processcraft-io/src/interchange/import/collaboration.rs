//! Parsers for collaboration, participants, conversations and message-flow artifacts.
use crate::interchange::import::elements::{event_reference, parse_correlation_key};
use crate::interchange::import::metadata::{
    anonymous_documentation, anonymous_extras, take_anonymous_base, take_flow_metadata,
};
use crate::interchange::import::tree::{
    Elem, attrs, documentation, element_identity, mixed_body, optional_bool, optional_id,
};
use crate::interchange::refusal::refused;
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{EventReference, NodeDetails};
use processcraft_core::ids::ElementSelector;
use processcraft_core::model::*;
pub(super) fn parse_participant_association(
    e: &Elem,
    namespace: &str,
) -> Option<processcraft_core::bpmn_collaboration::ParticipantAssociation> {
    if e.ns != BPMN
        || e.name != "participantAssociation"
        || !attrs(e, &["id"])
        || e.children.len() != 2
    {
        return refused(|| {
            "<participantAssociation> without exactly one inner and one outer reference".into()
        });
    }
    let reference = |child: &Elem, name: &str| -> Option<EventReference> {
        if child.ns != BPMN
            || child.name != name
            || !attrs(child, &[])
            || !child.children.is_empty()
        {
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
pub(super) fn parse_conversation(
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
        return refused(|| {
            "conversation node with unsupported attributes, content or nesting deeper than 32 levels".into()
        });
    }
    let kind = match e.name.as_str() {
        "conversation" => ConversationKind::Conversation,
        "subConversation" => ConversationKind::SubConversation,
        "callConversation" => ConversationKind::CallConversation,
        _ => return refused(|| "unsupported conversation node element".into()),
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
        return refused(|| "calledCollaborationRef outside a <callConversation>".into());
    }
    let mut correlation_keys = Vec::new();
    let mut phase = 0;
    for c in &e.children {
        if c.ns != BPMN {
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
pub(super) fn parse_participant(e: &Elem, namespace: &str) -> Option<Participant> {
    if !e.text.trim().is_empty() || !attrs(e, &["id", "name", "processRef"]) {
        return refused(|| "<participant> with unsupported attributes or content".into());
    }
    let mut interface_refs = Vec::new();
    let mut endpoint_refs = Vec::new();
    let mut multiplicity = None;
    let mut phase = 0;
    for child in &e.children {
        if child.ns != BPMN {
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
pub(super) fn parse_collaboration_artifact(
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
                if child.ns != BPMN || child.name != "text" || text.is_some() {
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
pub(super) fn parse_collaboration_extras_child(
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
                return refused(|| {
                    "collaboration association with unsupported attributes or content".into()
                });
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
pub(super) fn parse_partner_refs(e: &Elem, namespace: &str) -> Option<Vec<EventReference>> {
    if !e.text.trim().is_empty() {
        return None;
    }
    let mut result = Vec::new();
    for child in &e.children {
        if child.ns != BPMN
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
pub(super) fn collaboration_child_phase(name: &str) -> Option<u8> {
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
pub(super) fn parse_collaboration(e: &Elem, namespace: &str) -> Option<Collaboration> {
    use processcraft_core::bpmn_collaboration::ConversationLink;
    if !e.text.trim().is_empty() || !attrs(e, &["id", "name", "isClosed"]) {
        return refused(|| "<collaboration> with unsupported attributes or content".into());
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
            return refused(|| "<collaboration> children out of order".into());
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
