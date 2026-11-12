//! Shared read-only traversal for extended activity and callable owners.
use crate::{
    bpmn_activity::*,
    bpmn_data::{DataPort, IoSpecification},
    bpmn_metadata::BaseMetadata,
    bpmn_model::{EventDefinition, Expression, LoopCharacteristics},
    ids::ElementIdentity,
    model::BpmnDocument,
};
#[derive(Default)]
pub(crate) struct ActivityRead<'a> {
    pub ids: Vec<&'a ElementIdentity>,
    pub expressions: Vec<&'a Expression>,
    pub formal_expressions: Vec<&'a Expression>,
    pub bases: Vec<&'a BaseMetadata>,
    pub qnames: Vec<&'a crate::bpmn_model::QNameRef>,
    pub native_references: bool,
    pub qualified_references: bool,
    pub anonymous_owners: usize,
}
impl<'a> ActivityRead<'a> {
    fn reference(&mut self, r: &'a Option<ModelReference>) {
        self.native_references |= r.as_ref().is_some_and(ModelReference::is_native);
        self.qualified_references |= matches!(r, Some(ModelReference::Qualified(_)));
        if let Some(ModelReference::Qualified(qname)) = r {
            self.qnames.push(qname);
        }
    }
    fn refs(&mut self, refs: &'a [ModelReference]) {
        self.native_references |= refs.iter().any(ModelReference::is_native);
        self.qualified_references |= refs
            .iter()
            .any(|r| matches!(r, ModelReference::Qualified(_)));
        for r in refs {
            if let ModelReference::Qualified(qname) = r {
                self.qnames.push(qname);
            }
        }
    }
    fn owner(&mut self, id: &'a Option<ElementIdentity>, base: &'a Option<BaseMetadata>) {
        self.ids.extend(id);
        if id.is_none() {
            self.anonymous_owners = self.anonymous_owners.saturating_add(1);
        }
        self.bases.extend(base);
    }
    fn property(&mut self, p: &'a crate::bpmn_model::EventProperty) {
        self.owner(&p.id, &p.base);
        self.qualified_references |= p
            .item_aware
            .as_ref()
            .is_some_and(|m| m.item_subject_ref.is_some());
        if let Some(qname) = p
            .item_aware
            .as_ref()
            .and_then(|m| m.item_subject_ref.as_ref())
        {
            self.qnames.push(qname);
        }
        if let Some(state) = p.item_aware.as_ref().and_then(|v| v.data_state.as_ref()) {
            self.owner(&state.id, &state.base);
        }
    }
    fn port(&mut self, p: &'a DataPort) {
        self.owner(&p.id, &p.base);
        self.qualified_references |= p
            .item_aware
            .as_ref()
            .is_some_and(|m| m.item_subject_ref.is_some());
        if let Some(qname) = p
            .item_aware
            .as_ref()
            .and_then(|m| m.item_subject_ref.as_ref())
        {
            self.qnames.push(qname);
        }
        if let Some(state) = p.item_aware.as_ref().and_then(|v| v.data_state.as_ref()) {
            self.owner(&state.id, &state.base);
        }
    }
    fn io(&mut self, io: &'a IoSpecification) {
        self.owner(&io.id, &io.base);
        for p in io.inputs.iter().chain(&io.outputs) {
            self.port(p);
        }
        for set in io.input_sets.iter().chain(&io.output_sets) {
            self.owner(&set.id, &set.base);
            self.native_references |= set
                .data_refs
                .iter()
                .chain(&set.optional_refs)
                .chain(&set.while_executing_refs)
                .chain(&set.counterpart_refs)
                .any(|r| r.is_native());
        }
    }
    fn flow(&mut self, v: &'a Option<FlowElementMetadata>) {
        if let Some(v) = v {
            self.refs(&v.category_value_refs);
            self.refs(&v.incoming);
            self.refs(&v.outgoing);
            for x in [&v.auditing, &v.monitoring].into_iter().flatten() {
                self.owner(&x.id, &x.base);
            }
        }
    }
    fn role(&mut self, r: &'a ResourceRole) {
        self.owner(&r.id, &r.base);
        match &r.assignment {
            ResourceRoleAssignment::Resource {
                reference,
                bindings,
            } => {
                self.native_references |= reference.is_native();
                self.qualified_references |= matches!(reference, ModelReference::Qualified(_));
                if let ModelReference::Qualified(qname) = reference {
                    self.qnames.push(qname);
                }
                for b in bindings {
                    self.reference(&b.parameter_ref);
                    self.owner(&b.id, &b.base);
                    self.expressions.extend(&b.expression);
                }
            }
            ResourceRoleAssignment::Expression { assignment } => {
                self.owner(&assignment.id, &assignment.base);
                self.expressions.extend(&assignment.expression);
            }
            ResourceRoleAssignment::None => {}
        }
    }
    fn key(&mut self, k: &'a CorrelationKey) {
        self.owner(&k.id, &k.base);
        self.refs(&k.property_refs);
    }
    fn callable(&mut self, c: &'a CallableMetadata) {
        self.refs(&c.supported_interface_refs);
        for b in &c.io_bindings {
            self.reference(&b.operation_ref);
            self.native_references |= b.input_data_ref.as_ref().is_some_and(|r| r.is_native())
                || b.output_data_ref.as_ref().is_some_and(|r| r.is_native());
            self.owner(&b.id, &b.base);
        }
    }
    fn event_definition(&mut self, e: &'a EventDefinition) {
        self.ids.extend(e.id());
        self.bases.extend(e.base());
        self.native_references |= e.has_native_references();
        self.qualified_references |= e.has_qualified_references();
        match e {
            EventDefinition::Message {
                message_ref,
                operation_ref,
                ..
            } => {
                self.reference(message_ref);
                self.reference(operation_ref);
            }
            EventDefinition::Signal { signal_ref, .. } => self.reference(signal_ref),
            EventDefinition::Error { error_ref, .. } => self.reference(error_ref),
            EventDefinition::Escalation { escalation_ref, .. } => self.reference(escalation_ref),
            EventDefinition::Link {
                sources, target, ..
            } => {
                self.refs(sources);
                self.reference(target);
            }
            EventDefinition::Compensate { activity_ref, .. } => self.reference(activity_ref),
            _ => {}
        }
        match e {
            EventDefinition::Conditional { expression, .. } => self.expressions.push(expression),
            EventDefinition::Timer { expression, .. } => self.expressions.extend(expression),
            _ => {}
        }
    }
    fn event(&mut self, e: &'a ImplicitThrowEvent) {
        self.owner(&e.id, &e.base);
        self.refs(&e.definition_refs);
        self.refs(&e.incoming);
        self.refs(&e.outgoing);
        self.flow(&e.flow);
        for p in &e.properties {
            self.property(p)
        }
        for p in &e.inputs {
            self.port(p)
        }
        if let Some(set) = &e.input_set {
            self.owner(&set.id, &set.base);
            self.native_references |= set
                .data_refs
                .iter()
                .chain(&set.optional_refs)
                .chain(&set.while_executing_refs)
                .chain(&set.counterpart_refs)
                .any(|r| r.is_native());
        }
        for a in &e.input_associations {
            self.native_references |= a.sources.iter().any(|r| r.is_native())
                || a.target.as_ref().is_some_and(|r| r.is_native());
            self.owner(&a.id, &a.base);
            self.expressions.extend(&a.transformation);
            self.formal_expressions.extend(&a.transformation);
            for x in &a.assignments {
                self.owner(&x.id, &x.base);
                self.expressions.push(&x.from);
                self.expressions.push(&x.to);
            }
        }
        for d in &e.definitions {
            self.event_definition(d)
        }
    }
}
/// Newly introduced activity owners for one node; does not recurse into a subprocess.
pub(crate) fn node_activity_read(n: &crate::model::Node) -> ActivityRead<'_> {
    let mut v = ActivityRead::default();
    add_node(&mut v, n);
    v
}
fn add_node<'a>(v: &mut ActivityRead<'a>, n: &'a crate::model::Node) {
    v.flow(&n.flow_metadata);
    match &n.details {
        Some(crate::bpmn_model::NodeDetails::BoundaryEvent { attached_to, .. }) => {
            v.native_references |= attached_to.is_native();
            if let ModelReference::Qualified(qname) = attached_to {
                v.qualified_references = true;
                v.qnames.push(qname);
            }
        }
        Some(crate::bpmn_model::NodeDetails::Event { attached_to, .. }) => v.reference(attached_to),
        _ => {}
    }
    if let Some(a) = &n.activity {
        for p in &a.properties {
            v.property(p)
        }
        for r in &a.resource_roles {
            v.role(r)
        }
        for r in &a.renderings {
            v.owner(&r.id, &r.base)
        }
    }
    if let Some(LoopCharacteristics::MultiInstance {
        input_data_item,
        output_data_item,
        loop_data_input_ref,
        loop_data_output_ref,
        one_behavior_event_ref,
        none_behavior_event_ref,
        complex_behavior_definitions,
        ..
    }) = &n.loop_characteristics
    {
        for reference in [
            loop_data_input_ref,
            loop_data_output_ref,
            one_behavior_event_ref,
            none_behavior_event_ref,
        ] {
            v.reference(reference);
        }
        for p in [input_data_item, output_data_item].into_iter().flatten() {
            v.port(p)
        }
        for b in complex_behavior_definitions {
            v.owner(&b.id, &b.base);
            v.expressions.extend(&b.condition);
            v.formal_expressions.extend(&b.condition);
            if let Some(e) = &b.event {
                v.event(e)
            }
        }
    }
}
pub(crate) fn activity_read(d: &BpmnDocument) -> ActivityRead<'_> {
    let mut v = ActivityRead::default();
    for p in &d.correlation_properties {
        v.owner(&p.id, &p.base);
        v.qualified_references |= p.item_type.is_some();
        v.qnames.extend(&p.item_type);
        for r in &p.retrieval_expressions {
            v.owner(&r.id, &r.base);
            v.reference(&r.message_ref);
            v.expressions.extend(&r.message_path);
            v.formal_expressions.extend(&r.message_path);
        }
    }
    for task in &d.global_tasks {
        v.owner(&task.id, &task.base);
        v.callable(&task.callable);
        if let Some(io) = &task.io_specification {
            v.io(io)
        }
        for r in &task.resource_roles {
            v.role(r)
        }
        if let GlobalTaskKind::User { renderings, .. } = &task.kind {
            for r in renderings {
                v.owner(&r.id, &r.base)
            }
        }
    }
    for p in &d.processes {
        if let Some(m) = &p.activity_metadata {
            v.callable(&m.callable);
            v.reference(&m.definitional_collaboration_ref);
            v.refs(&m.supports);
            for a in [&m.auditing, &m.monitoring].into_iter().flatten() {
                v.owner(&a.id, &a.base)
            }
            for p in &m.properties {
                v.property(p)
            }
            for r in &m.resource_roles {
                v.role(r)
            }
            for sub in &m.correlation_subscriptions {
                v.owner(&sub.id, &sub.base);
                v.reference(&sub.key_ref);
                for b in &sub.bindings {
                    v.owner(&b.id, &b.base);
                    v.reference(&b.property_ref);
                    v.expressions.extend(&b.data_path);
                    v.formal_expressions.extend(&b.data_path)
                }
            }
        }
    }
    for scope in crate::queries::scopes(d) {
        for f in scope.flows {
            v.flow(&f.flow_metadata)
        }
        for object in scope.data_objects {
            v.flow(&object.flow_metadata)
        }
        for n in scope.nodes {
            add_node(&mut v, n);
        }
    }

    if let Some(c) = &d.collaboration {
        for flow in &c.flows {
            v.flow(&flow.flow_metadata);
        }
        for k in &c.correlation_keys {
            v.key(k)
        }
        for n in &c.conversation_nodes {
            for k in &n.correlation_keys {
                v.key(k)
            }
        }
    }
    for c in &d.choreographies {
        for f in c.message_flows.iter().chain(&c.sequence_flows) {
            v.flow(&f.flow_metadata)
        }
        for a in &c.activities {
            v.flow(&a.flow_metadata);
            for k in &a.correlation_keys {
                v.key(k)
            }
        }
    }
    for scope in crate::queries::choreography_scopes(d) {
        if scope.parent.is_none()
            && let Some(crate::bpmn_collaboration::ChoreographyRootKind::Global {
                initiating_participant_ref,
            }) = &scope.root.kind
        {
            v.reference(initiating_participant_ref);
        }
        for a in scope.activities {
            v.refs(&a.participant_refs);
            v.reference(&a.initiating_participant);
            v.refs(&a.message_flow_refs);
            if scope.root.graph.is_some() {
                v.flow(&a.flow_metadata);
                for k in &a.correlation_keys {
                    v.key(k);
                }
            }
            if let Some(crate::bpmn_collaboration::ChoreographyActivityDetails::Call {
                called_choreography_ref,
                participant_associations,
            }) = &a.details
            {
                v.reference(called_choreography_ref);
                for association in participant_associations {
                    v.owner(&association.id, &association.base);
                    v.reference(&association.inner);
                    v.reference(&association.outer);
                }
            }
        }
        if scope.root.graph.is_some() {
            for n in scope.events.into_iter().chain(scope.gateways) {
                add_node(&mut v, n);
                if let Some(crate::bpmn_model::NodeDetails::Event {
                    definitions,
                    definition_refs,
                    properties,
                    io,
                    ..
                }) = &n.details
                {
                    v.refs(definition_refs);
                    for property in properties {
                        if let Some(qname) = property
                            .item_aware
                            .as_ref()
                            .and_then(|m| m.item_subject_ref.as_ref())
                        {
                            v.qualified_references = true;
                            v.qnames.push(qname);
                        }
                    }
                    if let Some(io) = io {
                        use crate::bpmn_model::EventIo;
                        let ports = match io {
                            EventIo::Catch { outputs, .. } => outputs,
                            EventIo::Throw { inputs, .. } => inputs,
                        };
                        for port in ports {
                            if let Some(qname) = port
                                .item_aware
                                .as_ref()
                                .and_then(|m| m.item_subject_ref.as_ref())
                            {
                                v.qualified_references = true;
                                v.qnames.push(qname);
                            }
                        }
                    }
                    for definition in definitions {
                        match definition {
                            EventDefinition::Message {
                                message_ref,
                                operation_ref,
                                ..
                            } => {
                                v.reference(message_ref);
                                v.reference(operation_ref);
                            }
                            EventDefinition::Signal { signal_ref, .. } => v.reference(signal_ref),
                            EventDefinition::Error { error_ref, .. } => v.reference(error_ref),
                            EventDefinition::Escalation { escalation_ref, .. } => {
                                v.reference(escalation_ref)
                            }
                            EventDefinition::Link {
                                sources, target, ..
                            } => {
                                v.refs(sources);
                                v.reference(target);
                            }
                            EventDefinition::Compensate { activity_ref, .. } => {
                                v.reference(activity_ref)
                            }
                            _ => {}
                        }
                    }
                }
            }
            for f in scope.flows {
                v.flow(&f.flow_metadata);
            }
        }
    }
    v
}

