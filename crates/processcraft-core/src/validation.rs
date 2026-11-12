//! Non-blocking diagnostics: incomplete editing remains possible; structural validity is separate.
use crate::{
    bpmn_model::{EventDefinition, EventGatewayType, EventIo, GatewayDirection, NodeDetails},
    ids::{DocumentId, ElementSelector},
    model::{Capability, FlowKind, NodeKind, ProjectData},
    queries,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Hash index returning the same first candidate a linear `matches_identity`
/// search over `ids` would find for a non-external reference.
struct IdentityLookup<'a> {
    selectors: HashMap<ElementSelector, &'a crate::ids::ElementIdentity>,
    xml_ids: HashMap<&'a crate::ids::XmlId, &'a crate::ids::ElementIdentity>,
}
impl<'a> IdentityLookup<'a> {
    fn new(ids: &[&'a crate::ids::ElementIdentity]) -> Self {
        let mut lookup = Self {
            selectors: HashMap::with_capacity(ids.len()),
            xml_ids: HashMap::with_capacity(ids.len()),
        };
        for id in ids {
            lookup.selectors.entry(id.selector()).or_insert(id);
            if let Some(xml_id) = id.xml_id() {
                lookup.xml_ids.entry(xml_id).or_insert(id);
            }
        }
        lookup
    }
    fn candidates(
        &self,
        reference: &crate::bpmn_activity::ModelReference,
    ) -> Vec<&'a crate::ids::ElementIdentity> {
        match reference {
            crate::bpmn_activity::ModelReference::Local(selector) => self.selectors.get(selector),
            crate::bpmn_activity::ModelReference::Qualified(name) => self.xml_ids.get(&name.local),
        }
        .into_iter()
        .copied()
        .collect()
    }
}

pub fn invalid_activity_namespace(namespace: &str) -> bool {
    namespace.chars().any(|c| c.is_whitespace() || !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
}

/// Local references that cannot be resolved. External namespaces remain opaque.
pub fn activity_reference_issues(
    document: &crate::model::BpmnDocument,
    activity: &crate::bpmn_activity::ActivityProperties,
) -> Vec<&'static str> {
    let namespace = document
        .definitions
        .as_ref()
        .map(|m| m.target_namespace.as_str())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let mut issues = Vec::new();
    if activity
        .message_ref
        .as_ref()
        .is_some_and(|r| invalid_activity_namespace(&r.namespace))
    {
        issues.push("invalidActivityMessageNamespace");
    }
    if activity
        .operation_ref
        .as_ref()
        .is_some_and(|r| invalid_activity_namespace(&r.namespace))
    {
        issues.push("invalidActivityOperationNamespace");
    }
    if activity.message_ref.as_ref().is_some_and(|r| {
        namespace == Some(r.namespace.as_str())
            && !document
                .event_resources
                .iter()
                .any(|e| matches!(e, EventDefinitionResource::Message { id, .. } if id.xml_id() == Some(&r.local)))
    }) {
        issues.push("missingLocalActivityMessage");
    }
    if activity.operation_ref.as_ref().is_some_and(|r| {
        namespace == Some(r.namespace.as_str())
            && !document
                .catalog
                .interfaces
                .iter()
                .flat_map(|i| &i.operations)
                .any(|o| o.id.as_ref().and_then(|id| id.xml_id()) == Some(&r.local))
    }) {
        issues.push("missingLocalActivityOperation");
    }
    issues
}
use crate::bpmn_model::EventResource as EventDefinitionResource;

/// Bounded choreography graph checks. External names remain unresolved, not valid local targets.
pub(crate) fn choreography_graph_diagnostics(
    doc: &crate::model::BpmnDocument,
) -> Vec<(ElementSelector, &'static str, &'static str)> {
    use crate::bpmn_activity::ModelReference;
    use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyRootKind};
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    fn preceding<'a>(
        scope: &queries::ChoreographyScope<'a>,
        target: &ElementSelector,
        active: &mut HashSet<ElementSelector>,
        depth: usize,
        budget: &mut usize,
    ) -> Option<Vec<&'a crate::bpmn_collaboration::ChoreographyActivity>> {
        use crate::bpmn_activity::ModelReference;
        use crate::bpmn_collaboration::ChoreographyActivityDetails;
        let work = scope.flows.len().max(1);
        if depth > 128 || *budget < work || !active.insert(target.clone()) {
            return None;
        }
        *budget -= work;
        let incoming: Vec<_> = scope
            .flows
            .iter()
            .filter(|f| {
                f.kind == FlowKind::Sequence
                    && f.target.as_ref() == Some(&ModelReference::Local(target.clone()))
            })
            .collect();
        if incoming.is_empty() {
            active.remove(target);
            return None;
        }
        let mut result = Vec::new();
        for flow in incoming {
            if flow.condition.is_some() {
                active.remove(target);
                return None;
            }
            let Some(ModelReference::Local(source)) = &flow.source else {
                active.remove(target);
                return None;
            };
            if let Some(activity) = scope.activities.iter().find(|a| a.id.selector() == *source) {
                if matches!(
                    activity.details,
                    Some(ChoreographyActivityDetails::Call { .. })
                ) {
                    active.remove(target);
                    return None;
                }
                result.push(*activity);
            } else if scope
                .gateways
                .iter()
                .any(|g| g.id.selector() == *source && g.kind == NodeKind::ExclusiveGateway)
            {
                let Some(mut previous) = preceding(scope, source, active, depth + 1, budget) else {
                    active.remove(target);
                    return None;
                };
                result.append(&mut previous);
            } else {
                active.remove(target);
                return None;
            }
        }
        active.remove(target);
        Some(result)
    }
    let index = ReferenceIndex::new(doc);
    let scopes = queries::choreography_scopes(doc);
    let mut issues = Vec::new();
    for root in &doc.choreographies {
        if let Some(ChoreographyRootKind::Global {
            initiating_participant_ref: Some(reference),
        }) = &root.kind
        {
            match index.resolve(reference) {
                ReferenceResolution::MissingLocal => issues.push((root.id.selector(), "globalChoreographyInitiatorMissing", "The local global choreography initiator is unresolved; BPMN 2.0.2 §11.5.4.")),
                ReferenceResolution::Resolved(id) if !root.participants.iter().any(|p| p.id.selector() == id) => issues.push((root.id.selector(), "globalChoreographyInitiatorWrongOwner", "The global choreography initiator does not belong to this choreography; BPMN 2.0.2 §11.5.4.")),
                _ => {}
            }
        }
        if root.graph.is_some() && (!root.activities.is_empty() || !root.sequence_flows.is_empty())
        {
            issues.push((root.id.selector(), "mixedChoreographyAuthority", "A graph and legacy flow elements both have authority; retain only one representation."));
        }
        if matches!(root.kind, Some(ChoreographyRootKind::Global { .. }))
            && scopes
                .iter()
                .find(|s| s.owner == root.id.selector())
                .is_some_and(|s| !s.ordered.is_empty())
        {
            issues.push((
                root.id.selector(),
                "globalChoreographyFlowElements",
                "GlobalChoreographyTask MUST NOT contain Flow Elements; BPMN 2.0.2 §11.5.4.",
            ));
        }
    }
    for scope in &scopes {
        let graph = if scope.parent.is_none() {
            scope.root.graph.as_ref()
        } else {
            scopes
                .iter()
                .flat_map(|parent| &parent.activities)
                .find(|activity| activity.id.selector() == scope.owner)
                .and_then(|activity| match &activity.details {
                    Some(ChoreographyActivityDetails::Sub { graph, .. }) => Some(graph.as_ref()),
                    _ => None,
                })
        };
        if let Some(graph) = graph {
            for association in &graph.data_associations {
                if association
                    .sources
                    .iter()
                    .chain(std::iter::once(&association.target))
                    .any(|reference| {
                        doc.data_stores.iter().any(|store| {
                            store
                                .id
                                .as_ref()
                                .is_some_and(|id| id.selector() == *reference)
                        })
                    })
                {
                    issues.push((
                        association
                            .id
                            .as_ref()
                            .map_or_else(|| association.owner.clone(), |id| id.selector()),
                        "choreographyDataRepositoryUse",
                        "Choreographies do not use data repositories; BPMN 2.0.2 §11.3.",
                    ));
                }
            }
        }
        let root_participants: HashSet<_> = scope
            .root
            .participants
            .iter()
            .map(|p| p.id.selector())
            .collect();
        let nodes: HashSet<_> = scope
            .activities
            .iter()
            .map(|a| a.id.selector())
            .chain(scope.events.iter().map(|n| n.id.selector()))
            .chain(scope.gateways.iter().map(|n| n.id.selector()))
            .collect();
        let parent_participants: Option<HashSet<_>> = scope.parent.as_ref().and_then(|parent| {
            scopes
                .iter()
                .find(|s| &s.owner == parent)?
                .activities
                .iter()
                .find(|a| a.id.selector() == scope.owner)
                .map(|a| {
                    a.participant_refs
                        .iter()
                        .filter_map(|r| match index.resolve(r) {
                            ReferenceResolution::Resolved(id) => Some(id),
                            _ => None,
                        })
                        .collect()
                })
        });
        let incoming_targets: HashSet<_> = scope
            .flows
            .iter()
            .filter_map(|f| match &f.target {
                Some(ModelReference::Local(id)) if f.kind == FlowKind::Sequence => Some(id.clone()),
                _ => None,
            })
            .collect();
        let mut preceding_budget = 100_000;
        for activity in &scope.activities {
            let owner = activity.id.selector();
            if activity.participant_refs.len() < 2 {
                issues.push((owner.clone(), "choreographyParticipantsIncomplete", "A choreography activity requires at least two participant references; BPMN 2.0.2 Table 11.1."));
            }
            if activity.details.is_none() && activity.participant_refs.len() > 2 {
                issues.push((owner.clone(), "choreographyTaskParticipantCount", "A Choreography Task represents an exchange between two participants; BPMN 2.0.2 §11.5.1."));
            }
            if activity.details.is_none() && !(1..=2).contains(&activity.message_flow_refs.len()) {
                issues.push((owner.clone(), "choreographyTaskMessageFlowSchemaCardinality", "The pinned XSD requires one or two messageFlowRef values; Table 11.2 describes one or more."));
            }
            if activity.details.is_some() && !activity.message_flow_refs.is_empty() {
                issues.push((owner.clone(), "choreographyTaskOnlyMessageFlows", "Message flow references belong to Choreography Task, not Sub or Call Choreography; BPMN 2.0.2 Tables 11.2–11.4."));
            }
            for r in &activity.participant_refs {
                match index.resolve(r) {
                    ReferenceResolution::MissingLocal => issues.push((owner.clone(), "choreographyParticipantMissing", "A local participant reference is unresolved; BPMN 2.0.2 Table 11.1.")),
                    ReferenceResolution::Resolved(id) if !root_participants.contains(&id) => issues.push((owner.clone(), "choreographyParticipantWrongOwner", "A participant reference does not belong to this choreography; BPMN 2.0.2 Table 11.1.")),
                    ReferenceResolution::Resolved(id) if parent_participants.as_ref().is_some_and(|p| !p.contains(&id)) => {
                        // A parent with unresolved references cannot prove non-membership.
                        let parent_complete = scope.parent.as_ref().and_then(|p| scopes.iter().find(|s| &s.owner == p)).and_then(|s| s.activities.iter().find(|a| a.id.selector() == scope.owner)).is_some_and(|a| a.participant_refs.iter().all(|r| matches!(index.resolve(r), ReferenceResolution::Resolved(_))));
                        if parent_complete { issues.push((owner.clone(), "subChoreographyParticipantSubset", "A nested activity participant is not among its SubChoreography participants; BPMN 2.0.2 §11.5.2.")); }
                    }
                    _ => {}
                }
            }
            match &activity.initiating_participant {
                None => issues.push((owner.clone(), "choreographyInitiatorMissing", "The initiating participant is required; BPMN 2.0.2 Table 11.1.")),
                Some(r) => match index.resolve(r) {
                    ReferenceResolution::MissingLocal => issues.push((owner.clone(), "choreographyInitiatorUnresolved", "The local initiating participant is unresolved; BPMN 2.0.2 Table 11.1.")),
                    ReferenceResolution::Resolved(id) if activity.participant_refs.iter().all(|p| matches!(index.resolve(p), ReferenceResolution::Resolved(_))) && !activity.participant_refs.iter().any(|p| index.resolve(p) == ReferenceResolution::Resolved(id.clone())) => issues.push((owner.clone(), "choreographyInitiatorNotParticipant", "The initiating participant must be one of the activity participants; BPMN 2.0.2 Table 11.1.")),
                    _ => {}
                }
            }
            // Exclusive paths are evaluated only when every predecessor resolves
            // to an activity; a start, event, call, cycle or incomplete edge leaves
            // the sequencing claim unqualified.
            let previous = preceding(scope, &owner, &mut HashSet::new(), 0, &mut preceding_budget);
            let resolved_initiator =
                activity
                    .initiating_participant
                    .as_ref()
                    .and_then(|r| match index.resolve(r) {
                        ReferenceResolution::Resolved(id) => Some(id),
                        _ => None,
                    });
            let proven = previous.as_ref().is_some_and(|items| {
                items.iter().all(|a| {
                    a.participant_refs
                        .iter()
                        .all(|r| matches!(index.resolve(r), ReferenceResolution::Resolved(_)))
                })
            }) && resolved_initiator.is_some();
            if proven
                && previous.as_ref().is_some_and(|items| {
                    items.iter().any(|a| {
                        !a.participant_refs.iter().any(|r| {
                            index.resolve(r)
                                == ReferenceResolution::Resolved(
                                    resolved_initiator.clone().unwrap(),
                                )
                        })
                    })
                })
            {
                issues.push((owner.clone(), "choreographyInitiatorNotPreviouslyInvolved", "The initiator was not involved in a preceding activity on every resolved exclusive path; BPMN 2.0.2 §11.5.6."));
            } else if !proven && incoming_targets.contains(&owner) {
                issues.push((owner.clone(), "choreographyInitiatorSequenceUnqualified", "Preceding participant involvement cannot be established from the modeled paths; BPMN 2.0.2 §11.5.6."));
            }
            for r in &activity.message_flow_refs {
                match index.resolve(r) {
                    ReferenceResolution::MissingLocal => issues.push((owner.clone(), "choreographyMessageFlowMissing", "A local message flow reference is unresolved; BPMN 2.0.2 Table 11.2.")),
                    ReferenceResolution::Resolved(id) if !scope.root.message_flows.iter().any(|f| f.id.selector() == id && f.kind == FlowKind::Message) => issues.push((owner.clone(), "choreographyMessageFlowWrongOwner", "A message flow reference does not identify a message flow of this choreography; BPMN 2.0.2 Table 11.2.")),
                    _ => {}
                }
            }
            if let Some(ChoreographyActivityDetails::Call {
                called_choreography_ref,
                participant_associations,
            }) = &activity.details
            {
                if let Some(r) = called_choreography_ref {
                    match index.resolve(r) {
                        ReferenceResolution::MissingLocal => issues.push((owner.clone(), "calledChoreographyMissing", "The local called choreography is unresolved; BPMN 2.0.2 §11.5.3.")),
                        ReferenceResolution::Resolved(id) if !doc.choreographies.iter().any(|c| c.id.selector() == id) => issues.push((owner.clone(), "calledChoreographyWrongType", "The called element must be a choreography or global choreography task; BPMN 2.0.2 §11.5.3.")),
                        _ => {}
                    }
                }
                for a in participant_associations {
                    let association_owner =
                        a.id.as_ref()
                            .map_or_else(|| owner.clone(), |id| id.selector());
                    if a.inner.is_none() || a.outer.is_none() {
                        issues.push((association_owner.clone(), "callParticipantAssociationIncomplete", "Both inner and outer participant references are required; BPMN 2.0.2 Table 9.4."));
                    }
                    for reference in [&a.inner, &a.outer].into_iter().flatten() {
                        if matches!(index.resolve(reference), ReferenceResolution::MissingLocal) {
                            issues.push((association_owner.clone(), "callParticipantAssociationMissing", "A local participant association reference is unresolved; BPMN 2.0.2 §11.5.3."));
                        }
                    }
                    if let Some(r) = &a.outer
                        && let ReferenceResolution::Resolved(id) = index.resolve(r)
                        && !root_participants.contains(&id)
                    {
                        issues.push((association_owner.clone(), "callParticipantAssociationOuterWrongOwner", "The outer participant must belong to the calling choreography; BPMN 2.0.2 §11.5.3."));
                    }
                    if let (Some(inner), Some(called)) = (&a.inner, called_choreography_ref)
                        && let (
                            ReferenceResolution::Resolved(inner_id),
                            ReferenceResolution::Resolved(called_id),
                        ) = (index.resolve(inner), index.resolve(called))
                        && let Some(target) = doc
                            .choreographies
                            .iter()
                            .find(|c| c.id.selector() == called_id)
                        && !target
                            .participants
                            .iter()
                            .any(|p| p.id.selector() == inner_id)
                    {
                        issues.push((association_owner, "callParticipantAssociationInnerWrongOwner", "The inner participant must belong to the called choreography; BPMN 2.0.2 §11.5.3."));
                    }
                }
            }
        }
        for flow in &scope.flows {
            if flow.kind != FlowKind::Sequence {
                issues.push((
                    flow.id.selector(),
                    "choreographyFlowWrongType",
                    "A choreography graph sequence flow must be a SequenceFlow; BPMN 2.0.2 §11.5.",
                ));
            }
            for reference in [&flow.source, &flow.target] {
                match reference {
                    None => issues.push((flow.id.selector(), "choreographySequenceEndpointMissing", "A sequence flow endpoint is required; BPMN 2.0.2 Table 8.34.")),
                    Some(ModelReference::Qualified(_)) => issues.push((flow.id.selector(), "choreographySequenceEndpointNotIdref", "Sequence flow endpoints are local IDREF values; BPMN 2.0.2 Table 8.34.")),
                    Some(ModelReference::Local(id)) if !nodes.contains(id) => issues.push((flow.id.selector(), "choreographySequenceEndpointWrongScope", "A sequence flow endpoint must be a flow node in the same choreography scope; BPMN 2.0.2 §10.3.4.")),
                    _ => {}
                }
            }
        }
        for n in &scope.events {
            if !matches!(
                n.kind,
                NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent
            ) {
                issues.push((
                    n.id.selector(),
                    "choreographyEventWrongContext",
                    "This node is not a supported choreography event; BPMN 2.0.2 §11.4.",
                ));
            }
            if let Some(NodeDetails::Event {
                definitions,
                definition_refs,
                attached_to,
                cancel_activity,
                ..
            }) = &n.details
            {
                let resolved: Vec<_> = definition_refs
                    .iter()
                    .filter_map(|r| match index.resolve(r) {
                        ReferenceResolution::Resolved(id) => doc
                            .root_event_definitions
                            .iter()
                            .find(|d| d.id().is_some_and(|identity| identity.selector() == id)),
                        _ => None,
                    })
                    .collect();
                let known: Vec<_> = definitions.iter().chain(resolved.iter().copied()).collect();
                let unsupported = known.iter().any(|definition| match n.kind {
                    NodeKind::StartEvent => !matches!(
                        definition,
                        EventDefinition::Timer { .. }
                            | EventDefinition::Conditional { .. }
                            | EventDefinition::Signal { .. }
                    ),
                    NodeKind::EndEvent => !matches!(definition, EventDefinition::Terminate { .. }),
                    NodeKind::IntermediateCatchEvent => !matches!(
                        definition,
                        EventDefinition::Timer { .. }
                            | EventDefinition::Conditional { .. }
                            | EventDefinition::Signal { .. }
                            | EventDefinition::Link { .. }
                    ),
                    NodeKind::IntermediateThrowEvent => {
                        !matches!(definition, EventDefinition::Link { .. })
                    }
                    NodeKind::BoundaryEvent => !matches!(
                        definition,
                        EventDefinition::Message { .. }
                            | EventDefinition::Timer { .. }
                            | EventDefinition::Cancel { .. }
                            | EventDefinition::Compensate { .. }
                            | EventDefinition::Conditional { .. }
                            | EventDefinition::Signal { .. }
                    ),
                    _ => false,
                });
                if unsupported {
                    issues.push((n.id.selector(), "choreographyEventDefinitionWrongContext", "This event definition is not applicable in this choreography event context; BPMN 2.0.2 §11.6, Tables 11.6–11.8."));
                }
                if n.kind == NodeKind::BoundaryEvent {
                    if known
                        .iter()
                        .any(|d| matches!(d, EventDefinition::Cancel { .. }))
                        && *cancel_activity == Some(false)
                    {
                        issues.push((n.id.selector(), "choreographyCancelBoundaryNonInterrupting", "A choreography boundary Cancel Event must interrupt; BPMN 2.0.2 Table 11.7."));
                    }
                    if known
                        .iter()
                        .any(|d| matches!(d, EventDefinition::Message { .. }))
                        && let Some(reference) = attached_to
                        && let ReferenceResolution::Resolved(id) = index.resolve(reference)
                        && let Some(activity) =
                            scope.activities.iter().find(|a| a.id.selector() == id)
                        && activity.details.is_some()
                    {
                        issues.push((n.id.selector(), "choreographyMessageBoundaryWrongActivity", "A choreography boundary Message Event applies only to a Choreography Task; BPMN 2.0.2 Table 11.7."));
                    }
                }
                if n.kind == NodeKind::EndEvent && known.len() > 1 {
                    issues.push((n.id.selector(), "choreographyEndMultipleWrongContext", "A choreography End Event cannot have multiple results; BPMN 2.0.2 Table 11.8."));
                }
                if n.kind == NodeKind::StartEvent
                    && known.len() > 1
                    && known.iter().any(|d| {
                        !matches!(
                            d,
                            EventDefinition::Timer { .. } | EventDefinition::Signal { .. }
                        )
                    })
                {
                    issues.push((n.id.selector(), "choreographyStartMultipleWrongContext", "Multiple choreography start triggers may contain only signals or timers; BPMN 2.0.2 Table 11.6."));
                }
            }
        }
        for n in &scope.gateways {
            if !matches!(
                n.kind,
                NodeKind::ExclusiveGateway
                    | NodeKind::ParallelGateway
                    | NodeKind::InclusiveGateway
                    | NodeKind::ComplexGateway
                    | NodeKind::EventBasedGateway
            ) {
                issues.push((
                    n.id.selector(),
                    "choreographyGatewayWrongContext",
                    "This node is not a gateway; BPMN 2.0.2 §11.4.",
                ));
            }
        }
    }
    issues
}

