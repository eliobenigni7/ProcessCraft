//! Post-batch checks: reject new dangling references or invalid graphs and reconcile orders.
use super::*;
pub(super) fn counts<'a, T: Eq + std::hash::Hash + 'a>(
    items: impl IntoIterator<Item = &'a T>,
) -> std::collections::HashMap<&'a T, usize> {
    let mut counts = std::collections::HashMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}
/// True when some value occurs more often in `next` than in `previous`.
pub(super) fn introduces_new<T: Eq + std::hash::Hash>(previous: &[T], next: &[T]) -> bool {
    let before = counts(previous);
    counts(next)
        .into_iter()
        .any(|(item, count)| count > before.get(item).copied().unwrap_or(0))
}
pub(super) fn reject_new_choreography_graph_issues(
    old: &ProjectData,
    candidate: &ProjectData,
    mappings: &[SelectorMapping],
) -> Result<(), CommandError> {
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|d| d.id == after.id) else {
            continue;
        };
        // Both scans only visit choreography scopes.
        if Arc::ptr_eq(before, after)
            || (before.choreographies.is_empty() && after.choreographies.is_empty())
        {
            continue;
        }
        let selected = |d: &BpmnDocument| {
            crate::validation::choreography_graph_diagnostics(d)
                .into_iter()
                .filter(|(_, code, _)| {
                    matches!(
                        *code,
                        "mixedChoreographyAuthority"
                            | "choreographySequenceEndpointWrongScope"
                            | "choreographySequenceEndpointNotIdref"
                            | "choreographyFlowWrongType"
                            | "choreographyMessageFlowWrongOwner"
                    )
                })
                .map(|(owner, code, _)| (owner, code))
                .collect::<Vec<_>>()
        };
        let mut previous = selected(before);
        for (owner, _) in &mut previous {
            for mapping in mappings.iter().filter(|m| m.document_id == after.id) {
                if *owner == mapping.from {
                    *owner = mapping.to.clone();
                }
            }
        }
        let next = selected(after);
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an invalid choreography graph reference or kind",
            ));
        }
        let mut previous = crate::validation::choreography_reference_issues(before);
        for issue in &mut previous {
            for mapping in mappings.iter().filter(|m| m.document_id == after.id) {
                if issue.owner == mapping.from {
                    issue.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) = &mut issue.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
            }
        }
        let next = crate::validation::choreography_reference_issues(after);
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an invalid choreography reference",
            ));
        }
    }
    Ok(())
}
pub(super) type NativeXmlIds =
    std::collections::HashMap<(DocumentId, crate::ids::NativeElementId), Option<XmlId>>;
