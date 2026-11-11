//! Typed traversal for explicit BPMN identity edits.
//! Commands apply these traversals to an isolated candidate before validation.
use crate::{
    ids::{ElementIdentity, ElementSelector},
    model::*,
};

/// Visit selectors held by process and subprocess flow containers.
fn scope_selectors(
    nodes: &mut [Node],
    lanes: &mut [Lane],
    flows: &mut [Flow],
    groups: &mut [crate::bpmn_model::Group],
    associations: &mut [crate::bpmn_data::DataAssociation],
    visit: &mut impl FnMut(&mut ElementSelector),
) {
    for flow in flows {
        visit(&mut flow.source);
        visit(&mut flow.target);
    }
    for lane in lanes {
        if let Some(parent) = &mut lane.parent {
            visit(parent);
        }
        for member in &mut lane.members {
            visit(member);
        }
    }
    for group in groups {
        if let Some(value) = &mut group.category_value_ref {
            visit(value);
        }
    }
    for association in associations {
        visit(&mut association.owner);
        for source in &mut association.sources {
            visit(source);
        }
        visit(&mut association.target);
    }
    for node in nodes {
        if let Some(activity) = &mut node.activity
            && let Some(flow) = &mut activity.default_flow
        {
            visit(flow);
        }
        if let Some(io) = &mut node.io_specification {
            io_selectors(io, visit);
        }
        use crate::bpmn_model::{EventDefinition, NodeDetails};
        match &mut node.details {
            Some(NodeDetails::Gateway { default_flow, .. }) => {
                if let Some(flow) = default_flow {
                    visit(flow);
                }
            }
            Some(NodeDetails::DataObjectReference { data_object_ref }) => {
                if let Some(reference) = data_object_ref {
                    visit(reference);
                }
            }
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(crate::bpmn_data::DataStoreRef::Legacy(reference)),
            }) => visit(reference),
            Some(NodeDetails::BoundaryEvent { attached_to, .. }) => visit(attached_to),
            Some(NodeDetails::Event {
                definitions,
                attached_to,
                ..
            }) => {
                if let Some(reference) = attached_to {
                    visit(reference);
                }
                for definition in definitions {
                    match definition {
                        EventDefinition::Message {
                            message_ref,
                            operation_ref,
                            ..
                        } => {
                            if let Some(reference) = message_ref {
                                visit(reference);
                            }
                            if let Some(reference) = operation_ref {
                                visit(reference);
                            }
                        }
                        EventDefinition::Signal { signal_ref, .. } => {
                            if let Some(reference) = signal_ref {
                                visit(reference);
                            }
                        }
                        EventDefinition::Error { error_ref, .. } => {
                            if let Some(reference) = error_ref {
                                visit(reference);
                            }
                        }
                        EventDefinition::Escalation { escalation_ref, .. } => {
                            if let Some(reference) = escalation_ref {
                                visit(reference);
                            }
                        }
                        EventDefinition::Link {
                            sources, target, ..
                        } => {
                            for source in sources {
                                visit(source);
                            }
                            if let Some(reference) = target {
                                visit(reference);
                            }
                        }
                        EventDefinition::Compensate { activity_ref, .. } => {
                            if let Some(reference) = activity_ref {
                                visit(reference);
                            }
                        }
                        EventDefinition::Conditional { .. }
                        | EventDefinition::Cancel { .. }
                        | EventDefinition::Terminate { .. }
                        | EventDefinition::Timer { .. } => {}
                    }
                }
            }
            Some(NodeDetails::SubProcess { content }) => scope_selectors(
                &mut content.nodes,
                &mut content.lanes,
                &mut content.flows,
                &mut content.groups,
                &mut content.data_associations,
                visit,
            ),
            Some(
                NodeDetails::TextAnnotation { .. }
                | NodeDetails::DataStoreReference { .. }
                | NodeDetails::CallActivity { .. },
            )
            | None => {}
        }
    }
}
fn io_selectors(
    io: &mut crate::bpmn_data::IoSpecification,
    visit: &mut impl FnMut(&mut ElementSelector),
) {
    for set in io.input_sets.iter_mut().chain(&mut io.output_sets) {
        for reference in set
            .data_refs
            .iter_mut()
            .chain(&mut set.optional_refs)
            .chain(&mut set.while_executing_refs)
            .chain(&mut set.counterpart_refs)
        {
            visit(reference);
        }
    }
}
fn document_selectors(d: &mut BpmnDocument, visit: &mut impl FnMut(&mut ElementSelector)) {
    for process in &mut d.processes {
        if let Some(io) = &mut process.io_specification {
            io_selectors(io, visit);
        }
        scope_selectors(
            &mut process.nodes,
            &mut process.lanes,
            &mut process.flows,
            &mut process.groups,
            &mut process.data_associations,
            visit,
        );
    }
    if let Some(collaboration) = &mut d.collaboration {
        for participant in &mut collaboration.participants {
            if let Some(process) = &mut participant.process {
                visit(process);
            }
        }
        for flow in &mut collaboration.flows {
            visit(&mut flow.source);
            visit(&mut flow.target);
        }
        for node in &mut collaboration.conversation_nodes {
            for reference in &mut node.participant_refs {
                visit(reference);
            }
            if let Some(parent) = &mut node.parent {
                visit(parent);
            }
        }
        for link in &mut collaboration.conversation_links {
            visit(&mut link.source);
            visit(&mut link.target);
        }
    }
    for choreography in &mut d.choreographies {
        for participant in &mut choreography.participants {
            if let Some(process) = &mut participant.process {
                visit(process);
            }
        }
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            visit(&mut flow.source);
            visit(&mut flow.target);
        }
        for activity in &mut choreography.activities {
            for reference in &mut activity.participant_refs {
                visit(reference);
            }
            visit(&mut activity.initiating_participant);
            for reference in &mut activity.message_flow_refs {
                visit(reference);
            }
        }
    }
    for diagram in &mut d.diagrams {
        visit(&mut diagram.plane);
        for shape in &mut diagram.shapes {
            visit(&mut shape.element);
        }
        for edge in &mut diagram.edges {
            visit(&mut edge.element);
        }
    }
}
fn optional_identity(
    value: &mut Option<ElementIdentity>,
    visit: &mut impl FnMut(&mut ElementIdentity),
) {
    if let Some(value) = value {
        visit(value);
    }
}
fn expression_id(
    expression: &mut crate::bpmn_model::Expression,
    visit: &mut impl FnMut(&mut ElementIdentity),
) {
    if let crate::bpmn_model::Expression::Structured(data) = expression {
        optional_identity(&mut data.id, visit);
    }
}
fn metadata_ids(
    meta: &mut Option<crate::bpmn_data::ItemAwareMetadata>,
    visit: &mut impl FnMut(&mut ElementIdentity),
) {
    if let Some(state) = meta.as_mut().and_then(|m| m.data_state.as_mut()) {
        optional_identity(&mut state.id, visit);
    }
}
fn io_ids(
    io: &mut crate::bpmn_data::IoSpecification,
    visit: &mut impl FnMut(&mut ElementIdentity),
) {
    optional_identity(&mut io.id, visit);
    for port in io.inputs.iter_mut().chain(&mut io.outputs) {
        optional_identity(&mut port.id, visit);
        metadata_ids(&mut port.item_aware, visit);
    }
    for set in io.input_sets.iter_mut().chain(&mut io.output_sets) {
        optional_identity(&mut set.id, visit);
    }
}
fn flow_id(flow: &mut Flow, visit: &mut impl FnMut(&mut ElementIdentity)) {
    visit(&mut flow.id);
    if let Some(condition) = &mut flow.condition {
        expression_id(condition, visit);
    }
}
#[allow(clippy::too_many_arguments)]
fn scope_ids(
    nodes: &mut [Node],
    lanes: &mut [Lane],
    flows: &mut [Flow],
    groups: &mut [crate::bpmn_model::Group],
    objects: &mut [crate::bpmn_data::DataObject],
    associations: &mut [crate::bpmn_data::DataAssociation],
    lane_set_id: &mut Option<ElementIdentity>,
    visit: &mut impl FnMut(&mut ElementIdentity),
) {
    optional_identity(lane_set_id, visit);
    for lane in lanes {
        visit(&mut lane.id);
        optional_identity(&mut lane.child_lane_set_id, visit);
    }
    for flow in flows {
        flow_id(flow, visit);
    }
    for group in groups {
        visit(&mut group.id);
    }
    for object in objects {
        optional_identity(&mut object.id, visit);
        metadata_ids(&mut object.item_aware, visit);
    }
    for association in associations {
        optional_identity(&mut association.id, visit);
        if let Some(expression) = &mut association.transformation {
            expression_id(expression, visit);
        }
        for assignment in &mut association.assignments {
            optional_identity(&mut assignment.id, visit);
            expression_id(&mut assignment.from, visit);
            expression_id(&mut assignment.to, visit);
        }
    }
    for node in nodes {
        visit(&mut node.id);
        metadata_ids(&mut node.item_aware, visit);
        if let Some(io) = &mut node.io_specification {
            io_ids(io, visit);
        }
        if let Some(activity) = &mut node.activity
            && let Some(expression) = &mut activity.completion_condition
        {
            expression_id(expression, visit);
        }
        use crate::bpmn_model::{EventDefinition, LoopCharacteristics, NodeDetails};
        if let Some(characteristics) = &mut node.loop_characteristics {
            match characteristics {
                LoopCharacteristics::Standard { condition, .. } => {
                    if let Some(value) = condition {
                        expression_id(value, visit);
                    }
                }
                LoopCharacteristics::MultiInstance {
                    cardinality,
                    completion_condition,
                    ..
                } => {
                    if let Some(value) = cardinality {
                        expression_id(value, visit);
                    }
                    if let Some(value) = completion_condition {
                        expression_id(value, visit);
                    }
                }
            }
        }
        match &mut node.details {
            Some(NodeDetails::SubProcess { content }) => scope_ids(
                &mut content.nodes,
                &mut content.lanes,
                &mut content.flows,
                &mut content.groups,
                &mut content.data_objects,
                &mut content.data_associations,
                &mut content.lane_set_id,
                visit,
            ),
            Some(NodeDetails::Gateway {
                activation_condition: Some(expression),
                ..
            }) => expression_id(expression, visit),
            Some(NodeDetails::Event { definitions, .. }) => {
                for definition in definitions {
                    match definition {
                        EventDefinition::Conditional { id, expression }
                        | EventDefinition::Timer { id, expression, .. } => {
                            optional_identity(id, visit);
                            expression_id(expression, visit);
                        }
                        EventDefinition::Message { id, .. }
                        | EventDefinition::Signal { id, .. }
                        | EventDefinition::Error { id, .. }
                        | EventDefinition::Escalation { id, .. }
                        | EventDefinition::Link { id, .. }
                        | EventDefinition::Cancel { id }
                        | EventDefinition::Compensate { id, .. }
                        | EventDefinition::Terminate { id } => optional_identity(id, visit),
                    }
                }
            }
            _ => {}
        }
    }
}
fn document_ids(d: &mut BpmnDocument, visit: &mut impl FnMut(&mut ElementIdentity)) {
    visit(&mut d.definitions_id);
    for category in &mut d.categories {
        visit(&mut category.id);
        for value in &mut category.values {
            visit(&mut value.id);
        }
    }
    for resource in &mut d.event_resources {
        use crate::bpmn_model::EventResource;
        match resource {
            EventResource::Signal { id, .. }
            | EventResource::Message { id, .. }
            | EventResource::Error { id, .. }
            | EventResource::Escalation { id, .. } => visit(id),
        }
    }
    for store in &mut d.data_stores {
        optional_identity(&mut store.id, visit);
        metadata_ids(&mut store.item_aware, visit);
    }
    for item in &mut d.catalog.item_definitions {
        optional_identity(&mut item.id, visit);
    }
    for interface in &mut d.catalog.interfaces {
        optional_identity(&mut interface.id, visit);
        for operation in &mut interface.operations {
            optional_identity(&mut operation.id, visit);
        }
    }
    for resource in &mut d.catalog.resources {
        optional_identity(&mut resource.id, visit);
        for parameter in &mut resource.parameters {
            optional_identity(&mut parameter.id, visit);
        }
    }
    for process in &mut d.processes {
        visit(&mut process.id);
        if let Some(io) = &mut process.io_specification {
            io_ids(io, visit);
        }
        scope_ids(
            &mut process.nodes,
            &mut process.lanes,
            &mut process.flows,
            &mut process.groups,
            &mut process.data_objects,
            &mut process.data_associations,
            &mut process.lane_set_id,
            visit,
        );
    }
    if let Some(collaboration) = &mut d.collaboration {
        visit(&mut collaboration.id);
        for participant in &mut collaboration.participants {
            visit(&mut participant.id);
        }
        for flow in &mut collaboration.flows {
            flow_id(flow, visit);
        }
        for node in &mut collaboration.conversation_nodes {
            visit(&mut node.id);
        }
        for link in &mut collaboration.conversation_links {
            visit(&mut link.id);
        }
    }
    for choreography in &mut d.choreographies {
        visit(&mut choreography.id);
        for participant in &mut choreography.participants {
            visit(&mut participant.id);
        }
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            flow_id(flow, visit);
        }
        for activity in &mut choreography.activities {
            visit(&mut activity.id);
        }
    }
    for diagram in &mut d.diagrams {
        visit(&mut diagram.id);
        visit(&mut diagram.plane_id);
        for shape in &mut diagram.shapes {
            visit(&mut shape.id);
        }
        for edge in &mut diagram.edges {
            visit(&mut edge.id);
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum QNameKind {
    Item,
    Message,
    Operation,
    Error,
    Process,
    DataStore,
}
fn optional_qname(
    value: &mut Option<crate::bpmn_model::QNameRef>,
    kind: QNameKind,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    if let Some(value) = value {
        visit(value, kind);
    }
}
fn expression_qnames(
    value: &mut crate::bpmn_model::Expression,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    if let crate::bpmn_model::Expression::Structured(crate::bpmn_model::ExpressionData {
        kind:
            crate::bpmn_model::ExpressionKind::Formal {
                evaluates_to_type_ref,
                ..
            },
        ..
    }) = value
    {
        optional_qname(evaluates_to_type_ref, QNameKind::Item, visit);
    }
}
fn meta_qnames(
    value: &mut Option<crate::bpmn_data::ItemAwareMetadata>,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    if let Some(value) = value {
        optional_qname(&mut value.item_subject_ref, QNameKind::Item, visit);
    }
}
fn io_qnames(
    value: &mut crate::bpmn_data::IoSpecification,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    for port in value.inputs.iter_mut().chain(&mut value.outputs) {
        meta_qnames(&mut port.item_aware, visit);
    }
}
fn flow_qnames(
    flow: &mut Flow,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    if let Some(value) = &mut flow.condition {
        expression_qnames(value, visit);
    }
}
fn scope_qnames(
    nodes: &mut [Node],
    objects: &mut [crate::bpmn_data::DataObject],
    associations: &mut [crate::bpmn_data::DataAssociation],
    flows: &mut [Flow],
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    for object in objects {
        meta_qnames(&mut object.item_aware, visit);
    }
    for association in associations {
        if let Some(value) = &mut association.transformation {
            expression_qnames(value, visit);
        }
        for assignment in &mut association.assignments {
            expression_qnames(&mut assignment.from, visit);
            expression_qnames(&mut assignment.to, visit);
        }
    }
    for flow in flows {
        flow_qnames(flow, visit);
    }
    for node in nodes {
        meta_qnames(&mut node.item_aware, visit);
        if let Some(io) = &mut node.io_specification {
            io_qnames(io, visit);
        }
        if let Some(activity) = &mut node.activity {
            optional_qname(&mut activity.message_ref, QNameKind::Message, visit);
            optional_qname(&mut activity.operation_ref, QNameKind::Operation, visit);
            if let Some(value) = &mut activity.completion_condition {
                expression_qnames(value, visit);
            }
        }
        use crate::bpmn_model::{EventDefinition, LoopCharacteristics, NodeDetails};
        if let Some(characteristics) = &mut node.loop_characteristics {
            match characteristics {
                LoopCharacteristics::Standard { condition, .. } => {
                    if let Some(value) = condition {
                        expression_qnames(value, visit);
                    }
                }
                LoopCharacteristics::MultiInstance {
                    cardinality,
                    completion_condition,
                    ..
                } => {
                    if let Some(value) = cardinality {
                        expression_qnames(value, visit);
                    }
                    if let Some(value) = completion_condition {
                        expression_qnames(value, visit);
                    }
                }
            }
        }
        match &mut node.details {
            Some(NodeDetails::CallActivity { called_element }) => {
                visit(called_element, QNameKind::Process)
            }
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(crate::bpmn_data::DataStoreRef::Qualified(reference)),
            }) => visit(reference, QNameKind::DataStore),
            Some(NodeDetails::Gateway {
                activation_condition: Some(value),
                ..
            }) => expression_qnames(value, visit),
            Some(NodeDetails::Event { definitions, .. }) => {
                for definition in definitions {
                    match definition {
                        EventDefinition::Conditional { expression, .. }
                        | EventDefinition::Timer { expression, .. } => {
                            expression_qnames(expression, visit)
                        }
                        _ => {}
                    }
                }
            }
            Some(NodeDetails::SubProcess { content }) => scope_qnames(
                &mut content.nodes,
                &mut content.data_objects,
                &mut content.data_associations,
                &mut content.flows,
                visit,
            ),
            _ => {}
        }
    }
}
fn document_qnames(
    d: &mut BpmnDocument,
    visit: &mut impl FnMut(&mut crate::bpmn_model::QNameRef, QNameKind),
) {
    for interface in &mut d.catalog.interfaces {
        for operation in &mut interface.operations {
            visit(&mut operation.in_message_ref, QNameKind::Message);
            optional_qname(&mut operation.out_message_ref, QNameKind::Message, visit);
            for value in &mut operation.error_refs {
                visit(value, QNameKind::Error);
            }
        }
    }
    for store in &mut d.data_stores {
        meta_qnames(&mut store.item_aware, visit);
    }
    for process in &mut d.processes {
        if let Some(io) = &mut process.io_specification {
            io_qnames(io, visit);
        }
        scope_qnames(
            &mut process.nodes,
            &mut process.data_objects,
            &mut process.data_associations,
            &mut process.flows,
            visit,
        );
    }
    if let Some(collaboration) = &mut d.collaboration {
        for flow in &mut collaboration.flows {
            flow_qnames(flow, visit);
        }
    }
    for choreography in &mut d.choreographies {
        for flow in choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
        {
            flow_qnames(flow, visit);
        }
    }
}