fn io_incomplete(io: &crate::bpmn_data::IoSpecification) -> bool {
    if io.input_sets.is_empty() || io.output_sets.is_empty() {
        return true;
    }
    for (sets, ports, other) in [
        (&io.input_sets, &io.inputs, &io.output_sets),
        (&io.output_sets, &io.outputs, &io.input_sets),
    ] {
        if ports.iter().any(|port| {
            port.id.as_ref().is_none_or(|id| {
                !sets
                    .iter()
                    .any(|set| set.data_refs.contains(&id.selector()))
            })
        }) {
            return true;
        }
        for set in sets {
            if set
                .optional_refs
                .iter()
                .chain(&set.while_executing_refs)
                .any(|id| !set.data_refs.contains(id))
            {
                return true;
            }
            if set.counterpart_refs.iter().any(|id| {
                !other.iter().any(|counterpart| {
                    counterpart
                        .id
                        .as_ref()
                        .is_some_and(|value| value.selector() == *id)
                        && set.id.as_ref().is_some_and(|own| {
                            counterpart.counterpart_refs.contains(&own.selector())
                        })
                })
            }) {
                return true;
            }
        }
    }
    false
}

fn resource_role_incomplete(role: &crate::bpmn_activity::ResourceRole) -> bool {
    use crate::bpmn_activity::ResourceRoleAssignment;
    match &role.assignment {
        ResourceRoleAssignment::None => false,
        ResourceRoleAssignment::Resource { bindings, .. } => bindings
            .iter()
            .any(|binding| binding.parameter_ref.is_none() || binding.expression.is_none()),
        ResourceRoleAssignment::Expression { assignment } => assignment.expression.is_none(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub document_id: DocumentId,
    pub element_id: Option<ElementSelector>,
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
}
impl ProjectData {
    /// Reports bounded profile checks and advisory incomplete-model conditions; this is not BPMN certification.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut results = Vec::new();
        for doc in &self.documents {
            let mut emit = |id: ElementSelector, code, message: &str| {
                results.push(Diagnostic {
                    document_id: doc.id,
                    element_id: Some(id),
                    severity: DiagnosticSeverity::Warning,
                    code,
                    message: message.into(),
                })
            };
            for issue in activity_family_reference_issues(doc) {
                emit(
                    issue.owner,
                    if issue.field == "di.bpmnElement" {
                        "danglingPlaneElement"
                    } else {
                        "unresolvedActivityFamilyReference"
                    },
                    &format!(
                        "Unresolved local {} reference (editing advisory); BPMN 2.0.2 §§8.4.2, 10.3–10.4.",
                        issue.field
                    ),
                );
            }
            if doc
                .extensions
                .iter()
                .any(|extension| extension.must_understand == Some(true))
            {
                emit(
                    doc.definitions_id.selector(),
                    "requiredExtensionUnverified",
                    "A required extension's semantics are not verified; BPMN 2.0.2 §8.3.3, Table 8.7.",
                );
            }
            for relationship in &doc.relationships {
                if relationship.sources.is_empty() || relationship.targets.is_empty() {
                    emit(
                        relationship
                            .id
                            .as_ref()
                            .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                        "incompleteRelationship",
                        "Relationship needs at least one source and target (editing advisory); BPMN 2.0.2 §8.3.4, Table 8.15.",
                    );
                }
            }
            if !matches!(doc.capability, Capability::Editable) {
                emit(
                    doc.definitions_id.selector(),
                    "semanticCoverageLimited",
                    "Retained or preserved source cannot be fully checked; BPMN 2.0.2 §2.2.4–2.2.8.",
                );
            }
            for expression in doc
                .expressions()
                .into_iter()
                .filter(|e| e.plain_text().is_some_and(|text| text.trim().is_empty()))
                .take(10_000)
            {
                let id = match expression {
                    crate::bpmn_model::Expression::Structured(data) => data
                        .id
                        .as_ref()
                        .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                    crate::bpmn_model::Expression::Legacy(_) => doc.definitions_id.selector(),
                };
                emit(
                    id,
                    "emptyExpression",
                    "Expression has no content (editing advisory); BPMN 2.0.2 §8.4.6.",
                );
            }
            if let Some(collaboration) = &doc.collaboration {
                emit(
                    collaboration.id.selector(),
                    "collaborationCoverageLimited",
                    "Collaboration and conversation semantics are only partially checked; BPMN 2.0.2 Clause 9.",
                );
            }
            for collaboration in doc
                .collaboration
                .iter()
                .chain(&doc.additional_collaborations)
            {
                if collaboration.kind
                    == Some(crate::bpmn_collaboration::CollaborationRootKind::GlobalConversation)
                {
                    if !collaboration.conversation_nodes.is_empty() {
                        emit(
                            collaboration.id.selector(),
                            "globalConversationHasNodes",
                            "A GlobalConversation must not contain ConversationNodes; BPMN 2.0.2 §9.5.5.",
                        );
                    }
                    if collaboration
                        .extras
                        .as_ref()
                        .is_some_and(|extras| !extras.choreography_refs.is_empty())
                    {
                        emit(
                            collaboration.id.selector(),
                            "globalConversationChoreographyRef",
                            "choreographyRef does not apply to GlobalConversation; BPMN 2.0.2 §9.5.5.",
                        );
                    }
                }
            }
            let reference_index = crate::reference_resolution::ReferenceIndex::new(doc);
            for nodes in doc
                .collaboration
                .iter()
                .chain(&doc.additional_collaborations)
                .map(|c| &c.conversation_nodes)
                .chain(doc.choreographies.iter().map(|c| &c.conversation_nodes))
            {
                for node in nodes {
                    if node.kind == crate::bpmn_collaboration::ConversationKind::CallConversation
                        && !node.message_flow_refs.is_empty()
                    {
                        emit(
                            node.id.selector(),
                            "callConversationMessageFlowRef",
                            "messageFlowRef does not apply to CallConversation; BPMN 2.0.2 §9.5.4.",
                        );
                    }
                    if let Some(call) = &node.call
                        && call.participant_associations.iter().any(|association| association.inner.is_some())
                        && !call.called_collaboration_ref.as_ref().is_some_and(|reference| {
                            matches!(reference_index.resolve(reference), crate::reference_resolution::ReferenceResolution::Resolved(selector) if doc.collaborations().any(|collaboration| collaboration.id.selector() == selector))
                        })
                    {
                        emit(node.id.selector(), "associationInnerScopeUnqualified", "Called collaboration ownership is unresolved; participant association inner scope cannot be qualified; BPMN 2.0.2 §9.5.4.");
                    }
                }
            }
            for issue in conversation_link_endpoint_scope_unqualified(doc) {
                emit(
                    issue.owner,
                    "conversationLinkEndpointScopeUnqualified",
                    "Conversation Link endpoint ownership cannot be established in this collaboration; BPMN 2.0.2 §9.5.6.",
                );
            }
            for issue in message_flow_endpoint_scope_unqualified(doc) {
                emit(
                    issue.owner,
                    "messageFlowEndpointScopeUnqualified",
                    "Message Flow endpoint ownership cannot be established in this collaboration; BPMN 2.0.2 §9.4.",
                );
            }
            for issue in association_inner_scope_unqualified(doc) {
                emit(
                    issue.owner,
                    "associationInnerScopeUnqualified",
                    "Referenced inner association ownership cannot be established from modeled references; BPMN 2.0.2 §§9.3.1, 9.4.2 and 9.5.8.",
                );
            }
            for (owner, code, message) in choreography_graph_diagnostics(doc) {
                emit(owner, code, message);
            }
            for choreography in &doc.choreographies {
                emit(
                    choreography.id.selector(),
                    "choreographyCoverageLimited",
                    "Choreography semantics are only partially checked; BPMN 2.0.2 §2.5 and Clause 11.",
                );
            }
            let participants = doc
                .collaboration
                .iter()
                .flat_map(|collaboration| &collaboration.participants)
                .chain(
                    doc.choreographies
                        .iter()
                        .flat_map(|choreography| &choreography.participants),
                );
            for participant in participants {
                if let Some(multiplicity) = &participant.multiplicity {
                    let minimum = multiplicity.minimum.unwrap_or(0);
                    let maximum = multiplicity.maximum.unwrap_or(1);
                    if maximum < 1 || maximum < minimum {
                        emit(
                            multiplicity
                                .id
                                .as_ref()
                                .map_or_else(|| participant.id.selector(), |id| id.selector()),
                            "participantMultiplicityRange",
                            "Participant maximum must be at least one and no less than minimum; BPMN 2.0.2 §9.3.1, Table 9.5.",
                        );
                    }
                }
            }
            for diagram in &doc.diagrams {
                for shape in &diagram.shapes {
                    if shape.participant_band_kind.is_none()
                        && (shape.is_message_visible.is_some()
                            || shape.choreography_activity_shape.is_some())
                    {
                        emit(
                            shape.id.selector(),
                            "participantBandMetadataWithoutBand",
                            "Message visibility and choreography shape references apply to participant bands; BPMN 2.0.2 §12.2.3.4.",
                        );
                    }
                    if shape.participant_band_kind.is_some()
                        && let Some(crate::model::DiReference::Local(reference)) =
                            &shape.choreography_activity_shape
                        && let Some(owner) = diagram
                            .shapes
                            .iter()
                            .find(|candidate| candidate.id.selector() == *reference)
                    {
                        let a = &shape.bounds;
                        let b = &owner.bounds;
                        if a.x + a.width < b.x
                            || b.x + b.width < a.x
                            || a.y + a.height < b.y
                            || b.y + b.height < a.y
                        {
                            emit(
                                shape.id.selector(),
                                "participantBandGeometryDisjoint",
                                "Participant band and owning choreography shape do not touch (layout advisory); BPMN 2.0.2 §12.3.2, Table 12.33.",
                            );
                        }
                        if shape.is_message_visible == Some(true)
                            && let Some((choreography, activity)) =
                                doc.choreographies.iter().find_map(|c| {
                                    queries::choreography_scopes(doc).into_iter()
                                        .filter(|scope| scope.root.id == c.id)
                                        .flat_map(|scope| scope.activities)
                                        .find(|activity| activity.id.selector() == owner.element)
                                        .map(|activity| (c, activity))
                                })
                            && activity.participant_refs.iter().any(|r| matches!(crate::reference_resolution::ReferenceIndex::new(doc).resolve(r), crate::reference_resolution::ReferenceResolution::Resolved(id) if id == shape.element))
                        {
                            use crate::model::ParticipantBandKind;
                            if matches!(
                                shape.participant_band_kind,
                                Some(
                                    ParticipantBandKind::MiddleInitiating
                                        | ParticipantBandKind::MiddleNonInitiating
                                )
                            ) {
                                emit(
                                    shape.id.selector(),
                                    "participantBandMiddleMessage",
                                    "A middle participant band cannot show a message envelope; BPMN 2.0.2 §12.2.3.6.",
                                );
                            }
                            let flows: Option<Vec<_>> = activity
                                .message_flow_refs
                                .iter()
                                .map(|reference| {
                                    choreography.message_flows.iter().find(|flow| {
                                        matches!(crate::reference_resolution::ReferenceIndex::new(doc).resolve(reference), crate::reference_resolution::ReferenceResolution::Resolved(id) if flow.id.selector() == id)
                                            && flow.kind == FlowKind::Message
                                    })
                                })
                                .collect();
                            if let Some(flows) = flows
                                && !flows.is_empty()
                                && flows.iter().all(|flow| {
                                    matches!(
                                        flow.source,
                                        Some(crate::bpmn_model::EventReference::Local(_))
                                    )
                                })
                                && !flows.iter().any(|flow| {
                                    flow.source.as_ref()
                                        == Some(&crate::bpmn_model::EventReference::Local(
                                            shape.element.clone(),
                                        ))
                                })
                            {
                                emit(
                                    shape.id.selector(),
                                    "participantBandMessageMissing",
                                    "The visible message envelope has no task message flow sent by this participant; BPMN 2.0.2 §11.5.1.",
                                );
                            }
                        }
                    }
                    if shape.participant_band_kind.is_some()
                        && shape.choreography_activity_shape.is_none()
                    {
                        emit(
                            shape.id.selector(),
                            "participantBandMissingOwner",
                            "Participant band needs its owning choreography activity shape (editing advisory); BPMN 2.0.2 §12.2.3.4.",
                        );
                    }
                    if shape.participant_band_kind.is_some()
                        && !doc.choreographies.iter().any(|choreography| {
                            choreography
                                .participants
                                .iter()
                                .any(|participant| participant.id.selector() == shape.element)
                        })
                    {
                        emit(
                            shape.id.selector(),
                            "participantBandInvalidParticipant",
                            "A participant band should depict a choreography participant; BPMN 2.0.2 §12.2.3.4.",
                        );
                    }
                }
                for edge in &diagram.edges {
                    if edge.message_visible_kind.is_some() {
                        let is_message = doc
                            .collaboration
                            .iter()
                            .flat_map(|c| &c.flows)
                            .chain(doc.choreographies.iter().flat_map(|c| &c.message_flows))
                            .any(|flow| {
                                flow.id.selector() == edge.element && flow.kind == FlowKind::Message
                            });
                        if !is_message {
                            emit(
                                edge.id.selector(),
                                "messageDecoratorWrongElement",
                                "Message visibility applies only to Message Flow; BPMN 2.0.2 §12.2.3.6.",
                            );
                        }
                    }
                }
            }
            if !doc.data_stores.is_empty() {
                emit(
                    doc.definitions_id.selector(),
                    "dataCoverageLimited",
                    "Data semantics and assignment rules are only partially checked; BPMN 2.0.2 §10.3.",
                );
            }
            for property in &doc.correlation_properties {
                let owner = property
                    .id
                    .as_ref()
                    .map_or_else(|| doc.definitions_id.selector(), |id| id.selector());
                if property.retrieval_expressions.is_empty() {
                    emit(
                        owner.clone(),
                        "incompleteCorrelationProperty",
                        "Correlation property has no retrieval expression (editing advisory); BPMN 2.0.2 §8.4.2, Table 8.32.",
                    );
                }
                for retrieval in &property.retrieval_expressions {
                    if retrieval.message_ref.is_none() || retrieval.message_path.is_none() {
                        emit(
                            retrieval
                                .id
                                .as_ref()
                                .map_or_else(|| owner.clone(), |id| id.selector()),
                            "incompleteCorrelationRetrieval",
                            "Correlation retrieval needs a message reference and message path (editing advisory); BPMN 2.0.2 §8.4.2, Table 8.33.",
                        );
                    }
                }
            }
            let process_ids: HashSet<_> = doc.processes.iter().map(|p| p.id.selector()).collect();
            for task in &doc.global_tasks {
                if matches!(
                    task.kind,
                    crate::bpmn_activity::GlobalTaskKind::CallableElement
                ) && !task.resource_roles.is_empty()
                {
                    emit(
                        task.id
                            .as_ref()
                            .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                        "callableHasTaskResourceRoles",
                        "Generic callable elements do not own task resource roles (editing advisory); BPMN 2.0.2 §10.3.6–10.3.7.",
                    );
                }
            }
            for task in &doc.global_tasks {
                let owner = task
                    .id
                    .as_ref()
                    .map_or_else(|| doc.definitions_id.selector(), |id| id.selector());
                for binding in &task.callable.io_bindings {
                    if binding.operation_ref.is_none()
                        || binding.input_data_ref.is_none()
                        || binding.output_data_ref.is_none()
                    {
                        emit(
                            binding
                                .id
                                .as_ref()
                                .map_or_else(|| owner.clone(), |id| id.selector()),
                            "incompleteIoBinding",
                            "Callable I/O binding needs operation, input and output references (editing advisory); BPMN 2.0.2 §10.3.6, Table 10.25.",
                        );
                    }
                }
                for role in &task.resource_roles {
                    if resource_role_incomplete(role) {
                        emit(
                            role.id
                                .as_ref()
                                .map_or_else(|| owner.clone(), |id| id.selector()),
                            "incompleteResourceAssignment",
                            "Resource assignment or parameter binding is incomplete (editing advisory); BPMN 2.0.2 §10.3.1, Tables 10.6–10.7.",
                        );
                    }
                }
            }
            for process in &doc.processes {
                if let Some(metadata) = &process.activity_metadata {
                    for role in &metadata.resource_roles {
                        if resource_role_incomplete(role) {
                            emit(
                                role.id
                                    .as_ref()
                                    .map_or_else(|| process.id.selector(), |id| id.selector()),
                                "incompleteResourceAssignment",
                                "Resource assignment or parameter binding is incomplete (editing advisory); BPMN 2.0.2 §10.3.1, Tables 10.6–10.7.",
                            );
                        }
                    }
                    for binding in &metadata.callable.io_bindings {
                        if binding.operation_ref.is_none()
                            || binding.input_data_ref.is_none()
                            || binding.output_data_ref.is_none()
                        {
                            emit(
                                binding
                                    .id
                                    .as_ref()
                                    .map_or_else(|| process.id.selector(), |id| id.selector()),
                                "incompleteIoBinding",
                                "Callable I/O binding needs operation, input and output references (editing advisory); BPMN 2.0.2 §10.3.6, Table 10.25.",
                            );
                        }
                    }
                    for subscription in &metadata.correlation_subscriptions {
                        let owner = subscription
                            .id
                            .as_ref()
                            .map_or_else(|| process.id.selector(), |id| id.selector());
                        if subscription.key_ref.is_none() {
                            emit(
                                owner.clone(),
                                "incompleteCorrelationSubscription",
                                "Correlation subscription needs a key reference (editing advisory); BPMN 2.0.2 §8.4.2, Table 8.34.",
                            );
                        }
                        for binding in &subscription.bindings {
                            if binding.property_ref.is_none() || binding.data_path.is_none() {
                                emit(
                                    binding
                                        .id
                                        .as_ref()
                                        .map_or_else(|| owner.clone(), |id| id.selector()),
                                    "incompleteCorrelationBinding",
                                    "Correlation binding needs a property reference and data path (editing advisory); BPMN 2.0.2 §8.4.2, Table 8.35.",
                                );
                            }
                        }
                    }
                }
                if process.io_specification.as_ref().is_some_and(io_incomplete) {
                    emit(
                        process.id.selector(),
                        "incompleteIoSpecification",
                        "I/O sets or port membership are incomplete; BPMN 2.0.2 §10.4.1, Tables 10.61–10.62.",
                    );
                }
            }
            for issue in crate::validation_data::data_issues(doc) {
                emit(issue.owner, issue.code, &issue.message);
            }
            for issue in crate::validation_flow::flow_issues(doc) {
                emit(issue.owner, issue.code, &issue.message);
            }
            let scopes = queries::scopes(doc);
            let ad_hoc_scopes: HashSet<_> = scopes
                .iter()
                .flat_map(|scope| scope.nodes)
                .filter(|node| node.kind == NodeKind::AdHocSubProcess)
                .map(|node| node.id.selector())
                .collect();
            let event_subprocess_scopes: HashSet<_> = scopes
                .iter()
                .flat_map(|scope| scope.nodes)
                .filter(|node| {
                    node.activity
                        .as_ref()
                        .is_some_and(|a| a.triggered_by_event == Some(true))
                })
                .map(|node| node.id.selector())
                .collect();
            let mut output_sets: Vec<&crate::bpmn_data::IoSet> = Vec::new();
            for process in &doc.processes {
                if let Some(io) = &process.io_specification {
                    output_sets.extend(&io.output_sets);
                }
            }
            for task in &doc.global_tasks {
                if let Some(io) = &task.io_specification {
                    output_sets.extend(&io.output_sets);
                }
            }
            for scope in &scopes {
                for node in scope.nodes {
                    if let Some(io) = &node.io_specification {
                        output_sets.extend(&io.output_sets);
                    }
                    if let Some(NodeDetails::Event {
                        io:
                            Some(EventIo::Catch {
                                output_set: Some(set),
                                ..
                            }),
                        ..
                    }) = &node.details
                    {
                        output_sets.push(set);
                    }
                }
            }
            let target_namespace = doc
                .definitions
                .as_ref()
                .map(|d| d.target_namespace.as_str())
                .or_else(|| {
                    doc.provenance
                        .is_none()
                        .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
                });
            for definition in &doc.root_event_definitions {
                if matches!(
                    definition,
                    EventDefinition::Timer {
                        kind: None,
                        expression: None,
                        ..
                    }
                ) {
                    emit(
                        definition
                            .id()
                            .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                        "incompleteTimerDefinition",
                        "Timer definition has no date, duration or cycle (editing advisory); BPMN 2.0.2 §10.5.5, Table 10.101.",
                    );
                }
            }
            for scope in &scopes {
                for association in scope.data_associations {
                    if association.transformation.is_none() && association.sources.len() != 1 {
                        emit(
                            association
                                .id
                                .as_ref()
                                .map_or_else(|| scope.id.selector(), |id| id.selector()),
                            "dataAssociationSourceCount",
                            "A data association without transformation should have one source; BPMN 2.0.2 §10.4.1.",
                        );
                    }
                }
                if !scope.data_objects.is_empty()
                    || !scope.data_associations.is_empty()
                    || scope.nodes.iter().any(|n| n.io_specification.is_some())
                {
                    emit(
                        scope.id.selector(),
                        "dataCoverageLimited",
                        "Data and I/O association semantics are only partially checked; BPMN 2.0.2 §10.3.",
                    );
                }
                let mut incoming: HashMap<&ElementSelector, Vec<_>> = HashMap::new();
                let mut outgoing: HashMap<&ElementSelector, Vec<_>> = HashMap::new();
                for flow in scope.flows {
                    if flow.message_ref.is_some() && flow.kind != FlowKind::Message {
                        emit(
                            flow.id.selector(),
                            "messageReferenceWrongFlow",
                            "messageRef applies to Message Flow; BPMN 2.0.2 §9.4, Table 9.8.",
                        );
                    }
                }
                for flow in scope.flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
                    if let Some(crate::bpmn_model::EventReference::Local(target)) = &flow.target {
                        incoming.entry(target).or_default().push(flow);
                    }
                    if let Some(crate::bpmn_model::EventReference::Local(source)) = &flow.source {
                        outgoing.entry(source).or_default().push(flow);
                    }
                }
                let mut has_start = false;
                let mut has_end = false;
                for node in scope.nodes {
                    if let Some(io) = &node.io_specification {
                        if io_incomplete(io) {
                            emit(
                                node.id.selector(),
                                "incompleteIoSpecification",
                                "I/O sets or port membership are incomplete; BPMN 2.0.2 §10.4.1, Tables 10.61–10.62.",
                            );
                        }
                        if matches!(
                            node.kind,
                            NodeKind::SubProcess
                                | NodeKind::AdHocSubProcess
                                | NodeKind::Transaction
                        ) {
                            emit(
                                node.id.selector(),
                                "subprocessDirectIoUnverified",
                                "Embedded subprocess I/O requires review of the direct versus multi-instance mapping; BPMN 2.0.2 §10.4.1.",
                            );
                        }
                    }
                    if matches!(
                        &node.details,
                        Some(NodeDetails::CallActivity {
                            called_element: None
                        })
                    ) {
                        emit(
                            node.id.selector(),
                            "incompleteCallActivity",
                            "Call activity has no callable target (editing advisory); BPMN 2.0.2 §10.3.6, Table 10.23.",
                        );
                    }
                    if let Some(crate::bpmn_model::LoopCharacteristics::MultiInstance {
                        id,
                        cardinality,
                        loop_data_input_ref,
                        complex_behavior_definitions,
                        ..
                    }) = &node.loop_characteristics
                    {
                        let owner = id
                            .as_ref()
                            .map_or_else(|| node.id.selector(), |id| id.selector());
                        if cardinality.is_none() && loop_data_input_ref.is_none() {
                            emit(
                                owner.clone(),
                                "incompleteMultiInstanceInput",
                                "Multi-instance activity needs a loop cardinality or loop data input to initialize (editing advisory); BPMN 2.0.2 §10.3.8, Table 10.29.",
                            );
                        }
                        for behavior in complex_behavior_definitions {
                            if let Some(event) = &behavior.event {
                                if let Some(input_set) = &event.input_set {
                                    for reference in &input_set.counterpart_refs {
                                        if let Some(output_set) = output_sets.iter().find(|set| {
                                            set.id
                                                .as_ref()
                                                .is_some_and(|id| id.selector() == *reference)
                                        }) && input_set.id.as_ref().is_none_or(|id| {
                                            !output_set.counterpart_refs.contains(&id.selector())
                                        }) {
                                            emit(
                                                input_set
                                                    .id
                                                    .as_ref()
                                                    .or(event.id.as_ref())
                                                    .or(behavior.id.as_ref())
                                                    .map_or_else(
                                                        || owner.clone(),
                                                        |id| id.selector(),
                                                    ),
                                                "implicitIoSetPairIncomplete",
                                                "Input and output set references are not paired (editing advisory); BPMN 2.0.2 §10.4.1, Tables 10.61–10.62.",
                                            );
                                        }
                                    }
                                }
                                let roots = event.definition_refs.iter().filter_map(|reference| {
                                    doc.root_event_definitions.iter().find(|definition| {
                                        definition.id().is_some_and(|id| {
                                            reference.matches_identity(id, target_namespace)
                                        })
                                    })
                                });
                                if event.definitions.iter().chain(roots).any(|definition| {
                                    matches!(
                                        definition,
                                        EventDefinition::Timer { .. }
                                            | EventDefinition::Conditional { .. }
                                            | EventDefinition::Cancel { .. }
                                            | EventDefinition::Terminate { .. }
                                            | EventDefinition::Link { .. }
                                    )
                                }) {
                                    emit(
                                        event
                                            .id
                                            .as_ref()
                                            .or(behavior.id.as_ref())
                                            .map_or_else(|| owner.clone(), |id| id.selector()),
                                        "implicitEventTriggerContextUnverified",
                                        "Implicit throw event trigger needs context review (editing advisory); BPMN 2.0.2 §10.3.8 Table 10.31 and §10.5.5.",
                                    );
                                }
                            }
                            if behavior.condition.is_none() {
                                emit(
                                    behavior
                                        .id
                                        .as_ref()
                                        .map_or_else(|| owner.clone(), |id| id.selector()),
                                    "incompleteComplexBehaviorCondition",
                                    "Complex behavior needs a condition (editing advisory); BPMN 2.0.2 §10.3.8, Table 10.31.",
                                );
                            }
                        }
                    }
                    if let Some(activity) = &node.activity {
                        for role in &activity.resource_roles {
                            if resource_role_incomplete(role) {
                                emit(
                                    role.id
                                        .as_ref()
                                        .map_or_else(|| node.id.selector(), |id| id.selector()),
                                    "incompleteResourceAssignment",
                                    "Resource assignment or parameter binding is incomplete (editing advisory); BPMN 2.0.2 §10.3.1, Tables 10.6–10.7.",
                                );
                            }
                        }
                        for issue in activity_reference_issues(doc, activity) {
                            emit(
                                node.id.selector(),
                                issue,
                                if issue.starts_with("invalidActivity") {
                                    "Activity reference has a namespace that cannot be represented in XML (editing advisory)."
                                } else {
                                    "Activity has an unresolved local message or operation reference (editing advisory); BPMN 2.0.2 §10.3."
                                },
                            );
                        }
                        if activity.message_ref.is_some() && activity.operation_ref.is_some() {
                            emit(
                                node.id.selector(),
                                "activityBindingCompatibilityUnverified",
                                "Message and operation I/O compatibility is not fully checked; BPMN 2.0.2 §10.3.3–10.3.4.",
                            );
                        }
                    }
                    let ins = incoming
                        .get(&node.id.selector())
                        .map_or(&[][..], Vec::as_slice);
                    let outs = outgoing
                        .get(&node.id.selector())
                        .map_or(&[][..], Vec::as_slice);
                    if node.kind == NodeKind::ReceiveTask
                        && node
                            .activity
                            .as_ref()
                            .is_some_and(|activity| activity.instantiate == Some(true))
                        && !ins.is_empty()
                    {
                        emit(
                            node.id.selector(),
                            "instantiatingReceiveIncoming",
                            "An instantiating Receive Task must not have incoming sequence flow; BPMN 2.0.2 §10.3.3, Table 10.10.",
                        );
                    }
                    if let Some(activity) = &node.activity
                        && let Some(message_ref) = &activity.message_ref
                        && matches!(node.kind, NodeKind::SendTask | NodeKind::ReceiveTask)
                        && let Some(io) = &node.io_specification
                    {
                        let (ports, sets) = if node.kind == NodeKind::SendTask {
                            (&io.inputs, &io.input_sets)
                        } else {
                            (&io.outputs, &io.output_sets)
                        };
                        if ports.len() > 1 || sets.len() > 1 {
                            emit(
                                node.id.selector(),
                                "messageTaskIoCardinality",
                                "A message-bound Send/Receive Task has at most one corresponding data port and set; BPMN 2.0.2 §10.3.3, Tables 10.9–10.10.",
                            );
                        }
                        let message_type =
                            doc.event_resources
                                .iter()
                                .find_map(|resource| match resource {
                                    EventDefinitionResource::Message { id, item_ref, .. }
                                        if target_namespace
                                            == Some(message_ref.namespace.as_str())
                                            && id.xml_id() == Some(&message_ref.local) =>
                                    {
                                        item_ref.as_ref()
                                    }
                                    _ => None,
                                });
                        if let (Some(message_type), Some(port_type)) = (
                            message_type,
                            ports
                                .first()
                                .and_then(|port| port.item_aware.as_ref())
                                .and_then(|item| item.item_subject_ref.as_ref()),
                        ) && message_type != port_type
                            && target_namespace == Some(message_type.namespace.as_str())
                            && target_namespace == Some(port_type.namespace.as_str())
                            && doc.catalog.item_definitions.iter().any(|item| {
                                item.id.as_ref().and_then(|id| id.xml_id())
                                    == Some(&message_type.local)
                            })
                            && doc.catalog.item_definitions.iter().any(|item| {
                                item.id.as_ref().and_then(|id| id.xml_id())
                                    == Some(&port_type.local)
                            })
                        {
                            emit(
                                node.id.selector(),
                                "messageTaskItemEquivalenceUnverified",
                                "Message and task port reference different local item definitions; their equivalence needs review; BPMN 2.0.2 §10.3.3, Tables 10.9–10.10.",
                            );
                        }
                    }

                    if let Some(NodeDetails::Event {
                        definitions,
                        definition_refs,
                        parallel_multiple,
                        is_interrupting,
                        io,
                        ..
                    }) = &node.details
                    {
                        let resolved: Vec<_> = definition_refs
                            .iter()
                            .filter_map(|reference| {
                                doc.root_event_definitions.iter().find(|definition| {
                                    definition.id().is_some_and(|id| {
                                        reference.matches_identity(id, target_namespace)
                                    })
                                })
                            })
                            .collect();
                        if definition_refs
                            .iter()
                            .any(|reference| reference.is_external(target_namespace))
                        {
                            emit(
                                node.id.selector(),
                                "externalEventDefinitionUnverified",
                                "External event definition cannot be checked locally; BPMN 2.0.2 §10.5.1, Tables 10.82–10.83.",
                            );
                        }
                        if *parallel_multiple == Some(true)
                            && definitions.len() + definition_refs.len() <= 1
                        {
                            emit(
                                node.id.selector(),
                                "parallelMultipleNotApplicable",
                                "parallelMultiple is relevant only when a catch event has multiple definitions; BPMN 2.0.2 §10.5.1, Table 10.82.",
                            );
                        }
                        let event_subprocess =
                            event_subprocess_scopes.contains(&scope.id.selector());
                        if node.kind == NodeKind::StartEvent
                            && is_interrupting.is_some()
                            && !event_subprocess
                        {
                            emit(
                                node.id.selector(),
                                "startInterruptingIgnored",
                                "isInterrupting applies only to event-subprocess starts and is ignored elsewhere; BPMN 2.0.2 §10.5.2, Table 10.87.",
                            );
                        }
                        let all_definitions = definitions.iter().chain(resolved);
                        for definition in all_definitions {
                            if matches!(
                                definition,
                                EventDefinition::Timer {
                                    kind: None,
                                    expression: None,
                                    ..
                                }
                            ) {
                                emit(
                                    node.id.selector(),
                                    "incompleteTimerDefinition",
                                    "Timer definition has no date, duration or cycle (editing advisory); BPMN 2.0.2 §10.5.5, Table 10.101.",
                                );
                            }
                            if node.kind == NodeKind::StartEvent
                                && event_subprocess
                                && is_interrupting.is_some()
                                && (matches!(definition, EventDefinition::Compensate { .. })
                                    || (*is_interrupting == Some(false)
                                        && matches!(definition, EventDefinition::Error { .. })))
                            {
                                emit(
                                    node.id.selector(),
                                    "startInterruptingTriggerMismatch",
                                    "Error starts always interrupt; interruption does not apply to compensation starts; BPMN 2.0.2 §10.5.2, Table 10.87.",
                                );
                            }
                        }
                        let incomplete_io = match io {
                            Some(EventIo::Catch {
                                outputs,
                                output_set,
                            }) => !outputs.is_empty() && output_set.is_none(),
                            Some(EventIo::Throw { inputs, input_set }) => {
                                !inputs.is_empty() && input_set.is_none()
                            }
                            None => false,
                        };
                        if incomplete_io {
                            emit(
                                node.id.selector(),
                                "incompleteEventIo",
                                "Event data ports have no corresponding set (editing advisory); BPMN 2.0.2 §10.5.1, Tables 10.82–10.83.",
                            );
                        }
                    }
                    if let Some(NodeDetails::Event { definitions, .. }) = &node.details {
                        for definition in definitions {
                            if let EventDefinition::Link {
                                id,
                                name,
                                sources,
                                target,
                                ..
                            } = definition
                            {
                                if id.is_none()
                                    || name.trim().is_empty()
                                    || (node.kind == NodeKind::IntermediateCatchEvent
                                        && sources.is_empty())
                                    || (node.kind == NodeKind::IntermediateThrowEvent
                                        && target.is_none())
                                {
                                    emit(
                                        node.id.selector(),
                                        "incompleteLinkPairing",
                                        "Link event pairing is incomplete; BPMN 2.0.2 §10.5.5.",
                                    );
                                }
                                if (node.kind == NodeKind::IntermediateCatchEvent
                                    && !ins.is_empty())
                                    || (node.kind == NodeKind::IntermediateThrowEvent
                                        && !outs.is_empty())
                                {
                                    emit(
                                        node.id.selector(),
                                        "linkSequenceDirection",
                                        "Link catch events have no incoming and link throw events have no outgoing sequence flow; BPMN 2.0.2 §10.5.5.",
                                    );
                                }
                            }
                        }
                    }
                    has_start |= matches!(node.kind, NodeKind::StartEvent);
                    has_end |= matches!(node.kind, NodeKind::EndEvent);
                    // Compensation handlers, event sub-processes and ad-hoc children
                    // legitimately have no sequence flow; BPMN 2.0.2 §§10.3.5, 10.3.6.
                    let unconnected_by_design = node.activity.as_ref().is_some_and(|a| {
                        a.is_for_compensation == Some(true) || a.triggered_by_event == Some(true)
                    }) || ad_hoc_scopes.contains(&scope.id.selector());
                    if ins.is_empty()
                        && outs.is_empty()
                        && !unconnected_by_design
                        && !matches!(
                            node.kind,
                            NodeKind::TextAnnotation
                                | NodeKind::DataObjectReference
                                | NodeKind::DataStoreReference
                                | NodeKind::BoundaryEvent
                        )
                    {
                        emit(
                            node.id.selector(),
                            "unconnectedNode",
                            "Element has no sequence-flow connection (editing advisory).",
                        );
                    }
                    if matches!(node.kind, NodeKind::ExclusiveGateway)
                        && node.name.as_deref().unwrap_or_default().trim().is_empty()
                    {
                        emit(
                            node.id.selector(),
                            "unnamedExclusiveGateway",
                            "Exclusive gateway has no name (editing advisory).",
                        );
                    }
                    if matches!(node.kind, NodeKind::StartEvent) {
                        // Expanded-subprocess boundary start exception cannot be distinguished in this model; only check root scopes.
                        if process_ids.contains(&scope.id.selector()) && !ins.is_empty() {
                            emit(
                                node.id.selector(),
                                "startHasIncoming",
                                "Start Event has incoming sequence flow; BPMN 2.0.2 §10.5.2, p. 244.",
                            );
                        }
                        if outs.iter().any(|flow| flow.condition.is_some()) {
                            emit(
                                node.id.selector(),
                                "startConditionalFlow",
                                "Start Event outgoing sequence flow has a condition; BPMN 2.0.2 §10.5.2, p. 244.",
                            );
                        }
                    }
                    if matches!(node.kind, NodeKind::EndEvent) && !outs.is_empty() {
                        emit(
                            node.id.selector(),
                            "endHasOutgoing",
                            "End Event has outgoing sequence flow; BPMN 2.0.2 §10.5.3, p. 245.",
                        );
                    }
                    let gateway = matches!(
                        node.kind,
                        NodeKind::ExclusiveGateway
                            | NodeKind::ParallelGateway
                            | NodeKind::InclusiveGateway
                            | NodeKind::ComplexGateway
                            | NodeKind::EventBasedGateway
                    );
                    if gateway
                        && ins.len() <= 1
                        && outs.len() <= 1
                        && !(ins.is_empty() && outs.is_empty())
                    {
                        emit(
                            node.id.selector(),
                            "gatewayDoesNotSplitOrMerge",
                            "Gateway has neither multiple incoming nor multiple outgoing flows; BPMN 2.0.2 §10.6.1, p. 289.",
                        );
                    }
                    if let Some(NodeDetails::Gateway {
                        default_flow,
                        direction,
                        instantiate,
                        event_gateway_type,
                        ..
                    }) = &node.details
                    {
                        if let Some(id) = default_flow
                            && outs
                                .iter()
                                .any(|flow| flow.id.selector() == *id && flow.condition.is_some())
                        {
                            emit(
                                id.clone(),
                                "defaultFlowCondition",
                                "A default sequence flow condition is ignored; BPMN 2.0.2 §10.6.2, p. 291.",
                            );
                        }
                        let mismatch = match direction {
                            Some(GatewayDirection::Converging) => ins.len() < 2 || outs.len() > 1,
                            Some(GatewayDirection::Diverging) => outs.len() < 2 || ins.len() > 1,
                            Some(GatewayDirection::Mixed) => ins.len() < 2 || outs.len() < 2,
                            _ => false,
                        };
                        if mismatch {
                            emit(
                                node.id.selector(),
                                "gatewayDirectionMismatch",
                                "Gateway connections do not match its declared direction; BPMN 2.0.2 §10.6.1, p. 289.",
                            );
                        }
                        if matches!(node.kind, NodeKind::EventBasedGateway)
                            && *instantiate == Some(true)
                            && !ins.is_empty()
                        {
                            emit(
                                node.id.selector(),
                                "instantiatingGatewayIncoming",
                                "An instantiating event gateway has incoming sequence flow; BPMN 2.0.2 §10.6.6, p. 298.",
                            );
                        }
                        if matches!(node.kind, NodeKind::EventBasedGateway)
                            && *event_gateway_type == Some(EventGatewayType::Parallel)
                            && *instantiate != Some(true)
                        {
                            emit(
                                node.id.selector(),
                                "parallelEventGatewayNotInstantiating",
                                "A parallel event gateway must instantiate its process; BPMN 2.0.2 §10.6.6, p. 298.",
                            );
                        }
                    }
                    if matches!(node.kind, NodeKind::EventBasedGateway) {
                        if outs.len() < 2 {
                            emit(
                                node.id.selector(),
                                "eventGatewayTooFewOutgoing",
                                "Event-based gateway needs at least two outgoing sequence flows; BPMN 2.0.2 §10.6.6, p. 296.",
                            );
                        }
                        for flow in outs {
                            if flow.condition.is_some() {
                                emit(
                                    flow.id.selector(),
                                    "eventGatewayConditionalFlow",
                                    "Event-based gateway outgoing flow has a condition; BPMN 2.0.2 §10.6.6, p. 296.",
                                );
                            }
                            if let Some(target) = scope.nodes.iter().find(|n| {
                                flow.target.as_ref()
                                    == Some(&crate::bpmn_model::EventReference::Local(
                                        n.id.selector(),
                                    ))
                            }) {
                                if !matches!(
                                    target.kind,
                                    NodeKind::IntermediateCatchEvent | NodeKind::ReceiveTask
                                ) {
                                    emit(
                                        flow.id.selector(),
                                        "eventGatewayInvalidTarget",
                                        "Event-based gateway target is neither an intermediate catch event nor receive task; BPMN 2.0.2 §10.6.6, p. 297.",
                                    );
                                }
                                if incoming
                                    .get(&target.id.selector())
                                    .is_some_and(|flows| flows.len() > 1)
                                {
                                    emit(
                                        target.id.selector(),
                                        "eventGatewayTargetExtraIncoming",
                                        "Event-gateway target has additional incoming sequence flows; BPMN 2.0.2 §10.6.6, p. 297.",
                                    );
                                }
                                if let Some(NodeDetails::Event { definitions, .. }) =
                                    &target.details
                                    && definitions.iter().any(|d| {
                                        !matches!(
                                            d,
                                            EventDefinition::Message { .. }
                                                | EventDefinition::Signal { .. }
                                                | EventDefinition::Timer { .. }
                                                | EventDefinition::Conditional { .. }
                                        )
                                    })
                                {
                                    emit(
                                        target.id.selector(),
                                        "eventGatewayInvalidTrigger",
                                        "Event-gateway target includes an unsupported trigger for this configuration; BPMN 2.0.2 §10.6.6, p. 297.",
                                    );
                                }
                            }
                        }
                    }
                }
                // Advisory only: BPMN permits processes without an explicit Start Event (p. 245).
                if process_ids.contains(&scope.id.selector()) && !has_start {
                    emit(
                        scope.id.selector(),
                        "missingStart",
                        "Process has no explicit start event (editing advisory; BPMN 2.0.2 §10.5.2 permits omission).",
                    );
                }
                if process_ids.contains(&scope.id.selector()) && !has_end {
                    emit(
                        scope.id.selector(),
                        "missingEnd",
                        "Process has no explicit end event (editing advisory).",
                    );
                }
            }
        }
        results
    }
}

