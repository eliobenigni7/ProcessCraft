//! Read-only traversal of modeled common metadata, preserving its owning identity.
use crate::{
    bpmn_metadata::{BaseExtras, BaseMetadata, DocumentationSet},
    ids::ElementIdentity,
    model::*,
};

fn metadata<'a>(
    owner: Option<&ElementIdentity>,
    value: &'a Option<BaseMetadata>,
    visit: &mut impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    visit(owner, value.as_ref());
}
fn state<'a>(
    value: &'a Option<crate::bpmn_data::ItemAwareMetadata>,
    visit: &mut impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    if let Some(state) = value.as_ref().and_then(|value| value.data_state.as_ref()) {
        metadata(state.id.as_ref(), &state.base, visit);
    }
}
fn io<'a>(
    value: &'a crate::bpmn_data::IoSpecification,
    visit: &mut impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    metadata(value.id.as_ref(), &value.base, visit);
    for port in value.inputs.iter().chain(&value.outputs) {
        metadata(port.id.as_ref(), &port.base, visit);
        state(&port.item_aware, visit);
    }
    for set in value.input_sets.iter().chain(&value.output_sets) {
        metadata(set.id.as_ref(), &set.base, visit);
    }
}
fn definition<'a>(
    value: &'a crate::bpmn_model::EventDefinition,
    visit: &mut impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    let id = value.id().cloned();
    visit(id.as_ref(), value.base());
}
fn scope<'a>(
    nodes: &'a [Node],
    lanes: &'a [Lane],
    flows: &'a [Flow],
    groups: &'a [crate::bpmn_model::Group],
    objects: &'a [crate::bpmn_data::DataObject],
    associations: &'a [crate::bpmn_data::DataAssociation],
    visit: &mut impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    for lane in lanes {
        metadata(Some(&lane.id), &lane.base, visit);
        metadata(
            lane.child_lane_set_id.as_ref(),
            &lane.child_lane_set_base,
            visit,
        );
    }
    for group in groups {
        metadata(Some(&group.id), &group.base, visit);
    }
    for object in objects {
        metadata(object.id.as_ref(), &object.base, visit);
        state(&object.item_aware, visit);
    }
    for association in associations {
        metadata(association.id.as_ref(), &association.base, visit);
        for assignment in &association.assignments {
            metadata(assignment.id.as_ref(), &assignment.base, visit);
        }
    }
    for node in nodes {
        state(&node.item_aware, visit);
        if let Some(spec) = &node.io_specification {
            io(spec, visit);
        }
        if let Some(characteristics) = &node.loop_characteristics {
            let id = characteristics.id().cloned();
            visit(id.as_ref(), characteristics.base());
        }
        use crate::bpmn_model::{EventIo, NodeDetails};
        match &node.details {
            Some(NodeDetails::SubProcess { content }) => {
                metadata(content.lane_set_id.as_ref(), &content.lane_set_base, visit);
                scope(
                    &content.nodes,
                    &content.lanes,
                    &content.flows,
                    &content.groups,
                    &content.data_objects,
                    &content.data_associations,
                    visit,
                )
            }
            Some(NodeDetails::Event {
                definitions,
                properties,
                io: event_io,
                ..
            }) => {
                for value in definitions {
                    definition(value, visit);
                }
                for property in properties {
                    metadata(property.id.as_ref(), &property.base, visit);
                    state(&property.item_aware, visit);
                }
                if let Some(event_io) = event_io {
                    let (ports, set) = match event_io {
                        EventIo::Catch {
                            outputs,
                            output_set,
                        } => (outputs, output_set),
                        EventIo::Throw { inputs, input_set } => (inputs, input_set),
                    };
                    for port in ports {
                        metadata(port.id.as_ref(), &port.base, visit);
                        state(&port.item_aware, visit);
                    }
                    if let Some(set) = set {
                        metadata(set.id.as_ref(), &set.base, visit);
                    }
                }
            }
            _ => {}
        }
    }
    let _ = flows;
}
pub(crate) fn visit_base_metadata<'a>(
    d: &'a BpmnDocument,
    mut visit: impl FnMut(Option<&ElementIdentity>, Option<&'a BaseMetadata>),
) {
    use crate::bpmn_model::EventResource;
    for relationship in &d.relationships {
        metadata(relationship.id.as_ref(), &relationship.base, &mut visit);
    }
    for category in &d.categories {
        metadata(Some(&category.id), &category.base, &mut visit);
        for value in &category.values {
            metadata(Some(&value.id), &value.base, &mut visit);
        }
    }
    for resource in &d.event_resources {
        let id = match resource {
            EventResource::Signal { id, .. }
            | EventResource::Message { id, .. }
            | EventResource::Error { id, .. }
            | EventResource::Escalation { id, .. } => id.clone(),
        };
        visit(Some(&id), resource.base());
    }
    for value in &d.root_event_definitions {
        definition(value, &mut visit);
    }
    for store in &d.data_stores {
        metadata(store.id.as_ref(), &store.base, &mut visit);
        state(&store.item_aware, &mut visit);
    }
    for item in &d.catalog.item_definitions {
        metadata(item.id.as_ref(), &item.base, &mut visit);
    }
    for interface in &d.catalog.interfaces {
        metadata(interface.id.as_ref(), &interface.base, &mut visit);
        for operation in &interface.operations {
            metadata(operation.id.as_ref(), &operation.base, &mut visit);
        }
    }
    for resource in &d.catalog.resources {
        metadata(resource.id.as_ref(), &resource.base, &mut visit);
        for parameter in &resource.parameters {
            metadata(parameter.id.as_ref(), &parameter.base, &mut visit);
        }
    }
    for process in &d.processes {
        metadata(
            process.lane_set_id.as_ref(),
            &process.lane_set_base,
            &mut visit,
        );
        if let Some(spec) = &process.io_specification {
            io(spec, &mut visit);
        }
        scope(
            &process.nodes,
            &process.lanes,
            &process.flows,
            &process.groups,
            &process.data_objects,
            &process.data_associations,
            &mut visit,
        );
    }
    if let Some(c) = &d.collaboration {
        metadata(Some(&c.id), &c.base, &mut visit);
        for p in &c.participants {
            metadata(Some(&p.id), &p.base, &mut visit);
        }
        for n in &c.conversation_nodes {
            metadata(Some(&n.id), &n.base, &mut visit);
        }
        for l in &c.conversation_links {
            metadata(Some(&l.id), &l.base, &mut visit);
        }
    }
    for c in &d.choreographies {
        metadata(Some(&c.id), &c.base, &mut visit);
        for p in &c.participants {
            metadata(Some(&p.id), &p.base, &mut visit);
        }
        for a in &c.activities {
            metadata(Some(&a.id), &a.base, &mut visit);
        }
    }
}
fn scope_legacy<'a>(
    nodes: &'a [Node],
    flows: &'a [Flow],
    visit: &mut impl FnMut(&ElementIdentity, &'a DocumentationSet, &'a Option<BaseExtras>),
) {
    for flow in flows {
        visit(&flow.id, &flow.documentation, &flow.base_extras);
    }
    for node in nodes {
        visit(&node.id, &node.documentation, &node.base_extras);
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
            scope_legacy(&content.nodes, &content.flows, visit);
        }
    }
}
pub(crate) fn visit_legacy_metadata<'a>(
    d: &'a BpmnDocument,
    mut visit: impl FnMut(&ElementIdentity, &'a DocumentationSet, &'a Option<BaseExtras>),
) {
    for process in &d.processes {
        visit(&process.id, &process.documentation, &process.base_extras);
        scope_legacy(&process.nodes, &process.flows, &mut visit);
    }
    if let Some(c) = &d.collaboration {
        for flow in &c.flows {
            visit(&flow.id, &flow.documentation, &flow.base_extras);
        }
    }
    for c in &d.choreographies {
        for flow in c.sequence_flows.iter().chain(&c.message_flows) {
            visit(&flow.id, &flow.documentation, &flow.base_extras);
        }
    }
}

