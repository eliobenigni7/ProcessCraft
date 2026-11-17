//! Atomic, toolkit-independent project mutations and session history.
//!
//! A `Request::Batch` is applied to a candidate copy of the project (copy-on-write); after the
//! post-batch checks and structural validation pass, the content identity is recomputed and the
//! candidate replaces the state as one undo step. On any failure state, history and generation
//! are unchanged. Generation (monotonic, for conflict checks and stale-result discarding),
//! content identity (SHA-256 of persistent content, drives `dirty`) and checkpoints
//! (persistent revisions) are independent notions.
use crate::{
    history::{History, HistoryEntry},
    ids::{
        CheckpointId, CommentId, DictionaryEntryId, DocumentId, ElementKey, ElementSelector, XmlId,
    },
    model::*,
};
mod access;
mod canvas;
mod choreography;
mod collaboration;
mod data;
mod definitions;
mod diagram;
mod dictionary;
mod documentation;
mod groups;
mod post_batch;
mod process;
mod references;
mod rename;
mod review;
use self::access::*;
pub(crate) use self::access::{node_mut, scope_parts_mut};
use self::choreography::*;
use self::collaboration::*;
use self::diagram::*;
use self::post_batch::*;
use self::references::*;
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
    SetCollaborationRoot {
        document_id: DocumentId,
        collaboration: Collaboration,
    },
    DeleteCollaborationRoot {
        document_id: DocumentId,
        collaboration_id: ElementSelector,
    },
    SetPartnerCatalog {
        document_id: DocumentId,
        partner_entities: Vec<crate::bpmn_collaboration::PartnerEntity>,
        partner_roles: Vec<crate::bpmn_collaboration::PartnerRole>,
        endpoints: Vec<crate::bpmn_collaboration::EndPoint>,
    },
    SetDocumentRootOrder {
        document_id: DocumentId,
        order: Option<Vec<ElementSelector>>,
    },
    InitializeDocumentRootOrder {
        document_id: DocumentId,
    },
    SetDiagramPlaneMetadata {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        metadata: Option<DiElementMetadata>,
    },
    SetDiagramPlaneOrder {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        order: Option<Vec<ElementSelector>>,
    },
    SetShapeRepresentationMetadata {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
        metadata: Option<DiElementMetadata>,
        is_message_visible: Option<bool>,
        participant_band_kind: Option<ParticipantBandKind>,
        choreography_activity_shape: Option<DiReference>,
    },
    SetEdgeRepresentationMetadata {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        edge_id: ElementSelector,
        metadata: Option<DiElementMetadata>,
        message_visible_kind: Option<MessageVisibleKind>,
    },
    SetParticipantMultiplicity {
        document_id: DocumentId,
        participant_id: ElementSelector,
        multiplicity: Option<ParticipantMultiplicity>,
    },
    SetMessageFlowReference {
        document_id: DocumentId,
        flow_id: ElementSelector,
        message_ref: Option<crate::bpmn_activity::ModelReference>,
    },
    SetGlobalTasks {
        document_id: DocumentId,
        global_tasks: Vec<crate::bpmn_activity::GlobalTask>,
    },
    SetCorrelationProperties {
        document_id: DocumentId,
        correlation_properties: Vec<crate::bpmn_activity::CorrelationProperty>,
    },
    SetExpression {
        document_id: DocumentId,
        expression_id: ElementSelector,
        expression: crate::bpmn_model::Expression,
    },
    SetDefinitionsExtensions {
        document_id: DocumentId,
        extensions: Vec<DefinitionsExtension>,
    },
    SetDefinitionsRelationships {
        document_id: DocumentId,
        relationships: Vec<DefinitionsRelationship>,
    },
    SetDiagramMetadata {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        documentation: Option<String>,
        resolution: Option<f64>,
    },
    SetDiagramLabelStyles {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        label_styles: Vec<LabelStyle>,
    },
    SetShapeLabel {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
        label: Option<Label>,
        label_visibility: Option<DiLabelVisibility>,
    },
    SetEdgeLabel {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        edge_id: ElementSelector,
        label: Option<Label>,
        label_visibility: Option<DiLabelVisibility>,
    },
    SetDocumentationEntry {
        document_id: DocumentId,
        documentation_id: ElementSelector,
        documentation: crate::bpmn_metadata::Documentation,
    },
    SetBaseMetadata {
        document_id: DocumentId,
        element_id: ElementSelector,
        metadata: Option<crate::bpmn_metadata::BaseMetadata>,
    },
    SetBaseExtras {
        document_id: DocumentId,
        element_id: ElementSelector,
        extras: Option<crate::bpmn_metadata::BaseExtras>,
    },
    ReplaceDocumentation {
        document_id: DocumentId,
        element_id: ElementSelector,
        documentation: crate::bpmn_metadata::DocumentationSet,
    },
    SetAnnotationBody {
        document_id: DocumentId,
        element_id: ElementSelector,
        body: Option<crate::bpmn_xml::MixedXml>,
    },
    SetScriptBody {
        document_id: DocumentId,
        element_id: ElementSelector,
        body: Option<crate::bpmn_xml::MixedXml>,
    },
    ReplaceFlowCondition {
        document_id: DocumentId,
        flow_id: ElementSelector,
        condition: Option<crate::bpmn_model::Expression>,
    },
    SetEventResources {
        document_id: DocumentId,
        resources: Vec<crate::bpmn_model::EventResource>,
    },
    SetRootEventDefinitions {
        document_id: DocumentId,
        definitions: Vec<crate::bpmn_model::EventDefinition>,
    },
    SetShapeDiAttributes {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
        is_horizontal: Option<bool>,
        is_marker_visible: Option<bool>,
    },
    SetEdgeDiEndpoints {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        edge_id: ElementSelector,
        source_element: Option<ElementSelector>,
        target_element: Option<ElementSelector>,
    },
    SetElementXmlId {
        document_id: DocumentId,
        element_id: ElementSelector,
        xml_id: Option<XmlId>,
    },
    PromoteElementIdentity {
        document_id: DocumentId,
        model_path: String,
        xml_id: Option<XmlId>,
    },
    SetTextAnnotation {
        document_id: DocumentId,
        element_id: ElementSelector,
        text: Option<String>,
    },
    SetProcessExecutable {
        document_id: DocumentId,
        process_id: ElementSelector,
        is_executable: Option<bool>,
    },
    SetProcessActivityMetadata {
        document_id: DocumentId,
        process_id: ElementSelector,
        metadata: Option<crate::bpmn_activity::ProcessActivityMetadata>,
    },
    SetNodeFlowMetadata {
        document_id: DocumentId,
        element_id: ElementSelector,
        metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
    },
    SetFlowMetadata {
        document_id: DocumentId,
        element_id: ElementSelector,
        metadata: Option<crate::bpmn_activity::FlowElementMetadata>,
    },
    SetCollaborationName {
        document_id: DocumentId,
        name: Option<String>,
    },
    SetCollaborationCorrelationKeys {
        document_id: DocumentId,
        correlation_keys: Vec<crate::bpmn_activity::CorrelationKey>,
    },
    SetDiagramName {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        name: Option<String>,
    },
    SetNodeItemAware {
        document_id: DocumentId,
        element_id: ElementSelector,
        item_aware: Option<crate::bpmn_data::ItemAwareMetadata>,
    },
    SetProcessIoSpecification {
        document_id: DocumentId,
        process_id: ElementSelector,
        io_specification: Option<crate::bpmn_data::IoSpecification>,
    },
    SetDataStores {
        document_id: DocumentId,
        data_stores: Vec<crate::bpmn_data::DataStore>,
    },
    SetScopeDataObjects {
        document_id: DocumentId,
        scope_id: ElementSelector,
        data_objects: Vec<crate::bpmn_data::DataObject>,
    },
    SetScopeDataAssociations {
        document_id: DocumentId,
        scope_id: ElementSelector,
        data_associations: Vec<crate::bpmn_data::DataAssociation>,
    },
    SetSupportingCatalog {
        document_id: DocumentId,
        catalog: crate::bpmn_catalog::SupportingCatalog,
    },
    SetDefinitionsMetadata {
        document_id: DocumentId,
        metadata: DefinitionsMetadata,
    },
    SetActivityProperties {
        document_id: DocumentId,
        element_id: ElementSelector,
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
        category_id: ElementSelector,
    },
    CreateGroup {
        document_id: DocumentId,
        scope_id: ElementSelector,
        group: crate::bpmn_model::Group,
    },
    UpdateGroup {
        document_id: DocumentId,
        scope_id: ElementSelector,
        group: crate::bpmn_model::Group,
    },
    DeleteGroup {
        document_id: DocumentId,
        scope_id: ElementSelector,
        group_id: ElementSelector,
    },
    SetAssociationDirection {
        document_id: DocumentId,
        flow_id: ElementSelector,
        direction: Option<crate::bpmn_model::AssociationDirection>,
    },
    SetLaneSetId {
        document_id: DocumentId,
        scope_id: ElementSelector,
        lane_set_id: Option<crate::ids::ElementIdentity>,
    },
    SetChildLaneSetId {
        document_id: DocumentId,
        scope_id: ElementSelector,
        lane_id: ElementSelector,
        child_lane_set_id: Option<crate::ids::ElementIdentity>,
    },
    SetLaneSetName {
        document_id: DocumentId,
        scope_id: ElementSelector,
        lane_set_name: Option<String>,
    },
    SetChildLaneSetName {
        document_id: DocumentId,
        scope_id: ElementSelector,
        lane_id: ElementSelector,
        child_lane_set_name: Option<String>,
    },
    SetLanePartition {
        document_id: DocumentId,
        scope_id: ElementSelector,
        lane_id: ElementSelector,
        partition_element: Option<crate::model::LanePartitionElement>,
        partition_element_ref: Option<crate::bpmn_activity::ModelReference>,
    },
    SetLabelBounds {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        representation_id: ElementSelector,
        bounds: Bounds,
    },
    SetNodeIoSpecification {
        document_id: DocumentId,
        element_id: ElementSelector,
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
        data_store_id: ElementSelector,
    },
    CreateDataObject {
        document_id: DocumentId,
        scope_id: ElementSelector,
        data_object: crate::bpmn_data::DataObject,
    },
    UpdateDataObject {
        document_id: DocumentId,
        scope_id: ElementSelector,
        data_object: crate::bpmn_data::DataObject,
    },
    DeleteDataObject {
        document_id: DocumentId,
        scope_id: ElementSelector,
        data_object_id: ElementSelector,
    },
    CreateDataAssociation {
        document_id: DocumentId,
        scope_id: ElementSelector,
        association: crate::bpmn_data::DataAssociation,
    },
    UpdateDataAssociation {
        document_id: DocumentId,
        scope_id: ElementSelector,
        association: crate::bpmn_data::DataAssociation,
    },
    DeleteDataAssociation {
        document_id: DocumentId,
        scope_id: ElementSelector,
        association_id: ElementSelector,
    },
    SetDiagramPlane {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        plane: ElementSelector,
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
        choreography_id: ElementSelector,
    },
    PromoteChoreographyGraph {
        document_id: DocumentId,
        choreography_id: ElementSelector,
    },
    CreateChoreographyFlowElement {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        index: usize,
        element: crate::bpmn_collaboration::ChoreographyFlowElement,
    },
    UpdateChoreographyFlowElement {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        element: crate::bpmn_collaboration::ChoreographyFlowElement,
    },
    DeleteChoreographyFlowElement {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        element_id: ElementSelector,
    },
    ReorderChoreographyFlowElement {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        element_id: ElementSelector,
        index: usize,
    },
    ReparentChoreographyFlowElement {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        source_scope_id: ElementSelector,
        target_scope_id: ElementSelector,
        element_id: ElementSelector,
        index: usize,
    },
    CreateChoreographyArtifact {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        index: usize,
        artifact: crate::bpmn_collaboration::CollaborationArtifact,
    },
    UpdateChoreographyArtifact {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        artifact: crate::bpmn_collaboration::CollaborationArtifact,
    },
    DeleteChoreographyArtifact {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        artifact_id: ElementSelector,
    },
    ReorderChoreographyArtifact {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        artifact_id: ElementSelector,
        index: usize,
    },
    ReparentChoreographyArtifact {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        source_scope_id: ElementSelector,
        target_scope_id: ElementSelector,
        artifact_id: ElementSelector,
        index: usize,
    },
    CreateChoreographyDataAssociation {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        index: usize,
        association: crate::bpmn_data::DataAssociation,
    },
    UpdateChoreographyDataAssociation {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        association: crate::bpmn_data::DataAssociation,
    },
    DeleteChoreographyDataAssociation {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        association_id: ElementSelector,
    },
    ReorderChoreographyDataAssociation {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        scope_id: ElementSelector,
        association_id: ElementSelector,
        index: usize,
    },
    CreateChoreographyActivity {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        activity: crate::bpmn_collaboration::ChoreographyActivity,
    },
    UpdateChoreographyActivity {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        activity: crate::bpmn_collaboration::ChoreographyActivity,
    },
    DeleteChoreographyActivity {
        document_id: DocumentId,
        choreography_id: ElementSelector,
        activity_id: ElementSelector,
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
        node_id: ElementSelector,
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
        link_id: ElementSelector,
    },
    SetNodeLoop {
        document_id: DocumentId,
        element_id: ElementSelector,
        loop_characteristics: Option<crate::bpmn_model::LoopCharacteristics>,
    },
    SetNodeDetails {
        document_id: DocumentId,
        element_id: ElementSelector,
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
        process_id: ElementSelector,
        flow_id: ElementSelector,
        node: Node,
        new_flow: Flow,
        diagram_id: ElementSelector,
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
    CreateAttributeDefinition {
        definition: crate::attributes::AttributeDefinition,
    },
    UpdateAttributeDefinition {
        definition: crate::attributes::AttributeDefinition,
        resolution: crate::attributes::AttributeUpdateResolution,
    },
    DeleteAttributeDefinition {
        definition_id: crate::ids::AttributeDefinitionId,
        resolution: crate::attributes::AttributeDeleteResolution,
    },
    SetAttributeValues {
        source: ElementKey,
        values: Vec<crate::attributes::AttributeValueInput>,
    },
    /// Whole-value setter of the modeling conventions profile; `None` switches every rule off.
    SetConventionsProfile {
        profile: Option<crate::conventions::ConventionsProfile>,
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
        process_id: ElementSelector,
        lane: Lane,
    },
    CreateElement {
        document_id: DocumentId,
        process_id: ElementSelector,
        element: Node,
    },
    CreateFlow {
        document_id: DocumentId,
        process_id: Option<ElementSelector>,
        flow: Flow,
    },
    CreateDiagram {
        document_id: DocumentId,
        diagram: Diagram,
    },
    CreateShape {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape: Shape,
    },
    DeleteShape {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
    },
    CreateEdge {
        document_id: DocumentId,
        diagram_id: ElementSelector,
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
        flow_id: ElementSelector,
        condition: Option<crate::bpmn_model::Expression>,
    },
    SetNodeOwnerContact {
        document_id: DocumentId,
        element_id: ElementSelector,
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
        element_id: ElementSelector,
        name: Option<String>,
    },
    Documentation {
        document_id: DocumentId,
        element_id: ElementSelector,
        documentation: String,
    },
    Delete {
        document_id: DocumentId,
        element_id: ElementSelector,
    },
    MoveShape {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
        bounds: Bounds,
    },
    Reroute {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        edge_id: ElementSelector,
        waypoints: Vec<Point>,
    },
    AssignLane {
        document_id: DocumentId,
        process_id: ElementSelector,
        element_id: ElementSelector,
        lane_id: Option<ElementSelector>,
        bounds: Vec<ShapeBounds>,
    },
    /// Connect two shown elements with the flow kind the BPMN rules pick (see `modeling::connect`).
    Connect {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        source_id: ElementSelector,
        target_id: ElementSelector,
        flow_id: crate::ids::ElementIdentity,
        edge_id: crate::ids::ElementIdentity,
    },
    /// Append an auto-placed node connected from `source_id` (see `modeling::append`).
    Append {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        source_id: ElementSelector,
        kind: NodeKind,
        name: Option<String>,
        node_id: crate::ids::ElementIdentity,
        shape_id: crate::ids::ElementIdentity,
        flow_id: crate::ids::ElementIdentity,
        edge_id: crate::ids::ElementIdentity,
    },
    /// Replace a node or sequence flow with another BPMN type in place (see `replace::replace`).
    ReplaceElement {
        document_id: DocumentId,
        element_id: ElementSelector,
        target: crate::replace::ReplaceTarget,
    },
    /// Move shapes as one gesture with the bpmn-js move behaviors (see `manipulate::move_elements`):
    /// boundary events follow their host, attach/detach, lane membership, scope change, and, with
    /// `flow_id`/`edge_id`, insertion into the sequence flow under a single moved node.
    MoveElements {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shapes: Vec<ShapeBounds>,
        #[serde(default)]
        flow_id: Option<crate::ids::ElementIdentity>,
        #[serde(default)]
        edge_id: Option<crate::ids::ElementIdentity>,
    },
    /// Resize a shape under the bpmn-js resize rules (see `manipulate::resize`).
    ResizeShape {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_id: ElementSelector,
        bounds: Bounds,
    },
    /// Give a flow a new source and/or target, keeping its ID and kind (see `manipulate::reconnect`).
    Reconnect {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        flow_id: ElementSelector,
        source_id: ElementSelector,
        target_id: ElementSelector,
    },
    /// Create an element at a drop point with the bpmn-js create behaviors (see
    /// `manipulate::create_at`): scope and lane from the point, attach, drop on flow.
    CreateAt {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        kind: NodeKind,
        name: Option<String>,
        center: Point,
        node_id: crate::ids::ElementIdentity,
        shape_id: crate::ids::ElementIdentity,
        #[serde(default)]
        flow_id: Option<crate::ids::ElementIdentity>,
        #[serde(default)]
        edge_id: Option<crate::ids::ElementIdentity>,
    },
    /// Add a lane above or below a lane or pool (see `containers::add_lane`).
    AddLane {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        element_id: ElementSelector,
        location: crate::containers::LaneLocation,
    },
    /// Split a lane or pool into `count` even lanes (see `containers::split_lane`).
    SplitLane {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        element_id: ElementSelector,
        count: u8,
    },
    /// Create a pool, wrapping a process diagram's content (see `containers::create_pool`).
    CreatePool {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        name: Option<String>,
        #[serde(default)]
        center: Option<Point>,
    },
    /// Collapse or expand a subprocess (see `containers::toggle_collapse`).
    ToggleCollapse {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        element_id: ElementSelector,
    },
    /// Insert (positive `delta`) or remove space at a line (see `containers::make_space`); the
    /// shapes after the line move, or those before it with `before`.
    MakeSpace {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        axis: crate::containers::Axis,
        start: f64,
        delta: f64,
        #[serde(default)]
        before: bool,
        /// Drag start: inside a pool, only that pool makes space (whole diagram otherwise).
        #[serde(default)]
        at: Option<Point>,
    },
    /// Align shapes left/center/right/top/middle/bottom (see `arrange::prepare_align`).
    Align {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_ids: Vec<ElementSelector>,
        alignment: crate::arrange::Alignment,
    },
    /// Distribute shapes evenly (see `arrange::prepare_distribute`).
    Distribute {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        shape_ids: Vec<ElementSelector>,
        distribution: crate::arrange::Distribution,
    },
    /// Copy elements of a diagram and paste them moved by `dx`/`dy` with fresh IDs, into
    /// `target_diagram_id` (default: the same diagram) (see `clipboard::copy` and `paste`).
    Duplicate {
        document_id: DocumentId,
        diagram_id: ElementSelector,
        element_ids: Vec<ElementSelector>,
        dx: f64,
        dy: f64,
        #[serde(default)]
        target_diagram_id: Option<ElementSelector>,
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
    pub diagram_id: ElementSelector,
    pub shape_id: ElementSelector,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selector_mappings: Vec<SelectorMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promoted_identities: Vec<PromotedIdentity>,
    /// Non-fatal notes from compound mutations (e.g. routes laid out again by `moveElements`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
thread_local! {
    /// Warnings raised while a batch is applied; collected into its [`ChangeSet`].
    static WARNINGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
pub(crate) fn warn(warnings: impl IntoIterator<Item = String>) {
    WARNINGS.with(|w| w.borrow_mut().extend(warnings));
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromotedIdentity {
    pub document_id: DocumentId,
    pub model_path: String,
    pub selector: ElementSelector,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectorMapping {
    pub document_id: DocumentId,
    pub from: ElementSelector,
    pub to: ElementSelector,
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
    checkpoint_cache: crate::identity::CheckpointCache,
    activity_scans: ActivityScans,
}
impl Session {
    pub fn new(project: ProjectData) -> Result<Self, CommandError> {
        let mut checkpoint_cache = crate::identity::CheckpointCache::default();
        let content_id = project
            .content_identity_cached(&mut checkpoint_cache)
            .map_err(CommandError::InvalidParams)?;
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            project: Arc::new(project),
            generation: 0,
            content_id,
            saved: Some(content_id),
            selection: vec![],
            history: History::default(),
            checkpoint_cache,
            activity_scans: Vec::new(),
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
        let mut selector_mappings = Vec::new();
        let mut promoted_identities = Vec::new();
        WARNINGS.with(|w| w.borrow_mut().clear());
        match request {
            Request::Batch { mutations } => {
                let explicit_root_orders: Vec<_> = mutations
                    .iter()
                    .filter_map(|mutation| match mutation {
                        Mutation::SetDocumentRootOrder { document_id, .. }
                        | Mutation::InitializeDocumentRootOrder { document_id } => {
                            Some(*document_id)
                        }
                        _ => None,
                    })
                    .collect();
                let explicit_plane_orders: Vec<_> = mutations
                    .iter()
                    .filter_map(|mutation| match mutation {
                        Mutation::SetDiagramPlaneOrder {
                            document_id,
                            diagram_id,
                            ..
                        } => Some((*document_id, diagram_id.clone())),
                        _ => None,
                    })
                    .collect();
                let cleanup_activity_navigation = mutations.iter().any(cleans_activity_navigation);
                if envelope.expected_generation.is_none()
                    && mutations.iter().any(needs_expected_generation)
                {
                    return Err(CommandError::InvalidParams(
                        "identity and choreography scope operations require an expected generation"
                            .into(),
                    ));
                }
                if mutations.len() > 1000 {
                    return Err(CommandError::InvalidParams(
                        "batch exceeds 1000 mutations".into(),
                    ));
                }
                let requires_undo = mutations.iter().any(requires_undo_entry);
                let mut candidate = (*self.project).clone();
                let mut native_baseline = None;
                for mutation in mutations {
                    if let Mutation::ApplyLayout { proposal } = &mutation {
                        if proposal.session_id != self.id {
                            return Err(CommandError::SessionMismatch);
                        }
                        if proposal.generation != self.generation {
                            return Err(CommandError::GenerationConflict);
                        }
                    }
                    // Explicit identity edits may change native XML IDs: settle the
                    // implicit-change check first and rebuild the baseline lazily after.
                    if matches!(
                        mutation,
                        Mutation::SetElementXmlId { .. }
                            | Mutation::PromoteElementIdentity { .. }
                            | Mutation::InitializeDocumentRootOrder { .. }
                    ) && let Some(baseline) = native_baseline.take()
                    {
                        reject_implicit_native_xml_id_changes(&baseline, &candidate)?;
                    }
                    match mutation {
                        Mutation::SetElementXmlId {
                            document_id,
                            element_id,
                            xml_id,
                        } => {
                            if let Some((from, to)) = crate::element_identity_edit::set_xml_id(
                                &mut candidate,
                                document_id,
                                &element_id,
                                xml_id,
                            )
                            .map_err(CommandError::InvalidParams)?
                            {
                                selector_mappings.push(SelectorMapping {
                                    document_id,
                                    from,
                                    to,
                                });
                            }
                        }
                        Mutation::PromoteElementIdentity {
                            document_id,
                            model_path,
                            xml_id,
                        } => {
                            let selector = crate::element_identity_edit::promote_null_identity(
                                &mut candidate,
                                document_id,
                                &model_path,
                                xml_id,
                            )
                            .map_err(CommandError::InvalidParams)?;
                            promoted_identities.push(PromotedIdentity {
                                document_id,
                                model_path,
                                selector,
                            });
                        }
                        Mutation::InitializeDocumentRootOrder { document_id } => {
                            let document = doc_structural(&mut candidate, document_id)?;
                            if document.root_order.is_some() {
                                return Err(invalid("root order is already initialized"));
                            }
                            document.visit_optional_root_identities_with_path_mut(
                                |path, identity| {
                                    if identity.is_none() {
                                        let new_identity = crate::ids::ElementIdentity::anonymous();
                                        promoted_identities.push(PromotedIdentity {
                                            document_id,
                                            model_path: path.to_owned(),
                                            selector: new_identity.selector(),
                                        });
                                        *identity = Some(new_identity);
                                    }
                                },
                            );
                            document.root_order = Some(
                                document
                                    .root_element_identities()
                                    .into_iter()
                                    .map(|identity| {
                                        identity.map(|id| id.selector()).ok_or_else(|| {
                                            invalid("anonymous root could not be promoted")
                                        })
                                    })
                                    .collect::<Result<Vec<_>, _>>()?,
                            );
                        }
                        other => {
                            if native_baseline.is_none() {
                                native_baseline = Some(native_xml_ids(&candidate));
                            }
                            mutate(&mut candidate, other)?;
                        }
                    }
                }
                post_batch_checks(
                    &self.project,
                    &mut candidate,
                    PostBatch {
                        native_baseline,
                        mappings: &selector_mappings,
                        explicit_root_orders: &explicit_root_orders,
                        explicit_plane_orders: &explicit_plane_orders,
                        cleanup_activity_navigation,
                    },
                    &mut self.activity_scans,
                )?;
                let id = candidate
                    .content_identity_cached(&mut self.checkpoint_cache)
                    .map_err(CommandError::InvalidParams)?;
                if id == self.content_id {
                    changed = false;
                } else {
                    let entry = HistoryEntry {
                        project: self.project.clone(),
                        selection: self.selection.clone(),
                        selector_mappings: selector_mappings.clone(),
                    };
                    if requires_undo {
                        let redo = HistoryEntry {
                            project: Arc::new(candidate.clone()),
                            selection: self.selection.clone(),
                            selector_mappings: selector_mappings.clone(),
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
                    for mapping in &selector_mappings {
                        for key in &mut self.selection {
                            if key.document_id == mapping.document_id && key.xml_id == mapping.from
                            {
                                key.xml_id = mapping.to.clone();
                            }
                        }
                    }
                    self.selection
                        .retain(|key| selection_exists(&self.project, key));
                    self.content_id = id;
                }
            }
            Request::Undo => {
                let old = self.history.undo.pop().ok_or(CommandError::NothingToUndo)?;
                selector_mappings = old
                    .selector_mappings
                    .iter()
                    .rev()
                    .map(|mapping| SelectorMapping {
                        document_id: mapping.document_id,
                        from: mapping.to.clone(),
                        to: mapping.from.clone(),
                    })
                    .collect();
                self.history.redo.push(HistoryEntry {
                    project: self.project.clone(),
                    selection: self.selection.clone(),
                    selector_mappings: old.selector_mappings.clone(),
                });
                self.project = old.project;
                self.selection = old.selection;
                self.content_id = self
                    .project
                    .content_identity_cached(&mut self.checkpoint_cache)
                    .map_err(CommandError::InvalidParams)?;
            }
            Request::Redo => {
                let old = self.history.redo.pop().ok_or(CommandError::NothingToRedo)?;
                selector_mappings = old.selector_mappings.clone();
                self.history.undo.push(HistoryEntry {
                    project: self.project.clone(),
                    selection: self.selection.clone(),
                    selector_mappings: old.selector_mappings.clone(),
                });
                self.project = old.project;
                self.selection = old.selection;
                self.content_id = self
                    .project
                    .content_identity_cached(&mut self.checkpoint_cache)
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
            selector_mappings,
            promoted_identities,
            warnings: WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut())),
        })
    }
}
pub(crate) fn selection_exists(p: &ProjectData, key: &ElementKey) -> bool {
    p.documents
        .iter()
        .find(|d| d.id == key.document_id)
        .is_some_and(|d| crate::identity::semantic_ids(d).contains(&key.xml_id))
}
pub(crate) fn move_shape(s: &mut Shape, bounds: Bounds) {
    if let Some(label) = &mut s.label
        && let Some(label_bounds) = &mut label.bounds
    {
        label_bounds.x += bounds.x - s.bounds.x;
        label_bounds.y += bounds.y - s.bounds.y;
    }
    s.bounds = bounds;
}
fn invalid(s: &str) -> CommandError {
    CommandError::InvalidParams(s.into())
}
/// Document for an edit of the BPMN structure: refused for preserved-source and
/// retained-extension documents, and when opaque XML blocks structural edits.
fn doc_structural(p: &mut ProjectData, id: DocumentId) -> Result<&mut BpmnDocument, CommandError> {
    doc_for_edit(p, id, true)
}
/// Document for a diagram or metadata edit: only preserved-source documents are refused.
fn doc_nonstructural(
    p: &mut ProjectData,
    id: DocumentId,
) -> Result<&mut BpmnDocument, CommandError> {
    doc_for_edit(p, id, false)
}
fn doc_for_edit(
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
    if structural && d.blocks_structural_edits() {
        return Err(invalid("opaque XML prevents safe structural edits"));
    }
    Ok(Arc::make_mut(d))
}
/// Apply a prepared move batch, passing its route warnings to the change set.
fn apply_prepared(
    p: &mut ProjectData,
    prepared: crate::geometry::PreparedMove,
) -> Result<(), CommandError> {
    warn(prepared.warnings);
    let Request::Batch { mutations } = prepared.request else {
        return Err(invalid("invalid move"));
    };
    for m in mutations {
        mutate(p, m)?;
    }
    Ok(())
}
fn mutate(p: &mut ProjectData, m: Mutation) -> Result<(), CommandError> {
    match m {
        Mutation::SetCollaborationRoot {
            document_id,
            collaboration,
        } => collaboration::set_collaboration_root(p, document_id, collaboration)?,
        Mutation::DeleteCollaborationRoot {
            document_id,
            collaboration_id,
        } => collaboration::delete_collaboration_root(p, document_id, collaboration_id)?,
        Mutation::SetPartnerCatalog {
            document_id,
            partner_entities,
            partner_roles,
            endpoints,
        } => collaboration::set_partner_catalog(
            p,
            document_id,
            partner_entities,
            partner_roles,
            endpoints,
        )?,
        Mutation::SetDocumentRootOrder { document_id, order } => {
            doc_structural(p, document_id)?.root_order = order;
        }
        Mutation::InitializeDocumentRootOrder { .. } => {
            return Err(invalid(
                "root order initialization requires the shared session",
            ));
        }
        Mutation::SetDiagramPlaneMetadata {
            document_id,
            diagram_id,
            metadata,
        } => diagram::set_diagram_plane_metadata(p, document_id, diagram_id, metadata)?,
        Mutation::SetDiagramPlaneOrder {
            document_id,
            diagram_id,
            order,
        } => diagram::set_diagram_plane_order(p, document_id, diagram_id, order)?,
        Mutation::SetShapeRepresentationMetadata {
            document_id,
            diagram_id,
            shape_id,
            metadata,
            is_message_visible,
            participant_band_kind,
            choreography_activity_shape,
        } => diagram::set_shape_representation_metadata(
            p,
            document_id,
            diagram_id,
            shape_id,
            metadata,
            is_message_visible,
            participant_band_kind,
            choreography_activity_shape,
        )?,
        Mutation::SetEdgeRepresentationMetadata {
            document_id,
            diagram_id,
            edge_id,
            metadata,
            message_visible_kind,
        } => diagram::set_edge_representation_metadata(
            p,
            document_id,
            diagram_id,
            edge_id,
            metadata,
            message_visible_kind,
        )?,
        Mutation::SetParticipantMultiplicity {
            document_id,
            participant_id,
            multiplicity,
        } => collaboration::set_participant_multiplicity(
            p,
            document_id,
            participant_id,
            multiplicity,
        )?,
        Mutation::SetMessageFlowReference {
            document_id,
            flow_id,
            message_ref,
        } => collaboration::set_message_flow_reference(p, document_id, flow_id, message_ref)?,
        Mutation::SetGlobalTasks {
            document_id,
            global_tasks,
        } => {
            doc_structural(p, document_id)?.global_tasks = global_tasks;
        }
        Mutation::SetCorrelationProperties {
            document_id,
            correlation_properties,
        } => {
            doc_structural(p, document_id)?.correlation_properties = correlation_properties;
        }
        Mutation::SetExpression {
            document_id,
            expression_id,
            expression,
        } => process::set_expression(p, document_id, expression_id, expression)?,
        Mutation::SetDefinitionsExtensions {
            document_id,
            extensions,
        } => {
            doc_structural(p, document_id)?.extensions = extensions;
        }
        Mutation::SetDefinitionsRelationships {
            document_id,
            relationships,
        } => definitions::set_definitions_relationships(p, document_id, relationships)?,
        Mutation::SetDiagramMetadata {
            document_id,
            diagram_id,
            documentation,
            resolution,
        } => diagram::set_diagram_metadata(p, document_id, diagram_id, documentation, resolution)?,
        Mutation::SetDiagramLabelStyles {
            document_id,
            diagram_id,
            label_styles,
        } => diagram::set_diagram_label_styles(p, document_id, diagram_id, label_styles)?,
        Mutation::SetShapeLabel {
            document_id,
            diagram_id,
            shape_id,
            label,
            label_visibility,
        } => diagram::set_shape_label(
            p,
            document_id,
            diagram_id,
            shape_id,
            label,
            label_visibility,
        )?,
        Mutation::SetEdgeLabel {
            document_id,
            diagram_id,
            edge_id,
            label,
            label_visibility,
        } => diagram::set_edge_label(p, document_id, diagram_id, edge_id, label, label_visibility)?,
        Mutation::SetDocumentationEntry {
            document_id,
            documentation_id,
            documentation,
        } => {
            documentation::set_documentation_entry(p, document_id, documentation_id, documentation)?
        }
        Mutation::SetBaseMetadata {
            document_id,
            element_id,
            metadata,
        } => definitions::set_base_metadata(p, document_id, element_id, metadata)?,
        Mutation::SetBaseExtras {
            document_id,
            element_id,
            extras,
        } => definitions::set_base_extras(p, document_id, element_id, extras)?,
        Mutation::SetScriptBody {
            document_id,
            element_id,
            body,
        } => documentation::set_script_body(p, document_id, element_id, body)?,
        Mutation::SetAnnotationBody {
            document_id,
            element_id,
            body,
        } => documentation::set_annotation_body(p, document_id, element_id, body)?,
        Mutation::ReplaceDocumentation {
            document_id,
            element_id,
            documentation,
        } => documentation::replace_documentation(p, document_id, element_id, documentation)?,
        Mutation::SetEventResources {
            document_id,
            resources,
        } => definitions::set_event_resources(p, document_id, resources)?,
        Mutation::SetRootEventDefinitions {
            document_id,
            definitions,
        } => {
            doc_structural(p, document_id)?.root_event_definitions = definitions;
        }
        Mutation::SetShapeDiAttributes {
            document_id,
            diagram_id,
            shape_id,
            is_horizontal,
            is_marker_visible,
        } => diagram::set_shape_di_attributes(
            p,
            document_id,
            diagram_id,
            shape_id,
            is_horizontal,
            is_marker_visible,
        )?,
        Mutation::SetEdgeDiEndpoints {
            document_id,
            diagram_id,
            edge_id,
            source_element,
            target_element,
        } => diagram::set_edge_di_endpoints(
            p,
            document_id,
            diagram_id,
            edge_id,
            source_element,
            target_element,
        )?,
        Mutation::SetElementXmlId { .. } => {
            return Err(invalid(
                "XML ID changes are unavailable until reference rewriting is supported",
            ));
        }
        Mutation::PromoteElementIdentity { .. } => {
            return Err(invalid(
                "identity promotion must be applied through a session",
            ));
        }
        Mutation::SetTextAnnotation {
            document_id,
            element_id,
            text,
        } => documentation::set_text_annotation(p, document_id, element_id, text)?,
        Mutation::SetProcessExecutable {
            document_id,
            process_id,
            is_executable,
        } => process::set_process_executable(p, document_id, process_id, is_executable)?,
        Mutation::SetProcessActivityMetadata {
            document_id,
            process_id,
            metadata,
        } => process::set_process_activity_metadata(p, document_id, process_id, metadata)?,
        Mutation::SetNodeFlowMetadata {
            document_id,
            element_id,
            metadata,
        } => process::set_node_flow_metadata(p, document_id, element_id, metadata)?,
        Mutation::SetFlowMetadata {
            document_id,
            element_id,
            metadata,
        } => process::set_flow_metadata(p, document_id, element_id, metadata)?,
        Mutation::SetCollaborationName { document_id, name } => {
            collaboration_mut(doc_nonstructural(p, document_id)?)?.name = name;
        }
        Mutation::SetCollaborationCorrelationKeys {
            document_id,
            correlation_keys,
        } => {
            collaboration_mut(doc_structural(p, document_id)?)?.correlation_keys = correlation_keys;
        }
        Mutation::SetDiagramName {
            document_id,
            diagram_id,
            name,
        } => diagram::set_diagram_name(p, document_id, diagram_id, name)?,
        Mutation::SetNodeItemAware {
            document_id,
            element_id,
            item_aware,
        } => node_mut(doc_structural(p, document_id)?, &element_id)?.item_aware = item_aware,
        Mutation::SetProcessIoSpecification {
            document_id,
            process_id,
            io_specification,
        } => process::set_process_io_specification(p, document_id, process_id, io_specification)?,
        Mutation::SetDataStores {
            document_id,
            data_stores,
        } => doc_structural(p, document_id)?.data_stores = data_stores,
        Mutation::SetScopeDataObjects {
            document_id,
            scope_id,
            data_objects,
        } => *data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.0 = data_objects,
        Mutation::SetScopeDataAssociations {
            document_id,
            scope_id,
            data_associations,
        } => *data_parts_mut(doc_structural(p, document_id)?, &scope_id)?.1 = data_associations,
        Mutation::SetSupportingCatalog {
            document_id,
            catalog,
        } => definitions::set_supporting_catalog(p, document_id, catalog)?,
        Mutation::SetActivityProperties {
            document_id,
            element_id,
            activity,
        } => process::set_activity_properties(p, document_id, element_id, activity)?,
        Mutation::CreateCategory {
            document_id,
            category,
        } => doc_structural(p, document_id)?.categories.push(category),
        Mutation::UpdateCategory {
            document_id,
            category,
        } => definitions::update_category(p, document_id, category)?,
        Mutation::DeleteCategory {
            document_id,
            category_id,
        } => definitions::delete_category(p, document_id, category_id)?,
        Mutation::CreateGroup {
            document_id,
            scope_id,
            group,
        } => groups_mut(doc_structural(p, document_id)?, &scope_id)?.push(group),
        Mutation::UpdateGroup {
            document_id,
            scope_id,
            group,
        } => groups::update_group(p, document_id, scope_id, group)?,
        Mutation::DeleteGroup {
            document_id,
            scope_id,
            group_id,
        } => groups::delete_group(p, document_id, scope_id, group_id)?,
        Mutation::SetAssociationDirection {
            document_id,
            flow_id,
            direction,
        } => process::set_association_direction(p, document_id, flow_id, direction)?,
        Mutation::SetLaneSetId {
            document_id,
            scope_id,
            lane_set_id,
        } => *lane_set_mut(doc_structural(p, document_id)?, &scope_id)? = lane_set_id,
        Mutation::SetChildLaneSetId {
            document_id,
            scope_id,
            lane_id,
            child_lane_set_id,
        } => process::set_child_lane_set_id(p, document_id, scope_id, lane_id, child_lane_set_id)?,
        Mutation::SetLaneSetName {
            document_id,
            scope_id,
            lane_set_name,
        } => process::set_lane_set_name(p, document_id, scope_id, lane_set_name)?,
        Mutation::SetChildLaneSetName {
            document_id,
            scope_id,
            lane_id,
            child_lane_set_name,
        } => process::set_child_lane_set_name(
            p,
            document_id,
            scope_id,
            lane_id,
            child_lane_set_name,
        )?,
        Mutation::SetLanePartition {
            document_id,
            scope_id,
            lane_id,
            partition_element,
            partition_element_ref,
        } => process::set_lane_partition(
            p,
            document_id,
            scope_id,
            lane_id,
            partition_element,
            partition_element_ref,
        )?,
        Mutation::SetLabelBounds {
            document_id,
            diagram_id,
            representation_id,
            bounds,
        } => diagram::set_label_bounds(p, document_id, diagram_id, representation_id, bounds)?,
        Mutation::SetNodeIoSpecification {
            document_id,
            element_id,
            io_specification,
        } => process::set_node_io_specification(p, document_id, element_id, io_specification)?,
        Mutation::CreateDataStore {
            document_id,
            data_store,
        } => doc_structural(p, document_id)?.data_stores.push(data_store),
        Mutation::UpdateDataStore {
            document_id,
            data_store,
        } => data::update_data_store(p, document_id, data_store)?,
        Mutation::DeleteDataStore {
            document_id,
            data_store_id,
        } => data::delete_data_store(p, document_id, data_store_id)?,
        Mutation::CreateDataObject {
            document_id,
            scope_id,
            data_object,
        } => data::create_data_object(p, document_id, scope_id, data_object)?,
        Mutation::UpdateDataObject {
            document_id,
            scope_id,
            data_object,
        } => data::update_data_object(p, document_id, scope_id, data_object)?,
        Mutation::DeleteDataObject {
            document_id,
            scope_id,
            data_object_id,
        } => data::delete_data_object(p, document_id, scope_id, data_object_id)?,
        Mutation::CreateDataAssociation {
            document_id,
            scope_id,
            association,
        } => data::create_data_association(p, document_id, scope_id, association)?,
        Mutation::UpdateDataAssociation {
            document_id,
            scope_id,
            association,
        } => data::update_data_association(p, document_id, scope_id, association)?,
        Mutation::DeleteDataAssociation {
            document_id,
            scope_id,
            association_id,
        } => data::delete_data_association(p, document_id, scope_id, association_id)?,
        Mutation::SetDiagramPlane {
            document_id,
            diagram_id,
            plane,
        } => diagram::set_diagram_plane(p, document_id, diagram_id, plane)?,
        Mutation::CreateChoreography {
            document_id,
            choreography,
        } => doc_structural(p, document_id)?
            .choreographies
            .push(choreography),
        Mutation::UpdateChoreography {
            document_id,
            choreography,
        } => choreography::update_choreography(p, document_id, choreography)?,
        Mutation::DeleteChoreography {
            document_id,
            choreography_id,
        } => choreography::delete_choreography(p, document_id, choreography_id)?,
        Mutation::PromoteChoreographyGraph {
            document_id,
            choreography_id,
        } => choreography::promote_choreography_graph(p, document_id, choreography_id)?,
        Mutation::CreateChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            index,
            element,
        } => choreography::create_choreography_flow_element(
            p,
            document_id,
            choreography_id,
            scope_id,
            index,
            element,
        )?,
        Mutation::UpdateChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element,
        } => choreography::update_choreography_flow_element(
            p,
            document_id,
            choreography_id,
            scope_id,
            element,
        )?,
        Mutation::DeleteChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element_id,
        } => choreography::apply_delete_choreography_flow_element(
            p,
            document_id,
            choreography_id,
            scope_id,
            element_id,
        )?,
        Mutation::ReorderChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element_id,
            index,
        } => choreography::reorder_choreography_flow_element(
            p,
            document_id,
            choreography_id,
            scope_id,
            element_id,
            index,
        )?,
        Mutation::ReparentChoreographyFlowElement {
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            element_id,
            index,
        } => choreography::reparent_choreography_flow_element(
            p,
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            element_id,
            index,
        )?,
        Mutation::CreateChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            index,
            artifact,
        } => choreography::create_choreography_artifact(
            p,
            document_id,
            choreography_id,
            scope_id,
            index,
            artifact,
        )?,
        Mutation::UpdateChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact,
        } => choreography::update_choreography_artifact(
            p,
            document_id,
            choreography_id,
            scope_id,
            artifact,
        )?,
        Mutation::DeleteChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
        } => choreography::delete_choreography_artifact(
            p,
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
        )?,
        Mutation::ReorderChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
            index,
        } => choreography::reorder_choreography_artifact(
            p,
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
            index,
        )?,
        Mutation::ReparentChoreographyArtifact {
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            artifact_id,
            index,
        } => choreography::reparent_choreography_artifact(
            p,
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            artifact_id,
            index,
        )?,
        Mutation::CreateChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            index,
            association,
        } => choreography::create_choreography_data_association(
            p,
            document_id,
            choreography_id,
            scope_id,
            index,
            association,
        )?,
        Mutation::UpdateChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association,
        } => choreography::update_choreography_data_association(
            p,
            document_id,
            choreography_id,
            scope_id,
            association,
        )?,
        Mutation::DeleteChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association_id,
        } => choreography::delete_choreography_data_association(
            p,
            document_id,
            choreography_id,
            scope_id,
            association_id,
        )?,
        Mutation::ReorderChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association_id,
            index,
        } => choreography::reorder_choreography_data_association(
            p,
            document_id,
            choreography_id,
            scope_id,
            association_id,
            index,
        )?,
        Mutation::CreateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => choreography::create_choreography_activity(p, document_id, choreography_id, activity)?,
        Mutation::UpdateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => choreography::update_choreography_activity(p, document_id, choreography_id, activity)?,
        Mutation::DeleteChoreographyActivity {
            document_id,
            choreography_id,
            activity_id,
        } => choreography::delete_choreography_activity(
            p,
            document_id,
            choreography_id,
            activity_id,
        )?,
        Mutation::CreateConversationNode { document_id, node } => {
            collaboration_mut(doc_structural(p, document_id)?)?
                .conversation_nodes
                .push(node)
        }
        Mutation::UpdateConversationNode { document_id, node } => {
            collaboration::update_conversation_node(p, document_id, node)?
        }
        Mutation::DeleteConversationNode {
            document_id,
            node_id,
        } => collaboration::delete_conversation_node(p, document_id, node_id)?,
        Mutation::CreateConversationLink { document_id, link } => {
            collaboration_mut(doc_structural(p, document_id)?)?
                .conversation_links
                .push(link)
        }
        Mutation::UpdateConversationLink { document_id, link } => {
            collaboration::update_conversation_link(p, document_id, link)?
        }
        Mutation::DeleteConversationLink {
            document_id,
            link_id,
        } => collaboration::delete_conversation_link(p, document_id, link_id)?,
        Mutation::SetNodeLoop {
            document_id,
            element_id,
            loop_characteristics,
        } => process::set_node_loop(p, document_id, element_id, loop_characteristics)?,
        Mutation::SetNodeDetails {
            document_id,
            element_id,
            details,
        } => process::set_node_details(p, document_id, element_id, details)?,
        Mutation::CreateCheckpoint {
            id,
            name,
            created_unix_ms,
        } => review::create_checkpoint(p, id, name, created_unix_ms)?,
        Mutation::RenameCheckpoint { id, name } => review::rename_checkpoint(p, id, name)?,
        Mutation::DeleteCheckpoint { id } => review::delete_checkpoint(p, id)?,
        Mutation::SetReferenceCheckpoint { id } => {
            if id.is_some_and(|id| !p.checkpoints.iter().any(|c| c.id == id)) {
                return Err(invalid("checkpoint not found"));
            }
            p.reference_checkpoint = id;
        }
        Mutation::RestoreCheckpoint { id } => review::restore_checkpoint(p, id)?,
        Mutation::ApplyLayout { proposal } => canvas::apply_layout(p, proposal)?,
        Mutation::CreateComment { comment } => review::create_comment(p, comment)?,
        Mutation::UpdateComment {
            comment_id,
            author,
            text,
        } => review::update_comment(p, comment_id, author, text)?,
        Mutation::DeleteComment { comment_id } => review::delete_comment(p, comment_id)?,
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
        } => canvas::splice_activity(
            p,
            document_id,
            process_id,
            flow_id,
            node,
            new_flow,
            diagram_id,
            shape,
            old_waypoints,
            new_edge,
        )?,
        Mutation::CreateFolder { folder } => p.folders.push(folder),
        Mutation::RenameFolder { folder_id, name } => {
            dictionary::rename_folder(p, folder_id, name)?
        }
        Mutation::MoveFolder { folder_id, parent } => {
            dictionary::move_folder(p, folder_id, parent)?
        }
        Mutation::DeleteFolder { folder_id } => dictionary::delete_folder(p, folder_id)?,
        Mutation::CreateDictionaryEntry { entry } => p.dictionary_entries.push(entry),
        Mutation::UpdateDictionaryEntry { entry } => dictionary::update_dictionary_entry(p, entry)?,
        Mutation::DeleteDictionaryEntry {
            entry_id,
            resolution,
        } => dictionary::delete_dictionary_entry(p, entry_id, resolution)?,
        Mutation::SetDictionaryLinks {
            source,
            kind,
            targets,
        } => dictionary::set_dictionary_links(p, source, kind, targets)?,
        Mutation::CreateAttributeDefinition { definition } => {
            crate::attributes::apply_create(p, definition)?;
        }
        Mutation::UpdateAttributeDefinition {
            definition,
            resolution,
        } => {
            crate::attributes::apply_update(p, definition, resolution)?;
        }
        Mutation::DeleteAttributeDefinition {
            definition_id,
            resolution,
        } => {
            crate::attributes::apply_delete(p, definition_id, resolution)?;
        }
        Mutation::SetAttributeValues { source, values } => {
            // Values live at project level and leave the document untouched, so
            // opaque (foreign) XML in it does not block setting them.
            doc_nonstructural(p, source.document_id)?;
            crate::attributes::apply_set_values(p, source, values)?;
        }
        Mutation::SetConventionsProfile { profile } => {
            if let Some(profile) = &profile {
                profile.validate()?;
            }
            p.conventions = profile;
        }
        Mutation::CreateDocument { document } => p.documents.push(Arc::new(document)),
        Mutation::SetFlowCondition {
            document_id,
            flow_id,
            condition,
        } => process::set_flow_condition(p, document_id, flow_id, condition)?,
        Mutation::ReplaceFlowCondition {
            document_id,
            flow_id,
            condition,
        } => process::replace_flow_condition(p, document_id, flow_id, condition)?,
        Mutation::SetNodeOwnerContact {
            document_id,
            element_id,
            owner,
            contact,
        } => process::set_node_owner_contact(p, document_id, element_id, owner, contact)?,
        Mutation::RenameProject { name } => p.name = name,
        Mutation::ProjectProperties { name, description } => {
            p.name = name;
            p.description = description;
        }
        Mutation::DeleteDocument { document_id } => {
            doc_structural(p, document_id)?;
            p.documents.retain(|d| d.id != document_id);
            prune_removed_element_refs(p, document_id, None);
            p.links.retain(|l| l.target != document_id);
        }
        Mutation::CreateShape {
            document_id,
            diagram_id,
            shape,
        } => canvas::create_shape(p, document_id, diagram_id, shape)?,
        Mutation::DeleteShape {
            document_id,
            diagram_id,
            shape_id,
        } => canvas::delete_shape(p, document_id, diagram_id, shape_id)?,
        Mutation::CreateEdge {
            document_id,
            diagram_id,
            edge,
        } => canvas::create_edge(p, document_id, diagram_id, edge)?,
        Mutation::CreateProcess {
            document_id,
            process: value,
        } => doc_structural(p, document_id)?.processes.push(value),
        Mutation::CreateCollaboration {
            document_id,
            collaboration,
        } => canvas::create_collaboration(p, document_id, collaboration)?,
        Mutation::CreateParticipant {
            document_id,
            participant,
        } => canvas::create_participant(p, document_id, participant)?,
        Mutation::CreateLane {
            document_id,
            process_id,
            lane,
        } => canvas::create_lane(p, document_id, process_id, lane)?,
        Mutation::CreateElement {
            document_id,
            process_id,
            element,
        } => canvas::create_element(p, document_id, process_id, element)?,
        Mutation::CreateFlow {
            document_id,
            process_id,
            flow,
        } => canvas::create_flow(p, document_id, process_id, flow)?,
        Mutation::CreateDiagram {
            document_id,
            diagram,
        } => doc_structural(p, document_id)?.diagrams.push(diagram),
        Mutation::DocumentProperties {
            document_id,
            name,
            folder,
        } => definitions::document_properties(p, document_id, name, folder)?,
        Mutation::SetDefinitionsMetadata {
            document_id,
            metadata,
        } => definitions::set_definitions_metadata(p, document_id, metadata)?,
        Mutation::Rename {
            document_id,
            element_id,
            name,
        } => rename::rename(p, document_id, element_id, name)?,
        Mutation::Documentation {
            document_id,
            element_id,
            documentation,
        } => documentation::documentation(p, document_id, element_id, documentation)?,
        Mutation::MoveShape {
            document_id,
            diagram_id,
            shape_id,
            bounds,
        } => canvas::apply_move_shape(p, document_id, diagram_id, shape_id, bounds)?,
        Mutation::Reroute {
            document_id,
            diagram_id,
            edge_id,
            waypoints,
        } => canvas::reroute(p, document_id, diagram_id, edge_id, waypoints)?,
        Mutation::AssignLane {
            document_id,
            process_id,
            element_id,
            lane_id,
            bounds,
        } => canvas::assign_lane(p, document_id, process_id, element_id, lane_id, bounds)?,
        Mutation::Connect {
            document_id,
            diagram_id,
            source_id,
            target_id,
            flow_id,
            edge_id,
        } => canvas::connect(
            p,
            document_id,
            diagram_id,
            source_id,
            target_id,
            flow_id,
            edge_id,
        )?,
        Mutation::Append {
            document_id,
            diagram_id,
            source_id,
            kind,
            name,
            node_id,
            shape_id,
            flow_id,
            edge_id,
        } => canvas::append(
            p,
            document_id,
            diagram_id,
            source_id,
            kind,
            name,
            node_id,
            shape_id,
            flow_id,
            edge_id,
        )?,
        Mutation::ReplaceElement {
            document_id,
            element_id,
            target,
        } => canvas::replace_element(p, document_id, element_id, target)?,
        Mutation::MoveElements {
            document_id,
            diagram_id,
            shapes,
            flow_id,
            edge_id,
        } => canvas::move_elements(p, document_id, diagram_id, shapes, flow_id, edge_id)?,
        Mutation::ResizeShape {
            document_id,
            diagram_id,
            shape_id,
            bounds,
        } => canvas::resize_shape(p, document_id, diagram_id, shape_id, bounds)?,
        Mutation::Reconnect {
            document_id,
            diagram_id,
            flow_id,
            source_id,
            target_id,
        } => canvas::reconnect(p, document_id, diagram_id, flow_id, source_id, target_id)?,
        Mutation::CreateAt {
            document_id,
            diagram_id,
            kind,
            name,
            center,
            node_id,
            shape_id,
            flow_id,
            edge_id,
        } => canvas::create_at(
            p,
            document_id,
            diagram_id,
            kind,
            name,
            center,
            node_id,
            shape_id,
            flow_id,
            edge_id,
        )?,
        Mutation::AddLane {
            document_id,
            diagram_id,
            element_id,
            location,
        } => canvas::add_lane(p, document_id, diagram_id, element_id, location)?,
        Mutation::SplitLane {
            document_id,
            diagram_id,
            element_id,
            count,
        } => canvas::split_lane(p, document_id, diagram_id, element_id, count)?,
        Mutation::CreatePool {
            document_id,
            diagram_id,
            name,
            center,
        } => canvas::create_pool(p, document_id, diagram_id, name, center)?,
        Mutation::ToggleCollapse {
            document_id,
            diagram_id,
            element_id,
        } => canvas::toggle_collapse(p, document_id, diagram_id, element_id)?,
        Mutation::MakeSpace {
            document_id,
            diagram_id,
            axis,
            start,
            delta,
            before,
            at,
        } => canvas::make_space(p, document_id, diagram_id, axis, start, delta, before, at)?,
        Mutation::Align {
            document_id,
            diagram_id,
            shape_ids,
            alignment,
        } => canvas::align(p, document_id, diagram_id, shape_ids, alignment)?,
        Mutation::Distribute {
            document_id,
            diagram_id,
            shape_ids,
            distribution,
        } => canvas::distribute(p, document_id, diagram_id, shape_ids, distribution)?,
        Mutation::Duplicate {
            document_id,
            diagram_id,
            element_ids,
            dx,
            dy,
            target_diagram_id,
        } => canvas::duplicate(
            p,
            document_id,
            diagram_id,
            element_ids,
            dx,
            dy,
            target_diagram_id,
        )?,
        Mutation::Delete {
            document_id,
            element_id,
        } => canvas::delete(p, document_id, element_id)?,
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
    descriptor!("setCollaborationRoot", "Set collaboration root", "document_id":"DocumentId", "collaboration":"Collaboration"),
    descriptor!("deleteCollaborationRoot", "Delete collaboration root", "document_id":"DocumentId", "collaboration_id":"ElementSelector"),
    descriptor!("setPartnerCatalog", "Set partner and endpoint catalog", "document_id":"DocumentId", "partner_entities":"PartnerEntity[]", "partner_roles":"PartnerRole[]", "endpoints":"EndPoint[]"),
    descriptor!("setDocumentRootOrder", "Set document root order", "document_id":"DocumentId", "order":"ElementSelector[]|null"),
    descriptor!("initializeDocumentRootOrder", "Initialize document root order", "document_id":"DocumentId"),
    descriptor!("setDiagramPlaneMetadata", "Set plane metadata", "document_id":"DocumentId", "diagram_id":"ElementSelector", "metadata":"DiElementMetadata|null"),
    descriptor!("setDiagramPlaneOrder", "Set plane representation order", "document_id":"DocumentId", "diagram_id":"ElementSelector", "order":"ElementSelector[]|null"),
    descriptor!("setShapeRepresentationMetadata", "Set shape representation metadata", "document_id":"DocumentId", "diagram_id":"ElementSelector", "shape_id":"ElementSelector", "metadata":"DiElementMetadata|null", "is_message_visible":"bool|null", "participant_band_kind":"ParticipantBandKind|null", "choreography_activity_shape":"DiReference|null"),
    descriptor!("setEdgeRepresentationMetadata", "Set edge representation metadata", "document_id":"DocumentId", "diagram_id":"ElementSelector", "edge_id":"ElementSelector", "metadata":"DiElementMetadata|null", "message_visible_kind":"MessageVisibleKind|null"),
    descriptor!("setParticipantMultiplicity", "Set participant multiplicity", "document_id":"DocumentId", "participant_id":"ElementSelector", "multiplicity":"ParticipantMultiplicity|null"),
    descriptor!("setMessageFlowReference", "Set message flow reference", "document_id":"DocumentId", "flow_id":"ElementSelector", "message_ref":"ModelReference|null"),
    descriptor!("setGlobalTasks", "Set global callable tasks", "document_id":"DocumentId", "global_tasks":"GlobalTask[]"),
    descriptor!("setCorrelationProperties", "Set correlation properties", "document_id":"DocumentId", "correlation_properties":"CorrelationProperty[]"),
    descriptor!("setExpression", "Set structured expression", "document_id":"DocumentId", "expression_id":"ElementSelector", "expression":"Expression"),
    descriptor!("setDefinitionsExtensions", "Set definitions extensions", "document_id":"DocumentId", "extensions":"DefinitionsExtension[]"),
    descriptor!("setDefinitionsRelationships", "Set definitions relationships", "document_id":"DocumentId", "relationships":"DefinitionsRelationship[]"),
    descriptor!("setDiagramMetadata", "Set diagram metadata", "document_id":"DocumentId", "diagram_id":"ElementSelector", "documentation":"String|null", "resolution":"number|null"),
    descriptor!("setDiagramLabelStyles", "Set diagram label styles", "document_id":"DocumentId", "diagram_id":"ElementSelector", "label_styles":"LabelStyle[]"),
    descriptor!("setShapeLabel", "Set shape label", "document_id":"DocumentId", "diagram_id":"ElementSelector", "shape_id":"ElementSelector", "label":"Label|null", "label_visibility":"DiLabelVisibility|null"),
    descriptor!("setEdgeLabel", "Set edge label", "document_id":"DocumentId", "diagram_id":"ElementSelector", "edge_id":"ElementSelector", "label":"Label|null", "label_visibility":"DiLabelVisibility|null"),
    descriptor!("setDocumentationEntry", "Set documentation entry", "document_id":"DocumentId", "documentation_id":"ElementSelector", "documentation":"Documentation"),
    descriptor!("setBaseMetadata", "Set common base metadata", "document_id":"DocumentId", "element_id":"ElementSelector", "metadata":"BaseMetadata|null"),
    descriptor!("setBaseExtras", "Set common base extras", "document_id":"DocumentId", "element_id":"ElementSelector", "extras":"BaseExtras|null"),
    descriptor!("replaceFlowCondition", "Replace flow expression", "document_id":"DocumentId", "flow_id":"ElementSelector", "condition":"Expression|null"),
    descriptor!("setScriptBody", "Set script mixed content", "document_id":"DocumentId", "element_id":"ElementSelector", "body":"MixedXml|null"),
    descriptor!("setAnnotationBody", "Set annotation mixed content", "document_id":"DocumentId", "element_id":"ElementSelector", "body":"MixedXml|null"),
    descriptor!("replaceDocumentation", "Replace ordered documentation", "document_id":"DocumentId", "element_id":"ElementSelector", "documentation":"DocumentationSet"),
    descriptor!("setEventResources", "Set event resources", "document_id":"DocumentId", "resources":"EventResource[]"),
    descriptor!("setRootEventDefinitions", "Set reusable event definitions", "document_id":"DocumentId", "definitions":"EventDefinition[]"),
    descriptor!("setShapeDiAttributes", "Set shape diagram attributes", "document_id":"DocumentId", "diagram_id":"ElementSelector", "shape_id":"ElementSelector", "is_horizontal":"bool|null", "is_marker_visible":"bool|null"),
    descriptor!("setEdgeDiEndpoints", "Set edge diagram endpoints", "document_id":"DocumentId", "diagram_id":"ElementSelector", "edge_id":"ElementSelector", "source_element":"ElementSelector|null", "target_element":"ElementSelector|null"),
    descriptor!("setCollaborationName", "Set collaboration name", "document_id":"DocumentId", "name":"String|null"),
    descriptor!("setDiagramName", "Set diagram name", "document_id":"DocumentId", "diagram_id":"ElementSelector", "name":"String|null"),
    descriptor!("setProcessExecutable", "Set process executable attribute", "document_id":"DocumentId", "process_id":"ElementSelector", "is_executable":"bool|null"),
    descriptor!("setCollaborationCorrelationKeys", "Set collaboration correlation keys", "document_id":"DocumentId", "correlation_keys":"CorrelationKey[]"),
    descriptor!("setProcessActivityMetadata", "Set process activity metadata", "document_id":"DocumentId", "process_id":"ElementSelector", "metadata":"ProcessActivityMetadata|null"),
    descriptor!("setNodeFlowMetadata", "Set node flow metadata", "document_id":"DocumentId", "element_id":"ElementSelector", "metadata":"FlowElementMetadata|null"),
    descriptor!("setFlowMetadata", "Set flow metadata", "document_id":"DocumentId", "element_id":"ElementSelector", "metadata":"FlowElementMetadata|null"),
    descriptor!("setElementXmlId", "Set element XML ID", "document_id":"DocumentId", "element_id":"ElementSelector", "xml_id":"XmlId|null"),
    descriptor!("promoteElementIdentity", "Promote anonymous identity", "document_id":"DocumentId", "model_path":"String", "xml_id":"XmlId|null"),
    descriptor!("setTextAnnotation", "Set annotation text", "document_id":"DocumentId", "element_id":"ElementSelector", "text":"String|null"),
    descriptor!("setNodeItemAware", "Set data reference metadata", "document_id":"DocumentId", "element_id":"ElementSelector", "item_aware":"ItemAwareMetadata|null"),
    descriptor!("setProcessIoSpecification", "Set process I/O specification", "document_id":"DocumentId", "process_id":"ElementSelector", "io_specification":"IoSpecification|null"),
    descriptor!("setDataStores", "Set data stores", "document_id":"DocumentId", "data_stores":"DataStore[]"),
    descriptor!("setScopeDataObjects", "Set scope data objects", "document_id":"DocumentId", "scope_id":"ElementSelector", "data_objects":"DataObject[]"),
    descriptor!("setScopeDataAssociations", "Set scope data associations", "document_id":"DocumentId", "scope_id":"ElementSelector", "data_associations":"DataAssociation[]"),
    descriptor!("setSupportingCatalog", "Set supporting BPMN catalog", "document_id":"DocumentId", "catalog":"SupportingCatalog"),
    descriptor!("setDefinitionsMetadata", "Set definitions metadata", "document_id":"DocumentId", "metadata":"DefinitionsMetadata"),
    descriptor!("setActivityProperties", "Set activity properties", "document_id":"DocumentId", "element_id":"ElementSelector", "activity":"ActivityProperties|null"),
    descriptor!("createCategory","Create category","document_id":"DocumentId","category":"Category"),
    descriptor!("updateCategory","Update category","document_id":"DocumentId","category":"Category"),
    descriptor!("deleteCategory","Delete category","document_id":"DocumentId","category_id":"ElementSelector"),
    descriptor!("createGroup","Create group","document_id":"DocumentId","scope_id":"ElementSelector","group":"Group"),
    descriptor!("updateGroup","Update group","document_id":"DocumentId","scope_id":"ElementSelector","group":"Group"),
    descriptor!("deleteGroup","Delete group","document_id":"DocumentId","scope_id":"ElementSelector","group_id":"ElementSelector"),
    descriptor!("setAssociationDirection","Set association direction","document_id":"DocumentId","flow_id":"ElementSelector","direction":"AssociationDirection|null"),
    descriptor!("setLaneSetId","Set lane set ID","document_id":"DocumentId","scope_id":"ElementSelector","lane_set_id":"XmlId|null"),
    descriptor!("setChildLaneSetId","Set child lane set ID","document_id":"DocumentId","scope_id":"ElementSelector","lane_id":"ElementSelector","child_lane_set_id":"XmlId|null"),
    descriptor!("setLaneSetName","Set lane set name","document_id":"DocumentId","scope_id":"ElementSelector","lane_set_name":"string|null"),
    descriptor!("setChildLaneSetName","Set child lane set name","document_id":"DocumentId","scope_id":"ElementSelector","lane_id":"ElementSelector","child_lane_set_name":"string|null"),
    descriptor!("setLanePartition","Set lane partition element and partition element reference","document_id":"DocumentId","scope_id":"ElementSelector","lane_id":"ElementSelector","partition_element":"LanePartitionElement|null","partition_element_ref":"ModelReference|null"),
    descriptor!("setLabelBounds", "Set label bounds", "document_id":"DocumentId", "diagram_id":"ElementSelector", "representation_id":"ElementSelector", "bounds":"Bounds"),
    descriptor!("setNodeIoSpecification", "Set activity input/output", "document_id":"DocumentId", "element_id":"ElementSelector", "io_specification":"IoSpecification|null"),
    descriptor!("createDataStore","Create data store","document_id":"DocumentId","data_store":"DataStore"),
    descriptor!("updateDataStore","Update data store","document_id":"DocumentId","data_store":"DataStore"),
    descriptor!("deleteDataStore","Delete data store","document_id":"DocumentId","data_store_id":"ElementSelector"),
    descriptor!("createDataObject","Create data object","document_id":"DocumentId","scope_id":"ElementSelector","data_object":"DataObject"),
    descriptor!("updateDataObject","Update data object","document_id":"DocumentId","scope_id":"ElementSelector","data_object":"DataObject"),
    descriptor!("deleteDataObject","Delete data object","document_id":"DocumentId","scope_id":"ElementSelector","data_object_id":"ElementSelector"),
    descriptor!("createDataAssociation","Create data association","document_id":"DocumentId","scope_id":"ElementSelector","association":"DataAssociation"),
    descriptor!("updateDataAssociation","Update data association","document_id":"DocumentId","scope_id":"ElementSelector","association":"DataAssociation"),
    descriptor!("deleteDataAssociation","Delete data association","document_id":"DocumentId","scope_id":"ElementSelector","association_id":"ElementSelector"),
    descriptor!("setDiagramPlane", "Set diagram plane", "document_id":"DocumentId", "diagram_id":"ElementSelector", "plane":"ElementSelector"),
    descriptor!("createChoreography", "Create choreography", "document_id":"DocumentId", "choreography":"Choreography"),
    descriptor!("updateChoreography", "Update choreography", "document_id":"DocumentId", "choreography":"Choreography"),
    descriptor!("deleteChoreography", "Delete choreography", "document_id":"DocumentId", "choreography_id":"ElementSelector"),
    descriptor!("promoteChoreographyGraph", "Promote legacy choreography graph", "document_id":"DocumentId", "choreography_id":"ElementSelector"),
    descriptor!("createChoreographyFlowElement", "Create scoped choreography flow element", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "index":"usize", "element":"ChoreographyFlowElement"),
    descriptor!("updateChoreographyFlowElement", "Update scoped choreography flow element", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "element":"ChoreographyFlowElement"),
    descriptor!("deleteChoreographyFlowElement", "Delete scoped choreography flow element", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "element_id":"ElementSelector"),
    descriptor!("reorderChoreographyFlowElement", "Reorder scoped choreography flow element", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "element_id":"ElementSelector", "index":"usize"),
    descriptor!("reparentChoreographyFlowElement", "Move choreography flow element between scopes", "document_id":"DocumentId", "choreography_id":"ElementSelector", "source_scope_id":"ElementSelector", "target_scope_id":"ElementSelector", "element_id":"ElementSelector", "index":"usize"),
    descriptor!("createChoreographyArtifact", "Create scoped choreography artifact", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "index":"usize", "artifact":"CollaborationArtifact"),
    descriptor!("updateChoreographyArtifact", "Update scoped choreography artifact", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "artifact":"CollaborationArtifact"),
    descriptor!("deleteChoreographyArtifact", "Delete scoped choreography artifact", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "artifact_id":"ElementSelector"),
    descriptor!("reorderChoreographyArtifact", "Reorder scoped choreography artifact", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "artifact_id":"ElementSelector", "index":"usize"),
    descriptor!("reparentChoreographyArtifact", "Move choreography artifact between scopes", "document_id":"DocumentId", "choreography_id":"ElementSelector", "source_scope_id":"ElementSelector", "target_scope_id":"ElementSelector", "artifact_id":"ElementSelector", "index":"usize"),
    descriptor!("createChoreographyDataAssociation", "Create scoped choreography data association", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "index":"usize", "association":"DataAssociation"),
    descriptor!("updateChoreographyDataAssociation", "Update scoped choreography data association", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "association":"DataAssociation"),
    descriptor!("deleteChoreographyDataAssociation", "Delete scoped choreography data association", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "association_id":"ElementSelector"),
    descriptor!("reorderChoreographyDataAssociation", "Reorder scoped choreography data association", "document_id":"DocumentId", "choreography_id":"ElementSelector", "scope_id":"ElementSelector", "association_id":"ElementSelector", "index":"usize"),
    descriptor!("createChoreographyActivity", "Create choreography task", "document_id":"DocumentId", "choreography_id":"ElementSelector", "activity":"ChoreographyActivity"),
    descriptor!("updateChoreographyActivity", "Update choreography task", "document_id":"DocumentId", "choreography_id":"ElementSelector", "activity":"ChoreographyActivity"),
    descriptor!("deleteChoreographyActivity", "Delete choreography task", "document_id":"DocumentId", "choreography_id":"ElementSelector", "activity_id":"ElementSelector"),
    descriptor!("createConversationNode", "Create conversation", "document_id":"DocumentId", "node":"ConversationNode"),
    descriptor!("updateConversationNode", "Update conversation", "document_id":"DocumentId", "node":"ConversationNode"),
    descriptor!("deleteConversationNode", "Delete conversation", "document_id":"DocumentId", "node_id":"ElementSelector"),
    descriptor!("createConversationLink", "Create conversation link", "document_id":"DocumentId", "link":"ConversationLink"),
    descriptor!("updateConversationLink", "Update conversation link", "document_id":"DocumentId", "link":"ConversationLink"),
    descriptor!("deleteConversationLink", "Delete conversation link", "document_id":"DocumentId", "link_id":"ElementSelector"),
    descriptor!("applyLayout", "Apply layout", "proposal":"LayoutProposal"),
    descriptor!("createComment", "Create comment", "comment":"Comment"),
    descriptor!("updateComment", "Update comment", "comment_id":"CommentId", "author":"string", "text":"string"),
    descriptor!("deleteComment", "Delete comment", "comment_id":"CommentId"),
    descriptor!("createCheckpoint", "Create checkpoint", "id":"CheckpointId", "name":"string", "created_unix_ms":"u64"),
    descriptor!("renameCheckpoint", "Rename checkpoint", "id":"CheckpointId", "name":"string"),
    descriptor!("deleteCheckpoint", "Delete checkpoint", "id":"CheckpointId"),
    descriptor!("setReferenceCheckpoint", "Set reference checkpoint", "id":"CheckpointId|null"),
    descriptor!("restoreCheckpoint", "Restore checkpoint", "id":"CheckpointId"),
    descriptor!("setNodeLoop", "Set node loop characteristics", "document_id":"DocumentId", "element_id":"ElementSelector", "loop_characteristics":"LoopCharacteristics|null"),
    descriptor!("setNodeDetails", "Set node details", "document_id":"DocumentId", "element_id":"ElementSelector", "details":"NodeDetails|null"),
    descriptor!("addResource","Add resource","resource":"Resource"),
    descriptor!("spliceActivity","Splice activity","document_id":"DocumentId","process_id":"ElementSelector","flow_id":"ElementSelector","node":"Node","new_flow":"Flow","diagram_id":"ElementSelector","shape":"Shape","old_waypoints":"Point[]","new_edge":"Edge"),
    descriptor!("createFolder","Create folder","folder":"Folder"),
    descriptor!("renameFolder","Rename folder","folder_id":"XmlId","name":"string"),
    descriptor!("moveFolder","Move folder","folder_id":"XmlId","parent":"XmlId|null"),
    descriptor!("deleteFolder","Delete folder","folder_id":"XmlId"),
    descriptor!("createDictionaryEntry","Create entry","entry":"DictionaryEntry"),
    descriptor!("updateDictionaryEntry","Update entry","entry":"DictionaryEntry"),
    descriptor!("deleteDictionaryEntry","Delete entry","entry_id":"DictionaryEntryId","resolution":"DictionaryDeleteResolution"),
    descriptor!("setDictionaryLinks","Set links","source":"ElementKey","kind":"DictionaryKind","targets":"DictionaryTarget[]"),
    descriptor!("createAttributeDefinition","Create attribute definition","definition":"AttributeDefinition"),
    descriptor!("updateAttributeDefinition","Update attribute definition","definition":"AttributeDefinition","resolution":"AttributeUpdateResolution"),
    descriptor!("deleteAttributeDefinition","Delete attribute definition","definition_id":"AttributeDefinitionId","resolution":"AttributeDeleteResolution"),
    descriptor!("setAttributeValues","Set attribute values","source":"ElementKey","values":"AttributeValueInput[]"),
    descriptor!("setConventionsProfile","Set modeling conventions profile","profile":"ConventionsProfile|null"),
    descriptor!("batch","Apply batch","mutations":"Mutation[]"),
    descriptor!("undo", "Undo"),
    descriptor!("redo", "Redo"),
    descriptor!("createDocument","Create document","document":"BpmnDocument"),
    descriptor!("createProcess","Create process","document_id":"DocumentId","process":"Process"),
    descriptor!("createCollaboration","Create collaboration","document_id":"DocumentId","collaboration":"Collaboration"),
    descriptor!("createParticipant","Create participant","document_id":"DocumentId","participant":"Participant"),
    descriptor!("createLane","Create lane","document_id":"DocumentId","process_id":"ElementSelector","lane":"Lane"),
    descriptor!("createElement","Create element","document_id":"DocumentId","process_id":"ElementSelector","element":"Node"),
    descriptor!("createFlow","Create flow","document_id":"DocumentId","process_id":"ElementSelector|null","flow":"Flow"),
    descriptor!("createDiagram","Create diagram","document_id":"DocumentId","diagram":"Diagram"),
    descriptor!("createShape","Create shape","document_id":"DocumentId","diagram_id":"ElementSelector","shape":"Shape"),
    descriptor!("deleteShape","Delete shape representation","document_id":"DocumentId","diagram_id":"ElementSelector","shape_id":"ElementSelector"),
    descriptor!("createEdge","Create edge","document_id":"DocumentId","diagram_id":"ElementSelector","edge":"Edge"),
    descriptor!("deleteDocument","Delete document","document_id":"DocumentId"),
    descriptor!("projectProperties","Project properties","name":"string","description":"string"),
    descriptor!("renameProject","Rename project","name":"string"),
    descriptor!("documentProperties","Document properties","document_id":"DocumentId","name":"string","folder":"XmlId|null"),
    descriptor!("rename","Rename","document_id":"DocumentId","element_id":"ElementSelector","name":"string"),
    descriptor!("documentation","Set documentation","document_id":"DocumentId","element_id":"ElementSelector","documentation":"string"),
    descriptor!("delete","Delete element with its connected flows","document_id":"DocumentId","element_id":"ElementSelector"),
    descriptor!("moveShape","Move shape","document_id":"DocumentId","diagram_id":"ElementSelector","shape_id":"ElementSelector","bounds":"Bounds"),
    descriptor!("reroute","Reroute edge (the route is normalized: near-orthogonal segments straightened, duplicate and collinear points dropped, ends on a shape's border docked on its outline; an explicit label follows the route)","document_id":"DocumentId","diagram_id":"ElementSelector","edge_id":"ElementSelector","waypoints":"Point[]"),
    descriptor!("assignLane","Assign lane","document_id":"DocumentId","process_id":"ElementSelector","element_id":"ElementSelector","lane_id":"ElementSelector|null","bounds":"ShapeBounds[]"),
    descriptor!("setFlowCondition","Set flow condition","document_id":"DocumentId","flow_id":"ElementSelector","condition":"string|null"),
    descriptor!("setNodeOwnerContact","Set node owner and contact","document_id":"DocumentId","element_id":"ElementSelector","owner":"string","contact":"string"),
    descriptor!("connect","Connect elements (flow kind chosen by BPMN rules)","document_id":"DocumentId","diagram_id":"ElementSelector","source_id":"ElementSelector","target_id":"ElementSelector","flow_id":"ElementIdentity","edge_id":"ElementIdentity"),
    descriptor!("append","Append an auto-placed connected element","document_id":"DocumentId","diagram_id":"ElementSelector","source_id":"ElementSelector","kind":"NodeKind","name":"string|null","node_id":"ElementIdentity","shape_id":"ElementIdentity","flow_id":"ElementIdentity","edge_id":"ElementIdentity"),
    descriptor!("replaceElement","Replace an element with another BPMN type","document_id":"DocumentId","element_id":"ElementSelector","target":"ReplaceTarget"),
    descriptor!("moveElements","Move shapes with attach, drop-on-flow, lane, and boundary-follow behaviors","document_id":"DocumentId","diagram_id":"ElementSelector","shapes":"ShapeBounds[]","flow_id":"ElementIdentity|null","edge_id":"ElementIdentity|null"),
    descriptor!("resizeShape","Resize a shape under the BPMN resize rules","document_id":"DocumentId","diagram_id":"ElementSelector","shape_id":"ElementSelector","bounds":"Bounds"),
    descriptor!("reconnect","Reconnect a flow to another source or target","document_id":"DocumentId","diagram_id":"ElementSelector","flow_id":"ElementSelector","source_id":"ElementSelector","target_id":"ElementSelector"),
    descriptor!("createAt","Create an element at a drop point","document_id":"DocumentId","diagram_id":"ElementSelector","kind":"NodeKind","name":"string|null","center":"Point","node_id":"ElementIdentity","shape_id":"ElementIdentity","flow_id":"ElementIdentity|null","edge_id":"ElementIdentity|null"),
    descriptor!("addLane","Add a lane above or below a lane or pool","document_id":"DocumentId","diagram_id":"ElementSelector","element_id":"ElementSelector","location":"LaneLocation"),
    descriptor!("splitLane","Split a lane or pool into even lanes","document_id":"DocumentId","diagram_id":"ElementSelector","element_id":"ElementSelector","count":"u8"),
    descriptor!("createPool","Create a pool, wrapping a process diagram's content","document_id":"DocumentId","diagram_id":"ElementSelector","name":"string|null","center":"Point|null"),
    descriptor!("toggleCollapse","Collapse or expand a subprocess","document_id":"DocumentId","diagram_id":"ElementSelector","element_id":"ElementSelector"),
    descriptor!("makeSpace","Insert or remove space at a line","document_id":"DocumentId","diagram_id":"ElementSelector","axis":"Axis","start":"number","delta":"number","before":"bool","at":"Point|null"),
    descriptor!("align","Align shapes","document_id":"DocumentId","diagram_id":"ElementSelector","shape_ids":"ElementSelector[]","alignment":"Alignment"),
    descriptor!("distribute","Distribute shapes evenly","document_id":"DocumentId","diagram_id":"ElementSelector","shape_ids":"ElementSelector[]","distribution":"Distribution"),
    descriptor!("duplicate","Copy elements and paste them at an offset with fresh IDs","document_id":"DocumentId","diagram_id":"ElementSelector","element_ids":"ElementSelector[]","dx":"number","dy":"number","target_diagram_id":"ElementSelector|null"),
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
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
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
                di_metadata: None,
                id: None,
                style_ref: None,
                bounds: Some(Bounds {
                    x: 0.0,
                    y: 65.0,
                    width: 80.0,
                    height: 30.0,
                }),
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
        let label = shape.label.unwrap().bounds.unwrap();
        assert_eq!(
            (label.x, label.y, label.width, label.height),
            (15.0, 57.0, 80.0, 30.0)
        );
    }
}
