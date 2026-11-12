//! Schema-required collaboration values and IDREF/QName distinctions.
//! Incomplete native drafts remain unchanged when XML export is refused.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        CollaborationArtifact, CollaborationExtras, ConversationKind, ConversationNode,
    },
    model::{BpmnDocument, Flow, FlowKind},
};

fn required<T>(value: &Option<T>, path: &str) -> Result<(), String> {
    value.as_ref().map(|_| ()).ok_or_else(|| {
        format!("{path} is required for BPMN XML export; complete the native draft first")
    })
}

fn flow(value: &Flow, path: &str) -> Result<(), String> {
    required(&value.source, &format!("{path}.sourceRef"))?;
    required(&value.target, &format!("{path}.targetRef"))?;
    if value.kind == FlowKind::Sequence
        && [&value.source, &value.target]
            .into_iter()
            .any(|endpoint| matches!(endpoint, Some(ModelReference::Qualified(_))))
    {
        return Err(format!(
            "{path} requires local IDREF endpoints for a Sequence Flow"
        ));
    }
    if value.kind != FlowKind::Sequence
        && (value.condition.is_some()
            || value.flow_metadata.as_ref().is_some_and(|metadata| {
                metadata != &processcraft_core::bpmn_activity::FlowElementMetadata::default()
            }))
    {
        return Err(format!(
            "{path} contains Sequence Flow metadata that cannot be represented on this BPMN element"
        ));
    }
    if value.kind != FlowKind::Message && value.message_ref.is_some() {
        return Err(format!(
            "{path}.messageRef is only supported on a Message Flow"
        ));
    }
    if value.kind != FlowKind::Association && value.association_direction.is_some() {
        return Err(format!(
            "{path}.associationDirection is only supported on an Association"
        ));
    }
    if value.kind == FlowKind::Association
        && value.name.as_deref().is_some_and(|name| !name.is_empty())
    {
        return Err(format!(
            "{path}.name cannot be represented on a BPMN Association"
        ));
    }
    Ok(())
}

fn extras(value: Option<&CollaborationExtras>, path: &str) -> Result<(), String> {
    let Some(value) = value else { return Ok(()) };
    for (index, association) in value.conversation_associations.iter().enumerate() {
        let path = format!("{path}.conversation_associations[{index}]");
        required(
            &association.inner,
            &format!("{path}.innerConversationNodeRef"),
        )?;
        required(
            &association.outer,
            &format!("{path}.outerConversationNodeRef"),
        )?;
    }
    for (index, association) in value.participant_associations.iter().enumerate() {
        participant_association(
            association,
            &format!("{path}.participant_associations[{index}]"),
        )?;
    }
    for (index, association) in value.message_flow_associations.iter().enumerate() {
        let path = format!("{path}.message_flow_associations[{index}]");
        required(&association.inner, &format!("{path}.innerMessageFlowRef"))?;
        required(&association.outer, &format!("{path}.outerMessageFlowRef"))?;
    }
    for (index, artifact) in value.artifacts.iter().enumerate() {
        let path = format!("{path}.artifacts[{index}]");
        match artifact {
            CollaborationArtifact::Association(association) => {
                if association.kind != FlowKind::Association {
                    return Err(format!("{path} must contain an Association"));
                }
                flow(association, &path)?;
            }
            CollaborationArtifact::TextAnnotation(annotation) => {
                if annotation.kind != processcraft_core::model::NodeKind::TextAnnotation {
                    return Err(format!("{path} must contain a Text Annotation"));
                }
            }
            CollaborationArtifact::Group(_) => {}
        }
    }
    Ok(())
}

fn participant_association(
    association: &processcraft_core::bpmn_collaboration::ParticipantAssociation,
    path: &str,
) -> Result<(), String> {
    required(&association.inner, &format!("{path}.innerParticipantRef"))?;
    required(&association.outer, &format!("{path}.outerParticipantRef"))
}

fn conversations(values: &[ConversationNode], path: &str) -> Result<(), String> {
    for (index, node) in values.iter().enumerate() {
        let path = format!("{path}.conversation_nodes[{index}]");
        if node.kind != ConversationKind::CallConversation && node.call.is_some() {
            return Err(format!(
                "{path} has a call payload on a different conversation kind"
            ));
        }
        if node.kind == ConversationKind::CallConversation && !node.message_flow_refs.is_empty() {
            return Err(format!(
                "{path}: messageFlowRef does not apply to a Call Conversation (BPMN 2.0.2 §9.5.4)"
            ));
        }
        if let Some(call) = &node.call {
            for (index, association) in call.participant_associations.iter().enumerate() {
                participant_association(
                    association,
                    &format!("{path}.participant_associations[{index}]"),
                )?;
            }
        }
    }
    Ok(())
}

