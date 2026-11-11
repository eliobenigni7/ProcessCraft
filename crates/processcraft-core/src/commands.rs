//! Atomic, toolkit-independent project mutations and session history.
use crate::{
    history::{History, HistoryEntry},
    ids::{CheckpointId, CommentId, DictionaryEntryId, DocumentId, ElementKey, XmlId},
    model::*,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub request_id: String,
    pub session_id: String,
    pub expected_generation: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum Mutation {
    SetDefinitionsMetadata {
        document_id: DocumentId,
        metadata: DefinitionsMetadata,
    },
    SetActivityProperties {
        document_id: DocumentId,
        element_id: XmlId,
        activity: Option<crate::bpmn_activity::ActivityProperties>,
    },
    CreateCategory {
        document_id: DocumentId,
        category: crate::bpmn_model::Category,
    },
    UpdateCategory {
        document_id: DocumentId,
        category: crate::bpmn_model::Category,
    },
    DeleteCategory {
        document_id: DocumentId,
        category_id: XmlId,
    },
    CreateGroup {
        document_id: DocumentId,
        scope_id: XmlId,
        group: crate::bpmn_model::Group,
    },
    UpdateGroup {
        document_id: DocumentId,
        scope_id: XmlId,
        group: crate::bpmn_model::Group,
    },
    DeleteGroup {
        document_id: DocumentId,
        scope_id: XmlId,
        group_id: XmlId,
    },
    SetAssociationDirection {
        document_id: DocumentId,
        flow_id: XmlId,
        direction: Option<crate::bpmn_model::AssociationDirection>,
    },
    SetLaneSetId {
        document_id: DocumentId,
        scope_id: XmlId,
        lane_set_id: Option<XmlId>,
    },
    SetChildLaneSetId {
        document_id: DocumentId,
        scope_id: XmlId,
        lane_id: XmlId,
        child_lane_set_id: Option<XmlId>,
    },
    SetLabelBounds {
        document_id: DocumentId,
        diagram_id: XmlId,
        representation_id: XmlId,
        bounds: Bounds,
    },
    SetNodeIoSpecification {
        document_id: DocumentId,
        element_id: XmlId,
        io_specification: Option<crate::bpmn_data::IoSpecification>,
    },
    CreateDataStore {
        document_id: DocumentId,
        data_store: crate::bpmn_data::DataStore,
    },
    UpdateDataStore {
        document_id: DocumentId,
        data_store: crate::bpmn_data::DataStore,
    },
    DeleteDataStore {
        document_id: DocumentId,
        data_store_id: XmlId,
    },
    CreateDataObject {
        document_id: DocumentId,
        scope_id: XmlId,
        data_object: crate::bpmn_data::DataObject,
    },
    UpdateDataObject {
        document_id: DocumentId,
        scope_id: XmlId,
        data_object: crate::bpmn_data::DataObject,
    },
    DeleteDataObject {
        document_id: DocumentId,
        scope_id: XmlId,
        data_object_id: XmlId,
    },
    CreateDataAssociation {
        document_id: DocumentId,
        scope_id: XmlId,
        association: crate::bpmn_data::DataAssociation,
    },
    UpdateDataAssociation {
        document_id: DocumentId,
        scope_id: XmlId,
        association: crate::bpmn_data::DataAssociation,
    },
    DeleteDataAssociation {
        document_id: DocumentId,
        scope_id: XmlId,
        association_id: XmlId,
    },
    SetDiagramPlane {
        document_id: DocumentId,
        diagram_id: XmlId,
        plane: XmlId,
    },
    CreateChoreography {
        document_id: DocumentId,
        choreography: crate::bpmn_collaboration::Choreography,
    },
    UpdateChoreography {
        document_id: DocumentId,
        choreography: crate::bpmn_collaboration::Choreography,
    },
    DeleteChoreography {
        document_id: DocumentId,
        choreography_id: XmlId,
    },
    CreateChoreographyActivity {
        document_id: DocumentId,
        choreography_id: XmlId,
        activity: crate::bpmn_collaboration::ChoreographyActivity,
    },
    UpdateChoreographyActivity {
        document_id: DocumentId,
        choreography_id: XmlId,
        activity: crate::bpmn_collaboration::ChoreographyActivity,
    },
    DeleteChoreographyActivity {
        document_id: DocumentId,
        choreography_id: XmlId,
        activity_id: XmlId,
    },
    CreateConversationNode {
        document_id: DocumentId,
        node: crate::bpmn_collaboration::ConversationNode,
    },
    UpdateConversationNode {
        document_id: DocumentId,
        node: crate::bpmn_collaboration::ConversationNode,
    },
    DeleteConversationNode {
        document_id: DocumentId,
        node_id: XmlId,
    },
    CreateConversationLink {
        document_id: DocumentId,
        link: crate::bpmn_collaboration::ConversationLink,
    },
    UpdateConversationLink {
        document_id: DocumentId,
        link: crate::bpmn_collaboration::ConversationLink,
    },
    DeleteConversationLink {
        document_id: DocumentId,
        link_id: XmlId,
    },
    SetNodeLoop {
        document_id: DocumentId,
        element_id: XmlId,
        loop_characteristics: Option<crate::bpmn_model::LoopCharacteristics>,
    },
    SetNodeDetails {
        document_id: DocumentId,
        element_id: XmlId,
        details: Option<crate::bpmn_model::NodeDetails>,
    },
    CreateCheckpoint {
        id: CheckpointId,
        name: String,
        created_unix_ms: u64,
    },
    RenameCheckpoint {
        id: CheckpointId,
        name: String,
    },
    DeleteCheckpoint {
        id: CheckpointId,
    },
    SetReferenceCheckpoint {
        id: Option<CheckpointId>,
    },
    RestoreCheckpoint {
        id: CheckpointId,
    },
    ApplyLayout {
        proposal: crate::layout::LayoutProposal,
    },
    CreateComment {
        comment: Comment,
    },
    UpdateComment {
        comment_id: CommentId,
        author: String,
        text: String,
    },
    DeleteComment {
        comment_id: CommentId,
    },
    AddResource {
        resource: Resource,
    },
    SpliceActivity {
        document_id: DocumentId,
        process_id: XmlId,
        flow_id: XmlId,
        node: Node,
        new_flow: Flow,
        diagram_id: XmlId,
        shape: Shape,
        old_waypoints: Vec<Point>,
        new_edge: Edge,
    },
    CreateFolder {
        folder: Folder,
    },
    RenameFolder {
        folder_id: XmlId,
        name: String,
    },
    MoveFolder {
        folder_id: XmlId,
        parent: Option<XmlId>,
    },
    DeleteFolder {
        folder_id: XmlId,
    },
    CreateDictionaryEntry {
        entry: DictionaryEntry,
    },
    UpdateDictionaryEntry {
        entry: DictionaryEntry,
    },
    DeleteDictionaryEntry {
        entry_id: DictionaryEntryId,
        resolution: DictionaryDeleteResolution,
    },
    SetDictionaryLinks {
        source: ElementKey,
        kind: DictionaryKind,
        targets: Vec<DictionaryTarget>,
    },

    CreateDocument {
        document: BpmnDocument,
    },
    CreateProcess {
        document_id: DocumentId,
        process: Process,
    },
    CreateCollaboration {
        document_id: DocumentId,
        collaboration: Collaboration,
    },
    CreateParticipant {
        document_id: DocumentId,
        participant: Participant,
    },
    CreateLane {
        document_id: DocumentId,
        process_id: XmlId,
        lane: Lane,
    },
    CreateElement {
        document_id: DocumentId,
        process_id: XmlId,
        element: Node,
    },
    CreateFlow {
        document_id: DocumentId,
        process_id: Option<XmlId>,
        flow: Flow,
    },
    CreateDiagram {
        document_id: DocumentId,
        diagram: Diagram,
    },
    CreateShape {
        document_id: DocumentId,
        diagram_id: XmlId,
        shape: Shape,
    },
    CreateEdge {
        document_id: DocumentId,
        diagram_id: XmlId,
        edge: Edge,
    },
    DeleteDocument {
        document_id: DocumentId,
    },
    ProjectProperties {
        name: String,
        description: String,
    },
    SetFlowCondition {
        document_id: DocumentId,
        flow_id: XmlId,
        condition: Option<crate::bpmn_model::Expression>,
    },
    SetNodeOwnerContact {
        document_id: DocumentId,
        element_id: XmlId,
        owner: String,
        contact: String,
    },
    RenameProject {
        name: String,
    },
    DocumentProperties {
        document_id: DocumentId,
        name: String,
        folder: Option<XmlId>,
    },
    Rename {
        document_id: DocumentId,
        element_id: XmlId,
        name: String,
    },
    Documentation {
        document_id: DocumentId,
        element_id: XmlId,
        documentation: String,
    },
    Delete {
        document_id: DocumentId,
        element_id: XmlId,
    },
    MoveShape {
        document_id: DocumentId,
        diagram_id: XmlId,
        shape_id: XmlId,
        bounds: Bounds,
    },
    Reroute {
        document_id: DocumentId,
        diagram_id: XmlId,
        edge_id: XmlId,
        waypoints: Vec<Point>,
    },
    AssignLane {
        document_id: DocumentId,
        process_id: XmlId,
        element_id: XmlId,
        lane_id: Option<XmlId>,
        bounds: Vec<ShapeBounds>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "type",
    content = "entry_id",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum DictionaryDeleteResolution {
    Reject,
    Replace(DictionaryEntryId),
    Unlink,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeBounds {
    pub diagram_id: XmlId,
    pub shape_id: XmlId,
    pub bounds: Bounds,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    Batch { mutations: Vec<Mutation> },
    Undo,
    Redo,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "code", content = "message", rename_all = "camelCase")]
pub enum CommandError {
    InvalidParams(String),
    GenerationConflict,
    SessionMismatch,
    CapabilityDenied,
    NothingToUndo,
    NothingToRedo,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    pub request_id: String,
    pub generation: u64,
    pub content_id: [u8; 32],
    pub changed: bool,
    pub dirty: bool,
}
#[derive(Clone)]
pub struct Snapshot {
    pub project: Arc<ProjectData>,
    pub generation: u64,
    pub content_id: [u8; 32],
    pub dirty: bool,
    pub selection: Vec<ElementKey>,
    pub undo_count: usize,
    pub redo_count: usize,
}
pub struct Session {
    id: String,
    project: Arc<ProjectData>,
    generation: u64,
    content_id: [u8; 32],
    saved: Option<[u8; 32]>,
    selection: Vec<ElementKey>,
    history: History,
}
impl Session {
    pub fn new(project: ProjectData) -> Result<Self, CommandError> {
        let content_id = project
            .content_identity()
            .map_err(CommandError::InvalidParams)?;
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            project: Arc::new(project),
            generation: 0,
            content_id,
            saved: Some(content_id),
            selection: vec![],
            history: History::default(),
        })
    }
    pub fn id(&self) -> String {
        self.id.clone()
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            generation: self.generation,
            content_id: self.content_id,
            dirty: self.saved != Some(self.content_id),
            selection: self.selection.clone(),
            undo_count: self.history.undo.len(),
            redo_count: self.history.redo.len(),
        }
    }
    pub fn set_selection(&mut self, selection: Vec<ElementKey>) {
        self.selection = selection;
    }
    pub fn mark_saved(&mut self, content_id: [u8; 32]) {
        self.saved = Some(content_id);
    }
    pub fn mark_unsaved(&mut self) {
        self.saved = None;
    }
    pub fn apply(
        &mut self,
        envelope: Envelope,
        request: Request,
    ) -> Result<ChangeSet, CommandError> {
        if envelope.session_id != self.id {
            return Err(CommandError::SessionMismatch);
        }
        if envelope
            .expected_generation
            .is_some_and(|g| g != self.generation)
        {
            return Err(CommandError::GenerationConflict);
        }
        let mut changed = true;
        match request {
            Request::Batch { mutations } => {
                if mutations.len() > 1000 {
                    return Err(CommandError::InvalidParams(
                        "batch exceeds 1000 mutations".into(),
                    ));
                }
                let requires_undo = mutations.iter().any(|m| {
                    matches!(
                        m,
                        Mutation::RestoreCheckpoint { .. } | Mutation::ApplyLayout { .. }
                    )
                });
                let mut candidate = (*self.project).clone();
                for mutation in mutations {
                    if let Mutation::ApplyLayout { proposal } = &mutation {
                        if proposal.session_id != self.id {
                            return Err(CommandError::SessionMismatch);
                        }
                        if proposal.generation != self.generation {
                            return Err(CommandError::GenerationConflict);
                        }
                    }
                    mutate(&mut candidate, mutation)?;
                }
                let id = candidate
                    .content_identity()
                    .map_err(CommandError::InvalidParams)?;
                if id == self.content_id {
                    changed = false;
                } else {
                    let entry = HistoryEntry {
                        project: self.project.clone(),
                        selection: self.selection.clone(),
                    };
                    if requires_undo {
                        let redo = HistoryEntry {
                            project: Arc::new(candidate.clone()),
                            selection: self.selection.clone(),
                        };
                        if !History::fits_single(redo) || !self.history.push_required(entry) {
                            return Err(CommandError::InvalidParams(
                                "operation cannot fit in the undo/redo history budget".into(),
                            ));
                        }
                    } else {
                        self.history.push(entry);
                    }
                    self.project = Arc::new(candidate);
                    self.selection
                        .retain(|key| selection_exists(&self.project, key));
                    self.content_id = id;
                }
            }
            Request::Undo => {
                let old = self.history.undo.pop().ok_or(CommandError::NothingToUndo)?;
                self.history.redo.push(HistoryEntry {
                    project: self.project.clone(),
                    selection: self.selection.clone(),
                });
                self.project = old.project;
                self.selection = old.selection;
                self.content_id = self
                    .project
                    .content_identity()
                    .map_err(CommandError::InvalidParams)?;
            }
            Request::Redo => {
                let old = self.history.redo.pop().ok_or(CommandError::NothingToRedo)?;
                self.history.undo.push(HistoryEntry {
                    project: self.project.clone(),
                    selection: self.selection.clone(),
                });
                self.project = old.project;
                self.selection = old.selection;
                self.content_id = self
                    .project
                    .content_identity()
                    .map_err(CommandError::InvalidParams)?;
            }
        }
        if changed {
            self.history.trim();
            self.generation += 1;
        }
        Ok(ChangeSet {
            request_id: envelope.request_id,
            generation: self.generation,
            content_id: self.content_id,
            changed,
            dirty: self.saved != Some(self.content_id),
        })
    }
}
pub(crate) fn selection_exists(p: &ProjectData, key: &ElementKey) -> bool {
    p.documents
        .iter()
        .find(|d| d.id == key.document_id)
        .is_some_and(|d| crate::identity::semantic_ids(d).contains(&key.xml_id))
}
fn move_shape(s: &mut Shape, bounds: Bounds) {
    if let Some(label) = &mut s.label {
        label.bounds.x += bounds.x - s.bounds.x;
        label.bounds.y += bounds.y - s.bounds.y;
    }
    s.bounds = bounds;
}
fn invalid(s: &str) -> CommandError {
    CommandError::InvalidParams(s.into())
}
fn has_qualified_references(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| {
        matches!(
            node.details,
            Some(crate::bpmn_model::NodeDetails::CallActivity { .. })
        ) || node.activity.as_ref().is_some_and(|activity| {
            activity.message_ref.is_some() || activity.operation_ref.is_some()
        }) || match &node.details {
            Some(crate::bpmn_model::NodeDetails::SubProcess { content }) => {
                has_qualified_references(&content.nodes)
            }
            _ => false,
        }
    })
}
fn doc(
    p: &mut ProjectData,
    id: DocumentId,
    structural: bool,
) -> Result<&mut BpmnDocument, CommandError> {
    let d = p
        .documents
        .iter_mut()
        .find(|d| d.id == id)
        .ok_or_else(|| invalid("document not found"))?;
    match &d.capability {
        Capability::PreservedSource { .. } => return Err(CommandError::CapabilityDenied),
        Capability::RetainedExtensions { .. } if structural => {
            return Err(CommandError::CapabilityDenied);
        }
        _ => {}
    }
    Ok(Arc::make_mut(d))
}
fn nested_scope_mut<'a>(
    nodes: &'a mut [Node],
    id: &XmlId,
    depth: usize,
) -> Option<&'a mut FlowContainer> {
    if depth >= 32 {
        return None;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if &node.id == id {
                return Some(content);
            }
            if let Some(found) = nested_scope_mut(&mut content.nodes, id, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}
type ScopePartsMut<'a> = (&'a mut Vec<Node>, &'a mut Vec<Lane>, &'a mut Vec<Flow>);
fn scope_parts_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &XmlId,
) -> Result<ScopePartsMut<'a>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        let p = &mut document.processes[index];
        return Ok((&mut p.nodes, &mut p.lanes, &mut p.flows));
    }
    for process in &mut document.processes {
        if let Some(c) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok((&mut c.nodes, &mut c.lanes, &mut c.flows));
        }
    }
    Err(invalid("process or subprocess not found"))
}
fn nested_node_mut<'a>(nodes: &'a mut [Node], id: &XmlId, depth: usize) -> Option<&'a mut Node> {
    if depth > 32 {
        return None;
    }
    for node in nodes {
        if &node.id == id {
            return Some(node);
        }
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details
            && let Some(found) = nested_node_mut(&mut content.nodes, id, depth + 1)
        {
            return Some(found);
        }
    }
    None
}
fn node_mut<'a>(document: &'a mut BpmnDocument, id: &XmlId) -> Result<&'a mut Node, CommandError> {
    for process in &mut document.processes {
        if let Some(node) = nested_node_mut(&mut process.nodes, id, 0) {
            return Ok(node);
        }
    }
    Err(invalid("node not found"))
}
fn edit_nested(
    nodes: &mut [Node],
    id: &XmlId,
    text: &str,
    documentation: bool,
    depth: usize,
) -> bool {
    if depth >= 32 {
        return false;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            for child in &mut content.nodes {
                if &child.id == id {
                    if documentation {
                        child.documentation = text.into();
                    } else {
                        child.name = text.into();
                    }
                    return true;
                }
            }
            if !documentation {
                for lane in &mut content.lanes {
                    if &lane.id == id {
                        lane.name = text.into();
                        return true;
                    }
                }
            }
            for flow in &mut content.flows {
                if &flow.id == id {
                    if documentation {
                        flow.documentation = text.into();
                    } else {
                        flow.name = text.into();
                    }
                    return true;
                }
            }
            if edit_nested(&mut content.nodes, id, text, documentation, depth + 1) {
                return true;
            }
        }
    }
    false
}
fn nested_flow_mut<'a>(nodes: &'a mut [Node], id: &XmlId, depth: usize) -> Option<&'a mut Flow> {
    if depth >= 32 {
        return None;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if let Some(flow) = content.flows.iter_mut().find(|f| &f.id == id) {
                return Some(flow);
            }
            if let Some(flow) = nested_flow_mut(&mut content.nodes, id, depth + 1) {
                return Some(flow);
            }
        }
    }
    None
}
fn collect_node_ids(node: &Node, ids: &mut std::collections::HashSet<XmlId>, depth: usize) {
    ids.insert(node.id.clone());
    if let Some(io) = &node.io_specification {
        ids.extend(io.inputs.iter().chain(&io.outputs).map(|p| p.id.clone()));
    }
    if depth >= 32 {
        return;
    }
    if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
        for child in &content.nodes {
            collect_node_ids(child, ids, depth + 1);
        }
        ids.extend(content.lanes.iter().map(|l| l.id.clone()));
        ids.extend(content.flows.iter().map(|f| f.id.clone()));
        ids.extend(content.groups.iter().map(|g| g.id.clone()));
        if let Some(id) = &content.lane_set_id {
            ids.insert(id.clone());
        }
        ids.extend(
            content
                .lanes
                .iter()
                .filter_map(|lane| lane.child_lane_set_id.clone()),
        );
        ids.extend(content.data_objects.iter().map(|o| o.id.clone()));
        ids.extend(content.data_associations.iter().map(|a| a.id.clone()));
    }
}
fn delete_nested(
    nodes: &mut Vec<Node>,
    lanes: &mut Vec<Lane>,
    flows: &mut Vec<Flow>,
    id: &XmlId,
    removed: &mut std::collections::HashSet<XmlId>,
    depth: usize,
) -> bool {
    if depth > 32 {
        return false;
    }
    if let Some(index) = nodes.iter().position(|n| &n.id == id) {
        collect_node_ids(&nodes[index], removed, depth);
        nodes.remove(index);
        for lane in lanes {
            lane.members.retain(|member| member != id);
        }
        return true;
    }
    if let Some(index) = lanes.iter().position(|l| &l.id == id) {
        lanes.remove(index);
        removed.insert(id.clone());
        return true;
    }
    if let Some(index) = flows.iter().position(|f| &f.id == id) {
        flows.remove(index);
        removed.insert(id.clone());
        return true;
    }
    for node in nodes {
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details
            && delete_nested(
                &mut content.nodes,
                &mut content.lanes,
                &mut content.flows,
                id,
                removed,
                depth + 1,
            )
        {
            return true;
        }
    }
    false
}
fn choreography_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &XmlId,
) -> Result<&'a mut crate::bpmn_collaboration::Choreography, CommandError> {
    document
        .choreographies
        .iter_mut()
        .find(|c| &c.id == id)
        .ok_or_else(|| invalid("choreography not found"))
}
fn collaboration_mut(document: &mut BpmnDocument) -> Result<&mut Collaboration, CommandError> {
    document
        .collaboration
        .as_mut()
        .ok_or_else(|| invalid("collaboration not found"))
}
type DataPartsMut<'a> = (
    &'a mut Vec<crate::bpmn_data::DataObject>,
    &'a mut Vec<crate::bpmn_data::DataAssociation>,
);
fn data_parts_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &XmlId,
) -> Result<DataPartsMut<'a>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        let p = &mut document.processes[index];
        return Ok((&mut p.data_objects, &mut p.data_associations));
    }
    for process in &mut document.processes {
        if let Some(c) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok((&mut c.data_objects, &mut c.data_associations));
        }
    }
    Err(invalid("process or subprocess not found"))
}
fn groups_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &XmlId,
) -> Result<&'a mut Vec<crate::bpmn_model::Group>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].groups);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.groups);
        }
    }
    Err(invalid("process or subprocess not found"))
}
fn lane_set_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &XmlId,
) -> Result<&'a mut Option<XmlId>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].lane_set_id);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.lane_set_id);
        }
    }
    Err(invalid("process or subprocess not found"))
}
fn flow_mut<'a>(d: &'a mut BpmnDocument, id: &XmlId) -> Result<&'a mut Flow, CommandError> {
    for process in &mut d.processes {
        if let Some(flow) = process.flows.iter_mut().find(|f| &f.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = nested_flow_mut(&mut process.nodes, id, 0) {
            return Ok(flow);
        }
    }
    d.collaboration
        .as_mut()
        .and_then(|c| c.flows.iter_mut().find(|f| &f.id == id))
        .ok_or_else(|| invalid("flow not found"))
}
fn diagram<'a>(d: &'a mut BpmnDocument, id: &XmlId) -> Result<&'a mut Diagram, CommandError> {
    d.diagrams
        .iter_mut()
        .find(|p| &p.id == id)
        .ok_or_else(|| invalid("diagram not found"))
}
fn mutate(p: &mut ProjectData, m: Mutation) -> Result<(), CommandError> {
    match m {
        Mutation::SetActivityProperties {
            document_id,
            element_id,
            activity,
        } => {
            let node = node_mut(doc(p, document_id, true)?, &element_id)?;
            if !node.kind.is_activity() {
                return Err(invalid("activity properties require an activity"));
            }
            node.activity = activity;
        }
        Mutation::CreateCategory {
            document_id,
            category,
        } => doc(p, document_id, true)?.categories.push(category),
        Mutation::UpdateCategory {
            document_id,
            category,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .categories
                .iter()
                .position(|v| v.id == category.id)
                .ok_or_else(|| invalid("category not found"))?;
            d.categories[index] = category;
        }
        Mutation::DeleteCategory {
            document_id,
            category_id,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .categories
                .iter()
                .position(|v| v.id == category_id)
                .ok_or_else(|| invalid("category not found"))?;
            d.categories.remove(index);
        }
        Mutation::CreateGroup {
            document_id,
            scope_id,
            group,
        } => groups_mut(doc(p, document_id, true)?, &scope_id)?.push(group),
        Mutation::UpdateGroup {
            document_id,
            scope_id,
            group,
        } => {
            let groups = groups_mut(doc(p, document_id, true)?, &scope_id)?;
            let index = groups
                .iter()
                .position(|v| v.id == group.id)
                .ok_or_else(|| invalid("group not found"))?;
            groups[index] = group;
        }
        Mutation::DeleteGroup {
            document_id,
            scope_id,
            group_id,
        } => {
            let d = doc(p, document_id, true)?;
            let groups = groups_mut(d, &scope_id)?;
            let index = groups
                .iter()
                .position(|v| v.id == group_id)
                .ok_or_else(|| invalid("group not found"))?;
            groups.remove(index);
            for diagram in &mut d.diagrams {
                diagram.shapes.retain(|s| s.element != group_id);
            }
            p.links.retain(|link| {
                !(link.source.document_id == document_id && link.source.xml_id == group_id)
            });
        }
        Mutation::SetAssociationDirection {
            document_id,
            flow_id,
            direction,
        } => {
            let flow = flow_mut(doc(p, document_id, true)?, &flow_id)?;
            if flow.kind != FlowKind::Association {
                return Err(invalid("association direction requires an association"));
            }
            flow.association_direction = direction;
        }
        Mutation::SetLaneSetId {
            document_id,
            scope_id,
            lane_set_id,
        } => *lane_set_mut(doc(p, document_id, true)?, &scope_id)? = lane_set_id,
        Mutation::SetChildLaneSetId {
            document_id,
            scope_id,
            lane_id,
            child_lane_set_id,
        } => {
            let lanes = scope_parts_mut(doc(p, document_id, true)?, &scope_id)?.1;
            lanes
                .iter_mut()
                .find(|lane| lane.id == lane_id)
                .ok_or_else(|| invalid("lane not found"))?
                .child_lane_set_id = child_lane_set_id;
        }
        Mutation::SetLabelBounds {
            document_id,
            diagram_id,
            representation_id,
            bounds,
        } => {
            let diagram = diagram(doc(p, document_id, true)?, &diagram_id)?;
            let label = if let Some(shape) = diagram
                .shapes
                .iter_mut()
                .find(|s| s.id == representation_id)
            {
                shape.label.as_mut()
            } else {
                diagram
                    .edges
                    .iter_mut()
                    .find(|e| e.id == representation_id)
                    .and_then(|e| e.label.as_mut())
            };
            label
                .ok_or_else(|| invalid("representation label not found"))?
                .bounds = bounds;
        }
        Mutation::SetNodeIoSpecification {
            document_id,
            element_id,
            io_specification,
        } => {
            node_mut(doc(p, document_id, true)?, &element_id)?.io_specification = io_specification;
        }
        Mutation::CreateDataStore {
            document_id,
            data_store,
        } => doc(p, document_id, true)?.data_stores.push(data_store),
        Mutation::UpdateDataStore {
            document_id,
            data_store,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .data_stores
                .iter()
                .position(|v| v.id == data_store.id)
                .ok_or_else(|| invalid("data store not found"))?;
            d.data_stores[index] = data_store;
        }
        Mutation::DeleteDataStore {
            document_id,
            data_store_id,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .data_stores
                .iter()
                .position(|v| v.id == data_store_id)
                .ok_or_else(|| invalid("data store not found"))?;
            d.data_stores.remove(index);
        }
        Mutation::CreateDataObject {
            document_id,
            scope_id,
            data_object,
        } => data_parts_mut(doc(p, document_id, true)?, &scope_id)?
            .0
            .push(data_object),
        Mutation::UpdateDataObject {
            document_id,
            scope_id,
            data_object,
        } => {
            let items = data_parts_mut(doc(p, document_id, true)?, &scope_id)?.0;
            let index = items
                .iter()
                .position(|v| v.id == data_object.id)
                .ok_or_else(|| invalid("data object not found"))?;
            items[index] = data_object;
        }
        Mutation::DeleteDataObject {
            document_id,
            scope_id,
            data_object_id,
        } => {
            let items = data_parts_mut(doc(p, document_id, true)?, &scope_id)?.0;
            let index = items
                .iter()
                .position(|v| v.id == data_object_id)
                .ok_or_else(|| invalid("data object not found"))?;
            items.remove(index);
        }
        Mutation::CreateDataAssociation {
            document_id,
            scope_id,
            association,
        } => data_parts_mut(doc(p, document_id, true)?, &scope_id)?
            .1
            .push(association),
        Mutation::UpdateDataAssociation {
            document_id,
            scope_id,
            association,
        } => {
            let items = data_parts_mut(doc(p, document_id, true)?, &scope_id)?.1;
            let index = items
                .iter()
                .position(|v| v.id == association.id)
                .ok_or_else(|| invalid("data association not found"))?;
            items[index] = association;
        }
        Mutation::DeleteDataAssociation {
            document_id,
            scope_id,
            association_id,
        } => {
            let items = data_parts_mut(doc(p, document_id, true)?, &scope_id)?.1;
            let index = items
                .iter()
                .position(|v| v.id == association_id)
                .ok_or_else(|| invalid("data association not found"))?;
            items.remove(index);
        }
        Mutation::SetDiagramPlane {
            document_id,
            diagram_id,
            plane,
        } => {
            diagram(doc(p, document_id, true)?, &diagram_id)?.plane = plane;
        }
        Mutation::CreateChoreography {
            document_id,
            choreography,
        } => doc(p, document_id, true)?.choreographies.push(choreography),
        Mutation::UpdateChoreography {
            document_id,
            choreography,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .choreographies
                .iter()
                .position(|c| c.id == choreography.id)
                .ok_or_else(|| invalid("choreography not found"))?;
            d.choreographies[index] = choreography;
        }
        Mutation::DeleteChoreography {
            document_id,
            choreography_id,
        } => {
            let d = doc(p, document_id, true)?;
            let index = d
                .choreographies
                .iter()
                .position(|c| c.id == choreography_id)
                .ok_or_else(|| invalid("choreography not found"))?;
            d.choreographies.remove(index);
        }
        Mutation::CreateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => choreography_mut(doc(p, document_id, true)?, &choreography_id)?
            .activities
            .push(activity),
        Mutation::UpdateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => {
            let c = choreography_mut(doc(p, document_id, true)?, &choreography_id)?;
            let index = c
                .activities
                .iter()
                .position(|a| a.id == activity.id)
                .ok_or_else(|| invalid("choreography activity not found"))?;
            c.activities[index] = activity;
        }
        Mutation::DeleteChoreographyActivity {
            document_id,
            choreography_id,
            activity_id,
        } => {
            let c = choreography_mut(doc(p, document_id, true)?, &choreography_id)?;
            let index = c
                .activities
                .iter()
                .position(|a| a.id == activity_id)
                .ok_or_else(|| invalid("choreography activity not found"))?;
            c.activities.remove(index);
        }
        Mutation::CreateConversationNode { document_id, node } => {
            collaboration_mut(doc(p, document_id, true)?)?
                .conversation_nodes
                .push(node)
        }
        Mutation::UpdateConversationNode { document_id, node } => {
            let c = collaboration_mut(doc(p, document_id, true)?)?;
            let index = c
                .conversation_nodes
                .iter()
                .position(|n| n.id == node.id)
                .ok_or_else(|| invalid("conversation node not found"))?;
            c.conversation_nodes[index] = node;
        }
        Mutation::DeleteConversationNode {
            document_id,
            node_id,
        } => {
            let d = doc(p, document_id, true)?;
            let c = collaboration_mut(d)?;
            if !c.conversation_nodes.iter().any(|n| n.id == node_id) {
                return Err(invalid("conversation node not found"));
            }
            let mut removed = std::collections::HashSet::from([node_id]);
            loop {
                let old_len = removed.len();
                for node in &c.conversation_nodes {
                    if node.parent.as_ref().is_some_and(|id| removed.contains(id)) {
                        removed.insert(node.id.clone());
                    }
                }
                if removed.len() == old_len {
                    break;
                }
            }
            c.conversation_nodes.retain(|n| !removed.contains(&n.id));
            for diagram in &mut d.diagrams {
                diagram
                    .shapes
                    .retain(|shape| !removed.contains(&shape.element));
            }
            p.links.retain(|link| {
                !(link.source.document_id == document_id && removed.contains(&link.source.xml_id))
            });
        }
        Mutation::CreateConversationLink { document_id, link } => {
            collaboration_mut(doc(p, document_id, true)?)?
                .conversation_links
                .push(link)
        }
        Mutation::UpdateConversationLink { document_id, link } => {
            let c = collaboration_mut(doc(p, document_id, true)?)?;
            let index = c
                .conversation_links
                .iter()
                .position(|l| l.id == link.id)
                .ok_or_else(|| invalid("conversation link not found"))?;
            c.conversation_links[index] = link;
        }
        Mutation::DeleteConversationLink {
            document_id,
            link_id,
        } => {
            let d = doc(p, document_id, true)?;
            let c = collaboration_mut(d)?;
            let index = c
                .conversation_links
                .iter()
                .position(|l| l.id == link_id)
                .ok_or_else(|| invalid("conversation link not found"))?;
            c.conversation_links.remove(index);
            for diagram in &mut d.diagrams {
                diagram.edges.retain(|edge| edge.element != link_id);
            }
            p.links.retain(|link| {
                !(link.source.document_id == document_id && link.source.xml_id == link_id)
            });
        }
        Mutation::SetNodeLoop {
            document_id,
            element_id,
            loop_characteristics,
        } => {
            let document = doc(p, document_id, true)?;
            node_mut(document, &element_id)?.loop_characteristics = loop_characteristics;
        }
        Mutation::SetNodeDetails {
            document_id,
            element_id,
            details,
        } => {
            let document = doc(p, document_id, true)?;
            let node = node_mut(document, &element_id)?;
            node.details = details;
        }
        Mutation::CreateCheckpoint {
            id,
            name,
            created_unix_ms,
        } => {
            if p.checkpoints.len() >= 20 {
                return Err(invalid("checkpoint limit exceeded"));
            }
            if p.checkpoints.iter().any(|c| c.id == id) {
                return Err(invalid("duplicate checkpoint ID"));
            }
            p.checkpoints.push(Checkpoint {
                id,
                name,
                created_unix_ms,
                content: FrozenProject::capture(p),
            });
        }
        Mutation::RenameCheckpoint { id, name } => {
            p.checkpoints
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or_else(|| invalid("checkpoint not found"))?
                .name = name;
        }
        Mutation::DeleteCheckpoint { id } => {
            if p.reference_checkpoint == Some(id) {
                return Err(invalid(
                    "clear the reference before deleting this checkpoint",
                ));
            }
            let index = p
                .checkpoints
                .iter()
                .position(|c| c.id == id)
                .ok_or_else(|| invalid("checkpoint not found"))?;
            p.checkpoints.remove(index);
        }
        Mutation::SetReferenceCheckpoint { id } => {
            if id.is_some_and(|id| !p.checkpoints.iter().any(|c| c.id == id)) {
                return Err(invalid("checkpoint not found"));
            }
            p.reference_checkpoint = id;
        }
        Mutation::RestoreCheckpoint { id } => {
            let checkpoint = p
                .checkpoints
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| invalid("checkpoint not found"))?;
            let mut restored = checkpoint.content.project();
            restored.checkpoints = std::mem::take(&mut p.checkpoints);
            restored.reference_checkpoint = p.reference_checkpoint;
            *p = restored;
        }
        Mutation::ApplyLayout { proposal } => {
            let document = doc(p, proposal.document_id, true)?;
            crate::layout::validate_proposal(document, &proposal)
                .map_err(|e| invalid(&e.to_string()))?;
            let diagram = diagram(document, &proposal.diagram_id)?;
            for geometry in proposal.proposed.shapes {
                let shape = diagram
                    .shapes
                    .iter_mut()
                    .find(|s| s.id == geometry.id)
                    .ok_or_else(|| invalid("shape not found"))?;
                shape.bounds = geometry.bounds;
                if let (Some(label), Some(bounds)) = (&mut shape.label, geometry.label_bounds) {
                    label.bounds = bounds;
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
                    label.bounds = bounds;
                }
            }
        }
        Mutation::CreateComment { comment } => {
            if !selection_exists(p, &comment.target) {
                return Err(invalid("comment target not found"));
            }
            if p.comments.iter().any(|c| c.id == comment.id) {
                return Err(invalid("duplicate comment ID"));
            }
            p.comments.push(comment);
        }
        Mutation::UpdateComment {
            comment_id,
            author,
            text,
        } => {
            let comment = p
                .comments
                .iter_mut()
                .find(|c| c.id == comment_id)
                .ok_or_else(|| invalid("comment not found"))?;
            comment.author = author;
            comment.text = text;
        }
        Mutation::DeleteComment { comment_id } => {
            let i = p
                .comments
                .iter()
                .position(|c| c.id == comment_id)
                .ok_or_else(|| invalid("comment not found"))?;
            p.comments.remove(i);
        }
        Mutation::AddResource { resource } => p.resources.push(resource),
        Mutation::SpliceActivity {
            document_id,
            process_id,
            flow_id,
            node,
            new_flow,
            diagram_id,
            shape,
            old_waypoints,
            new_edge,
        } => {
            if !node.kind.is_activity() {
                return Err(invalid("splice requires activity"));
            }
            let d = doc(p, document_id, true)?;
            if d.diagrams.len() != 1 || d.diagrams[0].id != diagram_id {
                return Err(invalid("splice requires exactly one diagram"));
            }
            let (nodes, _, flows) = scope_parts_mut(d, &process_id)?;
            let old = flows
                .iter()
                .find(|f| f.id == flow_id && f.kind == FlowKind::Sequence)
                .ok_or_else(|| invalid("sequence flow not found"))?
                .clone();
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
                || new_flow.source != node.id
                || new_flow.target != old.target
                || new_flow.condition.is_some()
                || shape.element != node.id
                || new_edge.element != new_flow.id
            {
                return Err(invalid("invalid splice elements"));
            }
            flows.iter_mut().find(|f| f.id == flow_id).unwrap().target = node.id.clone();
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
        }
        Mutation::CreateFolder { folder } => p.folders.push(folder),
        Mutation::RenameFolder { folder_id, name } => {
            p.folders
                .iter_mut()
                .find(|f| f.id == folder_id)
                .ok_or_else(|| invalid("folder not found"))?
                .name = name
        }
        Mutation::MoveFolder { folder_id, parent } => {
            p.folders
                .iter_mut()
                .find(|f| f.id == folder_id)
                .ok_or_else(|| invalid("folder not found"))?
                .parent = parent
        }
        Mutation::DeleteFolder { folder_id } => {
            if p.folders
                .iter()
                .any(|f| f.parent.as_ref() == Some(&folder_id))
                || p.documents
                    .iter()
                    .any(|d| d.folder.as_ref() == Some(&folder_id))
            {
                return Err(invalid("folder is not empty"));
            }
            let len = p.folders.len();
            p.folders.retain(|f| f.id != folder_id);
            if len == p.folders.len() {
                return Err(invalid("folder not found"));
            }
        }
        Mutation::CreateDictionaryEntry { entry } => p.dictionary_entries.push(entry),
        Mutation::UpdateDictionaryEntry { entry } => {
            let current = p
                .dictionary_entries
                .iter_mut()
                .find(|e| e.id == entry.id)
                .ok_or_else(|| invalid("entry not found"))?;
            if current.kind != entry.kind {
                return Err(invalid("entry kind cannot change"));
            }
            *current = entry;
        }
        Mutation::DeleteDictionaryEntry {
            entry_id,
            resolution,
        } => {
            let entry = p
                .dictionary_entries
                .iter()
                .find(|e| e.id == entry_id)
                .ok_or_else(|| invalid("entry not found"))?
                .clone();
            let used = p
                .dictionary_links
                .iter()
                .any(|l| l.target == DictionaryTarget::Entry(entry_id));
            if used {
                let replacement = match resolution {
                    DictionaryDeleteResolution::Reject => return Err(invalid("entry is in use")),
                    DictionaryDeleteResolution::Unlink => DictionaryTarget::Literal(entry.name),
                    DictionaryDeleteResolution::Replace(id) => {
                        if id == entry_id
                            || !p
                                .dictionary_entries
                                .iter()
                                .any(|e| e.id == id && e.kind == entry.kind)
                        {
                            return Err(invalid("invalid replacement"));
                        }
                        DictionaryTarget::Entry(id)
                    }
                };
                for l in &mut p.dictionary_links {
                    if l.target == DictionaryTarget::Entry(entry_id) {
                        l.target = replacement.clone();
                    }
                }
                let mut seen = std::collections::HashSet::new();
                p.dictionary_links
                    .retain(|l| seen.insert((l.source.clone(), l.kind, l.target.clone())));
            }
            p.dictionary_entries.retain(|e| e.id != entry_id);
        }
        Mutation::SetDictionaryLinks {
            source,
            kind,
            targets,
        } => {
            doc(p, source.document_id, true)?;
            p.dictionary_links
                .retain(|l| l.source != source || l.kind != kind);
            p.dictionary_links
                .extend(targets.into_iter().map(|target| DictionaryLink {
                    source: source.clone(),
                    kind,
                    target,
                }));
        }
        Mutation::CreateDocument { document } => p.documents.push(Arc::new(document)),
        Mutation::SetFlowCondition {
            document_id,
            flow_id,
            condition,
        } => {
            let d = doc(p, document_id, true)?;
            let mut found = None;
            for process in &mut d.processes {
                if let Some(flow) = process.flows.iter_mut().find(|f| f.id == flow_id) {
                    found = Some(flow);
                    break;
                }
                if let Some(flow) = nested_flow_mut(&mut process.nodes, &flow_id, 0) {
                    found = Some(flow);
                    break;
                }
            }
            let f = found
                .or_else(|| {
                    d.collaboration
                        .as_mut()
                        .and_then(|c| c.flows.iter_mut().find(|f| f.id == flow_id))
                })
                .ok_or_else(|| invalid("flow not found"))?;
            if f.kind != FlowKind::Sequence {
                return Err(invalid("condition requires sequence flow"));
            }
            f.condition = condition;
        }
        Mutation::SetNodeOwnerContact {
            document_id,
            element_id,
            owner,
            contact,
        } => {
            let d = doc(p, document_id, true)?;
            let n = node_mut(d, &element_id)?;
            n.owner = owner;
            n.contact = contact;
        }
        Mutation::RenameProject { name } => p.name = name,
        Mutation::ProjectProperties { name, description } => {
            p.name = name;
            p.description = description;
        }
        Mutation::DeleteDocument { document_id } => {
            doc(p, document_id, true)?;
            p.documents.retain(|d| d.id != document_id);
            p.dictionary_links
                .retain(|l| l.source.document_id != document_id);
            p.links
                .retain(|l| l.source.document_id != document_id && l.target != document_id);
        }
        Mutation::CreateShape {
            document_id,
            diagram_id,
            shape,
        } => diagram(doc(p, document_id, true)?, &diagram_id)?
            .shapes
            .push(shape),
        Mutation::CreateEdge {
            document_id,
            diagram_id,
            edge,
        } => diagram(doc(p, document_id, true)?, &diagram_id)?
            .edges
            .push(edge),
        Mutation::CreateProcess {
            document_id,
            process: value,
        } => doc(p, document_id, true)?.processes.push(value),
        Mutation::CreateCollaboration {
            document_id,
            collaboration,
        } => {
            let d = doc(p, document_id, true)?;
            if d.collaboration.is_some() {
                return Err(invalid("collaboration already exists"));
            }
            d.collaboration = Some(collaboration);
        }
        Mutation::CreateParticipant {
            document_id,
            participant,
        } => doc(p, document_id, true)?
            .collaboration
            .as_mut()
            .ok_or_else(|| invalid("collaboration not found"))?
            .participants
            .push(participant),
        Mutation::CreateLane {
            document_id,
            process_id,
            lane,
        } => scope_parts_mut(doc(p, document_id, true)?, &process_id)?
            .1
            .push(lane),
        Mutation::CreateElement {
            document_id,
            process_id,
            element,
        } => scope_parts_mut(doc(p, document_id, true)?, &process_id)?
            .0
            .push(element),
        Mutation::CreateFlow {
            document_id,
            process_id,
            flow,
        } => {
            let d = doc(p, document_id, true)?;
            if let Some(id) = process_id {
                scope_parts_mut(d, &id)?.2.push(flow);
            } else {
                d.collaboration
                    .as_mut()
                    .ok_or_else(|| invalid("collaboration not found"))?
                    .flows
                    .push(flow);
            }
        }
        Mutation::CreateDiagram {
            document_id,
            diagram,
        } => doc(p, document_id, true)?.diagrams.push(diagram),
        Mutation::DocumentProperties {
            document_id,
            name,
            folder,
        } => {
            let d = doc(p, document_id, false)?;
            if matches!(d.capability, Capability::RetainedExtensions { .. }) && d.folder != folder {
                return Err(CommandError::CapabilityDenied);
            }
            d.name = name;
            d.folder = folder;
        }
        Mutation::SetDefinitionsMetadata {
            document_id,
            metadata,
        } => {
            let d = doc(p, document_id, true)?;
            if metadata.target_namespace.is_empty() {
                return Err(invalid(
                    "Empty target namespace is outside the qualified editable profile",
                ));
            }
            if d.definitions.is_none() && d.provenance.is_some() {
                return Err(invalid(
                    "Original target namespace must be recovered before editing definitions metadata",
                ));
            }
            let namespace_changed = d
                .definitions
                .as_ref()
                .map(|old| old.target_namespace.as_str())
                .unwrap_or(GENERATED_TARGET_NAMESPACE)
                != metadata.target_namespace.as_str();
            if namespace_changed
                && (d
                    .processes
                    .iter()
                    .any(|p| has_qualified_references(&p.nodes))
                    || d.expressions().iter().any(|expression| {
                        matches!(
                            expression,
                            crate::bpmn_model::Expression::Structured(
                                crate::bpmn_model::ExpressionData {
                                    kind: crate::bpmn_model::ExpressionKind::Formal {
                                        evaluates_to_type_ref: Some(_),
                                        ..
                                    },
                                    ..
                                }
                            )
                        )
                    }))
            {
                return Err(invalid(
                    "Cannot change an unknown or referenced target namespace",
                ));
            }
            d.definitions = Some(metadata);
        }
        Mutation::Rename {
            document_id,
            element_id,
            name,
        } => {
            let d = doc(p, document_id, false)?;
            let mut found = false;
            for pr in &mut d.processes {
                if pr.id == element_id {
                    pr.name = name.clone();
                    found = true;
                }
                found |= edit_nested(&mut pr.nodes, &element_id, &name, false, 0);
                for n in &mut pr.nodes {
                    if n.id == element_id {
                        n.name = name.clone();
                        found = true;
                    }
                }
                for l in &mut pr.lanes {
                    if l.id == element_id {
                        l.name = name.clone();
                        found = true;
                    }
                }
                for f in &mut pr.flows {
                    if f.id == element_id {
                        f.name = name.clone();
                        found = true;
                    }
                }
            }
            if let Some(c) = &mut d.collaboration {
                for part in &mut c.participants {
                    if part.id == element_id {
                        part.name = name.clone();
                        found = true;
                    }
                }
                for f in &mut c.flows {
                    if f.id == element_id {
                        f.name = name.clone();
                        found = true;
                    }
                }
            }
            if !found {
                return Err(invalid("element not found"));
            }
        }
        Mutation::Documentation {
            document_id,
            element_id,
            documentation,
        } => {
            let d = doc(p, document_id, false)?;
            let mut found = false;
            for pr in &mut d.processes {
                if pr.id == element_id {
                    pr.documentation = documentation.clone();
                    found = true;
                }
                found |= edit_nested(&mut pr.nodes, &element_id, &documentation, true, 0);
                for n in &mut pr.nodes {
                    if n.id == element_id {
                        n.documentation = documentation.clone();
                        found = true;
                    }
                }
                for f in &mut pr.flows {
                    if f.id == element_id {
                        f.documentation = documentation.clone();
                        found = true;
                    }
                }
            }
            if let Some(c) = &mut d.collaboration {
                for f in &mut c.flows {
                    if f.id == element_id {
                        f.documentation = documentation.clone();
                        found = true;
                    }
                }
            }
            if !found {
                return Err(invalid("element not found"));
            }
        }
        Mutation::MoveShape {
            document_id,
            diagram_id,
            shape_id,
            bounds,
        } => {
            let s = diagram(doc(p, document_id, false)?, &diagram_id)?
                .shapes
                .iter_mut()
                .find(|s| s.id == shape_id)
                .ok_or_else(|| invalid("shape not found"))?;
            move_shape(s, bounds);
        }
        Mutation::Reroute {
            document_id,
            diagram_id,
            edge_id,
            waypoints,
        } => {
            let e = diagram(doc(p, document_id, false)?, &diagram_id)?
                .edges
                .iter_mut()
                .find(|s| s.id == edge_id)
                .ok_or_else(|| invalid("edge not found"))?;
            e.waypoints = waypoints;
        }
        Mutation::AssignLane {
            document_id,
            process_id,
            element_id,
            lane_id,
            bounds,
        } => {
            let d = doc(p, document_id, true)?;
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
                .map(|l| &l.id);
            if current != lane_id.as_ref() {
                for l in lanes {
                    l.members.retain(|id| id != &element_id);
                    if Some(&l.id) == lane_id.as_ref() {
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
        }
        Mutation::Delete {
            document_id,
            element_id,
        } => {
            let d = doc(p, document_id, true)?;
            let mut removed = std::collections::HashSet::new();
            let mut found = false;
            for pr in &mut d.processes {
                found |= delete_nested(
                    &mut pr.nodes,
                    &mut pr.lanes,
                    &mut pr.flows,
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
            }
            if !found {
                return Err(invalid("element not found or cannot be deleted"));
            }
            for dg in &mut d.diagrams {
                dg.shapes.retain(|s| !removed.contains(&s.element));
                dg.edges.retain(|e| !removed.contains(&e.element));
            }
            p.dictionary_links.retain(|l| {
                !(l.source.document_id == document_id && removed.contains(&l.source.xml_id))
            });
            p.links.retain(|l| {
                !(l.source.document_id == document_id && removed.contains(&l.source.xml_id))
            });
        }
    }
    Ok(())
}

/// Stable discovery metadata for the type-tagged command protocol.
#[derive(Debug, Serialize)]
pub struct CommandDescriptor {
    pub id: &'static str,
    pub serde_tag: &'static str,
    pub label: &'static str,
    pub params: &'static [CommandParam],
}
#[derive(Debug, Serialize)]
pub struct CommandParam {
    pub name: &'static str,
    pub value_type: &'static str,
    pub required: bool,
}
macro_rules! descriptor { ($id:literal,$label:literal $(,$name:literal:$ty:literal)* $(,)?) => { CommandDescriptor { id: $id, serde_tag: $id, label: $label, params: &[$(CommandParam {name:$name,value_type:$ty,required:true}),*] } }; }
static COMMAND_DESCRIPTORS: &[CommandDescriptor] = &[
    descriptor!("setDefinitionsMetadata", "Set definitions metadata", "document_id":"DocumentId", "metadata":"DefinitionsMetadata"),
    descriptor!("setActivityProperties", "Set activity properties", "document_id":"DocumentId", "element_id":"XmlId", "activity":"ActivityProperties|null"),
    descriptor!("createCategory","Create category","document_id":"DocumentId","category":"Category"),
    descriptor!("updateCategory","Update category","document_id":"DocumentId","category":"Category"),
    descriptor!("deleteCategory","Delete category","document_id":"DocumentId","category_id":"XmlId"),
    descriptor!("createGroup","Create group","document_id":"DocumentId","scope_id":"XmlId","group":"Group"),
    descriptor!("updateGroup","Update group","document_id":"DocumentId","scope_id":"XmlId","group":"Group"),
    descriptor!("deleteGroup","Delete group","document_id":"DocumentId","scope_id":"XmlId","group_id":"XmlId"),
    descriptor!("setAssociationDirection","Set association direction","document_id":"DocumentId","flow_id":"XmlId","direction":"AssociationDirection|null"),
    descriptor!("setLaneSetId","Set lane set ID","document_id":"DocumentId","scope_id":"XmlId","lane_set_id":"XmlId|null"),
    descriptor!("setChildLaneSetId","Set child lane set ID","document_id":"DocumentId","scope_id":"XmlId","lane_id":"XmlId","child_lane_set_id":"XmlId|null"),
    descriptor!("setLabelBounds", "Set label bounds", "document_id":"DocumentId", "diagram_id":"XmlId", "representation_id":"XmlId", "bounds":"Bounds"),
    descriptor!("setNodeIoSpecification", "Set activity input/output", "document_id":"DocumentId", "element_id":"XmlId", "io_specification":"IoSpecification|null"),
    descriptor!("createDataStore","Create data store","document_id":"DocumentId","data_store":"DataStore"),
    descriptor!("updateDataStore","Update data store","document_id":"DocumentId","data_store":"DataStore"),
    descriptor!("deleteDataStore","Delete data store","document_id":"DocumentId","data_store_id":"XmlId"),
    descriptor!("createDataObject","Create data object","document_id":"DocumentId","scope_id":"XmlId","data_object":"DataObject"),
    descriptor!("updateDataObject","Update data object","document_id":"DocumentId","scope_id":"XmlId","data_object":"DataObject"),
    descriptor!("deleteDataObject","Delete data object","document_id":"DocumentId","scope_id":"XmlId","data_object_id":"XmlId"),
    descriptor!("createDataAssociation","Create data association","document_id":"DocumentId","scope_id":"XmlId","association":"DataAssociation"),
    descriptor!("updateDataAssociation","Update data association","document_id":"DocumentId","scope_id":"XmlId","association":"DataAssociation"),
    descriptor!("deleteDataAssociation","Delete data association","document_id":"DocumentId","scope_id":"XmlId","association_id":"XmlId"),
    descriptor!("setDiagramPlane", "Set diagram plane", "document_id":"DocumentId", "diagram_id":"XmlId", "plane":"XmlId"),
    descriptor!("createChoreography", "Create choreography", "document_id":"DocumentId", "choreography":"Choreography"),
    descriptor!("updateChoreography", "Update choreography", "document_id":"DocumentId", "choreography":"Choreography"),
    descriptor!("deleteChoreography", "Delete choreography", "document_id":"DocumentId", "choreography_id":"XmlId"),
    descriptor!("createChoreographyActivity", "Create choreography task", "document_id":"DocumentId", "choreography_id":"XmlId", "activity":"ChoreographyActivity"),
    descriptor!("updateChoreographyActivity", "Update choreography task", "document_id":"DocumentId", "choreography_id":"XmlId", "activity":"ChoreographyActivity"),
    descriptor!("deleteChoreographyActivity", "Delete choreography task", "document_id":"DocumentId", "choreography_id":"XmlId", "activity_id":"XmlId"),
    descriptor!("createConversationNode", "Create conversation", "document_id":"DocumentId", "node":"ConversationNode"),
    descriptor!("updateConversationNode", "Update conversation", "document_id":"DocumentId", "node":"ConversationNode"),
    descriptor!("deleteConversationNode", "Delete conversation", "document_id":"DocumentId", "node_id":"XmlId"),
    descriptor!("createConversationLink", "Create conversation link", "document_id":"DocumentId", "link":"ConversationLink"),
    descriptor!("updateConversationLink", "Update conversation link", "document_id":"DocumentId", "link":"ConversationLink"),
    descriptor!("deleteConversationLink", "Delete conversation link", "document_id":"DocumentId", "link_id":"XmlId"),
    descriptor!("applyLayout", "Apply layout", "proposal":"LayoutProposal"),
    descriptor!("createComment", "Create comment", "comment":"Comment"),
    descriptor!("updateComment", "Update comment", "comment_id":"CommentId", "author":"string", "text":"string"),
    descriptor!("deleteComment", "Delete comment", "comment_id":"CommentId"),
    descriptor!("createCheckpoint", "Create checkpoint", "id":"CheckpointId", "name":"string", "created_unix_ms":"u64"),
    descriptor!("renameCheckpoint", "Rename checkpoint", "id":"CheckpointId", "name":"string"),
    descriptor!("deleteCheckpoint", "Delete checkpoint", "id":"CheckpointId"),
    descriptor!("setReferenceCheckpoint", "Set reference checkpoint", "id":"CheckpointId|null"),
    descriptor!("restoreCheckpoint", "Restore checkpoint", "id":"CheckpointId"),
    descriptor!("setNodeLoop", "Set node loop characteristics", "document_id":"DocumentId", "element_id":"XmlId", "loop_characteristics":"LoopCharacteristics|null"),
    descriptor!("setNodeDetails", "Set node details", "document_id":"DocumentId", "element_id":"XmlId", "details":"NodeDetails|null"),
    descriptor!("addResource","Add resource","resource":"Resource"),
    descriptor!("spliceActivity","Splice activity","document_id":"DocumentId","process_id":"XmlId","flow_id":"XmlId","node":"Node","new_flow":"Flow","diagram_id":"XmlId","shape":"Shape","old_waypoints":"Point[]","new_edge":"Edge"),
    descriptor!("createFolder","Create folder","folder":"Folder"),
    descriptor!("renameFolder","Rename folder","folder_id":"XmlId","name":"string"),
    descriptor!("moveFolder","Move folder","folder_id":"XmlId","parent":"XmlId|null"),
    descriptor!("deleteFolder","Delete folder","folder_id":"XmlId"),
    descriptor!("createDictionaryEntry","Create entry","entry":"DictionaryEntry"),
    descriptor!("updateDictionaryEntry","Update entry","entry":"DictionaryEntry"),
    descriptor!("deleteDictionaryEntry","Delete entry","entry_id":"DictionaryEntryId","resolution":"DictionaryDeleteResolution"),
    descriptor!("setDictionaryLinks","Set links","source":"ElementKey","kind":"DictionaryKind","targets":"DictionaryTarget[]"),
    descriptor!("batch","Apply batch","mutations":"Mutation[]"),
    descriptor!("undo", "Undo"),
    descriptor!("redo", "Redo"),
    descriptor!("createDocument","Create document","document":"BpmnDocument"),
    descriptor!("createProcess","Create process","document_id":"DocumentId","process":"Process"),
    descriptor!("createCollaboration","Create collaboration","document_id":"DocumentId","collaboration":"Collaboration"),
    descriptor!("createParticipant","Create participant","document_id":"DocumentId","participant":"Participant"),
    descriptor!("createLane","Create lane","document_id":"DocumentId","process_id":"XmlId","lane":"Lane"),
    descriptor!("createElement","Create element","document_id":"DocumentId","process_id":"XmlId","element":"Node"),
    descriptor!("createFlow","Create flow","document_id":"DocumentId","process_id":"XmlId|null","flow":"Flow"),
    descriptor!("createDiagram","Create diagram","document_id":"DocumentId","diagram":"Diagram"),
    descriptor!("createShape","Create shape","document_id":"DocumentId","diagram_id":"XmlId","shape":"Shape"),
    descriptor!("createEdge","Create edge","document_id":"DocumentId","diagram_id":"XmlId","edge":"Edge"),
    descriptor!("deleteDocument","Delete document","document_id":"DocumentId"),
    descriptor!("projectProperties","Project properties","name":"string","description":"string"),
    descriptor!("renameProject","Rename project","name":"string"),
    descriptor!("documentProperties","Document properties","document_id":"DocumentId","name":"string","folder":"XmlId|null"),
    descriptor!("rename","Rename","document_id":"DocumentId","element_id":"XmlId","name":"string"),
    descriptor!("documentation","Set documentation","document_id":"DocumentId","element_id":"XmlId","documentation":"string"),
    descriptor!("delete","Delete element","document_id":"DocumentId","element_id":"XmlId"),
    descriptor!("moveShape","Move shape","document_id":"DocumentId","diagram_id":"XmlId","shape_id":"XmlId","bounds":"Bounds"),
    descriptor!("reroute","Reroute edge","document_id":"DocumentId","diagram_id":"XmlId","edge_id":"XmlId","waypoints":"Point[]"),
    descriptor!("assignLane","Assign lane","document_id":"DocumentId","process_id":"XmlId","element_id":"XmlId","lane_id":"XmlId|null","bounds":"ShapeBounds[]"),
    descriptor!("setFlowCondition","Set flow condition","document_id":"DocumentId","flow_id":"XmlId","condition":"string|null"),
    descriptor!("setNodeOwnerContact","Set node owner and contact","document_id":"DocumentId","element_id":"XmlId","owner":"string","contact":"string"),
];
pub fn command_descriptors() -> &'static [CommandDescriptor] {
    COMMAND_DESCRIPTORS
}

/// JSON Schema generated directly from the shared mutation protocol.
pub fn request_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Request)).expect("schema serializes")
}

#[cfg(test)]
mod shape_label_tests {
    use super::*;
    #[test]
    fn moving_shape_translates_label_without_resizing_it() {
        let mut shape = Shape {
            is_expanded: None,
            id: "Shape".parse().unwrap(),
            element: "Node".parse().unwrap(),
            bounds: Bounds {
                x: 10.0,
                y: 20.0,
                width: 40.0,
                height: 40.0,
            },
            label: Some(Label {
                bounds: Bounds {
                    x: 0.0,
                    y: 65.0,
                    width: 80.0,
                    height: 30.0,
                },
                style: None,
            }),
            style: None,
        };
        move_shape(
            &mut shape,
            Bounds {
                x: 25.0,
                y: 12.0,
                width: 40.0,
                height: 40.0,
            },
        );
        let label = shape.label.unwrap().bounds;
        assert_eq!(
            (label.x, label.y, label.width, label.height),
            (15.0, 57.0, 80.0, 30.0)
        );
    }
}