/// A stable, value-bearing issue key for comparing local reference problems
/// before and after an edit. Qualified external references remain opaque.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActivityReferenceIssue {
    pub owner: ElementSelector,
    pub field: &'static str,
    pub target: crate::bpmn_activity::ModelReference,
}

/// Find unresolved local activity-family references without resolving external
/// namespaces or synthesizing XML IDs. Incomplete optional fields are reported
/// separately by diagnostics, not as dangling-reference keys.
pub fn activity_family_reference_issues(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceIssue> {
    activity_family_reference_scan(doc).0
}

/// Proven missing or wrong-kind/owner local choreography references. External
/// and ownership-unqualified references are excluded from mutation enforcement.
pub fn choreography_reference_issues(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceIssue> {
    use crate::bpmn_collaboration::ChoreographyActivityDetails;
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    let index = ReferenceIndex::new(doc);
    let mut issues = Vec::new();
    for scope in queries::choreography_scopes(doc) {
        let mut check = |owner: ElementSelector,
                         field: &'static str,
                         r: &crate::bpmn_activity::ModelReference,
                         allowed: &[ElementSelector]| {
            let bad = match index.resolve(r) {
                ReferenceResolution::MissingLocal => true,
                ReferenceResolution::Resolved(id) => !allowed.contains(&id),
                _ => false,
            };
            if bad {
                issues.push(ActivityReferenceIssue {
                    owner,
                    field,
                    target: r.clone(),
                });
            }
        };
        let participants: Vec<_> = scope
            .root
            .participants
            .iter()
            .map(|p| p.id.selector())
            .collect();
        if scope.parent.is_none()
            && let Some(crate::bpmn_collaboration::ChoreographyRootKind::Global {
                initiating_participant_ref: Some(reference),
            }) = &scope.root.kind
        {
            check(
                scope.root.id.selector(),
                "globalChoreography.initiatingParticipantRef",
                reference,
                &participants,
            );
        }
        let messages: Vec<_> = scope
            .root
            .message_flows
            .iter()
            .filter(|f| f.kind == FlowKind::Message)
            .map(|f| f.id.selector())
            .collect();
        let roots: Vec<_> = doc.choreographies.iter().map(|c| c.id.selector()).collect();
        let flow_nodes: Vec<_> = scope
            .activities
            .iter()
            .map(|a| a.id.selector())
            .chain(scope.events.iter().map(|n| n.id.selector()))
            .chain(scope.gateways.iter().map(|n| n.id.selector()))
            .collect();
        for flow in &scope.flows {
            for (field, reference) in [
                ("choreography.sequenceFlow.sourceRef", &flow.source),
                ("choreography.sequenceFlow.targetRef", &flow.target),
            ] {
                if let Some(reference) = reference {
                    check(flow.id.selector(), field, reference, &flow_nodes);
                }
            }
        }
        let categories: Vec<_> = doc
            .categories
            .iter()
            .flat_map(|c| &c.values)
            .map(|v| v.id.selector())
            .collect();
        let base_elements: Vec<_> = doc.base_element_selectors().into_iter().collect();
        for artifact in &scope.artifacts {
            match artifact {
                crate::bpmn_collaboration::CollaborationArtifact::Group(group) => {
                    if let Some(r) = &group.category_value_ref {
                        check(
                            group.id.selector(),
                            "group.categoryValueRef",
                            r,
                            &categories,
                        );
                    }
                }
                crate::bpmn_collaboration::CollaborationArtifact::Association(flow) => {
                    for (field, reference) in [
                        ("association.sourceRef", &flow.source),
                        ("association.targetRef", &flow.target),
                    ] {
                        if let Some(r) = reference {
                            check(flow.id.selector(), field, r, &base_elements);
                        }
                    }
                }
                crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(_) => {}
            }
        }
        let activities: Vec<_> = scope.activities.iter().map(|a| a.id.selector()).collect();
        for event in &scope.events {
            let attachment = match &event.details {
                Some(NodeDetails::Event { attached_to, .. }) => attached_to.as_ref(),
                Some(NodeDetails::BoundaryEvent { attached_to, .. }) => Some(attached_to),
                _ => None,
            };
            if let Some(reference) = attachment {
                check(
                    event.id.selector(),
                    "boundaryEvent.attachedToRef",
                    reference,
                    &activities,
                );
            }
        }
        let metadata = scope
            .activities
            .iter()
            .map(|a| (&a.id, &a.flow_metadata))
            .chain(
                scope
                    .events
                    .iter()
                    .chain(&scope.gateways)
                    .map(|n| (&n.id, &n.flow_metadata)),
            );
        for (id, metadata) in metadata {
            if let Some(metadata) = metadata {
                for (field, refs, incoming) in [
                    ("flow.incoming", &metadata.incoming, true),
                    ("flow.outgoing", &metadata.outgoing, false),
                ] {
                    let flows: Vec<_> = scope
                        .flows
                        .iter()
                        .filter(|f| {
                            f.kind == FlowKind::Sequence
                                && if incoming {
                                    f.target.as_ref()
                                        == Some(&crate::bpmn_activity::ModelReference::Local(
                                            id.selector(),
                                        ))
                                } else {
                                    f.source.as_ref()
                                        == Some(&crate::bpmn_activity::ModelReference::Local(
                                            id.selector(),
                                        ))
                                }
                        })
                        .map(|f| f.id.selector())
                        .collect();
                    for r in refs {
                        check(id.selector(), field, r, &flows);
                    }
                }
                for r in &metadata.category_value_refs {
                    check(id.selector(), "flow.categoryValueRef", r, &categories);
                }
            }
        }
        for flow in &scope.flows {
            if let Some(metadata) = &flow.flow_metadata {
                for r in &metadata.category_value_refs {
                    check(flow.id.selector(), "flow.categoryValueRef", r, &categories);
                }
            }
        }
        for activity in scope.activities {
            let owner = activity.id.selector();
            for r in &activity.participant_refs {
                check(
                    owner.clone(),
                    "choreography.participantRef",
                    r,
                    &participants,
                );
            }
            if let Some(r) = &activity.initiating_participant {
                check(
                    owner.clone(),
                    "choreography.initiatingParticipantRef",
                    r,
                    &participants,
                );
            }
            for r in &activity.message_flow_refs {
                check(owner.clone(), "choreography.messageFlowRef", r, &messages);
            }
            if let Some(ChoreographyActivityDetails::Call {
                called_choreography_ref,
                participant_associations,
            }) = &activity.details
            {
                if let Some(r) = called_choreography_ref {
                    check(
                        owner.clone(),
                        "callChoreography.calledChoreographyRef",
                        r,
                        &roots,
                    );
                }
                let target =
                    called_choreography_ref
                        .as_ref()
                        .and_then(|r| match index.resolve(r) {
                            ReferenceResolution::Resolved(id) => {
                                doc.choreographies.iter().find(|c| c.id.selector() == id)
                            }
                            _ => None,
                        });
                for association in participant_associations {
                    let owner = association
                        .id
                        .as_ref()
                        .map_or_else(|| owner.clone(), |id| id.selector());
                    if let Some(r) = &association.outer {
                        check(
                            owner.clone(),
                            "callChoreography.participantAssociation.outerRef",
                            r,
                            &participants,
                        );
                    }
                    if let (Some(r), Some(target)) = (&association.inner, target) {
                        let ids: Vec<_> = target
                            .participants
                            .iter()
                            .map(|p| p.id.selector())
                            .collect();
                        check(
                            owner,
                            "callChoreography.participantAssociation.innerRef",
                            r,
                            &ids,
                        );
                    }
                }
            }
        }
    }
    issues
}

/// Inner association references whose referenced owner cannot be proven from
/// currently modeled choreography and CallConversation paths. External QNames
/// are inert and excluded. Caller and called process definitional collaboration
/// references are followed when their local ownership can be established.
pub fn association_inner_scope_unqualified(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceIssue> {
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    let index = ReferenceIndex::new(doc);
    let mut result = Vec::new();
    for (root, nodes, extras, owner_participants) in doc
        .collaborations()
        .map(|c| (&c.id, &c.conversation_nodes, &c.extras, &c.participants))
        .chain(
            doc.choreographies
                .iter()
                .map(|c| (&c.id, &c.conversation_nodes, &c.extras, &c.participants)),
        )
    {
        let Some(extras) = extras else { continue };
        let mut roots = Vec::new();
        for reference in extras.choreography_refs.iter().chain(
            nodes
                .iter()
                .filter_map(|n| n.call.as_ref()?.called_collaboration_ref.as_ref()),
        ) {
            if let ReferenceResolution::Resolved(selector) = index.resolve(reference) {
                roots.push(selector);
            }
        }
        // Follow only process and callable references actually owned by these participants.
        for participant in owner_participants {
            let Some(reference) = &participant.process else {
                continue;
            };
            let ReferenceResolution::Resolved(process_selector) = index.resolve(reference) else {
                continue;
            };
            let Some(process) = doc
                .processes
                .iter()
                .find(|p| p.id.selector() == process_selector)
            else {
                continue;
            };
            if let Some(reference) = process
                .activity_metadata
                .as_ref()
                .and_then(|m| m.definitional_collaboration_ref.as_ref())
                && let ReferenceResolution::Resolved(selector) = index.resolve(reference)
            {
                roots.push(selector);
            }
            let mut stack: Vec<_> = process.nodes.iter().collect();
            while let Some(node) = stack.pop() {
                if let Some(NodeDetails::SubProcess { content }) = &node.details {
                    stack.extend(content.nodes.iter());
                }
                if let Some(NodeDetails::CallActivity {
                    called_element: Some(qname),
                }) = &node.details
                {
                    let reference = crate::bpmn_activity::ModelReference::Qualified(qname.clone());
                    let ReferenceResolution::Resolved(called_selector) = index.resolve(&reference)
                    else {
                        continue;
                    };
                    let Some(called) = doc
                        .processes
                        .iter()
                        .find(|p| p.id.selector() == called_selector)
                    else {
                        continue;
                    };
                    if let Some(reference) = called
                        .activity_metadata
                        .as_ref()
                        .and_then(|m| m.definitional_collaboration_ref.as_ref())
                        && let ReferenceResolution::Resolved(selector) = index.resolve(reference)
                    {
                        roots.push(selector);
                    }
                }
            }
        }
        let mut participants = Vec::new();
        let mut conversations = Vec::new();
        let mut flows = Vec::new();
        for collaboration in doc
            .collaborations()
            .filter(|c| roots.contains(&c.id.selector()))
        {
            participants.extend(collaboration.participants.iter().map(|v| v.id.selector()));
            conversations.extend(
                collaboration
                    .conversation_nodes
                    .iter()
                    .map(|v| v.id.selector()),
            );
            flows.extend(
                collaboration
                    .flows
                    .iter()
                    .filter(|v| v.kind == FlowKind::Message)
                    .map(|v| v.id.selector()),
            );
        }
        for choreography in doc
            .choreographies
            .iter()
            .filter(|c| roots.contains(&c.id.selector()))
        {
            participants.extend(choreography.participants.iter().map(|v| v.id.selector()));
            conversations.extend(
                choreography
                    .conversation_nodes
                    .iter()
                    .map(|v| v.id.selector()),
            );
            flows.extend(choreography.message_flows.iter().map(|v| v.id.selector()));
        }
        let mut check = |owner: ElementSelector,
                         field: &'static str,
                         reference: &crate::bpmn_activity::ModelReference,
                         candidates: &[ElementSelector]| {
            match index.resolve(reference) {
                ReferenceResolution::External | ReferenceResolution::NamespaceUnknown => {}
                ReferenceResolution::Resolved(selector) if candidates.contains(&selector) => {}
                _ => result.push(ActivityReferenceIssue {
                    owner,
                    field,
                    target: reference.clone(),
                }),
            }
        };
        for association in &extras.participant_associations {
            if let Some(reference) = &association.inner {
                check(
                    association
                        .id
                        .as_ref()
                        .map_or_else(|| root.selector(), |id| id.selector()),
                    "participantAssociation.innerRef",
                    reference,
                    &participants,
                );
            }
        }
        for association in &extras.conversation_associations {
            if let Some(reference) = &association.inner {
                check(
                    association
                        .id
                        .as_ref()
                        .map_or_else(|| root.selector(), |id| id.selector()),
                    "conversationAssociation.innerRef",
                    reference,
                    &conversations,
                );
            }
        }
        for association in &extras.message_flow_associations {
            if let Some(reference) = &association.inner {
                check(
                    association
                        .id
                        .as_ref()
                        .map_or_else(|| root.selector(), |id| id.selector()),
                    "messageFlowAssociation.innerRef",
                    reference,
                    &flows,
                );
            }
        }
    }
    // CallChoreography mappings refer to the explicitly called root, including
    // calls nested inside SubChoreography scopes. Never infer a target root.
    for scope in queries::choreography_scopes(doc) {
        for activity in scope.activities {
            if let Some(crate::bpmn_collaboration::ChoreographyActivityDetails::Call {
                called_choreography_ref,
                participant_associations,
            }) = &activity.details
            {
                let target =
                    called_choreography_ref
                        .as_ref()
                        .and_then(|r| match index.resolve(r) {
                            ReferenceResolution::Resolved(id) => {
                                doc.choreographies.iter().find(|c| c.id.selector() == id)
                            }
                            _ => None,
                        });
                for association in participant_associations {
                    let Some(reference) = &association.inner else {
                        continue;
                    };
                    match index.resolve(reference) {
                        ReferenceResolution::External | ReferenceResolution::NamespaceUnknown => {}
                        ReferenceResolution::Resolved(id)
                            if target.is_some_and(|c| {
                                c.participants.iter().any(|p| p.id.selector() == id)
                            }) => {}
                        _ => result.push(ActivityReferenceIssue {
                            owner: association
                                .id
                                .as_ref()
                                .map_or_else(|| activity.id.selector(), |id| id.selector()),
                            field: "callChoreography.participantAssociation.innerRef",
                            target: reference.clone(),
                        }),
                    }
                }
            }
        }
    }
    result
}

/// Local Message Flow endpoints for which participant-process ownership cannot be established.
/// External references are inert. Incomplete participant process bindings remain unqualified.
pub fn message_flow_endpoint_scope_unqualified(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceIssue> {
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    let index = ReferenceIndex::new(doc);
    let mut issues = Vec::new();
    for (participants, flows) in doc
        .collaborations()
        .map(|c| (&c.participants, &c.flows))
        .chain(
            doc.choreographies
                .iter()
                .map(|c| (&c.participants, &c.message_flows)),
        )
    {
        let participant_ids: Vec<_> = participants.iter().map(|p| p.id.selector()).collect();
        let mut node_ids = Vec::new();
        for participant in participants {
            let Some(reference) = &participant.process else {
                continue;
            };
            let ReferenceResolution::Resolved(selector) = index.resolve(reference) else {
                continue;
            };
            let Some(process) = doc
                .processes
                .iter()
                .find(|process| process.id.selector() == selector)
            else {
                continue;
            };
            let mut stack: Vec<_> = process.nodes.iter().collect();
            while let Some(node) = stack.pop() {
                if node.kind.is_activity()
                    || matches!(
                        node.kind,
                        NodeKind::StartEvent
                            | NodeKind::EndEvent
                            | NodeKind::IntermediateCatchEvent
                            | NodeKind::IntermediateThrowEvent
                            | NodeKind::BoundaryEvent
                    )
                {
                    node_ids.push(node.id.selector());
                }
                if let Some(NodeDetails::SubProcess { content }) = &node.details {
                    stack.extend(content.nodes.iter());
                }
            }
        }
        for flow in flows.iter().filter(|flow| flow.kind == FlowKind::Message) {
            for (field, reference) in [
                ("messageFlow.sourceRef", &flow.source),
                ("messageFlow.targetRef", &flow.target),
            ] {
                let Some(reference) = reference else { continue };
                match index.resolve(reference) {
                    ReferenceResolution::Resolved(selector)
                        if participant_ids.contains(&selector) || node_ids.contains(&selector) => {}
                    ReferenceResolution::Resolved(_) | ReferenceResolution::MissingLocal => issues
                        .push(ActivityReferenceIssue {
                            owner: flow.id.selector(),
                            field,
                            target: reference.clone(),
                        }),
                    ReferenceResolution::External | ReferenceResolution::NamespaceUnknown => {}
                }
            }
        }
    }
    issues
}

/// Local Conversation Link endpoints outside provable root-owned conversation nodes,
/// participants, or participant processes remain unqualified.
pub fn conversation_link_endpoint_scope_unqualified(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceIssue> {
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    let index = ReferenceIndex::new(doc);
    let mut issues = Vec::new();
    for (participants, conversations, links) in doc
        .collaborations()
        .map(|c| {
            (
                &c.participants,
                &c.conversation_nodes,
                &c.conversation_links,
            )
        })
        .chain(doc.choreographies.iter().map(|c| {
            (
                &c.participants,
                &c.conversation_nodes,
                &c.conversation_links,
            )
        }))
    {
        let mut owned: Vec<_> = participants
            .iter()
            .map(|p| p.id.selector())
            .chain(conversations.iter().map(|n| n.id.selector()))
            .collect();
        for participant in participants {
            let Some(reference) = &participant.process else {
                continue;
            };
            let ReferenceResolution::Resolved(selector) = index.resolve(reference) else {
                continue;
            };
            let Some(process) = doc
                .processes
                .iter()
                .find(|process| process.id.selector() == selector)
            else {
                continue;
            };
            let mut stack: Vec<_> = process.nodes.iter().collect();
            while let Some(node) = stack.pop() {
                if node.kind.is_activity()
                    || matches!(
                        node.kind,
                        NodeKind::StartEvent
                            | NodeKind::EndEvent
                            | NodeKind::IntermediateCatchEvent
                            | NodeKind::IntermediateThrowEvent
                            | NodeKind::BoundaryEvent
                    )
                {
                    owned.push(node.id.selector());
                }
                if let Some(NodeDetails::SubProcess { content }) = &node.details {
                    stack.extend(content.nodes.iter());
                }
            }
        }
        for link in links {
            for (field, reference) in [
                ("conversationLink.sourceRef", &link.source),
                ("conversationLink.targetRef", &link.target),
            ] {
                let Some(reference) = reference else { continue };
                match index.resolve(reference) {
                    ReferenceResolution::Resolved(selector) if owned.contains(&selector) => {}
                    ReferenceResolution::Resolved(_) | ReferenceResolution::MissingLocal => issues
                        .push(ActivityReferenceIssue {
                            owner: link.id.selector(),
                            field,
                            target: reference.clone(),
                        }),
                    ReferenceResolution::External | ReferenceResolution::NamespaceUnknown => {}
                }
            }
        }
    }
    issues
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActivityReferenceResolution {
    pub reference: ActivityReferenceIssue,
    pub resolved_target: ElementSelector,
}
/// Resolved local targets for detecting identity replacement under a reused XML ID.
pub fn activity_family_reference_resolutions(
    doc: &crate::model::BpmnDocument,
) -> Vec<ActivityReferenceResolution> {
    activity_family_reference_scan(doc).1
}
pub(crate) fn activity_family_reference_scan(
    doc: &crate::model::BpmnDocument,
) -> (
    Vec<ActivityReferenceIssue>,
    Vec<ActivityReferenceResolution>,
) {
    use crate::bpmn_activity::{ModelReference, ResourceRole, ResourceRoleAssignment};
    let target_ns = doc
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.as_str())
        .or_else(|| {
            doc.provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let mut issues = Vec::new();
    let mut resolutions = Vec::new();
    let mut check = |owner: ElementSelector,
                     field: &'static str,
                     reference: &ModelReference,
                     candidates: Vec<&crate::ids::ElementIdentity>| {
        if reference.is_external(target_ns) {
            return;
        }
        let key = ActivityReferenceIssue {
            owner,
            field,
            target: reference.clone(),
        };
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| reference.matches_identity(candidate, target_ns))
        {
            resolutions.push(ActivityReferenceResolution {
                reference: key,
                resolved_target: candidate.selector(),
            });
        } else {
            issues.push(key);
        }
    };
    let item_definitions: Vec<_> = doc
        .catalog
        .item_definitions
        .iter()
        .filter_map(|item| item.id.as_ref())
        .collect();
    let interfaces: Vec<_> = doc
        .catalog
        .interfaces
        .iter()
        .filter_map(|x| x.id.as_ref())
        .collect();
    let operations: Vec<_> = doc
        .catalog
        .interfaces
        .iter()
        .flat_map(|x| &x.operations)
        .filter_map(|x| x.id.as_ref())
        .collect();
    let resources: Vec<_> = doc
        .catalog
        .resources
        .iter()
        .filter_map(|x| x.id.as_ref())
        .collect();
    let properties: Vec<_> = doc
        .correlation_properties
        .iter()
        .filter_map(|x| x.id.as_ref())
        .collect();
    let messages: Vec<_> = doc
        .event_resources
        .iter()
        .filter_map(|x| match x {
            EventDefinitionResource::Message { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    let categories: Vec<_> = doc
        .categories
        .iter()
        .flat_map(|c| &c.values)
        .map(|v| &v.id)
        .collect();
    let data_stores: Vec<_> = doc
        .data_stores
        .iter()
        .filter_map(|s| s.id.as_ref())
        .collect();
    let process_ids: Vec<_> = doc.processes.iter().map(|p| &p.id).collect();
    let collaboration_ids: Vec<_> = doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .map(|c| &c.id)
        .collect();
    let callable_ids: Vec<_> = doc
        .processes
        .iter()
        .map(|process| &process.id)
        .chain(doc.global_tasks.iter().filter_map(|task| task.id.as_ref()))
        .collect();
    let mut keys = Vec::new();
    for collaboration in doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
    {
        keys.extend(
            collaboration
                .correlation_keys
                .iter()
                .map(|key| (collaboration.id.selector(), key)),
        );
        for conversation in &collaboration.conversation_nodes {
            keys.extend(
                conversation
                    .correlation_keys
                    .iter()
                    .map(|key| (conversation.id.selector(), key)),
            );
        }
    }
    for choreography in &doc.choreographies {
        keys.extend(
            choreography
                .correlation_keys
                .iter()
                .map(|key| (choreography.id.selector(), key)),
        );
        for conversation in &choreography.conversation_nodes {
            keys.extend(
                conversation
                    .correlation_keys
                    .iter()
                    .map(|key| (conversation.id.selector(), key)),
            );
        }
        for activity in queries::choreography_scopes(doc)
            .into_iter()
            .filter(|scope| scope.root.id == choreography.id)
            .flat_map(|scope| scope.activities)
        {
            keys.extend(
                activity
                    .correlation_keys
                    .iter()
                    .map(|key| (activity.id.selector(), key)),
            );
        }
    }
    let key_ids: Vec<_> = keys.iter().filter_map(|(_, key)| key.id.as_ref()).collect();
    {
        let mut check_role = |owner: ElementSelector, role: &ResourceRole| {
            let owner = role.id.as_ref().map_or(owner, |id| id.selector());
            if let ResourceRoleAssignment::Resource {
                reference,
                bindings,
            } = &role.assignment
            {
                check(
                    owner.clone(),
                    "resourceRole.resourceRef",
                    reference,
                    resources.clone(),
                );
                // Parameter scope belongs to the selected resource. An external or
                // unresolved resource cannot be used to infer a local parameter error.
                if let Some(resource) = doc.catalog.resources.iter().find(|resource| {
                    resource
                        .id
                        .as_ref()
                        .is_some_and(|id| reference.matches_identity(id, target_ns))
                }) {
                    let parameters: Vec<_> = resource
                        .parameters
                        .iter()
                        .filter_map(|p| p.id.as_ref())
                        .collect();
                    for binding in bindings {
                        if let Some(reference) = &binding.parameter_ref {
                            check(
                                binding
                                    .id
                                    .as_ref()
                                    .map_or_else(|| owner.clone(), |id| id.selector()),
                                "resourceParameterBinding.parameterRef",
                                reference,
                                parameters.clone(),
                            );
                        }
                    }
                }
            }
        };
        for process in &doc.processes {
            if let Some(metadata) = &process.activity_metadata {
                for role in &metadata.resource_roles {
                    check_role(process.id.selector(), role);
                }
            }
        }
        for task in &doc.global_tasks {
            for role in &task.resource_roles {
                check_role(
                    task.id
                        .as_ref()
                        .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                    role,
                );
            }
        }
        for scope in queries::scopes(doc) {
            for node in scope.nodes {
                if let Some(activity) = &node.activity {
                    for role in &activity.resource_roles {
                        check_role(node.id.selector(), role);
                    }
                }
            }
        }
    }
    for process in &doc.processes {
        if let Some(metadata) = &process.activity_metadata {
            for property in &metadata.properties {
                if let Some(reference) = property
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.item_subject_ref.as_ref())
                {
                    check(
                        property
                            .id
                            .as_ref()
                            .map_or_else(|| process.id.selector(), |id| id.selector()),
                        "property.itemSubjectRef",
                        &ModelReference::Qualified(reference.clone()),
                        item_definitions.clone(),
                    );
                }
            }
            for reference in &metadata.callable.supported_interface_refs {
                check(
                    process.id.selector(),
                    "callable.supportedInterfaceRef",
                    reference,
                    interfaces.clone(),
                );
            }
            for binding in &metadata.callable.io_bindings {
                let owner = binding
                    .id
                    .as_ref()
                    .map_or_else(|| process.id.selector(), |id| id.selector());
                let inputs: Vec<_> = process
                    .io_specification
                    .iter()
                    .flat_map(|io| &io.inputs)
                    .filter_map(|port| port.id.as_ref())
                    .collect();
                let outputs: Vec<_> = process
                    .io_specification
                    .iter()
                    .flat_map(|io| &io.outputs)
                    .filter_map(|port| port.id.as_ref())
                    .collect();
                if let Some(reference) = &binding.input_data_ref {
                    check(
                        owner.clone(),
                        "ioBinding.inputDataRef",
                        &ModelReference::Local(reference.clone()),
                        inputs,
                    );
                }
                if let Some(reference) = &binding.output_data_ref {
                    check(
                        owner,
                        "ioBinding.outputDataRef",
                        &ModelReference::Local(reference.clone()),
                        outputs,
                    );
                }
                if let Some(reference) = &binding.operation_ref {
                    check(
                        binding
                            .id
                            .as_ref()
                            .map_or_else(|| process.id.selector(), |id| id.selector()),
                        "ioBinding.operationRef",
                        reference,
                        operations.clone(),
                    );
                }
            }
            for reference in &metadata.supports {
                check(
                    process.id.selector(),
                    "process.supports",
                    reference,
                    process_ids.clone(),
                );
            }
            if let Some(reference) = &metadata.definitional_collaboration_ref {
                check(
                    process.id.selector(),
                    "process.definitionalCollaborationRef",
                    reference,
                    collaboration_ids.clone(),
                );
            }
            for subscription in &metadata.correlation_subscriptions {
                if let Some(reference) = &subscription.key_ref {
                    check(
                        subscription
                            .id
                            .as_ref()
                            .map_or_else(|| process.id.selector(), |id| id.selector()),
                        "correlationSubscription.keyRef",
                        reference,
                        key_ids.clone(),
                    );
                }
                for binding in &subscription.bindings {
                    if let Some(reference) = &binding.property_ref {
                        check(
                            binding
                                .id
                                .as_ref()
                                .or(subscription.id.as_ref())
                                .map_or_else(|| process.id.selector(), |id| id.selector()),
                            "correlationBinding.propertyRef",
                            reference,
                            properties.clone(),
                        );
                    }
                }
            }
        }
    }
    {
        let mut check_io = |owner: ElementSelector, io: &crate::bpmn_data::IoSpecification| {
            for port in io.inputs.iter().chain(&io.outputs) {
                if let Some(reference) = port
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.item_subject_ref.as_ref())
                {
                    check(
                        port.id
                            .as_ref()
                            .map_or_else(|| owner.clone(), |id| id.selector()),
                        "callablePort.itemSubjectRef",
                        &ModelReference::Qualified(reference.clone()),
                        item_definitions.clone(),
                    );
                }
            }
            for (sets, ports, opposite) in [
                (&io.input_sets, &io.inputs, &io.output_sets),
                (&io.output_sets, &io.outputs, &io.input_sets),
            ] {
                let port_ids: Vec<_> = ports.iter().filter_map(|port| port.id.as_ref()).collect();
                let set_ids: Vec<_> = opposite.iter().filter_map(|set| set.id.as_ref()).collect();
                for set in sets {
                    let set_owner = set
                        .id
                        .as_ref()
                        .map_or_else(|| owner.clone(), |id| id.selector());
                    for selector in set
                        .data_refs
                        .iter()
                        .chain(&set.optional_refs)
                        .chain(&set.while_executing_refs)
                    {
                        check(
                            set_owner.clone(),
                            "ioSet.dataRef",
                            &ModelReference::Local(selector.clone()),
                            port_ids.clone(),
                        );
                    }
                    for selector in &set.counterpart_refs {
                        check(
                            set_owner.clone(),
                            "ioSet.counterpartRef",
                            &ModelReference::Local(selector.clone()),
                            set_ids.clone(),
                        );
                    }
                }
            }
        };
        for process in &doc.processes {
            if let Some(io) = &process.io_specification {
                check_io(process.id.selector(), io);
            }
        }
        for task in &doc.global_tasks {
            if let Some(io) = &task.io_specification {
                check_io(
                    task.id
                        .as_ref()
                        .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                    io,
                );
            }
        }
    }
    for task in &doc.global_tasks {
        let owner = task
            .id
            .as_ref()
            .map_or_else(|| doc.definitions_id.selector(), |id| id.selector());
        for reference in &task.callable.supported_interface_refs {
            check(
                owner.clone(),
                "callable.supportedInterfaceRef",
                reference,
                interfaces.clone(),
            );
        }
        for binding in &task.callable.io_bindings {
            let binding_owner = binding
                .id
                .as_ref()
                .map_or_else(|| owner.clone(), |id| id.selector());
            let inputs: Vec<_> = task
                .io_specification
                .iter()
                .flat_map(|io| &io.inputs)
                .filter_map(|port| port.id.as_ref())
                .collect();
            let outputs: Vec<_> = task
                .io_specification
                .iter()
                .flat_map(|io| &io.outputs)
                .filter_map(|port| port.id.as_ref())
                .collect();
            if let Some(reference) = &binding.input_data_ref {
                check(
                    binding_owner.clone(),
                    "ioBinding.inputDataRef",
                    &ModelReference::Local(reference.clone()),
                    inputs,
                );
            }
            if let Some(reference) = &binding.output_data_ref {
                check(
                    binding_owner,
                    "ioBinding.outputDataRef",
                    &ModelReference::Local(reference.clone()),
                    outputs,
                );
            }
            if let Some(reference) = &binding.operation_ref {
                check(
                    binding
                        .id
                        .as_ref()
                        .map_or_else(|| owner.clone(), |id| id.selector()),
                    "ioBinding.operationRef",
                    reference,
                    operations.clone(),
                );
            }
        }
    }
    for (owner, key) in &keys {
        let owner = key
            .id
            .as_ref()
            .map_or_else(|| owner.clone(), |id| id.selector());
        for reference in &key.property_refs {
            check(
                owner.clone(),
                "correlationKey.propertyRef",
                reference,
                properties.clone(),
            );
        }
    }
    for property in &doc.correlation_properties {
        for retrieval in &property.retrieval_expressions {
            if let Some(reference) = &retrieval.message_ref {
                check(
                    retrieval
                        .id
                        .as_ref()
                        .or(property.id.as_ref())
                        .map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                    "correlationRetrieval.messageRef",
                    reference,
                    messages.clone(),
                );
            }
        }
    }
    // Each collaboration root has its own participant, conversation and message-flow scope.
    let endpoints: Vec<_> = doc
        .endpoints
        .iter()
        .filter_map(|endpoint| endpoint.id.as_ref())
        .collect();
    let all_collaborations: Vec<_> = doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .map(|c| &c.id)
        .collect();
    let choreography_ids: Vec<_> = doc.choreographies.iter().map(|c| &c.id).collect();
    let all_participants: Vec<_> = doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .flat_map(|c| &c.participants)
        .chain(doc.choreographies.iter().flat_map(|c| &c.participants))
        .map(|p| &p.id)
        .collect();
    let all_conversations: Vec<_> = doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .flat_map(|c| &c.conversation_nodes)
        .chain(
            doc.choreographies
                .iter()
                .flat_map(|c| &c.conversation_nodes),
        )
        .map(|n| &n.id)
        .collect();
    let all_message_flows: Vec<_> = doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .flat_map(|c| &c.flows)
        .chain(doc.choreographies.iter().flat_map(|c| &c.message_flows))
        .filter(|f| f.kind == FlowKind::Message)
        .map(|f| &f.id)
        .collect();
    for (owner, refs) in doc
        .partner_entities
        .iter()
        .map(|p| (p.id.as_ref(), &p.participant_refs))
        .chain(
            doc.partner_roles
                .iter()
                .map(|p| (p.id.as_ref(), &p.participant_refs)),
        )
    {
        for reference in refs {
            check(
                owner.map_or_else(|| doc.definitions_id.selector(), |id| id.selector()),
                "partner.participantRef",
                reference,
                all_participants.clone(),
            );
        }
    }
    for (root, participants, flows, nodes, links, extras) in doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
        .map(|c| {
            (
                &c.id,
                &c.participants,
                &c.flows,
                &c.conversation_nodes,
                &c.conversation_links,
                &c.extras,
            )
        })
        .chain(doc.choreographies.iter().map(|c| {
            (
                &c.id,
                &c.participants,
                &c.message_flows,
                &c.conversation_nodes,
                &c.conversation_links,
                &c.extras,
            )
        }))
    {
        let participant_ids: Vec<_> = participants.iter().map(|p| &p.id).collect();
        let flow_ids: Vec<_> = flows
            .iter()
            .filter(|f| f.kind == FlowKind::Message)
            .map(|f| &f.id)
            .collect();
        let node_ids: Vec<_> = nodes.iter().map(|n| &n.id).collect();
        for participant in participants {
            if let Some(reference) = &participant.process {
                check(
                    participant.id.selector(),
                    "participant.processRef",
                    reference,
                    process_ids.clone(),
                );
            }
            for reference in &participant.interface_refs {
                check(
                    participant.id.selector(),
                    "participant.interfaceRef",
                    reference,
                    interfaces.clone(),
                );
            }
            for reference in &participant.endpoint_refs {
                check(
                    participant.id.selector(),
                    "participant.endPointRef",
                    reference,
                    endpoints.clone(),
                );
            }
        }
        for node in nodes {
            for reference in &node.participant_refs {
                check(
                    node.id.selector(),
                    "conversation.participantRef",
                    reference,
                    participant_ids.clone(),
                );
            }
            for reference in &node.message_flow_refs {
                check(
                    node.id.selector(),
                    "conversation.messageFlowRef",
                    reference,
                    flow_ids.clone(),
                );
            }
            if let Some(call) = &node.call {
                if let Some(reference) = &call.called_collaboration_ref {
                    check(
                        node.id.selector(),
                        "callConversation.calledCollaborationRef",
                        reference,
                        all_collaborations.clone(),
                    );
                }
                let inner_participants = call
                    .called_collaboration_ref
                    .as_ref()
                    .and_then(|reference| {
                        doc.collaboration
                            .iter()
                            .chain(&doc.additional_collaborations)
                            .find(|collaboration| {
                                reference.matches_identity(&collaboration.id, target_ns)
                            })
                    })
                    .map(|collaboration| {
                        collaboration
                            .participants
                            .iter()
                            .map(|participant| &participant.id)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| all_participants.clone());
                for association in &call.participant_associations {
                    let owner = association
                        .id
                        .as_ref()
                        .map_or_else(|| node.id.selector(), |id| id.selector());
                    for (field, reference) in [
                        ("participantAssociation.innerRef", &association.inner),
                        ("participantAssociation.outerRef", &association.outer),
                    ] {
                        if let Some(reference) = reference {
                            check(
                                owner.clone(),
                                field,
                                reference,
                                if field.ends_with(".outerRef") {
                                    participant_ids.clone()
                                } else {
                                    inner_participants.clone()
                                },
                            );
                        }
                    }
                }
            }
        }
        let mut message_endpoints = participant_ids.clone();
        for scope in queries::scopes(doc) {
            message_endpoints.extend(
                scope
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.kind.is_activity()
                            || matches!(
                                node.kind,
                                NodeKind::StartEvent
                                    | NodeKind::EndEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::BoundaryEvent
                            )
                    })
                    .map(|node| &node.id),
            );
        }
        for link in links {
            let mut candidates = message_endpoints.clone();
            candidates.extend(node_ids.iter().copied());
            let is_match = |reference: &ModelReference,
                            choices: &[&crate::ids::ElementIdentity]| {
                choices
                    .iter()
                    .any(|id| reference.matches_identity(id, target_ns))
            };
            if let Some(reference) = &link.source {
                let choices = match &link.target {
                    Some(other) if is_match(other, &node_ids) => message_endpoints.clone(),
                    Some(other) if is_match(other, &message_endpoints) => node_ids.clone(),
                    _ => candidates.clone(),
                };
                check(
                    link.id.selector(),
                    "conversationLink.sourceRef",
                    reference,
                    choices,
                );
            }
            if let Some(reference) = &link.target {
                let choices = match &link.source {
                    Some(other) if is_match(other, &node_ids) => message_endpoints.clone(),
                    Some(other) if is_match(other, &message_endpoints) => node_ids.clone(),
                    _ => candidates.clone(),
                };
                check(
                    link.id.selector(),
                    "conversationLink.targetRef",
                    reference,
                    choices,
                );
            }
        }
        let endpoint_lookup = IdentityLookup::new(&message_endpoints);
        for flow in flows.iter().filter(|f| f.kind == FlowKind::Message) {
            if let Some(reference) = &flow.source {
                check(
                    flow.id.selector(),
                    "messageFlow.sourceRef",
                    reference,
                    endpoint_lookup.candidates(reference),
                );
            }
            if let Some(reference) = &flow.target {
                check(
                    flow.id.selector(),
                    "messageFlow.targetRef",
                    reference,
                    endpoint_lookup.candidates(reference),
                );
            }
        }
        if let Some(extra) = extras {
            let base_selectors = doc.base_element_selectors();
            let base_elements: Vec<_> = doc
                .element_identities()
                .into_iter()
                .filter(|identity| base_selectors.contains(&identity.selector()))
                .collect();
            let base_lookup = IdentityLookup::new(&base_elements);
            for artifact in &extra.artifacts {
                if let crate::bpmn_collaboration::CollaborationArtifact::Group(group) = artifact
                    && let Some(reference) = &group.category_value_ref
                {
                    check(
                        group.id.selector(),
                        "group.categoryValueRef",
                        reference,
                        categories.clone(),
                    );
                }
                if let crate::bpmn_collaboration::CollaborationArtifact::Association(flow) =
                    artifact
                {
                    if let Some(reference) = &flow.source {
                        check(
                            flow.id.selector(),
                            "association.sourceRef",
                            reference,
                            base_lookup.candidates(reference),
                        );
                    }
                    if let Some(reference) = &flow.target {
                        check(
                            flow.id.selector(),
                            "association.targetRef",
                            reference,
                            base_lookup.candidates(reference),
                        );
                    }
                }
            }
            for association in &extra.participant_associations {
                let owner = association
                    .id
                    .as_ref()
                    .map_or_else(|| root.selector(), |id| id.selector());
                for (field, reference) in [
                    ("participantAssociation.innerRef", &association.inner),
                    ("participantAssociation.outerRef", &association.outer),
                ] {
                    if let Some(reference) = reference {
                        check(
                            owner.clone(),
                            field,
                            reference,
                            if field.ends_with(".outerRef") {
                                participant_ids.clone()
                            } else {
                                all_participants.clone()
                            },
                        );
                    }
                }
            }
            for association in &extra.conversation_associations {
                let owner = association
                    .id
                    .as_ref()
                    .map_or_else(|| root.selector(), |id| id.selector());
                for (field, reference) in [
                    ("conversationAssociation.innerRef", &association.inner),
                    ("conversationAssociation.outerRef", &association.outer),
                ] {
                    if let Some(reference) = reference {
                        check(
                            owner.clone(),
                            field,
                            reference,
                            if field.ends_with(".outerRef") {
                                node_ids.clone()
                            } else {
                                all_conversations.clone()
                            },
                        );
                    }
                }
            }
            for association in &extra.message_flow_associations {
                let owner = association
                    .id
                    .as_ref()
                    .map_or_else(|| root.selector(), |id| id.selector());
                for (field, reference) in [
                    ("messageFlowAssociation.innerRef", &association.inner),
                    ("messageFlowAssociation.outerRef", &association.outer),
                ] {
                    if let Some(reference) = reference {
                        check(
                            owner.clone(),
                            field,
                            reference,
                            if field.ends_with(".outerRef") {
                                flow_ids.clone()
                            } else {
                                all_message_flows.clone()
                            },
                        );
                    }
                }
            }
            for reference in &extra.choreography_refs {
                check(
                    root.selector(),
                    "collaboration.choreographyRef",
                    reference,
                    choreography_ids.clone(),
                );
            }
        }
    }
    // Message references are semantic; band ownership and plane order point to
    // representations and are resolved only within their owning diagram.
    for collaboration in doc
        .collaboration
        .iter()
        .chain(&doc.additional_collaborations)
    {
        for flow in &collaboration.flows {
            if let Some(reference) = &flow.message_ref {
                check(
                    flow.id.selector(),
                    "messageFlow.messageRef",
                    reference,
                    messages.clone(),
                );
            }
        }
    }
    for choreography in &doc.choreographies {
        for flow in &choreography.message_flows {
            if let Some(reference) = &flow.message_ref {
                check(
                    flow.id.selector(),
                    "messageFlow.messageRef",
                    reference,
                    messages.clone(),
                );
            }
        }
    }
    for diagram in &doc.diagrams {
        let representation_ids: Vec<_> = diagram
            .shapes
            .iter()
            .map(|shape| &shape.id)
            .chain(diagram.edges.iter().map(|edge| &edge.id))
            .collect();
        // BPMN 2.0.2 §12.2.3 (BPMNPlane): bpmnElement names the Process,
        // Collaboration or Choreography the plane depicts, or the Sub-Process /
        // Sub-Choreography it drills into.
        let scope_activities: Vec<_> = queries::choreography_scopes(doc)
            .into_iter()
            .flat_map(|scope| scope.activities)
            .map(|activity| &activity.id)
            .collect();
        check(
            diagram.plane_id.selector(),
            "di.bpmnElement",
            &ModelReference::Local(diagram.plane.clone()),
            doc.processes
                .iter()
                .flat_map(|p| std::iter::once(&p.id).chain(p.nodes.iter().map(|n| &n.id)))
                .chain(all_collaborations.iter().copied())
                .chain(choreography_ids.iter().copied())
                .chain(scope_activities)
                .collect(),
        );
        if let Some(order) = &diagram.plane_order {
            let representation_lookup = IdentityLookup::new(&representation_ids);
            for reference in order {
                let reference = ModelReference::Local(reference.clone());
                check(
                    diagram.plane_id.selector(),
                    "di.planeOrder",
                    &reference,
                    representation_lookup.candidates(&reference),
                );
            }
        }
        for shape in &diagram.shapes {
            if let Some(reference) = &shape.choreography_activity_shape {
                let candidates: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|candidate| {
                        doc.choreographies.iter().any(|choreography| {
                            queries::choreography_scopes(doc).into_iter().filter(|scope| scope.root.id == choreography.id).flat_map(|scope| scope.activities).any(|activity| {
                                activity.id.selector() == candidate.element
                                    && (shape.participant_band_kind.is_none()
                                        || (choreography.participants.iter().any(|participant| {
                                            participant.id.selector() == shape.element
                                        }) && activity.participant_refs.iter().any(|r| matches!(crate::reference_resolution::ReferenceIndex::new(doc).resolve(r), crate::reference_resolution::ReferenceResolution::Resolved(id) if id == shape.element))))
                            })
                        })
                    })
                    .map(|candidate| &candidate.id)
                    .collect();
                let reference = match reference {
                    crate::model::DiReference::Local(id) => ModelReference::Local(id.clone()),
                    crate::model::DiReference::Qualified(name) => {
                        ModelReference::Qualified(name.clone())
                    }
                };
                check(
                    shape.id.selector(),
                    "di.choreographyActivityShape",
                    &reference,
                    candidates,
                );
            }
        }
    }
    let signals: Vec<_> = doc
        .event_resources
        .iter()
        .filter_map(|e| match e {
            EventDefinitionResource::Signal { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    let errors: Vec<_> = doc
        .event_resources
        .iter()
        .filter_map(|e| match e {
            EventDefinitionResource::Error { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    let escalations: Vec<_> = doc
        .event_resources
        .iter()
        .filter_map(|e| match e {
            EventDefinitionResource::Escalation { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    let all_scopes = queries::scopes(doc);
    let base_selectors = doc.base_element_selectors();
    let base_elements: Vec<_> = doc
        .element_identities()
        .into_iter()
        .filter(|identity| base_selectors.contains(&identity.selector()))
        .collect();
    let base_lookup = IdentityLookup::new(&base_elements);
    for scope in &all_scopes {
        for group in scope.groups {
            if let Some(reference) = &group.category_value_ref {
                check(
                    group.id.selector(),
                    "group.categoryValueRef",
                    reference,
                    categories.clone(),
                );
            }
        }
        let flow_nodes: Vec<_> = scope
            .nodes
            .iter()
            .filter(|node| {
                !matches!(
                    node.kind,
                    NodeKind::TextAnnotation
                        | NodeKind::DataObjectReference
                        | NodeKind::DataStoreReference
                )
            })
            .map(|node| &node.id)
            .collect();
        let flow_node_lookup = IdentityLookup::new(&flow_nodes);
        for flow in scope
            .flows
            .iter()
            .filter(|flow| flow.kind == FlowKind::Association)
        {
            if let Some(reference) = &flow.source {
                check(
                    flow.id.selector(),
                    "association.sourceRef",
                    reference,
                    base_lookup.candidates(reference),
                );
            }
            if let Some(reference) = &flow.target {
                check(
                    flow.id.selector(),
                    "association.targetRef",
                    reference,
                    base_lookup.candidates(reference),
                );
            }
        }
        for flow in scope
            .flows
            .iter()
            .filter(|flow| flow.kind == FlowKind::Sequence)
        {
            if let Some(reference) = &flow.source {
                check(
                    flow.id.selector(),
                    "sequenceFlow.sourceRef",
                    reference,
                    flow_node_lookup.candidates(reference),
                );
            }
            if let Some(reference) = &flow.target {
                check(
                    flow.id.selector(),
                    "sequenceFlow.targetRef",
                    reference,
                    flow_node_lookup.candidates(reference),
                );
            }
        }
    }
    let mut all_output_sets: Vec<&crate::bpmn_data::IoSet> = Vec::new();
    for process in &doc.processes {
        if let Some(io) = &process.io_specification {
            all_output_sets.extend(&io.output_sets);
        }
    }
    for task in &doc.global_tasks {
        if let Some(io) = &task.io_specification {
            all_output_sets.extend(&io.output_sets);
        }
    }
    for scope in &all_scopes {
        for node in scope.nodes {
            if let Some(io) = &node.io_specification {
                all_output_sets.extend(&io.output_sets);
            }
            if let Some(NodeDetails::Event {
                io:
                    Some(EventIo::Catch {
                        output_set: Some(set),
                        ..
                    }),
                ..
            }) = &node.details
            {
                all_output_sets.push(set);
            }
        }
    }
    let all_output_set_ids: Vec<_> = all_output_sets
        .iter()
        .filter_map(|set| set.id.as_ref())
        .collect();
    // A scope is identified by the process or containing subprocess node.
    // Walk only its ancestor chain; sibling subprocess-local data is invisible.
    let scope_by_id: HashMap<_, _> = all_scopes
        .iter()
        .map(|scope| (scope.id.selector(), scope))
        .collect();
    let mut parent_scope = HashMap::new();
    let mut scope_owners = HashMap::new();
    for scope in &all_scopes {
        for node in scope.nodes {
            if matches!(node.details, Some(NodeDetails::SubProcess { .. })) {
                parent_scope.insert(node.id.selector(), scope.id.selector());
                scope_owners.insert(node.id.selector(), node);
            }
        }
    }
    let event_definitions: Vec<_> = doc
        .root_event_definitions
        .iter()
        .filter_map(|definition| definition.id())
        .collect();
    for scope in queries::choreography_scopes(doc) {
        for artifact in scope.artifacts.iter().filter(|_| scope.parent.is_some()) {
            match artifact {
                crate::bpmn_collaboration::CollaborationArtifact::Group(group) => {
                    if let Some(reference) = &group.category_value_ref {
                        check(
                            group.id.selector(),
                            "group.categoryValueRef",
                            reference,
                            categories.clone(),
                        );
                    }
                }
                crate::bpmn_collaboration::CollaborationArtifact::Association(flow) => {
                    for (field, reference) in [
                        ("association.sourceRef", &flow.source),
                        ("association.targetRef", &flow.target),
                    ] {
                        if let Some(reference) = reference {
                            check(
                                flow.id.selector(),
                                field,
                                reference,
                                base_lookup.candidates(reference),
                            );
                        }
                    }
                }
                crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(_) => {}
            }
        }
        let links: Vec<_> = scope
            .events
            .iter()
            .filter_map(|n| match &n.details {
                Some(NodeDetails::Event { definitions, .. }) => Some(definitions),
                _ => None,
            })
            .flatten()
            .filter_map(|d| match d {
                EventDefinition::Link { id, .. } => id.as_ref(),
                _ => None,
            })
            .collect();
        for node in &scope.events {
            if let Some(NodeDetails::Event {
                definitions,
                definition_refs,
                properties,
                io,
                ..
            }) = &node.details
            {
                let owner = node.id.selector();
                if let Some(io) = io {
                    let (ports, set) = match io {
                        EventIo::Catch {
                            outputs,
                            output_set,
                        } => (outputs, output_set),
                        EventIo::Throw { inputs, input_set } => (inputs, input_set),
                    };
                    let port_ids: Vec<_> =
                        ports.iter().filter_map(|port| port.id.as_ref()).collect();
                    for port in ports {
                        if let Some(reference) = port
                            .item_aware
                            .as_ref()
                            .and_then(|i| i.item_subject_ref.as_ref())
                        {
                            check(
                                port.id
                                    .as_ref()
                                    .map_or_else(|| owner.clone(), |id| id.selector()),
                                "event.port.itemSubjectRef",
                                &ModelReference::Qualified(reference.clone()),
                                item_definitions.clone(),
                            );
                        }
                    }
                    if let Some(set) = set {
                        let set_owner = set
                            .id
                            .as_ref()
                            .map_or_else(|| owner.clone(), |id| id.selector());
                        for reference in set
                            .data_refs
                            .iter()
                            .chain(&set.optional_refs)
                            .chain(&set.while_executing_refs)
                        {
                            check(
                                set_owner.clone(),
                                "event.set.dataRef",
                                &ModelReference::Local(reference.clone()),
                                port_ids.clone(),
                            );
                        }
                    }
                }
                for reference in definition_refs {
                    check(
                        owner.clone(),
                        "event.eventDefinitionRef",
                        reference,
                        event_definitions.clone(),
                    );
                }
                for property in properties {
                    if let Some(reference) = property
                        .item_aware
                        .as_ref()
                        .and_then(|i| i.item_subject_ref.as_ref())
                    {
                        check(
                            property
                                .id
                                .as_ref()
                                .map_or_else(|| owner.clone(), |id| id.selector()),
                            "event.property.itemSubjectRef",
                            &ModelReference::Qualified(reference.clone()),
                            item_definitions.clone(),
                        );
                    }
                }
                for definition in definitions {
                    let definition_owner = definition
                        .id()
                        .map_or_else(|| owner.clone(), |id| id.selector());
                    match definition {
                        EventDefinition::Message {
                            message_ref,
                            operation_ref,
                            ..
                        } => {
                            if let Some(r) = message_ref {
                                check(
                                    definition_owner.clone(),
                                    "event.messageRef",
                                    r,
                                    messages.clone(),
                                );
                            }
                            if let Some(r) = operation_ref {
                                check(
                                    definition_owner,
                                    "event.operationRef",
                                    r,
                                    operations.clone(),
                                );
                            }
                        }
                        EventDefinition::Signal {
                            signal_ref: Some(r),
                            ..
                        } => check(definition_owner, "event.signalRef", r, signals.clone()),
                        EventDefinition::Error {
                            error_ref: Some(r), ..
                        } => check(definition_owner, "event.errorRef", r, errors.clone()),
                        EventDefinition::Escalation {
                            escalation_ref: Some(r),
                            ..
                        } => check(
                            definition_owner,
                            "event.escalationRef",
                            r,
                            escalations.clone(),
                        ),
                        EventDefinition::Compensate {
                            activity_ref: Some(r),
                            ..
                        } => check(
                            definition_owner,
                            "event.activityRef",
                            r,
                            scope.activities.iter().map(|a| &a.id).collect(),
                        ),
                        EventDefinition::Link {
                            sources, target, ..
                        } => {
                            for r in sources.iter().chain(target.iter()) {
                                check(definition_owner.clone(), "event.linkRef", r, links.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let node_metadata = scope
            .activities
            .iter()
            .map(|a| (&a.id, &a.flow_metadata))
            .chain(
                scope
                    .events
                    .iter()
                    .chain(&scope.gateways)
                    .map(|n| (&n.id, &n.flow_metadata)),
            );
        for (node_id, metadata) in node_metadata {
            if let Some(metadata) = metadata {
                for (field, references, incoming) in [
                    ("flow.incoming", &metadata.incoming, true),
                    ("flow.outgoing", &metadata.outgoing, false),
                ] {
                    let candidates: Vec<_> = scope
                        .flows
                        .iter()
                        .filter(|flow| {
                            flow.kind == FlowKind::Sequence
                                && if incoming {
                                    flow.target.as_ref()
                                        == Some(&crate::bpmn_model::EventReference::Local(
                                            node_id.selector(),
                                        ))
                                } else {
                                    flow.source.as_ref()
                                        == Some(&crate::bpmn_model::EventReference::Local(
                                            node_id.selector(),
                                        ))
                                }
                        })
                        .map(|flow| &flow.id)
                        .collect();
                    for reference in references {
                        check(node_id.selector(), field, reference, candidates.clone());
                    }
                }
                for reference in &metadata.category_value_refs {
                    check(
                        node_id.selector(),
                        "flow.categoryValueRef",
                        reference,
                        categories.clone(),
                    );
                }
            }
        }
    }
    for scope in &all_scopes {
        let mut visible_objects = Vec::new();
        let mut visible_items: Vec<&crate::ids::ElementIdentity> = doc
            .data_stores
            .iter()
            .filter_map(|store| store.id.as_ref())
            .collect();
        let mut current = Some(scope.id.selector());
        let mut seen_scopes = HashSet::new();
        while let Some(id) = current {
            if !seen_scopes.insert(id.clone()) {
                break;
            }
            let Some(ancestor) = scope_by_id.get(&id) else {
                break;
            };
            let objects: Vec<_> = ancestor
                .data_objects
                .iter()
                .filter_map(|object| object.id.as_ref())
                .collect();
            visible_objects.extend(objects.iter().copied());
            visible_items.extend(objects);
            for candidate in ancestor.nodes {
                if matches!(
                    candidate.kind,
                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                ) {
                    visible_items.push(&candidate.id);
                }
            }
            if let Some(process) = doc
                .processes
                .iter()
                .find(|process| process.id.selector() == id)
            {
                if let Some(io) = &process.io_specification {
                    visible_items.extend(io.inputs.iter().filter_map(|port| port.id.as_ref()));
                }
                if let Some(metadata) = &process.activity_metadata {
                    visible_items.extend(
                        metadata
                            .properties
                            .iter()
                            .filter_map(|property| property.id.as_ref()),
                    );
                }
            }
            if let Some(owner) = scope_owners.get(&id)
                && let Some(activity) = &owner.activity
            {
                visible_items.extend(
                    activity
                        .properties
                        .iter()
                        .filter_map(|property| property.id.as_ref()),
                );
            }
            current = parent_scope.get(&id).cloned();
        }
        for object in scope.data_objects {
            if let Some(metadata) = &object.flow_metadata {
                let owner = object
                    .id
                    .as_ref()
                    .map_or_else(|| scope.id.selector(), |id| id.selector());
                for reference in &metadata.category_value_refs {
                    check(
                        owner.clone(),
                        "flow.categoryValueRef",
                        reference,
                        categories.clone(),
                    );
                }
            }
        }
        for node in scope.nodes {
            if let Some(NodeDetails::CallActivity {
                called_element: Some(reference),
            }) = &node.details
            {
                check(
                    node.id.selector(),
                    "callActivity.calledElement",
                    &ModelReference::Qualified(reference.clone()),
                    callable_ids.clone(),
                );
            }
            if let Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(reference),
            }) = &node.details
            {
                let reference = match reference {
                    crate::bpmn_data::DataStoreRef::Legacy(selector) => {
                        ModelReference::Local(selector.clone())
                    }
                    crate::bpmn_data::DataStoreRef::Qualified(name) => {
                        ModelReference::Qualified(name.clone())
                    }
                };
                check(
                    node.id.selector(),
                    "dataStoreReference.dataStoreRef",
                    &reference,
                    data_stores.clone(),
                );
            }
            if let Some(activity) = &node.activity {
                for property in &activity.properties {
                    if let Some(reference) = property
                        .item_aware
                        .as_ref()
                        .and_then(|item| item.item_subject_ref.as_ref())
                    {
                        check(
                            property
                                .id
                                .as_ref()
                                .map_or_else(|| node.id.selector(), |id| id.selector()),
                            "property.itemSubjectRef",
                            &ModelReference::Qualified(reference.clone()),
                            item_definitions.clone(),
                        );
                    }
                }
            }
            if let Some(crate::bpmn_model::LoopCharacteristics::MultiInstance {
                id,
                one_behavior_event_ref,
                none_behavior_event_ref,
                loop_data_input_ref,
                loop_data_output_ref,
                input_data_item,
                output_data_item,
                complex_behavior_definitions,
                ..
            }) = &node.loop_characteristics
            {
                let owner = id
                    .as_ref()
                    .map_or_else(|| node.id.selector(), |id| id.selector());
                for port in [input_data_item, output_data_item].into_iter().flatten() {
                    if let Some(reference) = port
                        .item_aware
                        .as_ref()
                        .and_then(|item| item.item_subject_ref.as_ref())
                    {
                        check(
                            port.id
                                .as_ref()
                                .map_or_else(|| owner.clone(), |id| id.selector()),
                            "loop.dataItem.itemSubjectRef",
                            &ModelReference::Qualified(reference.clone()),
                            item_definitions.clone(),
                        );
                    }
                }
                let is_subprocess = matches!(
                    node.kind,
                    NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
                );
                for (field, reference, input) in [
                    ("loop.loopDataInputRef", loop_data_input_ref, true),
                    ("loop.loopDataOutputRef", loop_data_output_ref, false),
                ] {
                    if let Some(reference) = reference {
                        let candidates: Vec<_> = if is_subprocess {
                            visible_objects.clone()
                        } else {
                            node.io_specification
                                .iter()
                                .flat_map(|io| if input { &io.inputs } else { &io.outputs })
                                .filter_map(|port| port.id.as_ref())
                                .collect()
                        };
                        check(owner.clone(), field, reference, candidates);
                    }
                }
                for (field, reference) in [
                    ("loop.oneBehaviorEventRef", one_behavior_event_ref),
                    ("loop.noneBehaviorEventRef", none_behavior_event_ref),
                ] {
                    if let Some(reference) = reference {
                        check(owner.clone(), field, reference, event_definitions.clone());
                    }
                }
                for behavior in complex_behavior_definitions {
                    if let Some(event) = &behavior.event {
                        let event_owner = event
                            .id
                            .as_ref()
                            .or(behavior.id.as_ref())
                            .map_or_else(|| owner.clone(), |id| id.selector());
                        for property in &event.properties {
                            if let Some(reference) = property
                                .item_aware
                                .as_ref()
                                .and_then(|item| item.item_subject_ref.as_ref())
                            {
                                check(
                                    property
                                        .id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.property.itemSubjectRef",
                                    &ModelReference::Qualified(reference.clone()),
                                    item_definitions.clone(),
                                );
                            }
                        }
                        for port in &event.inputs {
                            if let Some(reference) = port
                                .item_aware
                                .as_ref()
                                .and_then(|item| item.item_subject_ref.as_ref())
                            {
                                check(
                                    port.id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.input.itemSubjectRef",
                                    &ModelReference::Qualified(reference.clone()),
                                    item_definitions.clone(),
                                );
                            }
                        }
                        if let Some(flow) = &event.flow {
                            for reference in &flow.category_value_refs {
                                check(
                                    event_owner.clone(),
                                    "implicitEvent.categoryValueRef",
                                    reference,
                                    categories.clone(),
                                );
                            }
                        }
                        for (field, references, incoming) in [
                            ("implicitEvent.incoming", &event.incoming, true),
                            ("implicitEvent.outgoing", &event.outgoing, false),
                        ] {
                            let candidates: Vec<_> = scope
                                .flows
                                .iter()
                                .filter(|flow| {
                                    flow.kind == FlowKind::Sequence
                                        && event.id.as_ref().is_some_and(|id| {
                                            if incoming {
                                                flow.target.as_ref()
                                                    == Some(
                                                        &crate::bpmn_model::EventReference::Local(
                                                            id.selector(),
                                                        ),
                                                    )
                                            } else {
                                                flow.source.as_ref()
                                                    == Some(
                                                        &crate::bpmn_model::EventReference::Local(
                                                            id.selector(),
                                                        ),
                                                    )
                                            }
                                        })
                                })
                                .map(|flow| &flow.id)
                                .collect();
                            for reference in references {
                                check(event_owner.clone(), field, reference, candidates.clone());
                            }
                        }
                        for definition in &event.definitions {
                            let event_reference = match definition {
                                EventDefinition::Message {
                                    message_ref: Some(reference),
                                    ..
                                } => {
                                    Some(("implicitEvent.messageRef", reference, messages.clone()))
                                }
                                EventDefinition::Signal {
                                    signal_ref: Some(reference),
                                    ..
                                } => Some(("implicitEvent.signalRef", reference, signals.clone())),
                                EventDefinition::Error {
                                    error_ref: Some(reference),
                                    ..
                                } => Some(("implicitEvent.errorRef", reference, errors.clone())),
                                EventDefinition::Escalation {
                                    escalation_ref: Some(reference),
                                    ..
                                } => Some((
                                    "implicitEvent.escalationRef",
                                    reference,
                                    escalations.clone(),
                                )),
                                _ => None,
                            };
                            if let Some((field, reference, candidates)) = event_reference {
                                check(event_owner.clone(), field, reference, candidates);
                            }
                            if let EventDefinition::Compensate {
                                activity_ref: Some(reference),
                                ..
                            } = definition
                            {
                                let activities: Vec<_> = scope
                                    .nodes
                                    .iter()
                                    .filter(|candidate| candidate.kind.is_activity())
                                    .map(|candidate| &candidate.id)
                                    .collect();
                                check(
                                    event_owner.clone(),
                                    "implicitEvent.activityRef",
                                    reference,
                                    activities,
                                );
                            }
                            if let EventDefinition::Link {
                                sources, target, ..
                            } = definition
                            {
                                let links: Vec<_> = scope
                                    .nodes
                                    .iter()
                                    .filter_map(|candidate| match &candidate.details {
                                        Some(NodeDetails::Event { definitions, .. }) => {
                                            Some(definitions)
                                        }
                                        _ => None,
                                    })
                                    .flatten()
                                    .filter_map(|definition| match definition {
                                        EventDefinition::Link { id, .. } => id.as_ref(),
                                        _ => None,
                                    })
                                    .collect();
                                for reference in sources.iter().chain(target.iter()) {
                                    check(
                                        event_owner.clone(),
                                        "implicitEvent.linkRef",
                                        reference,
                                        links.clone(),
                                    );
                                }
                            }
                            if let EventDefinition::Message {
                                operation_ref: Some(reference),
                                ..
                            } = definition
                            {
                                check(
                                    event_owner.clone(),
                                    "implicitEvent.operationRef",
                                    reference,
                                    operations.clone(),
                                );
                            }
                        }
                        for reference in &event.definition_refs {
                            check(
                                event_owner.clone(),
                                "implicitEvent.eventDefinitionRef",
                                reference,
                                event_definitions.clone(),
                            );
                        }
                        let input_ids: Vec<_> = event
                            .inputs
                            .iter()
                            .filter_map(|port| port.id.as_ref())
                            .collect();
                        if let Some(set) = &event.input_set {
                            for reference in &set.counterpart_refs {
                                check(
                                    set.id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.inputSetCounterpart",
                                    &ModelReference::Local(reference.clone()),
                                    all_output_set_ids.clone(),
                                );
                            }
                            for reference in set
                                .data_refs
                                .iter()
                                .chain(&set.optional_refs)
                                .chain(&set.while_executing_refs)
                            {
                                check(
                                    set.id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.inputSetRef",
                                    &ModelReference::Local(reference.clone()),
                                    input_ids.clone(),
                                );
                            }
                        }
                        for association in &event.input_associations {
                            for source in &association.sources {
                                check(
                                    association
                                        .id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.associationSource",
                                    &ModelReference::Local(source.clone()),
                                    {
                                        let mut candidates = visible_items.clone();
                                        candidates.extend(
                                            [input_data_item, output_data_item]
                                                .into_iter()
                                                .flatten()
                                                .filter_map(|port| port.id.as_ref()),
                                        );
                                        if let Some(activity) = &node.activity {
                                            candidates.extend(
                                                activity
                                                    .properties
                                                    .iter()
                                                    .filter_map(|property| property.id.as_ref()),
                                            );
                                        }
                                        if let Some(io) = &node.io_specification {
                                            candidates.extend(
                                                io.inputs
                                                    .iter()
                                                    .chain(&io.outputs)
                                                    .filter_map(|port| port.id.as_ref()),
                                            );
                                        }
                                        candidates.extend(
                                            event
                                                .properties
                                                .iter()
                                                .filter_map(|property| property.id.as_ref()),
                                        );
                                        candidates
                                    },
                                );
                            }
                            if let Some(target) = &association.target {
                                check(
                                    association
                                        .id
                                        .as_ref()
                                        .map_or_else(|| event_owner.clone(), |id| id.selector()),
                                    "implicitEvent.associationTarget",
                                    &ModelReference::Local(target.clone()),
                                    input_ids.clone(),
                                );
                            }
                        }
                    }
                }
            }
            if let Some(metadata) = &node.flow_metadata {
                for (field, references, incoming) in [
                    ("flow.incoming", &metadata.incoming, true),
                    ("flow.outgoing", &metadata.outgoing, false),
                ] {
                    let candidates: Vec<_> = scope
                        .flows
                        .iter()
                        .filter(|flow| {
                            flow.kind == FlowKind::Sequence
                                && if incoming {
                                    flow.target.as_ref()
                                        == Some(&crate::bpmn_model::EventReference::Local(
                                            node.id.selector(),
                                        ))
                                } else {
                                    flow.source.as_ref()
                                        == Some(&crate::bpmn_model::EventReference::Local(
                                            node.id.selector(),
                                        ))
                                }
                        })
                        .map(|flow| &flow.id)
                        .collect();
                    for reference in references {
                        check(node.id.selector(), field, reference, candidates.clone());
                    }
                }
                for reference in &metadata.category_value_refs {
                    check(
                        node.id.selector(),
                        "flow.categoryValueRef",
                        reference,
                        categories.clone(),
                    );
                }
            }
        }
        for flow in scope.flows {
            if let Some(metadata) = &flow.flow_metadata {
                for reference in &metadata.category_value_refs {
                    check(
                        flow.id.selector(),
                        "flow.categoryValueRef",
                        reference,
                        categories.clone(),
                    );
                }
            }
        }
    }
    (issues, resolutions)
}
