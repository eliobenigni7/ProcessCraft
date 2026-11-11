use crate::{DiagramScene, RenderError, TextEngine, TextRequest};
use processcraft_core::{
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ConversationKind, ConversationLink, ConversationNode,
    },
    bpmn_data::{DataAssociation, DataObject},
    bpmn_model::{
        AssociationDirection, EventDefinition, EventGatewayType, LoopCharacteristics, NodeDetails,
    },
    ids::{ElementKey, XmlId},
    model::{Bounds, BpmnDocument, Capability, Flow, FlowKind, Lane, Node, NodeKind, Point, Style},
};
use tiny_skia::PathBuilder;

struct SemanticIndex<'a> {
    groups: Vec<&'a processcraft_core::bpmn_model::Group>,
    nodes: Vec<(&'a Node, usize)>,
    lanes: Vec<(&'a Lane, usize)>,
    flows: Vec<&'a Flow>,
    parents: std::collections::BTreeMap<XmlId, XmlId>,
    conversations: Vec<&'a ConversationNode>,
    conversation_links: Vec<&'a ConversationLink>,
    data_associations: Vec<&'a DataAssociation>,
    data_objects: Vec<&'a DataObject>,
    choreographies: Vec<&'a Choreography>,
}
impl<'a> SemanticIndex<'a> {
    fn new(document: &'a BpmnDocument) -> Result<Self, RenderError> {
        let mut result = Self {
            groups: Vec::new(),
            nodes: Vec::new(),
            lanes: Vec::new(),
            flows: Vec::new(),
            parents: std::collections::BTreeMap::new(),
            conversations: Vec::new(),
            conversation_links: Vec::new(),
            data_associations: Vec::new(),
            data_objects: Vec::new(),
            choreographies: document.choreographies.iter().collect(),
        };
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
                    None::<&XmlId>,
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
                for id in groups.iter().map(|g| &g.id).chain(
                    nodes
                        .iter()
                        .map(|n| &n.id)
                        .chain(lanes.iter().map(|l| &l.id))
                        .chain(flows.iter().map(|f| &f.id))
                        .chain(associations.iter().map(|a| &a.id)),
                ) {
                    result.parents.insert(id.clone(), parent.clone());
                }
            }
            for node in nodes {
                result.nodes.push((node, depth));
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
                        Some(&node.id),
                    ));
                }
            }
        }
        for choreography in &document.choreographies {
            result.flows.extend(&choreography.message_flows);
            result.flows.extend(&choreography.sequence_flows);
        }
        if let Some(c) = &document.collaboration {
            result.flows.extend(&c.flows);
            result.conversations.extend(&c.conversation_nodes);
            result.conversation_links.extend(&c.conversation_links);
            for link in &c.conversation_links {
                if let Some(parent) = c
                    .conversation_nodes
                    .iter()
                    .find(|node| node.id == link.source || node.id == link.target)
                    .and_then(|node| node.parent.as_ref())
                {
                    result.parents.insert(link.id.clone(), parent.clone());
                }
            }
            for node in &c.conversation_nodes {
                if let Some(parent) = &node.parent {
                    result.parents.insert(node.id.clone(), parent.clone());
                }
            }
        }
        Ok(result)
    }
    fn hidden(&self, id: &XmlId, diagram: &processcraft_core::model::Diagram) -> bool {
        let mut parent = self.parents.get(id);
        for _ in 0..=32 {
            let Some(id) = parent else {
                return false;
            };
            if diagram
                .shapes
                .iter()
                .any(|shape| &shape.element == id && shape.is_expanded == Some(false))
            {
                return true;
            }
            parent = self.parents.get(id);
        }
        true
    }
    fn choreography(&self, id: &XmlId) -> Option<(&'a Choreography, &'a ChoreographyActivity)> {
        self.choreographies.iter().find_map(|c| {
            c.activities
                .iter()
                .find(|activity| &activity.id == id)
                .map(|activity| (*c, activity))
        })
    }
    fn group_label<'b>(&self, id: &XmlId, document: &'b BpmnDocument) -> Option<&'b str> {
        let group = self.groups.iter().find(|g| &g.id == id)?;
        let value_id = group.category_value_ref.as_ref()?;
        document
            .categories
            .iter()
            .flat_map(|c| &c.values)
            .find(|v| &v.id == value_id)?
            .value
            .as_deref()
    }
    fn data_object(&self, id: &XmlId) -> Option<&'a DataObject> {
        self.data_objects
            .iter()
            .copied()
            .find(|item| &item.id == id)
    }
    fn data_association(&self, id: &XmlId) -> Option<&'a DataAssociation> {
        self.data_associations
            .iter()
            .copied()
            .find(|item| &item.id == id)
    }
    fn conversation(&self, id: &XmlId) -> Option<&'a ConversationNode> {
        self.conversations
            .iter()
            .copied()
            .find(|node| &node.id == id)
    }
    fn conversation_link(&self, id: &XmlId) -> Option<&'a ConversationLink> {
        self.conversation_links
            .iter()
            .copied()
            .find(|link| &link.id == id)
    }
    fn is_default_flow(&self, flow: &Flow) -> bool {
        self.node(&flow.source).is_some_and(|(node,_)| matches!(&node.details,Some(NodeDetails::Gateway{default_flow:Some(id),..}) if id==&flow.id) || node.activity.as_ref().and_then(|a| a.default_flow.as_ref()) == Some(&flow.id))
    }
    fn node(&self, id: &XmlId) -> Option<(&'a Node, usize)> {
        self.nodes.iter().copied().find(|(n, _)| &n.id == id)
    }
    fn lane(&self, id: &XmlId) -> Option<(&'a Lane, usize)> {
        self.lanes.iter().copied().find(|(lane, _)| &lane.id == id)
    }
    fn flow(&self, id: &XmlId) -> Option<&'a Flow> {
        self.flows.iter().copied().find(|f| &f.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}
