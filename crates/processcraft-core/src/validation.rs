//! Non-blocking diagnostics: incomplete editing remains possible; structural validity is separate.
use crate::{
    bpmn_model::{EventDefinition, EventGatewayType, EventIo, GatewayDirection, NodeDetails},
    ids::{DocumentId, ElementSelector},
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
                    "unresolvedActivityFamilyReference",
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
            for choreography in &doc.choreographies {
                emit(
                    choreography.id.selector(),
                    "choreographyCoverageLimited",
                    "Choreography semantics are only partially checked; BPMN 2.0.2 §2.5 and Clause 11.",
                );
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
            let scopes = queries::scopes(doc);
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
                for flow in scope.flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
                    incoming.entry(&flow.target).or_default().push(flow);
                    outgoing.entry(&flow.source).or_default().push(flow);
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
                        let event_subprocess = scopes.iter().flat_map(|s| s.nodes).any(|owner| {
                            owner.id.selector() == scope.id.selector()
                                && owner
                                    .activity
                                    .as_ref()
                                    .is_some_and(|a| a.triggered_by_event == Some(true))
                        });
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
                            if let Some(target) = scope.nodes.iter().find(|n| n.id == flow.target) {
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
fn activity_family_reference_scan(
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
    let process_ids: Vec<_> = doc.processes.iter().map(|p| &p.id).collect();
    let collaboration_ids: Vec<_> = doc.collaboration.iter().map(|c| &c.id).collect();
    let callable_ids: Vec<_> = doc
        .processes
        .iter()
        .map(|process| &process.id)
        .chain(doc.global_tasks.iter().filter_map(|task| task.id.as_ref()))
        .collect();
    let mut keys = Vec::new();
    if let Some(collaboration) = &doc.collaboration {
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
        for activity in &choreography.activities {
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
    for choreography in &doc.choreographies {
        for activity in &choreography.activities {
            if let Some(metadata) = &activity.flow_metadata {
                for (field, references, incoming) in [
                    ("flow.incoming", &metadata.incoming, true),
                    ("flow.outgoing", &metadata.outgoing, false),
                ] {
                    let candidates: Vec<_> = choreography
                        .sequence_flows
                        .iter()
                        .filter(|flow| {
                            flow.kind == FlowKind::Sequence
                                && if incoming {
                                    flow.target == activity.id.selector()
                                } else {
                                    flow.source == activity.id.selector()
                                }
                        })
                        .map(|flow| &flow.id)
                        .collect();
                    for reference in references {
                        check(activity.id.selector(), field, reference, candidates.clone());
                    }
                }
                for reference in &metadata.category_value_refs {
                    check(
                        activity.id.selector(),
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
                                                flow.target == id.selector()
                                            } else {
                                                flow.source == id.selector()
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
                                    flow.target == node.id.selector()
                                } else {
                                    flow.source == node.id.selector()
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
