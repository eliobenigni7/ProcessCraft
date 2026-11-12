use crate::ids::{
    CheckpointId, CommentId, DictionaryEntryId, DocumentId, ElementKey, ProjectId, XmlId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const GENERATED_TARGET_NAMESPACE: &str = "urn:processcraft:generated";
pub fn legacy_is_executable() -> Option<bool> {
    Some(false)
}
fn is_legacy_executable(value: &Option<bool>) -> bool {
    *value == Some(false)
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectData {
    pub id: ProjectId,
    pub name: String,
    pub description: String,
    pub folders: Vec<Folder>,
    pub documents: Vec<Arc<BpmnDocument>>,
    pub resources: Vec<Resource>,
    pub links: Vec<NavigationLink>,
    pub dictionary_entries: Vec<DictionaryEntry>,
    pub dictionary_links: Vec<DictionaryLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<Checkpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_checkpoint: Option<CheckpointId>,
}
impl ProjectData {
    pub fn new(name: String) -> Self {
        Self {
            id: ProjectId::new(),
            name,
            description: String::new(),
            folders: vec![],
            documents: vec![],
            resources: vec![],
            links: vec![],
            dictionary_entries: vec![],
            dictionary_links: vec![],
            comments: vec![],
            checkpoints: vec![],
            reference_checkpoint: None,
        }
    }
}
/// Frozen project content excludes the checkpoint catalog and reference marker.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenProject {
    pub id: ProjectId,
    pub name: String,
    pub description: String,
    pub folders: Vec<Folder>,
    pub documents: Vec<Arc<BpmnDocument>>,
    pub resources: Vec<Resource>,
    pub links: Vec<NavigationLink>,
    pub dictionary_entries: Vec<DictionaryEntry>,
    pub dictionary_links: Vec<DictionaryLink>,
    pub comments: Vec<Comment>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub name: String,
    pub created_unix_ms: u64,
    pub content: FrozenProject,
}
impl FrozenProject {
    pub fn capture(p: &ProjectData) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            description: p.description.clone(),
            folders: p.folders.clone(),
            documents: p.documents.clone(),
            resources: p.resources.clone(),
            links: p.links.clone(),
            dictionary_entries: p.dictionary_entries.clone(),
            dictionary_links: p.dictionary_links.clone(),
            comments: p.comments.clone(),
        }
    }
    pub fn project(&self) -> ProjectData {
        ProjectData {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            folders: self.folders.clone(),
            documents: self.documents.clone(),
            resources: self.resources.clone(),
            links: self.links.clone(),
            dictionary_entries: self.dictionary_entries.clone(),
            dictionary_links: self.dictionary_links.clone(),
            comments: self.comments.clone(),
            checkpoints: vec![],
            reference_checkpoint: None,
        }
    }
}
/// Local plain-text annotation; `author` is descriptive, not authenticated.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: CommentId,
    pub target: ElementKey,
    pub author: String,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DictionaryKind {
    Role,
    System,
    Document,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictionaryEntry {
    pub id: DictionaryEntryId,
    pub kind: DictionaryKind,
    pub name: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DictionaryTarget {
    Entry(DictionaryEntryId),
    Literal(String),
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictionaryLink {
    pub source: ElementKey,
    pub kind: DictionaryKind,
    pub target: DictionaryTarget,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: XmlId,
    pub name: String,
    pub parent: Option<XmlId>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: XmlId,
    pub media_type: String,
    pub content_hash: [u8; 32],
    pub bytes: Arc<[u8]>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NavigationLink {
    pub source: crate::ids::ElementKey,
    pub target: DocumentId,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BpmnDocument {
    pub id: DocumentId,
    pub name: String,
    pub folder: Option<XmlId>,
    pub definitions_id: crate::ids::ElementIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definitions: Option<DefinitionsMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub global_tasks: Vec<crate::bpmn_activity::GlobalTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correlation_properties: Vec<crate::bpmn_activity::CorrelationProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<DefinitionsExtension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<DefinitionsRelationship>,
    #[serde(
        default,
        skip_serializing_if = "crate::bpmn_catalog::SupportingCatalog::is_empty"
    )]
    pub catalog: crate::bpmn_catalog::SupportingCatalog,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<crate::bpmn_model::Category>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_resources: Vec<crate::bpmn_model::EventResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_event_definitions: Vec<crate::bpmn_model::EventDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_stores: Vec<crate::bpmn_data::DataStore>,
    pub processes: Vec<Process>,
    pub collaboration: Option<Collaboration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choreographies: Vec<crate::bpmn_collaboration::Choreography>,
    pub diagrams: Vec<Diagram>,
    pub provenance: Option<Provenance>,
    pub capability: Capability,
}
impl BpmnDocument {
    /// All modeled semantic and diagram identities, including optional child IDs.
    pub fn element_identities(&self) -> Vec<&crate::ids::ElementIdentity> {
        use crate::bpmn_model::{EventDefinition, EventResource, Expression, NodeDetails};
        fn io_ids<'a>(
            io: &'a crate::bpmn_data::IoSpecification,
            out: &mut Vec<&'a crate::ids::ElementIdentity>,
        ) {
            out.extend(io.id.iter());
            for port in io.inputs.iter().chain(&io.outputs) {
                out.extend(port.id.iter());
                if let Some(state) = port.item_aware.as_ref().and_then(|m| m.data_state.as_ref()) {
                    out.extend(state.id.iter());
                }
            }
            for set in io.input_sets.iter().chain(&io.output_sets) {
                out.extend(set.id.iter());
            }
        }
        let mut out = vec![&self.definitions_id];
        let activity = crate::activity_read::activity_read(self);
        out.extend(activity.ids);
        for base in activity.bases {
            for documentation in &base.documentation {
                out.extend(documentation.id.iter());
            }
        }
        for extension in &self.extensions {
            for documentation in &extension.documentation {
                out.extend(documentation.id.iter());
            }
        }
        for relationship in &self.relationships {
            out.extend(relationship.id.iter());
        }
        crate::metadata_read::visit_base_metadata(self, |_, base| {
            if let Some(base) = base {
                for doc in &base.documentation {
                    out.extend(doc.id.iter());
                }
            }
        });
        crate::metadata_read::visit_legacy_metadata(self, |_, docs, _| {
            if let crate::bpmn_metadata::DocumentationSet::Structured(docs) = docs {
                for doc in docs {
                    out.extend(doc.id.iter());
                }
            }
        });
        for category in &self.categories {
            out.push(&category.id);
            for value in &category.values {
                out.push(&value.id);
            }
        }
        for resource in &self.event_resources {
            out.push(match resource {
                EventResource::Signal { id, .. }
                | EventResource::Message { id, .. }
                | EventResource::Error { id, .. }
                | EventResource::Escalation { id, .. } => id,
            });
        }
        for definition in &self.root_event_definitions {
            out.extend(definition.id());
        }
        for store in &self.data_stores {
            out.extend(store.id.iter());
            if let Some(state) = store
                .item_aware
                .as_ref()
                .and_then(|m| m.data_state.as_ref())
            {
                out.extend(state.id.iter());
            }
        }
        for item in &self.catalog.item_definitions {
            out.extend(item.id.iter());
        }
        for interface in &self.catalog.interfaces {
            out.extend(interface.id.iter());
            for operation in &interface.operations {
                out.extend(operation.id.iter());
            }
        }
        for resource in &self.catalog.resources {
            out.extend(resource.id.iter());
            for parameter in &resource.parameters {
                out.extend(parameter.id.iter());
            }
        }
        for expression in self.expressions() {
            if let Expression::Structured(data) = expression {
                out.extend(data.id.iter());
                if let crate::bpmn_model::ExpressionBody::Mixed { parts } = &data.text {
                    for part in parts {
                        if let crate::bpmn_model::ExpressionPart::Documentation { documentation } =
                            part
                        {
                            out.extend(documentation.id.iter());
                        }
                    }
                }
            }
        }
        let mut pending = Vec::new();
        for process in &self.processes {
            out.push(&process.id);
            out.extend(process.lane_set_id.iter());
            if let Some(io) = &process.io_specification {
                io_ids(io, &mut out);
            }
            pending.push((
                &process.nodes[..],
                &process.lanes[..],
                &process.flows[..],
                &process.data_objects[..],
                &process.data_associations[..],
                &process.groups[..],
            ));
        }
        while let Some((nodes, lanes, flows, objects, associations, groups)) = pending.pop() {
            for lane in lanes {
                out.push(&lane.id);
                out.extend(lane.child_lane_set_id.iter());
            }
            for flow in flows {
                out.push(&flow.id);
            }
            for group in groups {
                out.push(&group.id);
            }
            for object in objects {
                out.extend(object.id.iter());
                if let Some(state) = object
                    .item_aware
                    .as_ref()
                    .and_then(|m| m.data_state.as_ref())
                {
                    out.extend(state.id.iter());
                }
            }
            for association in associations {
                out.extend(association.id.iter());
                for assignment in &association.assignments {
                    out.extend(assignment.id.iter());
                }
            }
            for node in nodes {
                out.push(&node.id);
                if let Some(loop_characteristics) = &node.loop_characteristics {
                    out.extend(loop_characteristics.id());
                }
                if let Some(io) = &node.io_specification {
                    io_ids(io, &mut out);
                }
                if let Some(state) = node.item_aware.as_ref().and_then(|m| m.data_state.as_ref()) {
                    out.extend(state.id.iter());
                }
                match &node.details {
                    Some(NodeDetails::SubProcess { content }) => {
                        out.extend(content.lane_set_id.iter());
                        pending.push((
                            &content.nodes,
                            &content.lanes,
                            &content.flows,
                            &content.data_objects,
                            &content.data_associations,
                            &content.groups,
                        ));
                    }
                    Some(NodeDetails::Event {
                        definitions,
                        properties,
                        io,
                        ..
                    }) => {
                        for property in properties {
                            out.extend(property.id.iter());
                            if let Some(state) = property
                                .item_aware
                                .as_ref()
                                .and_then(|m| m.data_state.as_ref())
                            {
                                out.extend(state.id.iter());
                            }
                        }
                        if let Some(io) = io {
                            use crate::bpmn_model::EventIo;
                            let (ports, set) = match io {
                                EventIo::Catch {
                                    outputs,
                                    output_set,
                                } => (outputs, output_set),
                                EventIo::Throw { inputs, input_set } => (inputs, input_set),
                            };
                            for port in ports {
                                out.extend(port.id.iter());
                                if let Some(state) =
                                    port.item_aware.as_ref().and_then(|m| m.data_state.as_ref())
                                {
                                    out.extend(state.id.iter());
                                }
                            }
                            if let Some(set) = set {
                                out.extend(set.id.iter());
                            }
                        }
                        for definition in definitions {
                            let id = match definition {
                                EventDefinition::Message { id, .. }
                                | EventDefinition::Signal { id, .. }
                                | EventDefinition::Error { id, .. }
                                | EventDefinition::Escalation { id, .. }
                                | EventDefinition::Conditional { id, .. }
                                | EventDefinition::Link { id, .. }
                                | EventDefinition::Cancel { id, .. }
                                | EventDefinition::Compensate { id, .. }
                                | EventDefinition::Terminate { id, .. }
                                | EventDefinition::Timer { id, .. } => id,
                            };
                            out.extend(id.iter());
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(collaboration) = &self.collaboration {
            out.push(&collaboration.id);
            for p in &collaboration.participants {
                out.push(&p.id);
            }
            for flow in &collaboration.flows {
                out.push(&flow.id);
            }
            for node in &collaboration.conversation_nodes {
                out.push(&node.id);
            }
            for link in &collaboration.conversation_links {
                out.push(&link.id);
            }
        }
        for choreography in &self.choreographies {
            out.push(&choreography.id);
            for p in &choreography.participants {
                out.push(&p.id);
            }
            for f in choreography
                .message_flows
                .iter()
                .chain(&choreography.sequence_flows)
            {
                out.push(&f.id);
            }
            for activity in &choreography.activities {
                out.push(&activity.id);
            }
        }
        for diagram in &self.diagrams {
            out.push(&diagram.id);
            out.push(&diagram.plane_id);
            for style in &diagram.label_styles {
                out.extend(style.id.iter());
            }
            for shape in &diagram.shapes {
                out.push(&shape.id);
                if let Some(label) = &shape.label {
                    out.extend(label.id.iter());
                }
            }
            for edge in &diagram.edges {
                out.push(&edge.id);
                if let Some(label) = &edge.label {
                    out.extend(label.id.iter());
                }
            }
        }
        out
    }
    pub fn xml_id_for_selector(&self, selector: &crate::ids::ElementSelector) -> Option<&XmlId> {
        self.element_identities()
            .into_iter()
            .find(|id| id.selector() == *selector)
            .and_then(|id| id.xml_id())
    }
    pub fn has_native_identity_features(&self) -> bool {
        use crate::ids::ElementIdentity;
        if crate::activity_read::activity_read(self).native_references {
            return true;
        }
        if self
            .element_identities()
            .iter()
            .any(|id| matches!(id, ElementIdentity::Native { .. }))
        {
            return true;
        }
        if self
            .processes
            .iter()
            .any(|p| p.name.is_none() || p.is_executable != Some(false))
        {
            return true;
        }
        if self
            .root_event_definitions
            .iter()
            .any(crate::bpmn_model::EventDefinition::has_native_references)
        {
            return true;
        }
        for scope in crate::queries::scopes(self) {
            if scope.lanes.iter().any(|l| {
                l.parent.as_ref().is_some_and(|r| r.is_native())
                    || l.members.iter().any(|r| r.is_native())
            }) || scope
                .flows
                .iter()
                .any(|f| f.source.is_native() || f.target.is_native())
                || scope.data_associations.iter().any(|a| {
                    a.owner.is_native()
                        || a.target.is_native()
                        || a.sources.iter().any(|r| r.is_native())
                })
                || scope
                    .groups
                    .iter()
                    .any(|g| g.category_value_ref.as_ref().is_some_and(|r| r.is_native()))
            {
                return true;
            }
            for node in scope.nodes {
                if node
                    .activity
                    .as_ref()
                    .and_then(|a| a.default_flow.as_ref())
                    .is_some_and(|r| r.is_native())
                {
                    return true;
                }
                if let Some(io) = &node.io_specification
                    && io.input_sets.iter().chain(&io.output_sets).any(|set| {
                        set.data_refs
                            .iter()
                            .chain(&set.optional_refs)
                            .chain(&set.while_executing_refs)
                            .chain(&set.counterpart_refs)
                            .any(|r| r.is_native())
                    })
                {
                    return true;
                }
                use crate::bpmn_model::{EventDefinition, NodeDetails};
                match &node.details {
                    Some(NodeDetails::Gateway {
                        default_flow: Some(r),
                        ..
                    })
                    | Some(NodeDetails::DataObjectReference {
                        data_object_ref: Some(r),
                    })
                    | Some(NodeDetails::BoundaryEvent { attached_to: r, .. })
                        if r.is_native() =>
                    {
                        return true;
                    }
                    Some(NodeDetails::DataStoreReference {
                        data_store_ref: Some(crate::bpmn_data::DataStoreRef::Legacy(r)),
                    }) if r.is_native() => return true,
                    Some(NodeDetails::Event {
                        definitions,
                        attached_to,
                        definition_refs,
                        io,
                        ..
                    }) => {
                        if attached_to.as_ref().is_some_and(|r| r.is_native())
                            || definition_refs.iter().any(|r| r.is_native())
                        {
                            return true;
                        }
                        if let Some(io) = io {
                            use crate::bpmn_model::EventIo;
                            let set = match io {
                                EventIo::Catch { output_set, .. } => output_set,
                                EventIo::Throw { input_set, .. } => input_set,
                            };
                            if set.as_ref().is_some_and(|set| {
                                set.data_refs
                                    .iter()
                                    .chain(&set.optional_refs)
                                    .chain(&set.while_executing_refs)
                                    .chain(&set.counterpart_refs)
                                    .any(|r| r.is_native())
                            }) {
                                return true;
                            }
                        }
                        for definition in definitions {
                            let native = match definition {
                                EventDefinition::Message {
                                    message_ref,
                                    operation_ref,
                                    ..
                                } => {
                                    message_ref.as_ref().is_some_and(|r| r.is_native())
                                        || operation_ref.as_ref().is_some_and(|r| r.is_native())
                                }
                                EventDefinition::Signal { signal_ref, .. } => {
                                    signal_ref.as_ref().is_some_and(|r| r.is_native())
                                }
                                EventDefinition::Error { error_ref, .. } => {
                                    error_ref.as_ref().is_some_and(|r| r.is_native())
                                }
                                EventDefinition::Escalation { escalation_ref, .. } => {
                                    escalation_ref.as_ref().is_some_and(|r| r.is_native())
                                }
                                EventDefinition::Link {
                                    sources, target, ..
                                } => {
                                    sources.iter().any(|r| r.is_native())
                                        || target.as_ref().is_some_and(|r| r.is_native())
                                }
                                EventDefinition::Compensate { activity_ref, .. } => {
                                    activity_ref.as_ref().is_some_and(|r| r.is_native())
                                }
                                _ => false,
                            };
                            if native {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if self
            .processes
            .iter()
            .filter_map(|p| p.io_specification.as_ref())
            .any(|io| {
                io.input_sets.iter().chain(&io.output_sets).any(|set| {
                    set.data_refs
                        .iter()
                        .chain(&set.optional_refs)
                        .chain(&set.while_executing_refs)
                        .chain(&set.counterpart_refs)
                        .any(|r| r.is_native())
                })
            })
        {
            return true;
        }
        if self.diagrams.iter().any(|d| {
            d.plane.is_native()
                || d.shapes.iter().any(|s| {
                    s.element.is_native()
                        || s.label.as_ref().is_some_and(
                            |l| matches!(&l.style_ref, Some(DiStyleRef::Local(r)) if r.is_native()),
                        )
                })
                || d.edges.iter().any(|e| {
                    e.element.is_native()
                        || e.label.as_ref().is_some_and(
                            |l| matches!(&l.style_ref, Some(DiStyleRef::Local(r)) if r.is_native()),
                        )
                        || e.source_element.as_ref().is_some_and(|r| r.is_native())
                        || e.target_element.as_ref().is_some_and(|r| r.is_native())
                })
        }) {
            return true;
        }
        if let Capability::RetainedExtensions { extensions, .. } = &self.capability
            && extensions.iter().any(|e| e.owner.is_native())
        {
            return true;
        }
        if self.collaboration.as_ref().is_some_and(|c| {
            c.participants
                .iter()
                .any(|p| p.process.as_ref().is_some_and(|r| r.is_native()))
                || c.flows
                    .iter()
                    .any(|f| f.source.is_native() || f.target.is_native())
                || c.conversation_nodes.iter().any(|n| {
                    n.parent.as_ref().is_some_and(|r| r.is_native())
                        || n.participant_refs.iter().any(|r| r.is_native())
                })
                || c.conversation_links
                    .iter()
                    .any(|l| l.source.is_native() || l.target.is_native())
        }) {
            return true;
        }
        if self.choreographies.iter().any(|c| {
            c.participants
                .iter()
                .any(|p| p.process.as_ref().is_some_and(|r| r.is_native()))
                || c.message_flows
                    .iter()
                    .chain(&c.sequence_flows)
                    .any(|f| f.source.is_native() || f.target.is_native())
                || c.activities.iter().any(|a| {
                    a.initiating_participant.is_native()
                        || a.participant_refs
                            .iter()
                            .chain(&a.message_flow_refs)
                            .any(|r| r.is_native())
                })
        }) {
            return true;
        }
        if crate::queries::scopes(self).iter().any(|s| {
            s.nodes.iter().any(|n| {
                n.name.is_none()
                    || matches!(
                        &n.details,
                        Some(crate::bpmn_model::NodeDetails::TextAnnotation { text: Some(_), .. })
                    )
            }) || s.lanes.iter().any(|l| l.name.is_none())
                || s.flows.iter().any(|f| f.name.is_none())
        }) {
            return true;
        }
        if self.collaboration.as_ref().is_some_and(|c| {
            c.name.is_some()
                || c.participants.iter().any(|p| p.name.is_none())
                || c.flows.iter().any(|f| f.name.is_none())
                || c.conversation_nodes.iter().any(|n| n.name.is_none())
                || c.conversation_links.iter().any(|l| l.name.is_none())
        }) {
            return true;
        }
        if self.choreographies.iter().any(|c| {
            c.name.is_none()
                || c.participants.iter().any(|p| p.name.is_none())
                || c.activities.iter().any(|a| a.name.is_none())
                || c.message_flows
                    .iter()
                    .chain(&c.sequence_flows)
                    .any(|f| f.name.is_none())
        }) {
            return true;
        }
        if self.diagrams.iter().any(|diagram| diagram.name.is_some()) {
            return true;
        }
        false
    }
    pub fn new(name: String, definitions_id: crate::ids::ElementIdentity) -> Self {
        Self {
            id: DocumentId::new(),
            name,
            folder: None,
            definitions_id,
            definitions: None,
            global_tasks: vec![],
            correlation_properties: vec![],
            extensions: vec![],
            relationships: vec![],
            catalog: crate::bpmn_catalog::SupportingCatalog::default(),
            categories: vec![],
            event_resources: vec![],
            root_event_definitions: vec![],
            data_stores: vec![],
            processes: vec![],
            collaboration: None,
            choreographies: vec![],
            diagrams: vec![],
            provenance: None,
            capability: Capability::Editable,
        }
    }
    pub fn expressions(&self) -> Vec<&crate::bpmn_model::Expression> {
        use crate::bpmn_model::{EventDefinition, LoopCharacteristics, NodeDetails};
        let mut result = Vec::new();
        for definition in &self.root_event_definitions {
            match definition {
                EventDefinition::Conditional { expression, .. } => result.push(expression),
                EventDefinition::Timer { expression, .. } => result.extend(expression.iter()),
                _ => {}
            }
        }
        for scope in crate::queries::scopes(self) {
            for association in scope.data_associations {
                result.extend(association.transformation.iter());
                for assignment in &association.assignments {
                    result.push(&assignment.from);
                    result.push(&assignment.to);
                }
            }
        }
        let mut pending: Vec<(&[Node], &[Flow])> = self
            .processes
            .iter()
            .map(|p| (p.nodes.as_slice(), p.flows.as_slice()))
            .collect();
        while let Some((nodes, flows)) = pending.pop() {
            result.extend(flows.iter().filter_map(|f| f.condition.as_ref()));
            for node in nodes {
                if let Some(a) = &node.activity {
                    result.extend(a.completion_condition.iter());
                }
                if let Some(l) = &node.loop_characteristics {
                    match l {
                        LoopCharacteristics::Standard { condition, .. } => {
                            result.extend(condition.iter())
                        }
                        LoopCharacteristics::MultiInstance {
                            cardinality,
                            completion_condition,
                            ..
                        } => {
                            result.extend(cardinality.iter());
                            result.extend(completion_condition.iter());
                        }
                    }
                }
                match &node.details {
                    Some(NodeDetails::SubProcess { content }) => {
                        pending.push((&content.nodes, &content.flows))
                    }
                    Some(NodeDetails::Gateway {
                        activation_condition,
                        ..
                    }) => result.extend(activation_condition.iter()),
                    Some(NodeDetails::Event { definitions, .. }) => {
                        for definition in definitions {
                            match definition {
                                EventDefinition::Conditional { expression, .. } => {
                                    result.push(expression)
                                }
                                EventDefinition::Timer { expression, .. } => {
                                    result.extend(expression.iter())
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(c) = &self.collaboration {
            result.extend(c.flows.iter().filter_map(|f| f.condition.as_ref()));
        }
        for c in &self.choreographies {
            result.extend(
                c.sequence_flows
                    .iter()
                    .chain(&c.message_flows)
                    .filter_map(|f| f.condition.as_ref()),
            );
        }
        result.extend(crate::activity_read::activity_read(self).expressions);
        result
    }
    pub fn has_structured_expressions(&self) -> bool {
        self.expressions()
            .iter()
            .any(|e| matches!(e, crate::bpmn_model::Expression::Structured(_)))
    }
    pub fn has_expanded_data(&self) -> bool {
        use crate::bpmn_data::DataStoreRef;
        self.data_stores
            .iter()
            .any(|s| s.id.is_none() || s.name.is_none() || s.item_aware.is_some())
            || self.processes.iter().any(|p| p.io_specification.is_some())
            || crate::queries::scopes(self).iter().any(|scope| {
                scope
                    .data_objects
                    .iter()
                    .any(|o| o.id.is_none() || o.name.is_none() || o.item_aware.is_some())
                    || scope.data_associations.iter().any(|a| {
                        a.id.is_none() || a.transformation.is_some() || !a.assignments.is_empty()
                    })
                    || scope.nodes.iter().any(|n| {
                        n.item_aware.is_some()
                            || matches!(
                                &n.details,
                                Some(crate::bpmn_model::NodeDetails::DataStoreReference {
                                    data_store_ref: Some(DataStoreRef::Qualified(_))
                                })
                            )
                            || n.io_specification.as_ref().is_some_and(|io| {
                                io.inputs.iter().chain(&io.outputs).any(|p| {
                                    p.id.is_none()
                                        || p.name.is_none()
                                        || p.item_aware.is_some()
                                        || p.is_collection.is_some()
                                })
                            })
                    })
            })
    }
    pub fn has_expanded_events_and_di(&self) -> bool {
        use crate::bpmn_model::{
            EventDefinition as E, EventReference as R, EventResource, NodeDetails,
        };
        fn qualified(reference: &Option<R>) -> bool {
            matches!(reference, Some(R::Qualified(_)))
        }
        fn expanded(definition: &E) -> bool {
            match definition {
                E::Message {
                    message_ref,
                    operation_ref,
                    ..
                } => qualified(message_ref) || operation_ref.is_some(),
                E::Signal { signal_ref, .. } => qualified(signal_ref),
                E::Error { error_ref, .. } => qualified(error_ref),
                E::Escalation { escalation_ref, .. } => qualified(escalation_ref),
                E::Compensate { activity_ref, .. } => qualified(activity_ref),
                E::Link {
                    sources, target, ..
                } => sources.iter().any(|r| matches!(r, R::Qualified(_))) || qualified(target),
                E::Timer {
                    kind, expression, ..
                } => kind.is_none() || expression.is_none(),
                _ => false,
            }
        }
        !self.root_event_definitions.is_empty()
            || self.event_resources.iter().any(|resource| match resource {
                EventResource::Message { item_ref, .. } => item_ref.is_some(),
                EventResource::Signal { structure_ref, .. }
                | EventResource::Error { structure_ref, .. }
                | EventResource::Escalation { structure_ref, .. } => structure_ref.is_some(),
            })
            || self.diagrams.iter().any(|d| {
                d.shapes
                    .iter()
                    .any(|s| s.is_horizontal.is_some() || s.is_marker_visible.is_some())
                    || d.edges
                        .iter()
                        .any(|e| e.source_element.is_some() || e.target_element.is_some())
            })
            || crate::queries::scopes(self).iter().any(|scope| {
                scope.data_associations.iter().any(|association| {
                    scope.nodes.iter().any(|node| {
                        node.id == association.owner
                            && matches!(
                                node.kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::EndEvent
                                    | NodeKind::BoundaryEvent
                            )
                    })
                }) || scope.nodes.iter().any(|node| match &node.details {
                    Some(NodeDetails::Event {
                        definitions,
                        definition_refs,
                        parallel_multiple,
                        is_interrupting,
                        properties,
                        io,
                        ..
                    }) => {
                        definitions.len() > 1
                            || (node.kind == NodeKind::StartEvent
                                && definitions
                                    .iter()
                                    .any(|d| matches!(d, E::Error { .. } | E::Escalation { .. })))
                            || definitions.iter().any(expanded)
                            || !definition_refs.is_empty()
                            || parallel_multiple.is_some()
                            || is_interrupting.is_some()
                            || !properties.is_empty()
                            || io.is_some()
                    }
                    _ => false,
                })
            })
    }
    pub fn has_event_qname_refs(&self) -> bool {
        use crate::bpmn_model::{
            EventDefinition, EventIo, EventReference, EventResource, NodeDetails,
        };
        let item = |metadata: &Option<crate::bpmn_data::ItemAwareMetadata>| {
            metadata
                .as_ref()
                .is_some_and(|metadata| metadata.item_subject_ref.is_some())
        };
        self.root_event_definitions
            .iter()
            .any(EventDefinition::has_qualified_references)
            || self.event_resources.iter().any(|resource| match resource {
                EventResource::Message { item_ref, .. } => item_ref.is_some(),
                EventResource::Signal { structure_ref, .. }
                | EventResource::Error { structure_ref, .. }
                | EventResource::Escalation { structure_ref, .. } => structure_ref.is_some(),
            })
            || crate::queries::scopes(self).iter().any(|scope| {
                scope.nodes.iter().any(|node| match &node.details {
                    Some(NodeDetails::Event {
                        definitions,
                        definition_refs,
                        properties,
                        io,
                        ..
                    }) => {
                        definitions
                            .iter()
                            .any(EventDefinition::has_qualified_references)
                            || definition_refs
                                .iter()
                                .any(|r| matches!(r, EventReference::Qualified(_)))
                            || properties.iter().any(|p| item(&p.item_aware))
                            || io.as_ref().is_some_and(|io| {
                                match io {
                                    EventIo::Catch { outputs, .. } => outputs,
                                    EventIo::Throw { inputs, .. } => inputs,
                                }
                                .iter()
                                .any(|port| item(&port.item_aware))
                            })
                    }
                    _ => false,
                })
            })
    }
    pub fn has_data_qname_refs(&self) -> bool {
        let has_ref = |meta: &Option<crate::bpmn_data::ItemAwareMetadata>| {
            meta.as_ref().is_some_and(|m| m.item_subject_ref.is_some())
        };
        self.data_stores.iter().any(|s| has_ref(&s.item_aware))
            || self.processes.iter().any(|p| {
                p.io_specification.as_ref().is_some_and(|io| {
                    io.inputs
                        .iter()
                        .chain(&io.outputs)
                        .any(|port| has_ref(&port.item_aware))
                })
            })
            || crate::queries::scopes(self).iter().any(|scope| {
                scope.data_objects.iter().any(|o| has_ref(&o.item_aware))
                    || scope.nodes.iter().any(|n| {
                        has_ref(&n.item_aware)
                            || matches!(
                                &n.details,
                                Some(crate::bpmn_model::NodeDetails::DataStoreReference {
                                    data_store_ref: Some(_)
                                })
                            )
                            || n.io_specification.as_ref().is_some_and(|io| {
                                io.inputs
                                    .iter()
                                    .chain(&io.outputs)
                                    .any(|port| has_ref(&port.item_aware))
                            })
                    })
            })
    }
}
/// Attributes of the BPMN definitions element. Optional fields preserve XML omission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionsMetadata {
    pub target_namespace: String,
    pub name: Option<String>,
    pub expression_language: Option<String>,
    pub type_language: Option<String>,
    pub exporter: Option<String>,
    pub exporter_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<DefinitionsImport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<crate::bpmn_xml::NamespaceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_attributes: Vec<crate::bpmn_xml::XmlAttribute>,
}
/// An extension declaration on definitions; it is not a BPMN BaseElement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionsExtension {
    pub documentation: Vec<crate::bpmn_metadata::Documentation>,
    pub definition: Option<crate::bpmn_model::QNameRef>,
    pub must_understand: Option<bool>,
}
/// An inert relationship between QName-addressed model elements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionsRelationship {
    pub id: Option<crate::ids::ElementIdentity>,
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,
    pub relationship_type: String,
    pub direction: Option<RelationshipDirection>,
    pub sources: Vec<crate::bpmn_model::QNameRef>,
    pub targets: Vec<crate::bpmn_model::QNameRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum RelationshipDirection {
    None,
    Forward,
    Backward,
    Both,
}
/// A BPMN import declaration; its location is model data and is never fetched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionsImport {
    pub namespace: String,
    pub location: String,
    pub import_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub source_resource_id: XmlId,
    pub source_name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum Capability {
    Editable,
    RetainedExtensions {
        source_resource_id: XmlId,
        extensions: Vec<RetainedExtension>,
    },
    PreservedSource {
        source_resource_id: XmlId,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedExtension {
    pub owner: crate::ids::ElementSelector,
    pub position: u32,
    pub namespaces: Vec<(String, String)>,
    pub content: String,
}
impl Capability {
    pub fn allows_structural_editing(&self) -> bool {
        matches!(self, Self::Editable)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Process {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_metadata: Option<crate::bpmn_activity::ProcessActivityMetadata>,
    #[serde(
        default = "legacy_is_executable",
        skip_serializing_if = "is_legacy_executable"
    )]
    pub is_executable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_specification: Option<crate::bpmn_data::IoSpecification>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_set_id: Option<crate::ids::ElementIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_set_base: Option<crate::bpmn_metadata::BaseMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<crate::bpmn_model::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_objects: Vec<crate::bpmn_data::DataObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_associations: Vec<crate::bpmn_data::DataAssociation>,
    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub documentation: crate::bpmn_metadata::DocumentationSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_extras: Option<crate::bpmn_metadata::BaseExtras>,
    pub nodes: Vec<Node>,
    pub lanes: Vec<Lane>,
    pub flows: Vec<Flow>,
}
/// A subprocess-owned scope; its owner is the containing node, not a synthetic process.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowContainer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_set_id: Option<crate::ids::ElementIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_set_base: Option<crate::bpmn_metadata::BaseMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<crate::bpmn_model::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_objects: Vec<crate::bpmn_data::DataObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_associations: Vec<crate::bpmn_data::DataAssociation>,
    pub nodes: Vec<Node>,
    pub lanes: Vec<Lane>,
    pub flows: Vec<Flow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_aware: Option<crate::bpmn_data::ItemAwareMetadata>,
    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub documentation: crate::bpmn_metadata::DocumentationSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_extras: Option<crate::bpmn_metadata::BaseExtras>,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<crate::bpmn_model::NodeDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_characteristics: Option<crate::bpmn_model::LoopCharacteristics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_specification: Option<crate::bpmn_data::IoSpecification>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<crate::bpmn_activity::ActivityProperties>,
    pub owner: String,
    pub contact: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum NodeKind {
    Task,
    UserTask,
    ManualTask,
    ServiceTask,
    SendTask,
    ReceiveTask,
    ScriptTask,
    BusinessRuleTask,
    InclusiveGateway,
    ComplexGateway,
    EventBasedGateway,
    IntermediateCatchEvent,
    IntermediateThrowEvent,
    BoundaryEvent,
    SubProcess,
    AdHocSubProcess,
    Transaction,
    CallActivity,
    StartEvent,
    EndEvent,
    ExclusiveGateway,
    ParallelGateway,
    TextAnnotation,
    DataObjectReference,
    DataStoreReference,
}
impl NodeKind {
    pub fn is_activity(self) -> bool {
        matches!(
            self,
            Self::Task
                | Self::UserTask
                | Self::ManualTask
                | Self::ServiceTask
                | Self::SendTask
                | Self::ReceiveTask
                | Self::ScriptTask
                | Self::BusinessRuleTask
                | Self::SubProcess
                | Self::AdHocSubProcess
                | Self::Transaction
                | Self::CallActivity
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_lane_set_id: Option<crate::ids::ElementIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_lane_set_base: Option<crate::bpmn_metadata::BaseMetadata>,
    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub parent: Option<crate::ids::ElementSelector>,
    pub members: Vec<crate::ids::ElementSelector>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collaboration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correlation_keys: Vec<crate::bpmn_activity::CorrelationKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub id: crate::ids::ElementIdentity,
    pub participants: Vec<Participant>,
    pub flows: Vec<Flow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_nodes: Vec<crate::bpmn_collaboration::ConversationNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_links: Vec<crate::bpmn_collaboration::ConversationLink>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<crate::bpmn_metadata::BaseMetadata>,

    pub id: crate::ids::ElementIdentity,
    pub name: Option<String>,
    pub process: Option<crate::ids::ElementSelector>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub association_direction: Option<crate::bpmn_model::AssociationDirection>,
    pub id: crate::ids::ElementIdentity,
    pub kind: FlowKind,
    pub source: crate::ids::ElementSelector,
    pub target: crate::ids::ElementSelector,
    pub name: Option<String>,
    pub documentation: crate::bpmn_metadata::DocumentationSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_extras: Option<crate::bpmn_metadata::BaseExtras>,
    pub condition: Option<crate::bpmn_model::Expression>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum FlowKind {
    Sequence,
    Message,
    Association,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Diagram {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_styles: Vec<LabelStyle>,
    pub id: crate::ids::ElementIdentity,
    pub plane_id: crate::ids::ElementIdentity,
    pub plane: crate::ids::ElementSelector,
    pub shapes: Vec<Shape>,
    pub edges: Vec<Edge>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub font: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Label {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<crate::ids::ElementIdentity>,
    pub bounds: Option<Bounds>,
    pub style: Option<Style>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<DiStyleRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DiLabelVisibility {
    Explicit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum DiStyleRef {
    Local(crate::ids::ElementSelector),
    Qualified(crate::bpmn_model::QNameRef),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelStyle {
    pub id: Option<crate::ids::ElementIdentity>,
    pub font: Option<DiFont>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiFont {
    pub name: Option<String>,
    pub size: Option<f64>,
    pub is_bold: Option<bool>,
    pub is_italic: Option<bool>,
    pub is_underline: Option<bool>,
    pub is_strike_through: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub id: crate::ids::ElementIdentity,
    pub element: crate::ids::ElementSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_expanded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_horizontal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_marker_visible: Option<bool>,
    pub bounds: Bounds,
    pub label: Option<Label>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_visibility: Option<DiLabelVisibility>,
    pub style: Option<Style>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub id: crate::ids::ElementIdentity,
    pub element: crate::ids::ElementSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_element: Option<crate::ids::ElementSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_element: Option<crate::ids::ElementSelector>,
    pub waypoints: Vec<Point>,
    pub label: Option<Label>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_visibility: Option<DiLabelVisibility>,
    pub style: Option<Style>,
}

impl Resource {
    pub fn new(id: XmlId, media_type: String, bytes: Arc<[u8]>) -> Self {
        let content_hash = Sha256::digest(&bytes).into();
        Self {
            id,
            media_type,
            content_hash,
            bytes,
        }
    }
}
