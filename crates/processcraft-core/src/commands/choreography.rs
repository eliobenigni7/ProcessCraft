//! Choreography mutations and the scope helpers they share.
use super::*;
pub(super) fn update_choreography(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography: crate::bpmn_collaboration::Choreography,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let index = d
        .choreographies
        .iter()
        .position(|c| c.id == choreography.id)
        .ok_or_else(|| invalid("choreography not found"))?;
    if d.choreographies[index].graph.is_some() || choreography.graph.is_some() {
        return Err(invalid("legacy choreography update cannot replace a graph"));
    }
    d.choreographies[index] = choreography;
    Ok(())
}
pub(super) fn delete_choreography(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let index = d
        .choreographies
        .iter()
        .position(|c| c.id == choreography_id)
        .ok_or_else(|| invalid("choreography not found"))?;
    if d.choreographies[index].graph.is_some() {
        return Err(invalid("legacy choreography delete cannot remove a graph"));
    }
    d.choreographies.remove(index);
    Ok(())
}
pub(super) fn promote_choreography_graph(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
) -> Result<(), CommandError> {
    use crate::bpmn_collaboration::{ChoreographyFlowElement, ChoreographyGraph};
    let c = choreography_mut(doc_structural(p, document_id)?, &choreography_id)?;
    if c.graph.is_some() {
        return Err(invalid("choreography already has a graph"));
    }
    let mut flow_elements = Vec::with_capacity(c.activities.len() + c.sequence_flows.len());
    flow_elements.extend(
        std::mem::take(&mut c.activities)
            .into_iter()
            .map(|a| ChoreographyFlowElement::Activity(Box::new(a))),
    );
    flow_elements.extend(
        std::mem::take(&mut c.sequence_flows)
            .into_iter()
            .map(|f| ChoreographyFlowElement::SequenceFlow(Box::new(f))),
    );
    c.graph = Some(ChoreographyGraph {
        flow_elements,
        data_associations: vec![],
    });
    Ok(())
}
pub(super) fn create_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    index: usize,
    element: crate::bpmn_collaboration::ChoreographyFlowElement,
) -> Result<(), CommandError> {
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    if index > graph.flow_elements.len() {
        return Err(invalid("choreography insertion index out of range"));
    }
    graph.flow_elements.insert(index, element);
    Ok(())
}
pub(super) fn update_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    element: crate::bpmn_collaboration::ChoreographyFlowElement,
) -> Result<(), CommandError> {
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    let id = choreography_element_id(&element);
    let old = graph
        .flow_elements
        .iter_mut()
        .find(|e| choreography_element_id(e) == id)
        .ok_or_else(|| invalid("choreography flow element not found"))?;
    if std::mem::discriminant(old) != std::mem::discriminant(&element) {
        return Err(invalid("flow element kind cannot be changed by update"));
    }
    // Structural subtree changes need their own scoped operations so descendants cannot disappear implicitly.
    if let (
        crate::bpmn_collaboration::ChoreographyFlowElement::Activity(a),
        crate::bpmn_collaboration::ChoreographyFlowElement::Activity(b),
    ) = (&*old, &element)
    {
        use crate::bpmn_collaboration::ChoreographyActivityDetails::Sub;
        let has_contents = |details: &Option<
            crate::bpmn_collaboration::ChoreographyActivityDetails,
        >| matches!(details, Some(Sub { graph, artifacts }) if !graph.flow_elements.is_empty() || !graph.data_associations.is_empty() || !artifacts.is_empty());
        if (has_contents(&a.details) || has_contents(&b.details))
            && serde_json::to_value(&a.details).ok() != serde_json::to_value(&b.details).ok()
        {
            return Err(invalid("sub-choreography structure requires scoped edits"));
        }
    }
    *old = element;
    Ok(())
}
pub(super) fn apply_delete_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    element_id: ElementSelector,
) -> Result<(), CommandError> {
    delete_choreography_flow_element(p, document_id, &choreography_id, &scope_id, &element_id)?;
    Ok(())
}
pub(super) fn reorder_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    element_id: ElementSelector,
    index: usize,
) -> Result<(), CommandError> {
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    if index >= graph.flow_elements.len() {
        return Err(invalid("choreography reorder index out of range"));
    }
    let old = graph
        .flow_elements
        .iter()
        .position(|e| choreography_element_id(e) == element_id)
        .ok_or_else(|| invalid("choreography flow element not found"))?;
    let element = graph.flow_elements.remove(old);
    graph.flow_elements.insert(index, element);
    Ok(())
}
pub(super) fn reparent_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    source_scope_id: ElementSelector,
    target_scope_id: ElementSelector,
    element_id: ElementSelector,
    index: usize,
) -> Result<(), CommandError> {
    if source_scope_id == target_scope_id {
        return Err(invalid("use reorder within one scope"));
    }
    let document = doc_structural(p, document_id)?;
    let c = choreography_mut(document, &choreography_id)?;
    let source = choreography_scope_mut(c, &source_scope_id)?;
    let old = source
        .flow_elements
        .iter()
        .position(|e| choreography_element_id(e) == element_id)
        .ok_or_else(|| invalid("choreography flow element not found"))?;
    let mut subtree = std::collections::HashSet::new();
    choreography_subtree_ids(&source.flow_elements[old], &mut subtree);
    if !choreography_reparent_geometry_safe(
        document,
        &choreography_id,
        &source_scope_id,
        &target_scope_id,
        &element_id,
        &subtree,
    ) {
        return Err(invalid(
            "reparenting represented choreography elements requires qualified geometry",
        ));
    }
    let c = choreography_mut(document, &choreography_id)?;
    let source = choreography_scope_mut(c, &source_scope_id)?;
    let element = source.flow_elements.remove(old);
    // Data associations are owned by their element and move with it.
    let (moved, kept) = std::mem::take(&mut source.data_associations)
        .into_iter()
        .partition(|a| a.owner == element_id);
    source.data_associations = kept;
    let target = choreography_scope_mut(c, &target_scope_id)?;
    if index > target.flow_elements.len() {
        return Err(invalid("choreography insertion index out of range"));
    }
    target.flow_elements.insert(index, element);
    target.data_associations.extend::<Vec<_>>(moved);
    Ok(())
}
pub(super) fn create_choreography_artifact(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    index: usize,
    artifact: crate::bpmn_collaboration::CollaborationArtifact,
) -> Result<(), CommandError> {
    let artifacts = choreography_artifacts_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
        true,
    )?;
    if index > artifacts.len() {
        return Err(invalid(
            "choreography artifact insertion index out of range",
        ));
    }
    artifacts.insert(index, artifact);
    Ok(())
}
pub(super) fn update_choreography_artifact(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    artifact: crate::bpmn_collaboration::CollaborationArtifact,
) -> Result<(), CommandError> {
    let artifacts = choreography_artifacts_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
        false,
    )?;
    let id = choreography_artifact_id(&artifact);
    let old = artifacts
        .iter_mut()
        .find(|a| choreography_artifact_id(a) == id)
        .ok_or_else(|| invalid("choreography artifact not found"))?;
    if std::mem::discriminant(old) != std::mem::discriminant(&artifact) {
        return Err(invalid("artifact kind cannot be changed by update"));
    }
    *old = artifact;
    Ok(())
}
pub(super) fn delete_choreography_artifact(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    artifact_id: ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let before: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let artifacts = choreography_artifacts_mut(
        choreography_mut(document, &choreography_id)?,
        &scope_id,
        false,
    )?;
    let index = artifacts
        .iter()
        .position(|a| choreography_artifact_id(a) == artifact_id)
        .ok_or_else(|| invalid("choreography artifact not found"))?;
    artifacts.remove(index);
    let after: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let removed: std::collections::HashSet<_> = before.difference(&after).cloned().collect();
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.clone())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then(|| GENERATED_TARGET_NAMESPACE.to_owned())
        });
    for diagram in &mut document.diagrams {
        let owners = diagram
            .shapes
            .iter()
            .filter(|shape| removed.contains(&shape.element))
            .map(|shape| shape.id.selector())
            .collect();
        remove_di_shapes(diagram, &owners, namespace.as_deref());
        diagram
            .edges
            .retain(|edge| !removed.contains(&edge.element));
    }
    prune_removed_element_refs(p, document_id, Some(&removed));
    Ok(())
}
pub(super) fn reorder_choreography_artifact(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    artifact_id: ElementSelector,
    index: usize,
) -> Result<(), CommandError> {
    let artifacts = choreography_artifacts_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
        false,
    )?;
    if index >= artifacts.len() {
        return Err(invalid("choreography artifact reorder index out of range"));
    }
    let old = artifacts
        .iter()
        .position(|a| choreography_artifact_id(a) == artifact_id)
        .ok_or_else(|| invalid("choreography artifact not found"))?;
    let artifact = artifacts.remove(old);
    artifacts.insert(index, artifact);
    Ok(())
}
pub(super) fn reparent_choreography_artifact(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    source_scope_id: ElementSelector,
    target_scope_id: ElementSelector,
    artifact_id: ElementSelector,
    index: usize,
) -> Result<(), CommandError> {
    if source_scope_id == target_scope_id {
        return Err(invalid("use reorder within one scope"));
    }
    let document = doc_structural(p, document_id)?;
    if !choreography_reparent_geometry_safe(
        document,
        &choreography_id,
        &source_scope_id,
        &target_scope_id,
        &artifact_id,
        &std::collections::HashSet::from([artifact_id.clone()]),
    ) {
        return Err(invalid(
            "reparenting represented choreography artifacts requires qualified geometry",
        ));
    }
    let c = choreography_mut(document, &choreography_id)?;
    let source = choreography_artifacts_mut(c, &source_scope_id, false)?;
    let old = source
        .iter()
        .position(|a| choreography_artifact_id(a) == artifact_id)
        .ok_or_else(|| invalid("choreography artifact not found"))?;
    let artifact = source.remove(old);
    let target = choreography_artifacts_mut(c, &target_scope_id, true)?;
    if index > target.len() {
        return Err(invalid(
            "choreography artifact insertion index out of range",
        ));
    }
    target.insert(index, artifact);
    Ok(())
}
pub(super) fn create_choreography_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    index: usize,
    association: crate::bpmn_data::DataAssociation,
) -> Result<(), CommandError> {
    if association.id.is_none() {
        return Err(invalid(
            "new choreography data association requires a stable identity",
        ));
    }
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    if index > graph.data_associations.len() {
        return Err(invalid("data association insertion index out of range"));
    }
    graph.data_associations.insert(index, association);
    Ok(())
}
pub(super) fn update_choreography_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    association: crate::bpmn_data::DataAssociation,
) -> Result<(), CommandError> {
    let id = association
        .id
        .as_ref()
        .ok_or_else(|| invalid("data association requires a stable identity"))?
        .selector();
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    let old = graph
        .data_associations
        .iter_mut()
        .find(|a| a.id.as_ref().is_some_and(|value| value.selector() == id))
        .ok_or_else(|| invalid("data association not found"))?;
    *old = association;
    Ok(())
}
pub(super) fn delete_choreography_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    association_id: ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let before: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let graph = choreography_scope_mut(choreography_mut(document, &choreography_id)?, &scope_id)?;
    let old = graph
        .data_associations
        .iter()
        .position(|a| {
            a.id.as_ref()
                .is_some_and(|value| value.selector() == association_id)
        })
        .ok_or_else(|| invalid("data association not found"))?;
    graph.data_associations.remove(old);
    let after: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let removed: std::collections::HashSet<_> = before.difference(&after).cloned().collect();
    for diagram in &mut document.diagrams {
        diagram
            .edges
            .retain(|edge| !removed.contains(&edge.element));
    }
    prune_removed_element_refs(p, document_id, Some(&removed));
    Ok(())
}
pub(super) fn reorder_choreography_data_association(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    scope_id: ElementSelector,
    association_id: ElementSelector,
    index: usize,
) -> Result<(), CommandError> {
    let graph = choreography_scope_mut(
        choreography_mut(doc_structural(p, document_id)?, &choreography_id)?,
        &scope_id,
    )?;
    if index >= graph.data_associations.len() {
        return Err(invalid("data association reorder index out of range"));
    }
    let old = graph
        .data_associations
        .iter()
        .position(|a| {
            a.id.as_ref()
                .is_some_and(|value| value.selector() == association_id)
        })
        .ok_or_else(|| invalid("data association not found"))?;
    let association = graph.data_associations.remove(old);
    graph.data_associations.insert(index, association);
    Ok(())
}
pub(super) fn create_choreography_activity(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    activity: crate::bpmn_collaboration::ChoreographyActivity,
) -> Result<(), CommandError> {
    let c = choreography_mut(doc_structural(p, document_id)?, &choreography_id)?;
    if c.graph.is_some() {
        return Err(invalid(
            "legacy activity command requires a legacy choreography",
        ));
    }
    c.activities.push(activity);
    Ok(())
}
pub(super) fn update_choreography_activity(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    activity: crate::bpmn_collaboration::ChoreographyActivity,
) -> Result<(), CommandError> {
    let c = choreography_mut(doc_structural(p, document_id)?, &choreography_id)?;
    if c.graph.is_some() {
        return Err(invalid(
            "legacy activity command requires a legacy choreography",
        ));
    }
    let index = c
        .activities
        .iter()
        .position(|a| a.id == activity.id)
        .ok_or_else(|| invalid("choreography activity not found"))?;
    c.activities[index] = activity;
    Ok(())
}
pub(super) fn delete_choreography_activity(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: ElementSelector,
    activity_id: ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let c = choreography_mut(document, &choreography_id)?;
    if c.graph.is_some() {
        return Err(invalid(
            "legacy activity command requires a legacy choreography",
        ));
    }
    let index = c
        .activities
        .iter()
        .position(|a| a.id == activity_id)
        .ok_or_else(|| invalid("choreography activity not found"))?;
    c.activities.remove(index);
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.clone())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then(|| GENERATED_TARGET_NAMESPACE.to_string())
        });
    for diagram in &mut document.diagrams {
        let owners = diagram
            .shapes
            .iter()
            .filter(|shape| shape.element == activity_id)
            .map(|shape| shape.id.selector())
            .collect();
        remove_di_shapes(diagram, &owners, namespace.as_deref());
    }
    Ok(())
}
pub(super) fn choreography_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut crate::bpmn_collaboration::Choreography, CommandError> {
    document
        .choreographies
        .iter_mut()
        .find(|c| &c.id == id)
        .ok_or_else(|| invalid("choreography not found"))
}

