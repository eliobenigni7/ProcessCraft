use crate::ids::{
    CheckpointId, CommentId, DictionaryEntryId, DocumentId, ElementKey, ProjectId, XmlId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const GENERATED_TARGET_NAMESPACE: &str = "urn:processcraft:generated";

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
    pub definitions_id: XmlId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definitions: Option<DefinitionsMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<crate::bpmn_model::Category>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_resources: Vec<crate::bpmn_model::EventResource>,
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
    pub fn new(name: String, definitions_id: XmlId) -> Self {
        Self {
            id: DocumentId::new(),
            name,
            folder: None,
            definitions_id,
            definitions: None,
            categories: vec![],
            event_resources: vec![],
            data_stores: vec![],
            processes: vec![],
            collaboration: None,
            choreographies: vec![],
            diagrams: vec![],
            provenance: None,
            capability: Capability::Editable,
        }
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
    pub owner: XmlId,
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
    pub lane_set_id: Option<XmlId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<crate::bpmn_model::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_objects: Vec<crate::bpmn_data::DataObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_associations: Vec<crate::bpmn_data::DataAssociation>,
    pub id: XmlId,
    pub name: String,
    pub documentation: String,
    pub nodes: Vec<Node>,
    pub lanes: Vec<Lane>,
    pub flows: Vec<Flow>,
}
/// A subprocess-owned scope; its owner is the containing node, not a synthetic process.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowContainer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_set_id: Option<XmlId>,
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
    pub id: XmlId,
    pub name: String,
    pub documentation: String,
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
    pub child_lane_set_id: Option<XmlId>,
    pub id: XmlId,
    pub name: String,
    pub parent: Option<XmlId>,
    pub members: Vec<XmlId>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collaboration {
    pub id: XmlId,
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
    pub id: XmlId,
    pub name: String,
    pub process: Option<XmlId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub association_direction: Option<crate::bpmn_model::AssociationDirection>,
    pub id: XmlId,
    pub kind: FlowKind,
    pub source: XmlId,
    pub target: XmlId,
    pub name: String,
    pub documentation: String,
    pub condition: Option<String>,
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
    pub id: XmlId,
    pub plane_id: XmlId,
    pub plane: XmlId,
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
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub bounds: Bounds,
    pub style: Option<Style>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub id: XmlId,
    pub element: XmlId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_expanded: Option<bool>,
    pub bounds: Bounds,
    pub label: Option<Label>,
    pub style: Option<Style>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub id: XmlId,
    pub element: XmlId,
    pub waypoints: Vec<Point>,
    pub label: Option<Label>,
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