/// Edits an identity and returns a selector translation when legacy identity is promoted.
/// The caller must operate on an isolated candidate and commit only after project validation.
pub(crate) fn set_xml_id(
    project: &mut ProjectData,
    document_id: crate::ids::DocumentId,
    selected: &ElementSelector,
    new_xml_id: Option<crate::ids::XmlId>,
) -> Result<Option<(ElementSelector, ElementSelector)>, String> {
    let doc = project
        .documents
        .iter_mut()
        .find(|d| d.id == document_id)
        .ok_or("document not found")?;
    if !matches!(doc.capability, Capability::Editable) {
        return Err("protected source prevents safe reference rewriting".into());
    }
    let doc = std::sync::Arc::make_mut(doc);
    let mut count = 0;
    let mut old_xml_id = None;
    let mut collision = false;
    document_ids(doc, &mut |identity| {
        if identity.selector() == *selected {
            count += 1;
            old_xml_id = identity.xml_id().cloned();
        } else if new_xml_id
            .as_ref()
            .is_some_and(|id| identity.xml_id() == Some(id))
        {
            collision = true;
        }
    });
    if count != 1 {
        return Err("element selector is missing or ambiguous".into());
    }
    if collision {
        return Err("XML ID already exists".into());
    }
    if old_xml_id == new_xml_id {
        return Ok(None);
    }
    use crate::bpmn_model::EventResource;
    let target_kind = if doc
        .catalog
        .item_definitions
        .iter()
        .any(|v| v.id.as_ref().is_some_and(|id| id.selector() == *selected))
    {
        Some(QNameKind::Item)
    } else if doc
        .catalog
        .interfaces
        .iter()
        .flat_map(|v| &v.operations)
        .any(|v| v.id.as_ref().is_some_and(|id| id.selector() == *selected))
    {
        Some(QNameKind::Operation)
    } else if doc
        .event_resources
        .iter()
        .any(|v| matches!(v, EventResource::Message { id, .. } if id.selector() == *selected))
    {
        Some(QNameKind::Message)
    } else if doc
        .event_resources
        .iter()
        .any(|v| matches!(v, EventResource::Error { id, .. } if id.selector() == *selected))
    {
        Some(QNameKind::Error)
    } else if doc.processes.iter().any(|v| v.id.selector() == *selected) {
        Some(QNameKind::Process)
    } else if doc
        .data_stores
        .iter()
        .any(|v| v.id.as_ref().is_some_and(|id| id.selector() == *selected))
    {
        Some(QNameKind::DataStore)
    } else {
        None
    };
    let namespace = doc
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.clone())
        .or_else(|| {
            doc.provenance
                .is_none()
                .then(|| GENERATED_TARGET_NAMESPACE.to_string())
        });
    let mut qname_referenced = false;
    let mut has_qnames = false;
    document_qnames(doc, &mut |reference, kind| {
        has_qnames |= Some(kind) == target_kind;
        if Some(kind) == target_kind
            && old_xml_id.as_ref() == Some(&reference.local)
            && namespace.as_deref() == Some(reference.namespace.as_str())
        {
            qname_referenced = true;
        }
    });
    if new_xml_id.is_none() && qname_referenced {
        return Err("cannot clear an XML ID while XML references remain".into());
    }
    if old_xml_id.is_some() && namespace.is_none() && has_qnames {
        return Err("cannot determine namespace for safe reference rewriting".into());
    }
    let new_selector = match selected {
        ElementSelector::Native { .. } => selected.clone(),
        ElementSelector::Xml(_) => ElementSelector::Native {
            native_id: crate::ids::NativeElementId::new(),
        },
    };
    document_ids(doc, &mut |identity| {
        if identity.selector() == *selected {
            let ElementSelector::Native { native_id } = new_selector else {
                unreachable!()
            };
            *identity = ElementIdentity::Native {
                native_id,
                xml_id: new_xml_id.clone(),
            };
        }
    });
    document_selectors(doc, &mut |reference| {
        if reference == selected {
            *reference = new_selector.clone();
        }
    });
    if let (Some(old), Some(new), Some(namespace)) = (
        old_xml_id.as_ref(),
        new_xml_id.as_ref(),
        namespace.as_deref(),
    ) {
        document_qnames(doc, &mut |reference, kind| {
            if Some(kind) == target_kind
                && &reference.local == old
                && reference.namespace == namespace
            {
                reference.local = new.clone();
            }
        });
    }
    for key in project
        .links
        .iter_mut()
        .map(|v| &mut v.source)
        .chain(project.dictionary_links.iter_mut().map(|v| &mut v.source))
        .chain(project.comments.iter_mut().map(|v| &mut v.target))
    {
        if key.document_id == document_id && key.xml_id == *selected {
            key.xml_id = new_selector.clone();
        }
    }
    Ok((new_selector != *selected).then(|| (selected.clone(), new_selector)))
}

