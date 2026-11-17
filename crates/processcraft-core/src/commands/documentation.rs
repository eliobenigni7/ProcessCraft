//! Documentation mutations and the helpers that edit documentation inside nested scopes.
use super::*;
pub(super) fn set_documentation_entry(
    p: &mut ProjectData,
    document_id: DocumentId,
    documentation_id: ElementSelector,
    documentation: crate::bpmn_metadata::Documentation,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    let opaque = d.has_opaque_xml();
    let mut found = 0;
    let mut invalid_id = false;
    let mut unsafe_content_change = false;
    let mut update = |item: &mut crate::bpmn_metadata::Documentation| {
        if item
            .id
            .as_ref()
            .is_some_and(|id| id.selector() == documentation_id)
        {
            found += 1;
            if serde_json::to_value(&item.id).ok() != serde_json::to_value(&documentation.id).ok() {
                invalid_id = true;
            } else if opaque && item.content != documentation.content {
                unsafe_content_change = true;
            } else {
                *item = documentation.clone();
            }
        }
    };
    for extension in &mut d.extensions {
        for item in &mut extension.documentation {
            update(item);
        }
    }
    crate::metadata_visit::visit_base_metadata_mut(d, |_, metadata| {
        if let Some(metadata) = metadata {
            for item in &mut metadata.documentation {
                update(item);
            }
        }
    });
    crate::metadata_visit::visit_legacy_metadata_mut(d, |_, documentation, _| {
        if let crate::bpmn_metadata::DocumentationSet::Structured(items) = documentation {
            for item in items {
                update(item);
            }
        }
    });
    crate::metadata_visit::visit_expressions_mut(d, |expression| {
        if let crate::bpmn_model::Expression::Structured(data) = expression
            && let crate::bpmn_model::ExpressionBody::Mixed { parts } = &mut data.text
        {
            for part in parts {
                if let crate::bpmn_model::ExpressionPart::Documentation { documentation } = part {
                    update(documentation);
                }
            }
        }
    });
    if found != 1 || invalid_id {
        return Err(invalid("documentation not found or identity changed"));
    }
    if unsafe_content_change {
        return Err(invalid(
            "opaque XML prevents replacing documentation content",
        ));
    }
    Ok(())
}
pub(super) fn set_script_body(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    body: Option<crate::bpmn_xml::MixedXml>,
) -> Result<(), CommandError> {
    let node = node_mut(doc_structural(p, document_id)?, &element_id)?;
    if node.kind != NodeKind::ScriptTask {
        return Err(invalid("element is not a script task"));
    }
    node.activity.get_or_insert_with(Default::default).script = body;
    Ok(())
}
pub(super) fn set_annotation_body(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    body: Option<crate::bpmn_xml::MixedXml>,
) -> Result<(), CommandError> {
    let node = node_mut(doc_structural(p, document_id)?, &element_id)?;
    if node.kind != NodeKind::TextAnnotation {
        return Err(invalid("element is not a text annotation"));
    }
    match &mut node.details {
        Some(crate::bpmn_model::NodeDetails::TextAnnotation { text, .. }) => {
            if text.is_none() {
                node.name = None;
            }
            *text = body;
        }
        None => {
            node.name = None;
            node.details = Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                text_format: None,
                text: body,
            });
        }
        _ => return Err(invalid("invalid text annotation details")),
    }
    Ok(())
}
pub(super) fn replace_documentation(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    documentation: crate::bpmn_metadata::DocumentationSet,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let mut found = false;
    for process in &mut d.processes {
        if process.id == element_id {
            process.documentation = documentation.clone();
            found = true;
        }
        found |= replace_nested_documentation(&mut process.nodes, &element_id, &documentation);
        if let Some(flow) = process.flows.iter_mut().find(|flow| flow.id == element_id) {
            flow.documentation = documentation.clone();
            found = true;
        }
    }
    if let Some(collaboration) = &mut d.collaboration
        && let Some(flow) = collaboration
            .flows
            .iter_mut()
            .find(|flow| flow.id == element_id)
    {
        flow.documentation = documentation.clone();
        found = true;
    }
    for collaboration in &mut d.additional_collaborations {
        for flow in &mut collaboration.flows {
            if flow.id == element_id {
                flow.documentation = documentation.clone();
                found = true;
            }
        }
    }
    for choreography in &mut d.choreographies {
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            if flow.id == element_id {
                flow.documentation = documentation.clone();
                found = true;
            }
        }
    }
    crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
        if id == &element_id {
            *value = documentation.clone();
            found = true;
        }
    });
    if !found {
        return Err(invalid("element has no legacy documentation field"));
    }
    Ok(())
}
pub(super) fn set_text_annotation(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    text: Option<String>,
) -> Result<(), CommandError> {
    let node = node_mut(doc_structural(p, document_id)?, &element_id)?;
    if node.kind != NodeKind::TextAnnotation {
        return Err(invalid("element is not a text annotation"));
    }
    match &mut node.details {
        Some(crate::bpmn_model::NodeDetails::TextAnnotation { text: value, .. }) => {
            if value
                .as_ref()
                .is_some_and(|value| matches!(value, crate::bpmn_xml::MixedXml::Structured(_)))
            {
                return Err(invalid(
                    "structured annotation requires an explicit mixed-content edit",
                ));
            }
            if value.is_none() {
                node.name = None;
            }
            *value = text.map(Into::into)
        }
        None => {
            node.name = None;
            node.details = Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                text_format: None,
                text: text.map(Into::into),
            })
        }
        _ => return Err(invalid("invalid text annotation details")),
    }
    Ok(())
}
pub(super) fn documentation(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    documentation: String,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    let mut structured = documentation_is_structured(d, &element_id);
    crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
        if id == &element_id
            && matches!(value, crate::bpmn_metadata::DocumentationSet::Structured(_))
        {
            structured = true;
        }
    });
    if structured {
        return Err(invalid(
            "structured documentation requires an explicit documentation edit",
        ));
    }
    let mut found = false;
    for pr in &mut d.processes {
        if pr.id == element_id {
            pr.documentation = documentation.clone().into();
            found = true;
        }
        found |= edit_nested(&mut pr.nodes, &element_id, Some(&documentation), true, 0);
        for n in &mut pr.nodes {
            if n.id == element_id {
                n.documentation = documentation.clone().into();
                found = true;
            }
        }
        for f in &mut pr.flows {
            if f.id == element_id {
                f.documentation = documentation.clone().into();
                found = true;
            }
        }
    }
    if let Some(c) = &mut d.collaboration {
        for f in &mut c.flows {
            if f.id == element_id {
                f.documentation = documentation.clone().into();
                found = true;
            }
        }
    }
    for collaboration in &mut d.additional_collaborations {
        for flow in &mut collaboration.flows {
            if flow.id == element_id {
                flow.documentation = documentation.clone().into();
                found = true;
            }
        }
    }
    for choreography in &mut d.choreographies {
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            if flow.id == element_id {
                flow.documentation = documentation.clone().into();
                found = true;
            }
        }
    }
    crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
        if id == &element_id {
            *value = documentation.clone().into();
            found = true;
        }
    });
    if !found {
        return Err(invalid("element not found"));
    }
    Ok(())
}
pub(super) fn documentation_is_structured(d: &BpmnDocument, id: &ElementSelector) -> bool {
    use crate::bpmn_metadata::DocumentationSet;
    d.processes
        .iter()
        .any(|p| p.id == *id && matches!(p.documentation, DocumentationSet::Structured(_)))
        || crate::queries::scopes(d).iter().any(|scope| {
            scope
                .nodes
                .iter()
                .any(|n| n.id == *id && matches!(n.documentation, DocumentationSet::Structured(_)))
                || scope.flows.iter().any(|f| {
                    f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_))
                })
        })
        || d.collaboration.as_ref().is_some_and(|c| {
            c.flows
                .iter()
                .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
        })
        || d.additional_collaborations
            .iter()
            .flat_map(|root| &root.flows)
            .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
        || d.choreographies
            .iter()
            .flat_map(|c| c.sequence_flows.iter().chain(&c.message_flows))
            .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
}
pub(super) fn replace_nested_documentation(
    nodes: &mut [Node],
    id: &ElementSelector,
    documentation: &crate::bpmn_metadata::DocumentationSet,
) -> bool {
    for node in nodes {
        if node.id == *id {
            node.documentation = documentation.clone();
            return true;
        }
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if let Some(flow) = content.flows.iter_mut().find(|flow| flow.id == *id) {
                flow.documentation = documentation.clone();
                return true;
            }
            if replace_nested_documentation(&mut content.nodes, id, documentation) {
                return true;
            }
        }
    }
    false
}
