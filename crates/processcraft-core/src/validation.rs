//! Non-blocking diagnostics: incomplete editing remains possible; structural validity is separate.
use crate::{
    bpmn_model::{EventDefinition, EventGatewayType, GatewayDirection, NodeDetails},
    ids::{DocumentId, XmlId},
    model::{Capability, FlowKind, NodeKind, ProjectData},
    queries,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

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
                .any(|e| matches!(e, EventDefinitionResource::Message { id, .. } if id == &r.local))
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
                .any(|o| o.id.as_ref() == Some(&r.local))
    }) {
        issues.push("missingLocalActivityOperation");
    }
    issues
}
use crate::bpmn_model::EventResource as EventDefinitionResource;

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
    pub element_id: Option<XmlId>,
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
}
impl ProjectData {
    /// Reports bounded profile checks and advisory incomplete-model conditions; this is not BPMN certification.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut results = Vec::new();
        for doc in &self.documents {
            let mut emit = |id: &XmlId, code, message: &str| {
                results.push(Diagnostic {
                    document_id: doc.id,
                    element_id: Some(id.clone()),
                    severity: DiagnosticSeverity::Warning,
                    code,
                    message: message.into(),
                })
            };
            if !matches!(doc.capability, Capability::Editable) {
                emit(
                    &doc.definitions_id,
                    "semanticCoverageLimited",
                    "Retained or preserved source cannot be fully checked; BPMN 2.0.2 §2.2.4–2.2.8.",
                );
            }
            for expression in doc
                .expressions()
                .into_iter()
                .filter(|e| e.trim().is_empty())
                .take(10_000)
            {
                let id = match expression {
                    crate::bpmn_model::Expression::Structured(data) => {
                        data.id.as_ref().unwrap_or(&doc.definitions_id)
                    }
                    crate::bpmn_model::Expression::Legacy(_) => &doc.definitions_id,
                };
                emit(
                    id,
                    "emptyExpression",
                    "Expression has no content (editing advisory); BPMN 2.0.2 §8.4.6.",
                );
            }
            if let Some(collaboration) = &doc.collaboration {
                emit(
                    &collaboration.id,
                    "collaborationCoverageLimited",
                    "Collaboration and conversation semantics are only partially checked; BPMN 2.0.2 Clause 9.",
                );
            }
            for choreography in &doc.choreographies {
                emit(
                    &choreography.id,
                    "choreographyCoverageLimited",
                    "Choreography semantics are only partially checked; BPMN 2.0.2 §2.5 and Clause 11.",
                );
            }
            if !doc.data_stores.is_empty() {
                emit(
                    &doc.definitions_id,
                    "dataCoverageLimited",
                    "Data semantics and assignment rules are only partially checked; BPMN 2.0.2 §10.3.",
                );
            }
            let process_ids: HashSet<_> = doc.processes.iter().map(|p| &p.id).collect();
            for scope in queries::scopes(doc) {
                if !scope.data_objects.is_empty()
                    || !scope.data_associations.is_empty()
                    || scope.nodes.iter().any(|n| n.io_specification.is_some())
                {
                    emit(
                        scope.id,
                        "dataCoverageLimited",
                        "Data and I/O association semantics are only partially checked; BPMN 2.0.2 §10.3.",
                    );
                }
                let mut incoming: HashMap<&XmlId, Vec<_>> = HashMap::new();
                let mut outgoing: HashMap<&XmlId, Vec<_>> = HashMap::new();
                for flow in scope.flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
                    incoming.entry(&flow.target).or_default().push(flow);
                    outgoing.entry(&flow.source).or_default().push(flow);
                }
                let mut has_start = false;
                let mut has_end = false;
                for node in scope.nodes {
                    if let Some(activity) = &node.activity {
                        for issue in activity_reference_issues(doc, activity) {
                            emit(
                                &node.id,
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
                                &node.id,
                                "activityBindingCompatibilityUnverified",
                                "Message and operation I/O compatibility is not fully checked; BPMN 2.0.2 §10.3.3–10.3.4.",
                            );
                        }
                    }
                    let ins = incoming.get(&node.id).map_or(&[][..], Vec::as_slice);
                    let outs = outgoing.get(&node.id).map_or(&[][..], Vec::as_slice);
                    if let Some(NodeDetails::Event { definitions, .. }) = &node.details {
                        for definition in definitions {
                            if let EventDefinition::Link {
                                id,
                                name,
                                sources,
                                target,
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
                                        &node.id,
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
                                        &node.id,
                                        "linkSequenceDirection",
                                        "Link catch events have no incoming and link throw events have no outgoing sequence flow; BPMN 2.0.2 §10.5.5.",
                                    );
                                }
                            }
                        }
                    }
                    has_start |= matches!(node.kind, NodeKind::StartEvent);
                    has_end |= matches!(node.kind, NodeKind::EndEvent);
                    if ins.is_empty()
                        && outs.is_empty()
                        && !matches!(
                            node.kind,
                            NodeKind::TextAnnotation
                                | NodeKind::DataObjectReference
                                | NodeKind::DataStoreReference
                                | NodeKind::BoundaryEvent
                        )
                    {
                        emit(
                            &node.id,
                            "unconnectedNode",
                            "Element has no sequence-flow connection (editing advisory).",
                        );
                    }
                    if matches!(node.kind, NodeKind::ExclusiveGateway)
                        && node.name.trim().is_empty()
                    {
                        emit(
                            &node.id,
                            "unnamedExclusiveGateway",
                            "Exclusive gateway has no name (editing advisory).",
                        );
                    }
                    if matches!(node.kind, NodeKind::StartEvent) {
                        // Expanded-subprocess boundary start exception cannot be distinguished in this model; only check root scopes.
                        if process_ids.contains(scope.id) && !ins.is_empty() {
                            emit(
                                &node.id,
                                "startHasIncoming",
                                "Start Event has incoming sequence flow; BPMN 2.0.2 §10.5.2, p. 244.",
                            );
                        }
                        if outs.iter().any(|flow| flow.condition.is_some()) {
                            emit(
                                &node.id,
                                "startConditionalFlow",
                                "Start Event outgoing sequence flow has a condition; BPMN 2.0.2 §10.5.2, p. 244.",
                            );
                        }
                    }
                    if matches!(node.kind, NodeKind::EndEvent) && !outs.is_empty() {
                        emit(
                            &node.id,
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
                            &node.id,
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
                                .any(|flow| &flow.id == id && flow.condition.is_some())
                        {
                            emit(
                                id,
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
                                &node.id,
                                "gatewayDirectionMismatch",
                                "Gateway connections do not match its declared direction; BPMN 2.0.2 §10.6.1, p. 289.",
                            );
                        }
                        if matches!(node.kind, NodeKind::EventBasedGateway)
                            && *instantiate == Some(true)
                            && !ins.is_empty()
                        {
                            emit(
                                &node.id,
                                "instantiatingGatewayIncoming",
                                "An instantiating event gateway has incoming sequence flow; BPMN 2.0.2 §10.6.6, p. 298.",
                            );
                        }
                        if matches!(node.kind, NodeKind::EventBasedGateway)
                            && *event_gateway_type == Some(EventGatewayType::Parallel)
                            && *instantiate != Some(true)
                        {
                            emit(
                                &node.id,
                                "parallelEventGatewayNotInstantiating",
                                "A parallel event gateway must instantiate its process; BPMN 2.0.2 §10.6.6, p. 298.",
                            );
                        }
                    }
                    if matches!(node.kind, NodeKind::EventBasedGateway) {
                        if outs.len() < 2 {
                            emit(
                                &node.id,
                                "eventGatewayTooFewOutgoing",
                                "Event-based gateway needs at least two outgoing sequence flows; BPMN 2.0.2 §10.6.6, p. 296.",
                            );
                        }
                        for flow in outs {
                            if flow.condition.is_some() {
                                emit(
                                    &flow.id,
                                    "eventGatewayConditionalFlow",
                                    "Event-based gateway outgoing flow has a condition; BPMN 2.0.2 §10.6.6, p. 296.",
                                );
                            }
                            if let Some(target) = scope.nodes.iter().find(|n| n.id == flow.target) {
                                if !matches!(
                                    target.kind,
                                    NodeKind::IntermediateCatchEvent | NodeKind::ReceiveTask
                                ) {
                                    emit(
                                        &flow.id,
                                        "eventGatewayInvalidTarget",
                                        "Event-based gateway target is neither an intermediate catch event nor receive task; BPMN 2.0.2 §10.6.6, p. 297.",
                                    );
                                }
                                if incoming
                                    .get(&target.id)
                                    .is_some_and(|flows| flows.len() > 1)
                                {
                                    emit(
                                        &target.id,
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
                                        &target.id,
                                        "eventGatewayInvalidTrigger",
                                        "Event-gateway target includes an unsupported trigger for this configuration; BPMN 2.0.2 §10.6.6, p. 297.",
                                    );
                                }
                            }
                        }
                    }
                }
                // Advisory only: BPMN permits processes without an explicit Start Event (p. 245).
                if process_ids.contains(scope.id) && !has_start {
                    emit(
                        scope.id,
                        "missingStart",
                        "Process has no explicit start event (editing advisory; BPMN 2.0.2 §10.5.2 permits omission).",
                    );
                }
                if process_ids.contains(scope.id) && !has_end {
                    emit(
                        scope.id,
                        "missingEnd",
                        "Process has no explicit end event (editing advisory).",
                    );
                }
            }
        }
        results
    }
}