pub(super) fn choreography_element_id(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
) -> ElementSelector {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    match element {
        ChoreographyFlowElement::Activity(a) => a.id.selector(),
        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => n.id.selector(),
        ChoreographyFlowElement::SequenceFlow(f) => f.id.selector(),
    }
}
pub(super) fn choreography_artifact_id(
    artifact: &crate::bpmn_collaboration::CollaborationArtifact,
) -> ElementSelector {
    use crate::bpmn_collaboration::CollaborationArtifact;
    match artifact {
        CollaborationArtifact::Group(g) => g.id.selector(),
        CollaborationArtifact::TextAnnotation(n) => n.id.selector(),
        CollaborationArtifact::Association(f) => f.id.selector(),
    }
}
pub(super) fn choreography_artifacts_mut<'a>(
    c: &'a mut crate::bpmn_collaboration::Choreography,
    scope: &ElementSelector,
    create_root: bool,
) -> Result<&'a mut Vec<crate::bpmn_collaboration::CollaborationArtifact>, CommandError> {
    fn nested<'a>(
        graph: &'a mut crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
    ) -> Option<&'a mut Vec<crate::bpmn_collaboration::CollaborationArtifact>> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &mut graph.flow_elements {
            if let ChoreographyFlowElement::Activity(activity) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) =
                    &mut activity.details
            {
                if activity.id == *scope {
                    return Some(artifacts);
                }
                if let Some(found) = nested(graph, scope) {
                    return Some(found);
                }
            }
        }
        None
    }
    if c.id == *scope {
        if create_root {
            return Ok(&mut c.extras.get_or_insert_with(Default::default).artifacts);
        }
        return c
            .extras
            .as_mut()
            .map(|e| &mut e.artifacts)
            .ok_or_else(|| invalid("choreography artifact not found"));
    }
    nested(
        c.graph
            .as_mut()
            .ok_or_else(|| invalid("choreography scope not found"))?,
        scope,
    )
    .ok_or_else(|| invalid("choreography scope not found"))
}

