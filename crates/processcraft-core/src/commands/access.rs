//! Mutable lookups into documents, scopes, nodes, flows and nested containers shared by the mutation modules.
use super::*;
pub(super) fn nested_scope_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
    depth: usize,
) -> Option<&'a mut FlowContainer> {
    if depth >= 32 {
        return None;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if &node.id == id {
                return Some(content);
            }
            if let Some(found) = nested_scope_mut(&mut content.nodes, id, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}
pub(crate) type ScopePartsMut<'a> = (&'a mut Vec<Node>, &'a mut Vec<Lane>, &'a mut Vec<Flow>);
pub(crate) fn scope_parts_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<ScopePartsMut<'a>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        let p = &mut document.processes[index];
        return Ok((&mut p.nodes, &mut p.lanes, &mut p.flows));
    }
    for process in &mut document.processes {
        if let Some(c) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok((&mut c.nodes, &mut c.lanes, &mut c.flows));
        }
    }
    Err(invalid("process or subprocess not found"))
}
pub(super) fn nested_node_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
    depth: usize,
) -> Option<&'a mut Node> {
    if depth > 32 {
        return None;
    }
    for node in nodes {
        if &node.id == id {
            return Some(node);
        }
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details
            && let Some(found) = nested_node_mut(&mut content.nodes, id, depth + 1)
        {
            return Some(found);
        }
    }
    None
}
pub(crate) fn node_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Node, CommandError> {
    for process in &mut document.processes {
        if let Some(node) = nested_node_mut(&mut process.nodes, id, 0) {
            return Ok(node);
        }
    }
    use crate::bpmn_collaboration::CollaborationArtifact;
    for extras in document
        .collaboration
        .iter_mut()
        .chain(&mut document.additional_collaborations)
        .filter_map(|root| root.extras.as_mut())
        .chain(
            document
                .choreographies
                .iter_mut()
                .filter_map(|root| root.extras.as_mut()),
        )
    {
        for artifact in &mut extras.artifacts {
            if let CollaborationArtifact::TextAnnotation(node) = artifact
                && &node.id == id
            {
                return Ok(node);
            }
        }
    }
    Err(invalid("node not found"))
}
pub(super) fn edit_nested(
    nodes: &mut [Node],
    id: &ElementSelector,
    text: Option<&str>,
    documentation: bool,
    depth: usize,
) -> bool {
    if depth >= 32 {
        return false;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            for child in &mut content.nodes {
                if &child.id == id {
                    if documentation {
                        child.documentation = text.unwrap_or_default().into();
                    } else {
                        child.name = text.map(str::to_owned);
                    }
                    return true;
                }
            }
            if !documentation {
                for lane in &mut content.lanes {
                    if &lane.id == id {
                        lane.name = text.map(str::to_owned);
                        return true;
                    }
                }
            }
            for flow in &mut content.flows {
                if &flow.id == id {
                    if documentation {
                        flow.documentation = text.unwrap_or_default().into();
                    } else {
                        flow.name = text.map(str::to_owned);
                    }
                    return true;
                }
            }
            if edit_nested(&mut content.nodes, id, text, documentation, depth + 1) {
                return true;
            }
        }
    }
    false
}
pub(super) fn nested_flow_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
    depth: usize,
) -> Option<&'a mut Flow> {
    if depth >= 32 {
        return None;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if let Some(flow) = content.flows.iter_mut().find(|f| &f.id == id) {
                return Some(flow);
            }
            if let Some(flow) = nested_flow_mut(&mut content.nodes, id, depth + 1) {
                return Some(flow);
            }
        }
    }
    None
}
pub(super) fn collect_association_ids(
    association: &crate::bpmn_data::DataAssociation,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    collect_base_documentation_ids(association.base.as_ref(), ids);
    if let Some(id) = &association.id {
        ids.insert(id.selector());
    }
    if let Some(value) = &association.transformation {
        collect_expression_ids(value, ids);
    }
    for assignment in &association.assignments {
        collect_base_documentation_ids(assignment.base.as_ref(), ids);
        if let Some(id) = &assignment.id {
            ids.insert(id.selector());
        }
        collect_expression_ids(&assignment.from, ids);
        collect_expression_ids(&assignment.to, ids);
    }
}
pub(super) fn collect_expression_ids(
    expression: &crate::bpmn_model::Expression,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    if let crate::bpmn_model::Expression::Structured(data) = expression {
        if let Some(id) = &data.id {
            ids.insert(id.selector());
        }
        if let crate::bpmn_model::ExpressionBody::Mixed { parts } = &data.text {
            for part in parts {
                if let crate::bpmn_model::ExpressionPart::Documentation { documentation } = part
                    && let Some(id) = &documentation.id
                {
                    ids.insert(id.selector());
                }
            }
        }
    }
}
pub(super) fn collect_base_documentation_ids(
    base: Option<&crate::bpmn_metadata::BaseMetadata>,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    if let Some(base) = base {
        for documentation in &base.documentation {
            if let Some(id) = &documentation.id {
                ids.insert(id.selector());
            }
        }
    }
}
/// Collects every identity owned by a node subtree (for pruning on delete). It must stay within
/// `element_identities` and mirror `activity_read::node_activity_read`; it is private to
/// `commands`, so `tests/walker_agreement.rs` does not check it directly.
pub(super) fn collect_node_ids(
    node: &Node,
    ids: &mut std::collections::HashSet<ElementSelector>,
    depth: usize,
) {
    ids.insert(node.id.selector());
    let activity = crate::activity_read::node_activity_read(node);
    ids.extend(activity.ids.iter().map(|id| id.selector()));
    for base in activity.bases {
        collect_base_documentation_ids(Some(base), ids);
    }
    for expression in activity.expressions {
        collect_expression_ids(expression, ids);
    }
    if let crate::bpmn_metadata::DocumentationSet::Structured(items) = &node.documentation {
        for item in items {
            if let Some(id) = &item.id {
                ids.insert(id.selector());
            }
        }
    }
    let mut collect_meta = |metadata: &Option<crate::bpmn_data::ItemAwareMetadata>| {
        if let Some(id) = metadata
            .as_ref()
            .and_then(|m| m.data_state.as_ref())
            .and_then(|s| s.id.as_ref())
        {
            ids.insert(id.selector());
        }
    };
    collect_meta(&node.item_aware);
    if let Some(value) = &node.activity
        && let Some(expression) = &value.completion_condition
    {
        collect_expression_ids(expression, ids);
    }
    if let Some(characteristics) = &node.loop_characteristics {
        if let Some(id) = characteristics.id() {
            ids.insert(id.selector());
        }
        collect_base_documentation_ids(characteristics.base(), ids);
        match characteristics {
            crate::bpmn_model::LoopCharacteristics::Standard { condition, .. } => {
                if let Some(expression) = condition {
                    collect_expression_ids(expression, ids);
                }
            }
            crate::bpmn_model::LoopCharacteristics::MultiInstance {
                cardinality,
                completion_condition,
                ..
            } => {
                if let Some(expression) = cardinality {
                    collect_expression_ids(expression, ids);
                }
                if let Some(expression) = completion_condition {
                    collect_expression_ids(expression, ids);
                }
            }
        }
    }
    if let Some(crate::bpmn_model::NodeDetails::Gateway {
        activation_condition: Some(expression),
        ..
    }) = &node.details
    {
        collect_expression_ids(expression, ids);
    }
    if let Some(crate::bpmn_model::NodeDetails::Event { properties, .. }) = &node.details {
        for property in properties {
            collect_base_documentation_ids(property.base.as_ref(), ids);
            if let Some(id) = &property.id {
                ids.insert(id.selector());
            }
            if let Some(id) = property
                .item_aware
                .as_ref()
                .and_then(|m| m.data_state.as_ref())
                .and_then(|s| s.id.as_ref())
            {
                ids.insert(id.selector());
            }
        }
    }
    if let Some(io) = &node.io_specification {
        collect_base_documentation_ids(io.base.as_ref(), ids);
        if let Some(id) = &io.id {
            ids.insert(id.selector());
        }
        ids.extend(
            io.inputs
                .iter()
                .chain(&io.outputs)
                .filter_map(|p| p.id.as_ref().map(|id| id.selector())),
        );
        for port in io.inputs.iter().chain(&io.outputs) {
            collect_base_documentation_ids(port.base.as_ref(), ids);
        }
        for set in io.input_sets.iter().chain(&io.output_sets) {
            collect_base_documentation_ids(set.base.as_ref(), ids);
        }
        ids.extend(
            io.input_sets
                .iter()
                .chain(&io.output_sets)
                .filter_map(|set| set.id.as_ref().map(|id| id.selector())),
        );
        ids.extend(io.inputs.iter().chain(&io.outputs).filter_map(|port| {
            port.item_aware
                .as_ref()
                .and_then(|m| m.data_state.as_ref())
                .and_then(|s| s.id.as_ref())
                .map(|id| id.selector())
        }));
    }
    if let Some(crate::bpmn_model::NodeDetails::Event {
        definitions, io, ..
    }) = &node.details
    {
        for definition in definitions {
            collect_base_documentation_ids(definition.base(), ids);
            if let Some(id) = definition.id() {
                ids.insert(id.selector());
            }
            let expression = match definition {
                crate::bpmn_model::EventDefinition::Conditional { expression, .. } => {
                    Some(expression)
                }
                crate::bpmn_model::EventDefinition::Timer { expression, .. } => expression.as_ref(),
                _ => None,
            };
            if let Some(expression) = expression {
                collect_expression_ids(expression, ids);
            }
        }
        if let Some(io) = io {
            let (ports, set) = match io {
                crate::bpmn_model::EventIo::Catch {
                    outputs,
                    output_set,
                } => (outputs, output_set),
                crate::bpmn_model::EventIo::Throw { inputs, input_set } => (inputs, input_set),
            };
            for port in ports {
                collect_base_documentation_ids(port.base.as_ref(), ids);
                if let Some(id) = &port.id {
                    ids.insert(id.selector());
                }
                if let Some(id) = port
                    .item_aware
                    .as_ref()
                    .and_then(|m| m.data_state.as_ref())
                    .and_then(|s| s.id.as_ref())
                {
                    ids.insert(id.selector());
                }
            }
            if let Some(id) = set.as_ref().and_then(|s| s.id.as_ref()) {
                ids.insert(id.selector());
            }
            if let Some(set) = set {
                collect_base_documentation_ids(set.base.as_ref(), ids);
            }
        }
    }
    if depth >= 32 {
        return;
    }
    if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
        for child in &content.nodes {
            collect_node_ids(child, ids, depth + 1);
        }
        ids.extend(content.lanes.iter().map(|l| l.id.selector()));
        ids.extend(content.flows.iter().map(|f| f.id.selector()));
        ids.extend(content.groups.iter().map(|g| g.id.selector()));
        collect_base_documentation_ids(content.lane_set_base.as_ref(), ids);
        for lane in &content.lanes {
            collect_base_documentation_ids(lane.base.as_ref(), ids);
            collect_base_documentation_ids(lane.child_lane_set_base.as_ref(), ids);
        }
        for group in &content.groups {
            collect_base_documentation_ids(group.base.as_ref(), ids);
        }
        for object in &content.data_objects {
            collect_base_documentation_ids(object.base.as_ref(), ids);
        }
        for flow in &content.flows {
            if let Some(expression) = &flow.condition {
                collect_expression_ids(expression, ids);
            }
            if let crate::bpmn_metadata::DocumentationSet::Structured(items) = &flow.documentation {
                for item in items {
                    if let Some(id) = &item.id {
                        ids.insert(id.selector());
                    }
                }
            }
        }
        if let Some(id) = &content.lane_set_id {
            ids.insert(id.selector());
        }
        ids.extend(
            content
                .lanes
                .iter()
                .filter_map(|lane| lane.child_lane_set_id.as_ref().map(|id| id.selector())),
        );
        ids.extend(
            content
                .data_objects
                .iter()
                .filter_map(|o| o.id.as_ref().map(|id| id.selector())),
        );
        for association in &content.data_associations {
            collect_association_ids(association, ids);
        }
    }
}
/// Clear gateway and activity default flows that point at deleted flows.
pub(super) fn clear_removed_defaults(
    nodes: &mut [Node],
    removed: &std::collections::HashSet<ElementSelector>,
) {
    for node in nodes {
        if let Some(activity) = &mut node.activity
            && activity
                .default_flow
                .as_ref()
                .is_some_and(|flow| removed.contains(flow))
        {
            activity.default_flow = None;
        }
        match &mut node.details {
            Some(crate::bpmn_model::NodeDetails::Gateway { default_flow, .. })
                if default_flow
                    .as_ref()
                    .is_some_and(|flow| removed.contains(flow)) =>
            {
                *default_flow = None;
            }
            Some(crate::bpmn_model::NodeDetails::SubProcess { content }) => {
                clear_removed_defaults(&mut content.nodes, removed);
            }
            _ => {}
        }
    }
}
pub(super) fn delete_nested(
    nodes: &mut Vec<Node>,
    lanes: &mut Vec<Lane>,
    flows: &mut Vec<Flow>,
    associations: &mut Vec<crate::bpmn_data::DataAssociation>,
    id: &ElementSelector,
    removed: &mut std::collections::HashSet<ElementSelector>,
    depth: usize,
) -> bool {
    if depth > 32 {
        return false;
    }
    if let Some(index) = nodes.iter().position(|n| &n.id == id) {
        collect_node_ids(&nodes[index], removed, depth);
        nodes.remove(index);
        associations.retain(|association| {
            if association.owner == *id {
                collect_association_ids(association, removed);
                false
            } else {
                true
            }
        });
        for lane in lanes {
            lane.members.retain(|member| member != id);
        }
        return true;
    }
    if lanes.iter().any(|l| &l.id == id) {
        // A lane goes with its child lanes at any depth (bpmn-js removes the lane's children).
        let mut doomed = vec![id.clone()];
        for _ in 0..32 {
            let before = doomed.len();
            for lane in lanes.iter() {
                if lane.parent.as_ref().is_some_and(|p| doomed.contains(p))
                    && !doomed.contains(&lane.id.selector())
                {
                    doomed.push(lane.id.selector());
                }
            }
            if doomed.len() == before {
                break;
            }
        }
        for lane in lanes.iter().filter(|l| doomed.contains(&l.id.selector())) {
            collect_base_documentation_ids(lane.base.as_ref(), removed);
            collect_base_documentation_ids(lane.child_lane_set_base.as_ref(), removed);
            if let Some(child) = &lane.child_lane_set_id {
                removed.insert(child.selector());
            }
        }
        lanes.retain(|l| !doomed.contains(&l.id.selector()));
        removed.extend(doomed);
        return true;
    }
    if let Some(index) = flows.iter().position(|f| &f.id == id) {
        if let Some(expression) = &flows[index].condition {
            collect_expression_ids(expression, removed);
        }
        if let crate::bpmn_metadata::DocumentationSet::Structured(items) =
            &flows[index].documentation
        {
            for item in items {
                if let Some(id) = &item.id {
                    removed.insert(id.selector());
                }
            }
        }
        flows.remove(index);
        removed.insert(id.clone());
        return true;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details
            && delete_nested(
                &mut content.nodes,
                &mut content.lanes,
                &mut content.flows,
                &mut content.data_associations,
                id,
                removed,
                depth + 1,
            )
        {
            return true;
        }
    }
    false
}
pub(super) type DataPartsMut<'a> = (
    &'a mut Vec<crate::bpmn_data::DataObject>,
    &'a mut Vec<crate::bpmn_data::DataAssociation>,
);
pub(super) fn data_parts_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<DataPartsMut<'a>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        let p = &mut document.processes[index];
        return Ok((&mut p.data_objects, &mut p.data_associations));
    }
    for process in &mut document.processes {
        if let Some(c) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok((&mut c.data_objects, &mut c.data_associations));
        }
    }
    Err(invalid("process or subprocess not found"))
}
pub(super) fn groups_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Vec<crate::bpmn_model::Group>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].groups);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.groups);
        }
    }
    Err(invalid("process or subprocess not found"))
}
pub(super) fn lane_set_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Option<crate::ids::ElementIdentity>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].lane_set_id);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.lane_set_id);
        }
    }
    Err(invalid("process or subprocess not found"))
}
pub(super) fn lane_set_name_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Option<String>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].lane_set_name);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.lane_set_name);
        }
    }
    Err(invalid("process or subprocess not found"))
}
/// tLaneSet@name is an xsd:string: only XML 1.0 characters, bounded length.
pub(super) fn check_lane_set_name(name: &Option<String>) -> Result<(), CommandError> {
    match name {
        Some(n)
            if n.len() > 16_384
                || n.chars().any(|c| {
                    !matches!(c as u32, 9 | 10 | 13 | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
                }) =>
        {
            Err(invalid("invalid lane set name"))
        }
        _ => Ok(()),
    }
}
pub(super) fn flow_mut<'a>(
    d: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Flow, CommandError> {
    for process in &mut d.processes {
        if let Some(flow) = process.flows.iter_mut().find(|f| &f.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = nested_flow_mut(&mut process.nodes, id, 0) {
            return Ok(flow);
        }
    }
    for choreography in &mut d.choreographies {
        if let Some(flow) = choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
            .find(|f| &f.id == id)
        {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut choreography.extras, id) {
            return Ok(flow);
        }
    }
    for collaboration in &mut d.additional_collaborations {
        if let Some(flow) = collaboration.flows.iter_mut().find(|flow| &flow.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut collaboration.extras, id) {
            return Ok(flow);
        }
    }
    if let Some(collaboration) = &mut d.collaboration {
        if let Some(flow) = collaboration.flows.iter_mut().find(|flow| &flow.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut collaboration.extras, id) {
            return Ok(flow);
        }
    }
    Err(invalid("flow not found"))
}
pub(super) fn artifact_flow_mut<'a>(
    extras: &'a mut Option<crate::bpmn_collaboration::CollaborationExtras>,
    id: &ElementSelector,
) -> Option<&'a mut Flow> {
    use crate::bpmn_collaboration::CollaborationArtifact;
    extras
        .as_mut()?
        .artifacts
        .iter_mut()
        .find_map(|artifact| match artifact {
            CollaborationArtifact::Association(flow) if &flow.id == id => Some(flow.as_mut()),
            _ => None,
        })
}