#[derive(Clone, Debug)]
pub enum HitGeometry {
    Shape(Bounds),
    Ellipse(Bounds),
    Diamond(Bounds),
    Hexagon(Bounds),
    Edge(Vec<Point>),
}
#[derive(Clone, Debug)]
pub struct HitRegion {
    pub key: ElementKey,
    pub diagram_element_id: XmlId,
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
        diagram_id: &XmlId,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
    ) -> Result<Self, RenderError> {
        Self::build_internal(document, diagram_id, appearance, fonts, family, true)
    }
    fn build_internal(
        document: &BpmnDocument,
        diagram_id: &XmlId,
        appearance: Appearance,
        fonts: &mut TextEngine,
        family: &str,
        shape_labels: bool,
    ) -> Result<Self, RenderError> {
        if matches!(document.capability, Capability::PreservedSource { .. }) {
            let mut scene = DiagramScene::new(33.0, 33.0)?;
            let bg = if matches!(appearance, Appearance::Dark) {
                [27, 31, 38]
            } else {
                [255, 255, 255]
            };
            scene.add_background_rect(0.0, 0.0, 33.0, 33.0, bg)?;
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
            if semantics.hidden(&shape.element, diagram) {
                continue;
            }
            let b = &shape.bounds;
            if b.width <= 0.0 || b.height <= 0.0 {
                return Err(RenderError("Invalid shape bounds"));
            }
            include(b.x, b.y)?;
            include(b.x + b.width, b.y + b.height)?;
            if let Some(l) = &shape.label {
                include(l.bounds.x, l.bounds.y)?;
                include(l.bounds.x + l.bounds.width, l.bounds.y + l.bounds.height)?;
            }
        }
        for edge in &diagram.edges {
            if semantics.hidden(&edge.element, diagram) {
                continue;
            }
            for p in &edge.waypoints {
                include(p.x, p.y)?;
            }
            if let Some(l) = &edge.label {
                include(l.bounds.x, l.bounds.y)?;
                include(l.bounds.x + l.bounds.width, l.bounds.y + l.bounds.height)?;
            }
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = extents.unwrap_or((0.0, 0.0, 1.0, 1.0));
        for edge in &diagram.edges {
            if semantics.hidden(&edge.element, diagram) {
                continue;
            }
            if edge.label.is_none()
                && let Some(p) = edge.waypoints.get(edge.waypoints.len() / 2)
            {
                let named = semantics
                    .flow(&edge.element)
                    .is_some_and(|f| !f.name.is_empty())
                    || semantics
                        .conversation_link(&edge.element)
                        .is_some_and(|link| !link.name.is_empty());
                if named {
                    minx = minx.min(p.x);
                    miny = miny.min(p.y);
                    maxx = maxx.max(p.x + 120.0);
                    maxy = maxy.max(p.y + 40.0);
                }
            }
        }
        let origin = (minx - 16.0, miny - 16.0);
        let mut scene = DiagramScene::for_diagram(
            ((maxx - minx) + 32.0) as f32,
            ((maxy - miny) + 32.0) as f32,
        )?;
        let (bg, fg, fill) = match appearance {
            Appearance::Light => ([255, 255, 255], [26, 37, 52], [255, 255, 255]),
            Appearance::Dark => ([27, 31, 38], [230, 234, 240], [47, 53, 63]),
        };
        scene.add_background_rect(0.0, 0.0, scene.width(), scene.height(), bg)?;
        let mut hits = Vec::new();
        let mut diagnostics = Vec::new();
        let mut ordered_shapes: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|shape| !semantics.hidden(&shape.element, diagram))
            .collect();
        ordered_shapes.sort_by_key(|shape| {
            if semantics.groups.iter().any(|g| g.id == shape.element)
                || document
                    .collaboration
                    .as_ref()
                    .is_some_and(|c| c.participants.iter().any(|p| p.id == shape.element))
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
            if semantics.groups.iter().any(|g| g.id == shape.element) {
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
                if shape_labels && let Some(name) = semantics.group_label(&shape.element, document)
                {
                    let fallback = Bounds {
                        x: b.x + 8.0,
                        y: b.y + 4.0,
                        width: (b.width - 16.0).max(0.0),
                        height: b.height.min(28.0),
                    };
                    let bounds = shape.label.as_ref().map(|l| &l.bounds).unwrap_or(&fallback);
                    if group_label_collision(bounds, &shape.id, diagram, &semantics) {
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
                        fg,
                        &mut diagnostics,
                    )?;
                    if shape.label.is_some() {
                        hits.push(HitRegion {
                            key: ElementKey {
                                document_id: document.id,
                                xml_id: shape.element.clone(),
                            },
                            diagram_element_id: shape.id.clone(),
                            geometry: HitGeometry::Shape(bounds.clone()),
                        });
                    }
                }
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.clone(),
                    geometry: HitGeometry::Shape(b.clone()),
                });
                continue;
            }
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let kind = node.map(|node| node.kind);
            let conversation = semantics.conversation(&shape.element);
            let choreography = semantics.choreography(&shape.element);
            let details = node.and_then(|node| node.details.as_ref());
            let mut name = choreography
                .map(|(_, activity)| activity.name.as_str())
                .or_else(|| conversation.map(|node| node.name.as_str()))
                .or_else(|| node.map(|node| node.name.as_str()))
                .or_else(|| {
                    semantics
                        .lane(&shape.element)
                        .map(|(lane, _)| lane.name.as_str())
                });
            if let Some(c) = &document.collaboration {
                for p in &c.participants {
                    if p.id == shape.element {
                        name = Some(p.name.as_str());
                    }
                }
            }
            let Some(name) = name else {
                diagnostics.push(format!("Unknown shape element {}", shape.element));
                continue;
            };
            if matches!(kind, Some(NodeKind::BoundaryEvent)) {
                let attached = match details {
                    Some(NodeDetails::BoundaryEvent { attached_to, .. }) => Some(attached_to),
                    Some(NodeDetails::Event { attached_to, .. }) => attached_to.as_ref(),
                    _ => None,
                };
                if attached.is_none_or(|id| {
                    !semantics.nodes.iter().any(|(n, _)| {
                        &n.id == id
                            && matches!(
                                n.kind,
                                NodeKind::Task
                                    | NodeKind::UserTask
                                    | NodeKind::ManualTask
                                    | NodeKind::ServiceTask
                                    | NodeKind::SendTask
                                    | NodeKind::ReceiveTask
                                    | NodeKind::ScriptTask
                                    | NodeKind::BusinessRuleTask
                            )
                    })
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
                (Appearance::Light, Some(NodeKind::StartEvent)) => [240, 249, 242],
                (Appearance::Dark, Some(NodeKind::StartEvent)) => [40, 68, 53],
                (Appearance::Light, Some(NodeKind::EndEvent)) => [255, 243, 240],
                (Appearance::Dark, Some(NodeKind::EndEvent)) => [74, 48, 45],
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
                        (x + w - 12.0, y),
                        (x + w, y + 12.0),
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
            if choreography.is_some() {
                let band_h = (h * 0.22).clamp(12.0, 24.0).min((h - 2.0).max(0.0));
                let band_color = if appearance == Appearance::Dark {
                    [60, 74, 90]
                } else {
                    [237, 241, 245]
                };
                if w > 2.0 && band_h > 1.0 {
                    scene.add_background_rect(
                        x + 1.0,
                        y + h - band_h,
                        (w - 2.0).max(0.0),
                        (band_h - 1.0).max(0.0),
                        band_color,
                    )?;
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
                    if matches!(kind, Some(NodeKind::BoundaryEvent)) && non_interrupting(details) {
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
                    } else if matches!(kind, Some(NodeKind::ExclusiveGateway)) {
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
                        (x + w - 12.0, y),
                        (x + w, y + 12.0),
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
                        outline(&mut scene, &path, stroke, 1.5)?;
                    }
                }
                _ if conversation.is_some() => {
                    lines(&mut scene, &hexagon_points(x, y, w, h), stroke, 1.5)?
                }
                _ => lines(
                    &mut scene,
                    &[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)],
                    stroke,
                    1.5,
                )?,
            }
            match kind {
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
                            (x + w - 12.0, y),
                            (x + w - 12.0, y + 12.0),
                            (x + w, y + 12.0),
                        ],
                        stroke,
                        1.0,
                    )?;
                }
                None if conversation.is_none() && choreography.is_none() => {
                    lines(&mut scene, &[(x + 24.0, y), (x + 24.0, y + h)], stroke, 1.0)?;
                }
                _ => {}
            }
            if matches!(
                kind,
                Some(
                    NodeKind::SubProcess
                        | NodeKind::AdHocSubProcess
                        | NodeKind::Transaction
                        | NodeKind::CallActivity
                )
            ) {
                match shape.is_expanded {
                    Some(false) => {
                        let offset = if node.is_some_and(|n| n.loop_characteristics.is_some()) {
                            -9.0
                        } else {
                            0.0
                        };
                        let (cx, cy) = (x + w / 2.0 + offset, y + h - 10.0);
                        let box_pts = [
                            (cx - 6.0, cy - 6.0),
                            (cx + 6.0, cy - 6.0),
                            (cx + 6.0, cy + 6.0),
                            (cx - 6.0, cy + 6.0),
                            (cx - 6.0, cy - 6.0),
                        ];
                        lines(&mut scene, &box_pts, stroke, 1.0)?;
                        lines(&mut scene, &[(cx - 4.0, cy), (cx + 4.0, cy)], stroke, 1.0)?;
                        lines(&mut scene, &[(cx, cy - 4.0), (cx, cy + 4.0)], stroke, 1.0)?;
                    }
                    None => diagnostics.push(format!(
                        "Activity {} expansion state is unspecified; collapsed marker omitted",
                        shape.element
                    )),
                    Some(true) => {}
                }
            }
            if let Some((scope, activity)) = choreography {
                if activity.participant_refs.len() != 2 {
                    diagnostics.push(format!(
                        "Choreography {} participant band order is not represented",
                        activity.id
                    ));
                }
                let other = activity
                    .participant_refs
                    .iter()
                    .find(|id| **id != activity.initiating_participant);
                let top = scope
                    .participants
                    .iter()
                    .find(|p| p.id == activity.initiating_participant)
                    .map(|p| p.name.as_str());
                let bottom = other
                    .and_then(|id| scope.participants.iter().find(|p| &p.id == id))
                    .map(|p| p.name.as_str());
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
                            fg,
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
            if let Some(characteristics) = node.and_then(|n| n.loop_characteristics.as_ref()) {
                let offset = if shape.is_expanded == Some(false)
                    && matches!(
                        kind,
                        Some(
                            NodeKind::SubProcess
                                | NodeKind::AdHocSubProcess
                                | NodeKind::Transaction
                                | NodeKind::CallActivity
                        )
                    ) {
                    9.0
                } else {
                    0.0
                };
                if w < 40.0 || h < 30.0 {
                    diagnostics.push(format!(
                        "Activity {} is too small to show its loop marker",
                        shape.element
                    ));
                } else {
                    draw_loop_marker(
                        &mut scene,
                        characteristics,
                        x + w / 2.0 + offset,
                        y + h - 10.0,
                        stroke,
                    )?;
                }
            }
            if node
                .and_then(|n| n.activity.as_ref())
                .is_some_and(|a| a.is_for_compensation == Some(true))
            {
                let has_neighbor = node.is_some_and(|n| n.loop_characteristics.is_some())
                    || shape.is_expanded == Some(false);
                if w < if has_neighbor { 60.0 } else { 30.0 } || h < 30.0 {
                    diagnostics.push(format!(
                        "Activity {} is too small to show its compensation marker",
                        shape.element
                    ));
                } else {
                    let offset = if node.is_some_and(|n| n.loop_characteristics.is_some())
                        || shape.is_expanded == Some(false)
                    {
                        20.0
                    } else {
                        0.0
                    };
                    let (cx, cy) = (x + w / 2.0 + offset, y + h - 10.0);
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
                }
            }
            draw_new_markers(
                &mut scene,
                kind,
                details,
                x,
                y,
                w,
                h,
                stroke,
                fc,
                &mut diagnostics,
            )?;
            let fallback_label = if choreography.is_some() {
                Bounds {
                    x: b.x,
                    y: b.y + b.height * 0.25,
                    width: b.width,
                    height: b.height * 0.5,
                }
            } else if matches!(
                kind,
                Some(
                    NodeKind::UserTask
                        | NodeKind::ManualTask
                        | NodeKind::ServiceTask
                        | NodeKind::SendTask
                        | NodeKind::ReceiveTask
                        | NodeKind::ScriptTask
                        | NodeKind::BusinessRuleTask
                )
            ) {
                Bounds {
                    x: b.x,
                    y: b.y + 20.0,
                    width: b.width,
                    height: (b.height - 20.0).max(0.0),
                }
            } else {
                b.clone()
            };
            let lb = shape
                .label
                .as_ref()
                .map(|l| &l.bounds)
                .unwrap_or(&fallback_label);
            let style = shape
                .label
                .as_ref()
                .and_then(|l| l.style.as_ref())
                .or(shape.style.as_ref());
            if shape_labels {
                label(
                    &mut scene,
                    fonts,
                    family,
                    name,
                    lb,
                    origin,
                    style,
                    fg,
                    &mut diagnostics,
                )?;
            }
            hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: shape.element.clone(),
                },
                diagram_element_id: shape.id.clone(),
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
                    _ if conversation.is_some() => HitGeometry::Hexagon(b.clone()),
                    _ => HitGeometry::Shape(b.clone()),
                },
            });
        }
        for edge in &diagram.edges {
            if semantics.hidden(&edge.element, diagram) {
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
                    diagram_element_id: edge.id.clone(),
                    geometry: HitGeometry::Edge(edge.waypoints.clone()),
                });
                continue;
            }
            if let Some(link) = semantics.conversation_link(&edge.element) {
                let stroke = color(edge.style.as_ref().and_then(|s| s.stroke.as_deref()), fg);
                let points: Vec<_> = edge
                    .waypoints
                    .iter()
                    .map(|p| ((p.x - origin.0) as f32, (p.y - origin.1) as f32))
                    .collect();
                double_link(&mut scene, &points, stroke)?;
                if !link.name.is_empty() && !edge.waypoints.is_empty() {
                    let p = &edge.waypoints[edge.waypoints.len() / 2];
                    let fallback = Bounds {
                        x: p.x,
                        y: p.y,
                        width: 120.0,
                        height: 40.0,
                    };
                    let bounds = edge
                        .label
                        .as_ref()
                        .map(|label| &label.bounds)
                        .unwrap_or(&fallback);
                    label(
                        &mut scene,
                        fonts,
                        family,
                        &link.name,
                        bounds,
                        origin,
                        edge.label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(edge.style.as_ref()),
                        fg,
                        &mut diagnostics,
                    )?;
                }
                hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: edge.element.clone(),
                    },
                    diagram_element_id: edge.id.clone(),
                    geometry: HitGeometry::Edge(edge.waypoints.clone()),
                });
                continue;
            }
            let flow = semantics.flow(&edge.element);
            let Some(flow) = flow else {
                diagnostics.push(format!("Unknown edge element {}", edge.element));
                continue;
            };
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
            if pts.len() > 1
                && (!matches!(flow.kind, FlowKind::Association)
                    || matches!(
                        flow.association_direction,
                        Some(AssociationDirection::One | AssociationDirection::Both)
                    ))
            {
                let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
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
            if pts.len() > 1
                && matches!(flow.kind, FlowKind::Association)
                && matches!(flow.association_direction, Some(AssociationDirection::Both))
            {
                let (a, b) = (pts[1], pts[0]);
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let wing = |v: f32| (b.0 - 9.0 * (angle + v).cos(), b.1 - 9.0 * (angle + v).sin());
                lines(&mut scene, &[wing(0.5), b, wing(-0.5)], stroke, 1.5)?;
            }
            if !flow.name.is_empty() && !edge.waypoints.is_empty() {
                let p = &edge.waypoints[edge.waypoints.len() / 2];
                let fallback = Bounds {
                    x: p.x,
                    y: p.y,
                    width: 120.0,
                    height: 40.0,
                };
                let bounds = edge.label.as_ref().map(|l| &l.bounds).unwrap_or(&fallback);
                label(
                    &mut scene,
                    fonts,
                    family,
                    &flow.name,
                    bounds,
                    origin,
                    edge.label
                        .as_ref()
                        .and_then(|l| l.style.as_ref())
                        .or(edge.style.as_ref()),
                    fg,
                    &mut diagnostics,
                )?;
            }
            hits.push(HitRegion {
                key: ElementKey {
                    document_id: document.id,
                    xml_id: edge.element.clone(),
                },
                diagram_element_id: edge.id.clone(),
                geometry: HitGeometry::Edge(edge.waypoints.clone()),
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
    shape_id: &XmlId,
    diagram: &processcraft_core::model::Diagram,
    semantics: &SemanticIndex<'_>,
) -> bool {
    diagram
        .shapes
        .iter()
        .filter(|shape| {
            &shape.id != shape_id
                && semantics.node(&shape.element).is_some()
                && !semantics.hidden(&shape.element, diagram)
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
#[allow(clippy::too_many_arguments)]
fn label(
    scene: &mut DiagramScene,
    fonts: &mut TextEngine,
    family: &str,
    text: &str,
    b: &Bounds,
    origin: (f64, f64),
    style: Option<&Style>,
    fg: [u8; 3],
    diagnostics: &mut Vec<String>,
) -> Result<(), RenderError> {
    if text.is_empty() {
        return Ok(());
    }
    let mut request = TextRequest::new(text, 12.0, ((b.width as f32) - 8.0).clamp(4.0, 16384.0));
    request.family = family;
    let mut layout = fonts.layout(request)?;
    diagnostics.extend(layout.diagnostics().iter().cloned());
    if layout.clip_to_bounds(
        ((b.width as f32) - 8.0).max(0.0),
        ((b.height as f32) - 8.0).max(0.0),
    ) {
        diagnostics.push(format!(
            "Label clipped because it exceeds its bounds: {text}"
        ));
    }
    scene.add_colored_label(
        layout,
        (b.x - origin.0) as f32 + 4.0,
        (b.y - origin.1) as f32 + 4.0,
        color(style.and_then(|s| s.font.as_deref()), fg),
    )
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
fn open_arrow(
    scene: &mut DiagramScene,
    points: &[(f32, f32)],
    stroke: [u8; 3],
) -> Result<(), RenderError> {
    if points.len() > 1 {
        let (a, b) = (points[points.len() - 2], points[points.len() - 1]);
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
        Some(NodeKind::AdHocSubProcess) => {
            let y = y + h - 24.0;
            lines(
                scene,
                &[
                    (cx - 7.0, y),
                    (cx - 4.0, y - 2.0),
                    (cx, y + 2.0),
                    (cx + 4.0, y + 2.0),
                    (cx + 7.0, y),
                ],
                stroke,
                1.2,
            )?;
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
    if let Some(NodeDetails::Event { definitions, .. }) = details {
        if definitions.len() > 1 {
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
                    lines(scene, &[(cx - r, cy - r), (cx + r, cy + r)], stroke, 3.0)?;
                    lines(scene, &[(cx + r, cy - r), (cx - r, cy + r)], stroke, 3.0)?;
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
                if *is_sequential {
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
    let r = 10.0_f32.min(w / 2.0).min(h / 2.0);
    let k = 0.5522848;
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k * r, y, x + w, y + r - k * r, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(
        x + w,
        y + h - r + k * r,
        x + w - r + k * r,
        y + h,
        x + w - r,
        y + h,
    );
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k * r, y + h, x, y + h - r + k * r, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k * r, x + r - k * r, y, x + r, y);
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
        diagram_id: &XmlId,
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
            if matches!(appearance, Appearance::Dark) {
                empty.add_background_rect(
                    0.0,
                    0.0,
                    view.width as f32,
                    view.height as f32,
                    [27, 31, 38],
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
        diagram
            .shapes
            .retain(|s| !visibility.hidden(&s.element, original_diagram));
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
            if s.label.as_ref().is_some_and(|l| !inside(&l.bounds)) {
                s.label = None;
            }
            if let Some(l) = &mut s.label {
                rebase(&mut l.bounds);
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
        let fg = if matches!(appearance, Appearance::Dark) {
            [230, 234, 240]
        } else {
            [26, 37, 52]
        };
        let semantics = SemanticIndex::new(document)?;
        // Labels are culled independently of their owning shape.
        for shape in &original.shapes {
            if semantics.hidden(&shape.element, original) {
                continue;
            }
            let node = semantics.node(&shape.element).map(|(node, _)| node);
            let kind = node.map(|node| node.kind);
            let conversation = semantics.conversation(&shape.element);
            let choreography = semantics.choreography(&shape.element);
            let mut name = semantics
                .group_label(&shape.element, document)
                .or_else(|| choreography.map(|(_, activity)| activity.name.as_str()))
                .or_else(|| conversation.map(|node| node.name.as_str()))
                .or_else(|| node.map(|node| node.name.as_str()))
                .or_else(|| {
                    semantics
                        .lane(&shape.element)
                        .map(|(lane, _)| lane.name.as_str())
                });
            if let Some(c) = &document.collaboration {
                for p in &c.participants {
                    if p.id == shape.element {
                        name = Some(p.name.as_str());
                    }
                }
            }
            let Some(name) = name else { continue };
            if shape.label.is_none() && !built.hits.iter().any(|h| h.diagram_element_id == shape.id)
            {
                continue;
            }
            let b = &shape.bounds;
            let fallback = if semantics.groups.iter().any(|g| g.id == shape.element) {
                Bounds {
                    x: b.x + 8.0,
                    y: b.y + 4.0,
                    width: (b.width - 16.0).max(0.0),
                    height: b.height.min(28.0),
                }
            } else if choreography.is_some() {
                Bounds {
                    x: b.x,
                    y: b.y + b.height * 0.25,
                    width: b.width,
                    height: b.height * 0.5,
                }
            } else if matches!(
                kind,
                Some(
                    NodeKind::UserTask
                        | NodeKind::ManualTask
                        | NodeKind::ServiceTask
                        | NodeKind::SendTask
                        | NodeKind::ReceiveTask
                        | NodeKind::ScriptTask
                        | NodeKind::BusinessRuleTask
                )
            ) {
                Bounds {
                    x: b.x,
                    y: b.y + 20.0,
                    width: b.width,
                    height: (b.height - 20.0).max(0.0),
                }
            } else {
                b.clone()
            };
            let bounds = shape.label.as_ref().map(|l| &l.bounds).unwrap_or(&fallback);
            if name.is_empty() || !inside(bounds) {
                continue;
            }
            if semantics.groups.iter().any(|g| g.id == shape.element)
                && group_label_collision(bounds, &shape.id, original, &semantics)
            {
                built.diagnostics.push(format!(
                    "Group {} label overlaps another shape",
                    shape.element
                ));
            }
            label(
                &mut built.scene,
                fonts,
                family,
                name,
                bounds,
                (view.x + built.origin.0, view.y + built.origin.1),
                shape
                    .label
                    .as_ref()
                    .and_then(|l| l.style.as_ref())
                    .or(shape.style.as_ref()),
                fg,
                &mut built.diagnostics,
            )?;
            if shape.label.is_some() {
                let mut local = bounds.clone();
                rebase(&mut local);
                built.hits.push(HitRegion {
                    key: ElementKey {
                        document_id: document.id,
                        xml_id: shape.element.clone(),
                    },
                    diagram_element_id: shape.id.clone(),
                    geometry: HitGeometry::Shape(local),
                });
            }
        }
        for edge in &original.edges {
            if semantics.hidden(&edge.element, original) {
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
                    let pts: Vec<_> = edge.waypoints[edge.waypoints.len() - 2..]
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
                        diagram_element_id: edge.id.clone(),
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            if let Some(link) = semantics.conversation_link(&edge.element) {
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
                        double_link(&mut built.scene, &points, stroke)?;
                        visible = true;
                    }
                }
                if !link.name.is_empty() && !edge.waypoints.is_empty() {
                    let p = &edge.waypoints[edge.waypoints.len() / 2];
                    let fallback = Bounds {
                        x: p.x,
                        y: p.y,
                        width: 120.0,
                        height: 40.0,
                    };
                    let bounds = edge.label.as_ref().map(|l| &l.bounds).unwrap_or(&fallback);
                    if inside(bounds) {
                        label(
                            &mut built.scene,
                            fonts,
                            family,
                            &link.name,
                            bounds,
                            (view.x + built.origin.0, view.y + built.origin.1),
                            edge.label
                                .as_ref()
                                .and_then(|l| l.style.as_ref())
                                .or(edge.style.as_ref()),
                            fg,
                            &mut built.diagnostics,
                        )?;
                    }
                }
                if visible {
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.clone(),
                        geometry: HitGeometry::Edge(edge.waypoints.clone()),
                    });
                }
                continue;
            }
            let Some(flow) = semantics.flow(&edge.element) else {
                continue;
            };
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
                && (!matches!(flow.kind, FlowKind::Association)
                    || matches!(
                        flow.association_direction,
                        Some(AssociationDirection::One | AssociationDirection::Both)
                    ))
            {
                let b = &edge.waypoints[edge.waypoints.len() - 1];
                let a = &edge.waypoints[edge.waypoints.len() - 2];
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
                let a = &edge.waypoints[1];
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
            if !flow.name.is_empty() && !edge.waypoints.is_empty() {
                let p = &edge.waypoints[edge.waypoints.len() / 2];
                let fallback = Bounds {
                    x: p.x,
                    y: p.y,
                    width: 120.0,
                    height: 40.0,
                };
                let bounds = edge.label.as_ref().map(|l| &l.bounds).unwrap_or(&fallback);
                if inside(bounds) {
                    let mut local = bounds.clone();
                    rebase(&mut local);
                    built.hits.push(HitRegion {
                        key: ElementKey {
                            document_id: document.id,
                            xml_id: edge.element.clone(),
                        },
                        diagram_element_id: edge.id.clone(),
                        geometry: HitGeometry::Shape(local),
                    });
                    label(
                        &mut built.scene,
                        fonts,
                        family,
                        &flow.name,
                        bounds,
                        (view.x + built.origin.0, view.y + built.origin.1),
                        edge.label
                            .as_ref()
                            .and_then(|l| l.style.as_ref())
                            .or(edge.style.as_ref()),
                        fg,
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
                diagram_element_id: edge.id.clone(),
                geometry: HitGeometry::Edge(edge.waypoints.clone()),
            });
        }
        let bg = if matches!(appearance, Appearance::Dark) {
            [27, 31, 38]
        } else {
            [255, 255, 255]
        };
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
        for h in &mut hits {
            match &mut h.geometry {
                HitGeometry::Shape(b)
                | HitGeometry::Ellipse(b)
                | HitGeometry::Diamond(b)
                | HitGeometry::Hexagon(b) => {
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
