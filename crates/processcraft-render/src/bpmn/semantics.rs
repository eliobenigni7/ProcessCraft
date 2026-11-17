//! Semantic index over a `BpmnDocument` plus the node/event/data naming helpers the scene builder shares.

use crate::RenderError;
use processcraft_core::{
    bpmn_collaboration::{Choreography, ChoreographyActivity, ConversationLink, ConversationNode},
    bpmn_data::{DataAssociation, DataObject, DataPort},
    bpmn_model::{EventDefinition, NodeDetails},
    ids::ElementSelector,
    model::{Bounds, BpmnDocument, Flow, FlowKind, Lane, Node, NodeKind},
};

pub(super) const DI_EXTENSION_DIAGNOSTIC: &str = "DI extensions/foreign attributes are preserved in BPMN; their visual effects are not interpreted.";

pub(super) fn has_uninterpreted_di(diagram: &processcraft_core::model::Diagram) -> bool {
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

pub(super) struct SemanticIndex<'a> {
    pub(super) references: processcraft_core::reference_resolution::ReferenceIndex,
    pub(super) participants: Vec<&'a processcraft_core::model::Participant>,
    pub(super) groups: Vec<&'a processcraft_core::bpmn_model::Group>,
    pub(super) nodes: Vec<(&'a Node, usize)>,
    pub(super) lanes: Vec<(&'a Lane, usize)>,
    pub(super) flows: Vec<&'a Flow>,
    pub(super) parents: std::collections::BTreeMap<ElementSelector, ElementSelector>,
    pub(super) conversations: Vec<&'a ConversationNode>,
    pub(super) conversation_links: Vec<&'a ConversationLink>,
    pub(super) data_associations: Vec<&'a DataAssociation>,
    pub(super) data_objects: Vec<&'a DataObject>,
    pub(super) ports: Vec<(&'a DataPort, bool)>,
    pub(super) activity_ports: std::collections::BTreeSet<ElementSelector>,
    pub(super) choreographies: Vec<(&'a Choreography, &'a ChoreographyActivity)>,
    pub(super) choreography_owners: std::collections::BTreeSet<ElementSelector>,
    pub(super) target_namespace: Option<String>,
    pub(super) event_resources: &'a [processcraft_core::bpmn_model::EventResource],
}

impl<'a> SemanticIndex<'a> {
    pub(super) fn new(document: &'a BpmnDocument) -> Result<Self, RenderError> {
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
    pub(super) fn resolved(
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
    pub(super) fn participant(
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
    pub(super) fn visibility(
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
    pub(super) fn shape_visibility(
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
    pub(super) fn edge_visibility(
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
    pub(super) fn choreography(
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
    pub(super) fn group_label(
        &self,
        id: &ElementSelector,
        document: &BpmnDocument,
    ) -> Option<String> {
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
    pub(super) fn data_object(&self, id: &ElementSelector) -> Option<&'a DataObject> {
        self.data_objects
            .iter()
            .copied()
            .find(|item| item.id.as_ref().is_some_and(|identity| identity == id))
    }
    pub(super) fn data_association(&self, id: &ElementSelector) -> Option<&'a DataAssociation> {
        self.data_associations
            .iter()
            .copied()
            .find(|item| item.id.as_ref().is_some_and(|identity| identity == id))
    }
    pub(super) fn port(&self, id: &ElementSelector) -> Option<(&'a DataPort, bool)> {
        self.ports
            .iter()
            .copied()
            .find(|(port, _)| port.id.as_ref().is_some_and(|identity| identity == id))
    }
    /// Name of the Message a message flow references (BPMN 2.0.2 section 9.3).
    pub(super) fn message_name(&self, flow: &Flow) -> Option<&'a str> {
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
    pub(super) fn conversation(&self, id: &ElementSelector) -> Option<&'a ConversationNode> {
        self.conversations
            .iter()
            .copied()
            .find(|node| &node.id == id)
    }
    pub(super) fn conversation_link(&self, id: &ElementSelector) -> Option<&'a ConversationLink> {
        self.conversation_links
            .iter()
            .copied()
            .find(|link| &link.id == id)
    }
    pub(super) fn is_default_flow(&self, flow: &Flow) -> bool {
        self.resolved(&flow.source).and_then(|source| self.node(&source)).is_some_and(|(node,_)| matches!(&node.details,Some(NodeDetails::Gateway{default_flow:Some(id),..}) if id==&flow.id) || node.activity.as_ref().and_then(|a| a.default_flow.as_ref()).is_some_and(|id| id == &flow.id))
    }
    pub(super) fn conditional_flow_marker(&self, flow: &Flow) -> bool {
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
    pub(super) fn node(&self, id: &ElementSelector) -> Option<(&'a Node, usize)> {
        self.nodes.iter().copied().find(|(n, _)| &n.id == id)
    }
    pub(super) fn lane(&self, id: &ElementSelector) -> Option<(&'a Lane, usize)> {
        self.lanes.iter().copied().find(|(lane, _)| &lane.id == id)
    }
    /// Authored paint rank of an edge. Message flows are lifted above every shape,
    /// as in common modelers, so a pool or participant band fill listed later in the
    /// plane never hides them.
    pub(super) fn edge_paint_rank(
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
    pub(super) fn flow(&self, id: &ElementSelector) -> Option<&'a Flow> {
        self.flows.iter().copied().find(|f| &f.id == id)
    }
}

pub(super) fn effective_event_definitions<'a>(
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CalledTarget {
    Unspecified,
    Process,
    Global(NodeKind),
    Unresolved,
}

pub(super) fn called_target(node: Option<&Node>, document: &BpmnDocument) -> CalledTarget {
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

pub(super) fn node_display_name<'a>(
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

pub(super) fn data_label(
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
