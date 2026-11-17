//! Data object, data store and data association mutations.
use super::*;
pub(super) fn update_data_store(
    p: &mut ProjectData,
    document_id: DocumentId,
    data_store: crate::bpmn_data::DataStore,
) -> Result<(), CommandError> {
    if data_store.id.is_none() {
        return Err(invalid("Use SetDataStores for a store without an ID"));
    }
    let d = doc_structural(p, document_id)?;
    let index = d
        .data_stores
        .iter()
        .position(|v| v.id == data_store.id)
        .ok_or_else(|| invalid("data store not found"))?;
    d.data_stores[index] = data_store;
    Ok(())
}
pub(super) fn delete_data_store(
    p: &mut ProjectData,
    document_id: DocumentId,
    data_store_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let index = d
        .data_stores
        .iter()
        .position(|v| v.id.as_ref().is_some_and(|id| id == &data_store_id))
        .ok_or_else(|| invalid("data store not found"))?;
    d.data_stores.remove(index);
    Ok(())
}
pub(super) fn create_data_object(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    data_object: crate::bpmn_data::DataObject,
) -> Result<(), CommandError> {
    data_parts_mut(doc_structural(p, document_id)?, &scope_id)?
        .0
        .push(data_object);
    Ok(())
}
pub(super) fn update_data_object(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    data_object: crate::bpmn_data::DataObject,
) -> Result<(), CommandError> {
    if data_object.id.is_none() {
        return Err(invalid(
            "Use SetScopeDataObjects for an object without an ID",
        ));
    }
    let items = data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.0;
    let index = items
        .iter()
        .position(|v| v.id == data_object.id)
        .ok_or_else(|| invalid("data object not found"))?;
    items[index] = data_object;
    Ok(())
}
pub(super) fn delete_data_object(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    data_object_id: ElementSelector,
) -> Result<(), CommandError> {
    let items = data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.0;
    let index = items
        .iter()
        .position(|v| v.id.as_ref().is_some_and(|id| id == &data_object_id))
        .ok_or_else(|| invalid("data object not found"))?;
    items.remove(index);
    Ok(())
}
pub(super) fn create_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    association: crate::bpmn_data::DataAssociation,
) -> Result<(), CommandError> {
    data_parts_mut(doc_structural(p, document_id)?, &scope_id)?
        .1
        .push(association);
    Ok(())
}
pub(super) fn update_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    association: crate::bpmn_data::DataAssociation,
) -> Result<(), CommandError> {
    if association.id.is_none() {
        return Err(invalid(
            "Use SetScopeDataAssociations for an association without an ID",
        ));
    }
    let items = data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.1;
    let index = items
        .iter()
        .position(|v| v.id == association.id)
        .ok_or_else(|| invalid("data association not found"))?;
    items[index] = association;
    Ok(())
}
pub(super) fn delete_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    scope_id: ElementSelector,
    association_id: ElementSelector,
) -> Result<(), CommandError> {
    let items = data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.1;
    let index = items
        .iter()
        .position(|v| v.id.as_ref().is_some_and(|id| id == &association_id))
        .ok_or_else(|| invalid("data association not found"))?;
    items.remove(index);
    Ok(())
}
