//! Shared immutable traversal of newly modeled collaboration owners.
use crate::{
    bpmn_collaboration::*,
    bpmn_metadata::{BaseExtras, BaseMetadata, DocumentationSet},
    bpmn_model::Expression,
    ids::ElementIdentity,
    model::{BpmnDocument, Collaboration},
};
#[derive(Default)]
pub(crate) struct CollaborationRead<'a> {
    pub ids: Vec<&'a ElementIdentity>,
    pub bases: Vec<&'a BaseMetadata>,
    pub expressions: Vec<&'a Expression>,
    pub anonymous: usize,
    pub docs: Vec<&'a DocumentationSet>,
    pub extras: Vec<&'a BaseExtras>,
    pub base_owners: Vec<(&'a ElementIdentity, &'a Option<BaseMetadata>)>,
    pub bodies: Vec<&'a crate::bpmn_xml::MixedXml>,
    pub legacy_owners: Vec<(
        &'a ElementIdentity,
        &'a DocumentationSet,
        &'a Option<BaseExtras>,
    )>,
}
impl<'a> CollaborationRead<'a> {
    fn owner(&mut self, id: Option<&'a ElementIdentity>, base: &'a Option<BaseMetadata>) {
        if id.is_none() {
            self.anonymous = self.anonymous.saturating_add(1)
        }
        if let Some(id) = id {
            self.base_owners.push((id, base));
        }
        self.ids.extend(id);
        self.bases.extend(base.as_ref());
    }
    fn association(&mut self, id: &'a Option<ElementIdentity>, base: &'a Option<BaseMetadata>) {
        self.owner(id.as_ref(), base);
    }
    fn node(&mut self, node: &'a ConversationNode) {
        self.owner(Some(&node.id), &node.base);
        for key in &node.correlation_keys {
            self.association(&key.id, &key.base);
        }
        if let Some(call) = &node.call {
            for a in &call.participant_associations {
                self.association(&a.id, &a.base);
            }
        }
    }
    fn extras(&mut self, extras: &'a Option<CollaborationExtras>) {
        if let Some(extra) = extras {
            for a in &extra.conversation_associations {
                self.association(&a.id, &a.base)
            }
            for a in &extra.participant_associations {
                self.association(&a.id, &a.base)
            }
            for a in &extra.message_flow_associations {
                self.association(&a.id, &a.base)
            }
            for a in &extra.artifacts {
                match a {
                    CollaborationArtifact::Group(g) => self.owner(Some(&g.id), &g.base),
                    CollaborationArtifact::TextAnnotation(n) => {
                        self.ids.push(&n.id);
                        if let Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                            text: Some(body),
                            ..
                        }) = &n.details
                        {
                            self.bodies.push(body);
                        }
                        self.legacy_owners
                            .push((&n.id, &n.documentation, &n.base_extras));
                        self.docs.push(&n.documentation);
                        self.extras.extend(&n.base_extras);
                    }
                    CollaborationArtifact::Association(f) => {
                        self.ids.push(&f.id);
                        self.legacy_owners
                            .push((&f.id, &f.documentation, &f.base_extras));
                        self.docs.push(&f.documentation);
                        self.extras.extend(&f.base_extras);
                        self.expressions.extend(&f.condition)
                    }
                }
            }
        }
    }
    fn additional(&mut self, c: &'a Collaboration) {
        self.owner(Some(&c.id), &c.base);
        for p in &c.participants {
            self.owner(Some(&p.id), &p.base);
            if let Some(m) = &p.multiplicity {
                self.association(&m.id, &m.base)
            }
        }
        for f in &c.flows {
            self.ids.push(&f.id);
            self.legacy_owners
                .push((&f.id, &f.documentation, &f.base_extras));
            self.docs.push(&f.documentation);
            self.extras.extend(&f.base_extras);
            self.expressions.extend(&f.condition)
        }
        for n in &c.conversation_nodes {
            self.node(n)
        }
        for l in &c.conversation_links {
            self.owner(Some(&l.id), &l.base)
        }
        for k in &c.correlation_keys {
            self.association(&k.id, &k.base)
        }
        self.extras(&c.extras)
    }
}
pub(crate) fn collaboration_read(d: &BpmnDocument) -> CollaborationRead<'_> {
    let mut out = CollaborationRead::default();
    for p in &d.partner_entities {
        out.association(&p.id, &p.base)
    }
    for p in &d.partner_roles {
        out.association(&p.id, &p.base)
    }
    for p in &d.endpoints {
        out.association(&p.id, &p.base)
    }
    if let Some(c) = &d.collaboration {
        out.extras(&c.extras);
        for n in &c.conversation_nodes {
            if let Some(call) = &n.call {
                for a in &call.participant_associations {
                    out.association(&a.id, &a.base)
                }
            }
        }
    }
    for c in &d.additional_collaborations {
        out.additional(c)
    }
    for c in &d.choreographies {
        out.extras(&c.extras);
        for n in &c.conversation_nodes {
            out.node(n)
        }
        for l in &c.conversation_links {
            out.owner(Some(&l.id), &l.base)
        }
        for k in &c.correlation_keys {
            out.association(&k.id, &k.base)
        }
    }
    out
}

