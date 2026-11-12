//! Deterministic, explicitly requested BPMN geometry proposals.
use crate::{
    geometry::{valid_bounds, valid_point},
    ids::{DocumentId, ElementSelector},
    model::{Bounds, BpmnDocument, Capability, Diagram, FlowKind, NodeKind, Point},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeGeometry {
    pub id: ElementSelector,
    pub bounds: Bounds,
    pub label_bounds: Option<Bounds>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeGeometry {
    pub id: ElementSelector,
    pub waypoints: Vec<Point>,
    pub label_bounds: Option<Bounds>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutGeometry {
    pub shapes: Vec<ShapeGeometry>,
    pub edges: Vec<EdgeGeometry>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutProposal {
    pub session_id: String,
    pub generation: u64,
    pub document_id: DocumentId,
    pub diagram_id: ElementSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choreography_scope_shape: Option<ElementSelector>,
    pub original: LayoutGeometry,
    pub proposed: LayoutGeometry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LayoutError {
    pub code: String,
    pub message: String,
}
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for LayoutError {}
fn err(code: &str, message: &str) -> LayoutError {
    LayoutError {
        code: code.into(),
        message: message.into(),
    }
}
fn cancelled(c: &AtomicBool) -> Result<(), LayoutError> {
    if c.load(Ordering::Relaxed) {
        Err(err("cancelled", "Layout was cancelled"))
    } else {
        Ok(())
    }
}
fn geometry(d: &Diagram) -> LayoutGeometry {
    let mut shapes: Vec<_> = d
        .shapes
        .iter()
        .map(|s| ShapeGeometry {
            id: s.id.selector(),
            bounds: s.bounds.clone(),
            label_bounds: s.label.as_ref().and_then(|l| l.bounds.clone()),
        })
        .collect();
    let mut edges: Vec<_> = d
        .edges
        .iter()
        .map(|e| EdgeGeometry {
            id: e.id.selector(),
            waypoints: e.waypoints.clone(),
            label_bounds: e.label.as_ref().and_then(|l| l.bounds.clone()),
        })
        .collect();
    shapes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    LayoutGeometry { shapes, edges }
}
fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}
fn valid_geometry(g: &LayoutGeometry) -> bool {
    let mut ids = BTreeSet::new();
    g.shapes.iter().all(|s| {
        ids.insert(&s.id)
            && valid_bounds(&s.bounds)
            && s.bounds.width > 0.0
            && s.bounds.height > 0.0
            && s.label_bounds.as_ref().is_none_or(valid_bounds)
    }) && {
        let mut ids = BTreeSet::new();
        g.edges.iter().all(|e| {
            ids.insert(&e.id)
                && (2..=16).contains(&e.waypoints.len())
                && e.waypoints.iter().all(valid_point)
                && e.label_bounds.as_ref().is_none_or(valid_bounds)
        })
    }
}
/// Check identity, exact base geometry and complete bounded output before atomic application.
pub fn validate_proposal(
    document: &BpmnDocument,
    proposal: &LayoutProposal,
) -> Result<(), LayoutError> {
    if !matches!(document.capability, Capability::Editable) || document.id != proposal.document_id {
        return Err(err(
            "identity",
            "Document is not the editable proposal target",
        ));
    }
    let diagram = document
        .diagrams
        .iter()
        .find(|d| d.id.selector() == proposal.diagram_id)
        .ok_or_else(|| err("identity", "Diagram not found"))?;
    let base = geometry(diagram);
    if !same(&base, &proposal.original) {
        return Err(err("stale", "Base geometry changed"));
    }
    if !valid_geometry(&proposal.original)
        || !valid_geometry(&proposal.proposed)
        || proposal.proposed.shapes.len() != base.shapes.len()
        || proposal.proposed.edges.len() != base.edges.len()
    {
        return Err(err("invalid_geometry", "Incomplete or invalid geometry"));
    }
    for (a, b) in base.shapes.iter().zip(&proposal.proposed.shapes) {
        if a.id != b.id || a.label_bounds.is_some() != b.label_bounds.is_some() {
            return Err(err("invalid_geometry", "Shape IDs or labels changed"));
        }
    }
    for (a, b) in base.edges.iter().zip(&proposal.proposed.edges) {
        if a.id != b.id || a.label_bounds.is_some() != b.label_bounds.is_some() {
            return Err(err("invalid_geometry", "Edge IDs or labels changed"));
        }
    }
    let cancelled = AtomicBool::new(false);
    let computed = if let Some(shape) = &proposal.choreography_scope_shape {
        propose_choreography_scope_layout(
            document,
            &proposal.diagram_id,
            shape,
            &proposal.session_id,
            proposal.generation,
            &cancelled,
        )?
    } else {
        propose_layout(
            document,
            &proposal.diagram_id,
            &proposal.session_id,
            proposal.generation,
            &cancelled,
        )?
    };
    if !same(&computed.proposed, &proposal.proposed) {
        return Err(err(
            "invalid_geometry",
            "Proposal differs from deterministic layout",
        ));
    }
    Ok(())
}

/// Lay out one unambiguous direct choreography scope using the existing bounded
/// router, then translate each actual participant-band representation with its owner.
/// Expanded nested geometry and ambiguous views require an explicit representation
/// selector and remain outside this profile.
fn propose_choreography_layout(
    document: &BpmnDocument,
    diagram: &Diagram,
    session_id: &str,
    generation: u64,
    cancel: &AtomicBool,
) -> Result<LayoutProposal, LayoutError> {
    use crate::bpmn_collaboration::ChoreographyActivityDetails;
    use crate::model::{DiReference, Node, Process};
    let scopes = crate::queries::choreography_scopes(document);
    let scope = scopes
        .iter()
        .find(|s| s.owner == diagram.plane)
        .ok_or_else(|| err("identity", "Choreography scope not found"))?;
    let owner_id = if scope.parent.is_none() {
        scope.root.id.clone()
    } else {
        scopes
            .iter()
            .flat_map(|s| &s.activities)
            .find(|a| a.id.selector() == scope.owner)
            .ok_or_else(|| err("identity", "SubChoreography scope owner not found"))?
            .id
            .clone()
    };
    let mut nodes: Vec<Node> = scope
        .activities
        .iter()
        .map(|a| Node {
            flow_metadata: None,
            item_aware: None,
            id: a.id.clone(),
            name: a.name.clone(),
            documentation: String::new().into(),
            base_extras: None,
            kind: NodeKind::Task,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        })
        .collect();
    for node in scope.events.iter().chain(&scope.gateways) {
        let mut node = (*node).clone();
        // The shared router distinguishes rectangular activities from other
        // shapes for port placement. Preserve the authored dimensions while
        // selecting its non-activity routing class in this transient view.
        node.kind = match node.kind {
            NodeKind::StartEvent
            | NodeKind::EndEvent
            | NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway => node.kind,
            NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent => {
                NodeKind::StartEvent
            }
            NodeKind::InclusiveGateway | NodeKind::ComplexGateway | NodeKind::EventBasedGateway => {
                NodeKind::ExclusiveGateway
            }
            NodeKind::BoundaryEvent => {
                return Err(err(
                    "unsupported",
                    "Boundary event layout needs attachment-aware routing",
                ));
            }
            _ => return Err(err("unsupported", "Unsupported choreography node kind")),
        };
        node.details = None;
        node.flow_metadata = None;
        nodes.push(node);
    }
    let node_ids: BTreeSet<_> = nodes.iter().map(|n| n.id.selector()).collect();
    let flow_ids: BTreeSet<_> = scope.flows.iter().map(|f| f.id.selector()).collect();
    if scope
        .activities
        .iter()
        .any(|a| matches!(a.details, Some(ChoreographyActivityDetails::Sub { .. })))
        && diagram.shapes.iter().any(|s| {
            scopes.iter().any(|child| {
                child.parent.as_ref() == Some(&scope.owner)
                    && (child
                        .activities
                        .iter()
                        .any(|a| a.id.selector() == s.element)
                        || child
                            .events
                            .iter()
                            .chain(&child.gateways)
                            .any(|n| n.id.selector() == s.element))
            })
        })
    {
        return Err(err(
            "unsupported",
            "Expanded nested choreography requires explicit representation selection",
        ));
    }
    let mut bands = Vec::new();
    let mut basic_shapes = Vec::new();
    for shape in &diagram.shapes {
        if node_ids.contains(&shape.element) && shape.participant_band_kind.is_none() {
            basic_shapes.push(shape.clone());
        } else if let (Some(_), Some(DiReference::Local(owner))) = (
            shape.participant_band_kind,
            &shape.choreography_activity_shape,
        ) {
            let owner_shape = diagram
                .shapes
                .iter()
                .find(|candidate| {
                    candidate.id.selector() == *owner && node_ids.contains(&candidate.element)
                })
                .ok_or_else(|| err("invalid_di", "Participant band owner is unresolved"))?;
            if shape.bounds.x < owner_shape.bounds.x
                || shape.bounds.y < owner_shape.bounds.y
                || shape.bounds.x + shape.bounds.width
                    > owner_shape.bounds.x + owner_shape.bounds.width
                || shape.bounds.y + shape.bounds.height
                    > owner_shape.bounds.y + owner_shape.bounds.height
            {
                return Err(err(
                    "unsupported",
                    "Participant band extends outside its activity shape",
                ));
            }
            let activity = scope
                .activities
                .iter()
                .find(|a| a.id.selector() == owner_shape.element)
                .ok_or_else(|| err("invalid_di", "Participant band owner is not an activity"))?;
            let index = crate::reference_resolution::ReferenceIndex::new(document);
            if !activity.participant_refs.iter().any(|r| matches!(index.resolve(r), crate::reference_resolution::ReferenceResolution::Resolved(id) if id == shape.element)) {
                return Err(err("invalid_di", "Participant band membership is unresolved"));
            }
            bands.push((shape, owner.clone()));
        } else {
            return Err(err(
                "unsupported",
                "Diagram contains an unqualified choreography representation",
            ));
        }
    }
    if diagram.edges.iter().any(|e| !flow_ids.contains(&e.element)) {
        return Err(err(
            "unsupported",
            "Diagram contains an unqualified choreography edge",
        ));
    }
    for edge in &diagram.edges {
        let flow = scope
            .flows
            .iter()
            .find(|f| f.id.selector() == edge.element)
            .ok_or_else(|| err("invalid_di", "Sequence flow edge is unresolved"))?;
        for (representation, endpoint) in [
            (&edge.source_element, &flow.source),
            (&edge.target_element, &flow.target),
        ] {
            if let Some(representation) = representation {
                let Some(shape) = basic_shapes
                    .iter()
                    .find(|s| s.id.selector() == *representation)
                else {
                    return Err(err(
                        "invalid_di",
                        "Sequence flow representation endpoint is unresolved",
                    ));
                };
                if endpoint.as_ref()
                    != Some(&crate::bpmn_activity::ModelReference::Local(
                        shape.element.clone(),
                    ))
                {
                    return Err(err(
                        "invalid_di",
                        "Sequence flow representation does not match its semantic endpoint",
                    ));
                }
            }
        }
    }
    let mut temporary = document.clone();
    temporary.choreographies.clear();
    temporary.collaboration = None;
    temporary.additional_collaborations.clear();
    temporary.processes = vec![Process {
        activity_metadata: None,
        is_executable: None,
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: owner_id,
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes,
        lanes: vec![],
        flows: scope.flows.iter().map(|f| (*f).clone()).collect(),
    }];
    let mut synthetic_diagram = diagram.clone();
    synthetic_diagram.shapes = basic_shapes;
    temporary.diagrams = vec![synthetic_diagram];
    let computed = propose_layout(
        &temporary,
        &diagram.id.selector(),
        session_id,
        generation,
        cancel,
    )?;
    let original = geometry(diagram);
    let mut proposed = original.clone();
    for shape in &computed.proposed.shapes {
        let target = proposed
            .shapes
            .iter_mut()
            .find(|s| s.id == shape.id)
            .ok_or_else(|| err("identity", "Shape missing from proposal"))?;
        *target = shape.clone();
    }
    for edge in &computed.proposed.edges {
        let target = proposed
            .edges
            .iter_mut()
            .find(|e| e.id == edge.id)
            .ok_or_else(|| err("identity", "Edge missing from proposal"))?;
        *target = edge.clone();
    }
    for (band, owner) in bands {
        let before = original
            .shapes
            .iter()
            .find(|s| s.id == owner)
            .ok_or_else(|| err("identity", "Band owner missing"))?;
        let after = proposed
            .shapes
            .iter()
            .find(|s| s.id == owner)
            .ok_or_else(|| err("identity", "Band owner missing"))?;
        let (dx, dy) = (
            after.bounds.x - before.bounds.x,
            after.bounds.y - before.bounds.y,
        );
        let target = proposed
            .shapes
            .iter_mut()
            .find(|s| s.id == band.id.selector())
            .ok_or_else(|| err("identity", "Band missing"))?;
        target.bounds.x += dx;
        target.bounds.y += dy;
        if let Some(label) = &mut target.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    if !valid_geometry(&proposed) {
        return Err(err("invalid_geometry", "Computed geometry exceeds bounds"));
    }
    Ok(LayoutProposal {
        session_id: session_id.into(),
        generation,
        document_id: document.id,
        diagram_id: diagram.id.selector(),
        choreography_scope_shape: None,
        original,
        proposed,
    })
}

/// Layout direct children of a selected expanded SubChoreography representation.
/// Other representations stay untouched; ambiguous ownership or edges fail closed.
pub fn propose_choreography_scope_layout(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    scope_shape_id: &ElementSelector,
    session_id: &str,
    generation: u64,
    cancel: &AtomicBool,
) -> Result<LayoutProposal, LayoutError> {
    propose_choreography_scope_impl(
        document,
        diagram_id,
        Some(scope_shape_id),
        session_id,
        generation,
        cancel,
    )
}

fn propose_choreography_scope_impl(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    scope_shape_id: Option<&ElementSelector>,
    session_id: &str,
    generation: u64,
    cancel: &AtomicBool,
) -> Result<LayoutProposal, LayoutError> {
    cancelled(cancel)?;
    if !matches!(document.capability, Capability::Editable) {
        return Err(err("unsupported", "Document is not editable"));
    }
    let diagram = document
        .diagrams
        .iter()
        .find(|d| d.id.selector() == *diagram_id)
        .ok_or_else(|| err("identity", "Diagram not found"))?;
    if diagram.shapes.len() > 400
        || diagram.edges.len() > 500
        || diagram.shapes.len() + diagram.edges.len() > 1000
    {
        return Err(err("limit", "Diagram exceeds layout limits"));
    }
    let owner = if let Some(id) = scope_shape_id {
        let shape = diagram
            .shapes
            .iter()
            .find(|s| s.id.selector() == *id)
            .ok_or_else(|| err("identity", "Scope representation not found"))?;
        if shape.is_expanded != Some(true) || shape.participant_band_kind.is_some() {
            return Err(err(
                "unsupported",
                "Select an explicitly expanded SubChoreography shape",
            ));
        }
        Some(shape)
    } else {
        None
    };
    let scopes = crate::queries::choreography_scopes(document);
    let scope = scopes
        .iter()
        .find(|s| match owner {
            Some(shape) => s.parent.is_some() && s.owner == shape.element,
            None => s.owner == diagram.plane,
        })
        .ok_or_else(|| err("identity", "Choreography scope not found"))?;
    let inside = |inner: &Bounds, outer: &Bounds| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.width <= outer.x + outer.width
            && inner.y + inner.height <= outer.y + outer.height
    };
    let within = |bounds: &Bounds| owner.is_none_or(|shape| inside(bounds, &shape.bounds));
    let point_within = |point: &Point| {
        owner.is_none_or(|shape| {
            point.x >= shape.bounds.x
                && point.y >= shape.bounds.y
                && point.x <= shape.bounds.x + shape.bounds.width
                && point.y <= shape.bounds.y + shape.bounds.height
        })
    };
    let members: BTreeSet<_> = scope
        .activities
        .iter()
        .map(|a| a.id.selector())
        .chain(
            scope
                .events
                .iter()
                .chain(&scope.gateways)
                .map(|n| n.id.selector()),
        )
        .collect();
    let mut shapes: Vec<_> = diagram
        .shapes
        .iter()
        .filter(|s| {
            members.contains(&s.element) && s.participant_band_kind.is_none() && within(&s.bounds)
        })
        .cloned()
        .collect();
    if let Some(owner) = owner
        && shapes.iter().any(|s| {
            diagram.shapes.iter().any(|other| {
                other.id != owner.id
                    && other.element == owner.element
                    && other.is_expanded == Some(true)
                    && inside(&s.bounds, &other.bounds)
            })
        })
    {
        return Err(err(
            "unsupported",
            "A nested representation belongs to multiple scope shapes",
        ));
    }
    let selected_ids: BTreeSet<_> = shapes.iter().map(|s| s.id.selector()).collect();
    for band in diagram
        .shapes
        .iter()
        .filter(|s| s.participant_band_kind.is_some())
    {
        if let Some(crate::model::DiReference::Local(id)) = &band.choreography_activity_shape
            && selected_ids.contains(id)
        {
            shapes.push(band.clone());
        }
    }
    let accepted: BTreeSet<_> = shapes.iter().map(|s| s.id.selector()).collect();
    let expanded_children: Vec<_> = shapes
        .iter()
        .filter(|s| {
            s.is_expanded == Some(true)
                && scopes.iter().any(|child| {
                    child.parent.as_ref() == Some(&scope.owner) && child.owner == s.element
                })
        })
        .cloned()
        .collect();
    let mut nested_shapes = BTreeSet::new();
    for shape in diagram.shapes.iter().filter(|s| {
        owner.is_none_or(|owner| s.id != owner.id)
            && !matches!((&s.choreography_activity_shape, scope_shape_id), (Some(crate::model::DiReference::Local(id)), Some(owner)) if id == owner)
            && within(&s.bounds)
            && !accepted.contains(&s.id.selector())
    }) {
        let parents: Vec<_> = expanded_children
            .iter()
            .filter(|parent| inside(&shape.bounds, &parent.bounds))
            .collect();
        if parents.len() != 1
            || shape
                .label
                .as_ref()
                .and_then(|label| label.bounds.as_ref())
                .is_some_and(|label| !inside(label, &parents[0].bounds))
        {
            return Err(err(
                "unsupported",
                "The selected scope contains an unqualified shape, band, or label",
            ));
        }
        nested_shapes.insert(shape.id.selector());
    }
    let mut nested_edges = BTreeSet::new();
    let mut edges = Vec::new();
    for edge in &diagram.edges {
        let flow = scope.flows.iter().find(|f| f.id.selector() == edge.element);
        let touches = edge
            .source_element
            .as_ref()
            .is_some_and(|id| selected_ids.contains(id))
            || edge
                .target_element
                .as_ref()
                .is_some_and(|id| selected_ids.contains(id));
        if let Some(flow) = flow {
            let endpoint = |reference: &Option<crate::bpmn_activity::ModelReference>,
                            representation: &Option<ElementSelector>|
             -> Option<bool> {
                let Some(crate::bpmn_activity::ModelReference::Local(semantic)) = reference else {
                    return None;
                };
                if let Some(id) = representation {
                    return Some(selected_ids.contains(id));
                }
                let all: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|s| s.participant_band_kind.is_none() && s.element == *semantic)
                    .collect();
                (all.len() == 1).then(|| selected_ids.contains(&all[0].id.selector()))
            };
            match (
                endpoint(&flow.source, &edge.source_element),
                endpoint(&flow.target, &edge.target_element),
            ) {
                (Some(true), Some(true)) => edges.push(edge.clone()),
                (Some(false), Some(false)) => {}
                _ => {
                    return Err(err(
                        "unsupported",
                        "Scope edge endpoints are ambiguous or cross the selected representation",
                    ));
                }
            }
        } else {
            let contained = expanded_children
                .iter()
                .filter(|parent| {
                    edge.waypoints.iter().all(|point| {
                        point.x >= parent.bounds.x
                            && point.y >= parent.bounds.y
                            && point.x <= parent.bounds.x + parent.bounds.width
                            && point.y <= parent.bounds.y + parent.bounds.height
                    })
                })
                .count();
            if contained == 1 && !edge.waypoints.is_empty() {
                let parent = expanded_children
                    .iter()
                    .find(|parent| {
                        edge.waypoints.iter().all(|p| {
                            p.x >= parent.bounds.x
                                && p.y >= parent.bounds.y
                                && p.x <= parent.bounds.x + parent.bounds.width
                                && p.y <= parent.bounds.y + parent.bounds.height
                        })
                    })
                    .ok_or_else(|| err("identity", "Nested edge parent missing"))?;
                if edge
                    .label
                    .as_ref()
                    .and_then(|label| label.bounds.as_ref())
                    .is_some_and(|label| !inside(label, &parent.bounds))
                {
                    return Err(err(
                        "unsupported",
                        "Nested edge label lies outside its parent",
                    ));
                }
                nested_edges.insert(edge.id.selector());
            } else if contained > 1
                || touches
                || (edge.source_element.is_none() && edge.target_element.is_none())
            {
                return Err(err(
                    "unsupported",
                    "An external edge may be connected to the selected scope",
                ));
            }
        }
    }
    let mut temporary = document.clone();
    let mut selected = diagram.clone();
    selected.plane = scope.owner.clone();
    selected.shapes = shapes;
    selected.edges = edges;
    temporary.diagrams = vec![selected.clone()];
    let computed =
        propose_choreography_layout(&temporary, &selected, session_id, generation, cancel)?;
    let original = geometry(diagram);
    let mut proposed = original.clone();
    let (dx, dy) = owner.map_or((0.0, 0.0), |owner| {
        (owner.bounds.x + 20.0, owner.bounds.y + 20.0)
    });
    for shape in &computed.proposed.shapes {
        let mut moved = shape.clone();
        moved.bounds.x += dx;
        moved.bounds.y += dy;
        if let Some(label) = &mut moved.label_bounds {
            label.x += dx;
            label.y += dy;
        }
        if !within(&moved.bounds) || moved.label_bounds.as_ref().is_some_and(|b| !within(b)) {
            return Err(err(
                "unsupported",
                "Expanded scope is too small for the proposed layout",
            ));
        }
        let slot = proposed
            .shapes
            .iter_mut()
            .find(|s| s.id == moved.id)
            .ok_or_else(|| err("identity", "Shape missing"))?;
        *slot = moved;
    }
    for edge in &computed.proposed.edges {
        let mut moved = edge.clone();
        for point in &mut moved.waypoints {
            point.x += dx;
            point.y += dy;
            if !point_within(point) {
                return Err(err(
                    "unsupported",
                    "A proposed route leaves the expanded scope",
                ));
            }
        }
        if let Some(label) = &mut moved.label_bounds {
            label.x += dx;
            label.y += dy;
            if !within(label) {
                return Err(err(
                    "unsupported",
                    "An edge label leaves the expanded scope",
                ));
            }
        }
        let slot = proposed
            .edges
            .iter_mut()
            .find(|e| e.id == moved.id)
            .ok_or_else(|| err("identity", "Edge missing"))?;
        *slot = moved;
    }
    // Move expanded direct children rigidly through the shared geometry proof.
    // The direct scope routes have already been computed; only descendant
    // mutations are incorporated from this request.
    let direct_shape_ids: BTreeSet<_> = computed
        .proposed
        .shapes
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let direct_edge_ids: BTreeSet<_> = computed
        .proposed
        .edges
        .iter()
        .map(|e| e.id.clone())
        .collect();
    let moves: Vec<_> = expanded_children
        .iter()
        .filter_map(|shape| {
            let target = proposed
                .shapes
                .iter()
                .find(|s| s.id == shape.id.selector())?;
            (target.bounds != shape.bounds).then(|| crate::commands::ShapeBounds {
                diagram_id: diagram_id.clone(),
                shape_id: shape.id.selector(),
                bounds: target.bounds.clone(),
            })
        })
        .collect();
    let mut moved_descendants = BTreeSet::new();
    let mut moved_nested_edges = BTreeSet::new();
    if !moves.is_empty() {
        let prepared = crate::geometry::prepare_move(document, diagram_id, moves)
            .map_err(|message| err("unsupported", message))?;
        if !prepared.warnings.is_empty() {
            return Err(err(
                "routing",
                "Moving the nested scope would require an unqualified route",
            ));
        }
        let crate::commands::Request::Batch { mutations } = prepared.request else {
            return Err(err("unsupported", "Unexpected geometry request"));
        };
        for mutation in mutations {
            match mutation {
                crate::commands::Mutation::MoveShape {
                    shape_id, bounds, ..
                } if !direct_shape_ids.contains(&shape_id) => {
                    if !nested_shapes.contains(&shape_id) {
                        return Err(err("unsupported", "Nested move affects an unrelated shape"));
                    }
                    let target = proposed
                        .shapes
                        .iter_mut()
                        .find(|s| s.id == shape_id)
                        .ok_or_else(|| err("identity", "Nested shape missing"))?;
                    let (dx, dy) = (bounds.x - target.bounds.x, bounds.y - target.bounds.y);
                    target.bounds = bounds;
                    if let Some(label) = &mut target.label_bounds {
                        label.x += dx;
                        label.y += dy;
                    }
                    if !within(&target.bounds)
                        || target
                            .label_bounds
                            .as_ref()
                            .is_some_and(|label| !within(label))
                    {
                        return Err(err(
                            "unsupported",
                            "A nested shape leaves the selected scope",
                        ));
                    }
                    moved_descendants.insert(shape_id);
                }
                crate::commands::Mutation::Reroute {
                    edge_id, waypoints, ..
                } if !direct_edge_ids.contains(&edge_id) => {
                    if !nested_edges.contains(&edge_id) {
                        return Err(err("unsupported", "Nested move affects an unrelated edge"));
                    }
                    let target = proposed
                        .edges
                        .iter_mut()
                        .find(|e| e.id == edge_id)
                        .ok_or_else(|| err("identity", "Nested edge missing"))?;
                    if waypoints.iter().any(|p| !point_within(p)) {
                        return Err(err(
                            "unsupported",
                            "A nested route leaves the selected scope",
                        ));
                    }
                    target.waypoints = waypoints;
                    moved_nested_edges.insert(edge_id);
                }
                crate::commands::Mutation::SetLabelBounds {
                    representation_id,
                    bounds,
                    ..
                } if !direct_edge_ids.contains(&representation_id) => {
                    if !nested_edges.contains(&representation_id) || !within(&bounds) {
                        return Err(err("unsupported", "A nested edge label is unqualified"));
                    }
                    proposed
                        .edges
                        .iter_mut()
                        .find(|e| e.id == representation_id)
                        .ok_or_else(|| err("identity", "Nested edge missing"))?
                        .label_bounds = Some(bounds);
                }
                crate::commands::Mutation::MoveShape { .. }
                | crate::commands::Mutation::Reroute { .. }
                | crate::commands::Mutation::SetLabelBounds { .. } => {}
                _ => return Err(err("unsupported", "Unexpected nested geometry mutation")),
            }
        }
        // A geometrically contained object not found by the semantic move proof
        // cannot be silently left behind when its parent moves.
        for shape_id in &nested_shapes {
            let shape = diagram
                .shapes
                .iter()
                .find(|s| s.id.selector() == *shape_id)
                .ok_or_else(|| err("identity", "Nested shape missing"))?;
            if expanded_children.iter().any(|parent| {
                inside(&shape.bounds, &parent.bounds)
                    && proposed
                        .shapes
                        .iter()
                        .find(|s| s.id == parent.id.selector())
                        .is_some_and(|s| s.bounds != parent.bounds)
            }) && !moved_descendants.contains(shape_id)
            {
                return Err(err("unsupported", "Nested shape ownership is unproven"));
            }
        }
        for edge_id in &nested_edges {
            let edge = diagram
                .edges
                .iter()
                .find(|e| e.id.selector() == *edge_id)
                .ok_or_else(|| err("identity", "Nested edge missing"))?;
            if expanded_children.iter().any(|parent| {
                edge.waypoints.iter().all(|p| {
                    p.x >= parent.bounds.x
                        && p.y >= parent.bounds.y
                        && p.x <= parent.bounds.x + parent.bounds.width
                        && p.y <= parent.bounds.y + parent.bounds.height
                }) && proposed
                    .shapes
                    .iter()
                    .find(|s| s.id == parent.id.selector())
                    .is_some_and(|s| s.bounds != parent.bounds)
            }) && !moved_nested_edges.contains(edge_id)
            {
                return Err(err("unsupported", "Nested edge ownership is unproven"));
            }
        }
    }
    if !valid_geometry(&proposed) {
        return Err(err("invalid_geometry", "Computed geometry exceeds bounds"));
    }
    let mut selected_shape_ids: BTreeSet<_> = computed
        .proposed
        .shapes
        .iter()
        .map(|s| s.id.clone())
        .collect();
    selected_shape_ids.extend(nested_shapes);
    let mut selected_edge_ids: BTreeSet<_> = computed
        .proposed
        .edges
        .iter()
        .map(|e| e.id.clone())
        .collect();
    selected_edge_ids.extend(nested_edges);
    for selected in proposed
        .shapes
        .iter()
        .filter(|s| selected_shape_ids.contains(&s.id))
    {
        if proposed
            .shapes
            .iter()
            .filter(|s| !selected_shape_ids.contains(&s.id) && scope_shape_id != Some(&s.id))
            .any(|other| {
                overlap(&selected.bounds, &other.bounds)
                    || other
                        .label_bounds
                        .as_ref()
                        .is_some_and(|label| overlap(&selected.bounds, label))
            })
        {
            return Err(err(
                "collision",
                "Selected scope overlaps another representation",
            ));
        }
        if proposed
            .edges
            .iter()
            .filter(|e| !selected_edge_ids.contains(&e.id))
            .any(|edge| {
                edge.waypoints
                    .windows(2)
                    .any(|segment| segment_hits(&segment[0], &segment[1], &selected.bounds))
            })
        {
            return Err(err(
                "collision",
                "Another representation's route crosses the selected scope",
            ));
        }
    }
    Ok(LayoutProposal {
        session_id: session_id.into(),
        generation,
        document_id: document.id,
        diagram_id: diagram_id.clone(),
        choreography_scope_shape: scope_shape_id.cloned(),
        original,
        proposed,
    })
}

fn overlap(a: &Bounds, b: &Bounds) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
fn segment_hits(a: &Point, b: &Point, r: &Bounds) -> bool {
    if a.x == b.x {
        a.x > r.x && a.x < r.x + r.width && a.y.max(b.y) > r.y && a.y.min(b.y) < r.y + r.height
    } else if a.y == b.y {
        a.y > r.y && a.y < r.y + r.height && a.x.max(b.x) > r.x && a.x.min(b.x) < r.x + r.width
    } else {
        true
    }
}
/// Build a deterministic proposal without changing the document. Unsupported diagrams fail closed.
pub fn propose_layout(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    session_id: &str,
    generation: u64,
    cancel: &AtomicBool,
) -> Result<LayoutProposal, LayoutError> {
    cancelled(cancel)?;
    if !matches!(document.capability, Capability::Editable) {
        return Err(err(
            "unsupported",
            "Document contains preserved source or extensions",
        ));
    }
    if let Some(diagram) = document
        .diagrams
        .iter()
        .find(|d| &d.id.selector() == diagram_id)
        && crate::queries::choreography_scopes(document)
            .iter()
            .any(|scope| scope.owner == diagram.plane)
    {
        return propose_choreography_scope_impl(
            document, diagram_id, None, session_id, generation, cancel,
        );
    }
    if document.processes.len() != 1 {
        return Err(err("unsupported", "Exactly one process is required"));
    }
    let process = &document.processes[0];
    if !process.data_objects.is_empty()
        || !process.data_associations.is_empty()
        || process
            .nodes
            .iter()
            .any(|node| node.io_specification.is_some())
    {
        return Err(err(
            "unsupported",
            "Data objects, associations and I/O specifications are outside the layout profile",
        ));
    }

    if process.lanes.len() > 64 || process.lanes.iter().any(|l| l.parent.is_some()) {
        return Err(err("unsupported", "Nested or too many lanes"));
    }
    if process.nodes.iter().any(|n| {
        n.details.is_some()
            || !matches!(
                n.kind,
                NodeKind::Task
                    | NodeKind::UserTask
                    | NodeKind::ManualTask
                    | NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::ExclusiveGateway
                    | NodeKind::ParallelGateway
            )
    }) || process.flows.iter().any(|f| f.kind != FlowKind::Sequence)
    {
        return Err(err("unsupported", "Unsupported node or flow"));
    }
    if !document.additional_collaborations.is_empty() {
        return Err(err(
            "unsupported",
            "Multiple collaboration roots are outside the layout profile",
        ));
    }
    let pool = if let Some(c) = &document.collaboration {
        if c.participants.len() != 1
            || !c.flows.is_empty()
            || !c.conversation_nodes.is_empty()
            || !c.conversation_links.is_empty()
            || c.participants[0].process.as_ref()
                != Some(&crate::bpmn_model::EventReference::Local(
                    process.id.selector(),
                ))
        {
            return Err(err("unsupported", "Multiple pools or collaboration flows"));
        }
        Some(c.participants[0].id.selector())
    } else {
        None
    };
    if document
        .diagrams
        .iter()
        .filter(|d| &d.id.selector() == diagram_id)
        .count()
        != 1
    {
        return Err(err(
            "invalid_di",
            "Diagram identity is missing or duplicated",
        ));
    }
    let d = document
        .diagrams
        .iter()
        .find(|d| &d.id.selector() == diagram_id)
        .ok_or_else(|| err("identity", "Diagram not found"))?;
    if d.shapes.len() > 400 || d.edges.len() > 500 || d.shapes.len() + d.edges.len() > 1000 {
        return Err(err("limit", "Diagram exceeds layout limits"));
    }
    if d.plane != process.id.selector()
        && pool.as_ref().is_none_or(|id| &d.plane != id)
        && document
            .collaboration
            .as_ref()
            .is_none_or(|c| d.plane != c.id.selector())
    {
        return Err(err(
            "unsupported",
            "Diagram plane does not identify process or pool",
        ));
    }
    let original = geometry(d);
    if !valid_geometry(&original) {
        return Err(err("invalid_geometry", "Malformed shape or edge geometry"));
    }
    let nodes: BTreeMap<_, _> = process.nodes.iter().map(|n| (n.id.selector(), n)).collect();
    let flows: BTreeMap<_, _> = process.flows.iter().map(|f| (f.id.selector(), f)).collect();
    if nodes.len() != process.nodes.len() || flows.len() != process.flows.len() {
        return Err(err("invalid_di", "Duplicate semantic IDs"));
    }
    let mut shape_by_element = BTreeMap::new();
    for (index, s) in d.shapes.iter().enumerate() {
        if !nodes.contains_key(&s.element)
            && !process.lanes.iter().any(|l| l.id.selector() == s.element)
            && pool.as_ref() != Some(&s.element)
        {
            return Err(err("unsupported", "Unknown diagram shape"));
        }
        if shape_by_element.insert(s.element.clone(), index).is_some() {
            return Err(err("invalid_di", "Duplicate diagram shape mapping"));
        }
    }
    if nodes.keys().any(|id| !shape_by_element.contains_key(id))
        || process
            .lanes
            .iter()
            .any(|l| !shape_by_element.contains_key(&l.id.selector()))
        || pool
            .as_ref()
            .is_some_and(|id| !shape_by_element.contains_key(id))
    {
        return Err(err("invalid_di", "Missing shape mapping"));
    }
    let mut lane_ids = BTreeSet::new();
    if process.lanes.iter().any(|lane| {
        !lane_ids.insert(lane.id.selector())
            || nodes.contains_key(&lane.id.selector())
            || pool.as_ref() == Some(&lane.id.selector())
    }) {
        return Err(err("invalid_di", "Duplicate semantic lane identity"));
    }
    let mut edge_by_element = BTreeMap::new();
    for e in &d.edges {
        if !flows.contains_key(&e.element) || edge_by_element.insert(e.element.clone(), e).is_some()
        {
            return Err(err("invalid_di", "Unknown or duplicate edge mapping"));
        }
    }
    if flows.keys().any(|id| !edge_by_element.contains_key(id)) {
        return Err(err("invalid_di", "Missing edge mapping"));
    }
    let ids: Vec<_> = nodes.keys().cloned().collect();
    let indices: BTreeMap<_, _> = ids
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let n = ids.len();
    let mut graph = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for f in flows.values() {
        let (
            Some(crate::bpmn_model::EventReference::Local(source)),
            Some(crate::bpmn_model::EventReference::Local(target)),
        ) = (&f.source, &f.target)
        else {
            return Err(err("invalid_di", "Flow endpoint is missing or external"));
        };
        let (Some(&a), Some(&b)) = (indices.get(source), indices.get(target)) else {
            return Err(err("invalid_di", "Flow endpoint is missing"));
        };
        graph[a].push(b);
        reverse[b].push(a);
    }
    for list in &mut graph {
        list.sort_unstable();
        list.dedup();
    }
    for list in &mut reverse {
        list.sort_unstable();
        list.dedup();
    }
    // Iterative Kosaraju, with stable sorted traversal and no recursion on imported data.
    let mut seen = vec![false; n];
    let mut order = Vec::new();
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((v, i)) = stack.pop() {
            if i < graph[v].len() {
                stack.push((v, i + 1));
                let w = graph[v][i];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
            }
        }
    }
    cancelled(cancel)?;
    let mut component = vec![usize::MAX; n];
    let mut count = 0;
    for &root in order.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        component[root] = count;
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            for &w in &reverse[v] {
                if component[w] == usize::MAX {
                    component[w] = count;
                    stack.push(w);
                }
            }
        }
        count += 1;
    }
    let mut dag = vec![BTreeSet::new(); count];
    let mut indegree = vec![0; count];
    for v in 0..n {
        for &w in &graph[v] {
            let (a, b) = (component[v], component[w]);
            if a != b && dag[a].insert(b) {
                indegree[b] += 1;
            }
        }
    }
    let mut ready: BTreeSet<_> = (0..count).filter(|&i| indegree[i] == 0).collect();
    let mut rank = vec![0usize; count];
    while let Some(v) = ready.pop_first() {
        for &w in &dag[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                ready.insert(w);
            }
        }
    }
    cancelled(cancel)?;
    let mut membership = BTreeMap::new();
    for (i, lane) in process.lanes.iter().enumerate() {
        for member in &lane.members {
            if !nodes.contains_key(member) || membership.insert(member.clone(), i).is_some() {
                return Err(err("unsupported", "Invalid or overlapping lane membership"));
            }
        }
    }
    let bands = process.lanes.len() + 1;
    let mut cells: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        cells
            .entry((
                membership.get(id).copied().unwrap_or(bands - 1),
                rank[component[i]],
            ))
            .or_default()
            .push(i);
    }
    let max_width = d
        .shapes
        .iter()
        .filter(|s| nodes.contains_key(&s.element))
        .map(|s| s.bounds.width)
        .fold(0.0, f64::max);
    let mut band_heights = vec![120.0_f64; bands];
    for (&(band, _), cell) in &cells {
        let h = cell
            .iter()
            .map(|&i| d.shapes[shape_by_element[&ids[i]]].bounds.height + 80.0)
            .sum::<f64>()
            + 40.0;
        band_heights[band] = band_heights[band].max(h);
    }
    let mut band_y = vec![0.0; bands];
    for i in 1..bands {
        band_y[i] = band_y[i - 1] + band_heights[i - 1];
    }
    let mut result = original.clone();
    let index: BTreeMap<_, _> = result
        .shapes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    for (&(band, layer), cell) in &cells {
        let mut y = band_y[band] + 60.0;
        for &i in cell {
            let shape = &d.shapes[shape_by_element[&ids[i]]];
            let out = &mut result.shapes[index[&shape.id.selector()]];
            let x = 80.0 + layer as f64 * (max_width + 160.0);
            let dx = x - out.bounds.x;
            let dy = y - out.bounds.y;
            out.bounds.x = x;
            out.bounds.y = y;
            if let Some(label) = &mut out.label_bounds {
                label.x += dx;
                label.y += dy;
            }
            y += out.bounds.height + 80.0;
        }
    }
    let width = 160.0 + (rank.iter().copied().max().unwrap_or(0) + 1) as f64 * (max_width + 160.0);
    for (i, lane) in process.lanes.iter().enumerate() {
        let shape = &d.shapes[shape_by_element[&lane.id.selector()]];
        let out = &mut result.shapes[index[&shape.id.selector()]];
        let dx = -out.bounds.x;
        let dy = band_y[i] - out.bounds.y;
        out.bounds = Bounds {
            x: 0.0,
            y: band_y[i],
            width,
            height: band_heights[i],
        };
        if let Some(label) = &mut out.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    if let Some(pool) = pool {
        let shape = &d.shapes[shape_by_element[&pool]];
        let out = &mut result.shapes[index[&shape.id.selector()]];
        let dx = -out.bounds.x;
        let dy = -out.bounds.y;
        out.bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width,
            height: band_heights.iter().sum(),
        };
        if let Some(label) = &mut out.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    cancelled(cancel)?;
    let edge_index: BTreeMap<_, _> = result
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();
    let mut outgoing = BTreeMap::<ElementSelector, Vec<ElementSelector>>::new();
    let mut incoming = BTreeMap::<ElementSelector, Vec<ElementSelector>>::new();
    for flow in flows.values() {
        let (
            Some(crate::bpmn_model::EventReference::Local(source_id)),
            Some(crate::bpmn_model::EventReference::Local(target_id)),
        ) = (&flow.source, &flow.target)
        else {
            return Err(err("invalid_di", "Flow endpoint is missing or external"));
        };
        outgoing
            .entry(source_id.clone())
            .or_default()
            .push(flow.id.selector());
        incoming
            .entry(target_id.clone())
            .or_default()
            .push(flow.id.selector());
    }
    let mut route_work = 0usize;
    const ROUTE_WORK_LIMIT: usize = 500_000;
    for (number, flow) in flows.values().enumerate() {
        cancelled(cancel)?;
        let (
            Some(crate::bpmn_model::EventReference::Local(source_id)),
            Some(crate::bpmn_model::EventReference::Local(target_id)),
        ) = (&flow.source, &flow.target)
        else {
            return Err(err("invalid_di", "Flow endpoint is missing or external"));
        };
        let source =
            &result.shapes[index[&d.shapes[shape_by_element[source_id]].id.selector()]].bounds;
        let target =
            &result.shapes[index[&d.shapes[shape_by_element[target_id]].id.selector()]].bounds;
        let port_y = |id: &ElementSelector, bounds: &Bounds, list: &[ElementSelector]| {
            if matches!(
                nodes[id].kind,
                NodeKind::Task | NodeKind::UserTask | NodeKind::ManualTask
            ) {
                let position = list
                    .iter()
                    .position(|edge| edge == &flow.id.selector())
                    .unwrap_or(0);
                bounds.y + bounds.height * (position + 1) as f64 / (list.len() + 1) as f64
            } else {
                bounds.y + bounds.height / 2.0
            }
        };
        let port =
            |id: &ElementSelector, bounds: &Bounds, list: &[ElementSelector], source: bool| {
                if matches!(
                    nodes[id].kind,
                    NodeKind::Task | NodeKind::UserTask | NodeKind::ManualTask
                ) {
                    Point {
                        x: if source {
                            bounds.x + bounds.width
                        } else {
                            bounds.x
                        },
                        y: port_y(id, bounds, list),
                    }
                } else {
                    let position = list
                        .iter()
                        .position(|edge| edge == &flow.id.selector())
                        .unwrap_or(0);
                    match (source, position % 4) {
                        (true, 0) | (false, 3) => Point {
                            x: bounds.x + bounds.width,
                            y: bounds.y + bounds.height / 2.0,
                        },
                        (false, 0) | (true, 3) => Point {
                            x: bounds.x,
                            y: bounds.y + bounds.height / 2.0,
                        },
                        (true, 2) | (false, 2) => Point {
                            x: bounds.x + bounds.width / 2.0,
                            y: bounds.y + bounds.height,
                        },
                        _ => Point {
                            x: bounds.x + bounds.width / 2.0,
                            y: bounds.y,
                        },
                    }
                }
            };
        let start = port(source_id, source, &outgoing[source_id], true);
        let end = port(target_id, target, &incoming[target_id], false);
        let points = if source_id == target_id {
            let gap = 40.0 + number as f64 * 2.0;
            vec![
                start.clone(),
                Point {
                    x: start.x + gap,
                    y: start.y,
                },
                Point {
                    x: start.x + gap,
                    y: source.y - gap,
                },
                Point {
                    x: source.x - gap,
                    y: source.y - gap,
                },
                Point {
                    x: source.x - gap,
                    y: end.y,
                },
                end.clone(),
            ]
        } else if rank[component[indices[source_id]]] >= rank[component[indices[target_id]]]
            && source_id > target_id
        {
            let gap = 40.0 + number as f64 * 16.0;
            let channel_y = -gap;
            vec![
                start.clone(),
                Point {
                    x: start.x + gap,
                    y: start.y,
                },
                Point {
                    x: start.x + gap,
                    y: channel_y,
                },
                Point {
                    x: end.x - gap,
                    y: channel_y,
                },
                Point {
                    x: end.x - gap,
                    y: end.y,
                },
                end.clone(),
            ]
        } else {
            let mut obstacles: Vec<_> = ids
                .iter()
                .filter(|id| *id != source_id && *id != target_id)
                .map(|id| {
                    result.shapes[index[&d.shapes[shape_by_element[id]].id.selector()]]
                        .bounds
                        .clone()
                })
                .collect();
            // External labels are real obstacles too; routing around them avoids
            // constructing a proposal which the later collision check must reject.
            obstacles.extend(
                result
                    .shapes
                    .iter()
                    .filter_map(|shape| shape.label_bounds.clone()),
            );
            route_work = route_work.saturating_add(obstacles.len());
            if route_work > ROUTE_WORK_LIMIT {
                return Err(err("budget", "Routing work limit exceeded"));
            }
            let straight = start.y == end.y
                && start.x < end.x
                && obstacles.iter().all(|b| {
                    let inflated = Bounds {
                        x: b.x - 8.0,
                        y: b.y - 8.0,
                        width: b.width + 16.0,
                        height: b.height + 16.0,
                    };
                    valid_bounds(&inflated) && !segment_hits(&start, &end, &inflated)
                });
            if straight {
                vec![start, end]
            } else {
                route_work = route_work.saturating_add(4 + 4 * obstacles.len());
                if route_work > ROUTE_WORK_LIMIT {
                    return Err(err("budget", "Routing work limit exceeded"));
                }
                crate::geometry::route(start, end, &obstacles, 8.0, 1000)
                    .map_err(|_| err("routing", "Invalid route geometry"))?
                    .waypoints
                    .ok_or_else(|| err("routing", "No collision-free route within budget"))?
            }
        };
        let original_edge = edge_by_element[&flow.id.selector()];
        let out = &mut result.edges[edge_index[&original_edge.id.selector()]];
        out.waypoints = points;
        if let Some(label) = &mut out.label_bounds {
            let segment = out
                .waypoints
                .windows(2)
                .max_by(|a, b| {
                    let length = |pair: &[Point]| {
                        (pair[0].x - pair[1].x).abs() + (pair[0].y - pair[1].y).abs()
                    };
                    length(a).total_cmp(&length(b))
                })
                .ok_or_else(|| err("routing", "Edge has no label segment"))?;
            let a = &segment[0];
            let b = &segment[1];
            if a.y == b.y {
                label.x = (a.x + b.x - label.width) / 2.0;
                label.y = a.y - label.height - 8.0;
            } else {
                label.x = a.x + 8.0;
                label.y = (a.y + b.y - label.height) / 2.0;
            }
        }
    }
    let proposal = LayoutProposal {
        session_id: session_id.into(),
        generation,
        document_id: document.id,
        diagram_id: diagram_id.clone(),
        choreography_scope_shape: None,
        original,
        proposed: result,
    };
    if !valid_geometry(&proposal.proposed) {
        return Err(err("invalid_geometry", "Computed geometry exceeds bounds"));
    }
    let node_shapes: Vec<_> = ids
        .iter()
        .map(|id| &proposal.proposed.shapes[index[&d.shapes[shape_by_element[id]].id.selector()]])
        .collect();
    for (i, shape) in node_shapes.iter().enumerate() {
        if node_shapes
            .iter()
            .skip(i + 1)
            .any(|other| overlap(&shape.bounds, &other.bounds))
        {
            return Err(err("collision", "Node placement overlaps"));
        }
    }
    let labels: Vec<_> = proposal
        .proposed
        .shapes
        .iter()
        .filter_map(|shape| shape.label_bounds.as_ref())
        .chain(
            proposal
                .proposed
                .edges
                .iter()
                .filter_map(|edge| edge.label_bounds.as_ref()),
        )
        .collect();
    for (i, label) in labels.iter().enumerate() {
        if labels.iter().skip(i + 1).any(|other| overlap(label, other))
            || node_shapes
                .iter()
                .any(|shape| overlap(label, &shape.bounds))
        {
            return Err(err("collision", "Label overlaps another label or node"));
        }
    }
    for edge in &proposal.proposed.edges {
        for segment in edge.waypoints.windows(2) {
            if labels
                .iter()
                .any(|label| segment_hits(&segment[0], &segment[1], label))
            {
                return Err(err("collision", "Route crosses a label"));
            }
        }
    }
    for flow in flows.values() {
        let (
            Some(crate::bpmn_model::EventReference::Local(source_id)),
            Some(crate::bpmn_model::EventReference::Local(target_id)),
        ) = (&flow.source, &flow.target)
        else {
            return Err(err("invalid_di", "Flow endpoint is missing or external"));
        };
        let edge = &proposal.proposed.edges
            [edge_index[&edge_by_element[&flow.id.selector()].id.selector()]];
        for (segment_index, segment) in edge.waypoints.windows(2).enumerate() {
            let source = &proposal.proposed.shapes
                [index[&d.shapes[shape_by_element[source_id]].id.selector()]]
                .bounds;
            let target = &proposal.proposed.shapes
                [index[&d.shapes[shape_by_element[target_id]].id.selector()]]
                .bounds;
            if (segment_index > 0 && segment_hits(&segment[0], &segment[1], source))
                || (segment_index + 2 < edge.waypoints.len()
                    && segment_hits(&segment[0], &segment[1], target))
            {
                return Err(err("routing", "Route re-enters an endpoint node"));
            }
            if ids
                .iter()
                .filter(|id| *id != source_id && *id != target_id)
                .any(|id| {
                    segment_hits(
                        &segment[0],
                        &segment[1],
                        &proposal.proposed.shapes
                            [index[&d.shapes[shape_by_element[id]].id.selector()]]
                            .bounds,
                    )
                })
            {
                return Err(err("routing", "Route crosses a node"));
            }
        }
    }
    for (index, edge) in proposal.proposed.edges.iter().enumerate() {
        for other in proposal.proposed.edges.iter().skip(index + 1) {
            for a in edge.waypoints.windows(2) {
                for b in other.waypoints.windows(2) {
                    route_work += 1;
                    if route_work > ROUTE_WORK_LIMIT {
                        return Err(err("budget", "Routing work limit exceeded"));
                    }
                    let shared = if a[0].x == a[1].x && b[0].x == b[1].x && a[0].x == b[0].x {
                        a[0].y.min(a[1].y).max(b[0].y.min(b[1].y))
                            < a[0].y.max(a[1].y).min(b[0].y.max(b[1].y))
                    } else if a[0].y == a[1].y && b[0].y == b[1].y && a[0].y == b[0].y {
                        a[0].x.min(a[1].x).max(b[0].x.min(b[1].x))
                            < a[0].x.max(a[1].x).min(b[0].x.max(b[1].x))
                    } else {
                        false
                    };
                    if shared {
                        return Err(err("routing", "Routes share a channel"));
                    }
                    let crosses = if a[0].x == a[1].x && b[0].y == b[1].y {
                        a[0].x > b[0].x.min(b[1].x)
                            && a[0].x < b[0].x.max(b[1].x)
                            && b[0].y > a[0].y.min(a[1].y)
                            && b[0].y < a[0].y.max(a[1].y)
                    } else if a[0].y == a[1].y && b[0].x == b[1].x {
                        b[0].x > a[0].x.min(a[1].x)
                            && b[0].x < a[0].x.max(a[1].x)
                            && a[0].y > b[0].y.min(b[1].y)
                            && a[0].y < b[0].y.max(b[1].y)
                    } else {
                        false
                    };
                    if crosses {
                        return Err(err("routing", "Routes cross"));
                    }
                }
            }
        }
    }
    Ok(proposal)
}
