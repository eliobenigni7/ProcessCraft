use crate::{DiagramScene, RenderError, TextEngine, TextRequest};
use processcraft_core::{
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyLoopType,
        ConversationKind, ConversationLink, ConversationNode,
    },
    bpmn_data::{DataAssociation, DataObject, DataPort},
    bpmn_model::{
        AssociationDirection, EventDefinition, EventGatewayType, LoopCharacteristics, NodeDetails,
    },
    ids::{ElementKey, ElementSelector},
    model::{Bounds, BpmnDocument, Capability, Flow, FlowKind, Lane, Node, NodeKind, Point, Style},
};
use tiny_skia::PathBuilder;

/// Pool/lane header band width, shared with the modeling rules (bpmn.io and most
/// modelers place lanes 30 px inside the pool edge).
const HEADER: f64 = processcraft_core::containers::LANE_INDENTATION;
const DI_EXTENSION_DIAGNOSTIC: &str = "DI extensions/foreign attributes are preserved in BPMN; their visual effects are not interpreted.";

fn has_uninterpreted_di(diagram: &processcraft_core::model::Diagram) -> bool {
    use processcraft_core::bpmn_xml::XmlNode;
    let opaque = |metadata: Option<&processcraft_core::model::DiElementMetadata>| {
        metadata.is_some_and(|m| {
            !m.foreign_attributes.is_empty()
                || m.extension.as_ref().is_some_and(|xml| {
                    xml.nodes.iter().any(|node| {
                        matches!(
                            node,
                            XmlNode::Element { .. } | XmlNode::ProcessingInstruction { .. }
                        )
                    })
                })
        })
    };
    opaque(diagram.plane_metadata.as_ref())
        || diagram.shapes.iter().any(|s| {
            opaque(s.di_metadata.as_ref())
                || s.label
                    .as_ref()
                    .is_some_and(|l| opaque(l.di_metadata.as_ref()))
        })
        || diagram.edges.iter().any(|e| {
            opaque(e.di_metadata.as_ref())
                || e.label
                    .as_ref()
                    .is_some_and(|l| opaque(l.di_metadata.as_ref()))
        })
}

struct SemanticIndex<'a> {
    references: processcraft_core::reference_resolution::ReferenceIndex,
    participants: Vec<&'a processcraft_core::model::Participant>,
    groups: Vec<&'a processcraft_core::bpmn_model::Group>,
    nodes: Vec<(&'a Node, usize)>,
    lanes: Vec<(&'a Lane, usize)>,
    flows: Vec<&'a Flow>,
    parents: std::collections::BTreeMap<ElementSelector, ElementSelector>,
    conversations: Vec<&'a ConversationNode>,
    conversation_links: Vec<&'a ConversationLink>,
    data_associations: Vec<&'a DataAssociation>,
    data_objects: Vec<&'a DataObject>,
    ports: Vec<(&'a DataPort, bool)>,
    activity_ports: std::collections::BTreeSet<ElementSelector>,
    choreographies: Vec<(&'a Choreography, &'a ChoreographyActivity)>,
    choreography_owners: std::collections::BTreeSet<ElementSelector>,
    target_namespace: Option<String>,
    event_resources: &'a [processcraft_core::bpmn_model::EventResource],
}
impl<'a> SemanticIndex<'a> {
    fn new(document: &'a BpmnDocument) -> Result<Self, RenderError> {
        let mut result = Self {
            references: processcraft_core::reference_resolution::ReferenceIndex::new(document),
            participants: Vec::new(),
            groups: Vec::new(),
            nodes: Vec::new(),
            lanes: Vec::new(),
            flows: Vec::new(),
            parents: std::collections::BTreeMap::new(),
            conversations: Vec::new(),
            conversation_links: Vec::new(),
            data_associations: Vec::new(),
            data_objects: Vec::new(),
            ports: Vec::new(),
            activity_ports: std::collections::BTreeSet::new(),
            choreographies: Vec::new(),
            choreography_owners: std::collections::BTreeSet::new(),
            event_resources: &document.event_resources,
            target_namespace: document
                .definitions
                .as_ref()
                .map(|d| d.target_namespace.clone()),
        };
        for process in &document.processes {
            if let Some(io) = &process.io_specification {
                result.ports.extend(io.inputs.iter().map(|p| (p, false)));
                result.ports.extend(io.outputs.iter().map(|p| (p, true)));
            }
        }
        let mut stack: Vec<_> = document
            .processes
            .iter()
            .rev()
            .map(|p| {
                (
                    &p.groups,
                    &p.nodes,
                    &p.lanes,
                    &p.flows,
                    &p.data_associations,
                    &p.data_objects,
                    0usize,
                    None::<ElementSelector>,
                )
            })
            .collect();
        while let Some((groups, nodes, lanes, flows, associations, objects, depth, parent)) =
            stack.pop()
        {
            if depth > 32 {
                return Err(RenderError("Nested BPMN exceeds renderer depth limit"));
            }
            result.groups.extend(groups);
            result.lanes.extend(lanes.iter().map(|lane| (lane, depth)));
            result.flows.extend(flows);
            result.data_associations.extend(associations);
            result.data_objects.extend(objects);
            if let Some(parent) = parent {
                for id in groups.iter().map(|g| g.id.selector()).chain(
                    nodes
                        .iter()
                        .map(|n| n.id.selector())
                        .chain(lanes.iter().map(|l| l.id.selector()))
                        .chain(flows.iter().map(|f| f.id.selector()))
                        .chain(
                            associations
                                .iter()
                                .filter_map(|a| a.id.as_ref().map(|id| id.selector())),
                        ),
                ) {
                    result.parents.insert(id, parent.clone());
                }
            }
            for node in nodes {
                result.nodes.push((node, depth));
                if let Some(NodeDetails::Event {
                    io: Some(event_io), ..
                }) = &node.details
                {
                    let event_ports: &[DataPort] = match event_io {
                        processcraft_core::bpmn_model::EventIo::Catch { outputs, .. } => outputs,
                        processcraft_core::bpmn_model::EventIo::Throw { inputs, .. } => inputs,
                    };
                    for port in event_ports {
                        if let Some(id) = &port.id {
                            result.activity_ports.insert(id.selector());
                            result.parents.insert(id.selector(), node.id.selector());
                        }
                    }
                }
                if let Some(io) = &node.io_specification {
                    for (port, output) in io
                        .inputs
                        .iter()
                        .map(|p| (p, false))
                        .chain(io.outputs.iter().map(|p| (p, true)))
                    {
                        result.ports.push((port, output));
                        if let Some(id) = &port.id {
                            result.activity_ports.insert(id.selector());
                            result.parents.insert(id.selector(), node.id.selector());
                        }
                    }
                }
            }
            for node in nodes.iter().rev() {
                if let Some(NodeDetails::SubProcess { content }) = &node.details {
                    stack.push((
                        &content.groups,
                        &content.nodes,
                        &content.lanes,
                        &content.flows,
                        &content.data_associations,
                        &content.data_objects,
                        depth + 1,
                        Some(node.id.selector()),
                    ));
                }
            }
        }
        for choreography in &document.choreographies {
            result.flows.extend(&choreography.message_flows);
            result
                .conversations
                .extend(&choreography.conversation_nodes);
            result
                .conversation_links
                .extend(&choreography.conversation_links);
            if let Some(extras) = &choreography.extras {
                for artifact in &extras.artifacts {
                    match artifact {
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(group) => result.groups.push(group),
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) => result.nodes.push((node, 0)),
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(flow) => result.flows.push(flow),
                    }
                }
            }
            for node in &choreography.conversation_nodes {
                if let Some(parent) = &node.parent {
                    result.parents.insert(node.id.selector(), parent.clone());
                }
            }
            for link in &choreography.conversation_links {
                if let Some(parent) = choreography
                    .conversation_nodes
                    .iter()
                    .find(|node| {
                        result
                            .resolved(&link.source)
                            .as_ref()
                            .is_some_and(|id| node.id == *id)
                            || result
                                .resolved(&link.target)
                                .as_ref()
                                .is_some_and(|id| node.id == *id)
                    })
                    .and_then(|node| node.parent.as_ref())
                {
                    result.parents.insert(link.id.selector(), parent.clone());
                }
            }
        }
        for scope in processcraft_core::queries::choreography_scopes(document) {
            let nested = scope.parent.is_some();
            if nested {
                result.choreography_owners.insert(scope.owner.clone());
            }
            for activity in &scope.activities {
                result.choreographies.push((scope.root, activity));
                if nested {
                    result
                        .parents
                        .insert(activity.id.selector(), scope.owner.clone());
                }
            }
            for node in scope.events.iter().chain(&scope.gateways) {
                result.nodes.push((node, usize::from(nested)));
                if nested {
                    result
                        .parents
                        .insert(node.id.selector(), scope.owner.clone());
                }
            }
            for flow in &scope.flows {
                result.flows.push(flow);
                if nested {
                    result
                        .parents
                        .insert(flow.id.selector(), scope.owner.clone());
                }
            }
            if nested {
                for artifact in &scope.artifacts {
                    match artifact {
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(group) => { result.groups.push(group); result.parents.insert(group.id.selector(), scope.owner.clone()); },
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) => { result.nodes.push((node, 1)); result.parents.insert(node.id.selector(), scope.owner.clone()); },
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(flow) => { result.flows.push(flow); result.parents.insert(flow.id.selector(), scope.owner.clone()); },
                    }
                }
            }
        }
        for c in document
            .collaboration
            .iter()
            .chain(&document.additional_collaborations)
        {
            result.participants.extend(&c.participants);
            if let Some(extras) = &c.extras {
                for artifact in &extras.artifacts {
                    match artifact {
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(group) => result.groups.push(group),
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) => result.nodes.push((node, 0)),
                        processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(flow) => result.flows.push(flow),
                    }
                }
            }
            result.flows.extend(&c.flows);
            result.conversations.extend(&c.conversation_nodes);
            result.conversation_links.extend(&c.conversation_links);
            for link in &c.conversation_links {
                if let Some(parent) = c
                    .conversation_nodes
                    .iter()
                    .find(|node| {
                        result
                            .resolved(&link.source)
                            .as_ref()
                            .is_some_and(|id| node.id == *id)
                            || result
                                .resolved(&link.target)
                                .as_ref()
                                .is_some_and(|id| node.id == *id)
                    })
                    .and_then(|node| node.parent.as_ref())
                {
                    result.parents.insert(link.id.selector(), parent.clone());
                }
            }
            for node in &c.conversation_nodes {
                if let Some(parent) = &node.parent {
                    result.parents.insert(node.id.selector(), parent.clone());
                }
            }
        }
        Ok(result)
    }
    fn resolved(
        &self,
        reference: &Option<processcraft_core::bpmn_activity::ModelReference>,
    ) -> Option<ElementSelector> {
        reference
            .as_ref()
            .and_then(|reference| match self.references.resolve(reference) {
                processcraft_core::reference_resolution::ReferenceResolution::Resolved(id) => {
                    Some(id)
                }
                _ => None,
            })
    }
    fn participant(
        &self,
        id: &ElementSelector,
    ) -> Option<&'a processcraft_core::model::Participant> {
        self.participants
            .iter()
            .copied()
            .find(|participant| &participant.id == id)
    }
    // Flat DI has no general parent pointer. Only a unique containing owner
    // representation proves which expanded/collapsed view owns a child.
    fn visibility(
        &self,
        id: &ElementSelector,
        bounds: &Bounds,
        diagram: &processcraft_core::model::Diagram,
    ) -> (bool, bool) {
        let mut parent = self.parents.get(id);
        let mut ambiguous = false;
        for _ in 0..=32 {
            let Some(owner) = parent else {
                return (false, ambiguous);
            };
            if &diagram.plane == owner {
                return (false, ambiguous);
            }
            if self.choreography_owners.contains(owner) {
                let candidates: Vec<_> = diagram
                    .shapes
                    .iter()
                    .filter(|shape| {
                        &shape.element == owner
                            && shape.bounds.x <= bounds.x
                            && shape.bounds.y <= bounds.y
                            && shape.bounds.x + shape.bounds.width >= bounds.x + bounds.width
                            && shape.bounds.y + shape.bounds.height >= bounds.y + bounds.height
                    })
                    .collect();
                match candidates.as_slice() {
                    [shape] if shape.is_expanded == Some(false) => return (true, ambiguous),
                    [_] => {}
                    _ => {
                        ambiguous = true;
                        return (false, ambiguous);
                    }
                }
            } else if diagram
                .shapes
                .iter()
                .any(|shape| &shape.element == owner && shape.is_expanded == Some(false))
            {
                return (true, ambiguous);
            }
            parent = self.parents.get(owner);
        }
        (false, true)
    }
    fn shape_visibility(
        &self,
        shape: &processcraft_core::model::Shape,
        diagram: &processcraft_core::model::Diagram,
    ) -> (bool, bool) {
        let owner = match &shape.choreography_activity_shape {
            Some(processcraft_core::model::DiReference::Local(id)) => {
                diagram.shapes.iter().find(|s| &s.id == id)
            }
            Some(processcraft_core::model::DiReference::Qualified(q)) => {
                diagram.shapes.iter().find(|s| {
                    s.id.xml_id() == Some(&q.local)
                        && self.target_namespace.as_ref() == Some(&q.namespace)
                })
            }
            None => None,
        };
        if let Some(owner) = owner {
            return self.visibility(&owner.element, &owner.bounds, diagram);
        }
        self.visibility(&shape.element, &shape.bounds, diagram)
    }
    fn edge_visibility(
        &self,
        edge: &processcraft_core::model::Edge,
        diagram: &processcraft_core::model::Diagram,
    ) -> (bool, bool) {
        let Some(first) = edge.waypoints.first() else {
            return (
                false,
                self.parents.get(&edge.element).is_some_and(|owner| {
                    self.choreography_owners.contains(owner) && &diagram.plane != owner
                }),
            );
        };
        let (mut x, mut y, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
        for p in &edge.waypoints {
            x = x.min(p.x);
            y = y.min(p.y);
            right = right.max(p.x);
            bottom = bottom.max(p.y);
        }
        self.visibility(
            &edge.element,
            &Bounds {
                x,
                y,
                width: right - x,
                height: bottom - y,
            },
            diagram,
        )
    }
    fn choreography(
        &self,
        id: &ElementSelector,
    ) -> Option<(&'a Choreography, &'a ChoreographyActivity)> {
        self.choreographies
            .iter()
            .copied()
            .find(|(_, activity)| &activity.id == id)
    }
    /// BPMN 2.0.2 Table 8.21 and the Category section: the Group label is the
    /// CategoryValue value, optionally prepended by the Category name and a delineator.
    /// The spec contradicts itself (Table 8.21 says "." and the Category section says ":");
    /// ":" is chosen as the more specific text. The space after it is not specified.
    fn group_label(&self, id: &ElementSelector, document: &BpmnDocument) -> Option<String> {
        let group = self.groups.iter().find(|g| &g.id == id)?;
        let value_id = self.resolved(&group.category_value_ref)?;
        let (category, value) = document.categories.iter().find_map(|c| {
            c.values
                .iter()
                .find(|v| v.id == value_id)
                .map(|v| (c, v.value.as_deref()))
        })?;
        let value = value?;
        Some(match category.name.as_deref().filter(|n| !n.is_empty()) {
            Some(name) => format!("{name}: {value}"),
            None => value.to_owned(),
        })
    }
    fn data_object(&self, id: &ElementSelector) -> Option<&'a DataObject> {
        self.data_objects
            .iter()
            .copied()
            .find(|item| item.id.as_ref().is_some_and(|identity| identity == id))
    }
    fn data_association(&self, id: &ElementSelector) -> Option<&'a DataAssociation> {
        self.data_associations
            .iter()
            .copied()
            .find(|item| item.id.as_ref().is_some_and(|identity| identity == id))
    }
    fn port(&self, id: &ElementSelector) -> Option<(&'a DataPort, bool)> {
        self.ports
            .iter()
            .copied()
            .find(|(port, _)| port.id.as_ref().is_some_and(|identity| identity == id))
    }
    /// Name of the Message a message flow references (BPMN 2.0.2 section 9.3).
    fn message_name(&self, flow: &Flow) -> Option<&'a str> {
        let id = self.resolved(&flow.message_ref)?;
        self.event_resources
            .iter()
            .find_map(|resource| match resource {
                processcraft_core::bpmn_model::EventResource::Message {
                    id: message,
                    name: Some(name),
                    ..
                } if message == &id && !name.is_empty() => Some(name.as_str()),
                _ => None,
            })
    }
    fn conversation(&self, id: &ElementSelector) -> Option<&'a ConversationNode> {
        self.conversations
            .iter()
            .copied()
            .find(|node| &node.id == id)
    }
    fn conversation_link(&self, id: &ElementSelector) -> Option<&'a ConversationLink> {
        self.conversation_links
            .iter()
            .copied()
            .find(|link| &link.id == id)
    }
    fn is_default_flow(&self, flow: &Flow) -> bool {
        self.resolved(&flow.source).and_then(|source| self.node(&source)).is_some_and(|(node,_)| matches!(&node.details,Some(NodeDetails::Gateway{default_flow:Some(id),..}) if id==&flow.id) || node.activity.as_ref().and_then(|a| a.default_flow.as_ref()).is_some_and(|id| id == &flow.id))
    }
    fn conditional_flow_marker(&self, flow: &Flow) -> bool {
        matches!(flow.kind, FlowKind::Sequence)
            && flow.condition.is_some()
            && !self.is_default_flow(flow)
            && self
                .resolved(&flow.source)
                .and_then(|source| self.node(&source))
                .is_some_and(|(node, _)| {
                    !matches!(
                        node.kind,
                        NodeKind::ExclusiveGateway
                            | NodeKind::ParallelGateway
                            | NodeKind::InclusiveGateway
                            | NodeKind::ComplexGateway
                            | NodeKind::EventBasedGateway
                    )
                })
    }
    fn node(&self, id: &ElementSelector) -> Option<(&'a Node, usize)> {
        self.nodes.iter().copied().find(|(n, _)| &n.id == id)
    }
    fn lane(&self, id: &ElementSelector) -> Option<(&'a Lane, usize)> {
        self.lanes.iter().copied().find(|(lane, _)| &lane.id == id)
    }
    /// Authored paint rank of an edge. Message flows are lifted above every shape,
    /// as in common modelers, so a pool or participant band fill listed later in the
    /// plane never hides them.
    fn edge_paint_rank(
        &self,
        ranks: &std::collections::BTreeMap<ElementSelector, usize>,
        edge: &processcraft_core::model::Edge,
    ) -> usize {
        let rank = *ranks.get(&edge.id.selector()).unwrap_or(&(ranks.len() + 1));
        if self
            .flow(&edge.element)
            .is_some_and(|flow| matches!(flow.kind, FlowKind::Message))
        {
            rank + ranks.len() + 1
        } else {
            rank
        }
    }
    fn flow(&self, id: &ElementSelector) -> Option<&'a Flow> {
        self.flows.iter().copied().find(|f| &f.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
    /// The interactive editor canvas: a dotted background, white activities and tinted pool and
    /// lane headers. Never used for export, so exported files keep the Light and Dark looks.
    CanvasLight,
    CanvasDark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark | Self::CanvasDark)
    }
    pub fn is_canvas(self) -> bool {
        matches!(self, Self::CanvasLight | Self::CanvasDark)
    }
    /// The background, also used to knock out markers drawn over lines.
    fn paper(self) -> [u8; 3] {
        match self {
            Self::Light => [255, 255, 255],
            Self::Dark => [27, 31, 38],
            Self::CanvasLight => [250, 251, 252],
            Self::CanvasDark => [18, 20, 23],
        }
    }
}