impl BpmnDocument {
    /// Whether this document uses the expanded common metadata or DI representation.
    pub fn has_semantic_common_metadata_features(&self) -> bool {
        use crate::bpmn_metadata::DocumentationSet;
        use crate::bpmn_model::{Expression, ExpressionBody, NodeDetails};
        use crate::bpmn_xml::MixedXml;
        let mut found = !crate::activity_read::activity_read(self).bases.is_empty();
        found |= self.global_tasks.iter().any(|task| {
            matches!(
                &task.kind,
                crate::bpmn_activity::GlobalTaskKind::Script {
                    script: Some(MixedXml::Structured(_)),
                    ..
                }
            )
        });
        visit_base_metadata(self, |_, value| found |= value.is_some());
        visit_legacy_metadata(self, |_, docs, extras| {
            found |= matches!(docs, DocumentationSet::Structured(_)) || extras.is_some()
        });
        found |= !self.extensions.is_empty()
            || !self.relationships.is_empty()
            || self
                .definitions
                .as_ref()
                .is_some_and(|d| !d.namespaces.is_empty() || !d.foreign_attributes.is_empty());
        found |= self.expressions().iter().any(|expr| matches!(expr, Expression::Structured(data) if !data.foreign_attributes.is_empty() || matches!(data.text, ExpressionBody::Mixed { .. })));
        for scope in crate::queries::scopes(self) {
            for node in scope.nodes {
                found |= node
                    .loop_characteristics
                    .as_ref()
                    .is_some_and(|value| value.id().is_some());
                found |= node
                    .activity
                    .as_ref()
                    .and_then(|a| a.script.as_ref())
                    .is_some_and(|body| matches!(body, MixedXml::Structured(_)));
                found |= matches!(
                    &node.details,
                    Some(NodeDetails::TextAnnotation {
                        text: Some(MixedXml::Structured(_)),
                        ..
                    })
                );
            }
        }
        found
    }
    pub fn has_common_metadata_features(&self) -> bool {
        self.has_semantic_common_metadata_features()
            || self.diagrams.iter().any(|d| {
                d.documentation.is_some()
                    || d.resolution.is_some()
                    || !d.label_styles.is_empty()
                    || d.shapes.iter().any(|s| {
                        s.label_visibility.is_some()
                            || s.label.as_ref().is_some_and(|l| {
                                l.id.is_some() || l.bounds.is_none() || l.style_ref.is_some()
                            })
                    })
                    || d.edges.iter().any(|e| {
                        e.label_visibility.is_some()
                            || e.label.as_ref().is_some_and(|l| {
                                l.id.is_some() || l.bounds.is_none() || l.style_ref.is_some()
                            })
                    })
            })
    }
    /// Opaque XML may contain references whose meaning is unknown.
    pub fn has_opaque_xml(&self) -> bool {
        use crate::bpmn_metadata::DocumentationSet;
        use crate::bpmn_model::{Expression, ExpressionBody, ExpressionPart, NodeDetails};
        use crate::bpmn_xml::{MixedXml, XmlContent, XmlNode};
        fn opaque_content(content: &XmlContent) -> bool {
            content.nodes.iter().any(|node| {
                matches!(
                    node,
                    XmlNode::Element { .. } | XmlNode::ProcessingInstruction { .. }
                )
            })
        }
        fn opaque_body(body: &MixedXml) -> bool {
            matches!(body, MixedXml::Structured(content) if opaque_content(content))
        }
        let mut found = false;
        for base in crate::activity_read::activity_read(self).bases {
            found |= !base.foreign_attributes.is_empty()
                || base
                    .extension_elements
                    .as_ref()
                    .is_some_and(|e| opaque_content(&e.content))
                || base.documentation.iter().any(|d| opaque_body(&d.content));
        }
        found |= self.global_tasks.iter().any(|task| matches!(&task.kind, crate::bpmn_activity::GlobalTaskKind::Script { script: Some(body), .. } if opaque_body(body)));
        visit_base_metadata(self, |_, base| {
            if let Some(base) = base {
                found |= !base.foreign_attributes.is_empty()
                    || base
                        .extension_elements
                        .as_ref()
                        .is_some_and(|e| opaque_content(&e.content))
                    || base.documentation.iter().any(|d| opaque_body(&d.content));
            }
        });
        visit_legacy_metadata(self, |_, docs, extras| {
            found |= matches!(docs, DocumentationSet::Structured(docs) if docs.iter().any(|d| opaque_body(&d.content)));
            found |= extras.as_ref().is_some_and(|e| {
                !e.foreign_attributes.is_empty()
                    || e.extension_elements
                        .as_ref()
                        .is_some_and(|e| opaque_content(&e.content))
            });
        });
        found |= self
            .definitions
            .as_ref()
            .is_some_and(|d| !d.foreign_attributes.is_empty())
            || self
                .extensions
                .iter()
                .any(|e| e.documentation.iter().any(|d| opaque_body(&d.content)));
        found |= self.expressions().iter().any(|expr| match expr {
            Expression::Legacy(_) => false,
            Expression::Structured(data) => {
                !data.foreign_attributes.is_empty()
                    || match &data.text {
                        ExpressionBody::Text(_) => false,
                        ExpressionBody::Mixed { parts } => parts.iter().any(|part| match part {
                            ExpressionPart::Text { .. } | ExpressionPart::Comment { .. } => false,
                            ExpressionPart::ProcessingInstruction { .. } => true,
                            ExpressionPart::Documentation { documentation } => {
                                opaque_body(&documentation.content)
                            }
                            ExpressionPart::ExtensionElements { extension_elements } => {
                                opaque_content(&extension_elements.content)
                            }
                        }),
                    }
            }
        });
        for scope in crate::queries::scopes(self) {
            for node in scope.nodes {
                found |= node
                    .activity
                    .as_ref()
                    .and_then(|a| a.script.as_ref())
                    .is_some_and(opaque_body);
                found |= matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(body), .. }) if opaque_body(body));
            }
        }
        found
    }
}

