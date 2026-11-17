//! The `rename` mutation: one title/name edit routed by element kind.
use super::*;
pub(super) fn rename(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    name: Option<String>,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    if crate::queries::find_node(d, &element_id)
        .is_some_and(|node| node.kind == NodeKind::TextAnnotation)
        || d.collaboration.iter().chain(&d.additional_collaborations).filter_map(|root| root.extras.as_ref()).chain(d.choreographies.iter().filter_map(|root| root.extras.as_ref())).flat_map(|extras| &extras.artifacts).any(|artifact| matches!(artifact, crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) if node.id == element_id))
    {
        return Err(invalid("use SetTextAnnotation to edit annotation text"));
    }
    if crate::queries::scopes(d)
        .iter()
        .flat_map(|scope| scope.flows)
        .chain(d.collaboration.iter().flat_map(|c| &c.flows))
        .any(|flow| flow.id == element_id && flow.kind == FlowKind::Association)
        || d.collaboration.iter().chain(&d.additional_collaborations).filter_map(|root| root.extras.as_ref()).chain(d.choreographies.iter().filter_map(|root| root.extras.as_ref())).flat_map(|extras| &extras.artifacts).any(|artifact| matches!(artifact, crate::bpmn_collaboration::CollaborationArtifact::Association(flow) if flow.id == element_id))
    {
        return Err(invalid(
            "association has no BPMN name; use its native label where available",
        ));
    }
    let mut found = false;
    for pr in &mut d.processes {
        if pr.id == element_id {
            pr.name = name.clone();
            found = true;
        }
        found |= edit_nested(&mut pr.nodes, &element_id, name.as_deref(), false, 0);
        for n in &mut pr.nodes {
            if n.id == element_id {
                n.name = name.clone();
                found = true;
            }
        }
        for l in &mut pr.lanes {
            if l.id == element_id {
                l.name = name.clone();
                found = true;
            }
        }
        for f in &mut pr.flows {
            if f.id == element_id {
                f.name = name.clone();
                found = true;
            }
        }
    }
    if let Some(c) = &mut d.collaboration {
        if c.id == element_id {
            c.name = name.clone();
            found = true;
        }
        for node in &mut c.conversation_nodes {
            if node.id == element_id {
                node.name = name.clone();
                found = true;
            }
        }
        for link in &mut c.conversation_links {
            if link.id == element_id {
                link.name = name.clone();
                found = true;
            }
        }
        for part in &mut c.participants {
            if part.id == element_id {
                part.name = name.clone();
                found = true;
            }
        }
        for f in &mut c.flows {
            if f.id == element_id {
                f.name = name.clone();
                found = true;
            }
        }
    }
    for c in &mut d.additional_collaborations {
        if c.id == element_id {
            c.name = name.clone();
            found = true;
        }
        for participant in &mut c.participants {
            if participant.id == element_id {
                participant.name = name.clone();
                found = true;
            }
        }
        for node in &mut c.conversation_nodes {
            if node.id == element_id {
                node.name = name.clone();
                found = true;
            }
        }
        for link in &mut c.conversation_links {
            if link.id == element_id {
                link.name = name.clone();
                found = true;
            }
        }
        for flow in &mut c.flows {
            if flow.id == element_id {
                flow.name = name.clone();
                found = true;
            }
        }
    }
    for partner in &mut d.partner_entities {
        if partner.id.as_ref().is_some_and(|id| id == &element_id) {
            partner.name = name.clone();
            found = true;
        }
    }
    for partner in &mut d.partner_roles {
        if partner.id.as_ref().is_some_and(|id| id == &element_id) {
            partner.name = name.clone();
            found = true;
        }
    }
    for choreography in &mut d.choreographies {
        if choreography.id == element_id {
            choreography.name = name.clone();
            found = true;
        }
        for participant in &mut choreography.participants {
            if participant.id == element_id {
                participant.name = name.clone();
                found = true;
            }
        }
        for activity in &mut choreography.activities {
            if activity.id == element_id {
                activity.name = name.clone();
                found = true;
            }
        }
        if let Some(graph) = &mut choreography.graph {
            found |= rename_choreography_element(graph, &element_id, &name);
        }
        for node in &mut choreography.conversation_nodes {
            if node.id == element_id {
                node.name = name.clone();
                found = true;
            }
        }
        for link in &mut choreography.conversation_links {
            if link.id == element_id {
                link.name = name.clone();
                found = true;
            }
        }
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            if flow.id == element_id {
                flow.name = name.clone();
                found = true;
            }
        }
    }
    if !found {
        return Err(invalid("element not found"));
    }
    Ok(())
}