/// Canvas dot grid: one dot every `DOT_SPACING` model units, hidden when they would crowd.
const DOT_SPACING: f64 = 16.0;
#[derive(Clone, Debug)]
pub enum HitGeometry {
    Shape(Bounds),
    Ellipse(Bounds),
    Diamond(Bounds),
    Hexagon(Bounds),
    DataObject(Bounds),
    DataStore(Bounds),
    Edge(Vec<Point>),
}
#[derive(Clone, Debug)]
pub struct HitRegion {
    pub key: ElementKey,
    pub diagram_element_id: ElementSelector,
    /// Independent BPMNLabel identity when this hit addresses an explicitly identified label.
    pub label_id: Option<ElementSelector>,
    /// True for label hits even when the BPMNLabel has no independent ID.
    pub is_label: bool,
    pub geometry: HitGeometry,
}
impl HitRegion {
    pub fn contains(&self, x: f64, y: f64, tolerance: f64) -> bool {
        match &self.geometry {
            HitGeometry::Shape(b) => {
                x >= b.x - tolerance
                    && x <= b.x + b.width + tolerance
                    && y >= b.y - tolerance
                    && y <= b.y + b.height + tolerance
            }
            HitGeometry::Ellipse(b) | HitGeometry::Diamond(b) => {
                let rx = b.width / 2.0 + tolerance;
                let ry = b.height / 2.0 + tolerance;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let dx = (x - b.x - b.width / 2.0).abs() / rx;
                let dy = (y - b.y - b.height / 2.0).abs() / ry;
                if matches!(self.geometry, HitGeometry::Ellipse(_)) {
                    dx * dx + dy * dy <= 1.0
                } else {
                    dx + dy <= 1.0
                }
            }
            HitGeometry::DataObject(b) | HitGeometry::DataStore(b) => {
                let kind = if matches!(self.geometry, HitGeometry::DataObject(_)) {
                    NodeKind::DataObjectReference
                } else {
                    NodeKind::DataStoreReference
                };
                processcraft_core::geometry::hit_shape(kind, b, &Point { x, y }, tolerance, 1.0)
                    .unwrap_or(false)
            }
            HitGeometry::Hexagon(b) => {
                let x = (x - b.x) / b.width;
                let y = (y - b.y) / b.height;
                let t = tolerance / b.width.min(b.height).max(1.0);
                x >= -t
                    && x <= 1.0 + t
                    && y >= -t
                    && y <= 1.0 + t
                    && x >= 0.25 * (1.0 - 2.0 * y).abs() - t
                    && x <= 1.0 - 0.25 * (1.0 - 2.0 * y).abs() + t
            }
            HitGeometry::Edge(points) => points.windows(2).any(|p| {
                let dx = p[1].x - p[0].x;
                let dy = p[1].y - p[0].y;
                let l = dx * dx + dy * dy;
                let t = if l == 0.0 {
                    0.0
                } else {
                    ((x - p[0].x) * dx + (y - p[0].y) * dy) / l
                }
                .clamp(0.0, 1.0);
                (x - p[0].x - t * dx).hypot(y - p[0].y - t * dy) <= tolerance
            }),
        }
    }
}
fn effective_event_definitions<'a>(
    details: Option<&'a NodeDetails>,
    document: &'a BpmnDocument,
) -> Result<(Vec<&'a EventDefinition>, bool), ()> {
    let Some(NodeDetails::Event {
        definitions,
        definition_refs,
        parallel_multiple,
        ..
    }) = details
    else {
        return Ok((Vec::new(), false));
    };
    let mut result: Vec<_> = definitions.iter().collect();
    for reference in definition_refs {
        let definition = document.root_event_definitions.iter().find(|definition| {
            definition.id().is_some_and(|id| match reference {
                processcraft_core::bpmn_model::EventReference::Local(selector) => id == selector,
                processcraft_core::bpmn_model::EventReference::Qualified(name) => {
                    document
                        .definitions
                        .as_ref()
                        .is_some_and(|defs| defs.target_namespace == name.namespace)
                        && id.xml_id() == Some(&name.local)
                }
            })
        });
        result.push(definition.ok_or(())?);
    }
    Ok((result, parallel_multiple == &Some(true)))
}

/// BPMN DI: a shape or edge without a BPMNLabel still shows its name at the
/// tool's default position, so the explicit-label policy never hides a name.
fn label_visible(
    _label: Option<&processcraft_core::model::Label>,
    _policy: Option<processcraft_core::model::DiLabelVisibility>,
) -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelAlign {
    TopLeft,
    TopCenter,
    Center,
}

/// Events, gateways and data references carry their name below the shape.
fn has_external_label(kind: Option<NodeKind>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::StartEvent
                | NodeKind::EndEvent
                | NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
                | NodeKind::ExclusiveGateway
                | NodeKind::ParallelGateway
                | NodeKind::InclusiveGateway
                | NodeKind::ComplexGateway
                | NodeKind::EventBasedGateway
                | NodeKind::DataObjectReference
                | NodeKind::DataStoreReference
        )
    )
}

/// Default external label box: centered under the shape, room for three lines.
fn external_label_bounds(b: &Bounds) -> Bounds {
    let width = b.width.max(100.0);
    Bounds {
        x: b.x + (b.width - width) / 2.0,
        y: b.y + b.height + 2.0,
        width,
        height: 64.0,
    }
}

/// Authored DI label bounds, or the default placement for the shape kind.
fn shape_label_layout(
    shape: &processcraft_core::model::Shape,
    kind: Option<NodeKind>,
    marker_kind: Option<NodeKind>,
    node: Option<&Node>,
    choreography: bool,
    is_lane_or_pool: bool,
) -> (Bounds, LabelAlign) {
    let b = &shape.bounds;
    if let Some(bounds) = shape.label.as_ref().and_then(|l| l.bounds.as_ref()) {
        let align = if has_external_label(kind) {
            LabelAlign::TopCenter
        } else {
            LabelAlign::TopLeft
        };
        return (bounds.clone(), align);
    }
    if is_lane_or_pool {
        return if shape.is_horizontal == Some(false) {
            (
                Bounds {
                    height: b.height.min(HEADER),
                    ..b.clone()
                },
                LabelAlign::TopLeft,
            )
        } else {
            (
                Bounds {
                    width: b.width.min(HEADER),
                    ..b.clone()
                },
                LabelAlign::Center,
            )
        };
    }
    if choreography {
        return (
            Bounds {
                x: b.x,
                y: b.y + b.height * 0.25,
                width: b.width,
                height: b.height * 0.5,
            },
            LabelAlign::Center,
        );
    }
    let bottom_markers = node.is_some_and(|node| {
        node.loop_characteristics.is_some()
            || node
                .activity
                .as_ref()
                .is_some_and(|activity| activity.is_for_compensation == Some(true))
    });
    if is_task_kind(marker_kind) {
        let top = if matches!(marker_kind, Some(NodeKind::Task)) {
            0.0
        } else {
            20.0
        };
        let bottom = if bottom_markers { 18.0 } else { 0.0 };
        return (
            Bounds {
                x: b.x,
                y: b.y + top,
                width: b.width,
                height: (b.height - top - bottom).max(0.0),
            },
            LabelAlign::Center,
        );
    }
    if has_external_label(kind) {
        return (external_label_bounds(b), LabelAlign::TopCenter);
    }
    if matches!(
        kind,
        Some(
            NodeKind::SubProcess
                | NodeKind::AdHocSubProcess
                | NodeKind::Transaction
                | NodeKind::CallActivity
        )
    ) && shape.is_expanded != Some(true)
    {
        // Collapsed: center like a task, above the collapse/loop marker row.
        return (
            Bounds {
                height: (b.height - 18.0).max(0.0),
                ..b.clone()
            },
            LabelAlign::Center,
        );
    }
    (b.clone(), LabelAlign::TopLeft)
}

/// Default edge label: below a horizontal midpoint segment, beside a vertical one.
fn edge_label_layout(edge: &processcraft_core::model::Edge) -> Option<(Bounds, LabelAlign)> {
    if let Some(bounds) = edge.label.as_ref().and_then(|l| l.bounds.as_ref()) {
        return Some((bounds.clone(), LabelAlign::TopCenter));
    }
    let first = edge.waypoints.first()?;
    let points: Vec<_> = edge
        .waypoints
        .iter()
        .map(|p| (p.x as f32, p.y as f32))
        .collect();
    let ((mx, my), (dx, dy)) = polyline_midpoint_segment(&points)
        .unwrap_or(((first.x as f32, first.y as f32), (1.0, 0.0)));
    let (mx, my) = (f64::from(mx), f64::from(my));
    Some(if dx.abs() >= dy.abs() {
        (
            Bounds {
                x: mx - 60.0,
                y: my + 2.0,
                width: 120.0,
                height: 40.0,
            },
            LabelAlign::TopCenter,
        )
    } else {
        (
            Bounds {
                x: mx + 4.0,
                y: my - 20.0,
                width: 120.0,
                height: 40.0,
            },
            LabelAlign::TopLeft,
        )
    })
}

/// Last segment with non-zero length, so coincident end waypoints keep a defined angle.
fn last_segment(points: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    points
        .windows(2)
        .rev()
        .find(|p| p[0] != p[1])
        .map(|p| (p[0], p[1]))
}

fn label_font<'a>(
    document: &BpmnDocument,
    diagram: &'a processcraft_core::model::Diagram,
    label: Option<&processcraft_core::model::Label>,
    diagnostics: &mut Vec<String>,
) -> Option<&'a processcraft_core::model::DiFont> {
    use processcraft_core::model::DiStyleRef;
    let reference = label.and_then(|label| label.style_ref.as_ref())?;
    let style = diagram.label_styles.iter().find(|style| {
        style.id.as_ref().is_some_and(|id| match reference {
            DiStyleRef::Local(selector) => id == selector,
            DiStyleRef::Qualified(qname) => {
                document
                    .definitions
                    .as_ref()
                    .is_some_and(|definitions| definitions.target_namespace == qname.namespace)
                    && id.xml_id() == Some(&qname.local)
            }
        })
    });
    match style {
        Some(style) => match style.font.as_ref() {
            Some(font) => Some(font),
            None => {
                diagnostics.push("Label style has no font; using default".into());
                None
            }
        },
        None => {
            diagnostics.push("Label style is unresolved; using default".into());
            None
        }
    }
}

fn is_task_kind(kind: Option<NodeKind>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::Task
                | NodeKind::UserTask
                | NodeKind::ManualTask
                | NodeKind::ServiceTask
                | NodeKind::SendTask
                | NodeKind::ReceiveTask
                | NodeKind::ScriptTask
                | NodeKind::BusinessRuleTask
        )
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalledTarget {
    Unspecified,
    Process,
    Global(NodeKind),
    Unresolved,
}
fn called_target(node: Option<&Node>, document: &BpmnDocument) -> CalledTarget {
    let Some(Node {
        kind: NodeKind::CallActivity,
        details: Some(NodeDetails::CallActivity { called_element }),
        ..
    }) = node
    else {
        return CalledTarget::Unspecified;
    };
    let Some(reference) = called_element else {
        return CalledTarget::Unspecified;
    };
    let is_local = document
        .definitions
        .as_ref()
        .is_some_and(|definitions| definitions.target_namespace == reference.namespace);
    if !is_local {
        return CalledTarget::Unresolved;
    }
    if document
        .processes
        .iter()
        .any(|process| process.id.xml_id() == Some(&reference.local))
    {
        return CalledTarget::Process;
    }
    let Some(task) = document
        .global_tasks
        .iter()
        .find(|task| task.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local))
    else {
        return CalledTarget::Unresolved;
    };
    use processcraft_core::bpmn_activity::GlobalTaskKind;
    CalledTarget::Global(match task.kind {
        GlobalTaskKind::CallableElement | GlobalTaskKind::Task => NodeKind::Task,
        GlobalTaskKind::Manual => NodeKind::ManualTask,
        GlobalTaskKind::BusinessRule { .. } => NodeKind::BusinessRuleTask,
        GlobalTaskKind::Script { .. } => NodeKind::ScriptTask,
        GlobalTaskKind::User { .. } => NodeKind::UserTask,
    })
}

fn node_display_name<'a>(
    node: &'a Node,
    document: &'a BpmnDocument,
) -> (std::borrow::Cow<'a, str>, bool) {
    use processcraft_core::bpmn_xml::{MixedXml, XmlNode};
    // BPMN 2.0.2 section 10.5.5 (Link Events): paired link events are matched by the
    // shared link definition name, so it labels an event that has no name of its own.
    if node.name.as_deref().is_none_or(str::is_empty)
        && let Ok((definitions, _)) = effective_event_definitions(node.details.as_ref(), document)
        && let Some(name) = definitions.iter().find_map(|definition| match definition {
            EventDefinition::Link { name, .. } if !name.is_empty() => Some(name.as_str()),
            _ => None,
        })
    {
        return (std::borrow::Cow::Borrowed(name), false);
    }
    if let Some(NodeDetails::TextAnnotation {
        text: Some(text), ..
    }) = &node.details
    {
        return match text {
            MixedXml::Text(text) => (std::borrow::Cow::Borrowed(text), false),
            MixedXml::Structured(content) => {
                // Project text as inert characters. Markup, comments and processing
                // instructions are never interpreted or executed by the renderer.
                let mut result = String::new();
                let mut pending: Vec<_> = content.nodes.iter().rev().collect();
                while let Some(part) = pending.pop() {
                    match part {
                        XmlNode::Text { text } => {
                            if result.len().saturating_add(text.len()) > 16_384 {
                                return (
                                    std::borrow::Cow::Borrowed(
                                        "[Annotation text exceeds display limit]",
                                    ),
                                    true,
                                );
                            }
                            result.push_str(text);
                        }
                        XmlNode::Element { element } => {
                            pending.extend(element.children.iter().rev())
                        }
                        XmlNode::Comment { .. } | XmlNode::ProcessingInstruction { .. } => {}
                    }
                }
                (std::borrow::Cow::Owned(result), false)
            }
        };
    }
    (
        std::borrow::Cow::Borrowed(node.name.as_deref().unwrap_or("")),
        false,
    )
}

fn data_label(
    node: Option<&processcraft_core::model::Node>,
    name: &str,
    semantics: &SemanticIndex<'_>,
) -> String {
    let Some(node) = node else {
        return name.to_owned();
    };
    let base = match &node.details {
        Some(NodeDetails::DataObjectReference {
            data_object_ref: Some(id),
        }) => semantics
            .data_object(id)
            .and_then(|object| object.name.as_deref())
            .filter(|object_name| !object_name.is_empty())
            .unwrap_or(name),
        _ => name,
    };
    match node
        .item_aware
        .as_ref()
        .and_then(|item| item.data_state.as_ref())
        .and_then(|state| state.name.as_deref())
        .filter(|name| !name.is_empty())
    {
        Some(state) if !base.is_empty() => format!("{base} [{state}]"),
        Some(state) => format!("[{state}]"),
        None => base.to_owned(),
    }
}