pub(super) fn choreography_subtree_ids(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    use crate::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, CollaborationArtifact,
    };
    ids.insert(choreography_element_id(element));
    if let ChoreographyFlowElement::Activity(activity) = element
        && let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) = &activity.details
    {
        for child in &graph.flow_elements {
            choreography_subtree_ids(child, ids);
        }
        for association in &graph.data_associations {
            if let Some(id) = &association.id {
                ids.insert(id.selector());
            }
        }
        for artifact in artifacts {
            ids.insert(match artifact {
                CollaborationArtifact::Group(group) => group.id.selector(),
                CollaborationArtifact::TextAnnotation(node) => node.id.selector(),
                CollaborationArtifact::Association(flow) => flow.id.selector(),
            });
        }
    }
}

// A narrow geometry-preserving move into an expanded Sub is safe when every view proves a
// unique containing representation and no connected DI edge needs rerouting. All coordinates,
// labels and participant bands remain untouched.
pub(super) fn choreography_reparent_geometry_safe(
    document: &BpmnDocument,
    root: &ElementSelector,
    source: &ElementSelector,
    target: &ElementSelector,
    moved: &ElementSelector,
    subtree: &std::collections::HashSet<ElementSelector>,
) -> bool {
    let scopes = crate::queries::choreography_scopes(document);
    let scope_ids: std::collections::HashSet<_> = scopes
        .iter()
        .filter(|s| s.root.id == *root && s.owner != *root)
        .map(|s| s.owner.clone())
        .collect();
    let contains = |outer: &Bounds, inner: &Bounds| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.width <= outer.x + outer.width
            && inner.y + inner.height <= outer.y + outer.height
    };
    for diagram in &document.diagrams {
        let represented: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| subtree.contains(&s.element))
            .collect();
        if represented.is_empty() && !diagram.edges.iter().any(|e| subtree.contains(&e.element)) {
            continue;
        }
        if *source != *root
            || subtree.len() != 1
            || diagram.plane != *root
            || represented.len() != 1
            || represented[0].element != *moved
            || diagram.edges.iter().any(|e| subtree.contains(&e.element))
        {
            return false;
        }
        let shape = represented[0];
        let targets: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| s.element == *target)
            .collect();
        if targets.len() != 1
            || targets[0].is_expanded != Some(true)
            || !contains(&targets[0].bounds, &shape.bounds)
        {
            return false;
        }
        if diagram.shapes.iter().any(|s| {
            scope_ids.contains(&s.element)
                && s.element != *target
                && s.element != *moved
                && contains(&s.bounds, &shape.bounds)
        }) {
            return false;
        }
        if diagram.edges.iter().any(|e| {
            e.source_element.as_ref() == Some(&shape.id.selector())
                || e.target_element.as_ref() == Some(&shape.id.selector())
        }) {
            return false;
        }
        if diagram.shapes.iter().any(|band| matches!(&band.choreography_activity_shape, Some(DiReference::Local(owner)) if *owner == shape.id.selector()) && !contains(&targets[0].bounds, &band.bounds)) { return false; }
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
        if diagram.shapes.iter().any(|band| {
            match (&band.choreography_activity_shape, shape.id.xml_id()) {
                (Some(DiReference::Qualified(reference)), Some(id)) if &reference.local == id => {
                    namespace.is_none_or(|ns| ns == reference.namespace)
                        && !contains(&targets[0].bounds, &band.bounds)
                }
                _ => false,
            }
        }) {
            return false;
        }
    }
    true
}

