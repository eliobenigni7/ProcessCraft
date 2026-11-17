//! Items used by both the import and export halves: scopes, ports and pending base metadata.
use crate::interchange::export::collaboration::{
    collaboration_base_scopes, collaboration_flow_scopes,
};
use processcraft_core::bpmn_model::{EventDefinition, EventIo, EventReference, NodeDetails};
use processcraft_core::ids::{ElementIdentity, ElementSelector};
use processcraft_core::model::*;
pub(super) const ANONYMOUS_CONTEXT_MARKER: &str = "data-processcraft-internal-context";
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingBase {
    pub(super) docs: Vec<processcraft_core::bpmn_metadata::Documentation>,
    pub(super) extras: processcraft_core::bpmn_metadata::BaseExtras,
}
pub(super) struct Scope<'a> {
    pub(super) id: &'a ElementIdentity,
    pub(super) nodes: &'a [Node],
    pub(super) lanes: &'a [Lane],
    pub(super) flows: &'a [Flow],
    pub(super) groups: &'a [processcraft_core::bpmn_model::Group],
    pub(super) data_objects: &'a [processcraft_core::bpmn_data::DataObject],
    pub(super) data_associations: &'a [processcraft_core::bpmn_data::DataAssociation],
}
pub(super) fn scopes(d: &BpmnDocument) -> Option<Vec<Scope<'_>>> {
    let mut pending: Vec<(Scope<'_>, usize)> = d
        .processes
        .iter()
        .map(|p| {
            (
                Scope {
                    id: &p.id,
                    nodes: &p.nodes,
                    lanes: &p.lanes,
                    flows: &p.flows,
                    groups: &p.groups,
                    data_objects: &p.data_objects,
                    data_associations: &p.data_associations,
                },
                0,
            )
        })
        .collect();
    let mut result = Vec::new();
    let mut count = 0usize;
    while let Some((scope, depth)) = pending.pop() {
        if depth > 32 {
            return None;
        }
        count = count.checked_add(scope.nodes.len() + scope.lanes.len() + scope.flows.len())?;
        if count > 10_000 {
            return None;
        }
        for n in scope.nodes {
            if let Some(NodeDetails::SubProcess { content }) = &n.details {
                if !content.data_associations.is_empty() {
                    return None;
                }
                pending.push((
                    Scope {
                        id: &n.id,
                        nodes: &content.nodes,
                        lanes: &content.lanes,
                        flows: &content.flows,
                        groups: &content.groups,
                        data_objects: &content.data_objects,
                        data_associations: &content.data_associations,
                    },
                    depth + 1,
                ));
            }
        }
        result.push(scope);
    }
    Some(result)
}
fn choreography_node_scopes<'a>(
    node: &'a Node,
    scopes: &mut Vec<&'a [processcraft_core::bpmn_xml::NamespaceBinding]>,
) {
    use processcraft_core::{bpmn_metadata::DocumentationSet, bpmn_xml::MixedXml};
    if let DocumentationSet::Structured(docs) = &node.documentation {
        for doc in docs {
            if let MixedXml::Structured(body) = &doc.content {
                scopes.push(&body.namespaces);
            }
        }
    }
    if let Some(extras) = &node.base_extras {
        if !extras.namespaces.is_empty() {
            scopes.push(&extras.namespaces);
        }
        if let Some(ext) = &extras.extension_elements {
            scopes.push(&ext.content.namespaces);
        }
    }
    if let Some(NodeDetails::TextAnnotation {
        text: Some(MixedXml::Structured(body)),
        ..
    }) = &node.details
    {
        scopes.push(&body.namespaces);
    }
}
pub(super) fn choreography_child_scopes<'a>(
    graph: &'a processcraft_core::bpmn_collaboration::ChoreographyGraph,
    artifacts: &'a [processcraft_core::bpmn_collaboration::CollaborationArtifact],
    scopes: &mut Vec<&'a [processcraft_core::bpmn_xml::NamespaceBinding]>,
) {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails as Details, ChoreographyFlowElement as Element,
        CollaborationArtifact as Artifact,
    };
    for artifact in artifacts {
        match artifact {
            Artifact::Group(group) => collaboration_base_scopes(group.base.as_ref(), scopes),
            Artifact::TextAnnotation(node) => choreography_node_scopes(node, scopes),
            Artifact::Association(flow) => scopes.extend(collaboration_flow_scopes(flow)),
        }
    }
    for element in &graph.flow_elements {
        match element {
            Element::Activity(activity) => {
                collaboration_base_scopes(activity.base.as_ref(), scopes);
                for key in &activity.correlation_keys {
                    collaboration_base_scopes(key.base.as_ref(), scopes);
                }
                if let Some(Details::Sub { graph, artifacts }) = &activity.details {
                    choreography_child_scopes(graph, artifacts, scopes);
                }
                if let Some(Details::Call {
                    participant_associations,
                    ..
                }) = &activity.details
                {
                    for association in participant_associations {
                        collaboration_base_scopes(association.base.as_ref(), scopes);
                    }
                }
            }
            Element::Event(node) | Element::Gateway(node) => choreography_node_scopes(node, scopes),
            Element::SequenceFlow(flow) => scopes.extend(collaboration_flow_scopes(flow)),
        }
    }
}
/// Shared by the XML writer and parser so export never emits what import refuses.
pub(super) const MAX_CHOREOGRAPHY_DEPTH: usize = 16;