/// Document-only scene. Coordinates in hit regions remain in absolute document space.
#[derive(Clone, Debug)]
pub struct BpmnScene {
    pub scene: DiagramScene,
    pub origin: (f64, f64),
    pub hits: Vec<HitRegion>,
    pub diagnostics: Vec<String>,
    pub capability: Capability,
}
impl BpmnScene {
    pub fn build(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
    ) -> Result<Self, RenderError> {
        Self::build_internal(document, diagram_id, appearance, fonts, family, true)
    }
    fn build_internal(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        shape_labels: bool,
    ) -> Result<Self, RenderError> {
        if matches!(document.capability, Capability::PreservedSource { .. }) {
            let mut scene = DiagramScene::new(33.0, 33.0)?;
            scene.add_background_rect(0.0, 0.0, 33.0, 33.0, appearance.paper())?;
            return Ok(Self {
                scene,
                origin: (0.0, 0.0),
                hits: Vec::new(),
                diagnostics: vec!["Preserved source: semantic preview is unavailable".into()],
                capability: document.capability.clone(),
            });
        }
        let diagram = document
            .diagrams
            .iter()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        let semantics = SemanticIndex::new(document)?;
        let mut extents: Option<(f64, f64, f64, f64)> = None;
        let mut include = |x: f64, y: f64| -> Result<(), RenderError> {
            if !x.is_finite() || !y.is_finite() || x.abs() > 1_000_000.0 || y.abs() > 1_000_000.0 {
                return Err(RenderError("Invalid diagram geometry"));
            }
            extents = Some(match extents {
                None => (x, y, x, y),
                Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
            });
            Ok(())
        };
        for shape in &diagram.shapes {
            if semantics.activity_ports.contains(&shape.element) {
                continue;
            }
            if semantics.shape_visibility(shape, diagram).0 {
                continue;
            }
            let b = &shape.bounds;
            if b.width <= 0.0 || b.height <= 0.0 {
                return Err(RenderError("Invalid shape bounds"));
            }
            include(b.x, b.y)?;
            include(b.x + b.width, b.y + b.height)?;
            if shape.is_message_visible == Some(true) && shape.participant_band_kind.is_some() {
                include(b.x + b.width / 2.0 - 12.0, b.y - 24.0)?;
                include(b.x + b.width / 2.0 + 12.0, b.y + b.height + 24.0)?;
            }
            if let Some(b) = shape.label.as_ref().and_then(|label| label.bounds.as_ref()) {
                include(b.x, b.y)?;
                include(b.x + b.width, b.y + b.height)?;
            } else if label_visible(shape.label.as_ref(), shape.label_visibility) {
                // Default external labels sit outside the shape; keep export margins even.
                let node = semantics.node(&shape.element).map(|(node, _)| node);
                let port = semantics.port(&shape.element).map(|(port, _)| port);
                let store = document
                    .data_stores
                    .iter()
                    .find(|store| store.id.as_ref().is_some_and(|id| id == &shape.element));
                let kind = node
                    .map(|node| node.kind)
                    .or(port.map(|_| NodeKind::DataObjectReference))
                    .or(store.map(|_| NodeKind::DataStoreReference));
                let named = match (node, port, store) {
                    (Some(node), ..) => {
                        !data_label(Some(node), &node_display_name(node, document).0, &semantics)
                            .is_empty()
                    }
                    (_, Some(port), _) => port.name.as_deref().is_some_and(|n| !n.is_empty()),
                    (.., Some(store)) => store.name.as_deref().is_some_and(|n| !n.is_empty()),
                    _ => false,
                };
                if named && has_external_label(kind) {
                    let lb = external_label_bounds(b);
                    include(lb.x, lb.y)?;
                    include(lb.x + lb.width, lb.y + lb.height)?;
                }
            }
        }
        for edge in &diagram.edges {
            if semantics.edge_visibility(edge, diagram).0 {
                continue;
            }
            for p in &edge.waypoints {
                include(p.x, p.y)?;
            }
            if let Some(b) = edge.label.as_ref().and_then(|label| label.bounds.as_ref()) {
                include(b.x, b.y)?;
                include(b.x + b.width, b.y + b.height)?;
            }
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = extents.unwrap_or((0.0, 0.0, 1.0, 1.0));
        for edge in &diagram.edges {
            if semantics.edge_visibility(edge, diagram).0 {
                continue;
            }
            if label_visible(edge.label.as_ref(), edge.label_visibility)
                && edge
                    .label
                    .as_ref()
                    .and_then(|label| label.bounds.as_ref())
                    .is_none()
                && let Some((p, _)) = edge_label_layout(edge)
            {
                let named = semantics
                    .flow(&edge.element)
                    .is_some_and(|f| f.name.as_deref().is_some_and(|name| !name.is_empty()))
                    || semantics
                        .conversation_link(&edge.element)
                        .is_some_and(|link| {
                            link.name.as_deref().is_some_and(|name| !name.is_empty())
                        });
                if named {
                    minx = minx.min(p.x);
                    miny = miny.min(p.y);
                    maxx = maxx.max(p.x + p.width);
                    maxy = maxy.max(p.y + p.height);
                }
            }
        }
        let origin = (minx - 16.0, miny - 16.0);
        let mut scene = DiagramScene::for_diagram(
            ((maxx - minx) + 32.0) as f32,
            ((maxy - miny) + 32.0) as f32,
        )?;
        let bg = appearance.paper();
        let (fg, fill) = match appearance {
            Appearance::Light => ([26, 37, 52], [255, 255, 255]),
            Appearance::Dark => ([230, 234, 240], [47, 53, 63]),
            Appearance::CanvasLight => ([42, 46, 54], [255, 255, 255]),
            Appearance::CanvasDark => ([201, 206, 214], [32, 36, 42]),
        };
        scene.add_background_rect(0.0, 0.0, scene.width(), scene.height(), bg)?;
        let mut hits = Vec::new();
        let mut diagnostics = Vec::new();
        if diagram
            .shapes
            .iter()
            .any(|shape| semantics.shape_visibility(shape, diagram).1)
            || diagram
                .edges
                .iter()
                .any(|edge| semantics.edge_visibility(edge, diagram).1)
        {
            diagnostics.push("Choreography representation ownership is ambiguous; affected elements remain visible".into());
        }
        if has_uninterpreted_di(diagram) {
            diagnostics.push(DI_EXTENSION_DIAGNOSTIC.into());
        }
        let paint_ranks = diagram.plane_order.as_ref().map(|order| {
            order
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i + 1))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        let mut ordered_shapes: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|shape| !semantics.shape_visibility(shape, diagram).0)
            .collect();
        ordered_shapes.sort_by_key(|shape| {
            if semantics.groups.iter().any(|g| g.id == shape.element)
                || semantics.participant(&shape.element).is_some()
            {
                (0, 0)
            } else if let Some((_, depth)) = semantics.lane(&shape.element) {
                (depth + 1, 0)
            } else if let Some((_, depth)) = semantics.node(&shape.element) {
                (depth + 1, 1)
            } else {
                (usize::MAX, 0)
            }
        });
        for shape in ordered_shapes {
            if let Some(ranks) = &paint_ranks {
                scene.set_paint_rank(
                    *ranks
                        .get(&shape.id.selector())
                        .unwrap_or(&(ranks.len() + 1)),
                );
            }
            if semantics.activity_ports.contains(&shape.element) {
                diagnostics.push(format!("Activity-local or event-local data port {} cannot be depicted as a BPMN shape; only process ports can be shown", shape.element));
                continue;
            }
            if let Some(band_kind) = shape.participant_band_kind {
                use processcraft_core::model::{DiReference, ParticipantBandKind};
                let owner = match &shape.choreography_activity_shape {
                    Some(DiReference::Local(id)) => diagram.shapes.iter().find(|s| &s.id == id),
                    Some(DiReference::Qualified(q))
                        if document
                            .definitions
                            .as_ref()
                            .is_some_and(|defs| defs.target_namespace == q.namespace) =>
                    {
                        diagram
                            .shapes
                            .iter()
                            .find(|s| s.id.xml_id() == Some(&q.local))
                    }
                    _ => None,
                };
                let context = owner.and_then(|owner| semantics.choreography(&owner.element));
                let participant = context.and_then(|(scope, _)| {
                    scope.participants.iter().find(|p| {
                        p.id == shape.element
                            && context.is_some_and(|(_, activity)| {
                                activity
                                    .participant_refs
                                    .iter()
                                    .any(|r| self_reference_resolves(&semantics, r, &shape.element))
                            })
                    })
                });
                let Some(participant) = participant else {
                    diagnostics.push(format!("Participant band {} has an unresolved activity representation or participant", shape.id));
                    continue;
                };
                let b = &shape.bounds;
                let (x, y, w, h) = (
                    (b.x - origin.0) as f32,
                    (b.y - origin.1) as f32,
                    b.width as f32,
                    b.height as f32,
                );
                let non_initiating = matches!(
                    band_kind,
                    ParticipantBandKind::TopNonInitiating
                        | ParticipantBandKind::MiddleNonInitiating
                        | ParticipantBandKind::BottomNonInitiating
                );
                let default_band_fill = if non_initiating {
                    [225, 225, 225]
                } else {
                    fill
                };
                let band_fill = color(
                    shape.style.as_ref().and_then(|s| s.fill.as_deref()),
                    default_band_fill,
                );
                // Corners shared with the owner's rounded outline are clipped to it.
                let band_path = {
                    let o = owner.map_or(b, |owner| &owner.bounds);
                    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
                    let (top, bottom, left, right) = (
                        near(b.y, o.y),
                        near(b.y + b.height, o.y + o.height),
                        near(b.x, o.x),
                        near(b.x + b.width, o.x + o.width),
                    );
                    let mut p = PathBuilder::new();
                    rounded_corners(
                        &mut p,
                        x,
                        y,
                        w,
                        h,
                        rounding(o.width as f32, o.height as f32),
                        [top && left, top && right, bottom && right, bottom && left],
                    );
                    p.finish().ok_or(RenderError("Invalid participant band"))?
                };
                scene.add_path(band_path.clone(), band_fill)?;
                let band_fg = if non_initiating {
                    if low_contrast([26, 37, 52], band_fill) {
                        [255, 255, 255]
                    } else {
                        [26, 37, 52]
                    }
                } else {
                    fg
                };
                let explicit_stroke = shape.style.as_ref().and_then(|s| s.stroke.as_deref());
                let stroke = color(explicit_stroke, band_fg);
                if non_initiating && explicit_stroke.is_some() && low_contrast(stroke, band_fill) {
                    diagnostics.push(format!(
                        "Participant band {} explicit stroke has low contrast against its fill",
                        shape.id
                    ));
                }
                let label_style = shape
                    .label
                    .as_ref()
                    .and_then(|label| label.style.as_ref())
                    .or(shape.style.as_ref());
                if non_initiating
                    && label_style
                        .and_then(|s| s.font.as_deref())
                        .is_some_and(|value| low_contrast(color(Some(value), band_fg), band_fill))
                {
                    diagnostics.push(format!("Participant band {} explicit label color has low contrast against its fill", shape.id));
                }
                outline(&mut scene, &band_path, stroke, 1.0)?;
                if participant
                    .multiplicity
                    .as_ref()
                    .and_then(|m| m.maximum)
                    .is_some_and(|n| n > 1)
                {
                    for offset in [-3.0, 0.0, 3.0] {
                        lines(
                            &mut scene,
                            &[
                                (x + w / 2.0 + offset, y + h - 7.0),
                                (x + w / 2.0 + offset, y + h - 2.0),
                            ],
                            stroke,
                            1.0,
                        )?;
                    }
                }
                let fallback = b;
                let lb = shape
                    .label
                    .as_ref()
                    .and_then(|l| l.bounds.as_ref())
                    .unwrap_or(fallback);
                let name = participant.name.as_deref().unwrap_or("");
                if shape_labels && label_visible(shape.label.as_ref(), shape.label_visibility) {
                    label(
                        &mut scene,
                        fonts,
                        family,
                        name,
                        lb,
                        origin,
                        label_style,
                        label_font(document, diagram, shape.label.as_ref(), &mut diagnostics),
                        band_fg,
                        LabelAlign::Center,
                        &mut diagnostics,
                    )?;
                }
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.selector(),
                    label_id: None,
                    is_label: false,
                    geometry: HitGeometry::Shape(b.clone()),
                });
                if shape_labels
                    && label_visible(shape.label.as_ref(), shape.label_visibility)
                    && !name.is_empty()
                {
                    hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: shape.element.clone(),
                        },
                        diagram_element_id: shape.id.selector(),
                        label_id: shape
                            .label
                            .as_ref()
                            .and_then(|l| l.id.as_ref())
                            .map(|id| id.selector()),
                        is_label: true,
                        geometry: HitGeometry::Shape(lb.clone()),
                    });
                }
                if shape.is_message_visible == Some(true) {
                    let bottom = matches!(
                        band_kind,
                        ParticipantBandKind::BottomInitiating
                            | ParticipantBandKind::BottomNonInitiating
                    );
                    if matches!(
                        band_kind,
                        ParticipantBandKind::MiddleInitiating
                            | ParticipantBandKind::MiddleNonInitiating
                    ) {
                        diagnostics.push(format!("Participant band {} message visibility cannot be depicted on a middle band", shape.id));
                    } else if let Some(band_flow) = context.and_then(|(scope, activity)| {
                        activity.message_flow_refs.iter().find_map(|id| {
                            scope.message_flows.iter().find(|flow| {
                                semantics.resolved(&Some(id.clone())).as_ref()
                                    == Some(&flow.id.selector())
                                    && semantics.resolved(&flow.source).as_ref()
                                        == Some(&shape.element)
                            })
                        })
                    }) {
                        let cy = if bottom { y + h + 13.0 } else { y - 13.0 };
                        let edge_y = if bottom { y + h } else { y };
                        let envelope_y = if bottom { cy - 7.0 } else { cy + 7.0 };
                        dashed_lines(
                            &mut scene,
                            &[(x + w / 2.0, edge_y), (x + w / 2.0, envelope_y)],
                            stroke,
                            2.0,
                        )?;
                        envelope(&mut scene, x + w / 2.0, cy, stroke, non_initiating, fill)?;
                        if let Some(text) = semantics.message_name(band_flow) {
                            message_name_label(
                                &mut scene,
                                fonts,
                                family,
                                text,
                                (x + w / 2.0, cy),
                                origin,
                                fg,
                                &mut diagnostics,
                            )?;
                        }
                    } else {
                        diagnostics.push(format!(
                            "Participant band {} has no resolved outgoing choreography message",
                            shape.id
                        ));
                    }
                }
                continue;
            }
            if semantics.groups.iter().any(|g| g.id == shape.element) {
                if semantics
                    .groups
                    .iter()
                    .find(|g| g.id == shape.element)
                    .is_some_and(|group| {
                        group.category_value_ref.is_some()
                            && semantics.resolved(&group.category_value_ref).is_none()
                    })
                {
                    diagnostics.push(format!(
                        "Group {} has an unresolved category value; its label is omitted",
                        shape.element
                    ));
                }
                let b = &shape.bounds;
                let (x, y, w, h) = (
                    (b.x - origin.0) as f32,
                    (b.y - origin.1) as f32,
                    b.width as f32,
                    b.height as f32,
                );
                let r = 8.0_f32.min(w / 2.0).min(h / 2.0);
                let pts = [
                    (x + r, y),
                    (x + w - r, y),
                    (x + w - r * 0.293, y + r * 0.293),
                    (x + w, y + r),
                    (x + w, y + h - r),
                    (x + w - r * 0.293, y + h - r * 0.293),
                    (x + w - r, y + h),
                    (x + r, y + h),
                    (x + r * 0.293, y + h - r * 0.293),
                    (x, y + h - r),
                    (x, y + r),
                    (x + r * 0.293, y + r * 0.293),
                    (x + r, y),
                ];
                let stroke = color(shape.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                dashed_lines(&mut scene, &pts, stroke, 5.0)?;
                if shape_labels
                    && label_visible(shape.label.as_ref(), shape.label_visibility)
                    && let Some(name) = semantics.group_label(&shape.element, document).as_deref()
                {
                    let fallback = Bounds {
                        x: b.x + 8.0,
                        y: b.y + 4.0,
                        width: (b.width - 16.0).max(0.0),
                        height: b.height.min(28.0),
                    };
                    let bounds = shape
                        .label
                        .as_ref()
                        .and_then(|l| l.bounds.as_ref())
                        .unwrap_or(&fallback);
                    if group_label_collision(bounds, &shape.id.selector(), diagram, &semantics) {
                        diagnostics.push(format!(
                            "Group {} label overlaps another shape",
                            shape.element
                        ));
                    }
                    label(
                        &mut scene,
                        fonts,
                        family,
                        name,
                        bounds,
                        origin,
                        shape
                            .label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(shape.style.as_ref()),
                        label_font(document, diagram, shape.label.as_ref(), &mut diagnostics),
                        fg,
                        LabelAlign::TopLeft,
                        &mut diagnostics,
                    )?;
                    if label_visible(shape.label.as_ref(), shape.label_visibility) {
                        hits.push(HitRegion {
                            key: ElementKey {
                                document_id: document.id,
                                xml_id: shape.element.clone(),
                            },
                            diagram_element_id: shape.id.selector(),
                            label_id: shape
                                .label
                                .as_ref()
                                .and_then(|label| label.id.as_ref())
                                .map(|id| id.selector()),
                            is_label: true,
                            geometry: HitGeometry::Shape(bounds.clone()),
                        });
                    }
                }
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.selector(),
                    label_id: None,
                    is_label: false,
                    geometry: HitGeometry::Shape(b.clone()),
                });
                continue;
            }
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let port = semantics.port(&shape.element);
            let store = document.data_stores.iter().find(|store| {
                store
                    .id
                    .as_ref()
                    .is_some_and(|identity| identity == &shape.element)
            });
            let kind = node
                .map(|node| node.kind)
                .or(port.map(|_| NodeKind::DataObjectReference))
                .or(store.map(|_| NodeKind::DataStoreReference));
            let conversation = semantics.conversation(&shape.element);
            let choreography = semantics.choreography(&shape.element);
            let details = node.and_then(|node| node.details.as_ref());
            let call_target = called_target(node, document);
            let marker_kind = match call_target {
                CalledTarget::Global(task_kind) => Some(task_kind),
                _ => kind,
            };
            if matches!(kind, Some(NodeKind::CallActivity))
                && matches!(call_target, CalledTarget::Unresolved)
            {
                diagnostics.push(format!("Call activity {} target cannot be resolved locally; called task marker omitted", shape.element));
            }
            let node_name = node.map(|node| node_display_name(node, document));
            if node_name.as_ref().is_some_and(|(_, truncated)| *truncated) {
                diagnostics.push(format!(
                    "Annotation {} text exceeds display limit",
                    shape.element
                ));
            }
            let mut name = choreography
                .map(|(_, activity)| activity.name.as_deref().unwrap_or(""))
                .or_else(|| conversation.map(|node| node.name.as_deref().unwrap_or("")))
                .or(node_name.as_ref().map(|(text, _)| text.as_ref()))
                .or_else(|| port.map(|(port, _)| port.name.as_deref().unwrap_or("")))
                .or_else(|| store.map(|store| store.name.as_deref().unwrap_or("")))
                .or_else(|| {
                    semantics
                        .lane(&shape.element)
                        .map(|(lane, _)| lane.name.as_deref().unwrap_or(""))
                });
            if let Some(p) = semantics.participant(&shape.element) {
                name = Some(p.name.as_deref().unwrap_or(""));
            }
            let Some(name) = name else {
                if document
                    .choreographies
                    .iter()
                    .any(|root| root.id == shape.element && root.kind.is_some())
                {
                    diagnostics.push(format!("Global choreography {} has no standalone shape specified by BPMN 2.0.2 section 11.5.4; representation omitted", shape.element));
                } else {
                    diagnostics.push(format!("Unknown shape element {}", shape.element));
                }
                continue;
            };
            let display_name = if let Some((port, _)) = port {
                match port
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|name| !name.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else if let Some(store) = store {
                match store
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|state| !state.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else {
                data_label(node, name, &semantics)
            };
            let name = display_name.as_str();
            if matches!(kind, Some(NodeKind::BoundaryEvent)) {
                let attached = match details {
                    Some(NodeDetails::BoundaryEvent { attached_to, .. }) => Some(attached_to),
                    Some(NodeDetails::Event { attached_to, .. }) => attached_to.as_ref(),
                    _ => None,
                };
                let attached_id = semantics.resolved(&attached.cloned());
                if attached_id.as_ref().is_none_or(|id| {
                    !semantics
                        .nodes
                        .iter()
                        .any(|(n, _)| &n.id == id && n.kind.is_activity())
                        && semantics.choreography(id).is_none()
                }) {
                    diagnostics.push(format!(
                        "Boundary event {} has no valid attached activity",
                        shape.element
                    ));
                    continue;
                }
            }
            let b = &shape.bounds;
            let x = (b.x - origin.0) as f32;
            let y = (b.y - origin.1) as f32;
            let w = b.width as f32;
            let h = b.height as f32;
            let stroke = color(shape.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
            let default_fill = match (appearance, kind) {
                (Appearance::Light | Appearance::CanvasLight, Some(NodeKind::StartEvent)) => {
                    [240, 249, 242]
                }
                (Appearance::Dark | Appearance::CanvasDark, Some(NodeKind::StartEvent)) => {
                    [40, 68, 53]
                }
                (Appearance::Light | Appearance::CanvasLight, Some(NodeKind::EndEvent)) => {
                    [255, 243, 240]
                }
                (Appearance::Dark | Appearance::CanvasDark, Some(NodeKind::EndEvent)) => {
                    [74, 48, 45]
                }
                (
                    Appearance::Light,
                    Some(
                        NodeKind::Task
                        | NodeKind::UserTask
                        | NodeKind::ManualTask
                        | NodeKind::ServiceTask
                        | NodeKind::SendTask
                        | NodeKind::ReceiveTask
                        | NodeKind::ScriptTask
                        | NodeKind::BusinessRuleTask
                        | NodeKind::SubProcess
                        | NodeKind::AdHocSubProcess
                        | NodeKind::Transaction
                        | NodeKind::CallActivity,
                    ),
                ) => [255, 252, 240],
                (
                    Appearance::Dark,
                    Some(
                        NodeKind::Task
                        | NodeKind::UserTask
                        | NodeKind::ManualTask
                        | NodeKind::ServiceTask
                        | NodeKind::SendTask
                        | NodeKind::ReceiveTask
                        | NodeKind::ScriptTask
                        | NodeKind::BusinessRuleTask
                        | NodeKind::SubProcess
                        | NodeKind::AdHocSubProcess
                        | NodeKind::Transaction
                        | NodeKind::CallActivity,
                    ),
                ) => [54, 56, 45],
                _ => fill,
            };
            let fc = color(
                shape.style.as_ref().and_then(|s| s.fill.as_deref()),
                default_fill,
            );
            let mut path = PathBuilder::new();
            let fold = w.min(h).min(12.0);
            match kind {
                Some(
                    NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent,
                ) => ellipse(&mut path, x, y, w, h),
                Some(
                    NodeKind::ExclusiveGateway
                    | NodeKind::ParallelGateway
                    | NodeKind::InclusiveGateway
                    | NodeKind::ComplexGateway
                    | NodeKind::EventBasedGateway,
                ) => polygon(
                    &mut path,
                    &[
                        (x + w / 2.0, y),
                        (x + w, y + h / 2.0),
                        (x + w / 2.0, y + h),
                        (x, y + h / 2.0),
                    ],
                ),
                Some(
                    NodeKind::Task
                    | NodeKind::UserTask
                    | NodeKind::ManualTask
                    | NodeKind::ServiceTask
                    | NodeKind::SendTask
                    | NodeKind::ReceiveTask
                    | NodeKind::ScriptTask
                    | NodeKind::BusinessRuleTask
                    | NodeKind::SubProcess
                    | NodeKind::AdHocSubProcess
                    | NodeKind::Transaction
                    | NodeKind::CallActivity,
                ) => rounded(&mut path, x, y, w, h),
                Some(NodeKind::TextAnnotation) => {}
                Some(NodeKind::DataStoreReference) => cylinder(&mut path, x, y, w, h),
                Some(NodeKind::DataObjectReference) => polygon(
                    &mut path,
                    &[
                        (x, y),
                        (x + w - fold, y),
                        (x + w, y + fold),
                        (x + w, y + h),
                        (x, y + h),
                    ],
                ),
                _ if choreography.is_some() => rounded(&mut path, x, y, w, h),
                _ if conversation.is_some() => polygon(&mut path, &hexagon_points(x, y, w, h)),
                _ => polygon(&mut path, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]),
            };
            if let Some(p) = path.finish() {
                scene.add_path(p, fc)?;
            }
            if choreography.is_some_and(|(root, _)| root.graph.is_none())
                && diagram.plane_order.is_none()
            {
                let band_h = (h * 0.22).clamp(12.0, 24.0).min((h - 2.0).max(0.0));
                let band_color = if appearance.is_dark() {
                    [60, 74, 90]
                } else {
                    [237, 241, 245]
                };
                if w > 2.0 && band_h > 1.0 {
                    // Bottom corners follow the rounded outline, 1 px inside it.
                    let mut band = PathBuilder::new();
                    rounded_corners(
                        &mut band,
                        x + 1.0,
                        y + h - band_h,
                        w - 2.0,
                        band_h - 1.0,
                        rounding(w, h) - 1.0,
                        [false, false, true, true],
                    );
                    if let Some(band) = band.finish() {
                        scene.add_path(band, band_color)?;
                    }
                }
            }
            match kind {
                Some(
                    NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent,
                ) => {
                    let non_interrupting_start = matches!(kind, Some(NodeKind::StartEvent))
                        && matches!(
                            details,
                            Some(NodeDetails::Event {
                                is_interrupting: Some(false),
                                ..
                            })
                        )
                        && semantics
                            .parents
                            .get(&shape.element)
                            .and_then(|parent| semantics.node(parent))
                            .and_then(|(node, _)| node.activity.as_ref())
                            .is_some_and(|activity| activity.triggered_by_event == Some(true));
                    if (matches!(kind, Some(NodeKind::BoundaryEvent)) && non_interrupting(details))
                        || non_interrupting_start
                    {
                        dashed_ellipse(&mut scene, x, y, w, h, stroke)?;
                    } else {
                        let mut p = PathBuilder::new();
                        ellipse(&mut p, x, y, w, h);
                        if let Some(path) = p.finish() {
                            outline(
                                &mut scene,
                                &path,
                                stroke,
                                if matches!(kind, Some(NodeKind::EndEvent)) {
                                    3.0
                                } else {
                                    1.5
                                },
                            )?;
                        }
                    }
                }
                Some(
                    NodeKind::ExclusiveGateway
                    | NodeKind::ParallelGateway
                    | NodeKind::InclusiveGateway
                    | NodeKind::ComplexGateway
                    | NodeKind::EventBasedGateway,
                ) => {
                    lines(
                        &mut scene,
                        &[
                            (x + w / 2.0, y),
                            (x + w, y + h / 2.0),
                            (x + w / 2.0, y + h),
                            (x, y + h / 2.0),
                            (x + w / 2.0, y),
                        ],
                        stroke,
                        1.5,
                    )?;
                    if matches!(kind, Some(NodeKind::ParallelGateway)) {
                        lines(
                            &mut scene,
                            &[(x + w * 0.3, y + h * 0.5), (x + w * 0.7, y + h * 0.5)],
                            stroke,
                            2.0,
                        )?;
                        lines(
                            &mut scene,
                            &[(x + w * 0.5, y + h * 0.3), (x + w * 0.5, y + h * 0.7)],
                            stroke,
                            2.0,
                        )?;
                    } else if matches!(kind, Some(NodeKind::ExclusiveGateway))
                        && shape.is_marker_visible == Some(true)
                    {
                        lines(
                            &mut scene,
                            &[(x + w * 0.35, y + h * 0.35), (x + w * 0.65, y + h * 0.65)],
                            stroke,
                            2.0,
                        )?;
                        lines(
                            &mut scene,
                            &[(x + w * 0.65, y + h * 0.35), (x + w * 0.35, y + h * 0.65)],
                            stroke,
                            2.0,
                        )?;
                    }
                }
                Some(
                    NodeKind::Task
                    | NodeKind::UserTask
                    | NodeKind::ManualTask
                    | NodeKind::ServiceTask
                    | NodeKind::SendTask
                    | NodeKind::ReceiveTask
                    | NodeKind::ScriptTask
                    | NodeKind::BusinessRuleTask
                    | NodeKind::SubProcess
                    | NodeKind::AdHocSubProcess
                    | NodeKind::Transaction
                    | NodeKind::CallActivity,
                ) => {
                    let event_subprocess = matches!(kind, Some(NodeKind::SubProcess))
                        && node
                            .and_then(|n| n.activity.as_ref())
                            .is_some_and(|a| a.triggered_by_event == Some(true));
                    if event_subprocess {
                        let r = 8.0_f32.min(w / 2.0).min(h / 2.0);
                        let mut points = Vec::new();
                        for (cx, cy, start) in [
                            (x + w - r, y + r, -std::f32::consts::FRAC_PI_2),
                            (x + w - r, y + h - r, 0.0),
                            (x + r, y + h - r, std::f32::consts::FRAC_PI_2),
                            (x + r, y + r, std::f32::consts::PI),
                        ] {
                            for i in 0..=6 {
                                let a = start + i as f32 * std::f32::consts::FRAC_PI_2 / 6.0;
                                points.push((cx + r * a.cos(), cy + r * a.sin()));
                            }
                        }
                        points.push(points[0]);
                        dashed_lines(&mut scene, &points, stroke, 5.0)?;
                    } else {
                        let mut p = PathBuilder::new();
                        rounded(&mut p, x, y, w, h);
                        if let Some(path) = p.finish() {
                            outline(
                                &mut scene,
                                &path,
                                stroke,
                                if matches!(kind, Some(NodeKind::CallActivity)) {
                                    3.0
                                } else {
                                    1.5
                                },
                            )?;
                        }
                    }
                    if matches!(kind, Some(NodeKind::Transaction)) {
                        let inset = 4.0;
                        let mut inner = PathBuilder::new();
                        rounded(
                            &mut inner,
                            x + inset,
                            y + inset,
                            (w - 2.0 * inset).max(0.0),
                            (h - 2.0 * inset).max(0.0),
                        );
                        if let Some(path) = inner.finish() {
                            outline(&mut scene, &path, stroke, 1.0)?;
                        }
                    }
                }
                Some(NodeKind::DataStoreReference) => {
                    let mut p = PathBuilder::new();
                    cylinder(&mut p, x, y, w, h);
                    if let Some(path) = p.finish() {
                        outline(&mut scene, &path, stroke, 1.5)?;
                    }
                    for offset in [0.10_f32, 0.18] {
                        let mut arc = PathBuilder::new();
                        arc.move_to(x, y + h * offset);
                        arc.cubic_to(
                            x + w * 0.2,
                            y + h * (offset + 0.10),
                            x + w * 0.8,
                            y + h * (offset + 0.10),
                            x + w,
                            y + h * offset,
                        );
                        if let Some(path) = arc.finish() {
                            outline(&mut scene, &path, stroke, 1.0)?;
                        }
                    }
                }
                Some(NodeKind::TextAnnotation) => lines(
                    &mut scene,
                    &[(x + 10.0, y), (x, y), (x, y + h), (x + 10.0, y + h)],
                    stroke,
                    1.5,
                )?,
                Some(NodeKind::DataObjectReference) => lines(
                    &mut scene,
                    &[
                        (x, y),
                        (x + w - fold, y),
                        (x + w, y + fold),
                        (x + w, y + h),
                        (x, y + h),
                        (x, y),
                    ],
                    stroke,
                    1.5,
                )?,
                _ if choreography.is_some() => {
                    let mut p = PathBuilder::new();
                    rounded(&mut p, x, y, w, h);
                    if let Some(path) = p.finish() {
                        outline(
                            &mut scene,
                            &path,
                            stroke,
                            if choreography.is_some_and(|(_, a)| {
                                matches!(a.details, Some(ChoreographyActivityDetails::Call { .. }))
                            }) {
                                3.0
                            } else {
                                1.5
                            },
                        )?;
                    }
                }
                _ if conversation.is_some() => {
                    let width = if conversation
                        .is_some_and(|node| node.kind == ConversationKind::CallConversation)
                    {
                        3.0
                    } else {
                        1.5
                    };
                    lines(&mut scene, &hexagon_points(x, y, w, h), stroke, width)?
                }
                _ => lines(
                    &mut scene,
                    &[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)],
                    stroke,
                    1.5,
                )?,
            }
            match marker_kind {
                Some(NodeKind::UserTask) => {
                    let mut head = PathBuilder::new();
                    ellipse(&mut head, x + 8.0, y + 6.0, 7.0, 7.0);
                    if let Some(path) = head.finish() {
                        outline(&mut scene, &path, stroke, 1.0)?;
                    }
                    lines(
                        &mut scene,
                        &[
                            (x + 7.0, y + 19.0),
                            (x + 7.0, y + 16.0),
                            (x + 10.0, y + 13.0),
                            (x + 14.0, y + 13.0),
                            (x + 17.0, y + 16.0),
                            (x + 17.0, y + 19.0),
                        ],
                        stroke,
                        1.0,
                    )?;
                }
                Some(NodeKind::ManualTask) => {
                    lines(
                        &mut scene,
                        &[
                            (x + 7.0, y + 14.0),
                            (x + 9.0, y + 10.0),
                            (x + 11.0, y + 14.0),
                            (x + 11.0, y + 8.0),
                            (x + 13.0, y + 8.0),
                            (x + 13.0, y + 14.0),
                            (x + 15.0, y + 10.0),
                            (x + 17.0, y + 11.0),
                            (x + 16.0, y + 18.0),
                            (x + 10.0, y + 19.0),
                            (x + 7.0, y + 14.0),
                        ],
                        stroke,
                        1.0,
                    )?;
                }
                Some(NodeKind::DataObjectReference) => {
                    lines(
                        &mut scene,
                        &[
                            (x + w - fold, y),
                            (x + w - fold, y + fold),
                            (x + w, y + fold),
                        ],
                        stroke,
                        1.0,
                    )?;
                }
                None if conversation.is_none() && choreography.is_none() => {
                    if appearance.is_canvas() {
                        // Tinted header band, inside the outline.
                        let header = HEADER as f32;
                        let band = if shape.is_horizontal == Some(false) {
                            tiny_skia::Rect::from_xywh(x + 1.0, y + 1.0, w - 2.0, header - 1.0)
                        } else {
                            tiny_skia::Rect::from_xywh(x + 1.0, y + 1.0, header - 1.0, h - 2.0)
                        };
                        if let Some(band) = band.filter(|_| w > header && h > header) {
                            scene.add_path(
                                PathBuilder::from_rect(band),
                                if appearance.is_dark() {
                                    [35, 39, 45]
                                } else {
                                    [242, 244, 247]
                                },
                            )?;
                        }
                    }
                    if shape.is_horizontal == Some(false) {
                        let header = HEADER as f32;
                        lines(
                            &mut scene,
                            &[(x, y + header), (x + w, y + header)],
                            stroke,
                            1.0,
                        )?;
                    } else {
                        let header = HEADER as f32;
                        lines(
                            &mut scene,
                            &[(x + header, y), (x + header, y + h)],
                            stroke,
                            1.0,
                        )?;
                    }
                }
                _ => {}
            }
            let expandable = matches!(
                kind,
                Some(
                    NodeKind::SubProcess
                        | NodeKind::AdHocSubProcess
                        | NodeKind::Transaction
                        | NodeKind::CallActivity
                )
            );
            let collapsed = choreography.is_some_and(|(_, a)| {
                matches!(a.details, Some(ChoreographyActivityDetails::Sub { .. }))
                    && shape.is_expanded == Some(false)
            }) || expandable
                && match (kind, call_target) {
                    (Some(NodeKind::CallActivity), CalledTarget::Process) => {
                        shape.is_expanded != Some(true)
                    }
                    (Some(NodeKind::CallActivity), _) => false,
                    _ => shape.is_expanded != Some(true),
                };
            if expandable
                && shape.is_expanded.is_none()
                && matches!(kind, Some(NodeKind::CallActivity))
                && matches!(call_target, CalledTarget::Unresolved)
            {
                diagnostics.push(format!(
                    "Activity {} expansion state is unspecified; collapsed marker omitted",
                    shape.element
                ));
            }
            let loop_marker = node.and_then(|node| node.loop_characteristics.as_ref());
            let choreography_loop = choreography
                .and_then(|(_, a)| a.loop_type)
                .filter(|v| *v != ChoreographyLoopType::None);
            let compensation = node
                .and_then(|node| node.activity.as_ref())
                .is_some_and(|activity| activity.is_for_compensation == Some(true));
            let ad_hoc = matches!(kind, Some(NodeKind::AdHocSubProcess));
            let marker_count = usize::from(loop_marker.is_some() || choreography_loop.is_some())
                + usize::from(compensation)
                + usize::from(collapsed)
                + usize::from(ad_hoc);
            if marker_count > 0 {
                if w < marker_count as f32 * 18.0 || h < 30.0 {
                    diagnostics.push(format!(
                        "Activity {} is too small to show its markers",
                        shape.element
                    ));
                } else {
                    let mut cx = x + w / 2.0 - (marker_count as f32 - 1.0) * 9.0;
                    let cy = if choreography.is_some() {
                        let bottom_band = diagram.shapes.iter().filter(|band| {
                            let owner = match &band.choreography_activity_shape {
                                Some(processcraft_core::model::DiReference::Local(id)) => id == &shape.id.selector(),
                                Some(processcraft_core::model::DiReference::Qualified(q)) => document.definitions.as_ref().is_some_and(|defs| defs.target_namespace == q.namespace) && shape.id.xml_id() == Some(&q.local),
                                None => false,
                            };
                            owner && matches!(band.participant_band_kind, Some(processcraft_core::model::ParticipantBandKind::BottomInitiating | processcraft_core::model::ParticipantBandKind::BottomNonInitiating))
                        }).map(|band| band.bounds.y as f32 - origin.1 as f32).fold(y + h, f32::min);
                        bottom_band - 10.0
                    } else {
                        y + h - 10.0
                    };
                    if let Some(characteristics) = loop_marker {
                        draw_loop_marker(&mut scene, characteristics, cx, cy, stroke)?;
                        cx += 18.0;
                    }
                    if let Some(marker) = choreography_loop {
                        draw_choreography_loop(&mut scene, marker, cx, cy, stroke)?;
                        cx += 18.0;
                    }
                    if compensation {
                        for dx in [-5.0_f32, 1.0] {
                            lines(
                                &mut scene,
                                &[
                                    (cx + dx + 5.0, cy - 5.0),
                                    (cx + dx, cy),
                                    (cx + dx + 5.0, cy + 5.0),
                                    (cx + dx + 5.0, cy - 5.0),
                                ],
                                stroke,
                                1.1,
                            )?;
                        }
                        cx += 18.0;
                    }
                    if collapsed {
                        lines(
                            &mut scene,
                            &[
                                (cx - 6.0, cy - 6.0),
                                (cx + 6.0, cy - 6.0),
                                (cx + 6.0, cy + 6.0),
                                (cx - 6.0, cy + 6.0),
                                (cx - 6.0, cy - 6.0),
                            ],
                            stroke,
                            1.0,
                        )?;
                        lines(&mut scene, &[(cx - 4.0, cy), (cx + 4.0, cy)], stroke, 1.0)?;
                        lines(&mut scene, &[(cx, cy - 4.0), (cx, cy + 4.0)], stroke, 1.0)?;
                        cx += 18.0;
                    }
                    if ad_hoc {
                        lines(
                            &mut scene,
                            &[
                                (cx - 7.0, cy),
                                (cx - 4.0, cy - 2.0),
                                (cx, cy + 2.0),
                                (cx + 4.0, cy + 2.0),
                                (cx + 7.0, cy),
                            ],
                            stroke,
                            1.2,
                        )?;
                    }
                }
            }
            let has_explicit_bands =
                diagram
                    .shapes
                    .iter()
                    .any(|band| match &band.choreography_activity_shape {
                        Some(processcraft_core::model::DiReference::Local(id)) => {
                            id == &shape.id.selector()
                        }
                        Some(processcraft_core::model::DiReference::Qualified(q)) => {
                            document
                                .definitions
                                .as_ref()
                                .is_some_and(|defs| defs.target_namespace == q.namespace)
                                && shape.id.xml_id() == Some(&q.local)
                        }
                        None => false,
                    });
            if choreography.is_some() && !has_explicit_bands && diagram.plane_order.is_some() {
                diagnostics.push(format!(
                    "Choreography {} has no explicit participant band representations",
                    shape.element
                ));
            }
            if let Some((scope, activity)) = choreography.filter(|(root, _)| {
                root.graph.is_none() && !has_explicit_bands && diagram.plane_order.is_none()
            }) {
                if activity.participant_refs.len() != 2 {
                    diagnostics.push(format!(
                        "Choreography {} participant band order is not represented",
                        activity.id
                    ));
                }
                let initiating = semantics.resolved(&activity.initiating_participant);
                let other = activity
                    .participant_refs
                    .iter()
                    .filter_map(|r| semantics.resolved(&Some(r.clone())))
                    .find(|id| Some(id) != initiating.as_ref());
                let top = scope
                    .participants
                    .iter()
                    .find(|p| Some(&p.id.selector()) == initiating.as_ref())
                    .map(|p| p.name.as_deref().unwrap_or(""));
                let bottom = other
                    .and_then(|id| scope.participants.iter().find(|p| p.id == id))
                    .map(|p| p.name.as_deref().unwrap_or(""));
                let band_h = (h * 0.22).clamp(12.0, 24.0);
                lines(
                    &mut scene,
                    &[(x, y + band_h), (x + w, y + band_h)],
                    stroke,
                    1.0,
                )?;
                lines(
                    &mut scene,
                    &[(x, y + h - band_h), (x + w, y + h - band_h)],
                    stroke,
                    1.0,
                )?;
                for (name, by) in [(top, b.y), (bottom, b.y + b.height - band_h as f64)] {
                    if let Some(name) = name {
                        let band = Bounds {
                            x: b.x,
                            y: by,
                            width: b.width,
                            height: band_h as f64,
                        };
                        label(
                            &mut scene,
                            fonts,
                            family,
                            name,
                            &band,
                            origin,
                            shape.style.as_ref(),
                            None,
                            fg,
                            LabelAlign::TopLeft,
                            &mut diagnostics,
                        )?;
                    }
                }
                if !activity.message_flow_refs.is_empty() {
                    diagnostics.push(format!(
                        "Choreography {} message-band visibility is not represented",
                        activity.id
                    ));
                }
            }
            if conversation.is_some_and(|node| node.kind == ConversationKind::SubConversation)
                && shape.is_expanded.is_none()
            {
                diagnostics.push(format!(
                    "Sub-conversation {} expansion state is unspecified; collapsed marker omitted",
                    shape.element
                ));
            }
            if conversation.is_some_and(|node| node.kind == ConversationKind::SubConversation)
                && shape.is_expanded == Some(false)
            {
                let (cx, cy) = (x + w / 2.0, y + h - 10.0);
                let pts = [
                    (cx - 5.0, cy - 5.0),
                    (cx + 5.0, cy - 5.0),
                    (cx + 5.0, cy + 5.0),
                    (cx - 5.0, cy + 5.0),
                    (cx - 5.0, cy - 5.0),
                ];
                lines(&mut scene, &pts, stroke, 1.0)?;
                lines(&mut scene, &[(cx - 3.0, cy), (cx + 3.0, cy)], stroke, 1.0)?;
                lines(&mut scene, &[(cx, cy - 3.0), (cx, cy + 3.0)], stroke, 1.0)?;
            }
            if let Some((port, output)) = port {
                let r = w.min(h) * 0.12;
                let (ax, ay) = (x + w * 0.3, y + h * 0.3);
                let arrow = [
                    (ax - r, ay - r * 0.45),
                    (ax, ay - r * 0.45),
                    (ax, ay - r),
                    (ax + r, ay),
                    (ax, ay + r),
                    (ax, ay + r * 0.45),
                    (ax - r, ay + r * 0.45),
                    (ax - r, ay - r * 0.45),
                ];
                marker_polygon(&mut scene, &arrow, stroke, output)?;
                if port.is_collection == Some(true) {
                    for offset in [-4.0_f32, 0.0, 4.0] {
                        lines(
                            &mut scene,
                            &[
                                (x + w / 2.0 + offset, y + h - 12.0),
                                (x + w / 2.0 + offset, y + h - 4.0),
                            ],
                            stroke,
                            1.2,
                        )?;
                    }
                }
            }
            if let Some(NodeDetails::DataObjectReference {
                data_object_ref: Some(id),
            }) = details
                && semantics
                    .data_object(id)
                    .is_some_and(|object| object.is_collection == Some(true))
            {
                for offset in [-4.0_f32, 0.0, 4.0] {
                    lines(
                        &mut scene,
                        &[
                            (x + w / 2.0 + offset, y + h - 12.0),
                            (x + w / 2.0 + offset, y + h - 4.0),
                        ],
                        stroke,
                        1.2,
                    )?;
                }
            }
            let effective = match effective_event_definitions(details, document) {
                Ok(effective) => Some(effective),
                Err(()) => {
                    diagnostics.push(format!(
                        "Event {} has an unresolved event definition reference; marker omitted",
                        shape.element
                    ));
                    None
                }
            };
            draw_new_markers(
                &mut scene,
                marker_kind,
                details,
                effective,
                node.and_then(|node| node.activity.as_ref())
                    .is_some_and(|activity| activity.instantiate == Some(true)),
                x,
                y,
                w,
                h,
                stroke,
                fc,
                &mut diagnostics,
            )?;
            let is_lane_or_pool = semantics.lane(&shape.element).is_some()
                || semantics.participant(&shape.element).is_some();
            if semantics
                .participant(&shape.element)
                .and_then(|p| p.multiplicity.as_ref())
                .and_then(|m| m.maximum)
                .is_some_and(|maximum| maximum > 1)
            {
                for offset in [-3.0_f32, 0.0, 3.0] {
                    lines(
                        &mut scene,
                        &[
                            (x + w / 2.0 + offset, y + h - 9.0),
                            (x + w / 2.0 + offset, y + h - 3.0),
                        ],
                        stroke,
                        1.0,
                    )?;
                }
            }
            let (label_bounds, label_align) = shape_label_layout(
                shape,
                kind,
                marker_kind,
                node,
                choreography.is_some(),
                is_lane_or_pool,
            );
            let lb = &label_bounds;
            let style = shape
                .label
                .as_ref()
                .and_then(|l| l.style.as_ref())
                .or(shape.style.as_ref());
            if shape_labels && label_visible(shape.label.as_ref(), shape.label_visibility) {
                let rotate_pool_label = is_lane_or_pool
                    && shape.is_horizontal != Some(false)
                    && shape
                        .label
                        .as_ref()
                        .and_then(|l| l.bounds.as_ref())
                        .is_none();
                let rotated_bounds = Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: lb.height,
                    height: lb.width,
                };
                label(
                    &mut scene,
                    fonts,
                    family,
                    name,
                    if rotate_pool_label {
                        &rotated_bounds
                    } else {
                        lb
                    },
                    if rotate_pool_label {
                        (0.0, 0.0)
                    } else {
                        origin
                    },
                    style,
                    label_font(document, diagram, shape.label.as_ref(), &mut diagnostics),
                    fg,
                    label_align,
                    &mut diagnostics,
                )?;
                if rotate_pool_label && !name.is_empty() {
                    scene.rotate_last_label(
                        (lb.x - origin.0) as f32,
                        (lb.y + lb.height - origin.1) as f32,
                    );
                }
            }
            hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: shape.element.clone(),
                },
                diagram_element_id: shape.id.selector(),
                label_id: None,
                is_label: false,
                geometry: match kind {
                    Some(
                        NodeKind::StartEvent
                        | NodeKind::EndEvent
                        | NodeKind::IntermediateCatchEvent
                        | NodeKind::IntermediateThrowEvent
                        | NodeKind::BoundaryEvent,
                    ) => HitGeometry::Ellipse(b.clone()),
                    Some(
                        NodeKind::ExclusiveGateway
                        | NodeKind::ParallelGateway
                        | NodeKind::InclusiveGateway
                        | NodeKind::ComplexGateway
                        | NodeKind::EventBasedGateway,
                    ) => HitGeometry::Diamond(b.clone()),
                    Some(NodeKind::DataObjectReference) => HitGeometry::DataObject(b.clone()),
                    Some(NodeKind::DataStoreReference) => HitGeometry::DataStore(b.clone()),
                    _ if conversation.is_some() => HitGeometry::Hexagon(b.clone()),
                    _ => HitGeometry::Shape(b.clone()),
                },
            });
            if shape_labels
                && label_visible(shape.label.as_ref(), shape.label_visibility)
                && !name.is_empty()
            {
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.selector(),
                    label_id: shape
                        .label
                        .as_ref()
                        .and_then(|label| label.id.as_ref())
                        .map(|id| id.selector()),
                    is_label: true,
                    geometry: HitGeometry::Shape(lb.clone()),
                });
            }
        }
        for edge in &diagram.edges {
            if let Some(ranks) = &paint_ranks {
                scene.set_paint_rank(semantics.edge_paint_rank(ranks, edge));
            }
            if semantics.edge_visibility(edge, diagram).0 {
                continue;
            }
            if semantics.data_association(&edge.element).is_some() {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                let points: Vec<_> = edge
                    .waypoints
                    .iter()
                    .map(|p| ((p.x - origin.0) as f32, (p.y - origin.1) as f32))
                    .collect();
                dashed_lines(&mut scene, &points, stroke, 2.0)?;
                open_arrow(&mut scene, &points, stroke)?;
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: edge.element.clone(),
                    },
                    diagram_element_id: edge.id.selector(),
                    label_id: None,
                    is_label: false,
                    geometry: HitGeometry::Edge(edge.waypoints.clone()),
                });
                continue;
            }
            if let Some(link) = semantics.conversation_link(&edge.element) {
                if semantics.resolved(&link.source).is_none()
                    || semantics.resolved(&link.target).is_none()
                {
                    diagnostics.push(format!("Conversation link {} has an unresolved endpoint; authored DI route is retained", link.id));
                }
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                let points: Vec<_> = edge
                    .waypoints
                    .iter()
                    .map(|p| ((p.x - origin.0) as f32, (p.y - origin.1) as f32))
                    .collect();
                double_link(&mut scene, &points, stroke)?;
                if label_visible(edge.label.as_ref(), edge.label_visibility)
                    && link.name.as_deref().is_some_and(|name| !name.is_empty())
                    && let Some((bounds, align)) = edge_label_layout(edge)
                {
                    let bounds = &bounds;
                    label(
                        &mut scene,
                        fonts,
                        family,
                        link.name.as_deref().unwrap_or(""),
                        bounds,
                        origin,
                        edge.label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(edge.style.as_ref()),
                        label_font(document, diagram, edge.label.as_ref(), &mut diagnostics),
                        fg,
                        align,
                        &mut diagnostics,
                    )?;
                    if label_visible(edge.label.as_ref(), edge.label_visibility) {
                        hits.push(HitRegion {
                            key: ElementKey {
                                document_id: document.id,
                                xml_id: edge.element.clone(),
                            },
                            diagram_element_id: edge.id.selector(),
                            label_id: edge
                                .label
                                .as_ref()
                                .and_then(|label| label.id.as_ref())
                                .map(|id| id.selector()),
                            is_label: true,
                            geometry: HitGeometry::Shape(bounds.clone()),
                        });
                    }
                }
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: edge.element.clone(),
                    },
                    diagram_element_id: edge.id.selector(),
                    label_id: None,
                    is_label: false,
                    geometry: HitGeometry::Edge(edge.waypoints.clone()),
                });
                continue;
            }
            let flow = semantics.flow(&edge.element);
            let Some(flow) = flow else {
                diagnostics.push(format!("Unknown edge element {}", edge.element));
                continue;
            };
            if semantics.resolved(&flow.source).is_none()
                || semantics.resolved(&flow.target).is_none()
            {
                diagnostics.push(format!(
                    "Flow {} has an unresolved endpoint; authored DI route is retained",
                    flow.id
                ));
            }
            let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
            let pts: Vec<_> = edge
                .waypoints
                .iter()
                .map(|p| ((p.x - origin.0) as f32, (p.y - origin.1) as f32))
                .collect();
            if matches!(flow.kind, FlowKind::Sequence) {
                lines(&mut scene, &pts, stroke, 1.5)?;
            } else {
                dashed_lines(
                    &mut scene,
                    &pts,
                    stroke,
                    if matches!(flow.kind, FlowKind::Message) {
                        7.0
                    } else {
                        2.0
                    },
                )?;
            }
            if matches!(flow.kind, FlowKind::Sequence) && semantics.is_default_flow(flow) {
                draw_default_slash(&mut scene, &pts, stroke)?;
            }
            if matches!(flow.kind, FlowKind::Message)
                && let Some(visible) = edge.message_visible_kind
                && let Some((cx, cy)) = polyline_midpoint(&pts)
            {
                let filled = matches!(
                    visible,
                    processcraft_core::model::MessageVisibleKind::NonInitiating
                );
                let explicit = edge.style.as_ref().and_then(|s| s.stroke.as_deref());
                let envelope_stroke = if filled && explicit.is_none() {
                    [26, 37, 52]
                } else {
                    stroke
                };
                if filled && explicit.is_some() && low_contrast(envelope_stroke, [225, 225, 225]) {
                    diagnostics.push(format!("Message edge {} explicit stroke has low contrast against the envelope fill", edge.id));
                }
                envelope(&mut scene, cx, cy, envelope_stroke, filled, bg)?;
                if let Some(text) = semantics.message_name(flow) {
                    message_name_label(
                        &mut scene,
                        fonts,
                        family,
                        text,
                        (cx, cy),
                        origin,
                        fg,
                        &mut diagnostics,
                    )?;
                }
            }
            if matches!(flow.kind, FlowKind::Message) || semantics.conditional_flow_marker(flow) {
                draw_flow_source_marker(
                    &mut scene,
                    &pts,
                    stroke,
                    bg,
                    matches!(flow.kind, FlowKind::Message),
                )?;
            }
            if (!matches!(flow.kind, FlowKind::Association)
                || matches!(
                    flow.association_direction,
                    Some(AssociationDirection::One | AssociationDirection::Both)
                ))
                && let Some((a, b)) = last_segment(&pts)
            {
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
                if matches!(flow.kind, FlowKind::Sequence) {
                    let mut arrow = PathBuilder::new();
                    polygon(&mut arrow, &[wing(0.5), b, wing(-0.5)]);
                    if let Some(path) = arrow.finish() {
                        scene.add_path(path, stroke)?;
                    }
                } else {
                    lines(&mut scene, &[wing(0.5), b, wing(-0.5)], stroke, 1.5)?;
                }
            }
            if matches!(flow.kind, FlowKind::Association)
                && matches!(flow.association_direction, Some(AssociationDirection::Both))
                && let Some((a, b)) = last_segment(&pts.iter().rev().copied().collect::<Vec<_>>())
            {
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
                lines(&mut scene, &[wing(0.5), b, wing(-0.5)], stroke, 1.5)?;
            }
            if label_visible(edge.label.as_ref(), edge.label_visibility)
                && flow.name.as_deref().is_some_and(|name| !name.is_empty())
                && let Some((bounds, align)) = edge_label_layout(edge)
            {
                let bounds = &bounds;
                label(
                    &mut scene,
                    fonts,
                    family,
                    flow.name.as_deref().unwrap_or(""),
                    bounds,
                    origin,
                    edge.label
                        .as_ref()
                        .and_then(|l| l.style.as_ref())
                        .or(edge.style.as_ref()),
                    label_font(document, diagram, edge.label.as_ref(), &mut diagnostics),
                    fg,
                    align,
                    &mut diagnostics,
                )?;
                if label_visible(edge.label.as_ref(), edge.label_visibility) {
                    hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: edge
                            .label
                            .as_ref()
                            .and_then(|label| label.id.as_ref())
                            .map(|id| id.selector()),
                        is_label: true,
                        geometry: HitGeometry::Shape(bounds.clone()),
                    });
                }
            }
            hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: edge.element.clone(),
                },
                diagram_element_id: edge.id.selector(),
                label_id: None,
                is_label: false,
                geometry: HitGeometry::Edge(edge.waypoints.clone()),
            });
        }
        if let Some(ranks) = &paint_ranks {
            hits.sort_by_key(|hit| {
                *ranks
                    .get(&hit.diagram_element_id)
                    .unwrap_or(&(ranks.len() + 1))
            });
        }
        Ok(Self {
            scene,
            origin,
            hits,
            diagnostics,
            capability: document.capability.clone(),
        })
    }
    pub fn hit_test(&self, x: f64, y: f64, tolerance: f64) -> Option<&HitRegion> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.contains(x, y, tolerance.max(0.0)))
    }
}
fn group_label_collision(
    bounds: &Bounds,
    shape_id: &ElementSelector,
    diagram: &processcraft_core::model::Diagram,
    semantics: &SemanticIndex<'_>,
) -> bool {
    diagram
        .shapes
        .iter()
        .filter(|shape| {
            &shape.id != shape_id
                && semantics.node(&shape.element).is_some()
                && !semantics.shape_visibility(shape, diagram).0
        })
        .any(|shape| {
            let b = &shape.bounds;
            bounds.x < b.x + b.width
                && bounds.x + bounds.width > b.x
                && bounds.y < b.y + b.height
                && bounds.y + bounds.height > b.y
        })
}
fn color(value: Option<&str>, default: [u8; 3]) -> [u8; 3] {
    value
        .and_then(|s| {
            let s = s.strip_prefix('#')?;
            if s.len() != 6 || !s.is_ascii() {
                return None;
            }
            Some([
                u8::from_str_radix(&s[0..2], 16).ok()?,
                u8::from_str_radix(&s[2..4], 16).ok()?,
                u8::from_str_radix(&s[4..6], 16).ok()?,
            ])
        })
        .unwrap_or(default)
}
fn low_contrast(a: [u8; 3], b: [u8; 3]) -> bool {
    fn luminance(c: [u8; 3]) -> f64 {
        let channel = |v: u8| {
            let x = f64::from(v) / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(c[0]) * 0.2126 + channel(c[1]) * 0.7152 + channel(c[2]) * 0.0722
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05) < 3.0
}
#[allow(clippy::too_many_arguments)]
/// Message name beside an envelope decorator (BPMN 2.0.2 section 12.3.2 BPMNShape,
/// Choreography Participant Bands: "The name of the message may be used as a label").
fn message_name_label(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    center: (f32, f32),
    origin: (f64, f64),
    fg: [u8; 3],
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let bounds = Bounds {
        x: origin.0 + f64::from(center.0) + 14.0,
        y: origin.1 + f64::from(center.1) - 8.0,
        width: 120.0,
        height: 16.0,
    };
    label(
        scene,
        fonts,
        family,
        text,
        &bounds,
        origin,
        None,
        None,
        fg,
        LabelAlign::TopLeft,
        diagnostics,
    )
}

#[allow(clippy::too_many_arguments)]
fn label(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    b: &Bounds,
    origin: (f64, f64),
    style: Option<&Style>,
    di_font: Option<&processcraft_core::model::DiFont>,
    fg: [u8; 3],
    align: LabelAlign,
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    if text.is_empty() {
        return Ok(());
    }
    let center = align == LabelAlign::Center;
    let size = match di_font.and_then(|font| font.size) {
        Some(size) if size.is_finite() && (4.0..=256.0).contains(&size) => size as f32,
        Some(_) => {
            diagnostics
                .push("Label font size is outside the supported range; using default".into());
            12.0
        }
        None => 12.0,
    };
    let mut request = TextRequest::new(text, size, ((b.width as f32) - 8.0).clamp(4.0, 16384.0));
    request.family = di_font
        .and_then(|font| font.name.as_deref())
        .unwrap_or(family);
    request.bold = di_font.is_some_and(|font| font.is_bold == Some(true));
    request.italic = di_font.is_some_and(|font| font.is_italic == Some(true));
    request.center = align != LabelAlign::TopLeft;
    let mut layout = fonts.layout(request)?;
    // Preserve glyph descenders when the text itself fits the label box.
    // Padding is discretionary and must not consume space needed by outlines.
    let required_height = layout
        .glyphs()
        .iter()
        .filter_map(|g| g.outline.as_ref())
        .fold(layout.height(), |height, path| {
            height.max(path.bounds().bottom())
        });
    let vertical_padding = (((b.height as f32) - required_height) / 2.0).clamp(0.0, 4.0);
    let vertical_offset = if center {
        (((b.height as f32) - vertical_padding * 2.0 - required_height) / 2.0).max(0.0)
    } else {
        0.0
    };
    diagnostics.extend(layout.diagnostics().iter().cloned());
    let (fit_w, fit_h) = (
        ((b.width as f32) - 8.0).max(0.0),
        ((b.height as f32) - vertical_padding * 2.0 - vertical_offset).max(0.0),
    );
    let mut ellipsized = false;
    if layout.overflows(fit_w, fit_h) {
        // Replace the hidden tail with an ellipsis: the longest prefix that still fits
        // wins (binary search, so at most ~log2(chars) extra layouts).
        let ends: Vec<_> = text.char_indices().map(|(i, _)| i).skip(1).collect();
        let (mut lo, mut hi, mut best) = (0, ends.len(), None);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let candidate = format!("{}\u{2026}", text[..ends[mid]].trim_end());
            let attempt = fonts.layout(TextRequest {
                text: &candidate,
                ..request
            })?;
            if attempt.overflows(fit_w, fit_h) {
                hi = mid;
            } else {
                best = Some(attempt);
                lo = mid + 1;
            }
        }
        if let Some(best) = best {
            layout = best;
            layout.set_source_text(text);
            ellipsized = true;
        }
    }
    if layout.clip_to_bounds(fit_w, fit_h) || ellipsized {
        diagnostics.push(format!(
            "Label clipped because it exceeds its bounds: {text}"
        ));
    }
    let (x, y) = (
        (b.x - origin.0) as f32 + 4.0,
        (b.y - origin.1) as f32 + vertical_padding + vertical_offset,
    );
    let ink = color(style.and_then(|s| s.font.as_deref()), fg);
    let mut decorations = Vec::new();
    if let Some(font) = di_font
        && (font.is_underline == Some(true) || font.is_strike_through == Some(true))
    {
        for line in 0..layout.line_count() {
            let glyphs: Vec<_> = layout
                .glyphs()
                .iter()
                .filter(|glyph| glyph.visual_line == line)
                .collect();
            if glyphs.is_empty() {
                continue;
            }
            let left = glyphs
                .iter()
                .map(|glyph| glyph.x)
                .fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.advance)
                .fold(f32::NEG_INFINITY, f32::max);
            let baseline = glyphs[0].baseline;
            if font.is_underline == Some(true) {
                decorations.push((left, right, baseline + size * 0.10));
            }
            if font.is_strike_through == Some(true) {
                decorations.push((left, right, baseline - size * 0.30));
            }
        }
    }
    scene.add_colored_label(layout, x, y, ink)?;
    for (left, right, line_y) in decorations {
        if line_y >= 0.0 && line_y <= (b.height as f32 - 8.0 - vertical_offset).max(0.0) {
            lines(
                scene,
                &[(x + left, y + line_y), (x + right, y + line_y)],
                ink,
                (size / 16.0).max(0.75),
            )?;
        } else {
            diagnostics.push(format!(
                "Label decoration clipped because it exceeds its bounds: {text}"
            ));
        }
    }
    Ok(())
}
fn cylinder(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    let r = (h * 0.12).min(10.0);
    p.move_to(x, y + r);
    p.cubic_to(x, y - r * 0.33, x + w, y - r * 0.33, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h + r * 0.33, x, y + h + r * 0.33, x, y + h - r);
    p.close();
}
fn hexagon_points(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 7] {
    [
        (x + w * 0.25, y),
        (x + w * 0.75, y),
        (x + w, y + h * 0.5),
        (x + w * 0.75, y + h),
        (x + w * 0.25, y + h),
        (x, y + h * 0.5),
        (x + w * 0.25, y),
    ]
}
fn draw_default_slash(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let a = points[0];
    let b = points[1];
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = dx.hypot(dy);
    if length <= 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / length, dy / length);
    let distance = (length * 0.35).min(12.0);
    let center = (a.0 + ux * distance, a.1 + uy * distance);
    let (sx, sy) = ((ux - uy) * 4.0, (uy + ux) * 4.0);
    lines(
        scene,
        &[
            (center.0 - sx, center.1 - sy),
            (center.0 + sx, center.1 + sy),
        ],
        stroke,
        1.5,
    )
}
fn draw_flow_source_marker(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
    background: [u8; 3],
    message: bool,
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let a = points[0];
    let dx = points[1].0 - a.0;
    let dy = points[1].1 - a.1;
    let length = dx.hypot(dy);
    if length == 0.0 {
        return Ok(());
    }
    if message {
        filled_ellipse(scene, a.0 - 4.0, a.1 - 4.0, 8.0, 8.0, background)?;
        return ring(scene, a.0 - 4.0, a.1 - 4.0, 8.0, 8.0, stroke, 1.2);
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy * 4.0, ux * 4.0);
    let mid = (a.0 + ux * 6.0, a.1 + uy * 6.0);
    let end = (a.0 + ux * 12.0, a.1 + uy * 12.0);
    let mut interior = PathBuilder::new();
    polygon(
        &mut interior,
        &[a, (mid.0 + nx, mid.1 + ny), end, (mid.0 - nx, mid.1 - ny)],
    );
    if let Some(path) = interior.finish() {
        scene.add_path(path, background)?;
    }
    lines(
        scene,
        &[
            a,
            (mid.0 + nx, mid.1 + ny),
            end,
            (mid.0 - nx, mid.1 - ny),
            a,
        ],
        stroke,
        1.2,
    )
}
fn open_arrow(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
) -> Result<(), RenderError> {
    if let Some((a, b)) = last_segment(points) {
        let angle = (b.1 - a.1).atan2(b.0 - a.0);
        let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
        lines(scene, &[wing(0.5), b, wing(-0.5)], stroke, 1.5)?;
    }
    Ok(())
}
fn double_link(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
) -> Result<(), RenderError> {
    for pair in points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        let length = dx.hypot(dy);
        if length == 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / length * 2.0, dx / length * 2.0);
        for sign in [-1.0, 1.0] {
            lines(
                scene,
                &[
                    (pair[0].0 + nx * sign, pair[0].1 + ny * sign),
                    (pair[1].0 + nx * sign, pair[1].1 + ny * sign),
                ],
                color,
                1.1,
            )?;
        }
    }
    Ok(())
}
fn polygon(p: &mut PathBuilder, points: &[(f32, f32)]) {
    p.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        p.line_to(x, y)
    }
    p.close()
}
fn ring(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    let mut p = PathBuilder::new();
    ellipse(&mut p, x, y, w, h);
    if let Some(path) = p.finish() {
        outline(scene, &path, color, width)?;
    }
    Ok(())
}
fn filled_ellipse(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    let mut p = PathBuilder::new();
    ellipse(&mut p, x, y, w, h);
    if let Some(path) = p.finish() {
        scene.add_path(path, color)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn draw_new_markers(
    scene: &mut DiagramScene,
    kind: Option<NodeKind>,
    details: Option<&NodeDetails>,
    effective: Option<(Vec<&EventDefinition>, bool)>,
    instantiate: bool,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: [u8; 3],
    fill: [u8; 3],
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    match kind {
        Some(
            NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent,
        ) => {
            let inset = (w.min(h) * 0.10).max(2.0);
            if matches!(kind, Some(NodeKind::BoundaryEvent)) && non_interrupting(details) {
                dashed_ellipse(
                    scene,
                    x + inset,
                    y + inset,
                    (w - 2.0 * inset).max(0.0),
                    (h - 2.0 * inset).max(0.0),
                    stroke,
                )?;
            } else {
                ring(
                    scene,
                    x + inset,
                    y + inset,
                    (w - 2.0 * inset).max(0.0),
                    (h - 2.0 * inset).max(0.0),
                    stroke,
                    1.2,
                )?;
            }
        }
        Some(NodeKind::InclusiveGateway) => {
            let r = w.min(h) * 0.19;
            ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, 1.6)?;
        }
        Some(NodeKind::ComplexGateway) => {
            let r = w.min(h) * 0.2;
            for angle in [
                0.0_f32,
                std::f32::consts::FRAC_PI_4,
                std::f32::consts::FRAC_PI_2,
                3.0 * std::f32::consts::FRAC_PI_4,
            ] {
                let (s, c) = angle.sin_cos();
                lines(
                    scene,
                    &[(cx - r * c, cy - r * s), (cx + r * c, cy + r * s)],
                    stroke,
                    2.0,
                )?;
            }
        }
        Some(NodeKind::EventBasedGateway) => {
            let (instantiate, parallel) = match details {
                Some(NodeDetails::Gateway {
                    instantiate,
                    event_gateway_type,
                    ..
                }) => (
                    instantiate.unwrap_or(false),
                    *event_gateway_type == Some(EventGatewayType::Parallel),
                ),
                _ => (false, false),
            };
            let r = w.min(h) * 0.25;
            ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, 1.2)?;
            if !instantiate {
                ring(
                    scene,
                    cx - r * 0.82,
                    cy - r * 0.82,
                    1.64 * r,
                    1.64 * r,
                    stroke,
                    1.0,
                )?;
            }
            if parallel {
                lines(
                    scene,
                    &[(cx - r * 0.55, cy), (cx + r * 0.55, cy)],
                    stroke,
                    1.6,
                )?;
                lines(
                    scene,
                    &[(cx, cy - r * 0.55), (cx, cy + r * 0.55)],
                    stroke,
                    1.6,
                )?;
            } else {
                let points: Vec<_> = (0..=5)
                    .map(|i| {
                        let a =
                            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
                        (cx + a.cos() * r * 0.55, cy + a.sin() * r * 0.55)
                    })
                    .collect();
                lines(scene, &points, stroke, 1.2)?;
            }
        }
        Some(NodeKind::ServiceTask) => {
            let (mx, my) = (x + 14.0, y + 13.0);
            ring(scene, mx - 4.0, my - 4.0, 8.0, 8.0, stroke, 1.2)?;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                lines(
                    scene,
                    &[
                        (mx + a.cos() * 4.0, my + a.sin() * 4.0),
                        (mx + a.cos() * 7.0, my + a.sin() * 7.0),
                    ],
                    stroke,
                    1.8,
                )?;
            }
        }
        Some(NodeKind::SendTask | NodeKind::ReceiveTask) => {
            if matches!(kind, Some(NodeKind::ReceiveTask)) && instantiate {
                ring(scene, x + 3.0, y + 2.0, 22.0, 22.0, stroke, 1.2)?;
            }
            let pts = [
                (x + 7.0, y + 8.0),
                (x + 21.0, y + 8.0),
                (x + 21.0, y + 18.0),
                (x + 7.0, y + 18.0),
                (x + 7.0, y + 8.0),
            ];
            if matches!(kind, Some(NodeKind::SendTask)) {
                let mut p = PathBuilder::new();
                polygon(&mut p, &pts);
                if let Some(p) = p.finish() {
                    scene.add_path(p, stroke)?;
                }
                lines(
                    scene,
                    &[
                        (x + 7.0, y + 8.0),
                        (x + 14.0, y + 13.0),
                        (x + 21.0, y + 8.0),
                    ],
                    fill,
                    1.0,
                )?;
            } else {
                lines(scene, &pts, stroke, 1.2)?;
                lines(
                    scene,
                    &[
                        (x + 7.0, y + 8.0),
                        (x + 14.0, y + 13.0),
                        (x + 21.0, y + 8.0),
                    ],
                    stroke,
                    1.0,
                )?;
            }
        }
        Some(NodeKind::ScriptTask) => {
            lines(
                scene,
                &[
                    (x + 8.0, y + 7.0),
                    (x + 19.0, y + 7.0),
                    (x + 19.0, y + 19.0),
                    (x + 8.0, y + 19.0),
                    (x + 8.0, y + 7.0),
                ],
                stroke,
                1.2,
            )?;
            for yy in [10.0, 13.0, 16.0] {
                lines(
                    scene,
                    &[(x + 10.0, y + yy), (x + 17.0, y + yy)],
                    stroke,
                    0.8,
                )?;
            }
        }
        Some(NodeKind::BusinessRuleTask) => {
            lines(
                scene,
                &[
                    (x + 7.0, y + 7.0),
                    (x + 21.0, y + 7.0),
                    (x + 21.0, y + 19.0),
                    (x + 7.0, y + 19.0),
                    (x + 7.0, y + 7.0),
                ],
                stroke,
                1.2,
            )?;
            for yy in [11.0, 15.0] {
                lines(scene, &[(x + 7.0, y + yy), (x + 21.0, y + yy)], stroke, 0.8)?;
            }
            lines(
                scene,
                &[(x + 14.0, y + 7.0), (x + 14.0, y + 19.0)],
                stroke,
                0.8,
            )?;
        }
        _ => {}
    }
    if let Some((definitions, parallel_multiple)) = effective {
        if definitions.len() > 1 && parallel_multiple {
            let r = w.min(h) * 0.26;
            lines(scene, &[(cx - r, cy), (cx + r, cy)], stroke, 3.0)?;
            lines(scene, &[(cx, cy - r), (cx, cy + r)], stroke, 3.0)?;
        } else if definitions.len() > 1 {
            let r = w.min(h) * 0.26;
            let pts: Vec<_> = (0..=5)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
                    (cx + a.cos() * r, cy + a.sin() * r)
                })
                .collect();
            if matches!(
                kind,
                Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
            ) {
                let mut path = PathBuilder::new();
                polygon(&mut path, &pts);
                if let Some(path) = path.finish() {
                    scene.add_path(path, stroke)?;
                }
            } else {
                lines(scene, &pts, stroke, 1.2)?;
            }
        }
        for definition in definitions.iter().filter(|_| definitions.len() == 1) {
            match definition {
                EventDefinition::Message { .. } => {
                    let r = w.min(h) * 0.25;
                    let pts = [
                        (cx - r, cy - r * 0.65),
                        (cx + r, cy - r * 0.65),
                        (cx + r, cy + r * 0.65),
                        (cx - r, cy + r * 0.65),
                        (cx - r, cy - r * 0.65),
                    ];
                    let throwing = matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    );
                    if throwing {
                        let mut path = PathBuilder::new();
                        polygon(&mut path, &pts);
                        if let Some(path) = path.finish() {
                            scene.add_path(path, stroke)?;
                        }
                    } else {
                        lines(scene, &pts, stroke, 1.1)?;
                    }
                    lines(
                        scene,
                        &[(cx - r, cy - r * 0.65), (cx, cy), (cx + r, cy - r * 0.65)],
                        if throwing { fill } else { stroke },
                        1.0,
                    )?;
                }
                EventDefinition::Signal { .. } => {
                    let r = w.min(h) * 0.25;
                    let pts = [
                        (cx, cy - r),
                        (cx + r, cy + r),
                        (cx - r, cy + r),
                        (cx, cy - r),
                    ];
                    if matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    ) {
                        let mut path = PathBuilder::new();
                        polygon(&mut path, &pts);
                        if let Some(path) = path.finish() {
                            scene.add_path(path, stroke)?;
                        }
                    } else {
                        lines(scene, &pts, stroke, 1.2)?;
                    }
                }
                EventDefinition::Error { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx - r * 0.75, cy - r),
                        (cx + r * 0.30, cy - r * 0.12),
                        (cx - r * 0.05, cy - r * 0.12),
                        (cx + r * 0.75, cy + r),
                        (cx - r * 0.35, cy + r * 0.12),
                        (cx + r * 0.05, cy + r * 0.12),
                        (cx - r * 0.75, cy - r),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Escalation { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx, cy - r),
                        (cx + r, cy + r),
                        (cx, cy + r * 0.35),
                        (cx - r, cy + r),
                        (cx, cy - r),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Conditional { .. } => {
                    let r = w.min(h) * 0.24;
                    let pts = [
                        (cx - r * 0.7, cy - r),
                        (cx + r * 0.7, cy - r),
                        (cx + r * 0.7, cy + r),
                        (cx - r * 0.7, cy + r),
                        (cx - r * 0.7, cy - r),
                    ];
                    lines(scene, &pts, stroke, 1.1)?;
                    for dy in [-0.45_f32, 0.0, 0.45] {
                        lines(
                            scene,
                            &[(cx - r * 0.48, cy + r * dy), (cx + r * 0.48, cy + r * dy)],
                            stroke,
                            0.8,
                        )?;
                    }
                }
                EventDefinition::Link { .. } => {
                    let r = w.min(h) * 0.27;
                    let pts = [
                        (cx - r, cy - r * 0.45),
                        (cx + r * 0.1, cy - r * 0.45),
                        (cx + r * 0.1, cy - r),
                        (cx + r, cy),
                        (cx + r * 0.1, cy + r),
                        (cx + r * 0.1, cy + r * 0.45),
                        (cx - r, cy + r * 0.45),
                        (cx - r, cy - r * 0.45),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(
                            kind,
                            Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        ),
                    )?;
                }
                EventDefinition::Cancel { .. } => {
                    let r = w.min(h) * 0.21;
                    let t = r * 0.32;
                    let pts = [
                        (cx - r, cy - r + t),
                        (cx - r + t, cy - r),
                        (cx, cy - t),
                        (cx + r - t, cy - r),
                        (cx + r, cy - r + t),
                        (cx + t, cy),
                        (cx + r, cy + r - t),
                        (cx + r - t, cy + r),
                        (cx, cy + t),
                        (cx - r + t, cy + r),
                        (cx - r, cy + r - t),
                        (cx - t, cy),
                        (cx - r, cy - r + t),
                    ];
                    marker_polygon(
                        scene,
                        &pts,
                        stroke,
                        matches!(kind, Some(NodeKind::EndEvent)),
                    )?;
                }
                EventDefinition::Compensate { .. } => {
                    let r = w.min(h) * 0.24;
                    let left = [(cx - r, cy), (cx, cy - r), (cx, cy + r), (cx - r, cy)];
                    let right = [(cx, cy), (cx + r, cy - r), (cx + r, cy + r), (cx, cy)];
                    let filled = matches!(
                        kind,
                        Some(NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    );
                    marker_polygon(scene, &left, stroke, filled)?;
                    marker_polygon(scene, &right, stroke, filled)?;
                }
                EventDefinition::Timer { .. } => {
                    let r = w.min(h) * 0.27;
                    ring(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke, 1.2)?;
                    for i in 0..12 {
                        let a = i as f32 * std::f32::consts::TAU / 12.0;
                        lines(
                            scene,
                            &[
                                (cx + a.cos() * r * 0.75, cy + a.sin() * r * 0.75),
                                (cx + a.cos() * r * 0.95, cy + a.sin() * r * 0.95),
                            ],
                            stroke,
                            0.8,
                        )?;
                    }
                    lines(
                        scene,
                        &[(cx, cy - r * 0.62), (cx, cy), (cx + r * 0.48, cy)],
                        stroke,
                        1.1,
                    )?;
                }
                EventDefinition::Terminate { .. } if matches!(kind, Some(NodeKind::EndEvent)) => {
                    let r = w.min(h) * 0.29;
                    filled_ellipse(scene, cx - r, cy - r, 2.0 * r, 2.0 * r, stroke)?;
                }
                _ => diagnostics.push(format!(
                    "Event definition {:?} cannot be rendered",
                    definition
                )),
            }
        }
    }
    Ok(())
}
fn polyline_midpoint(points: &[(f32, f32)]) -> Option<(f32, f32)> {
    polyline_midpoint_segment(points).map(|(point, _)| point)
}
/// Arc-length midpoint and the direction vector of the segment containing it.
fn polyline_midpoint_segment(points: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    let total: f32 = points
        .windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
        .sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let mut remaining = total / 2.0;
    for pair in points.windows(2) {
        let length = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        if length > 0.0 && remaining <= length {
            let t = remaining / length;
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            return Some(((pair[0].0 + dx * t, pair[0].1 + dy * t), (dx, dy)));
        }
        remaining -= length;
    }
    None
}