pub(super) fn native_xml_ids(project: &ProjectData) -> NativeXmlIds {
    project
        .documents
        .iter()
        .flat_map(|document| {
            document
                .element_identities()
                .into_iter()
                .filter_map(|id| match id {
                    crate::ids::ElementIdentity::Native { native_id, xml_id } => {
                        Some(((document.id, *native_id), xml_id.clone()))
                    }
                    _ => None,
                })
        })
        .collect()
}
pub(super) fn reject_implicit_native_xml_id_changes(
    before: &NativeXmlIds,
    after: &ProjectData,
) -> Result<(), CommandError> {
    for new_doc in &after.documents {
        if new_doc.element_identities().into_iter().any(|id| match id {
            crate::ids::ElementIdentity::Native { native_id, xml_id } => before
                .get(&(new_doc.id, *native_id))
                .is_some_and(|previous| previous != xml_id),
            _ => false,
        }) {
            return Err(invalid(
                "changing an existing native element XML ID requires SetElementXmlId",
            ));
        }
    }
    Ok(())
}
/// Activity-reference scan of a document, memoized by `Arc` identity: the
/// previous commit's "after" scan is the next commit's "before" scan.
pub(super) type ActivityScans = Vec<(
    Arc<BpmnDocument>,
    Arc<(
        Vec<crate::validation::ActivityReferenceIssue>,
        Vec<crate::validation::ActivityReferenceResolution>,
    )>,
)>;
/// Foreign attributes (e.g. `camunda:*`) are kept verbatim and may name other elements by ID.
/// Structural edits are allowed around them, but a batch may not remove an element that such
/// an attribute names, since the retained value would then dangle.
pub(super) fn reject_removed_foreign_attribute_targets(
    old: &ProjectData,
    candidate: &ProjectData,
) -> Result<(), CommandError> {
    for before in &old.documents {
        let Some(after) = candidate.documents.iter().find(|d| d.id == before.id) else {
            continue;
        };
        if Arc::ptr_eq(before, after) {
            continue;
        }
        let values = before.foreign_attribute_values();
        if values.is_empty() {
            continue;
        }
        let xml_ids = |doc: &BpmnDocument| -> std::collections::HashSet<String> {
            doc.element_identities()
                .iter()
                .filter_map(|id| id.xml_id().map(|x| x.to_string()))
                .collect()
        };
        let (existing, remaining) = (xml_ids(before), xml_ids(after));
        for token in values
            .iter()
            .flat_map(|v| v.split(|c: char| c.is_whitespace() || c == ','))
            .map(|t| t.rsplit(':').next().unwrap_or(t))
        {
            if existing.contains(token) && !remaining.contains(token) {
                return Err(invalid(&format!(
                    "{token} is referenced by an extension attribute; edit or remove that attribute first"
                )));
            }
        }
    }
    Ok(())
}
pub(super) fn reject_new_activity_reference_issues(
    old: &ProjectData,
    candidate: &ProjectData,
    mappings: &[SelectorMapping],
    scans: &mut ActivityScans,
) -> Result<(), CommandError> {
    let mut fresh: ActivityScans = Vec::new();
    let mut scan = |doc: &Arc<BpmnDocument>| {
        let result = scans
            .iter()
            .chain(&fresh)
            .find(|(cached, _)| Arc::ptr_eq(cached, doc))
            .map(|(_, result)| result.clone())
            .unwrap_or_else(|| Arc::new(crate::validation::activity_family_reference_scan(doc)));
        fresh.push((doc.clone(), result.clone()));
        result
    };
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|doc| doc.id == after.id) else {
            // A document created in this batch has no earlier state: every
            // unresolved local activity reference is new.
            if !crate::validation::activity_family_reference_issues(after).is_empty() {
                return Err(invalid(
                    "change introduces an unresolved local activity reference",
                ));
            }
            continue;
        };
        if Arc::ptr_eq(before, after) {
            continue;
        }
        let (before_scan, after_scan) = (scan(before), scan(after));
        // Unchanged references and no identity renames (e.g. a label edit): nothing
        // new can be unresolved or retargeted.
        if before_scan == after_scan
            && !mappings
                .iter()
                .any(|mapping| mapping.document_id == after.id)
        {
            continue;
        }
        let (mut previous, mut old_resolutions) = (*before_scan).clone();
        let (next, new_resolutions) = (*after_scan).clone();
        for issue in &mut previous {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == after.id)
            {
                if issue.owner == mapping.from {
                    issue.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) = &mut issue.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
            }
        }
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an unresolved local activity reference",
            ));
        }
        for resolution in &mut old_resolutions {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == after.id)
            {
                if resolution.reference.owner == mapping.from {
                    resolution.reference.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) =
                    &mut resolution.reference.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
                if resolution.resolved_target == mapping.from {
                    resolution.resolved_target = mapping.to.clone();
                }
            }
        }
        let old_references = counts(old_resolutions.iter().map(|r| &r.reference));
        let new_references = counts(new_resolutions.iter().map(|r| &r.reference));
        let old_targets = counts(old_resolutions.iter());
        let new_targets = counts(new_resolutions.iter());
        for old_resolution in &old_resolutions {
            let old_count = old_references[&old_resolution.reference];
            let new_count = new_references
                .get(&old_resolution.reference)
                .copied()
                .unwrap_or(0);
            if new_count >= old_count
                && new_count > 0
                && old_targets[old_resolution]
                    > new_targets.get(old_resolution).copied().unwrap_or(0)
            {
                return Err(invalid("change would retarget a local activity reference"));
            }
        }
    }
    // Keep one scan per candidate document; the clones keep their `Arc`s alive.
    let mut kept: ActivityScans = Vec::new();
    for (doc, result) in fresh.into_iter().chain(std::mem::take(scans)) {
        if candidate.documents.iter().any(|d| Arc::ptr_eq(d, &doc))
            && !kept.iter().any(|(k, _)| Arc::ptr_eq(k, &doc))
        {
            kept.push((doc, result));
        }
    }
    *scans = kept;
    Ok(())
}
/// Drops every project-level reference to removed elements of one document:
/// navigation links, dictionary links and attribute values. The single helper
/// for all delete paths; `removed: None` removes every reference to the
/// document (`deleteDocument`).
pub(super) fn prune_removed_element_refs(
    project: &mut ProjectData,
    document_id: DocumentId,
    removed: Option<&std::collections::HashSet<ElementSelector>>,
) {
    let keep = |source: &ElementKey| match removed {
        None => source.document_id != document_id,
        Some(set) => source.document_id != document_id || !set.contains(&source.xml_id),
    };
    project.links.retain(|link| keep(&link.source));
    project.dictionary_links.retain(|link| keep(&link.source));
    project.attribute_values.retain(|value| keep(&value.source));
}
pub(super) fn prune_removed_activity_navigation(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
) {
    for before in &old.documents {
        let Some(after) = candidate
            .documents
            .iter_mut()
            .find(|doc| doc.id == before.id)
        else {
            continue;
        };
        let before_ids = crate::identity::semantic_ids(before);
        let after_ids = crate::identity::semantic_ids(after);
        let removed: std::collections::HashSet<_> = before_ids
            .difference(&after_ids)
            .filter(|id| {
                !mappings
                    .iter()
                    .any(|mapping| mapping.document_id == before.id && &mapping.from == *id)
            })
            .cloned()
            .collect();
        if removed.is_empty() {
            continue;
        }
        let document = Arc::make_mut(after);
        for diagram in &mut document.diagrams {
            diagram
                .shapes
                .retain(|shape| !removed.contains(&shape.element));
            diagram
                .edges
                .retain(|edge| !removed.contains(&edge.element));
        }
        prune_removed_element_refs(candidate, before.id, Some(&removed));
    }
}
pub(super) fn reconcile_root_orders(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
    explicit: &[DocumentId],
) -> Result<(), CommandError> {
    for document in &mut candidate.documents {
        if explicit.contains(&document.id)
            || document.root_order.is_none()
            || !old
                .documents
                .iter()
                .any(|before| before.id == document.id && before.root_order.is_some())
        {
            continue;
        }
        let available: Vec<_> = document
            .root_element_identities()
            .into_iter()
            .map(|identity| {
                identity.map(|id| id.selector()).ok_or_else(|| {
                    invalid("promote anonymous roots before adding them to an ordered document")
                })
            })
            .collect::<Result<_, _>>()?;
        let mut order = document.root_order.clone().unwrap_or_default();
        for id in &mut order {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == document.id)
            {
                if *id == mapping.from {
                    *id = mapping.to.clone();
                }
            }
        }
        order.retain(|id| available.contains(id));
        for id in available {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        if document.root_order.as_ref() != Some(&order) {
            Arc::make_mut(document).root_order = Some(order);
        }
    }
    Ok(())
}
pub(super) fn reconcile_plane_orders(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
    explicit: &[(DocumentId, ElementSelector)],
) {
    let remap = |document_id, selector: &ElementSelector| {
        mappings
            .iter()
            .filter(|mapping| mapping.document_id == document_id)
            .fold(selector.clone(), |current, mapping| {
                if current == mapping.from {
                    mapping.to.clone()
                } else {
                    current
                }
            })
    };
    for document in &mut candidate.documents {
        let Some(previous) = old.documents.iter().find(|old| old.id == document.id) else {
            continue;
        };
        let document_id = document.id;
        let is_explicit = |diagram: &Diagram| {
            explicit.iter().any(|(id, selector)| {
                *id == document_id && remap(document_id, selector) == diagram.id.selector()
            })
        };
        let needs_update = document.diagrams.iter().any(|diagram| {
            if is_explicit(diagram) {
                return false;
            }
            let old_diagram = previous
                .diagrams
                .iter()
                .find(|old| remap(document_id, &old.id.selector()) == diagram.id.selector());
            let Some(order) = diagram
                .plane_order
                .as_ref()
                .filter(|_| old_diagram.is_some_and(|old| old.plane_order.is_some()))
            else {
                return false;
            };
            let available: Vec<_> = diagram
                .shapes
                .iter()
                .map(|shape| shape.id.selector())
                .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
                .collect();
            order.iter().any(|id| !available.contains(id))
                || available.iter().any(|id| !order.contains(id))
        });
        if !needs_update {
            continue;
        }
        let document = Arc::make_mut(document);
        for diagram in &mut document.diagrams {
            if is_explicit(diagram) {
                continue;
            }
            let old_diagram = previous
                .diagrams
                .iter()
                .find(|old| remap(document_id, &old.id.selector()) == diagram.id.selector());
            if !old_diagram.is_some_and(|old| old.plane_order.is_some()) {
                continue;
            }
            let Some(order) = &mut diagram.plane_order else {
                continue;
            };
            let available: Vec<_> = diagram
                .shapes
                .iter()
                .map(|shape| shape.id.selector())
                .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
                .collect();
            order.retain(|id| available.contains(id));
            for id in available {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
        }
    }
}
pub(super) fn reject_newly_dangling_relationships(
    old: &ProjectData,
    candidate: &ProjectData,
) -> Result<(), CommandError> {
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|doc| doc.id == after.id) else {
            continue;
        };
        let namespace = before
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.as_str())
            .or_else(|| {
                before
                    .provenance
                    .is_none()
                    .then_some(GENERATED_TARGET_NAMESPACE)
            });
        let after_namespace = after
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.as_str())
            .or_else(|| {
                after
                    .provenance
                    .is_none()
                    .then_some(GENERATED_TARGET_NAMESPACE)
            });
        if namespace.is_none() || namespace != after_namespace {
            continue;
        }
        let old_ids: std::collections::HashMap<_, _> = before
            .element_identities()
            .into_iter()
            .filter_map(|id| id.xml_id().map(|xml| (xml, id.selector())))
            .collect();
        let new_ids: std::collections::HashMap<_, _> = after
            .element_identities()
            .into_iter()
            .filter_map(|id| id.xml_id().map(|xml| (xml, id.selector())))
            .collect();
        if after
            .relationships
            .iter()
            .flat_map(|relationship| relationship.sources.iter().chain(&relationship.targets))
            .any(|reference| {
                Some(reference.namespace.as_str()) == namespace
                    && old_ids
                        .get(&reference.local)
                        .is_some_and(|old| new_ids.get(&reference.local) != Some(old))
            })
        {
            return Err(invalid(
                "change would leave a local relationship reference dangling or retargeted",
            ));
        }
    }
    Ok(())
}

