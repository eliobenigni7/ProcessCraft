//! Definitions-level mutations: metadata, extensions, catalogs, categories and root event definitions.
use super::*;
pub(super) fn set_definitions_relationships(
    p: &mut ProjectData,
    document_id: DocumentId,
    relationships: Vec<DefinitionsRelationship>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.as_str())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then_some(GENERATED_TARGET_NAMESPACE)
        });
    if let Some(namespace) = namespace {
        let ids: std::collections::HashSet<_> = document
            .element_identities()
            .into_iter()
            .filter_map(|id| id.xml_id())
            .collect();
        let previous: std::collections::HashSet<_> = document
            .relationships
            .iter()
            .flat_map(|r| r.sources.iter().chain(&r.targets))
            .filter(|reference| reference.namespace == namespace && !ids.contains(&reference.local))
            .collect();
        if relationships
            .iter()
            .flat_map(|r| r.sources.iter().chain(&r.targets))
            .any(|reference| {
                reference.namespace == namespace
                    && !ids.contains(&reference.local)
                    && !previous.contains(reference)
            })
        {
            return Err(invalid("new local relationship reference does not resolve"));
        }
    } else if relationships
        .iter()
        .flat_map(|r| r.sources.iter().chain(&r.targets))
        .any(|reference| {
            !document
                .relationships
                .iter()
                .flat_map(|r| r.sources.iter().chain(&r.targets))
                .any(|old| old == reference)
        })
    {
        return Err(invalid(
            "cannot determine namespace for new relationship references",
        ));
    }
    document.relationships = relationships;
    Ok(())
}
pub(super) fn set_base_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    metadata: Option<crate::bpmn_metadata::BaseMetadata>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let mut found = false;
    crate::metadata_visit::visit_base_metadata_mut(d, |id, slot| {
        if id.is_some_and(|id| id.selector() == element_id) {
            *slot = metadata.clone();
            found = true;
        }
    });
    if !found {
        return Err(invalid("element does not support common base metadata"));
    }
    Ok(())
}
pub(super) fn set_base_extras(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    extras: Option<crate::bpmn_metadata::BaseExtras>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let mut found = false;
    crate::metadata_visit::visit_legacy_metadata_mut(d, |id, _, slot| {
        if id.selector() == element_id {
            *slot = extras.clone();
            found = true;
        }
    });
    if !found {
        return Err(invalid("element does not support base extras"));
    }
    Ok(())
}
pub(super) fn set_event_resources(
    p: &mut ProjectData,
    document_id: DocumentId,
    resources: Vec<crate::bpmn_model::EventResource>,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    for scope in crate::queries::scopes(document) {
        for node in scope.nodes {
            if let Some(reference) = node
                .activity
                .as_ref()
                .and_then(|activity| activity.message_ref.as_ref())
                && local_message_exists(document, reference, &document.event_resources)
                && !local_message_exists(document, reference, &resources)
            {
                return Err(invalid(
                    "Event resource change would remove a referenced local message",
                ));
            }
        }
    }
    document.event_resources = resources;
    Ok(())
}
pub(super) fn set_supporting_catalog(
    p: &mut ProjectData,
    document_id: DocumentId,
    catalog: crate::bpmn_catalog::SupportingCatalog,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    for scope in crate::queries::scopes(d) {
        for node in scope.nodes {
            if let Some(reference) = node
                .activity
                .as_ref()
                .and_then(|a| a.operation_ref.as_ref())
                && local_operation_exists(d, reference, &d.catalog)
                && !local_operation_exists(d, reference, &catalog)
            {
                return Err(invalid(
                    "Catalog change would remove a referenced local operation",
                ));
            }
        }
    }
    d.catalog = catalog;
    Ok(())
}
pub(super) fn update_category(
    p: &mut ProjectData,
    document_id: DocumentId,
    category: crate::bpmn_model::Category,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let index = d
        .categories
        .iter()
        .position(|v| v.id == category.id)
        .ok_or_else(|| invalid("category not found"))?;
    d.categories[index] = category;
    Ok(())
}
pub(super) fn delete_category(
    p: &mut ProjectData,
    document_id: DocumentId,
    category_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let index = d
        .categories
        .iter()
        .position(|v| v.id == category_id)
        .ok_or_else(|| invalid("category not found"))?;
    d.categories.remove(index);
    Ok(())
}
pub(super) fn document_properties(
    p: &mut ProjectData,
    document_id: DocumentId,
    name: String,
    folder: Option<XmlId>,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    if matches!(d.capability, Capability::RetainedExtensions { .. }) && d.folder != folder {
        return Err(CommandError::CapabilityDenied);
    }
    d.name = name;
    d.folder = folder;
    Ok(())
}
pub(super) fn set_definitions_metadata(
    p: &mut ProjectData,
    document_id: DocumentId,
    metadata: DefinitionsMetadata,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    if d.definitions.is_none() && d.provenance.is_some() {
        return Err(invalid(
            "Original target namespace must be recovered before editing definitions metadata",
        ));
    }
    let namespace_changed = d
        .definitions
        .as_ref()
        .map(|old| old.target_namespace.as_str())
        .unwrap_or(GENERATED_TARGET_NAMESPACE)
        != metadata.target_namespace.as_str();
    if namespace_changed
        && (d.catalog.has_qname_refs()
            || d.has_data_qname_refs()
            || d.has_event_qname_refs()
            || d.has_activity_qname_refs()
            || d.has_di_choreography_qname_refs()
            || d.has_collaboration_qname_refs()
            || d.extensions.iter().any(|e| e.definition.is_some())
            || d.relationships
                .iter()
                .any(|r| !r.sources.is_empty() || !r.targets.is_empty())
            || d.processes
                .iter()
                .any(|p| has_qualified_references(&p.nodes))
            || d.expressions().iter().any(|expression| {
                matches!(
                    expression,
                    crate::bpmn_model::Expression::Structured(crate::bpmn_model::ExpressionData {
                        kind: crate::bpmn_model::ExpressionKind::Formal {
                            evaluates_to_type_ref: Some(_),
                            ..
                        },
                        ..
                    })
                )
            }))
    {
        return Err(invalid(
            "Cannot change an unknown or referenced target namespace",
        ));
    }
    d.definitions = Some(metadata);
    Ok(())
}