pub(super) fn choreography_scope_mut<'a>(
    c: &'a mut crate::bpmn_collaboration::Choreography,
    scope: &ElementSelector,
) -> Result<&'a mut crate::bpmn_collaboration::ChoreographyGraph, CommandError> {
    fn nested<'a>(
        graph: &'a mut crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
    ) -> Option<&'a mut crate::bpmn_collaboration::ChoreographyGraph> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &mut graph.flow_elements {
            if let ChoreographyFlowElement::Activity(activity) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &mut activity.details
            {
                if activity.id == *scope {
                    return Some(graph);
                }
                if let Some(found) = nested(graph, scope) {
                    return Some(found);
                }
            }
        }
        None
    }
    let root = c.id == *scope;
    let graph = c
        .graph
        .as_mut()
        .ok_or_else(|| invalid("scoped graph command requires a promoted graph"))?;
    if root {
        Ok(graph)
    } else {
        nested(graph, scope).ok_or_else(|| invalid("choreography scope not found"))
    }
}
pub(super) fn choreography_element_identity(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
) -> crate::ids::ElementIdentity {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    match element {
        ChoreographyFlowElement::Activity(a) => a.id.clone(),
        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => n.id.clone(),
        ChoreographyFlowElement::SequenceFlow(f) => f.id.clone(),
    }
}
/// Scope that directly contains a graph-mode flow element, searching nested Sub graphs.
pub(super) fn choreography_graph_owner(
    document: &BpmnDocument,
    id: &ElementSelector,
) -> Option<(ElementSelector, ElementSelector)> {
    fn walk(
        graph: &crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
        id: &ElementSelector,
    ) -> Option<ElementSelector> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &graph.flow_elements {
            if choreography_element_id(element) == *id {
                return Some(scope.clone());
            }
            if let ChoreographyFlowElement::Activity(a) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &a.details
                && let Some(found) = walk(graph, &a.id.selector(), id)
            {
                return Some(found);
            }
        }
        None
    }
    document.choreographies.iter().find_map(|c| {
        walk(c.graph.as_ref()?, &c.id.selector(), id).map(|scope| (c.id.selector(), scope))
    })
}
pub(super) fn rename_choreography_element(
    graph: &mut crate::bpmn_collaboration::ChoreographyGraph,
    id: &ElementSelector,
    name: &Option<String>,
) -> bool {
    use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
    let mut found = false;
    for element in &mut graph.flow_elements {
        match element {
            ChoreographyFlowElement::Activity(a) => {
                if a.id == *id {
                    a.name = name.clone();
                    found = true;
                }
                if let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &mut a.details {
                    found |= rename_choreography_element(graph, id, name);
                }
            }
            ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                if n.id == *id {
                    n.name = name.clone();
                    found = true;
                }
            }
            ChoreographyFlowElement::SequenceFlow(f) => {
                if f.id == *id {
                    f.name = name.clone();
                    found = true;
                }
            }
        }
    }
    found
}
/// Removes an element plus what cannot outlive it: connected sequence flows, events attached to
/// it, and data associations it owns (recursively for removed dependents).
pub(super) fn cascade_choreography_removal(
    graph: &mut crate::bpmn_collaboration::ChoreographyGraph,
    element_id: &ElementSelector,
    namespace: Option<&str>,
) -> Result<(), CommandError> {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    use crate::bpmn_model::NodeDetails;
    fn depends(
        element: &ChoreographyFlowElement,
        doomed: &[crate::ids::ElementIdentity],
        namespace: Option<&str>,
    ) -> bool {
        let hits = |r: &crate::bpmn_model::EventReference| {
            doomed.iter().any(|d| r.matches_identity(d, namespace))
        };
        match element {
            ChoreographyFlowElement::SequenceFlow(f) => {
                [&f.source, &f.target].into_iter().flatten().any(hits)
            }
            ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                match &n.details {
                    Some(NodeDetails::Event {
                        attached_to: Some(r),
                        ..
                    })
                    | Some(NodeDetails::BoundaryEvent { attached_to: r, .. }) => hits(r),
                    _ => false,
                }
            }
            ChoreographyFlowElement::Activity(_) => false,
        }
    }
    let mut doomed = vec![
        graph
            .flow_elements
            .iter()
            .find(|e| choreography_element_id(e) == *element_id)
            .map(choreography_element_identity)
            .ok_or_else(|| invalid("choreography flow element not found"))?,
    ];
    while let Some(next) = graph
        .flow_elements
        .iter()
        .find(|e| {
            !doomed.iter().any(|d| *d == choreography_element_id(e))
                && depends(e, &doomed, namespace)
        })
        .map(choreography_element_identity)
    {
        doomed.push(next);
    }
    graph
        .flow_elements
        .retain(|e| !doomed.iter().any(|d| *d == choreography_element_id(e)));
    graph
        .data_associations
        .retain(|a| !doomed.iter().any(|d| *d == a.owner));
    Ok(())
}
pub(super) fn delete_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: &ElementSelector,
    scope_id: &ElementSelector,
    element_id: &ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
    let before: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.clone())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then(|| GENERATED_TARGET_NAMESPACE.to_owned())
        });
    let graph = choreography_scope_mut(choreography_mut(document, choreography_id)?, scope_id)?;
    cascade_choreography_removal(graph, element_id, namespace.as_deref())?;
    let after: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let removed: std::collections::HashSet<_> = before.difference(&after).cloned().collect();
    for diagram in &mut document.diagrams {
        let owners = diagram
            .shapes
            .iter()
            .filter(|shape| removed.contains(&shape.element))
            .map(|shape| shape.id.selector())
            .collect();
        remove_di_shapes(diagram, &owners, namespace.as_deref());
        diagram
            .edges
            .retain(|edge| !removed.contains(&edge.element));
    }
    prune_removed_element_refs(p, document_id, Some(&removed));
    Ok(())
}
