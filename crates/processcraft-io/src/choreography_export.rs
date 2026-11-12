//! XML admission for choreography scopes; native drafts are never repaired here.
use processcraft_core::{
    bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyRootKind},
    model::{BpmnDocument, FlowKind},
    queries,
};

pub(crate) fn validate(document: &BpmnDocument) -> Result<(), String> {
    for root in &document.choreographies {
        if root.graph.is_some() && (!root.activities.is_empty() || !root.sequence_flows.is_empty())
        {
            return Err("Choreography has competing graph and legacy content".into());
        }
        if matches!(root.kind, Some(ChoreographyRootKind::Global { .. }))
            && (root
                .graph
                .as_ref()
                .is_some_and(|graph| !graph.flow_elements.is_empty())
                || !root.activities.is_empty()
                || !root.sequence_flows.is_empty())
        {
            return Err(
                "BPMN 2.0.2 §11.5.4 forbids Flow Elements in a Global Choreography Task".into(),
            );
        }
    }
    if let Some(issue) =
        processcraft_core::validation::choreography_reference_issues(document).first()
    {
        return Err(format!(
            "{} has an unresolved, wrong-kind or out-of-scope local target on {:?}",
            issue.field, issue.owner
        ));
    }
    for (index, scope) in queries::choreography_scopes(document).iter().enumerate() {
        let path = format!("choreography_scopes[{index}]");
        for (index, activity) in scope.activities.iter().enumerate() {
            let path = format!("{path}.activities[{index}]");
            if activity.initiating_participant.is_none() {
                return Err(format!(
                    "{path}.initiatingParticipantRef is required for BPMN XML export"
                ));
            }
            if activity.participant_refs.len() < 2 {
                return Err(format!(
                    "{path} requires at least two participantRef values for BPMN XML export"
                ));
            }
            match &activity.details {
                None => {
                    if !(1..=2).contains(&activity.message_flow_refs.len()) {
                        return Err(format!(
                            "{path} requires one or two messageFlowRef values for BPMN XML export"
                        ));
                    }
                }
                Some(details) => {
                    if !activity.message_flow_refs.is_empty() {
                        return Err(format!(
                            "{path}.messageFlowRef is only representable on a Choreography Task"
                        ));
                    }
                    if let ChoreographyActivityDetails::Call {
                        participant_associations,
                        ..
                    } = details
                    {
                        for (index, association) in participant_associations.iter().enumerate() {
                            crate::collaboration_export::participant_association(
                                association,
                                &format!("{path}.participant_associations[{index}]"),
                            )?;
                        }
                    }
                }
            }
        }
        for (index, flow) in scope.flows.iter().enumerate() {
            let path = format!("{path}.sequence_flows[{index}]");
            if flow.kind != FlowKind::Sequence {
                return Err(format!("{path} has the wrong flow kind"));
            }
            crate::collaboration_export::flow(flow, &path)?;
        }
        // Root artifacts are checked with inherited Collaboration metadata;
        // nested SubChoreography artifacts have the same grammar and checks.
        for (index, artifact) in scope.artifacts.iter().enumerate() {
            crate::collaboration_export::artifacts(
                std::slice::from_ref(*artifact),
                &format!("{path}.artifact[{index}]"),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{interchange::prepare_import, interchange_compare::xml_model_value};
    use processcraft_core::{
        bpmn_collaboration::{ChoreographyFlowElement, ChoreographyGraph},
        ids::ElementIdentity,
    };

    fn legacy() -> BpmnDocument {
        let source = include_bytes!("../../../tests/fixtures/bpmn20-schema/choreography.bpmn");
        let parsed = prepare_import(source, "legacy choreography").unwrap();
        assert!(parsed.report.editable);
        parsed.document
    }

    #[test]
    fn graph_promotion_keeps_xml_meaning_and_anonymous_owner_paths() {
        let mut document = legacy();
        document.choreographies[0].activities[0].id = ElementIdentity::anonymous();
        let mut second = document.choreographies[0].activities[0].clone();
        second.id = ElementIdentity::anonymous();
        second.name = Some("Second exchange".into());
        document.choreographies[0].activities.push(second);
        let before = serde_json::to_value(&document).unwrap();
        let expected = xml_model_value(&document).unwrap();
        let mut promoted = document.clone();
        let root = &mut promoted.choreographies[0];
        root.graph = Some(ChoreographyGraph {
            flow_elements: std::mem::take(&mut root.activities)
                .into_iter()
                .map(|activity| ChoreographyFlowElement::Activity(Box::new(activity)))
                .collect(),
            ..Default::default()
        });
        assert_eq!(xml_model_value(&promoted).unwrap(), expected);
        assert_ne!(serde_json::to_value(&promoted).unwrap(), before);
        promoted.choreographies[0]
            .graph
            .as_mut()
            .unwrap()
            .flow_elements
            .reverse();
        assert_ne!(xml_model_value(&promoted).unwrap(), expected);
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }

    #[test]
    fn incomplete_nested_draft_is_refused_without_native_changes() {
        let source = include_bytes!("../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn");
        let parsed = prepare_import(source, "nested choreography").unwrap();
        assert!(parsed.report.editable);
        let mut document = parsed.document;
        let ChoreographyFlowElement::Activity(activity) = &mut document.choreographies[0]
            .graph
            .as_mut()
            .unwrap()
            .flow_elements[1]
        else {
            panic!("SubChoreography expected")
        };
        activity.initiating_participant = None;
        let before = serde_json::to_value(&document).unwrap();
        assert!(
            validate(&document)
                .unwrap_err()
                .contains("initiatingParticipantRef")
        );
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }
}