/// Promote an existing null/omitted optional identity at a transient JSON pointer.
/// The typed round trip and identity visitor ensure the path identifies an actual
/// ElementIdentity field; no generic project value is accepted as an identity.
pub(crate) fn promote_null_identity(
    project: &mut ProjectData,
    document_id: crate::ids::DocumentId,
    model_path: &str,
    xml_id: Option<crate::ids::XmlId>,
) -> Result<ElementSelector, String> {
    let doc = project
        .documents
        .iter_mut()
        .find(|d| d.id == document_id)
        .ok_or("document not found")?;
    if !matches!(doc.capability, Capability::Editable) {
        return Err("protected source prevents identity promotion".into());
    }
    let parent_path = model_path
        .strip_suffix("/id")
        .ok_or("identity path must end in /id")?;
    let mut value = serde_json::to_value(doc.as_ref()).map_err(|e| e.to_string())?;
    let parent = value
        .pointer_mut(parent_path)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("identity parent not found")?;
    if parent.get("id").is_some_and(|value| !value.is_null()) {
        return Err("identity is not null or absent".into());
    }
    let native_id = crate::ids::NativeElementId::new();
    let identity = ElementIdentity::Native { native_id, xml_id };
    parent.insert(
        "id".into(),
        serde_json::to_value(identity).map_err(|e| e.to_string())?,
    );
    let mut candidate: BpmnDocument = serde_json::from_value(value.clone())
        .map_err(|_| "path is not an optional element identity")?;
    if serde_json::to_value(&candidate).map_err(|e| e.to_string())? != value {
        return Err("promotion would change unmodeled document content".into());
    }
    let mut occurrences = 0;
    document_ids(&mut candidate, &mut |id| {
        if matches!(id, ElementIdentity::Native { native_id: current, .. } if *current == native_id)
        {
            occurrences += 1;
        }
    });
    if occurrences != 1 {
        return Err("path is not a unique optional element identity".into());
    }
    *doc = std::sync::Arc::new(candidate);
    Ok(ElementSelector::Native { native_id })
}