/// True for mutations that can remove or retarget what an activity navigation link names.
pub(super) fn cleans_activity_navigation(mutation: &Mutation) -> bool {
    matches!(
        mutation,
        Mutation::SetGlobalTasks { .. }
            | Mutation::SetSupportingCatalog { .. }
            | Mutation::SetDataStores { .. }
            | Mutation::SetRootEventDefinitions { .. }
            | Mutation::SetEventResources { .. }
            | Mutation::UpdateCategory { .. }
            | Mutation::DeleteCategory { .. }
            | Mutation::Delete { .. }
            | Mutation::SetCollaborationRoot { .. }
            | Mutation::DeleteCollaborationRoot { .. }
            | Mutation::SetPartnerCatalog { .. }
            | Mutation::DeleteChoreographyActivity { .. }
            | Mutation::DeleteChoreographyFlowElement { .. }
            | Mutation::UpdateChoreographyFlowElement { .. }
            | Mutation::DeleteChoreographyArtifact { .. }
            | Mutation::UpdateChoreographyArtifact { .. }
            | Mutation::DeleteChoreographyDataAssociation { .. }
            | Mutation::UpdateChoreographyDataAssociation { .. }
            | Mutation::SetParticipantMultiplicity { .. }
            | Mutation::SetCorrelationProperties { .. }
            | Mutation::SetProcessActivityMetadata { .. }
            | Mutation::SetNodeFlowMetadata { .. }
            | Mutation::SetFlowMetadata { .. }
            | Mutation::SetCollaborationCorrelationKeys { .. }
            | Mutation::SetActivityProperties { .. }
            | Mutation::SetNodeLoop { .. }
            | Mutation::SetNodeDetails { .. }
            | Mutation::UpdateDataObject { .. }
            | Mutation::DeleteDataObject { .. }
            | Mutation::SetScopeDataObjects { .. }
            | Mutation::UpdateConversationNode { .. }
            | Mutation::UpdateChoreographyActivity { .. }
    )
}