impl BpmnDocument {
    /// Qualified names in collaboration content must retain their expanded namespace.
    pub fn has_collaboration_qname_refs(&self) -> bool {
        use crate::bpmn_activity::ModelReference;
        fn qualified(reference: &ModelReference) -> bool {
            matches!(reference, ModelReference::Qualified(_))
        }
        fn pair(a: &Option<ModelReference>, b: &Option<ModelReference>) -> bool {
            a.as_ref().is_some_and(qualified) || b.as_ref().is_some_and(qualified)
        }
        fn node(n: &ConversationNode) -> bool {
            n.participant_refs
                .iter()
                .chain(&n.message_flow_refs)
                .any(qualified)
                || n.correlation_keys.iter().any(key)
                || n.call.as_ref().is_some_and(|c| {
                    c.called_collaboration_ref.as_ref().is_some_and(qualified)
                        || c.participant_associations
                            .iter()
                            .any(|a| pair(&a.inner, &a.outer))
                })
        }
        fn participant(p: &crate::model::Participant) -> bool {
            p.process.as_ref().is_some_and(qualified)
                || p.interface_refs
                    .iter()
                    .chain(&p.endpoint_refs)
                    .any(qualified)
        }
        fn flow(f: &crate::model::Flow) -> bool {
            pair(&f.source, &f.target) || f.message_ref.as_ref().is_some_and(qualified)
        }
        fn link(l: &ConversationLink) -> bool {
            pair(&l.source, &l.target)
        }
        fn key(k: &crate::bpmn_activity::CorrelationKey) -> bool {
            k.property_refs.iter().any(qualified)
        }
        fn extras(e: &Option<CollaborationExtras>) -> bool {
            e.as_ref().is_some_and(|e| {
                e.choreography_refs.iter().any(qualified)
                    || e.conversation_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.participant_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.message_flow_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.artifacts.iter().any(|a| match a {
                        CollaborationArtifact::Association(f) => flow(f),
                        CollaborationArtifact::Group(group) => {
                            group.category_value_ref.as_ref().is_some_and(qualified)
                        }
                        CollaborationArtifact::TextAnnotation(_) => false,
                    })
            })
        }
        self.partner_entities
            .iter()
            .any(|p| p.participant_refs.iter().any(qualified))
            || self
                .partner_roles
                .iter()
                .any(|p| p.participant_refs.iter().any(qualified))
            || self
                .collaboration
                .iter()
                .chain(&self.additional_collaborations)
                .any(|c| {
                    c.participants.iter().any(participant)
                        || c.flows.iter().any(flow)
                        || c.conversation_nodes.iter().any(node)
                        || c.conversation_links.iter().any(link)
                        || c.correlation_keys.iter().any(key)
                        || extras(&c.extras)
                })
            || self.choreographies.iter().any(|c| {
                c.participants.iter().any(participant)
                    || c.message_flows.iter().chain(&c.sequence_flows).any(flow)
                    || c.conversation_nodes.iter().any(node)
                    || c.conversation_links.iter().any(link)
                    || c.correlation_keys.iter().any(key)
                    || extras(&c.extras)
            })
            || crate::queries::scopes(self).iter().any(|s| {
                s.flows.iter().any(flow)
                    || s.groups
                        .iter()
                        .any(|group| group.category_value_ref.as_ref().is_some_and(qualified))
            })
    }
    /// Native selectors in collaboration references require native identity support.
    pub fn has_collaboration_native_refs(&self) -> bool {
        use crate::bpmn_activity::ModelReference;
        fn qualified(reference: &ModelReference) -> bool {
            reference.is_native()
        }
        fn pair(a: &Option<ModelReference>, b: &Option<ModelReference>) -> bool {
            a.as_ref().is_some_and(qualified) || b.as_ref().is_some_and(qualified)
        }
        fn node(n: &ConversationNode) -> bool {
            n.participant_refs
                .iter()
                .chain(&n.message_flow_refs)
                .any(qualified)
                || n.correlation_keys.iter().any(key)
                || n.call.as_ref().is_some_and(|c| {
                    c.called_collaboration_ref.as_ref().is_some_and(qualified)
                        || c.participant_associations
                            .iter()
                            .any(|a| pair(&a.inner, &a.outer))
                })
        }
        fn participant(p: &crate::model::Participant) -> bool {
            p.process.as_ref().is_some_and(qualified)
                || p.interface_refs
                    .iter()
                    .chain(&p.endpoint_refs)
                    .any(qualified)
        }
        fn flow(f: &crate::model::Flow) -> bool {
            pair(&f.source, &f.target) || f.message_ref.as_ref().is_some_and(qualified)
        }
        fn link(l: &ConversationLink) -> bool {
            pair(&l.source, &l.target)
        }
        fn key(k: &crate::bpmn_activity::CorrelationKey) -> bool {
            k.property_refs.iter().any(qualified)
        }
        fn extras(e: &Option<CollaborationExtras>) -> bool {
            e.as_ref().is_some_and(|e| {
                e.choreography_refs.iter().any(qualified)
                    || e.conversation_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.participant_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.message_flow_associations
                        .iter()
                        .any(|a| pair(&a.inner, &a.outer))
                    || e.artifacts.iter().any(|a| match a {
                        CollaborationArtifact::Association(f) => flow(f),
                        CollaborationArtifact::Group(group) => {
                            group.category_value_ref.as_ref().is_some_and(qualified)
                        }
                        CollaborationArtifact::TextAnnotation(_) => false,
                    })
            })
        }
        self.partner_entities
            .iter()
            .any(|p| p.participant_refs.iter().any(qualified))
            || self
                .partner_roles
                .iter()
                .any(|p| p.participant_refs.iter().any(qualified))
            || self
                .collaboration
                .iter()
                .chain(&self.additional_collaborations)
                .any(|c| {
                    c.participants.iter().any(participant)
                        || c.flows.iter().any(flow)
                        || c.conversation_nodes.iter().any(node)
                        || c.conversation_links.iter().any(link)
                        || c.correlation_keys.iter().any(key)
                        || extras(&c.extras)
                })
            || self.choreographies.iter().any(|c| {
                c.participants.iter().any(participant)
                    || c.message_flows.iter().chain(&c.sequence_flows).any(flow)
                    || c.conversation_nodes.iter().any(node)
                    || c.conversation_links.iter().any(link)
                    || c.correlation_keys.iter().any(key)
                    || extras(&c.extras)
            })
            || crate::queries::scopes(self).iter().any(|s| {
                s.flows.iter().any(flow)
                    || s.groups
                        .iter()
                        .any(|group| group.category_value_ref.as_ref().is_some_and(qualified))
            })
    }
}

