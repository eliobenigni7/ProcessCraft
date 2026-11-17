//! Process, node, flow and lane-set property mutations.
use super::*;
pub(super) fn set_expression(
    p: &mut ProjectData,
    document_id: DocumentId,
    expression_id: ElementSelector,
    expression: crate::bpmn_model::Expression,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    if !matches!(d.capability, Capability::Editable) {
        return Err(CommandError::CapabilityDenied);
    }
    let opaque = d.has_opaque_xml();
    let mut found = 0;
    let mut changed_identity = false;
    let mut changed_opaque = false;
    crate::metadata_visit::visit_expressions_mut(d, |existing| {
        if let crate::bpmn_model::Expression::Structured(data) = existing
            && data
                .id
                .as_ref()
                .is_some_and(|id| id.selector() == expression_id)
        {
            found += 1;
            let new = match &expression {
                crate::bpmn_model::Expression::Structured(value) => value,
                _ => {
                    changed_identity = true;
                    return;
                }
            };
            if serde_json::to_value(&data.id).ok() != serde_json::to_value(&new.id).ok() {
                changed_identity = true;
            } else if opaque
                && (data.text != new.text
                    || data.namespaces != new.namespaces
                    || data.foreign_attributes != new.foreign_attributes)
            {
                changed_opaque = true;
            } else {
                *existing = expression.clone();
            }
        }
    });
    if found != 1 || changed_identity {
        return Err(invalid("expression not found or identity changed"));
    }
    if changed_opaque {
        return Err(invalid("opaque XML prevents replacing expression content"));
    }
    Ok(())
}
pub(super) fn set_process_executable(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    is_executable: Option<bool>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let process = document
        .processes
        .iter_mut()
        .find(|process| process.id == process_id)
        .ok_or_else(|| invalid("process not found"))?;
    process.is_executable = is_executable;
    Ok(())
}
pub(super) fn set_process_activity_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    metadata: Option<crate::bpmn_activity::ProcessActivityMetadata>,
) -> Result<(), CommandError> {
    let process = doc_structural(p, document_id)?
        .processes
        .iter_mut()
        .find(|process| process.id == process_id)
        .ok_or_else(|| invalid("process not found"))?;
    process.activity_metadata = metadata;
    Ok(())
}
pub(super) fn set_node_flow_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
) -> Result<(), CommandError> {
    node_mut(doc_structural(p, document_id)?, &element_id)?.flow_metadata = metadata;
    Ok(())
}
pub(super) fn set_flow_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
) -> Result<(), CommandError> {
    flow_mut(doc_structural(p, document_id)?, &element_id)?.flow_metadata = metadata;
    Ok(())
}
pub(super) fn set_process_io_specification(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    io_specification: Option<crate::bpmn_data::IoSpecification>,
) -> Result<(), CommandError> {
    let process = doc_structural(p, document_id)?
        .processes
        .iter_mut()
        .find(|p| p.id == process_id)
        .ok_or_else(|| invalid("process not found"))?;
    process.io_specification = io_specification;
    Ok(())
}
pub(super) fn set_activity_properties(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    activity: Option<crate::bpmn_activity::ActivityProperties>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let old_script = crate::queries::find_node(d, &element_id)
        .and_then(|n| n.activity.as_ref())
        .and_then(|a| a.script.as_ref());
    if old_script.is_some_and(|s| matches!(s, crate::bpmn_xml::MixedXml::Structured(_)))
        && old_script != activity.as_ref().and_then(|a| a.script.as_ref())
    {
        return Err(invalid(
            "structured script requires an explicit mixed-content edit",
        ));
    }
    if let Some(activity) = &activity {
        let previous = crate::queries::find_node(d, &element_id).and_then(|n| n.activity.as_ref());
        check_activity_references(d, activity, previous)?;
    }
    let node = node_mut(d, &element_id)?;
    if !node.kind.is_activity() {
        return Err(invalid("activity properties require an activity"));
    }
    node.activity = activity;
    Ok(())
}
pub(super) fn set_association_direction(
    p: &mut ProjectData,
    document_id: DocumentId,
    flow_id: ElementSelector,
    direction: Option<crate::bpmn_model::AssociationDirection>,
) -> Result<(), CommandError> {
    let flow = flow_mut(doc_structural(p, document_id)?, &flow_id)?;
    if flow.kind != FlowKind::Association {
        return Err(invalid("association direction requires an association"));
    }
    flow.association_direction = direction;
    Ok(())
}
pub(super) fn set_child_lane_set_id(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    lane_id: ElementSelector,
    child_lane_set_id: Option<crate::ids::ElementIdentity>,
) -> Result<(), CommandError> {
    let lanes = scope_parts_mut(doc_structural(p, document_id)?, &scope_id)?.1;
    lanes
        .iter_mut()
        .find(|lane| lane.id == lane_id)
        .ok_or_else(|| invalid("lane not found"))?
        .child_lane_set_id = child_lane_set_id;
    Ok(())
}
pub(super) fn set_lane_set_name(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    lane_set_name: Option<String>,
) -> Result<(), CommandError> {
    check_lane_set_name(&lane_set_name)?;
    *lane_set_name_mut(doc_structural(p, document_id)?, &scope_id)? = lane_set_name;
    Ok(())
}
pub(super) fn set_child_lane_set_name(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    lane_id: ElementSelector,
    child_lane_set_name: Option<String>,
) -> Result<(), CommandError> {
    check_lane_set_name(&child_lane_set_name)?;
    scope_parts_mut(doc_structural(p, document_id)?, &scope_id)?
        .1
        .iter_mut()
        .find(|lane| lane.id == lane_id)
        .ok_or_else(|| invalid("lane not found"))?
        .child_lane_set_name = child_lane_set_name;
    Ok(())
}
pub(super) fn set_lane_partition(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    lane_id: ElementSelector,
    partition_element: Option<crate::model::LanePartitionElement>,
    partition_element_ref: Option<crate::bpmn_activity::ModelReference>,
) -> Result<(), CommandError> {
    check_lane_set_name(&partition_element.as_ref().and_then(|e| e.value.clone()))?;
    let lane = scope_parts_mut(doc_structural(p, document_id)?, &scope_id)?
        .1
        .iter_mut()
        .find(|lane| lane.id == lane_id)
        .ok_or_else(|| invalid("lane not found"))?;
    lane.partition_element = partition_element;
    lane.partition_element_ref = partition_element_ref;
    Ok(())
}
pub(super) fn set_node_io_specification(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    io_specification: Option<crate::bpmn_data::IoSpecification>,
) -> Result<(), CommandError> {
    node_mut(doc_structural(p, document_id)?, &element_id)?.io_specification = io_specification;
    Ok(())
}
pub(super) fn set_node_loop(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    loop_characteristics: Option<crate::bpmn_model::LoopCharacteristics>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    node_mut(document, &element_id)?.loop_characteristics = loop_characteristics;
    Ok(())
}
pub(super) fn set_node_details(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    details: Option<crate::bpmn_model::NodeDetails>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let node = node_mut(document, &element_id)?;
    if matches!(
        &node.details,
        Some(crate::bpmn_model::NodeDetails::TextAnnotation {
            text: Some(crate::bpmn_xml::MixedXml::Structured(_)),
            ..
        })
    ) && node.details != details
    {
        return Err(invalid(
            "structured annotation requires an explicit mixed-content edit",
        ));
    }
    node.details = details;
    Ok(())
}
pub(super) fn set_flow_condition(
    p: &mut ProjectData,
    document_id: DocumentId,
    flow_id: ElementSelector,
    condition: Option<crate::bpmn_model::Expression>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let mut found = None;
    for process in &mut d.processes {
        if let Some(flow) = process.flows.iter_mut().find(|f| f.id == flow_id) {
            found = Some(flow);
            break;
        }
        if let Some(flow) = nested_flow_mut(&mut process.nodes, &flow_id, 0) {
            found = Some(flow);
            break;
        }
    }
    let f = found
        .or_else(|| {
            d.collaboration
                .as_mut()
                .and_then(|c| c.flows.iter_mut().find(|f| f.id == flow_id))
        })
        .ok_or_else(|| invalid("flow not found"))?;
    if f.kind != FlowKind::Sequence {
        return Err(invalid("condition requires sequence flow"));
    }
    if f.condition
        .as_ref()
        .is_some_and(|value| value.plain_text().is_none())
        && f.condition != condition
    {
        return Err(invalid("mixed expression requires an explicit replacement"));
    }
    f.condition = condition;
    Ok(())
}
pub(super) fn replace_flow_condition(
    p: &mut ProjectData,
    document_id: DocumentId,
    flow_id: ElementSelector,
    condition: Option<crate::bpmn_model::Expression>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let f = flow_mut(d, &flow_id)?;
    if f.kind != FlowKind::Sequence {
        return Err(invalid("condition requires sequence flow"));
    }
    f.condition = condition;
    Ok(())
}
pub(super) fn set_node_owner_contact(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    owner: String,
    contact: String,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let n = node_mut(d, &element_id)?;
    n.owner = owner;
    n.contact = contact;
    Ok(())
}