/// Choreography scope operations: they renumber graph identities, so they need an expected
/// generation and a history entry that is guaranteed to fit.
fn is_choreography_scope_operation(mutation: &Mutation) -> bool {
    matches!(
        mutation,
        Mutation::PromoteChoreographyGraph { .. }
            | Mutation::CreateChoreographyFlowElement { .. }
            | Mutation::UpdateChoreographyFlowElement { .. }
            | Mutation::DeleteChoreographyFlowElement { .. }
            | Mutation::ReorderChoreographyFlowElement { .. }
            | Mutation::ReparentChoreographyFlowElement { .. }
            | Mutation::CreateChoreographyArtifact { .. }
            | Mutation::UpdateChoreographyArtifact { .. }
            | Mutation::DeleteChoreographyArtifact { .. }
            | Mutation::ReorderChoreographyArtifact { .. }
            | Mutation::ReparentChoreographyArtifact { .. }
            | Mutation::CreateChoreographyDataAssociation { .. }
            | Mutation::UpdateChoreographyDataAssociation { .. }
            | Mutation::DeleteChoreographyDataAssociation { .. }
            | Mutation::ReorderChoreographyDataAssociation { .. }
    ) || matches!(mutation, Mutation::CreateChoreography { choreography, .. } if choreography.graph.is_some())
}

