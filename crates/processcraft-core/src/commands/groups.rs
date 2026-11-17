//! Group mutations.
use super::*;
pub(super) fn update_group(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    group: crate::bpmn_model::Group,
) -> Result<(), CommandError> {
    let groups = groups_mut(doc_structural(p, document_id)?, &scope_id)?;
    let index = groups
        .iter()
        .position(|v| v.id == group.id)
        .ok_or_else(|| invalid("group not found"))?;
    groups[index] = group;
    Ok(())
}
pub(super) fn delete_group(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    group_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let groups = groups_mut(d, &scope_id)?;
    let index = groups
        .iter()
        .position(|v| v.id == group_id)
        .ok_or_else(|| invalid("group not found"))?;
    groups.remove(index);
    for diagram in &mut d.diagrams {
        diagram.shapes.retain(|s| s.element != group_id);
    }
    p.links
        .retain(|link| !(link.source.document_id == document_id && link.source.xml_id == group_id));
    Ok(())
}