fn envelope(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    stroke: [u8; 3],
    filled: bool,
    background: [u8; 3],
) -> Result<(), RenderError> {
    let fill = if filled { [225, 225, 225] } else { background };
    scene.add_background_rect(x - 10.0, y - 6.0, 20.0, 12.0, fill)?;
    let line = stroke;
    lines(
        scene,
        &[
            (x - 10.0, y - 6.0),
            (x + 10.0, y - 6.0),
            (x + 10.0, y + 6.0),
            (x - 10.0, y + 6.0),
            (x - 10.0, y - 6.0),
        ],
        stroke,
        1.0,
    )?;
    lines(
        scene,
        &[(x - 10.0, y - 6.0), (x, y + 1.0), (x + 10.0, y - 6.0)],
        line,
        1.0,
    )
}

fn marker_polygon(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    filled: bool,
) -> Result<(), RenderError> {
    if filled {
        let mut path = PathBuilder::new();
        polygon(&mut path, points);
        if let Some(path) = path.finish() {
            scene.add_path(path, color)?;
        }
    } else {
        lines(scene, points, color, 1.2)?;
    }
    Ok(())
}
fn self_reference_resolves(
    index: &SemanticIndex<'_>,
    reference: &processcraft_core::bpmn_activity::ModelReference,
    id: &ElementSelector,
) -> bool {
    index.resolved(&Some(reference.clone())).as_ref() == Some(id)
}
fn draw_choreography_loop(
    scene: &mut DiagramScene,
    marker: ChoreographyLoopType,
    cx: f32,
    cy: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    if marker == ChoreographyLoopType::Standard {
        let mut points = Vec::new();
        for i in 0..=16 {
            let a = (-0.15 + i as f32 / 16.0 * 1.65) * std::f32::consts::PI;
            points.push((cx + a.cos() * 5.0, cy + a.sin() * 5.0));
        }
        lines(scene, &points, color, 1.3)?;
        let tip = points[16];
        lines(
            scene,
            &[(tip.0 - 3.0, tip.1 - 1.0), tip, (tip.0 - 1.0, tip.1 + 3.0)],
            color,
            1.2,
        )?;
    } else {
        for offset in [-3.5_f32, 0.0, 3.5] {
            let points = if marker == ChoreographyLoopType::MultiInstanceSequential {
                [(cx - 5.0, cy + offset), (cx + 5.0, cy + offset)]
            } else {
                [(cx + offset, cy - 5.0), (cx + offset, cy + 5.0)]
            };
            lines(scene, &points, color, 1.4)?;
        }
    }
    Ok(())
}
fn draw_loop_marker(
    scene: &mut DiagramScene,
    marker: &LoopCharacteristics,
    cx: f32,
    cy: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    match marker {
        LoopCharacteristics::Standard { .. } => {
            let mut points = Vec::new();
            for i in 0..=16 {
                let a = (-0.15 + i as f32 / 16.0 * 1.65) * std::f32::consts::PI;
                points.push((cx + a.cos() * 5.0, cy + a.sin() * 5.0));
            }
            lines(scene, &points, color, 1.3)?;
            let tip = *points.last().expect("non-empty arc");
            lines(
                scene,
                &[(tip.0 - 3.0, tip.1 - 1.0), tip, (tip.0 - 1.0, tip.1 + 3.0)],
                color,
                1.2,
            )?;
        }
        LoopCharacteristics::MultiInstance { is_sequential, .. } => {
            for offset in [-3.5_f32, 0.0, 3.5] {
                if *is_sequential == Some(true) {
                    lines(
                        scene,
                        &[(cx - 5.0, cy + offset), (cx + 5.0, cy + offset)],
                        color,
                        1.4,
                    )?;
                } else {
                    lines(
                        scene,
                        &[(cx + offset, cy - 5.0), (cx + offset, cy + 5.0)],
                        color,
                        1.4,
                    )?;
                }
            }
        }
    }
    Ok(())
}
fn non_interrupting(details: Option<&NodeDetails>) -> bool {
    matches!(
        details,
        Some(NodeDetails::BoundaryEvent {
            cancel_activity: false,
            ..
        }) | Some(NodeDetails::Event {
            cancel_activity: Some(false),
            ..
        })
    )
}
fn dashed_ellipse(
    scene: &mut DiagramScene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) -> Result<(), RenderError> {
    for i in 0..16 {
        let mut pts = Vec::new();
        for j in 0..=3 {
            let a = (i as f32 + j as f32 / 6.0) * std::f32::consts::TAU / 16.0;
            pts.push((x + w * (1.0 + a.cos()) / 2.0, y + h * (1.0 + a.sin()) / 2.0));
        }
        lines(scene, &pts, color, 1.5)?;
    }
    Ok(())
}
fn ellipse(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    let k = 0.5522848;
    p.move_to(cx + rx, cy);
    p.cubic_to(cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry);
    p.cubic_to(cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy);
    p.cubic_to(cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry);
    p.cubic_to(cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy);
    p.close();
}
fn outline(
    scene: &mut DiagramScene,
    path: &tiny_skia::Path,
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    let stroke = tiny_skia::Stroke {
        width,
        ..Default::default()
    };
    let p = path
        .stroke(&stroke, 1.0)
        .ok_or(RenderError("Invalid stroke"))?;
    scene.add_path(p, color)
}
fn lines(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    width: f32,
) -> Result<(), RenderError> {
    if points.len() < 2 {
        return Ok(());
    }
    let mut p = PathBuilder::new();
    p.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        p.line_to(x, y)
    }
    if let Some(path) = p.finish() {
        outline(scene, &path, color, width)?
    }
    Ok(())
}

