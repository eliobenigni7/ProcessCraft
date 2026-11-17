//! Gesture-level modeling mutations (create, append, connect, move, delete, lanes, layout).
use super::*;
pub(super) fn apply_layout(
    p: &mut ProjectData,
    proposal: crate::layout::LayoutProposal,
) -> Result<(), CommandError> {
    let document = doc_structural(p, proposal.document_id)?;
    crate::layout::validate_proposal(document, &proposal).map_err(|e| invalid(&e.to_string()))?;
    let diagram = diagram(document, &proposal.diagram_id)?;
    for geometry in proposal.proposed.shapes {
        let shape = diagram
            .shapes
            .iter_mut()
            .find(|s| s.id == geometry.id)
            .ok_or_else(|| invalid("shape not found"))?;
        shape.bounds = geometry.bounds;
        if let (Some(label), Some(bounds)) = (&mut shape.label, geometry.label_bounds) {
            label.bounds = Some(bounds);
        }
    }
    for geometry in proposal.proposed.edges {
        let edge = diagram
            .edges
            .iter_mut()
            .find(|e| e.id == geometry.id)
            .ok_or_else(|| invalid("edge not found"))?;
        edge.waypoints = geometry.waypoints;
        if let (Some(label), Some(bounds)) = (&mut edge.label, geometry.label_bounds) {
            label.bounds = Some(bounds);
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn splice_activity(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    flow_id: ElementSelector,
    node: Node,
    new_flow: Flow,
    diagram_id: ElementSelector,
    shape: Shape,
    old_waypoints: Vec<Point>,
    new_edge: Edge,
) -> Result<(), CommandError> {
    if !node.kind.is_activity() {
        return Err(invalid("splice requires activity"));
    }
    let d = doc_structural(p, document_id)?;
    if d.diagrams.len() != 1 || d.diagrams[0].id != diagram_id {
        return Err(invalid("splice requires exactly one diagram"));
    }
    let (nodes, _, flows) = scope_parts_mut(d, &process_id)?;
    let old = flows
        .iter()
        .find(|f| f.id == flow_id && f.kind == FlowKind::Sequence)
        .ok_or_else(|| invalid("sequence flow not found"))?
        .clone();
    if !matches!(
        &old.source,
        Some(crate::bpmn_model::EventReference::Local(_))
    ) || !matches!(
        &old.target,
        Some(crate::bpmn_model::EventReference::Local(_))
    ) {
        return Err(invalid(
            "splice requires resolved local sequence-flow endpoints",
        ));
    }
    if flows
        .iter()
        .filter(|f| f.kind == FlowKind::Sequence && f.source == old.source)
        .count()
        != 1
        || flows
            .iter()
            .filter(|f| f.kind == FlowKind::Sequence && f.target == old.target)
            .count()
            != 1
    {
        return Err(invalid("ambiguous branch"));
    }
    if new_flow.kind != FlowKind::Sequence
        || !matches!(&new_flow.source, Some(crate::bpmn_model::EventReference::Local(id)) if id == &node.id.selector())
        || new_flow.target != old.target
        || new_flow.condition.is_some()
        || shape.element != node.id
        || new_edge.element != new_flow.id
    {
        return Err(invalid("invalid splice elements"));
    }
    flows.iter_mut().find(|f| f.id == flow_id).unwrap().target =
        Some(crate::bpmn_model::EventReference::Local(node.id.selector()));
    nodes.push(node);
    flows.push(new_flow);
    let dg = diagram(d, &diagram_id)?;
    let matches = dg.edges.iter().filter(|e| e.element == flow_id).count();
    if matches != 1 {
        return Err(invalid("splice requires one existing edge"));
    }
    dg.edges
        .iter_mut()
        .find(|e| e.element == flow_id)
        .unwrap()
        .waypoints = old_waypoints;
    dg.shapes.push(shape);
    dg.edges.push(new_edge);
    Ok(())
}
pub(super) fn create_shape(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape: Shape,
) -> Result<(), CommandError> {
    let diagram = diagram(doc_structural(p, document_id)?, &diagram_id)?;
    if let Some(order) = &mut diagram.plane_order {
        order.push(shape.id.selector());
    }
    diagram.shapes.push(shape);
    Ok(())
}
pub(super) fn delete_shape(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
) -> Result<(), CommandError> {
    let document = doc_structural(p, document_id)?;
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
    let diagram = diagram(document, &diagram_id)?;
    if !diagram.shapes.iter().any(|shape| shape.id == shape_id) {
        return Err(invalid("shape not found"));
    }
    remove_di_shapes(
        diagram,
        &std::collections::HashSet::from([shape_id]),
        namespace.as_deref(),
    );
    Ok(())
}
pub(super) fn create_edge(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    edge: Edge,
) -> Result<(), CommandError> {
    let diagram = diagram(doc_structural(p, document_id)?, &diagram_id)?;
    if let Some(order) = &mut diagram.plane_order {
        order.push(edge.id.selector());
    }
    diagram.edges.push(edge);
    Ok(())
}
pub(super) fn create_collaboration(
    p: &mut ProjectData,
    document_id: DocumentId,
    collaboration: Collaboration,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    if d.collaboration.is_some() {
        return Err(invalid("collaboration already exists"));
    }
    d.collaboration = Some(collaboration);
    Ok(())
}
pub(super) fn create_participant(
    p: &mut ProjectData,
    document_id: DocumentId,
    participant: Participant,
) -> Result<(), CommandError> {
    doc_structural(p, document_id)?
        .collaboration
        .as_mut()
        .ok_or_else(|| invalid("collaboration not found"))?
        .participants
        .push(participant);
    Ok(())
}
pub(super) fn create_lane(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    lane: Lane,
) -> Result<(), CommandError> {
    scope_parts_mut(doc_structural(p, document_id)?, &process_id)?
        .1
        .push(lane);
    Ok(())
}
pub(super) fn create_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    element: Node,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let mut pending = vec![&element];
    while let Some(node) = pending.pop() {
        if let Some(activity) = &node.activity {
            check_activity_references(d, activity, None)?;
        }
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
            pending.extend(&content.nodes);
        }
    }
    scope_parts_mut(d, &process_id)?.0.push(element);
    Ok(())
}
pub(super) fn create_flow(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: Option<ElementSelector>,
    flow: Flow,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    if let Some(id) = process_id {
        // BPMN export writes a scope's sequence flows before its associations; keep the
        // same order so an exported file re-imports to the same model.
        let flows = scope_parts_mut(d, &id)?.2;
        let at = match flow.kind {
            FlowKind::Sequence => flows
                .iter()
                .position(|f| f.kind == FlowKind::Association)
                .unwrap_or(flows.len()),
            _ => flows.len(),
        };
        flows.insert(at, flow);
    } else {
        d.collaboration
            .as_mut()
            .ok_or_else(|| invalid("collaboration not found"))?
            .flows
            .push(flow);
    }
    Ok(())
}
pub(super) fn apply_move_shape(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
    bounds: Bounds,
) -> Result<(), CommandError> {
    let s = diagram(doc_nonstructural(p, document_id)?, &diagram_id)?
        .shapes
        .iter_mut()
        .find(|s| s.id == shape_id)
        .ok_or_else(|| invalid("shape not found"))?;
    move_shape(s, bounds);
    Ok(())
}
pub(super) fn reroute(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    edge_id: ElementSelector,
    waypoints: Vec<Point>,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    // Every route change is normalized here (ends docked on the true outline), and an
    // explicit label follows the route's midpoint, so moves, resizes, reconnects and
    // manual edits all keep it in place.
    let waypoints = if waypoints.iter().all(crate::geometry::valid_point) {
        let dg = d
            .diagrams
            .iter()
            .find(|g| g.id == diagram_id)
            .ok_or_else(|| invalid("diagram not found"))?;
        let edge = dg
            .edges
            .iter()
            .find(|s| s.id == edge_id)
            .ok_or_else(|| invalid("edge not found"))?;
        crate::geometry::dock_ends(d, dg, edge, crate::geometry::tidy(waypoints))
    } else {
        waypoints
    };
    let e = diagram(d, &diagram_id)?
        .edges
        .iter_mut()
        .find(|s| s.id == edge_id)
        .ok_or_else(|| invalid("edge not found"))?;
    if let (Some(old), Some(new)) = (
        crate::geometry::midpoint(&e.waypoints),
        crate::geometry::midpoint(&waypoints),
    ) && let Some(b) = e.label.as_mut().and_then(|l| l.bounds.as_mut())
    {
        let shifted = Bounds {
            x: b.x + new.x - old.x,
            y: b.y + new.y - old.y,
            ..b.clone()
        };
        if crate::geometry::valid_bounds(&shifted) {
            *b = shifted;
        }
    }
    e.waypoints = waypoints;
    Ok(())
}
pub(super) fn assign_lane(
    p: &mut ProjectData,
    document_id: DocumentId,
    process_id: ElementSelector,
    element_id: ElementSelector,
    lane_id: Option<ElementSelector>,
    bounds: Vec<ShapeBounds>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let (nodes, lanes, _) = scope_parts_mut(d, &process_id)?;
    if !nodes.iter().any(|n| n.id == element_id) {
        return Err(invalid("node not found"));
    }
    if lane_id
        .as_ref()
        .is_some_and(|id| !lanes.iter().any(|l| &l.id == id))
    {
        return Err(invalid("lane not found"));
    }
    let current = lanes
        .iter()
        .find(|l| l.members.contains(&element_id))
        .map(|l| l.id.selector());
    if current.as_ref() != lane_id.as_ref() {
        for l in lanes {
            l.members.retain(|id| id != &element_id);
            if Some(l.id.selector()).as_ref() == lane_id.as_ref() {
                l.members.push(element_id.clone());
            }
        }
    }
    for dg in &d.diagrams {
        for shape in &dg.shapes {
            if shape.element == element_id
                && !bounds
                    .iter()
                    .any(|b| b.diagram_id == dg.id && b.shape_id == shape.id)
            {
                return Err(invalid("all visible node bounds required"));
            }
        }
    }
    for b in bounds {
        let s = diagram(d, &b.diagram_id)?
            .shapes
            .iter_mut()
            .find(|s| s.id == b.shape_id && s.element == element_id)
            .ok_or_else(|| invalid("invalid lane assignment bounds"))?;
        move_shape(s, b.bounds);
    }
    Ok(())
}
pub(super) fn connect(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    source_id: ElementSelector,
    target_id: ElementSelector,
    flow_id: crate::ids::ElementIdentity,
    edge_id: crate::ids::ElementIdentity,
) -> Result<(), CommandError> {
    doc_structural(p, document_id)?;
    let d = p
        .documents
        .iter()
        .find(|d| d.id == document_id)
        .cloned()
        .ok_or_else(|| invalid("document not found"))?;
    let batch = crate::modeling::connect(&d, &diagram_id, &source_id, &target_id, flow_id, edge_id)
        .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn append(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    source_id: ElementSelector,
    kind: NodeKind,
    name: Option<String>,
    node_id: crate::ids::ElementIdentity,
    shape_id: crate::ids::ElementIdentity,
    flow_id: crate::ids::ElementIdentity,
    edge_id: crate::ids::ElementIdentity,
) -> Result<(), CommandError> {
    doc_structural(p, document_id)?;
    let d = p
        .documents
        .iter()
        .find(|d| d.id == document_id)
        .cloned()
        .ok_or_else(|| invalid("document not found"))?;
    let node = node_id.selector();
    let batch = crate::modeling::append(
        &d,
        &diagram_id,
        &source_id,
        kind,
        name,
        crate::modeling::AppendIds {
            node: node_id,
            shape: shape_id,
            flow: flow_id,
            edge: edge_id,
        },
    )
    .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    auto_resize(p, document_id, &diagram_id, vec![node], &d)?;
    Ok(())
}
pub(super) fn replace_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
    target: crate::replace::ReplaceTarget,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    if let Some(diagram_id) = crate::containers::collapse_toggle_target(d, &element_id, &target) {
        return mutate(
            p,
            Mutation::ToggleCollapse {
                document_id,
                diagram_id,
                element_id,
            },
        );
    }
    let batch =
        crate::replace::replace(d, &element_id, &target).map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    Ok(())
}
pub(super) fn move_elements(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shapes: Vec<ShapeBounds>,
    flow_id: Option<crate::ids::ElementIdentity>,
    edge_id: Option<crate::ids::ElementIdentity>,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    let old = d.clone();
    let semantic = d.capability.allows_structural_editing() && !d.blocks_structural_edits();
    let moved: Vec<ElementSelector> = d
        .diagrams
        .iter()
        .filter(|g| g.id == diagram_id)
        .flat_map(|g| &g.shapes)
        .filter(|s| shapes.iter().any(|b| b.shape_id == s.id))
        .map(|s| s.element.clone())
        .collect();
    let batch =
        crate::manipulate::move_elements(d, &diagram_id, shapes, flow_id.zip(edge_id), semantic)
            .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    auto_resize(p, document_id, &diagram_id, moved, &old)?;
    Ok(())
}
pub(super) fn resize_shape(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_id: ElementSelector,
    bounds: Bounds,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    let old = d.clone();
    let resized: Vec<ElementSelector> = d
        .diagrams
        .iter()
        .filter(|g| g.id == diagram_id)
        .flat_map(|g| &g.shapes)
        .filter(|s| s.id == shape_id)
        .map(|s| s.element.clone())
        .collect();
    let batch = crate::manipulate::resize(d, &diagram_id, &shape_id, bounds)
        .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    let d = doc_nonstructural(p, document_id)?;
    if d.capability.allows_structural_editing() && !d.blocks_structural_edits() {
        crate::containers::sync_lanes(&old, d, &diagram_id);
    }
    // Pools and expanded subprocesses grow around the resized shape, as on a move.
    auto_resize(p, document_id, &diagram_id, resized, &old)?;
    Ok(())
}
pub(super) fn reconnect(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    flow_id: ElementSelector,
    source_id: ElementSelector,
    target_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let batch = crate::manipulate::reconnect(d, &diagram_id, &flow_id, &source_id, &target_id)
        .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn create_at(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    kind: NodeKind,
    name: Option<String>,
    center: Point,
    node_id: crate::ids::ElementIdentity,
    shape_id: crate::ids::ElementIdentity,
    flow_id: Option<crate::ids::ElementIdentity>,
    edge_id: Option<crate::ids::ElementIdentity>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let old = d.clone();
    let node = node_id.selector();
    let ids = crate::manipulate::CreateIds {
        node: node_id,
        shape: shape_id,
        flow: flow_id,
        edge: edge_id,
    };
    let batch = crate::manipulate::create_at(d, &diagram_id, kind, name, center, ids)
        .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    auto_resize(p, document_id, &diagram_id, vec![node], &old)?;
    Ok(())
}
pub(super) fn add_lane(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    element_id: ElementSelector,
    location: crate::containers::LaneLocation,
) -> Result<(), CommandError> {
    crate::containers::add_lane(
        doc_structural(p, document_id)?,
        &diagram_id,
        &element_id,
        location,
    )
    .map_err(CommandError::InvalidParams)?;
    Ok(())
}
pub(super) fn split_lane(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    element_id: ElementSelector,
    count: u8,
) -> Result<(), CommandError> {
    crate::containers::split_lane(
        doc_structural(p, document_id)?,
        &diagram_id,
        &element_id,
        count,
    )
    .map_err(CommandError::InvalidParams)?;
    Ok(())
}
pub(super) fn create_pool(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    name: Option<String>,
    center: Option<Point>,
) -> Result<(), CommandError> {
    let batch =
        crate::containers::create_pool(doc_structural(p, document_id)?, &diagram_id, name, center)
            .map_err(CommandError::InvalidParams)?;
    for m in batch {
        mutate(p, m)?;
    }
    Ok(())
}
pub(super) fn toggle_collapse(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    element_id: ElementSelector,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let old = d.clone();
    if crate::containers::toggle_collapse(d, &diagram_id, &element_id)
        .map_err(CommandError::InvalidParams)?
    {
        auto_resize(p, document_id, &diagram_id, vec![element_id], &old)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn make_space(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    axis: crate::containers::Axis,
    start: f64,
    delta: f64,
    before: bool,
    at: Option<Point>,
) -> Result<(), CommandError> {
    let d = doc_nonstructural(p, document_id)?;
    let semantic = d.capability.allows_structural_editing() && !d.blocks_structural_edits();
    let space = crate::containers::Space {
        axis,
        start,
        delta,
        after: !before,
    };
    crate::containers::make_space(d, &diagram_id, space, semantic, at)
        .map_err(CommandError::InvalidParams)?;
    Ok(())
}
pub(super) fn align(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_ids: Vec<ElementSelector>,
    alignment: crate::arrange::Alignment,
) -> Result<(), CommandError> {
    let prepared = crate::arrange::prepare_align(
        doc_nonstructural(p, document_id)?,
        &diagram_id,
        &shape_ids,
        alignment,
    )
    .map_err(invalid)?;
    apply_prepared(p, prepared)?;
    Ok(())
}
pub(super) fn distribute(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    shape_ids: Vec<ElementSelector>,
    distribution: crate::arrange::Distribution,
) -> Result<(), CommandError> {
    let prepared = crate::arrange::prepare_distribute(
        doc_nonstructural(p, document_id)?,
        &diagram_id,
        &shape_ids,
        distribution,
    )
    .map_err(invalid)?;
    apply_prepared(p, prepared)?;
    Ok(())
}
pub(super) fn duplicate(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: ElementSelector,
    element_ids: Vec<ElementSelector>,
    dx: f64,
    dy: f64,
    target_diagram_id: Option<ElementSelector>,
) -> Result<(), CommandError> {
    let d = doc_structural(p, document_id)?;
    let clipboard = crate::clipboard::copy(d, &diagram_id, &element_ids).map_err(invalid)?;
    let target = target_diagram_id.unwrap_or(diagram_id);
    let fallback = crate::clipboard::default_scope(d, &target)
        .ok_or_else(|| invalid("Create a process before pasting"))?;
    let pasted =
        crate::clipboard::paste(d, &target, &fallback, &clipboard, dx, dy).map_err(invalid)?;
    for m in pasted.mutations {
        mutate(p, m)?;
    }
    Ok(())
}
pub(super) fn delete(
    p: &mut ProjectData,
    document_id: DocumentId,
    element_id: ElementSelector,
) -> Result<(), CommandError> {
    if let Some((choreography_id, scope_id)) =
        choreography_graph_owner(doc_structural(p, document_id)?, &element_id)
    {
        return delete_choreography_flow_element(
            p,
            document_id,
            &choreography_id,
            &scope_id,
            &element_id,
        );
    }
    let d = doc_structural(p, document_id)?;
    // Sibling lanes take a deleted lane's space; membership follows afterwards.
    let lane_old = crate::queries::scopes(d)
        .iter()
        .any(|s| s.lanes.iter().any(|l| l.id == element_id))
        .then(|| d.clone());
    let lane_diagrams = match &lane_old {
        Some(_) => crate::containers::compensate_lane_delete(d, &element_id),
        None => Vec::new(),
    };
    let index = crate::reference_resolution::ReferenceIndex::new(d);
    let mut removed = std::collections::HashSet::new();
    let mut found = false;
    for pr in &mut d.processes {
        found |= delete_nested(
            &mut pr.nodes,
            &mut pr.lanes,
            &mut pr.flows,
            &mut pr.data_associations,
            &element_id,
            &mut removed,
            0,
        );
    }
    if let Some(c) = &mut d.collaboration {
        let n = c.participants.len() + c.flows.len();
        c.participants.retain(|x| x.id != element_id);
        c.flows.retain(|x| x.id != element_id);
        if n != c.participants.len() + c.flows.len() {
            removed.insert(element_id.clone());
            found = true;
        }
        if remove_collaboration_artifact(&mut c.extras, &element_id) {
            removed.insert(element_id.clone());
            found = true;
        }
    }
    for c in &mut d.additional_collaborations {
        let n = c.participants.len() + c.flows.len();
        c.participants.retain(|value| value.id != element_id);
        c.flows.retain(|value| value.id != element_id);
        if n != c.participants.len() + c.flows.len()
            || remove_collaboration_artifact(&mut c.extras, &element_id)
        {
            removed.insert(element_id.clone());
            found = true;
        }
    }
    for c in &mut d.choreographies {
        let n = c.participants.len() + c.message_flows.len() + c.sequence_flows.len();
        c.participants.retain(|value| value.id != element_id);
        c.message_flows.retain(|value| value.id != element_id);
        c.sequence_flows.retain(|value| value.id != element_id);
        if n != c.participants.len() + c.message_flows.len() + c.sequence_flows.len()
            || remove_collaboration_artifact(&mut c.extras, &element_id)
        {
            removed.insert(element_id.clone());
            found = true;
        }
    }
    if !found {
        return Err(invalid("element not found or cannot be deleted"));
    }
    // Cascade in the same undo step: flows touching a deleted element go
    // with it, and a deleted flow stops being any node's default flow.
    let touches = |reference: &Option<crate::bpmn_activity::ModelReference>| {
        reference.as_ref().is_some_and(|reference| {
            matches!(index.resolve(reference),
                crate::reference_resolution::ReferenceResolution::Resolved(id)
                    if removed.contains(&id))
        })
    };
    let incident: Vec<ElementSelector> = crate::queries::scopes(d)
        .iter()
        .flat_map(|scope| scope.flows.iter())
        .chain(d.collaborations().flat_map(|c| c.flows.iter()))
        .filter(|f| {
            !removed.contains(&f.id.selector()) && (touches(&f.source) || touches(&f.target))
        })
        .map(|f| f.id.selector())
        .collect();
    for flow_id in &incident {
        for pr in &mut d.processes {
            delete_nested(
                &mut pr.nodes,
                &mut pr.lanes,
                &mut pr.flows,
                &mut pr.data_associations,
                flow_id,
                &mut removed,
                0,
            );
        }
        for c in d
            .collaboration
            .iter_mut()
            .chain(&mut d.additional_collaborations)
        {
            let n = c.flows.len();
            c.flows.retain(|x| x.id != *flow_id);
            if n != c.flows.len() {
                removed.insert(flow_id.clone());
            }
        }
    }
    for pr in &mut d.processes {
        clear_removed_defaults(&mut pr.nodes, &removed);
    }
    for dg in &mut d.diagrams {
        dg.shapes.retain(|s| !removed.contains(&s.element));
        dg.edges.retain(|e| !removed.contains(&e.element));
    }
    if let Some(old) = &lane_old {
        for diagram_id in &lane_diagrams {
            crate::containers::sync_lanes(old, d, diagram_id);
        }
    }
    prune_removed_element_refs(p, document_id, Some(&removed));
    Ok(())
}