/// Nesting levels that contain flow elements (0 for an empty graph).
pub(super) fn choreography_graph_levels(
    graph: &processcraft_core::bpmn_collaboration::ChoreographyGraph,
) -> usize {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails as Details, ChoreographyFlowElement as Element,
    };
    graph
        .flow_elements
        .iter()
        .filter_map(|element| match element {
            Element::Activity(a) => match &a.details {
                Some(Details::Sub { graph, .. }) => Some(choreography_graph_levels(graph)),
                _ => None,
            },
            _ => None,
        })
        .max()
        .unwrap_or(0)
        + usize::from(!graph.flow_elements.is_empty())
}

pub(super) fn activity_ports(
    document: &BpmnDocument,
) -> Option<std::collections::HashSet<ElementSelector>> {
    let mut ports = std::collections::HashSet::new();
    for node in scopes(document)?.iter().flat_map(|scope| scope.nodes) {
        if let Some(io) = &node.io_specification {
            ports.extend(
                io.inputs
                    .iter()
                    .chain(&io.outputs)
                    .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
            );
        }
        if let Some(NodeDetails::Event { io, .. }) = &node.details {
            match io {
                Some(EventIo::Catch { outputs, .. }) => ports.extend(
                    outputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
                ),
                Some(EventIo::Throw { inputs, .. }) => ports.extend(
                    inputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(ElementIdentity::selector)),
                ),
                None => {}
            }
        }
    }
    Some(ports)
}
pub(super) fn is_process_port(
    document: &BpmnDocument,
    plane: &ElementSelector,
    selector: &ElementSelector,
) -> bool {
    document
        .processes
        .iter()
        .filter(|p| p.id.selector() == *plane)
        .filter_map(|p| p.io_specification.as_ref())
        .flat_map(|io| io.inputs.iter().chain(&io.outputs))
        .any(|port| {
            port.id
                .as_ref()
                .is_some_and(|id| id.selector() == *selector)
        })
}
pub(super) fn event_definition_identity(definition: &EventDefinition) -> Option<&ElementIdentity> {
    match definition {
        EventDefinition::Message { id, .. }
        | EventDefinition::Signal { id, .. }
        | EventDefinition::Error { id, .. }
        | EventDefinition::Escalation { id, .. }
        | EventDefinition::Conditional { id, .. }
        | EventDefinition::Link { id, .. }
        | EventDefinition::Cancel { id, .. }
        | EventDefinition::Compensate { id, .. }
        | EventDefinition::Terminate { id, .. }
        | EventDefinition::Timer { id, .. } => id.as_ref(),
    }
}
pub(super) fn event_local_selectors(definition: &EventDefinition) -> Vec<&ElementSelector> {
    let refs: Vec<&EventReference> = match definition {
        EventDefinition::Message {
            message_ref,
            operation_ref,
            ..
        } => message_ref.iter().chain(operation_ref.iter()).collect(),
        EventDefinition::Signal { signal_ref, .. } => signal_ref.iter().collect(),
        EventDefinition::Error { error_ref, .. } => error_ref.iter().collect(),
        EventDefinition::Escalation { escalation_ref, .. } => escalation_ref.iter().collect(),
        EventDefinition::Link {
            sources, target, ..
        } => sources.iter().chain(target.iter()).collect(),
        EventDefinition::Compensate { activity_ref, .. } => activity_ref.iter().collect(),
        _ => Vec::new(),
    };
    refs.into_iter()
        .filter_map(|reference| match reference {
            EventReference::Local(selector) => Some(selector),
            EventReference::Qualified(_) => None,
        })
        .collect()
}