fn dashed_lines(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    color: [u8; 3],
    dash: f32,
) -> Result<(), RenderError> {
    let mut path = PathBuilder::new();
    let mut count = 0usize;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if length == 0.0 {
            continue;
        }
        let mut at = 0.0;
        while at < length {
            let end = (at + dash).min(length);
            let p = |t: f32| {
                (
                    a.0 + (b.0 - a.0) * t / length,
                    a.1 + (b.1 - a.1) * t / length,
                )
            };
            count += 1;
            if count > 100_000 {
                return Err(RenderError("Dashed edge exceeds complexity limit"));
            }
            let a = p(at);
            let b = p(end);
            path.move_to(a.0, a.1);
            path.line_to(b.0, b.1);
            at += dash * 2.0;
        }
    }
    if let Some(path) = path.finish() {
        outline(scene, &path, color, 1.5)?;
    }
    Ok(())
}

fn rounded(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    rounded_corners(p, x, y, w, h, rounding(w, h), [true; 4]);
}
/// Corner radius of a rounded activity outline.
fn rounding(w: f32, h: f32) -> f32 {
    10.0_f32.min(w / 2.0).min(h / 2.0)
}
/// Rectangle with the selected corners (top-left, top-right, bottom-right,
/// bottom-left) rounded by `r`.
fn rounded_corners(p: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32, c: [bool; 4]) {
    let k = 0.5522848;
    let [tl, tr, br, bl] = c.map(|on| {
        if on {
            r.min(w / 2.0).min(h).max(0.0)
        } else {
            0.0
        }
    });
    p.move_to(x + tl, y);
    p.line_to(x + w - tr, y);
    if tr > 0.0 {
        p.cubic_to(
            x + w - tr + k * tr,
            y,
            x + w,
            y + tr - k * tr,
            x + w,
            y + tr,
        );
    }
    p.line_to(x + w, y + h - br);
    if br > 0.0 {
        p.cubic_to(
            x + w,
            y + h - br + k * br,
            x + w - br + k * br,
            y + h,
            x + w - br,
            y + h,
        );
    }
    p.line_to(x + bl, y + h);
    if bl > 0.0 {
        p.cubic_to(
            x + bl - k * bl,
            y + h,
            x,
            y + h - bl + k * bl,
            x,
            y + h - bl,
        );
    }
    p.line_to(x, y + tl);
    if tl > 0.0 {
        p.cubic_to(x, y + tl - k * tl, x + tl - k * tl, y, x + tl, y);
    }
    p.close();
}