/// All qualified reference values introduced by the collaboration model.
pub(crate) fn collaboration_qnames(document: &BpmnDocument) -> Vec<&crate::bpmn_model::QNameRef> {
    use crate::bpmn_activity::ModelReference;
    fn one<'a>(out: &mut Vec<&'a crate::bpmn_model::QNameRef>, value: &'a ModelReference) {
        if let ModelReference::Qualified(value) = value {
            out.push(value);
        }
    }
    fn optional<'a>(
        out: &mut Vec<&'a crate::bpmn_model::QNameRef>,
        value: &'a Option<ModelReference>,
    ) {
        if let Some(value) = value {
            one(out, value)
        }
    }
    fn pair<'a>(
        out: &mut Vec<&'a crate::bpmn_model::QNameRef>,
        a: &'a Option<ModelReference>,
        b: &'a Option<ModelReference>,
    ) {
        optional(out, a);
        optional(out, b);
    }
    fn participant<'a>(
        out: &mut Vec<&'a crate::bpmn_model::QNameRef>,
        p: &'a crate::model::Participant,
    ) {
        optional(out, &p.process);
        for r in p.interface_refs.iter().chain(&p.endpoint_refs) {
            one(out, r)
        }
    }
    fn flow<'a>(out: &mut Vec<&'a crate::bpmn_model::QNameRef>, f: &'a crate::model::Flow) {
        pair(out, &f.source, &f.target);
        optional(out, &f.message_ref);
    }
    fn node<'a>(out: &mut Vec<&'a crate::bpmn_model::QNameRef>, n: &'a ConversationNode) {
        for r in n.participant_refs.iter().chain(&n.message_flow_refs) {
            one(out, r)
        }
        if let Some(call) = &n.call {
            optional(out, &call.called_collaboration_ref);
            for a in &call.participant_associations {
                pair(out, &a.inner, &a.outer)
            }
        }
    }
    fn extras<'a>(
        out: &mut Vec<&'a crate::bpmn_model::QNameRef>,
        value: &'a Option<CollaborationExtras>,
    ) {
        if let Some(value) = value {
            for r in &value.choreography_refs {
                one(out, r)
            }
            for a in &value.conversation_associations {
                pair(out, &a.inner, &a.outer)
            }
            for a in &value.participant_associations {
                pair(out, &a.inner, &a.outer)
            }
            for a in &value.message_flow_associations {
                pair(out, &a.inner, &a.outer)
            }
            for a in &value.artifacts {
                match a {
                    CollaborationArtifact::Association(f) => flow(out, f),
                    CollaborationArtifact::Group(group) => optional(out, &group.category_value_ref),
                    CollaborationArtifact::TextAnnotation(_) => {}
                }
            }
        }
    }
    let mut out = Vec::new();
    for p in &document.partner_entities {
        for r in &p.participant_refs {
            one(&mut out, r)
        }
    }
    for p in &document.partner_roles {
        for r in &p.participant_refs {
            one(&mut out, r)
        }
    }
    for c in document.collaborations() {
        for p in &c.participants {
            participant(&mut out, p)
        }
        for f in &c.flows {
            flow(&mut out, f)
        }
        for n in &c.conversation_nodes {
            node(&mut out, n)
        }
        for l in &c.conversation_links {
            pair(&mut out, &l.source, &l.target)
        }
        for key in &c.correlation_keys {
            for reference in &key.property_refs {
                one(&mut out, reference);
            }
        }
        for node in &c.conversation_nodes {
            for key in &node.correlation_keys {
                for reference in &key.property_refs {
                    one(&mut out, reference);
                }
            }
        }
        extras(&mut out, &c.extras)
    }
    for c in &document.choreographies {
        for p in &c.participants {
            participant(&mut out, p)
        }
        for f in c.message_flows.iter().chain(&c.sequence_flows) {
            flow(&mut out, f)
        }
        for n in &c.conversation_nodes {
            node(&mut out, n)
        }
        for l in &c.conversation_links {
            pair(&mut out, &l.source, &l.target)
        }
        for key in &c.correlation_keys {
            for reference in &key.property_refs {
                one(&mut out, reference);
            }
        }
        for node in &c.conversation_nodes {
            for key in &node.correlation_keys {
                for reference in &key.property_refs {
                    one(&mut out, reference);
                }
            }
        }
        extras(&mut out, &c.extras)
    }
    for scope in crate::queries::scopes(document) {
        for group in scope.groups {
            optional(&mut out, &group.category_value_ref);
        }
        for f in scope.flows {
            flow(&mut out, f)
        }
    }
    out
}
