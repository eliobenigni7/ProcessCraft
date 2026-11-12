//! Mutable traversal of modeled common metadata, preserving its owning identity.
use crate::{
    bpmn_metadata::{BaseExtras, BaseMetadata, DocumentationSet},
    ids::ElementIdentity,
    model::*,
};

fn metadata(
    owner: Option<&ElementIdentity>,
    value: &mut Option<BaseMetadata>,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    visit(owner, value);
}
fn state(
    value: &mut Option<crate::bpmn_data::ItemAwareMetadata>,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    if let Some(state) = value.as_mut().and_then(|value| value.data_state.as_mut()) {
        metadata(state.id.as_ref(), &mut state.base, visit);
    }
}
fn io(
    value: &mut crate::bpmn_data::IoSpecification,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    metadata(value.id.as_ref(), &mut value.base, visit);
    for port in value.inputs.iter_mut().chain(&mut value.outputs) {
        metadata(port.id.as_ref(), &mut port.base, visit);
        state(&mut port.item_aware, visit);
    }
    for set in value.input_sets.iter_mut().chain(&mut value.output_sets) {
        metadata(set.id.as_ref(), &mut set.base, visit);
    }
}
fn definition(
    value: &mut crate::bpmn_model::EventDefinition,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    let id = value.id().cloned();
    metadata(id.as_ref(), value.base_mut(), visit);
}
fn flow_metadata(
    value: &mut Option<crate::bpmn_activity::FlowElementMetadata>,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    if let Some(value) = value {
        for owner in [&mut value.auditing, &mut value.monitoring]
            .into_iter()
            .flatten()
        {
            metadata(owner.id.as_ref(), &mut owner.base, visit);
        }
    }
}
fn keys(
    values: &mut [crate::bpmn_activity::CorrelationKey],
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    for value in values {
        metadata(value.id.as_ref(), &mut value.base, visit);
    }
}
fn roles(
    values: &mut [crate::bpmn_activity::ResourceRole],
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    use crate::bpmn_activity::ResourceRoleAssignment;
    for value in values {
        metadata(value.id.as_ref(), &mut value.base, visit);
        match &mut value.assignment {
            ResourceRoleAssignment::Resource { bindings, .. } => {
                for binding in bindings {
                    metadata(binding.id.as_ref(), &mut binding.base, visit);
                }
            }
            ResourceRoleAssignment::Expression { assignment } => {
                metadata(assignment.id.as_ref(), &mut assignment.base, visit)
            }
            ResourceRoleAssignment::None => {}
        }
    }
}
fn callable(
    value: &mut crate::bpmn_activity::CallableMetadata,
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    for binding in &mut value.io_bindings {
        metadata(binding.id.as_ref(), &mut binding.base, visit);
    }
}
fn properties(
    values: &mut [crate::bpmn_model::EventProperty],
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    for value in values {
        metadata(value.id.as_ref(), &mut value.base, visit);
        state(&mut value.item_aware, visit);
    }
}
fn scope(
    nodes: &mut [Node],
    lanes: &mut [Lane],
    flows: &mut [Flow],
    groups: &mut [crate::bpmn_model::Group],
    objects: &mut [crate::bpmn_data::DataObject],
    associations: &mut [crate::bpmn_data::DataAssociation],
    visit: &mut impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    for lane in lanes {
        metadata(Some(&lane.id), &mut lane.base, visit);
        metadata(
            lane.child_lane_set_id.as_ref(),
            &mut lane.child_lane_set_base,
            visit,
        );
    }
    for group in groups {
        metadata(Some(&group.id), &mut group.base, visit);
    }
    for object in objects {
        flow_metadata(&mut object.flow_metadata, visit);
        metadata(object.id.as_ref(), &mut object.base, visit);
        state(&mut object.item_aware, visit);
    }
    for association in associations {
        metadata(association.id.as_ref(), &mut association.base, visit);
        for assignment in &mut association.assignments {
            metadata(assignment.id.as_ref(), &mut assignment.base, visit);
        }
    }
    for node in nodes {
        flow_metadata(&mut node.flow_metadata, visit);
        if let Some(activity) = &mut node.activity {
            properties(&mut activity.properties, visit);
            roles(&mut activity.resource_roles, visit);
            for value in &mut activity.renderings {
                metadata(value.id.as_ref(), &mut value.base, visit);
            }
        }
        state(&mut node.item_aware, visit);
        if let Some(spec) = &mut node.io_specification {
            io(spec, visit);
        }
        if let Some(characteristics) = &mut node.loop_characteristics {
            let id = characteristics.id().cloned();
            metadata(id.as_ref(), characteristics.base_mut(), visit);
            if let crate::bpmn_model::LoopCharacteristics::MultiInstance {
                input_data_item,
                output_data_item,
                complex_behavior_definitions,
                ..
            } = characteristics
            {
                for port in [input_data_item, output_data_item].into_iter().flatten() {
                    metadata(port.id.as_ref(), &mut port.base, visit);
                    state(&mut port.item_aware, visit);
                }
                for behavior in complex_behavior_definitions {
                    metadata(behavior.id.as_ref(), &mut behavior.base, visit);
                    if let Some(event) = &mut behavior.event {
                        metadata(event.id.as_ref(), &mut event.base, visit);
                        flow_metadata(&mut event.flow, visit);
                        properties(&mut event.properties, visit);
                        for port in &mut event.inputs {
                            metadata(port.id.as_ref(), &mut port.base, visit);
                            state(&mut port.item_aware, visit);
                        }
                        if let Some(set) = &mut event.input_set {
                            metadata(set.id.as_ref(), &mut set.base, visit);
                        }
                        for association in &mut event.input_associations {
                            metadata(association.id.as_ref(), &mut association.base, visit);
                            for assignment in &mut association.assignments {
                                metadata(assignment.id.as_ref(), &mut assignment.base, visit);
                            }
                        }
                        for value in &mut event.definitions {
                            definition(value, visit);
                        }
                    }
                }
            }
        }
        use crate::bpmn_model::{EventIo, NodeDetails};
        match &mut node.details {
            Some(NodeDetails::SubProcess { content }) => {
                metadata(
                    content.lane_set_id.as_ref(),
                    &mut content.lane_set_base,
                    visit,
                );
                scope(
                    &mut content.nodes,
                    &mut content.lanes,
                    &mut content.flows,
                    &mut content.groups,
                    &mut content.data_objects,
                    &mut content.data_associations,
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
                    metadata(property.id.as_ref(), &mut property.base, visit);
                    state(&mut property.item_aware, visit);
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
                        metadata(port.id.as_ref(), &mut port.base, visit);
                        state(&mut port.item_aware, visit);
                    }
                    if let Some(set) = set {
                        metadata(set.id.as_ref(), &mut set.base, visit);
                    }
                }
            }
            _ => {}
        }
    }
    for flow in flows {
        flow_metadata(&mut flow.flow_metadata, visit);
    }
}
pub fn visit_base_metadata_mut(
    d: &mut BpmnDocument,
    mut visit: impl FnMut(Option<&ElementIdentity>, &mut Option<BaseMetadata>),
) {
    use crate::bpmn_model::EventResource;
    for property in &mut d.correlation_properties {
        metadata(property.id.as_ref(), &mut property.base, &mut visit);
        for retrieval in &mut property.retrieval_expressions {
            metadata(retrieval.id.as_ref(), &mut retrieval.base, &mut visit);
        }
    }
    for task in &mut d.global_tasks {
        metadata(task.id.as_ref(), &mut task.base, &mut visit);
        callable(&mut task.callable, &mut visit);
        roles(&mut task.resource_roles, &mut visit);
        if let Some(spec) = &mut task.io_specification {
            io(spec, &mut visit);
        }
        if let crate::bpmn_activity::GlobalTaskKind::User { renderings, .. } = &mut task.kind {
            for value in renderings {
                metadata(value.id.as_ref(), &mut value.base, &mut visit);
            }
        }
    }
    for relationship in &mut d.relationships {
        metadata(relationship.id.as_ref(), &mut relationship.base, &mut visit);
    }
    for category in &mut d.categories {
        metadata(Some(&category.id), &mut category.base, &mut visit);
        for value in &mut category.values {
            metadata(Some(&value.id), &mut value.base, &mut visit);
        }
    }
    for resource in &mut d.event_resources {
        let id = match resource {
            EventResource::Signal { id, .. }
            | EventResource::Message { id, .. }
            | EventResource::Error { id, .. }
            | EventResource::Escalation { id, .. } => id.clone(),
        };
        metadata(Some(&id), resource.base_mut(), &mut visit);
    }
    for value in &mut d.root_event_definitions {
        definition(value, &mut visit);
    }
    for store in &mut d.data_stores {
        metadata(store.id.as_ref(), &mut store.base, &mut visit);
        state(&mut store.item_aware, &mut visit);
    }
    for item in &mut d.catalog.item_definitions {
        metadata(item.id.as_ref(), &mut item.base, &mut visit);
    }
    for interface in &mut d.catalog.interfaces {
        metadata(interface.id.as_ref(), &mut interface.base, &mut visit);
        for operation in &mut interface.operations {
            metadata(operation.id.as_ref(), &mut operation.base, &mut visit);
        }
    }
    for resource in &mut d.catalog.resources {
        metadata(resource.id.as_ref(), &mut resource.base, &mut visit);
        for parameter in &mut resource.parameters {
            metadata(parameter.id.as_ref(), &mut parameter.base, &mut visit);
        }
    }
    for process in &mut d.processes {
        if let Some(value) = &mut process.activity_metadata {
            callable(&mut value.callable, &mut visit);
            for owner in [&mut value.auditing, &mut value.monitoring]
                .into_iter()
                .flatten()
            {
                metadata(owner.id.as_ref(), &mut owner.base, &mut visit);
            }
            properties(&mut value.properties, &mut visit);
            roles(&mut value.resource_roles, &mut visit);
            for subscription in &mut value.correlation_subscriptions {
                metadata(subscription.id.as_ref(), &mut subscription.base, &mut visit);
                for binding in &mut subscription.bindings {
                    metadata(binding.id.as_ref(), &mut binding.base, &mut visit);
                }
            }
        }
        metadata(
            process.lane_set_id.as_ref(),
            &mut process.lane_set_base,
            &mut visit,
        );
        if let Some(spec) = &mut process.io_specification {
            io(spec, &mut visit);
        }
        scope(
            &mut process.nodes,
            &mut process.lanes,
            &mut process.flows,
            &mut process.groups,
            &mut process.data_objects,
            &mut process.data_associations,
            &mut visit,
        );
    }
    if let Some(c) = &mut d.collaboration {
        keys(&mut c.correlation_keys, &mut visit);
        for flow in &mut c.flows {
            flow_metadata(&mut flow.flow_metadata, &mut visit);
        }
        metadata(Some(&c.id), &mut c.base, &mut visit);
        for p in &mut c.participants {
            metadata(Some(&p.id), &mut p.base, &mut visit);
        }
        for n in &mut c.conversation_nodes {
            keys(&mut n.correlation_keys, &mut visit);
            metadata(Some(&n.id), &mut n.base, &mut visit);
        }
        for l in &mut c.conversation_links {
            metadata(Some(&l.id), &mut l.base, &mut visit);
        }
    }
    for c in &mut d.choreographies {
        metadata(Some(&c.id), &mut c.base, &mut visit);
        for p in &mut c.participants {
            metadata(Some(&p.id), &mut p.base, &mut visit);
        }
        for a in &mut c.activities {
            flow_metadata(&mut a.flow_metadata, &mut visit);
            keys(&mut a.correlation_keys, &mut visit);
            metadata(Some(&a.id), &mut a.base, &mut visit);
        }
        for flow in c.sequence_flows.iter_mut().chain(&mut c.message_flows) {
            flow_metadata(&mut flow.flow_metadata, &mut visit);
        }
    }
}
fn scope_legacy(
    nodes: &mut [Node],
    flows: &mut [Flow],
    visit: &mut impl FnMut(&ElementIdentity, &mut DocumentationSet, &mut Option<BaseExtras>),
) {
    for flow in flows {
        visit(&flow.id, &mut flow.documentation, &mut flow.base_extras);
    }
    for node in nodes {
        visit(&node.id, &mut node.documentation, &mut node.base_extras);
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            scope_legacy(&mut content.nodes, &mut content.flows, visit);
        }
    }
}
pub fn visit_legacy_metadata_mut(
    d: &mut BpmnDocument,
    mut visit: impl FnMut(&ElementIdentity, &mut DocumentationSet, &mut Option<BaseExtras>),
) {
    for process in &mut d.processes {
        visit(
            &process.id,
            &mut process.documentation,
            &mut process.base_extras,
        );
        scope_legacy(&mut process.nodes, &mut process.flows, &mut visit);
    }
    if let Some(c) = &mut d.collaboration {
        for flow in &mut c.flows {
            visit(&flow.id, &mut flow.documentation, &mut flow.base_extras);
        }
    }
    for c in &mut d.choreographies {
        for flow in c.sequence_flows.iter_mut().chain(&mut c.message_flows) {
            visit(&flow.id, &mut flow.documentation, &mut flow.base_extras);
        }
    }
}
fn expression_definition(
    value: &mut crate::bpmn_model::EventDefinition,
    visit: &mut impl FnMut(&mut crate::bpmn_model::Expression),
) {
    use crate::bpmn_model::EventDefinition;
    match value {
        EventDefinition::Conditional { expression, .. } => visit(expression),
        EventDefinition::Timer {
            expression: Some(expression),
            ..
        } => visit(expression),
        _ => {}
    }
}
fn role_expressions(
    roles: &mut [crate::bpmn_activity::ResourceRole],
    visit: &mut impl FnMut(&mut crate::bpmn_model::Expression),
) {
    use crate::bpmn_activity::ResourceRoleAssignment;
    for role in roles {
        match &mut role.assignment {
            ResourceRoleAssignment::Resource { bindings, .. } => {
                for binding in bindings {
                    if let Some(expression) = &mut binding.expression {
                        visit(expression);
                    }
                }
            }
            ResourceRoleAssignment::Expression { assignment } => {
                if let Some(expression) = &mut assignment.expression {
                    visit(expression);
                }
            }
            ResourceRoleAssignment::None => {}
        }
    }
}
fn expression_scope(
    nodes: &mut [Node],
    flows: &mut [Flow],
    associations: &mut [crate::bpmn_data::DataAssociation],
    visit: &mut impl FnMut(&mut crate::bpmn_model::Expression),
) {
    for flow in flows {
        if let Some(expression) = &mut flow.condition {
            visit(expression);
        }
    }
    for association in associations {
        if let Some(expression) = &mut association.transformation {
            visit(expression);
        }
        for assignment in &mut association.assignments {
            visit(&mut assignment.from);
            visit(&mut assignment.to);
        }
    }
    for node in nodes {
        if let Some(activity) = &mut node.activity {
            role_expressions(&mut activity.resource_roles, visit);
        }
        if let Some(activity) = &mut node.activity
            && let Some(expression) = &mut activity.completion_condition
        {
            visit(expression);
        }
        use crate::bpmn_model::{LoopCharacteristics, NodeDetails};
        if let Some(value) = &mut node.loop_characteristics {
            match value {
                LoopCharacteristics::Standard { condition, .. } => {
                    if let Some(expression) = condition {
                        visit(expression);
                    }
                }
                LoopCharacteristics::MultiInstance {
                    cardinality,
                    completion_condition,
                    complex_behavior_definitions,
                    ..
                } => {
                    for behavior in complex_behavior_definitions {
                        if let Some(expression) = &mut behavior.condition {
                            visit(expression);
                        }
                        if let Some(event) = &mut behavior.event {
                            for definition in &mut event.definitions {
                                expression_definition(definition, visit);
                            }
                            for association in &mut event.input_associations {
                                if let Some(expression) = &mut association.transformation {
                                    visit(expression);
                                }
                                for assignment in &mut association.assignments {
                                    visit(&mut assignment.from);
                                    visit(&mut assignment.to);
                                }
                            }
                        }
                    }
                    if let Some(expression) = cardinality {
                        visit(expression);
                    }
                    if let Some(expression) = completion_condition {
                        visit(expression);
                    }
                }
            }
        }
        match &mut node.details {
            Some(NodeDetails::Gateway {
                activation_condition: Some(expression),
                ..
            }) => visit(expression),
            Some(NodeDetails::Event { definitions, .. }) => {
                for definition in definitions {
                    expression_definition(definition, visit);
                }
            }
            Some(NodeDetails::SubProcess { content }) => expression_scope(
                &mut content.nodes,
                &mut content.flows,
                &mut content.data_associations,
                visit,
            ),
            _ => {}
        }
    }
}
pub fn visit_expressions_mut(
    d: &mut BpmnDocument,
    mut visit: impl FnMut(&mut crate::bpmn_model::Expression),
) {
    for property in &mut d.correlation_properties {
        for retrieval in &mut property.retrieval_expressions {
            if let Some(expression) = &mut retrieval.message_path {
                visit(expression);
            }
        }
    }
    for task in &mut d.global_tasks {
        role_expressions(&mut task.resource_roles, &mut visit);
    }
    for definition in &mut d.root_event_definitions {
        expression_definition(definition, &mut visit);
    }
    for process in &mut d.processes {
        if let Some(metadata) = &mut process.activity_metadata {
            role_expressions(&mut metadata.resource_roles, &mut visit);
            for subscription in &mut metadata.correlation_subscriptions {
                for binding in &mut subscription.bindings {
                    if let Some(expression) = &mut binding.data_path {
                        visit(expression);
                    }
                }
            }
        }
        expression_scope(
            &mut process.nodes,
            &mut process.flows,
            &mut process.data_associations,
            &mut visit,
        );
    }
    if let Some(c) = &mut d.collaboration {
        for flow in &mut c.flows {
            if let Some(expression) = &mut flow.condition {
                visit(expression);
            }
        }
    }
    for c in &mut d.choreographies {
        for flow in c.sequence_flows.iter_mut().chain(&mut c.message_flows) {
            if let Some(expression) = &mut flow.condition {
                visit(expression);
            }
        }
    }
}