pub(crate) fn validate(document: &BpmnDocument) -> Result<(), String> {
    for scope in processcraft_core::queries::scopes(document) {
        for (index, value) in scope.flows.iter().enumerate() {
            flow(value, &format!("process.flows[{index}]"))?;
        }
    }
    for (index, root) in document.collaborations().enumerate() {
        let path = format!("collaborations[{index}]");
        // BPMN 2.0.2 §9.5.5, p.131 excludes these inherited features.
        if root.kind
            == Some(
                processcraft_core::bpmn_collaboration::CollaborationRootKind::GlobalConversation,
            )
            && (!root.conversation_nodes.is_empty()
                || root
                    .extras
                    .as_ref()
                    .is_some_and(|extra| !extra.choreography_refs.is_empty()))
        {
            return Err(format!(
                "{path}: a Global Conversation cannot contain Conversation Nodes or choreography references"
            ));
        }
        for (index, value) in root.flows.iter().enumerate() {
            if value.kind != FlowKind::Message {
                return Err(format!("{path}.flows[{index}] must contain a Message Flow"));
            }
            flow(value, &format!("{path}.flows[{index}]"))?;
        }
        for (index, link) in root.conversation_links.iter().enumerate() {
            required(
                &link.source,
                &format!("{path}.conversation_links[{index}].sourceRef"),
            )?;
            required(
                &link.target,
                &format!("{path}.conversation_links[{index}].targetRef"),
            )?;
        }
        conversations(&root.conversation_nodes, &path)?;
        extras(root.extras.as_ref(), &path)?;
    }
    for (index, root) in document.choreographies.iter().enumerate() {
        let path = format!("choreographies[{index}]");
        for (family, expected, values) in [
            ("message_flows", FlowKind::Message, &root.message_flows),
            ("sequence_flows", FlowKind::Sequence, &root.sequence_flows),
        ] {
            for (index, value) in values.iter().enumerate() {
                if value.kind != expected {
                    return Err(format!("{path}.{family}[{index}] has the wrong flow kind"));
                }
                flow(value, &format!("{path}.{family}[{index}]"))?;
            }
        }
        for (index, link) in root.conversation_links.iter().enumerate() {
            required(
                &link.source,
                &format!("{path}.conversation_links[{index}].sourceRef"),
            )?;
            required(
                &link.target,
                &format!("{path}.conversation_links[{index}].targetRef"),
            )?;
        }
        conversations(&root.conversation_nodes, &path)?;
        extras(root.extras.as_ref(), &path)?;
    }
    for (issues, action) in [
        (
            processcraft_core::validation::association_inner_scope_unqualified(document),
            "Declare the referenced collaboration or choreography before exporting this local mapping",
        ),
        (
            processcraft_core::validation::message_flow_endpoint_scope_unqualified(document),
            "Associate the referenced activity or event with a participant's process before exporting",
        ),
        (
            processcraft_core::validation::conversation_link_endpoint_scope_unqualified(document),
            "Associate the referenced activity or event with a participant's process before exporting the conversation link",
        ),
    ] {
        if let Some(issue) = issues.first() {
            let owner = processcraft_core::queries::element_name(document, &issue.owner)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    document
                        .xml_id_for_selector(&issue.owner)
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| "an anonymous model element".into());
            return Err(format!(
                "Cannot qualify {} on {owner}. {action}",
                issue.field
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::{
        bpmn_collaboration::CollaborationRootKind, bpmn_model::QNameRef, ids::ElementIdentity,
        model::Collaboration,
    };

    fn external() -> ModelReference {
        ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "Remote".parse().unwrap(),
        })
    }

    fn root() -> Collaboration {
        Collaboration {
            id: ElementIdentity::anonymous(),
            name: None,
            base: None,
            kind: None,
            extras: None,
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
        }
    }

    #[test]
    fn incomplete_endpoint_is_refused_without_changing_native_draft() {
        let mut document = BpmnDocument::new("Draft".into(), ElementIdentity::anonymous());
        let mut collaboration = root();
        collaboration.flows.push(Flow {
            id: ElementIdentity::anonymous(),
            kind: FlowKind::Message,
            source: None,
            target: Some(external()),
            name: None,
            documentation: "".into(),
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            base_extras: None,
            condition: None,
        });
        document.collaboration = Some(collaboration);
        let before = serde_json::to_value(&document).unwrap();
        assert!(
            validate(&document)
                .unwrap_err()
                .contains("sourceRef is required")
        );
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
        let value = &mut document.collaboration.as_mut().unwrap().flows[0];
        value.source = Some(external());
        // An external QName stays a value; admission does not fetch its target.
        validate(&document).unwrap();
        let value = &mut document.collaboration.as_mut().unwrap().flows[0];
        value.kind = FlowKind::Sequence;
        assert!(flow(value, "flow").unwrap_err().contains("local IDREF"));
    }

    #[test]
    fn schema_inherited_global_and_call_members_obey_specific_bpmn_context() {
        let mut document = BpmnDocument::new("Draft".into(), ElementIdentity::anonymous());
        let mut collaboration = root();
        collaboration.kind = Some(CollaborationRootKind::GlobalConversation);
        collaboration.conversation_nodes.push(ConversationNode {
            id: ElementIdentity::anonymous(),
            name: None,
            base: None,
            kind: ConversationKind::Conversation,
            participant_refs: vec![],
            parent: None,
            message_flow_refs: vec![],
            call: None,
            correlation_keys: vec![],
        });
        document.collaboration = Some(collaboration);
        assert!(
            validate(&document)
                .unwrap_err()
                .contains("Global Conversation")
        );
        let collaboration = document.collaboration.as_mut().unwrap();
        collaboration.kind = None;
        let node = &mut collaboration.conversation_nodes[0];
        node.kind = ConversationKind::CallConversation;
        node.message_flow_refs.push(external());
        assert!(
            validate(&document)
                .unwrap_err()
                .contains("messageFlowRef does not apply")
        );
        document.collaboration.as_mut().unwrap().conversation_nodes[0]
            .message_flow_refs
            .clear();
        validate(&document).unwrap();
    }
}