/// Identity and choreography scope operations are only valid against a known generation.
pub(super) fn needs_expected_generation(mutation: &Mutation) -> bool {
    matches!(
        mutation,
        Mutation::PromoteElementIdentity { .. } | Mutation::InitializeDocumentRootOrder { .. }
    ) || is_choreography_scope_operation(mutation)
}

/// Mutations whose undo entry must fit the history budget instead of evicting older entries.
pub(super) fn requires_undo_entry(mutation: &Mutation) -> bool {
    matches!(
        mutation,
        Mutation::RestoreCheckpoint { .. } | Mutation::ApplyLayout { .. }
    ) || is_choreography_scope_operation(mutation)
}

/// Inputs of the post-batch passes besides the before/after projects.
pub(super) struct PostBatch<'a> {
    pub(super) native_baseline: Option<NativeXmlIds>,
    pub(super) mappings: &'a [SelectorMapping],
    pub(super) explicit_root_orders: &'a [DocumentId],
    pub(super) explicit_plane_orders: &'a [(DocumentId, ElementSelector)],
    pub(super) cleanup_activity_navigation: bool,
}

/// Every pass that runs over the candidate after the batch's mutations, in order: prune and
/// reconcile derived state, then reject newly introduced dangling or invalid content.
pub(super) fn post_batch_checks(
    old: &ProjectData,
    candidate: &mut ProjectData,
    pass: PostBatch<'_>,
    activity_scans: &mut ActivityScans,
) -> Result<(), CommandError> {
    if let Some(baseline) = pass.native_baseline {
        reject_implicit_native_xml_id_changes(&baseline, candidate)?;
    }
    if pass.cleanup_activity_navigation {
        prune_removed_activity_navigation(old, candidate, pass.mappings);
    }
    reconcile_root_orders(old, candidate, pass.mappings, pass.explicit_root_orders)?;
    reconcile_plane_orders(old, candidate, pass.mappings, pass.explicit_plane_orders);
    reject_newly_dangling_relationships(old, candidate)?;
    reject_removed_foreign_attribute_targets(old, candidate)?;
    reject_new_choreography_graph_issues(old, candidate, pass.mappings)?;
    reject_new_activity_reference_issues(old, candidate, pass.mappings, activity_scans)
}