/// Metadata attached to an addressable BPMN BaseElement.
#[derive(Clone, Debug)]
pub enum OwnerMetadata {
    Legacy {
        documentation: DocumentationSet,
        extras: Option<BaseExtras>,
    },
    Base(Option<BaseMetadata>),
}
impl BpmnDocument {
    pub fn metadata_for_selector(
        &self,
        selector: &crate::ids::ElementSelector,
    ) -> Option<OwnerMetadata> {
        let mut result = None;
        visit_base_metadata(self, |owner, base| {
            if owner.is_some_and(|id| id == selector) {
                result = Some(OwnerMetadata::Base(base.cloned()));
            }
        });
        visit_legacy_metadata(self, |owner, documentation, extras| {
            if owner == selector {
                result = Some(OwnerMetadata::Legacy {
                    documentation: documentation.clone(),
                    extras: extras.clone(),
                });
            }
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bpmn_metadata::Documentation, bpmn_xml::MixedXml, model::DefinitionsExtension};

    #[test]
    fn extension_document_identity_and_capability_are_visible() {
        let mut doc = BpmnDocument::new("model".into(), "definitions".parse().unwrap());
        assert!(!doc.has_common_metadata_features());
        doc.extensions.push(DefinitionsExtension {
            documentation: vec![Documentation {
                id: Some("doc_id".parse().unwrap()),
                text_format: None,
                content: MixedXml::Text("note".into()),
            }],
            definition: None,
            must_understand: Some(false),
        });
        assert!(doc.has_common_metadata_features());
        assert!(!doc.has_opaque_xml());
        assert!(
            doc.element_identities()
                .iter()
                .any(|id| id.xml_id().is_some_and(|id| id.to_string() == "doc_id"))
        );
    }
    #[test]
    fn opaque_detection_distinguishes_text_comments_from_markup() {
        use crate::bpmn_xml::{XmlContent, XmlElement, XmlNode};
        let mut doc = BpmnDocument::new("model".into(), "definitions".parse().unwrap());
        doc.extensions.push(DefinitionsExtension {
            documentation: vec![Documentation {
                id: None,
                text_format: None,
                content: MixedXml::Structured(XmlContent {
                    namespaces: vec![],
                    nodes: vec![
                        XmlNode::Text {
                            text: "note".into(),
                        },
                        XmlNode::Comment {
                            text: "comment".into(),
                        },
                    ],
                }),
            }],
            definition: None,
            must_understand: None,
        });
        assert!(!doc.has_opaque_xml());
        let MixedXml::Structured(content) = &mut doc.extensions[0].documentation[0].content else {
            unreachable!()
        };
        content.nodes.push(XmlNode::ProcessingInstruction {
            target: "instruction".into(),
            data: None,
        });
        assert!(doc.has_opaque_xml());
        let MixedXml::Structured(content) = &mut doc.extensions[0].documentation[0].content else {
            unreachable!()
        };
        content.nodes.pop();
        content.nodes.push(XmlNode::Element {
            element: XmlElement {
                namespace: String::new(),
                local: "element".parse().unwrap(),
                prefix: None,
                namespaces: vec![],
                attributes: vec![],
                children: vec![],
            },
        });
        assert!(doc.has_opaque_xml());
    }
    #[test]
    fn activity_metadata_clear_matches_immutable_scan() {
        use crate::bpmn_activity::*;
        let mut doc = BpmnDocument::new("model".into(), "definitions".parse().unwrap());
        let base = || Some(BaseMetadata::default());
        doc.global_tasks.push(GlobalTask {
            id: Some("global".parse().unwrap()),
            base: base(),
            name: None,
            callable: CallableMetadata::default(),
            io_specification: None,
            resource_roles: vec![ResourceRole {
                id: None,
                base: base(),
                name: None,
                kind: ResourceRoleKind::ResourceRole,
                assignment: ResourceRoleAssignment::Expression {
                    assignment: ResourceAssignmentExpression {
                        id: None,
                        base: base(),
                        expression: None,
                    },
                },
            }],
            kind: GlobalTaskKind::Task,
        });
        doc.correlation_properties.push(CorrelationProperty {
            id: None,
            base: base(),
            name: None,
            item_type: None,
            retrieval_expressions: vec![CorrelationRetrieval {
                id: None,
                base: base(),
                message_ref: None,
                message_path: None,
            }],
        });
        assert!(doc.has_semantic_common_metadata_features());
        let mut removed = 0;
        crate::metadata_visit::visit_base_metadata_mut(&mut doc, |_, base| {
            if base.take().is_some() {
                removed += 1;
            }
        });
        assert_eq!(removed, 5);
        assert!(!doc.has_semantic_common_metadata_features());
    }
}
