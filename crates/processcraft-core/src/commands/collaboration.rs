//! Collaboration, participant and conversation mutations.
use super::*;
pub(super) fn set_collaboration_root(
    p: &mut ProjectData,
    document_id: DocumentId,
    collaboration: Collaboration,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    if let Some(existing) = document
        .collaboration
        .as_mut()
        .filter(|root| root.id == collaboration.id)
    {
        *existing = collaboration;
    } else if let Some(existing) = document
        .additional_collaborations
        .iter_mut()
        .find(|root| root.id == collaboration.id)
    {
        *existing = collaboration;
    } else if document.collaboration.is_none() {
        document.collaboration = Some(collaboration);
    } else {
        document.additional_collaborations.push(collaboration);
    }
    Ok(())
}
pub(super) fn delete_collaboration_root(
    p: &mut ProjectData,
    document_id: DocumentId,
    collaboration_id: ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    if document
        .collaboration
        .as_ref()
        .is_some_and(|root| root.id == collaboration_id)
    {
        document.collaboration = None;
    } else {
        let index = document
            .additional_collaborations
            .iter()
            .position(|root| root.id == collaboration_id)
            .ok_or_else(|| invalid("collaboration root not found"))?;
        document.additional_collaborations.remove(index);
    }
    document
        .diagrams
        .retain(|diagram| diagram.plane != collaboration_id);
    Ok(())
}
pub(super) fn set_partner_catalog(
    p: &mut ProjectData,
    document_id: DocumentId,
    partner_entities: Vec<crate::bpmn_collaboration::PartnerEntity>,
    partner_roles: Vec<crate::bpmn_collaboration::PartnerRole>,
    endpoints: Vec<crate::bpmn_collaboration::EndPoint>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    document.partner_entities = partner_entities;
    document.partner_roles = partner_roles;
    document.endpoints = endpoints;
    Ok(())
}
pub(super) fn set_participant_multiplicity(
    p: &mut ProjectData,
    document_id: DocumentId,
    participant_id: ElementSelector,
    multiplicity: Option<ParticipantMultiplicity>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let participant = document
        .collaboration
        .iter_mut()
        .flat_map(|c| &mut c.participants)
        .chain(
            document
                .choreographies
                .iter_mut()
                .flat_map(|c| &mut c.participants),
        )
        .find(|participant| participant.id == participant_id)
        .ok_or_else(|| invalid("participant not found"))?;
    participant.multiplicity = multiplicity;
    Ok(())
}
pub(super) fn set_message_flow_reference(
    p: &mut ProjectData,
    document_id: DocumentId,
    flow_id: ElementSelector,
    message_ref: Option<crate::bpmn_activity::ModelReference>,
) -> Result<(), CommandError> {
    let flow = flow_mut(doc_structural(p, document_id)?, &flow_id)?;
    if flow.kind != FlowKind::Message {
        return Err(invalid("message reference requires a message flow"));
    }
    flow.message_ref = message_ref;
    Ok(())
}
pub(super) fn update_conversation_node(
    p: &mut ProjectData,
    document_id: DocumentId,
    node: crate::bpmn_collaboration::ConversationNode,
) -> Result<(), CommandError> {
    let c = collaboration_mut(doc_structural(p, document_id)?)?;
    let index = c
        .conversation_nodes
        .iter()
        .position(|n| n.id == node.id)
        .ok_or_else(|| invalid("conversation node not found"))?;
    c.conversation_nodes[index] = node;
    Ok(())
}
pub(super) fn delete_conversation_node(
    p: &mut ProjectData,
    document_id: DocumentId,
    node_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let c = collaboration_mut(d)?;
    if !c.conversation_nodes.iter().any(|n| n.id == node_id) {
        return Err(invalid("conversation node not found"));
    }
    let mut removed = std::collections::HashSet::from([node_id]);
    loop {
        let old_len = removed.len();
        for node in &c.conversation_nodes {
            if node.parent.as_ref().is_some_and(|id| removed.contains(id)) {
                removed.insert(node.id.selector());
            }
        }
        if removed.len() == old_len {
            break;
        }
    }
    c.conversation_nodes
        .retain(|n| !removed.contains(&n.id.selector()));
    for diagram in &mut d.diagrams {
        diagram
            .shapes
            .retain(|shape| !removed.contains(&shape.element));
    }
    p.links.retain(|link| {
        !(link.source.document_id == document_id && removed.contains(&link.source.xml_id))
    });
    Ok(())
}
pub(super) fn update_conversation_link(
    p: &mut ProjectData,
    document_id: DocumentId,
    link: crate::bpmn_collaboration::ConversationLink,
) -> Result<(), CommandError> {
    let c = collaboration_mut(doc_structural(p, document_id)?)?;
    let index = c
        .conversation_links
        .iter()
        .position(|l| l.id == link.id)
        .ok_or_else(|| invalid("conversation link not found"))?;
    c.conversation_links[index] = link;
    Ok(())
}
pub(super) fn delete_conversation_link(
    p: &mut ProjectData,
    document_id: DocumentId,
    link_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let c = collaboration_mut(d)?;
    let index = c
        .conversation_links
        .iter()
        .position(|l| l.id == link_id)
        .ok_or_else(|| invalid("conversation link not found"))?;
    c.conversation_links.remove(index);
    for diagram in &mut d.diagrams {
        diagram.edges.retain(|edge| edge.element != link_id);
    }
    p.links
        .retain(|link| !(link.source.document_id == document_id && link.source.xml_id == link_id));
    Ok(())
}
pub(super) fn collaboration_mut(
    document: &mut BpmnDocument,
) -> Result<&mut Collaboration, CommandError> {
    document
        .collaboration
        .as_mut()
        .ok_or_else(|| invalid("collaboration not found"))
}
pub(super) fn remove_collaboration_artifact(
    extras: &mut Option<crate::bpmn_collaboration::CollaborationExtras>,
    id: &ElementSelector,
) -> bool {
    use crate::bpmn_collaboration::CollaborationArtifact;
    let Some(extras) = extras else { return false };
    let Some(index) = extras.artifacts.iter().position(|artifact| match artifact {
        CollaborationArtifact::Group(value) => &value.id == id,
        CollaborationArtifact::TextAnnotation(value) => &value.id == id,
        CollaborationArtifact::Association(value) => &value.id == id,
    }) else {
        return false;
    };
    extras.artifacts.remove(index);
    true
}