impl BpmnDocument {
    /// Qualified activity references must not be silently retargeted.
    pub fn has_activity_qname_refs(&self) -> bool {
        let read = activity_read(self);
        read.qualified_references || read.expressions.iter().any(|e| matches!(e, Expression::Structured(data) if matches!(&data.kind, crate::bpmn_model::ExpressionKind::Formal { evaluates_to_type_ref: Some(_), .. })))
    }
    /// New representation requiring an activity-aware native reader.
    pub fn has_expanded_activities(&self) -> bool {
        use crate::bpmn_model::NodeDetails;
        if !self.global_tasks.is_empty()
            || !self.correlation_properties.is_empty()
            || self.processes.iter().any(|p| p.activity_metadata.is_some())
            || self.collaboration.as_ref().is_some_and(|c| {
                !c.correlation_keys.is_empty()
                    || c.flows.iter().any(|flow| flow.flow_metadata.is_some())
                    || c.conversation_nodes
                        .iter()
                        .any(|n| !n.correlation_keys.is_empty())
            })
            || self.choreographies.iter().any(|c| {
                c.activities
                    .iter()
                    .any(|a| a.flow_metadata.is_some() || !a.correlation_keys.is_empty())
                    || c.message_flows
                        .iter()
                        .chain(&c.sequence_flows)
                        .any(|f| f.flow_metadata.is_some())
            })
        {
            return true;
        }
        crate::queries::scopes(self).iter().any(|scope| {
            scope.flows.iter().any(|f| f.flow_metadata.is_some())
                || scope.data_objects.iter().any(|o| o.flow_metadata.is_some())
                || scope.nodes.iter().any(|n| {
                    n.flow_metadata.is_some()
                        || n.activity.as_ref().is_some_and(|a| {
                            !a.properties.is_empty()
                                || !a.resource_roles.is_empty()
                                || !a.renderings.is_empty()
                        })
                        || matches!(
                            &n.details,
                            Some(NodeDetails::CallActivity {
                                called_element: None
                            })
                        )
                        || match &n.loop_characteristics {
                            Some(LoopCharacteristics::Standard { test_before, .. }) => {
                                test_before.is_none()
                            }
                            Some(LoopCharacteristics::MultiInstance {
                                is_sequential,
                                behavior,
                                loop_data_input_ref,
                                loop_data_output_ref,
                                input_data_item,
                                output_data_item,
                                complex_behavior_definitions,
                                one_behavior_event_ref,
                                none_behavior_event_ref,
                                ..
                            }) => {
                                is_sequential.is_none()
                                    || behavior.is_some()
                                    || loop_data_input_ref.is_some()
                                    || loop_data_output_ref.is_some()
                                    || input_data_item.is_some()
                                    || output_data_item.is_some()
                                    || !complex_behavior_definitions.is_empty()
                                    || one_behavior_event_ref.is_some()
                                    || none_behavior_event_ref.is_some()
                            }
                            None => false,
                        }
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bpmn_model::ExpressionKind;

    #[test]
    fn correlation_formal_expression_and_owner_are_traversed() {
        let mut document = BpmnDocument::new("process".into(), "definitions".parse().unwrap());
        let formal = Expression::Structured(crate::bpmn_model::ExpressionData {
            id: None,
            kind: ExpressionKind::Formal {
                explicit_type: false,
                language: None,
                evaluates_to_type_ref: None,
            },
            text: "x".into(),
            namespaces: vec![],
            foreign_attributes: vec![],
        });
        document.correlation_properties.push(CorrelationProperty {
            id: Some("corr".parse().unwrap()),
            base: None,
            name: None,
            item_type: None,
            retrieval_expressions: vec![CorrelationRetrieval {
                id: Some("retrieval".parse().unwrap()),
                base: None,
                message_ref: None,
                message_path: Some(formal),
            }],
        });
        let read = activity_read(&document);
        assert_eq!(read.ids.len(), 2);
        assert_eq!(read.expressions.len(), 1);
        assert_eq!(read.formal_expressions.len(), 1);
        assert!(document.has_expanded_activities());
        assert!(
            document
                .element_identities()
                .iter()
                .any(|id| id.xml_id().is_some_and(|id| id.to_string() == "retrieval"))
        );
    }
    #[test]
    fn legacy_loop_boolean_wire_format_remains_boolean() {
        let loop_data = LoopCharacteristics::Standard {
            id: None,
            base: None,
            test_before: Some(false),
            condition: None,
            maximum: None,
        };
        let wire = serde_json::to_value(loop_data).unwrap();
        assert_eq!(wire["test_before"], false);
    }
    #[test]
    fn anonymous_activity_owners_count_towards_document_limit() {
        let mut document = BpmnDocument::new("process".into(), "definitions".parse().unwrap());
        let property = CorrelationProperty {
            id: None,
            base: None,
            name: None,
            item_type: None,
            retrieval_expressions: vec![],
        };
        document.correlation_properties = vec![property; 10_000];
        let mut project = crate::model::ProjectData::new("project".into());
        project.documents.push(std::sync::Arc::new(document));
        assert!(
            project
                .validate()
                .unwrap_err()
                .contains("document element limit")
        );
    }
}
