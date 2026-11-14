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
    /// Counted quality defects of an otherwise valid proposal (one per code).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<LayoutWarning>,
    #[serde(default)]
    pub metrics: LayoutMetrics,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LayoutWarning {
    pub code: String,
    pub message: String,
}
/// Measured quality of the proposed geometry. `overlaps` counts labels touching
/// labels or nodes and route segments passing over labels or nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutMetrics {
    pub crossings: usize,
    pub overlaps: usize,
    pub bends: usize,
    pub edge_length: f64,
    pub area: f64,
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
        lane_set_name: None,
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
        warnings: computed.warnings,
        metrics: computed.metrics,
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
        warnings: computed.warnings,
        metrics: computed.metrics,
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
    // Every flow node is accepted except artifacts, data references and attached
    // events; expanded subprocesses are refused once their shapes are known.
    if process.nodes.iter().any(|n| {
        matches!(
            n.kind,
            NodeKind::TextAnnotation
                | NodeKind::DataObjectReference
                | NodeKind::DataStoreReference
                | NodeKind::BoundaryEvent
        ) || matches!(
            n.details,
            Some(
                crate::bpmn_model::NodeDetails::BoundaryEvent { .. }
                    | crate::bpmn_model::NodeDetails::Event {
                        attached_to: Some(_),
                        ..
                    }
            )
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
    if process.nodes.iter().any(|n| {
        matches!(
            n.kind,
            NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
        ) && d.shapes[shape_by_element[&n.id.selector()]].is_expanded == Some(true)
    }) {
        return Err(err(
            "unsupported",
            "Expanded subprocesses are outside the layout profile",
        ));
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
    // Rank by longest path after removing DFS back-edges (iterative, stable sorted
    // traversal, no recursion on imported data). Sources are visited first so loops
    // keep their entry node on the left and draw the returning edge as the back-edge.
    let mut state = vec![0u8; n];
    let mut back = BTreeSet::new();
    let roots = (0..n)
        .filter(|&v| reverse[v].is_empty())
        .chain(0..n)
        .collect::<Vec<_>>();
    for root in roots {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some((v, i)) = stack.pop() {
            if i < graph[v].len() {
                stack.push((v, i + 1));
                let w = graph[v][i];
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => {
                        back.insert((v, w));
                    }
                    _ => {}
                }
            } else {
                state[v] = 2;
            }
        }
    }
    cancelled(cancel)?;
    let mut indegree = vec![0; n];
    for (v, targets) in graph.iter().enumerate() {
        for &w in targets {
            if !back.contains(&(v, w)) {
                indegree[w] += 1;
            }
        }
    }
    let mut ready: BTreeSet<_> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut rank = vec![0usize; n];
    while let Some(v) = ready.pop_first() {
        for &w in &graph[v] {
            if back.contains(&(v, w)) {
                continue;
            }
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
    let band_of = |i: usize| membership.get(&ids[i]).copied().unwrap_or(bands - 1);
    let lay = Layered::new(
        &graph,
        &reverse,
        &back,
        rank,
        (0..n).map(band_of).collect(),
        cancel,
    )?;
    let rank = lay.layer[..n].to_vec();
    let layers = lay.order.len();
    let key = |v: usize| (lay.band[v], lay.comp[v], lay.row[v]);
    let shape_of = |i: usize| &d.shapes[shape_by_element[&ids[i]]];
    // Back-edges run in a channel over the top of their component block in the
    // upper band, or under it in the lower band when fewer vertices stand in the
    // way. Channels are assigned innermost-first by column span so nested loops
    // never cross, and each block's margins grow with its channel count. Every
    // inter-layer segment or back-edge end may need a vertical leg in a gap.
    let mut legs = vec![0usize; layers];
    let mut planned = Vec::new();
    for f in flows.values() {
        let (
            Some(crate::bpmn_model::EventReference::Local(s)),
            Some(crate::bpmn_model::EventReference::Local(t)),
        ) = (&f.source, &f.target)
        else {
            continue;
        };
        let (a, b) = (indices[s], indices[t]);
        if a == b {
            continue;
        }
        if rank[a] < rank[b] {
            for pair in lay.chains[&(a, b)].windows(2) {
                if key(pair[0]) != key(pair[1]) || pair[0] < n || pair[1] < n {
                    legs[lay.layer[pair[0]]] += 1;
                }
            }
            continue;
        }
        legs[rank[a]] += 1;
        if rank[b] > 0 {
            legs[rank[b] - 1] += 1;
        }
        let bottom = !lay.channel_top(a, b);
        let band = if bottom {
            band_of(a).max(band_of(b))
        } else {
            band_of(a).min(band_of(b))
        };
        planned.push((
            bottom,
            (band, lay.comp[a]),
            rank[a] - rank[b],
            f.id.selector(),
        ));
    }
    planned.sort_by_key(|(bottom, block, span, _)| (*bottom, *block, *span));
    let mut channel_count = BTreeMap::<(usize, usize), [usize; 2]>::new();
    let mut loop_plan = BTreeMap::new();
    for (bottom, block, _, flow) in planned {
        let count = &mut channel_count.entry(block).or_default()[usize::from(bottom)];
        loop_plan.insert(flow, (bottom, block, *count));
        *count += 1;
    }
    // Each node with its external label spans (left, right, up, down) around its
    // centre. Columns and rows take the widest and tallest of their members;
    // dummy-only columns are empty and dummy-only rows 16 high.
    let mut column = vec![[0.0_f64; 2]; layers];
    let mut row_extent = BTreeMap::<(usize, usize, usize), [f64; 2]>::new();
    for v in 0..lay.layer.len() {
        let e = if v < n {
            let b = &shape_of(v).bounds;
            let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
            let half = [b.width / 2.0, b.width / 2.0, b.height / 2.0, b.height / 2.0];
            match shape_of(v).label.as_ref().and_then(|l| l.bounds.as_ref()) {
                Some(l) => [
                    half[0].max(cx - l.x),
                    half[1].max(l.x + l.width - cx),
                    half[2].max(cy - l.y),
                    half[3].max(l.y + l.height - cy),
                ],
                None => half,
            }
        } else {
            [0.0, 0.0, 8.0, 8.0]
        };
        let c = &mut column[lay.layer[v]];
        (c[0], c[1]) = (c[0].max(e[0]), c[1].max(e[1]));
        let r = row_extent.entry(key(v)).or_default();
        (r[0], r[1]) = (r[0].max(e[2]), r[1].max(e[3]));
    }
    // Inside a pool, lanes start after the pool header and nodes move with them.
    let lane_x = if pool.is_some() {
        crate::containers::LANE_INDENTATION
    } else {
        0.0
    };
    let mut col_x = vec![80.0 + lane_x; layers];
    for l in 1..layers {
        col_x[l] = col_x[l - 1]
            + column[l - 1][0]
            + column[l - 1][1]
            + (24.0 + 8.0 * legs[l - 1] as f64).max(60.0);
    }
    let width = layers
        .checked_sub(1)
        .map_or(80.0 + lane_x, |l| col_x[l] + column[l][0] + column[l][1])
        + 80.0;
    // Bands stack their component blocks; a block's margins hold its channels
    // (top channel k sits 12 + 8k above its rows, bottom channel k as far below).
    let mut row_y = BTreeMap::new();
    let mut block_rows = BTreeMap::new();
    let mut band_y = Vec::with_capacity(bands + 1);
    let mut y = 0.0_f64;
    for band in 0..bands {
        let start = y;
        band_y.push(start);
        let comps: BTreeSet<_> = row_extent
            .keys()
            .filter(|k| k.0 == band)
            .map(|k| k.1)
            .collect();
        for &comp in &comps {
            let [top, bottom] = channel_count
                .get(&(band, comp))
                .copied()
                .unwrap_or_default();
            y += (20.0 + 8.0 * top as f64).max(60.0);
            let rows_top = y;
            for (&k, e) in row_extent.range((band, comp, 0)..=(band, comp, usize::MAX)) {
                row_y.insert(k, y + e[0]);
                y += e[0] + e[1] + 40.0;
            }
            y -= 40.0;
            block_rows.insert((band, comp), (rows_top, y));
            y += (20.0 + 8.0 * bottom as f64).max(40.0);
        }
        // An empty unlaned band beside lanes takes no room.
        if !comps.is_empty() || bands == 1 || band + 1 < bands {
            y = y.max(start + 120.0);
        }
    }
    band_y.push(y);
    let mut result = original.clone();
    let index: BTreeMap<_, _> = result
        .shapes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    for i in 0..n {
        let out = &mut result.shapes[index[&shape_of(i).id.selector()]];
        let x = col_x[rank[i]] + column[rank[i]][0] - out.bounds.width / 2.0;
        let y = row_y[&key(i)] - out.bounds.height / 2.0;
        let dx = x - out.bounds.x;
        let dy = y - out.bounds.y;
        out.bounds.x = x;
        out.bounds.y = y;
        if let Some(label) = &mut out.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    for (i, lane) in process.lanes.iter().enumerate() {
        let shape = &d.shapes[shape_by_element[&lane.id.selector()]];
        let out = &mut result.shapes[index[&shape.id.selector()]];
        let dx = lane_x - out.bounds.x;
        let dy = band_y[i] - out.bounds.y;
        out.bounds = Bounds {
            x: lane_x,
            y: band_y[i],
            width: width - lane_x,
            height: band_y[i + 1] - band_y[i],
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
            height: band_y[bands],
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
    // Event and gateway ports face their partner: aligned partners use the
    // horizontal side, others the top or bottom vertex. Back-edges (rank does not
    // increase) leave and enter over the top. Each side serves one flow per node.
    let bounds_of = |id: &ElementSelector| {
        result.shapes[index[&d.shapes[shape_by_element[id]].id.selector()]]
            .bounds
            .clone()
    };
    let is_task = |id: &ElementSelector| nodes[id].kind.is_activity();
    let is_back = |s: &ElementSelector, t: &ElementSelector| rank[indices[s]] >= rank[indices[t]];
    let mut sides = BTreeMap::<(ElementSelector, bool), Point>::new();
    let mut used = BTreeMap::<ElementSelector, Vec<u8>>::new();
    for back_pass in [true, false] {
        for flow in flows.values() {
            let (
                Some(crate::bpmn_model::EventReference::Local(s)),
                Some(crate::bpmn_model::EventReference::Local(t)),
            ) = (&flow.source, &flow.target)
            else {
                return Err(err("invalid_di", "Flow endpoint is missing or external"));
            };
            if s == t || is_back(s, t) != back_pass {
                continue;
            }
            for (own, other, source) in [(s, t, true), (t, s, false)] {
                if !back_pass && is_task(own) {
                    continue;
                }
                let (b, o) = (bounds_of(own), bounds_of(other));
                let (cy, ocy) = (b.y + b.height / 2.0, o.y + o.height / 2.0);
                // 0 top, 1 right, 2 bottom, 3 left.
                let horizontal = if source { 1 } else { 3 };
                let preferred = if back_pass {
                    if loop_plan[&flow.id.selector()].0 {
                        2
                    } else {
                        0
                    }
                } else if (ocy - cy).abs() < 0.5 {
                    horizontal
                } else if ocy > cy {
                    2
                } else {
                    0
                };
                // An external label below (or above) the node blocks that vertex.
                let taken = used.entry(own.clone()).or_insert_with(|| {
                    let label = result.shapes
                        [index[&d.shapes[shape_by_element[own]].id.selector()]]
                        .label_bounds
                        .as_ref();
                    match label {
                        Some(l) if l.y >= b.y + b.height / 2.0 => vec![2],
                        Some(_) => vec![0],
                        None => vec![],
                    }
                });
                let side = [preferred, horizontal, 2, 0]
                    .into_iter()
                    .find(|side| !taken.contains(side))
                    .unwrap_or(preferred);
                taken.push(side);
                let point = match side {
                    0 => Point {
                        x: b.x + b.width / 2.0,
                        y: b.y,
                    },
                    1 => Point {
                        x: b.x + b.width,
                        y: cy,
                    },
                    2 => Point {
                        x: b.x + b.width / 2.0,
                        y: b.y + b.height,
                    },
                    _ => Point { x: b.x, y: cy },
                };
                sides.insert((flow.id.selector(), source), point);
            }
        }
    }
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
            if nodes[id].kind.is_activity() {
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
                if nodes[id].kind.is_activity() {
                    Point {
                        x: if source {
                            bounds.x + bounds.width
                        } else {
                            bounds.x
                        },
                        y: port_y(id, bounds, list),
                    }
                } else {
                    // Only self-loops have no assigned side; they use the horizontal sides.
                    sides
                        .get(&(flow.id.selector(), source))
                        .cloned()
                        .unwrap_or(Point {
                            x: if source {
                                bounds.x + bounds.width
                            } else {
                                bounds.x
                            },
                            y: bounds.y + bounds.height / 2.0,
                        })
                }
            };
        let back = source_id != target_id && is_back(source_id, target_id);
        let start = match sides.get(&(flow.id.selector(), true)) {
            Some(point) => point.clone(),
            None => port(source_id, source, &outgoing[source_id], true),
        };
        let end = match sides.get(&(flow.id.selector(), false)) {
            Some(point) => point.clone(),
            None => port(target_id, target, &incoming[target_id], false),
        };
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
        } else if back {
            let (bottom, block, k) = loop_plan[&flow.id.selector()];
            let (rows_top, rows_bottom) = block_rows[&block];
            let channel_y = if bottom {
                rows_bottom + 12.0 + 8.0 * k as f64
            } else {
                rows_top - 12.0 - 8.0 * k as f64
            };
            vec![
                start.clone(),
                Point {
                    x: start.x,
                    y: channel_y,
                },
                Point {
                    x: end.x,
                    y: channel_y,
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
    let shape_at = |id: &ElementSelector| index[&d.shapes[shape_by_element[id]].id.selector()];
    let node_shapes: Vec<_> = ids.iter().map(&shape_at).collect();
    let mut endpoints = Vec::with_capacity(flows.len());
    for flow in flows.values() {
        let (
            Some(crate::bpmn_model::EventReference::Local(source_id)),
            Some(crate::bpmn_model::EventReference::Local(target_id)),
        ) = (&flow.source, &flow.target)
        else {
            return Err(err("invalid_di", "Flow endpoint is missing or external"));
        };
        endpoints.push((
            edge_index[&edge_by_element[&flow.id.selector()].id.selector()],
            shape_at(source_id),
            shape_at(target_id),
        ));
    }
    let (warnings, metrics) = measure(&result, &node_shapes, &endpoints)?;
    Ok(LayoutProposal {
        session_id: session_id.into(),
        generation,
        document_id: document.id,
        diagram_id: diagram_id.clone(),
        choreography_scope_shape: None,
        original,
        proposed: result,
        warnings,
        metrics,
    })
}

/// Layered graph used for ordering and rows. Real vertices keep their indices
/// `0..n`; each forward edge gets one dummy vertex per layer it crosses, in its
/// source's band and component. Rows are numbered per (band, component) block.
struct Layered {
    real: usize,
    layer: Vec<usize>,
    band: Vec<usize>,
    comp: Vec<usize>,
    row: Vec<usize>,
    order: Vec<Vec<usize>>,
    pred: Vec<Vec<usize>>,
    succ: Vec<Vec<usize>>,
    /// Vertex chain `[source, dummies.., target]` of every forward node pair.
    chains: BTreeMap<(usize, usize), Vec<usize>>,
}
const MAX_LAYERED_VERTICES: usize = 20_000;
impl Layered {
    fn new(
        graph: &[Vec<usize>],
        reverse: &[Vec<usize>],
        back: &BTreeSet<(usize, usize)>,
        mut layer: Vec<usize>,
        mut band: Vec<usize>,
        cancel: &AtomicBool,
    ) -> Result<Self, LayoutError> {
        let n = graph.len();
        let forward = |v: usize, w: usize| v != w && !back.contains(&(v, w));
        // Pull each source next to its nearest successor.
        for v in 0..n {
            if !reverse[v].iter().any(|&u| forward(u, v))
                && let Some(m) = graph[v]
                    .iter()
                    .filter(|&&w| forward(v, w))
                    .map(|&w| layer[w])
                    .min()
            {
                layer[v] = m - 1;
            }
        }
        // Weakly connected components, numbered by their smallest member.
        let mut comp = vec![usize::MAX; n];
        let mut count = 0;
        for root in 0..n {
            if comp[root] != usize::MAX
                || graph[root].iter().chain(&reverse[root]).all(|&w| w == root)
            {
                continue;
            }
            comp[root] = count;
            let mut stack = vec![root];
            while let Some(v) = stack.pop() {
                for &w in graph[v].iter().chain(&reverse[v]) {
                    if comp[w] == usize::MAX {
                        comp[w] = count;
                        stack.push(w);
                    }
                }
            }
            count += 1;
        }
        // Unconnected nodes form a final pseudo-component packed in a square grid.
        let isolated: Vec<_> = (0..n).filter(|&v| comp[v] == usize::MAX).collect();
        let side = (1usize..).find(|k| k * k >= isolated.len()).unwrap_or(1);
        for (k, &v) in isolated.iter().enumerate() {
            comp[v] = count;
            layer[v] = k % side;
        }
        let (mut pred, mut succ) = (vec![Vec::new(); n], vec![Vec::new(); n]);
        let mut chains = BTreeMap::new();
        for v in 0..n {
            for &w in graph[v].iter().filter(|&&w| forward(v, w)) {
                let mut chain = vec![v];
                for l in layer[v] + 1..layer[w] {
                    if layer.len() >= MAX_LAYERED_VERTICES {
                        return Err(err("budget", "Layered graph exceeds the vertex limit"));
                    }
                    chain.push(layer.len());
                    layer.push(l);
                    band.push(band[v]);
                    comp.push(comp[v]);
                    pred.push(Vec::new());
                    succ.push(Vec::new());
                }
                chain.push(w);
                for pair in chain.windows(2) {
                    succ[pair[0]].push(pair[1]);
                    pred[pair[1]].push(pair[0]);
                }
                chains.insert((v, w), chain);
            }
        }
        let layers = layer.iter().max().map_or(0, |m| m + 1);
        let mut order = vec![Vec::new(); layers];
        for (v, &l) in layer.iter().enumerate() {
            order[l].push(v);
        }
        let mut lay = Layered {
            real: n,
            row: vec![0; layer.len()],
            layer,
            band,
            comp,
            order,
            pred,
            succ,
            chains,
        };
        lay.sort_layers(cancel)?;
        lay.align_rows();
        Ok(lay)
    }
    fn group(&self, v: usize) -> (usize, usize) {
        (self.band[v], self.comp[v])
    }
    /// Barycenter sweeps plus adjacent transpositions inside each (band,
    /// component) group, keeping the order with the fewest crossings.
    fn sort_layers(&mut self, cancel: &AtomicBool) -> Result<(), LayoutError> {
        let mut pos = vec![0; self.layer.len()];
        for list in &mut self.order {
            list.sort_by_key(|&v| (self.band[v], self.comp[v], v));
            for (i, &v) in list.iter().enumerate() {
                pos[v] = i;
            }
        }
        let mut best = (crossings(&self.order, &pos, &self.pred), self.order.clone());
        let layers = self.order.len();
        for _ in 0..4 {
            cancelled(cancel)?;
            for down in [true, false] {
                let nbrs = if down { &self.pred } else { &self.succ };
                let seq: Vec<usize> = if down {
                    (1..layers).collect()
                } else {
                    (0..layers.saturating_sub(1)).rev().collect()
                };
                for l in seq {
                    let mut keyed: Vec<_> = self.order[l]
                        .iter()
                        .map(|&v| {
                            let bary = if nbrs[v].is_empty() {
                                pos[v] as f64
                            } else {
                                nbrs[v].iter().map(|&u| pos[u] as f64).sum::<f64>()
                                    / nbrs[v].len() as f64
                            };
                            (self.group(v), bary, v)
                        })
                        .collect();
                    keyed.sort_by(|a, b| {
                        a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2))
                    });
                    self.order[l] = keyed.into_iter().map(|(_, _, v)| v).collect();
                    for (i, &v) in self.order[l].iter().enumerate() {
                        pos[v] = i;
                    }
                }
            }
            for l in 0..layers {
                for i in 1..self.order[l].len() {
                    let (u, v) = (self.order[l][i - 1], self.order[l][i]);
                    let pair = |a: usize, b: usize| -> usize {
                        [&self.pred, &self.succ]
                            .iter()
                            .map(|nb| {
                                nb[a]
                                    .iter()
                                    .map(|&x| nb[b].iter().filter(|&&y| pos[x] > pos[y]).count())
                                    .sum::<usize>()
                            })
                            .sum()
                    };
                    if self.group(u) == self.group(v) && pair(v, u) < pair(u, v) {
                        self.order[l].swap(i - 1, i);
                        (pos[u], pos[v]) = (i, i - 1);
                    }
                }
            }
            let count = crossings(&self.order, &pos, &self.pred);
            if count < best.0 {
                best = (count, self.order.clone());
            }
        }
        self.order = best.1;
        Ok(())
    }
    /// Integer rows inside each (band, component) block: a left-to-right pass
    /// towards the lower median of same-band predecessors, then a right-to-left
    /// pass towards successors, so splits, upper branches and joins share a row.
    /// Dummies follow their chain predecessor. Rows are then renumbered densely.
    fn align_rows(&mut self) {
        let mut row = vec![0i64; self.layer.len()];
        for list in &self.order {
            for group in list.chunk_by(|&a, &b| self.group(a) == self.group(b)) {
                for (i, &v) in group.iter().enumerate() {
                    row[v] = i as i64;
                }
            }
        }
        let layers = self.order.len();
        for down in [true, false] {
            let seq: Vec<usize> = if down {
                (0..layers).collect()
            } else {
                (0..layers).rev().collect()
            };
            for l in seq {
                for group in self.order[l].chunk_by(|&a, &b| self.group(a) == self.group(b)) {
                    let desired: Vec<i64> = group
                        .iter()
                        .map(|&v| {
                            let nbrs = if down || self.is_dummy(v) {
                                &self.pred[v]
                            } else {
                                &self.succ[v]
                            };
                            let mut rows: Vec<i64> = nbrs
                                .iter()
                                .filter(|&&u| self.band[u] == self.band[v])
                                .map(|&u| row[u])
                                .collect();
                            rows.sort_unstable();
                            rows.get(rows.len().saturating_sub(1) / 2)
                                .copied()
                                .unwrap_or(row[v])
                        })
                        .collect();
                    for (&v, r) in group.iter().zip(fit_rows(&desired)) {
                        row[v] = r;
                    }
                }
            }
        }
        let mut used = BTreeMap::<(usize, usize), BTreeSet<i64>>::new();
        for (v, &r) in row.iter().enumerate() {
            used.entry(self.group(v)).or_default().insert(r);
        }
        for (v, &r) in row.iter().enumerate() {
            self.row[v] = used[&self.group(v)].range(..r).count();
        }
    }
    fn is_dummy(&self, v: usize) -> bool {
        v >= self.real
    }
    /// Vertices in an endpoint's layer between it and the top (or bottom) edge
    /// of the channel's block in band `limit`.
    fn between(&self, e: usize, top: bool, limit: usize) -> usize {
        let key = |v: usize| (self.band[v], self.comp[v], self.row[v]);
        self.order[self.layer[e]]
            .iter()
            .filter(|&&w| {
                w != e
                    && if top {
                        key(w) < key(e) && key(w) >= (limit, self.comp[e], 0)
                    } else {
                        key(w) > key(e) && key(w) <= (limit, self.comp[e], usize::MAX)
                    }
            })
            .count()
    }
    /// A back edge runs above unless fewer vertices block the way down (ties go up).
    fn channel_top(&self, a: usize, b: usize) -> bool {
        let (lo, hi) = (
            self.band[a].min(self.band[b]),
            self.band[a].max(self.band[b]),
        );
        self.between(a, true, lo) + self.between(b, true, lo)
            <= self.between(a, false, hi) + self.between(b, false, hi)
    }
}
/// Crossings between adjacent layers, counted as inversions with a Fenwick tree.
fn crossings(order: &[Vec<usize>], pos: &[usize], pred: &[Vec<usize>]) -> usize {
    let mut total = 0;
    for list in order {
        let mut pairs: Vec<(usize, usize)> = list
            .iter()
            .flat_map(|&v| pred[v].iter().map(move |&u| (pos[u], pos[v])))
            .collect();
        pairs.sort_unstable();
        let mut tree = vec![0usize; list.len() + 1];
        for (k, &(_, b)) in pairs.iter().enumerate() {
            let (mut i, mut at_most) = (b + 1, 0);
            while i > 0 {
                at_most += tree[i];
                i &= i - 1;
            }
            total += k - at_most;
            let mut i = b + 1;
            while i < tree.len() {
                tree[i] += 1;
                i += i.isolate_lowest_one();
            }
        }
    }
    total
}
/// Strictly increasing rows closest (in L1) to `desired`: pool adjacent
/// violators of `desired[i] - i`, each pool taking its upper median.
fn fit_rows(desired: &[i64]) -> Vec<i64> {
    let mut pools: Vec<(Vec<i64>, i64)> = Vec::new();
    for (i, &d) in desired.iter().enumerate() {
        let mut values = vec![d - i as i64];
        loop {
            let mut sorted = values.clone();
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];
            match pools.last() {
                Some((_, m)) if *m > median => {
                    if let Some((previous, _)) = pools.pop() {
                        values.extend(previous);
                    }
                }
                _ => {
                    pools.push((values, median));
                    break;
                }
            }
        }
    }
    let mut rows = Vec::with_capacity(desired.len());
    for (values, median) in pools {
        for _ in values {
            rows.push(median + rows.len() as i64);
        }
    }
    rows
}

/// Final proof of a computed layout. Invalid geometry and overlapping nodes refuse
/// the proposal; every other defect becomes one counted warning per code. The
/// pairwise checks are bounded by the shape and edge limits checked on entry.
/// `nodes` are flow-node shape indices and `endpoints` are
/// `(edge, source shape, target shape)` indices into `g`.
fn measure(
    g: &LayoutGeometry,
    nodes: &[usize],
    endpoints: &[(usize, usize, usize)],
) -> Result<(Vec<LayoutWarning>, LayoutMetrics), LayoutError> {
    if !valid_geometry(g) {
        return Err(err("invalid_geometry", "Computed geometry exceeds bounds"));
    }
    let node_bounds: Vec<_> = nodes.iter().map(|&i| &g.shapes[i].bounds).collect();
    for (i, a) in node_bounds.iter().enumerate() {
        if node_bounds[i + 1..].iter().any(|b| overlap(a, b)) {
            return Err(err("collision", "Node placement overlaps"));
        }
    }
    let labels: Vec<_> = g
        .shapes
        .iter()
        .filter_map(|shape| shape.label_bounds.as_ref())
        .chain(g.edges.iter().filter_map(|edge| edge.label_bounds.as_ref()))
        .collect();
    let mut label_overlaps = 0;
    for (i, label) in labels.iter().enumerate() {
        label_overlaps += labels[i + 1..]
            .iter()
            .filter(|other| overlap(label, other))
            .count()
            + node_bounds.iter().filter(|b| overlap(label, b)).count();
    }
    let mut over_label = 0;
    for edge in &g.edges {
        for s in edge.waypoints.windows(2) {
            over_label += labels
                .iter()
                .filter(|label| segment_hits(&s[0], &s[1], label))
                .count();
        }
    }
    let mut over_node = 0;
    for &(edge, source, target) in endpoints {
        let points = &g.edges[edge].waypoints;
        for (k, s) in points.windows(2).enumerate() {
            let hits = |shape: usize| segment_hits(&s[0], &s[1], &g.shapes[shape].bounds);
            // Leaving the source or entering the target is not a re-entry.
            over_node +=
                usize::from((k > 0 && hits(source)) || (k + 2 < points.len() && hits(target)));
            over_node += nodes
                .iter()
                .filter(|&&n| n != source && n != target && hits(n))
                .count();
        }
    }
    let (mut shared_channels, mut crossings) = (0, 0);
    for (i, edge) in g.edges.iter().enumerate() {
        for other in &g.edges[i + 1..] {
            for a in edge.waypoints.windows(2) {
                for b in other.waypoints.windows(2) {
                    let shared = if a[0].x == a[1].x && b[0].x == b[1].x && a[0].x == b[0].x {
                        a[0].y.min(a[1].y).max(b[0].y.min(b[1].y))
                            < a[0].y.max(a[1].y).min(b[0].y.max(b[1].y))
                    } else if a[0].y == a[1].y && b[0].y == b[1].y && a[0].y == b[0].y {
                        a[0].x.min(a[1].x).max(b[0].x.min(b[1].x))
                            < a[0].x.max(a[1].x).min(b[0].x.max(b[1].x))
                    } else {
                        false
                    };
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
                    shared_channels += usize::from(shared);
                    crossings += usize::from(crosses);
                }
            }
        }
    }
    let warnings = [
        ("label_overlap", label_overlaps, "label overlaps"),
        (
            "route_over_label",
            over_label,
            "route segments over a label",
        ),
        ("route_over_node", over_node, "route segments over a node"),
        (
            "shared_channel",
            shared_channels,
            "route segments sharing a channel",
        ),
        ("crossings", crossings, "route crossings"),
    ]
    .into_iter()
    .filter(|(_, count, _)| *count > 0)
    .map(|(code, count, what)| LayoutWarning {
        code: code.into(),
        message: format!("{count} {what}"),
    })
    .collect();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for b in g.shapes.iter().map(|s| &s.bounds) {
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.width);
        max_y = max_y.max(b.y + b.height);
    }
    let metrics = LayoutMetrics {
        crossings,
        overlaps: label_overlaps + over_label + over_node,
        bends: g.edges.iter().map(|e| e.waypoints.len() - 2).sum(),
        edge_length: g
            .edges
            .iter()
            .flat_map(|e| e.waypoints.windows(2))
            .map(|s| (s[1].x - s[0].x).hypot(s[1].y - s[0].y))
            .sum(),
        area: if g.shapes.is_empty() {
            0.0
        } else {
            (max_x - min_x) * (max_y - min_y)
        },
    };
    Ok((warnings, metrics))
}
