//! Diagram interchange mutations (shapes, edges, labels, planes) and their helpers.
use super::*;
pub(super) fn set_diagram_plane_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    metadata: Option<DiElementMetadata>,
) -> Result<(), CommandError> {
    diagram(doc_structural(p, document_id)?, &diagram_id)?.plane_metadata = metadata;
    Ok(())
}
pub(super) fn set_diagram_plane_order(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    order: Option<Vec<ElementSelector>>,
) -> Result<(), CommandError> {
    diagram(doc_structural(p, document_id)?, &diagram_id)?.plane_order = order;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn set_shape_representation_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
    metadata: Option<DiElementMetadata>,
    is_message_visible: Option<bool>,
    participant_band_kind: Option<ParticipantBandKind>,
    choreography_activity_shape: Option<DiReference>,
) -> Result<(), CommandError> {
    let shape = diagram(doc_structural(p, document_id)?, &diagram_id)?
        .shapes
        .iter_mut()
        .find(|shape| shape.id == shape_id)
        .ok_or_else(|| invalid("shape not found"))?;
    shape.di_metadata = metadata;
    shape.is_message_visible = is_message_visible;
    shape.participant_band_kind = participant_band_kind;
    shape.choreography_activity_shape = choreography_activity_shape;
    Ok(())
}
pub(super) fn set_edge_representation_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    edge_id: ElementSelector,
    metadata: Option<DiElementMetadata>,
    message_visible_kind: Option<MessageVisibleKind>,
) -> Result<(), CommandError> {
    let edge = diagram(doc_structural(p, document_id)?, &diagram_id)?
        .edges
        .iter_mut()
        .find(|edge| edge.id == edge_id)
        .ok_or_else(|| invalid("edge not found"))?;
    edge.di_metadata = metadata;
    edge.message_visible_kind = message_visible_kind;
    Ok(())
}
pub(super) fn set_diagram_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    documentation: Option<String>,
    resolution: Option<f64>,
) -> Result<(), CommandError> {
    let diagram = diagram(doc_nonstructural(p, document_id)?, &diagram_id)?;
    diagram.documentation = documentation;
    diagram.resolution = resolution;
    Ok(())
}
pub(super) fn set_diagram_label_styles(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    label_styles: Vec<LabelStyle>,
) -> Result<(), CommandError> {
    diagram(doc_structural(p, document_id)?, &diagram_id)?.label_styles = label_styles;
    Ok(())
}
pub(super) fn set_shape_label(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
    label: Option<Label>,
    label_visibility: Option<DiLabelVisibility>,
) -> Result<(), CommandError> {
    let shape = diagram(doc_structural(p, document_id)?, &diagram_id)?
        .shapes
        .iter_mut()
        .find(|shape| shape.id == shape_id)
        .ok_or_else(|| invalid("shape not found"))?;
    shape.label = label;
    shape.label_visibility = label_visibility;
    Ok(())
}
pub(super) fn set_edge_label(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    edge_id: ElementSelector,
    label: Option<Label>,
    label_visibility: Option<DiLabelVisibility>,
) -> Result<(), CommandError> {
    let edge = diagram(doc_structural(p, document_id)?, &diagram_id)?
        .edges
        .iter_mut()
        .find(|edge| edge.id == edge_id)
        .ok_or_else(|| invalid("edge not found"))?;
    edge.label = label;
    edge.label_visibility = label_visibility;
    Ok(())
}
pub(super) fn set_shape_di_attributes(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
    is_horizontal: Option<bool>,
    is_marker_visible: Option<bool>,
) -> Result<(), CommandError> {
    let shape = diagram(doc_nonstructural(p, document_id)?, &diagram_id)?
        .shapes
        .iter_mut()
        .find(|shape| shape.id == shape_id)
        .ok_or_else(|| invalid("shape not found"))?;
    shape.is_horizontal = is_horizontal;
    shape.is_marker_visible = is_marker_visible;
    Ok(())
}
pub(super) fn set_edge_di_endpoints(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    edge_id: ElementSelector,
    source_element: Option<ElementSelector>,
    target_element: Option<ElementSelector>,
) -> Result<(), CommandError> {
    let edge = diagram(doc_nonstructural(p, document_id)?, &diagram_id)?
        .edges
        .iter_mut()
        .find(|edge| edge.id == edge_id)
        .ok_or_else(|| invalid("edge not found"))?;
    edge.source_element = source_element;
    edge.target_element = target_element;
    Ok(())
}
pub(super) fn set_diagram_name(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    name: Option<String>,
) -> Result<(), CommandError> {
    diagram(doc_nonstructural(p, document_id)?, &diagram_id)?.name = name;
    Ok(())
}
pub(super) fn set_label_bounds(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    representation_id: ElementSelector,
    bounds: Bounds,
) -> Result<(), CommandError> {
    let diagram = diagram(doc_structural(p, document_id)?, &diagram_id)?;
    let label = if let Some(shape) = diagram
        .shapes
        .iter_mut()
        .find(|s| s.id == representation_id)
    {
        shape.label.as_mut()
    } else {
        diagram
            .edges
            .iter_mut()
            .find(|e| e.id == representation_id)
            .and_then(|e| e.label.as_mut())
    };
    label
        .ok_or_else(|| invalid("representation label not found"))?
        .bounds = Some(bounds);
    Ok(())
}
pub(super) fn set_diagram_plane(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    plane: ElementSelector,
) -> Result<(), CommandError> {
    diagram(doc_structural(p, document_id)?, &diagram_id)?.plane = plane;
    Ok(())
}
pub(super) fn diagram<'a>(
    d: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Diagram, CommandError> {
    d.diagrams
        .iter_mut()
        .find(|p| &p.id == id)
        .ok_or_else(|| invalid("diagram not found"))
}
pub(super) fn remove_di_shapes(
    diagram: &mut Diagram,
    owners: &std::collections::HashSet<ElementSelector>,
    namespace: Option<&str>,
) {
    let owner_xml_ids: std::collections::HashSet<_> = diagram
        .shapes
        .iter()
        .filter(|shape| owners.contains(&shape.id.selector()))
        .filter_map(|shape| shape.id.xml_id().cloned())
        .collect();
    diagram.shapes.retain(|shape| {
        if owners.contains(&shape.id.selector()) {
            return false;
        }
        match &shape.choreography_activity_shape {
            Some(DiReference::Local(id)) => !owners.contains(id),
            Some(DiReference::Qualified(qname)) => {
                !(namespace == Some(qname.namespace.as_str())
                    && owner_xml_ids.contains(&qname.local))
            }
            None => true,
        }
    });
}
/// Auto-resize the containers of `elements` (and then their containers) after a create, move,
/// or expand (see `containers::auto_resize_step`); lane membership follows the grown pools.
pub(super) fn auto_resize(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: &ElementSelector,
    mut elements: Vec<ElementSelector>,
    old: &BpmnDocument,
) -> Result<(), CommandError> {
    let mut resized = false;
    for _ in 0..32 {
        let steps = crate::containers::auto_resize_step(
            doc_nonstructural(p, document_id)?,
            diagram_id,
            &elements,
        );
        if steps.is_empty() {
            break;
        }
        elements.clear();
        for (element, shape_id, bounds) in steps {
            let batch = crate::manipulate::resize(
                doc_nonstructural(p, document_id)?,
                diagram_id,
                &shape_id,
                bounds,
            )
            .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
            elements.push(element);
            resized = true;
        }
    }
    let d = doc_nonstructural(p, document_id)?;
    if resized && d.capability.allows_structural_editing() && !d.blocks_structural_edits() {
        crate::containers::sync_lanes(old, d, diagram_id);
    }
    Ok(())
}