/// View rectangle in absolute document units; zoom is pixels per document unit.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f32,
}
/// Pixels and hit geometry for one viewport. Hit geometry remains in document coordinates.
pub struct BpmnViewport {
    pub pixels: tiny_skia::Pixmap,
    pub hits: Vec<HitRegion>,
    pub diagnostics: Vec<String>,
    pub capability: Capability,
    pub viewport: Viewport,
}
impl BpmnViewport {
    pub fn build(
        document: &BpmnDocument,
        diagram_id: &ElementSelector,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        view: Viewport,
    ) -> Result<Self, RenderError> {
        if !view.x.is_finite()
            || !view.y.is_finite()
            || !view.width.is_finite()
            || !view.height.is_finite()
            || view.width <= 0.0
            || view.height <= 0.0
            || !view.zoom.is_finite()
            || !(0.125..=8.0).contains(&view.zoom)
            || view.width * f64::from(view.zoom) > 16384.0
            || view.height * f64::from(view.zoom) > 16384.0
        {
            return Err(RenderError("Invalid viewport"));
        }
        if matches!(document.capability, Capability::PreservedSource { .. }) {
            let mut empty = DiagramScene::new(view.width as f32, view.height as f32)?;
            if appearance != Appearance::Light {
                empty.add_background_rect(
                    0.0,
                    0.0,
                    view.width as f32,
                    view.height as f32,
                    appearance.paper(),
                )?;
            }
            let pixels = empty.rasterize(view.zoom)?;
            return Ok(Self {
                pixels,
                hits: vec![],
                diagnostics: vec!["Preserved source: semantic preview is unavailable".into()],
                capability: document.capability.clone(),
                viewport: view,
            });
        }
        let original_diagram = document
            .diagrams
            .iter()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        let visibility = SemanticIndex::new(document)?;
        let mut copy = document.clone();
        let diagram = copy
            .diagrams
            .iter_mut()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        let inside = |b: &Bounds| {
            b.x <= view.x + view.width
                && b.y <= view.y + view.height
                && b.x + b.width >= view.x
                && b.y + b.height >= view.y
        };
        let rebase = |b: &mut Bounds| {
            b.x -= view.x;
            b.y -= view.y;
        };
        let mut viewport_diagnostics = Vec::new();
        if original_diagram
            .shapes
            .iter()
            .any(|shape| visibility.shape_visibility(shape, original_diagram).1)
            || original_diagram
                .edges
                .iter()
                .any(|edge| visibility.edge_visibility(edge, original_diagram).1)
        {
            viewport_diagnostics.push("Choreography representation ownership is ambiguous; affected elements remain visible".into());
        }
        diagram
            .shapes
            .retain(|s| !visibility.shape_visibility(s, original_diagram).0);
        diagram.shapes.retain(|s| {
            if !inside(&s.bounds) {
                return false;
            }
            let b = &s.bounds;
            if [
                b.x - view.x,
                b.y - view.y,
                b.x + b.width - view.x,
                b.y + b.height - view.y,
            ]
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 999_000.0)
            {
                viewport_diagnostics.push(format!(
                    "Shape {} omitted: geometry exceeds viewport renderer limits",
                    s.id
                ));
                false
            } else {
                true
            }
        });
        for s in &mut diagram.shapes {
            rebase(&mut s.bounds);
            if s.label
                .as_ref()
                .and_then(|label| label.bounds.as_ref())
                .is_some_and(|bounds| !inside(bounds))
            {
                s.label = None;
            }
            if let Some(l) = &mut s.label
                && let Some(bounds) = &mut l.bounds
            {
                rebase(bounds);
            }
        }
        diagram.edges.clear();
        let mut built =
            BpmnScene::build_internal(&copy, diagram_id, appearance, fonts, family, false)?;
        built.diagnostics.extend(viewport_diagnostics);
        let original = document
            .diagrams
            .iter()
            .find(|d| &d.id == diagram_id)
            .ok_or(RenderError("Diagram not found"))?;
        if has_uninterpreted_di(original)
            && !built
                .diagnostics
                .iter()
                .any(|d| d == DI_EXTENSION_DIAGNOSTIC)
        {
            built.diagnostics.push(DI_EXTENSION_DIAGNOSTIC.into());
        }
        let paint_ranks = original.plane_order.as_ref().map(|order| {
            order
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i + 1))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        let fg = match appearance {
            Appearance::Light => [26, 37, 52],
            Appearance::Dark => [230, 234, 240],
            Appearance::CanvasLight => [42, 46, 54],
            Appearance::CanvasDark => [201, 206, 214],
        };
        let semantics = SemanticIndex::new(document)?;
        // Labels are culled independently of their owning shape.
        for shape in &original.shapes {
            if let Some(ranks) = &paint_ranks {
                built.scene.set_paint_rank(
                    *ranks
                        .get(&shape.id.selector())
                        .unwrap_or(&(ranks.len() + 1)),
                );
            }
            if semantics.activity_ports.contains(&shape.element)
                || semantics.shape_visibility(shape, original).0
            {
                continue;
            }
            if shape.participant_band_kind.is_some() {
                let owner = match &shape.choreography_activity_shape {
                    Some(processcraft_core::model::DiReference::Local(id)) => {
                        original.shapes.iter().find(|s| &s.id == id)
                    }
                    Some(processcraft_core::model::DiReference::Qualified(q))
                        if document
                            .definitions
                            .as_ref()
                            .is_some_and(|defs| defs.target_namespace == q.namespace) =>
                    {
                        original
                            .shapes
                            .iter()
                            .find(|s| s.id.xml_id() == Some(&q.local))
                    }
                    _ => None,
                };
                let valid = owner
                    .and_then(|owner| semantics.choreography(&owner.element))
                    .is_some_and(|(root, activity)| {
                        root.participants.iter().any(|p| p.id == shape.element)
                            && activity
                                .participant_refs
                                .iter()
                                .any(|r| self_reference_resolves(&semantics, r, &shape.element))
                    });
                if !valid {
                    continue;
                }
            }
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let port = semantics.port(&shape.element);
            let store = document.data_stores.iter().find(|store| {
                store
                    .id
                    .as_ref()
                    .is_some_and(|identity| identity == &shape.element)
            });
            let kind = node
                .map(|node| node.kind)
                .or(port.map(|_| NodeKind::DataObjectReference))
                .or(store.map(|_| NodeKind::DataStoreReference));
            let marker_kind = match called_target(node, document) {
                CalledTarget::Global(task_kind) => Some(task_kind),
                _ => kind,
            };
            let conversation = semantics.conversation(&shape.element);
            let choreography = semantics.choreography(&shape.element);
            let node_name = node.map(|node| node_display_name(node, document));
            if node_name.as_ref().is_some_and(|(_, truncated)| *truncated) {
                built.diagnostics.push(format!(
                    "Annotation {} text exceeds display limit",
                    shape.element
                ));
            }
            let group_text = semantics.group_label(&shape.element, document);
            let mut name = group_text
                .as_deref()
                .or_else(|| {
                    choreography.map(|(_, activity)| activity.name.as_deref().unwrap_or(""))
                })
                .or_else(|| conversation.map(|node| node.name.as_deref().unwrap_or("")))
                .or(node_name.as_ref().map(|(text, _)| text.as_ref()))
                .or_else(|| port.map(|(port, _)| port.name.as_deref().unwrap_or("")))
                .or_else(|| store.map(|store| store.name.as_deref().unwrap_or("")))
                .or_else(|| {
                    semantics
                        .lane(&shape.element)
                        .map(|(lane, _)| lane.name.as_deref().unwrap_or(""))
                })
                .or_else(|| {
                    semantics
                        .choreographies
                        .iter()
                        .flat_map(|(c, _)| &c.participants)
                        .find(|p| p.id == shape.element)
                        .map(|p| p.name.as_deref().unwrap_or(""))
                });
            if let Some(p) = semantics.participant(&shape.element) {
                name = Some(p.name.as_deref().unwrap_or(""));
            }
            let Some(name) = name else { continue };
            let display_name = if let Some((port, _)) = port {
                match port
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|name| !name.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else if let Some(store) = store {
                match store
                    .item_aware
                    .as_ref()
                    .and_then(|item| item.data_state.as_ref())
                    .and_then(|state| state.name.as_deref())
                    .filter(|state| !state.is_empty())
                {
                    Some(state) if !name.is_empty() => format!("{name} [{state}]"),
                    Some(state) => format!("[{state}]"),
                    None => name.to_owned(),
                }
            } else {
                data_label(node, name, &semantics)
            };
            let name = display_name.as_str();
            if shape.label.is_none() && !built.hits.iter().any(|h| h.diagram_element_id == shape.id)
            {
                continue;
            }
            let b = &shape.bounds;
            let is_lane_or_pool = semantics.lane(&shape.element).is_some()
                || semantics.participant(&shape.element).is_some();
            let (label_bounds, label_align) =
                if semantics.groups.iter().any(|g| g.id == shape.element)
                    && shape
                        .label
                        .as_ref()
                        .and_then(|l| l.bounds.as_ref())
                        .is_none()
                {
                    (
                        Bounds {
                            x: b.x + 8.0,
                            y: b.y + 4.0,
                            width: (b.width - 16.0).max(0.0),
                            height: b.height.min(28.0),
                        },
                        LabelAlign::TopLeft,
                    )
                } else {
                    shape_label_layout(
                        shape,
                        kind,
                        marker_kind,
                        node,
                        choreography.is_some(),
                        is_lane_or_pool,
                    )
                };
            let bounds = &label_bounds;
            if !label_visible(shape.label.as_ref(), shape.label_visibility)
                || name.is_empty()
                || !inside(bounds)
            {
                continue;
            }
            if semantics.groups.iter().any(|g| g.id == shape.element)
                && group_label_collision(bounds, &shape.id.selector(), original, &semantics)
            {
                built.diagnostics.push(format!(
                    "Group {} label overlaps another shape",
                    shape.element
                ));
            }
            let rotate_pool_label = is_lane_or_pool
                && shape.is_horizontal != Some(false)
                && shape
                    .label
                    .as_ref()
                    .and_then(|l| l.bounds.as_ref())
                    .is_none();
            let rotated_bounds = Bounds {
                x: 0.0,
                y: 0.0,
                width: bounds.height,
                height: bounds.width,
            };
            label(
                &mut built.scene,
                fonts,
                family,
                name,
                if rotate_pool_label {
                    &rotated_bounds
                } else {
                    bounds
                },
                if rotate_pool_label {
                    (0.0, 0.0)
                } else {
                    (view.x + built.origin.0, view.y + built.origin.1)
                },
                shape
                    .label
                    .as_ref()
                    .and_then(|l| l.style.as_ref())
                    .or(shape.style.as_ref()),
                label_font(
                    document,
                    original,
                    shape.label.as_ref(),
                    &mut built.diagnostics,
                ),
                if matches!(
                    shape.participant_band_kind,
                    Some(
                        processcraft_core::model::ParticipantBandKind::TopNonInitiating
                            | processcraft_core::model::ParticipantBandKind::MiddleNonInitiating
                            | processcraft_core::model::ParticipantBandKind::BottomNonInitiating
                    )
                ) {
                    let band_fill = color(
                        shape.style.as_ref().and_then(|s| s.fill.as_deref()),
                        [225, 225, 225],
                    );
                    if low_contrast([26, 37, 52], band_fill) {
                        [255, 255, 255]
                    } else {
                        [26, 37, 52]
                    }
                } else {
                    fg
                },
                if shape.participant_band_kind.is_some() {
                    LabelAlign::Center
                } else {
                    label_align
                },
                &mut built.diagnostics,
            )?;
            if rotate_pool_label {
                built.scene.rotate_last_label(
                    (bounds.x - view.x - built.origin.0) as f32,
                    (bounds.y + bounds.height - view.y - built.origin.1) as f32,
                );
            }
            if label_visible(shape.label.as_ref(), shape.label_visibility) {
                let mut local = bounds.clone();
                rebase(&mut local);
                built.hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.selector(),
                    label_id: shape
                        .label
                        .as_ref()
                        .and_then(|label| label.id.as_ref())
                        .map(|id| id.selector()),
                    is_label: true,
                    geometry: HitGeometry::Shape(local),
                });
            }
        }
        for edge in &original.edges {
            if let Some(ranks) = &paint_ranks {
                built
                    .scene
                    .set_paint_rank(semantics.edge_paint_rank(ranks, edge));
            }
            if semantics.edge_visibility(edge, original).0 {
                continue;
            }
            if semantics.data_association(&edge.element).is_some() {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                let mut visible = false;
                for pair in edge.waypoints.windows(2) {
                    if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
                        let points = [
                            (
                                (a.0 - view.x - built.origin.0) as f32,
                                (a.1 - view.y - built.origin.1) as f32,
                            ),
                            (
                                (b.0 - view.x - built.origin.0) as f32,
                                (b.1 - view.y - built.origin.1) as f32,
                            ),
                        ];
                        dashed_lines(&mut built.scene, &points, stroke, 2.0)?;
                        visible = true;
                    }
                }
                if let Some(end) = edge.waypoints.last()
                    && end.x >= view.x
                    && end.x <= view.x + view.width
                    && end.y >= view.y
                    && end.y <= view.y + view.height
                    && edge.waypoints.len() > 1
                {
                    let pts: Vec<_> = edge
                        .waypoints
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    open_arrow(&mut built.scene, &pts, stroke)?;
                }
                if visible {
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: None,
                        is_label: false,
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            if let Some(link) = semantics.conversation_link(&edge.element) {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                if semantics.resolved(&link.source).is_none()
                    || semantics.resolved(&link.target).is_none()
                {
                    built.diagnostics.push(format!("Conversation link {} has an unresolved endpoint; authored DI route is retained", link.id));
                }
                let mut visible = false;
                for pair in edge.waypoints.windows(2) {
                    if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
                        let points = [
                            (
                                (a.0 - view.x - built.origin.0) as f32,
                                (a.1 - view.y - built.origin.1) as f32,
                            ),
                            (
                                (b.0 - view.x - built.origin.0) as f32,
                                (b.1 - view.y - built.origin.1) as f32,
                            ),
                        ];
                        double_link(&mut built.scene, &points, stroke)?;
                        visible = true;
                    }
                }
                if label_visible(edge.label.as_ref(), edge.label_visibility)
                    && link.name.as_deref().is_some_and(|name| !name.is_empty())
                    && let Some((bounds, align)) = edge_label_layout(edge)
                {
                    let bounds = &bounds;
                    if inside(bounds) {
                        label(
                            &mut built.scene,
                            fonts,
                            family,
                            link.name.as_deref().unwrap_or(""),
                            bounds,
                            (view.x + built.origin.0, view.y + built.origin.1),
                            edge.label
                                .as_ref()
                                .and_then(|l| l.style.as_ref())
                                .or(edge.style.as_ref()),
                            label_font(
                                document,
                                original,
                                edge.label.as_ref(),
                                &mut built.diagnostics,
                            ),
                            fg,
                            align,
                            &mut built.diagnostics,
                        )?;
                        if label_visible(edge.label.as_ref(), edge.label_visibility) {
                            let mut local = bounds.clone();
                            rebase(&mut local);
                            built.hits.push(HitRegion {
                                key: ElementKey {
                                    document_id: document.id,
                                    xml_id: edge.element.clone(),
                                },
                                diagram_element_id: edge.id.selector(),
                                label_id: edge
                                    .label
                                    .as_ref()
                                    .and_then(|label| label.id.as_ref())
                                    .map(|id| id.selector()),
                                is_label: true,
                                geometry: HitGeometry::Shape(local),
                            });
                        }
                    }
                }
                if visible {
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: None,
                        is_label: false,
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            let Some(flow) = semantics.flow(&edge.element) else {
                continue;
            };
            if semantics.resolved(&flow.source).is_none()
                || semantics.resolved(&flow.target).is_none()
            {
                built.diagnostics.push(format!(
                    "Flow {} has an unresolved endpoint; authored DI route is retained",
                    flow.id
                ));
            }
            let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
            let mut path = PathBuilder::new();
            let mut segments = 0usize;
            for pair in edge.waypoints.windows(2) {
                if let Some((a, b)) = clip_segment(&pair[0], &pair[1], view) {
                    let a = (
                        (a.0 - view.x - built.origin.0) as f32,
                        (a.1 - view.y - built.origin.1) as f32,
                    );
                    let b = (
                        (b.0 - view.x - built.origin.0) as f32,
                        (b.1 - view.y - built.origin.1) as f32,
                    );
                    if matches!(flow.kind, FlowKind::Sequence) {
                        path.move_to(a.0, a.1);
                        path.line_to(b.0, b.1);
                        segments += 1;
                    } else {
                        let dash = if matches!(flow.kind, FlowKind::Message) {
                            7.0
                        } else {
                            2.0
                        };
                        let len = (b.0 - a.0).hypot(b.1 - a.1);
                        let mut at = 0.0;
                        while at < len {
                            segments += 1;
                            if segments > 100_000 {
                                return Err(RenderError("Dashed edge exceeds complexity limit"));
                            }
                            let end = (at + dash).min(len);
                            let pos =
                                |t: f32| (a.0 + (b.0 - a.0) * t / len, a.1 + (b.1 - a.1) * t / len);
                            let p = pos(at);
                            let q = pos(end);
                            path.move_to(p.0, p.1);
                            path.line_to(q.0, q.1);
                            at += dash * 2.0;
                        }
                    }
                }
            }
            if let Some(path) = path.finish() {
                outline(&mut built.scene, &path, stroke, 1.5)?;
            }
            if matches!(flow.kind, FlowKind::Message)
                && let Some(visible) = edge.message_visible_kind
            {
                let points: Vec<_> = edge
                    .waypoints
                    .iter()
                    .map(|p| {
                        (
                            (p.x - view.x - built.origin.0) as f32,
                            (p.y - view.y - built.origin.1) as f32,
                        )
                    })
                    .collect();
                if let Some((cx, cy)) = polyline_midpoint(&points)
                    && cx >= -64.0
                    && cy >= -64.0
                    && cx <= view.width as f32 + 64.0
                    && cy <= view.height as f32 + 64.0
                {
                    let background = appearance.paper();
                    let filled = matches!(
                        visible,
                        processcraft_core::model::MessageVisibleKind::NonInitiating
                    );
                    let explicit = edge.style.as_ref().and_then(|s| s.stroke.as_deref());
                    let envelope_stroke = if filled && explicit.is_none() {
                        [26, 37, 52]
                    } else {
                        stroke
                    };
                    if filled
                        && explicit.is_some()
                        && low_contrast(envelope_stroke, [225, 225, 225])
                    {
                        built.diagnostics.push(format!("Message edge {} explicit stroke has low contrast against the envelope fill", edge.id));
                    }
                    envelope(
                        &mut built.scene,
                        cx,
                        cy,
                        envelope_stroke,
                        filled,
                        background,
                    )?;
                    if let Some(text) = semantics.message_name(flow) {
                        message_name_label(
                            &mut built.scene,
                            fonts,
                            family,
                            text,
                            (cx, cy),
                            (view.x + built.origin.0, view.y + built.origin.1),
                            fg,
                            &mut built.diagnostics,
                        )?;
                    }
                }
            }
            if segments > 0
                && matches!(flow.kind, FlowKind::Sequence)
                && semantics.is_default_flow(flow)
                && edge.waypoints.len() > 1
            {
                let a = &edge.waypoints[0];
                if a.x >= view.x
                    && a.x <= view.x + view.width
                    && a.y >= view.y
                    && a.y <= view.y + view.height
                {
                    let points: Vec<_> = edge.waypoints[..2]
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    draw_default_slash(&mut built.scene, &points, stroke)?;
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && (matches!(flow.kind, FlowKind::Message)
                    || semantics.conditional_flow_marker(flow))
            {
                let a = &edge.waypoints[0];
                if a.x >= view.x
                    && a.x <= view.x + view.width
                    && a.y >= view.y
                    && a.y <= view.y + view.height
                {
                    let points: Vec<_> = edge.waypoints[..2]
                        .iter()
                        .map(|p| {
                            (
                                (p.x - view.x - built.origin.0) as f32,
                                (p.y - view.y - built.origin.1) as f32,
                            )
                        })
                        .collect();
                    draw_flow_source_marker(
                        &mut built.scene,
                        &points,
                        stroke,
                        appearance.paper(),
                        matches!(flow.kind, FlowKind::Message),
                    )?;
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && (!matches!(flow.kind, FlowKind::Association)
                    || matches!(
                        flow.association_direction,
                        Some(AssociationDirection::One | AssociationDirection::Both)
                    ))
            {
                let b = &edge.waypoints[edge.waypoints.len() - 1];
                let a = edge
                    .waypoints
                    .iter()
                    .rev()
                    .find(|p| p.x != b.x || p.y != b.y)
                    .unwrap_or(&edge.waypoints[edge.waypoints.len() - 2]);
                if b.x >= view.x
                    && b.x <= view.x + view.width
                    && b.y >= view.y
                    && b.y <= view.y + view.height
                {
                    let end = (
                        (b.x - view.x - built.origin.0) as f32,
                        (b.y - view.y - built.origin.1) as f32,
                    );
                    let angle = (b.y - a.y).atan2(b.x - a.x) as f32;
                    let wing = |v: f32| {
                        (
                            end.0 - 9.0 * (angle + v).cos(),
                            end.1 - 9.0 * (angle + v).sin(),
                        )
                    };
                    if matches!(flow.kind, FlowKind::Sequence) {
                        let mut p = PathBuilder::new();
                        polygon(&mut p, &[wing(0.5), end, wing(-0.5)]);
                        if let Some(p) = p.finish() {
                            built.scene.add_path(p, stroke)?;
                        }
                    } else {
                        lines(&mut built.scene, &[wing(0.5), end, wing(-0.5)], stroke, 1.5)?;
                    }
                }
            }
            if segments > 0
                && edge.waypoints.len() > 1
                && matches!(flow.kind, FlowKind::Association)
                && matches!(flow.association_direction, Some(AssociationDirection::Both))
            {
                let b = &edge.waypoints[0];
                let a = edge
                    .waypoints
                    .iter()
                    .find(|p| p.x != b.x || p.y != b.y)
                    .unwrap_or(&edge.waypoints[1]);
                if b.x >= view.x
                    && b.x <= view.x + view.width
                    && b.y >= view.y
                    && b.y <= view.y + view.height
                {
                    let end = (
                        (b.x - view.x - built.origin.0) as f32,
                        (b.y - view.y - built.origin.1) as f32,
                    );
                    let angle = (b.y - a.y).atan2(b.x - a.x) as f32;
                    let wing = |v: f32| {
                        (
                            end.0 - 9.0 * (angle + v).cos(),
                            end.1 - 9.0 * (angle + v).sin(),
                        )
                    };
                    lines(&mut built.scene, &[wing(0.5), end, wing(-0.5)], stroke, 1.5)?;
                }
            }
            if label_visible(edge.label.as_ref(), edge.label_visibility)
                && flow.name.as_deref().is_some_and(|name| !name.is_empty())
                && let Some((bounds, align)) = edge_label_layout(edge)
            {
                let bounds = &bounds;
                if inside(bounds) {
                    let mut local = bounds.clone();
                    rebase(&mut local);
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.selector(),
                        label_id: edge
                            .label
                            .as_ref()
                            .and_then(|label| label.id.as_ref())
                            .map(|id| id.selector()),
                        is_label: true,
                        geometry: HitGeometry::Shape(local),
                    });
                    label(
                        &mut built.scene,
                        fonts,
                        family,
                        flow.name.as_deref().unwrap_or(""),
                        bounds,
                        (view.x + built.origin.0, view.y + built.origin.1),
                        edge.label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(edge.style.as_ref()),
                        label_font(
                            document,
                            original,
                            edge.label.as_ref(),
                            &mut built.diagnostics,
                        ),
                        fg,
                        align,
                        &mut built.diagnostics,
                    )?;
                }
            }
            if segments == 0 {
                continue;
            }
            built.hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: edge.element.clone(),
                },
                diagram_element_id: edge.id.selector(),
                label_id: None,
                is_label: false,
                geometry: HitGeometry::Edge(edge.waypoints.clone()),
            });
        }
        let bg = appearance.paper();
        if appearance.is_canvas() {
            // Dots stay 1.5 px on screen and vanish when closer than 10 px.
            let zoom = f64::from(view.zoom);
            if DOT_SPACING * zoom >= 10.0 {
                let mut dots = PathBuilder::new();
                let size = (1.5 / zoom) as f32;
                let first_x = (view.x / DOT_SPACING).ceil() * DOT_SPACING;
                let first_y = (view.y / DOT_SPACING).ceil() * DOT_SPACING;
                let mut gy = first_y;
                while gy < view.y + view.height {
                    let mut gx = first_x;
                    while gx < view.x + view.width {
                        if let Some(dot) = tiny_skia::Rect::from_xywh(
                            (gx - view.x - built.origin.0) as f32 - size / 2.0,
                            (gy - view.y - built.origin.1) as f32 - size / 2.0,
                            size,
                            size,
                        ) {
                            dots.push_rect(dot);
                        }
                        gx += DOT_SPACING;
                    }
                    gy += DOT_SPACING;
                }
                if let Some(dots) = dots.finish() {
                    built.scene.prepend_background_path(
                        dots,
                        if appearance.is_dark() {
                            [46, 51, 58]
                        } else {
                            [211, 216, 223]
                        },
                    );
                }
            }
        }
        built.scene.prepend_background_rect(
            (-built.origin.0) as f32,
            (-built.origin.1) as f32,
            view.width as f32,
            view.height as f32,
            bg,
        )?;
        let pixels = built.scene.rasterize_viewport(
            (-built.origin.0) as f32,
            (-built.origin.1) as f32,
            view.width as f32,
            view.height as f32,
            view.zoom,
        )?;
        let mut hits = built.hits;
        if let Some(ranks) = &paint_ranks {
            hits.sort_by_key(|hit| {
                *ranks
                    .get(&hit.diagram_element_id)
                    .unwrap_or(&(ranks.len() + 1))
            });
        }
        for h in &mut hits {
            match &mut h.geometry {
                HitGeometry::Shape(b)
                | HitGeometry::Ellipse(b)
                | HitGeometry::Diamond(b)
                | HitGeometry::Hexagon(b)
                | HitGeometry::DataObject(b)
                | HitGeometry::DataStore(b) => {
                    b.x += view.x;
                    b.y += view.y;
                }
                HitGeometry::Edge(_) => {}
            }
        }
        Ok(Self {
            pixels,
            hits,
            diagnostics: built.diagnostics,
            capability: built.capability,
            viewport: view,
        })
    }
    pub fn hit_test(&self, x: f64, y: f64, tolerance: f64) -> Option<&HitRegion> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.contains(x, y, tolerance.max(0.0)))
    }
}
fn clip_segment(a: &Point, b: &Point, v: Viewport) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut lo: f64 = 0.0;
    let mut hi: f64 = 1.0;
    for (p, q) in [
        (-dx, a.x - v.x),
        (dx, v.x + v.width - a.x),
        (-dy, a.y - v.y),
        (dy, v.y + v.height - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                lo = lo.max(r)
            } else {
                hi = hi.min(r)
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some((
        (a.x + lo * dx, a.y + lo * dy),
        (a.x + hi * dx, a.y + hi * dy),
    ))
}

#[cfg(test)]
mod source_marker_tests {
    use super::*;
    #[test]
    fn message_envelope_uses_arc_midpoint_and_bounded_vector() {
        let midpoint = polyline_midpoint(&[(0.0, 0.0), (80.0, 0.0), (80.0, 20.0)]).unwrap();
        assert_eq!(midpoint, (50.0, 0.0));
        let mut scene = DiagramScene::new(100.0, 100.0).unwrap();
        envelope(
            &mut scene,
            midpoint.0,
            midpoint.1 + 20.0,
            [1, 2, 3],
            true,
            [255, 255, 255],
        )
        .unwrap();
        let paths: Vec<_> = scene.paths().collect();
        assert_eq!(paths[0].color, [225, 225, 225]);
        assert_eq!(
            (
                paths[0].path.bounds().left(),
                paths[0].path.bounds().right()
            ),
            (40.0, 60.0)
        );
        assert_eq!(
            (
                paths[0].path.bounds().top(),
                paths[0].path.bounds().bottom()
            ),
            (14.0, 26.0)
        );
    }
    #[test]
    fn explicit_paint_rank_interleaves_paths_and_glyphs() {
        let mut scene = DiagramScene::new(20.0, 20.0).unwrap();
        scene
            .add_background_rect(0.0, 0.0, 20.0, 20.0, [255, 255, 255])
            .unwrap();
        scene.set_paint_rank(2);
        scene
            .add_background_rect(1.0, 1.0, 10.0, 10.0, [255, 0, 0])
            .unwrap();
        scene.set_paint_rank(1);
        scene
            .add_background_rect(1.0, 1.0, 10.0, 10.0, [0, 0, 255])
            .unwrap();
        assert_eq!(
            scene.paths().map(|p| p.color).collect::<Vec<_>>(),
            vec![[255, 255, 255], [0, 0, 255], [255, 0, 0]]
        );
    }
    #[test]
    fn source_markers_have_expected_bounds_and_distinct_outlines() {
        let mut diamond = DiagramScene::new(100.0, 100.0).unwrap();
        draw_flow_source_marker(
            &mut diamond,
            &[(20.0, 30.0), (60.0, 30.0)],
            [0, 0, 0],
            [255, 255, 255],
            false,
        )
        .unwrap();
        let paths: Vec<_> = diamond.paths().collect();
        assert_eq!(paths.len(), 2);
        let bounds = paths[1].path.bounds();
        assert!(bounds.left() <= 20.0 && bounds.right() >= 32.0);
        assert!(bounds.top() <= 26.0 && bounds.bottom() >= 34.0);
        let mut circle = DiagramScene::new(100.0, 100.0).unwrap();
        draw_flow_source_marker(
            &mut circle,
            &[(20.0, 30.0), (60.0, 30.0)],
            [0, 0, 0],
            [255, 255, 255],
            true,
        )
        .unwrap();
        let paths: Vec<_> = circle.paths().collect();
        assert_eq!(paths.len(), 2);
        let bounds = paths[1].path.bounds();
        assert!(bounds.left() <= 16.0 && bounds.right() >= 24.0);
        assert!(bounds.top() <= 26.0 && bounds.bottom() >= 34.0);
    }
}
