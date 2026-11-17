//! Writer for the ordered choreography graph and its nested scopes.
use crate::interchange::export::ExportContext;
use crate::interchange::export::collaboration::{
    collaboration_base_scopes, write_collaboration_artifacts, write_collaboration_qname_owner,
    write_event_qname_text, write_participant_association,
};
use crate::interchange::export::elements::{
    anonymous_base_attributes, anonymous_base_children, write_correlation_key, write_flow_metadata,
    write_xml_id, write_xml_name,
};
use crate::interchange::export::scope::write_scope;
use crate::interchange::shared::{MAX_CHOREOGRAPHY_DEPTH, choreography_child_scopes};
use processcraft_core::bpmn_model::EventReference;
use processcraft_core::model::*;
pub(super) fn write_choreography_graph(
    xml: &mut String,
    graph: &processcraft_core::bpmn_collaboration::ChoreographyGraph,
    namespace: &str,
    context: &ExportContext,
    depth: usize,
) -> bool {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails as Details, ChoreographyFlowElement as Element,
        CollaborationExtras,
    };
    if depth > MAX_CHOREOGRAPHY_DEPTH && !graph.flow_elements.is_empty() {
        return false;
    }
    for element in &graph.flow_elements {
        match element {
            Element::Activity(a) => {
                let Some(initiator) = a.initiating_participant.as_ref() else {
                    return false;
                };
                if a.participant_refs.len() < 2 {
                    return false;
                }
                let mut refs = vec![("initiatingParticipantRef", initiator)];
                let tag = match &a.details {
                    None => {
                        if !(1..=2).contains(&a.message_flow_refs.len()) {
                            return false;
                        }
                        "choreographyTask"
                    }
                    Some(Details::Sub { .. }) => {
                        if !a.message_flow_refs.is_empty() {
                            return false;
                        }
                        "subChoreography"
                    }
                    Some(Details::Call {
                        called_choreography_ref,
                        ..
                    }) => {
                        if !a.message_flow_refs.is_empty() {
                            return false;
                        }
                        if let Some(called) = called_choreography_ref {
                            refs.push(("calledChoreographyRef", called));
                        }
                        "callChoreography"
                    }
                };
                let mut scopes = Vec::new();
                collaboration_base_scopes(a.base.as_ref(), &mut scopes);
                for key in &a.correlation_keys {
                    collaboration_base_scopes(key.base.as_ref(), &mut scopes);
                }
                if let Some(Details::Sub { graph, artifacts }) = &a.details {
                    choreography_child_scopes(graph, artifacts, &mut scopes);
                }
                let tag =
                    write_collaboration_qname_owner(xml, tag, &refs, namespace, context, &scopes);
                write_xml_id(xml, &a.id);
                write_xml_name(xml, a.name.as_deref());
                if let Some(loop_type) = a.loop_type {
                    use processcraft_core::bpmn_collaboration::ChoreographyLoopType;
                    let value = match loop_type {
                        ChoreographyLoopType::None => "None",
                        ChoreographyLoopType::Standard => "Standard",
                        ChoreographyLoopType::MultiInstanceSequential => "MultiInstanceSequential",
                        ChoreographyLoopType::MultiInstanceParallel => "MultiInstanceParallel",
                    };
                    xml.push_str(&format!(" loopType=\"{value}\""));
                }
                anonymous_base_attributes(xml, Some(&a.id), a.base.as_ref());
                xml.push('>');
                anonymous_base_children(xml, Some(&a.id), a.base.as_ref());
                write_flow_metadata(xml, a.flow_metadata.as_ref(), namespace, context);
                for reference in &a.participant_refs {
                    write_event_qname_text(xml, "participantRef", reference, namespace, context);
                }
                for key in &a.correlation_keys {
                    write_correlation_key(xml, key, namespace, context);
                }
                match &a.details {
                    None => {
                        for reference in &a.message_flow_refs {
                            write_event_qname_text(
                                xml,
                                "messageFlowRef",
                                reference,
                                namespace,
                                context,
                            );
                        }
                    }
                    Some(Details::Sub { graph, artifacts }) => {
                        if !write_choreography_graph(xml, graph, namespace, context, depth + 1) {
                            return false;
                        }
                        let extras = CollaborationExtras {
                            artifacts: artifacts.clone(),
                            ..Default::default()
                        };
                        write_collaboration_artifacts(xml, Some(&extras), namespace, context);
                    }
                    Some(Details::Call {
                        participant_associations,
                        ..
                    }) => {
                        for association in participant_associations {
                            if association.inner.is_none() || association.outer.is_none() {
                                return false;
                            }
                            write_participant_association(xml, association, namespace, context);
                        }
                    }
                }
                xml.push_str(&format!("</{tag}>"));
            }
            Element::Event(node) | Element::Gateway(node) => {
                let valid = match element {
                    Element::Event(_) => matches!(
                        node.kind,
                        NodeKind::StartEvent
                            | NodeKind::EndEvent
                            | NodeKind::IntermediateCatchEvent
                            | NodeKind::IntermediateThrowEvent
                            | NodeKind::BoundaryEvent
                    ),
                    Element::Gateway(_) => matches!(
                        node.kind,
                        NodeKind::ExclusiveGateway
                            | NodeKind::InclusiveGateway
                            | NodeKind::ParallelGateway
                            | NodeKind::ComplexGateway
                            | NodeKind::EventBasedGateway
                    ),
                    _ => false,
                };
                if !valid {
                    return false;
                }
                write_scope(
                    xml,
                    std::slice::from_ref(node.as_ref()),
                    &[],
                    &[],
                    &graph.data_associations,
                    &[],
                    context,
                    namespace,
                );
            }
            Element::SequenceFlow(flow) => {
                if flow.kind != FlowKind::Sequence
                    || !matches!(flow.source, Some(EventReference::Local(_)))
                    || !matches!(flow.target, Some(EventReference::Local(_)))
                {
                    return false;
                }
                write_scope(
                    xml,
                    &[],
                    std::slice::from_ref(flow.as_ref()),
                    &[],
                    &[],
                    &[],
                    context,
                    namespace,
                );
            }
        }
    }
    true
}
