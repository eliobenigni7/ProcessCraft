//! Writers for collaboration, participants, conversations, message flows and collaboration artifacts.
use crate::interchange::export::ExportContext;
use crate::interchange::export::elements::{
    anonymous_base_attributes, anonymous_base_children, anonymous_extras_attributes,
    finish_anonymous_base_empty, write_correlation_key, write_xml_id, write_xml_name,
    xml_reference,
};
use crate::interchange::export::scope::write_scope;
use crate::interchange::export::xml::{esc, write_doc};
use crate::ns::BPMN;
use processcraft_core::bpmn_model::{EventReference, QNameRef};
use processcraft_core::ids::{ElementSelector, XmlId};
use processcraft_core::model::*;
pub(super) fn write_collaboration_artifacts(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    if let Some(extras) = extras {
        for artifact in &extras.artifacts {
            match artifact {
                CollaborationArtifact::Group(group) => write_scope(
                    xml,
                    &[],
                    &[],
                    &[],
                    &[],
                    std::slice::from_ref(group),
                    context,
                    namespace,
                ),
                CollaborationArtifact::TextAnnotation(node) => write_scope(
                    xml,
                    std::slice::from_ref(node.as_ref()),
                    &[],
                    &[],
                    &[],
                    &[],
                    context,
                    namespace,
                ),
                CollaborationArtifact::Association(flow) => write_scope(
                    xml,
                    &[],
                    std::slice::from_ref(flow.as_ref()),
                    &[],
                    &[],
                    &[],
                    context,
                    namespace,
                ),
            }
        }
    }
}
pub(super) fn write_participant_association(
    xml: &mut String,
    value: &processcraft_core::bpmn_collaboration::ParticipantAssociation,
    namespace: &str,
    context: &ExportContext,
) {
    xml.push_str(&format!("<participantAssociation xmlns=\"{BPMN}\""));
    if let Some(id) = &value.id {
        write_xml_id(xml, id);
    }
    anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
    if let Some(inner) = &value.inner {
        write_event_qname_text(xml, "innerParticipantRef", inner, namespace, context);
    }
    if let Some(outer) = &value.outer {
        write_event_qname_text(xml, "outerParticipantRef", outer, namespace, context);
    }
    xml.push_str("</participantAssociation>");
}
pub(super) fn write_collaboration_associations(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    let Some(extras) = extras else {
        return;
    };
    for value in &extras.conversation_associations {
        let refs: Vec<_> = [
            ("innerConversationNodeRef", value.inner.as_ref()),
            ("outerConversationNodeRef", value.outer.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag = write_collaboration_qname_owner(
            xml,
            "conversationAssociation",
            &refs,
            namespace,
            context,
            &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
                scopes
            },
        );
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        finish_anonymous_base_empty(xml, &tag, value.id.as_ref(), value.base.as_ref());
    }
    for value in &extras.participant_associations {
        write_participant_association(xml, value, namespace, context);
    }
    for value in &extras.message_flow_associations {
        let refs: Vec<_> = [
            ("innerMessageFlowRef", value.inner.as_ref()),
            ("outerMessageFlowRef", value.outer.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag = write_collaboration_qname_owner(
            xml,
            "messageFlowAssociation",
            &refs,
            namespace,
            context,
            &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
                scopes
            },
        );
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        finish_anonymous_base_empty(xml, &tag, value.id.as_ref(), value.base.as_ref());
    }
}
pub(super) fn write_conversation_links(
    xml: &mut String,
    links: &[processcraft_core::bpmn_collaboration::ConversationLink],
    namespace: &str,
    context: &ExportContext,
) {
    for link in links {
        let refs: Vec<_> = [
            ("sourceRef", link.source.as_ref()),
            ("targetRef", link.target.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, r)| r.map(|r| (name, r)))
        .collect();
        let tag =
            write_collaboration_qname_owner(xml, "conversationLink", &refs, namespace, context, &{
                let mut scopes = Vec::new();
                collaboration_base_scopes(link.base.as_ref(), &mut scopes);
                scopes
            });
        write_xml_id(xml, &link.id);
        write_xml_name(xml, link.name.as_deref());
        finish_anonymous_base_empty(xml, &tag, Some(&link.id), link.base.as_ref());
    }
}
pub(super) fn write_collaboration_refs(
    xml: &mut String,
    extras: Option<&processcraft_core::bpmn_collaboration::CollaborationExtras>,
    namespace: &str,
    context: &ExportContext,
) {
    if let Some(extras) = extras {
        for reference in &extras.choreography_refs {
            write_event_qname_text(xml, "choreographyRef", reference, namespace, context);
        }
    }
}
pub(super) fn write_conversations(
    xml: &mut String,
    nodes: &[processcraft_core::bpmn_collaboration::ConversationNode],
    parent: Option<&ElementSelector>,
    namespace: &str,
    document: &ExportContext,
) {
    for node in nodes.iter().filter(|n| n.parent.as_ref() == parent) {
        let tag = match node.kind {
            processcraft_core::bpmn_collaboration::ConversationKind::Conversation => "conversation",
            processcraft_core::bpmn_collaboration::ConversationKind::SubConversation => {
                "subConversation"
            }
            processcraft_core::bpmn_collaboration::ConversationKind::CallConversation => {
                "callConversation"
            }
        };
        let refs: Vec<_> = node
            .call
            .as_ref()
            .and_then(|c| c.called_collaboration_ref.as_ref())
            .map(|r| vec![("calledCollaborationRef", r)])
            .unwrap_or_default();
        let mut scopes = Vec::new();
        collaboration_base_scopes(node.base.as_ref(), &mut scopes);
        for key in &node.correlation_keys {
            collaboration_base_scopes(key.base.as_ref(), &mut scopes);
        }
        if let Some(call) = &node.call {
            for value in &call.participant_associations {
                collaboration_base_scopes(value.base.as_ref(), &mut scopes);
            }
        }
        let tag = write_collaboration_qname_owner(xml, tag, &refs, namespace, document, &scopes);
        write_xml_id(xml, &node.id);
        write_xml_name(xml, node.name.as_deref());
        anonymous_base_attributes(xml, Some(&node.id), node.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&node.id), node.base.as_ref());
        for participant in &node.participant_refs {
            write_event_qname_text(xml, "participantRef", participant, namespace, document);
        }
        for reference in &node.message_flow_refs {
            write_event_qname_text(xml, "messageFlowRef", reference, namespace, document);
        }
        for key in &node.correlation_keys {
            write_correlation_key(xml, key, namespace, document);
        }
        write_conversations(xml, nodes, Some(&node.id.selector()), namespace, document);
        if let Some(call) = &node.call {
            for association in &call.participant_associations {
                write_participant_association(xml, association, namespace, document);
            }
        }
        xml.push_str(&format!("</{tag}>"));
    }
}
pub(super) fn local_qname_value(id: &XmlId, namespace: &str) -> String {
    if namespace.is_empty() {
        id.to_string()
    } else {
        format!("tns:{id}")
    }
}
pub(super) fn write_local_qname_open(xml: &mut String, tag: &str, namespace: &str) -> String {
    if namespace.is_empty() {
        xml.push_str(&format!("<pc_q:{tag} xmlns:pc_q=\"{BPMN}\" xmlns=\"\""));
        format!("pc_q:{tag}")
    } else {
        xml.push_str(&format!("<{tag} xmlns:tns=\"{}\"", esc(namespace)));
        tag.to_owned()
    }
}
pub(in crate::interchange) fn collaboration_base_scopes<'a>(
    base: Option<&'a processcraft_core::bpmn_metadata::BaseMetadata>,
    scopes: &mut Vec<&'a [processcraft_core::bpmn_xml::NamespaceBinding]>,
) {
    use processcraft_core::bpmn_xml::MixedXml;
    if let Some(base) = base {
        if !base.namespaces.is_empty() {
            scopes.push(&base.namespaces);
        }
        for doc in &base.documentation {
            if let MixedXml::Structured(content) = &doc.content {
                scopes.push(&content.namespaces);
            }
        }
        if let Some(ext) = &base.extension_elements {
            scopes.push(&ext.content.namespaces);
        }
    }
}
pub(in crate::interchange) fn collaboration_flow_scopes(
    flow: &Flow,
) -> Vec<&[processcraft_core::bpmn_xml::NamespaceBinding]> {
    use processcraft_core::{bpmn_metadata::DocumentationSet, bpmn_xml::MixedXml};
    let mut scopes = Vec::new();
    if let DocumentationSet::Structured(docs) = &flow.documentation {
        for doc in docs {
            if let MixedXml::Structured(content) = &doc.content {
                scopes.push(content.namespaces.as_slice());
            }
        }
    }
    if let Some(extras) = &flow.base_extras {
        if !extras.namespaces.is_empty() {
            scopes.push(&extras.namespaces);
        }
        if let Some(ext) = &extras.extension_elements {
            scopes.push(&ext.content.namespaces);
        }
    }
    scopes
}
pub(super) fn write_collaboration_qname_owner(
    xml: &mut String,
    tag: &str,
    refs: &[(&str, &EventReference)],
    namespace: &str,
    context: &ExportContext,
    scopes: &[&[processcraft_core::bpmn_xml::NamespaceBinding]],
) -> String {
    let values: Vec<_> = refs
        .iter()
        .map(|(name, reference)| (*name, event_reference_qname(reference, namespace, context)))
        .collect();
    let empty_default = values.iter().any(|(_, q)| q.namespace.is_empty());
    let mut prefixes = std::collections::BTreeMap::<String, String>::new();
    let mut next_prefix = |uri: &str| -> String {
        if let Some(prefix) = prefixes.get(uri) {
            return prefix.clone();
        }
        let prefix = scopes
            .first()
            .and_then(|first| {
                first
                    .iter()
                    .find(|binding| {
                        !binding.prefix.is_empty()
                            && binding.prefix != "xml"
                            && binding.uri == uri
                            && !prefixes.values().any(|used| used == &binding.prefix)
                            && scopes
                                .iter()
                                .all(|scope| scope.iter().any(|n| n.prefix == binding.prefix))
                    })
                    .or_else(|| {
                        first.iter().find(|binding| {
                            !binding.prefix.is_empty()
                                && binding.prefix != "xml"
                                && !prefixes.values().any(|used| used == &binding.prefix)
                                && scopes
                                    .iter()
                                    .all(|scope| scope.iter().any(|n| n.prefix == binding.prefix))
                        })
                    })
            })
            .map(|binding| binding.prefix.clone())
            .unwrap_or_else(|| {
                let mut i = prefixes.len();
                loop {
                    let p = format!("pc_cref{i}");
                    if !prefixes.values().any(|used| used == &p) {
                        break p;
                    }
                    i += 1;
                }
            });
        prefixes.insert(uri.to_owned(), prefix.clone());
        prefix
    };
    let owner = if empty_default {
        format!("{}:{tag}", next_prefix(BPMN))
    } else {
        tag.to_owned()
    };
    let mut attrs = Vec::new();
    for (name, q) in values {
        let lexical = if q.namespace.is_empty() || (!empty_default && q.namespace == BPMN) {
            q.local.to_string()
        } else {
            format!("{}:{}", next_prefix(&q.namespace), q.local)
        };
        attrs.push((name, lexical));
    }
    xml.push('<');
    xml.push_str(&owner);
    if empty_default {
        xml.push_str(" xmlns=\"\"");
    }
    for (uri, prefix) in prefixes {
        xml.push_str(&format!(" xmlns:{prefix}=\"{}\"", esc(&uri)));
    }
    for (name, value) in attrs {
        xml.push_str(&format!(" {name}=\"{}\"", esc(&value)));
    }
    owner
}
pub(super) fn write_participant(
    xml: &mut String,
    participant: &Participant,
    namespace: &str,
    document: &ExportContext,
) {
    let refs: Vec<_> = participant
        .process
        .iter()
        .map(|r| ("processRef", r))
        .collect();
    let mut scopes = Vec::new();
    collaboration_base_scopes(participant.base.as_ref(), &mut scopes);
    if let Some(multiplicity) = &participant.multiplicity {
        collaboration_base_scopes(multiplicity.base.as_ref(), &mut scopes);
    }
    let tag =
        write_collaboration_qname_owner(xml, "participant", &refs, namespace, document, &scopes);
    write_xml_id(xml, &participant.id);
    write_xml_name(xml, participant.name.as_deref());
    anonymous_base_attributes(xml, Some(&participant.id), participant.base.as_ref());
    xml.push('>');
    anonymous_base_children(xml, Some(&participant.id), participant.base.as_ref());
    for reference in &participant.interface_refs {
        write_event_qname_text(xml, "interfaceRef", reference, namespace, document);
    }
    for reference in &participant.endpoint_refs {
        write_event_qname_text(xml, "endPointRef", reference, namespace, document);
    }
    if let Some(value) = &participant.multiplicity {
        xml.push_str(&format!("<participantMultiplicity xmlns=\"{BPMN}\""));
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        if let Some(minimum) = value.minimum {
            xml.push_str(&format!(" minimum=\"{minimum}\""));
        }
        if let Some(maximum) = value.maximum {
            xml.push_str(&format!(" maximum=\"{maximum}\""));
        }
        finish_anonymous_base_empty(
            xml,
            "participantMultiplicity",
            value.id.as_ref(),
            value.base.as_ref(),
        );
    }
    xml.push_str(&format!("</{tag}>"));
}
pub(super) fn write_message_flow(
    xml: &mut String,
    flow: &Flow,
    namespace: &str,
    document: &ExportContext,
) {
    let refs: Vec<_> = [
        ("sourceRef", flow.source.as_ref()),
        ("targetRef", flow.target.as_ref()),
        ("messageRef", flow.message_ref.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, r)| r.map(|r| (name, r)))
    .collect();
    let tag = write_collaboration_qname_owner(
        xml,
        "messageFlow",
        &refs,
        namespace,
        document,
        &collaboration_flow_scopes(flow),
    );
    write_xml_id(xml, &flow.id);
    write_xml_name(xml, flow.name.as_deref());
    anonymous_extras_attributes(xml, &flow.id, flow.base_extras.as_ref());
    xml.push('>');
    write_doc(xml, &flow.documentation, flow.base_extras.as_ref());
    xml.push_str(&format!("</{tag}>"));
}
pub(super) fn event_reference_qname(
    reference: &EventReference,
    namespace: &str,
    context: &ExportContext,
) -> QNameRef {
    match reference {
        EventReference::Local(selector) => QNameRef {
            namespace: namespace.to_owned(),
            local: xml_reference(context, selector).clone(),
        },
        EventReference::Qualified(qname) => qname.clone(),
    }
}
pub(super) fn write_event_qname_text(
    xml: &mut String,
    tag: &str,
    reference: &EventReference,
    namespace: &str,
    context: &ExportContext,
) {
    let qname = event_reference_qname(reference, namespace, context);
    xml.push_str(&format!(
        "<pc_event:{tag} xmlns:pc_event=\"{BPMN}\" xmlns=\"{}\">{}</pc_event:{tag}>",
        esc(&qname.namespace),
        qname.local
    ));
}
