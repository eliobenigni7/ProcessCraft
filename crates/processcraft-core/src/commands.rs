//! Atomic, toolkit-independent project mutations and session history.
use crate::{
    history::{History, HistoryEntry},
    ids::{
        CheckpointId, CommentId, DictionaryEntryId, DocumentId, ElementKey, ElementSelector, XmlId,
    },
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
                let cleanup_activity_navigation = mutations.iter().any(|mutation| {
                    matches!(
                        mutation,
                        Mutation::SetGlobalTasks { .. }
                            | Mutation::SetSupportingCatalog { .. }
                            | Mutation::SetDataStores { .. }
                            | Mutation::SetRootEventDefinitions { .. }
                            | Mutation::SetEventResources { .. }
                            | Mutation::UpdateCategory { .. }
                            | Mutation::DeleteCategory { .. }
                            | Mutation::Delete { .. }
                            | Mutation::SetCollaborationRoot { .. }
                            | Mutation::DeleteCollaborationRoot { .. }
                            | Mutation::SetPartnerCatalog { .. }
                            | Mutation::DeleteChoreographyActivity { .. }
                            | Mutation::DeleteChoreographyFlowElement { .. }
                            | Mutation::UpdateChoreographyFlowElement { .. }
                            | Mutation::DeleteChoreographyArtifact { .. }
                            | Mutation::UpdateChoreographyArtifact { .. }
                            | Mutation::DeleteChoreographyDataAssociation { .. }
                            | Mutation::UpdateChoreographyDataAssociation { .. }
                            | Mutation::SetParticipantMultiplicity { .. }
                            | Mutation::SetCorrelationProperties { .. }
                            | Mutation::SetProcessActivityMetadata { .. }
                            | Mutation::SetNodeFlowMetadata { .. }
                            | Mutation::SetFlowMetadata { .. }
                            | Mutation::SetCollaborationCorrelationKeys { .. }
                            | Mutation::SetActivityProperties { .. }
                            | Mutation::SetNodeLoop { .. }
                            | Mutation::SetNodeDetails { .. }
                            | Mutation::UpdateDataObject { .. }
                            | Mutation::DeleteDataObject { .. }
                            | Mutation::SetScopeDataObjects { .. }
                            | Mutation::UpdateConversationNode { .. }
                            | Mutation::UpdateChoreographyActivity { .. }
                    )
                });
                if envelope.expected_generation.is_none()
                    && mutations.iter().any(|m| {
                        matches!(
                            m,
                            Mutation::PromoteElementIdentity { .. }
                                | Mutation::InitializeDocumentRootOrder { .. }
                                | Mutation::PromoteChoreographyGraph { .. }
                                | Mutation::CreateChoreographyFlowElement { .. }
                                | Mutation::UpdateChoreographyFlowElement { .. }
                                | Mutation::DeleteChoreographyFlowElement { .. }
                                | Mutation::ReorderChoreographyFlowElement { .. }
                                | Mutation::ReparentChoreographyFlowElement { .. }
                                | Mutation::CreateChoreographyArtifact { .. }
                                | Mutation::UpdateChoreographyArtifact { .. }
                                | Mutation::DeleteChoreographyArtifact { .. }
                                | Mutation::ReorderChoreographyArtifact { .. }
                                | Mutation::ReparentChoreographyArtifact { .. }
                                | Mutation::CreateChoreographyDataAssociation { .. }
                                | Mutation::UpdateChoreographyDataAssociation { .. }
                                | Mutation::DeleteChoreographyDataAssociation { .. }
                                | Mutation::ReorderChoreographyDataAssociation { .. }
                        ) || matches!(m, Mutation::CreateChoreography { choreography, .. } if choreography.graph.is_some())
                    })
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
                let requires_undo = mutations.iter().any(|m| {
                    matches!(
                        m,
                        Mutation::RestoreCheckpoint { .. }
                            | Mutation::ApplyLayout { .. }
                            | Mutation::PromoteChoreographyGraph { .. }
                            | Mutation::CreateChoreographyFlowElement { .. }
                            | Mutation::UpdateChoreographyFlowElement { .. }
                            | Mutation::DeleteChoreographyFlowElement { .. }
                            | Mutation::ReorderChoreographyFlowElement { .. }
                            | Mutation::ReparentChoreographyFlowElement { .. }
                            | Mutation::CreateChoreographyArtifact { .. }
                            | Mutation::UpdateChoreographyArtifact { .. }
                            | Mutation::DeleteChoreographyArtifact { .. }
                            | Mutation::ReorderChoreographyArtifact { .. }
                            | Mutation::ReparentChoreographyArtifact { .. }
                            | Mutation::CreateChoreographyDataAssociation { .. }
                            | Mutation::UpdateChoreographyDataAssociation { .. }
                            | Mutation::DeleteChoreographyDataAssociation { .. }
                            | Mutation::ReorderChoreographyDataAssociation { .. }
                    ) || matches!(m, Mutation::CreateChoreography { choreography, .. } if choreography.graph.is_some())
                });
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
                            let document = doc(&mut candidate, document_id, true)?;
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
                if let Some(baseline) = native_baseline {
                    reject_implicit_native_xml_id_changes(&baseline, &candidate)?;
                }
                if cleanup_activity_navigation {
                    prune_removed_activity_navigation(
                        &self.project,
                        &mut candidate,
                        &selector_mappings,
                    );
                }
                reconcile_root_orders(
                    &self.project,
                    &mut candidate,
                    &selector_mappings,
                    &explicit_root_orders,
                )?;
                reconcile_plane_orders(
                    &self.project,
                    &mut candidate,
                    &selector_mappings,
                    &explicit_plane_orders,
                );
                reject_newly_dangling_relationships(&self.project, &candidate)?;
                reject_removed_foreign_attribute_targets(&self.project, &candidate)?;
                reject_new_choreography_graph_issues(
                    &self.project,
                    &candidate,
                    &selector_mappings,
                )?;
                reject_new_activity_reference_issues(
                    &self.project,
                    &candidate,
                    &selector_mappings,
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
fn counts<'a, T: Eq + std::hash::Hash + 'a>(
    items: impl IntoIterator<Item = &'a T>,
) -> std::collections::HashMap<&'a T, usize> {
    let mut counts = std::collections::HashMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}
/// True when some value occurs more often in `next` than in `previous`.
fn introduces_new<T: Eq + std::hash::Hash>(previous: &[T], next: &[T]) -> bool {
    let before = counts(previous);
    counts(next)
        .into_iter()
        .any(|(item, count)| count > before.get(item).copied().unwrap_or(0))
}
fn reject_new_choreography_graph_issues(
    old: &ProjectData,
    candidate: &ProjectData,
    mappings: &[SelectorMapping],
) -> Result<(), CommandError> {
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|d| d.id == after.id) else {
            continue;
        };
        // Both scans only visit choreography scopes.
        if Arc::ptr_eq(before, after)
            || (before.choreographies.is_empty() && after.choreographies.is_empty())
        {
            continue;
        }
        let selected = |d: &BpmnDocument| {
            crate::validation::choreography_graph_diagnostics(d)
                .into_iter()
                .filter(|(_, code, _)| {
                    matches!(
                        *code,
                        "mixedChoreographyAuthority"
                            | "choreographySequenceEndpointWrongScope"
                            | "choreographySequenceEndpointNotIdref"
                            | "choreographyFlowWrongType"
                            | "choreographyMessageFlowWrongOwner"
                    )
                })
                .map(|(owner, code, _)| (owner, code))
                .collect::<Vec<_>>()
        };
        let mut previous = selected(before);
        for (owner, _) in &mut previous {
            for mapping in mappings.iter().filter(|m| m.document_id == after.id) {
                if *owner == mapping.from {
                    *owner = mapping.to.clone();
                }
            }
        }
        let next = selected(after);
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an invalid choreography graph reference or kind",
            ));
        }
        let mut previous = crate::validation::choreography_reference_issues(before);
        for issue in &mut previous {
            for mapping in mappings.iter().filter(|m| m.document_id == after.id) {
                if issue.owner == mapping.from {
                    issue.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) = &mut issue.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
            }
        }
        let next = crate::validation::choreography_reference_issues(after);
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an invalid choreography reference",
            ));
        }
    }
    Ok(())
}
type NativeXmlIds =
    std::collections::HashMap<(DocumentId, crate::ids::NativeElementId), Option<XmlId>>;
fn native_xml_ids(project: &ProjectData) -> NativeXmlIds {
    project
        .documents
        .iter()
        .flat_map(|document| {
            document
                .element_identities()
                .into_iter()
                .filter_map(|id| match id {
                    crate::ids::ElementIdentity::Native { native_id, xml_id } => {
                        Some(((document.id, *native_id), xml_id.clone()))
                    }
                    _ => None,
                })
        })
        .collect()
}
fn reject_implicit_native_xml_id_changes(
    before: &NativeXmlIds,
    after: &ProjectData,
) -> Result<(), CommandError> {
    for new_doc in &after.documents {
        if new_doc.element_identities().into_iter().any(|id| match id {
            crate::ids::ElementIdentity::Native { native_id, xml_id } => before
                .get(&(new_doc.id, *native_id))
                .is_some_and(|previous| previous != xml_id),
            _ => false,
        }) {
            return Err(invalid(
                "changing an existing native element XML ID requires SetElementXmlId",
            ));
        }
    }
    Ok(())
}
/// Activity-reference scan of a document, memoized by `Arc` identity: the
/// previous commit's "after" scan is the next commit's "before" scan.
type ActivityScans = Vec<(
    Arc<BpmnDocument>,
    Arc<(
        Vec<crate::validation::ActivityReferenceIssue>,
        Vec<crate::validation::ActivityReferenceResolution>,
    )>,
)>;
/// Foreign attributes (e.g. `camunda:*`) are kept verbatim and may name other elements by ID.
/// Structural edits are allowed around them, but a batch may not remove an element that such
/// an attribute names, since the retained value would then dangle.
fn reject_removed_foreign_attribute_targets(
    old: &ProjectData,
    candidate: &ProjectData,
) -> Result<(), CommandError> {
    for before in &old.documents {
        let Some(after) = candidate.documents.iter().find(|d| d.id == before.id) else {
            continue;
        };
        if Arc::ptr_eq(before, after) {
            continue;
        }
        let values = before.foreign_attribute_values();
        if values.is_empty() {
            continue;
        }
        let xml_ids = |doc: &BpmnDocument| -> std::collections::HashSet<String> {
            doc.element_identities()
                .iter()
                .filter_map(|id| id.xml_id().map(|x| x.to_string()))
                .collect()
        };
        let (existing, remaining) = (xml_ids(before), xml_ids(after));
        for token in values
            .iter()
            .flat_map(|v| v.split(|c: char| c.is_whitespace() || c == ','))
            .map(|t| t.rsplit(':').next().unwrap_or(t))
        {
            if existing.contains(token) && !remaining.contains(token) {
                return Err(invalid(&format!(
                    "{token} is referenced by an extension attribute; edit or remove that attribute first"
                )));
            }
        }
    }
    Ok(())
}
fn reject_new_activity_reference_issues(
    old: &ProjectData,
    candidate: &ProjectData,
    mappings: &[SelectorMapping],
    scans: &mut ActivityScans,
) -> Result<(), CommandError> {
    let mut fresh: ActivityScans = Vec::new();
    let mut scan = |doc: &Arc<BpmnDocument>| {
        let result = scans
            .iter()
            .chain(&fresh)
            .find(|(cached, _)| Arc::ptr_eq(cached, doc))
            .map(|(_, result)| result.clone())
            .unwrap_or_else(|| Arc::new(crate::validation::activity_family_reference_scan(doc)));
        fresh.push((doc.clone(), result.clone()));
        result
    };
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|doc| doc.id == after.id) else {
            // A document created in this batch has no earlier state: every
            // unresolved local activity reference is new.
            if !crate::validation::activity_family_reference_issues(after).is_empty() {
                return Err(invalid(
                    "change introduces an unresolved local activity reference",
                ));
            }
            continue;
        };
        if Arc::ptr_eq(before, after) {
            continue;
        }
        let (before_scan, after_scan) = (scan(before), scan(after));
        // Unchanged references and no identity renames (e.g. a label edit): nothing
        // new can be unresolved or retargeted.
        if before_scan == after_scan
            && !mappings
                .iter()
                .any(|mapping| mapping.document_id == after.id)
        {
            continue;
        }
        let (mut previous, mut old_resolutions) = (*before_scan).clone();
        let (next, new_resolutions) = (*after_scan).clone();
        for issue in &mut previous {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == after.id)
            {
                if issue.owner == mapping.from {
                    issue.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) = &mut issue.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
            }
        }
        if introduces_new(&previous, &next) {
            return Err(invalid(
                "change introduces an unresolved local activity reference",
            ));
        }
        for resolution in &mut old_resolutions {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == after.id)
            {
                if resolution.reference.owner == mapping.from {
                    resolution.reference.owner = mapping.to.clone();
                }
                if let crate::bpmn_model::EventReference::Local(target) =
                    &mut resolution.reference.target
                    && *target == mapping.from
                {
                    *target = mapping.to.clone();
                }
                if resolution.resolved_target == mapping.from {
                    resolution.resolved_target = mapping.to.clone();
                }
            }
        }
        let old_references = counts(old_resolutions.iter().map(|r| &r.reference));
        let new_references = counts(new_resolutions.iter().map(|r| &r.reference));
        let old_targets = counts(old_resolutions.iter());
        let new_targets = counts(new_resolutions.iter());
        for old_resolution in &old_resolutions {
            let old_count = old_references[&old_resolution.reference];
            let new_count = new_references
                .get(&old_resolution.reference)
                .copied()
                .unwrap_or(0);
            if new_count >= old_count
                && new_count > 0
                && old_targets[old_resolution]
                    > new_targets.get(old_resolution).copied().unwrap_or(0)
            {
                return Err(invalid("change would retarget a local activity reference"));
            }
        }
    }
    // Keep one scan per candidate document; the clones keep their `Arc`s alive.
    let mut kept: ActivityScans = Vec::new();
    for (doc, result) in fresh.into_iter().chain(std::mem::take(scans)) {
        if candidate.documents.iter().any(|d| Arc::ptr_eq(d, &doc))
            && !kept.iter().any(|(k, _)| Arc::ptr_eq(k, &doc))
        {
            kept.push((doc, result));
        }
    }
    *scans = kept;
    Ok(())
}
/// Drops every project-level reference to removed elements of one document:
/// navigation links, dictionary links and attribute values. The single helper
/// for all delete paths; `removed: None` removes every reference to the
/// document (`deleteDocument`).
fn prune_removed_element_refs(
    project: &mut ProjectData,
    document_id: DocumentId,
    removed: Option<&std::collections::HashSet<ElementSelector>>,
) {
    let keep = |source: &ElementKey| match removed {
        None => source.document_id != document_id,
        Some(set) => source.document_id != document_id || !set.contains(&source.xml_id),
    };
    project.links.retain(|link| keep(&link.source));
    project.dictionary_links.retain(|link| keep(&link.source));
    project.attribute_values.retain(|value| keep(&value.source));
}
fn prune_removed_activity_navigation(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
) {
    for before in &old.documents {
        let Some(after) = candidate
            .documents
            .iter_mut()
            .find(|doc| doc.id == before.id)
        else {
            continue;
        };
        let before_ids = crate::identity::semantic_ids(before);
        let after_ids = crate::identity::semantic_ids(after);
        let removed: std::collections::HashSet<_> = before_ids
            .difference(&after_ids)
            .filter(|id| {
                !mappings
                    .iter()
                    .any(|mapping| mapping.document_id == before.id && &mapping.from == *id)
            })
            .cloned()
            .collect();
        if removed.is_empty() {
            continue;
        }
        let document = Arc::make_mut(after);
        for diagram in &mut document.diagrams {
            diagram
                .shapes
                .retain(|shape| !removed.contains(&shape.element));
            diagram
                .edges
                .retain(|edge| !removed.contains(&edge.element));
        }
        prune_removed_element_refs(candidate, before.id, Some(&removed));
    }
}
fn reconcile_root_orders(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
    explicit: &[DocumentId],
) -> Result<(), CommandError> {
    for document in &mut candidate.documents {
        if explicit.contains(&document.id)
            || document.root_order.is_none()
            || !old
                .documents
                .iter()
                .any(|before| before.id == document.id && before.root_order.is_some())
        {
            continue;
        }
        let available: Vec<_> = document
            .root_element_identities()
            .into_iter()
            .map(|identity| {
                identity.map(|id| id.selector()).ok_or_else(|| {
                    invalid("promote anonymous roots before adding them to an ordered document")
                })
            })
            .collect::<Result<_, _>>()?;
        let mut order = document.root_order.clone().unwrap_or_default();
        for id in &mut order {
            for mapping in mappings
                .iter()
                .filter(|mapping| mapping.document_id == document.id)
            {
                if *id == mapping.from {
                    *id = mapping.to.clone();
                }
            }
        }
        order.retain(|id| available.contains(id));
        for id in available {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        if document.root_order.as_ref() != Some(&order) {
            Arc::make_mut(document).root_order = Some(order);
        }
    }
    Ok(())
}
fn reconcile_plane_orders(
    old: &ProjectData,
    candidate: &mut ProjectData,
    mappings: &[SelectorMapping],
    explicit: &[(DocumentId, ElementSelector)],
) {
    let remap = |document_id, selector: &ElementSelector| {
        mappings
            .iter()
            .filter(|mapping| mapping.document_id == document_id)
            .fold(selector.clone(), |current, mapping| {
                if current == mapping.from {
                    mapping.to.clone()
                } else {
                    current
                }
            })
    };
    for document in &mut candidate.documents {
        let Some(previous) = old.documents.iter().find(|old| old.id == document.id) else {
            continue;
        };
        let document_id = document.id;
        let is_explicit = |diagram: &Diagram| {
            explicit.iter().any(|(id, selector)| {
                *id == document_id && remap(document_id, selector) == diagram.id.selector()
            })
        };
        let needs_update = document.diagrams.iter().any(|diagram| {
            if is_explicit(diagram) {
                return false;
            }
            let old_diagram = previous
                .diagrams
                .iter()
                .find(|old| remap(document_id, &old.id.selector()) == diagram.id.selector());
            let Some(order) = diagram
                .plane_order
                .as_ref()
                .filter(|_| old_diagram.is_some_and(|old| old.plane_order.is_some()))
            else {
                return false;
            };
            let available: Vec<_> = diagram
                .shapes
                .iter()
                .map(|shape| shape.id.selector())
                .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
                .collect();
            order.iter().any(|id| !available.contains(id))
                || available.iter().any(|id| !order.contains(id))
        });
        if !needs_update {
            continue;
        }
        let document = Arc::make_mut(document);
        for diagram in &mut document.diagrams {
            if is_explicit(diagram) {
                continue;
            }
            let old_diagram = previous
                .diagrams
                .iter()
                .find(|old| remap(document_id, &old.id.selector()) == diagram.id.selector());
            if !old_diagram.is_some_and(|old| old.plane_order.is_some()) {
                continue;
            }
            let Some(order) = &mut diagram.plane_order else {
                continue;
            };
            let available: Vec<_> = diagram
                .shapes
                .iter()
                .map(|shape| shape.id.selector())
                .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
                .collect();
            order.retain(|id| available.contains(id));
            for id in available {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
        }
    }
}
fn reject_newly_dangling_relationships(
    old: &ProjectData,
    candidate: &ProjectData,
) -> Result<(), CommandError> {
    for after in &candidate.documents {
        let Some(before) = old.documents.iter().find(|doc| doc.id == after.id) else {
            continue;
        };
        let namespace = before
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.as_str())
            .or_else(|| {
                before
                    .provenance
                    .is_none()
                    .then_some(GENERATED_TARGET_NAMESPACE)
            });
        let after_namespace = after
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.as_str())
            .or_else(|| {
                after
                    .provenance
                    .is_none()
                    .then_some(GENERATED_TARGET_NAMESPACE)
            });
        if namespace.is_none() || namespace != after_namespace {
            continue;
        }
        let old_ids: std::collections::HashMap<_, _> = before
            .element_identities()
            .into_iter()
            .filter_map(|id| id.xml_id().map(|xml| (xml, id.selector())))
            .collect();
        let new_ids: std::collections::HashMap<_, _> = after
            .element_identities()
            .into_iter()
            .filter_map(|id| id.xml_id().map(|xml| (xml, id.selector())))
            .collect();
        if after
            .relationships
            .iter()
            .flat_map(|relationship| relationship.sources.iter().chain(&relationship.targets))
            .any(|reference| {
                Some(reference.namespace.as_str()) == namespace
                    && old_ids
                        .get(&reference.local)
                        .is_some_and(|old| new_ids.get(&reference.local) != Some(old))
            })
        {
            return Err(invalid(
                "change would leave a local relationship reference dangling or retargeted",
            ));
        }
    }
    Ok(())
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
fn documentation_is_structured(d: &BpmnDocument, id: &ElementSelector) -> bool {
    use crate::bpmn_metadata::DocumentationSet;
    d.processes
        .iter()
        .any(|p| p.id == *id && matches!(p.documentation, DocumentationSet::Structured(_)))
        || crate::queries::scopes(d).iter().any(|scope| {
            scope
                .nodes
                .iter()
                .any(|n| n.id == *id && matches!(n.documentation, DocumentationSet::Structured(_)))
                || scope.flows.iter().any(|f| {
                    f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_))
                })
        })
        || d.collaboration.as_ref().is_some_and(|c| {
            c.flows
                .iter()
                .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
        })
        || d.additional_collaborations
            .iter()
            .flat_map(|root| &root.flows)
            .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
        || d.choreographies
            .iter()
            .flat_map(|c| c.sequence_flows.iter().chain(&c.message_flows))
            .any(|f| f.id == *id && matches!(f.documentation, DocumentationSet::Structured(_)))
}
fn replace_nested_documentation(
    nodes: &mut [Node],
    id: &ElementSelector,
    documentation: &crate::bpmn_metadata::DocumentationSet,
) -> bool {
    for node in nodes {
        if node.id == *id {
            node.documentation = documentation.clone();
            return true;
        }
        if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &mut node.details {
            if let Some(flow) = content.flows.iter_mut().find(|flow| flow.id == *id) {
                flow.documentation = documentation.clone();
                return true;
            }
            if replace_nested_documentation(&mut content.nodes, id, documentation) {
                return true;
            }
        }
    }
    false
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
fn local_operation_exists(
    d: &BpmnDocument,
    reference: &crate::bpmn_model::QNameRef,
    catalog: &crate::bpmn_catalog::SupportingCatalog,
) -> bool {
    let namespace = d
        .definitions
        .as_ref()
        .map(|m| m.target_namespace.as_str())
        .or_else(|| d.provenance.is_none().then_some(GENERATED_TARGET_NAMESPACE));
    namespace != Some(reference.namespace.as_str())
        || catalog
            .interfaces
            .iter()
            .flat_map(|i| &i.operations)
            .any(|o| {
                o.id.as_ref()
                    .is_some_and(|id| id.xml_id() == Some(&reference.local))
            })
}
fn local_message_exists(
    d: &BpmnDocument,
    reference: &crate::bpmn_model::QNameRef,
    resources: &[crate::bpmn_model::EventResource],
) -> bool {
    let namespace = d
        .definitions
        .as_ref()
        .map(|m| m.target_namespace.as_str())
        .or_else(|| d.provenance.is_none().then_some(GENERATED_TARGET_NAMESPACE));
    namespace != Some(reference.namespace.as_str()) || resources.iter().any(|resource| matches!(resource, crate::bpmn_model::EventResource::Message { id, .. } if id.xml_id() == Some(&reference.local)))
}
fn check_activity_references(
    d: &BpmnDocument,
    activity: &crate::bpmn_activity::ActivityProperties,
    previous: Option<&crate::bpmn_activity::ActivityProperties>,
) -> Result<(), CommandError> {
    let issues = crate::validation::activity_reference_issues(d, activity);
    for (current, old) in [
        (
            &activity.message_ref,
            previous.and_then(|p| p.message_ref.as_ref()),
        ),
        (
            &activity.operation_ref,
            previous.and_then(|p| p.operation_ref.as_ref()),
        ),
    ] {
        if let Some(name) = current
            && Some(name) != old
            && crate::validation::invalid_activity_namespace(&name.namespace)
        {
            return Err(invalid("Invalid activity QName namespace"));
        }
    }
    if issues.contains(&"missingLocalActivityMessage")
        && previous.map(|p| &p.message_ref) != Some(&activity.message_ref)
    {
        return Err(invalid("Activity references a missing local message"));
    }
    if issues.contains(&"missingLocalActivityOperation")
        && previous.map(|p| &p.operation_ref) != Some(&activity.operation_ref)
    {
        return Err(invalid("Activity references a missing local operation"));
    }
    Ok(())
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
    if structural && d.blocks_structural_edits() {
        return Err(invalid("opaque XML prevents safe structural edits"));
    }
    Ok(Arc::make_mut(d))
}
fn nested_scope_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
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
pub(crate) type ScopePartsMut<'a> = (&'a mut Vec<Node>, &'a mut Vec<Lane>, &'a mut Vec<Flow>);
pub(crate) fn scope_parts_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
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
fn nested_node_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
    depth: usize,
) -> Option<&'a mut Node> {
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
pub(crate) fn node_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Node, CommandError> {
    for process in &mut document.processes {
        if let Some(node) = nested_node_mut(&mut process.nodes, id, 0) {
            return Ok(node);
        }
    }
    use crate::bpmn_collaboration::CollaborationArtifact;
    for extras in document
        .collaboration
        .iter_mut()
        .chain(&mut document.additional_collaborations)
        .filter_map(|root| root.extras.as_mut())
        .chain(
            document
                .choreographies
                .iter_mut()
                .filter_map(|root| root.extras.as_mut()),
        )
    {
        for artifact in &mut extras.artifacts {
            if let CollaborationArtifact::TextAnnotation(node) = artifact
                && &node.id == id
            {
                return Ok(node);
            }
        }
    }
    Err(invalid("node not found"))
}
fn edit_nested(
    nodes: &mut [Node],
    id: &ElementSelector,
    text: Option<&str>,
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
                        child.documentation = text.unwrap_or_default().into();
                    } else {
                        child.name = text.map(str::to_owned);
                    }
                    return true;
                }
            }
            if !documentation {
                for lane in &mut content.lanes {
                    if &lane.id == id {
                        lane.name = text.map(str::to_owned);
                        return true;
                    }
                }
            }
            for flow in &mut content.flows {
                if &flow.id == id {
                    if documentation {
                        flow.documentation = text.unwrap_or_default().into();
                    } else {
                        flow.name = text.map(str::to_owned);
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
fn nested_flow_mut<'a>(
    nodes: &'a mut [Node],
    id: &ElementSelector,
    depth: usize,
) -> Option<&'a mut Flow> {
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
fn collect_association_ids(
    association: &crate::bpmn_data::DataAssociation,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    collect_base_documentation_ids(association.base.as_ref(), ids);
    if let Some(id) = &association.id {
        ids.insert(id.selector());
    }
    if let Some(value) = &association.transformation {
        collect_expression_ids(value, ids);
    }
    for assignment in &association.assignments {
        collect_base_documentation_ids(assignment.base.as_ref(), ids);
        if let Some(id) = &assignment.id {
            ids.insert(id.selector());
        }
        collect_expression_ids(&assignment.from, ids);
        collect_expression_ids(&assignment.to, ids);
    }
}
fn collect_expression_ids(
    expression: &crate::bpmn_model::Expression,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    if let crate::bpmn_model::Expression::Structured(data) = expression {
        if let Some(id) = &data.id {
            ids.insert(id.selector());
        }
        if let crate::bpmn_model::ExpressionBody::Mixed { parts } = &data.text {
            for part in parts {
                if let crate::bpmn_model::ExpressionPart::Documentation { documentation } = part
                    && let Some(id) = &documentation.id
                {
                    ids.insert(id.selector());
                }
            }
        }
    }
}
fn collect_base_documentation_ids(
    base: Option<&crate::bpmn_metadata::BaseMetadata>,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    if let Some(base) = base {
        for documentation in &base.documentation {
            if let Some(id) = &documentation.id {
                ids.insert(id.selector());
            }
        }
    }
}
fn collect_node_ids(
    node: &Node,
    ids: &mut std::collections::HashSet<ElementSelector>,
    depth: usize,
) {
    ids.insert(node.id.selector());
    let activity = crate::activity_read::node_activity_read(node);
    ids.extend(activity.ids.iter().map(|id| id.selector()));
    for base in activity.bases {
        collect_base_documentation_ids(Some(base), ids);
    }
    for expression in activity.expressions {
        collect_expression_ids(expression, ids);
    }
    if let crate::bpmn_metadata::DocumentationSet::Structured(items) = &node.documentation {
        for item in items {
            if let Some(id) = &item.id {
                ids.insert(id.selector());
            }
        }
    }
    let mut collect_meta = |metadata: &Option<crate::bpmn_data::ItemAwareMetadata>| {
        if let Some(id) = metadata
            .as_ref()
            .and_then(|m| m.data_state.as_ref())
            .and_then(|s| s.id.as_ref())
        {
            ids.insert(id.selector());
        }
    };
    collect_meta(&node.item_aware);
    if let Some(value) = &node.activity
        && let Some(expression) = &value.completion_condition
    {
        collect_expression_ids(expression, ids);
    }
    if let Some(characteristics) = &node.loop_characteristics {
        if let Some(id) = characteristics.id() {
            ids.insert(id.selector());
        }
        collect_base_documentation_ids(characteristics.base(), ids);
        match characteristics {
            crate::bpmn_model::LoopCharacteristics::Standard { condition, .. } => {
                if let Some(expression) = condition {
                    collect_expression_ids(expression, ids);
                }
            }
            crate::bpmn_model::LoopCharacteristics::MultiInstance {
                cardinality,
                completion_condition,
                ..
            } => {
                if let Some(expression) = cardinality {
                    collect_expression_ids(expression, ids);
                }
                if let Some(expression) = completion_condition {
                    collect_expression_ids(expression, ids);
                }
            }
        }
    }
    if let Some(crate::bpmn_model::NodeDetails::Gateway {
        activation_condition: Some(expression),
        ..
    }) = &node.details
    {
        collect_expression_ids(expression, ids);
    }
    if let Some(crate::bpmn_model::NodeDetails::Event { properties, .. }) = &node.details {
        for property in properties {
            collect_base_documentation_ids(property.base.as_ref(), ids);
            if let Some(id) = &property.id {
                ids.insert(id.selector());
            }
            if let Some(id) = property
                .item_aware
                .as_ref()
                .and_then(|m| m.data_state.as_ref())
                .and_then(|s| s.id.as_ref())
            {
                ids.insert(id.selector());
            }
        }
    }
    if let Some(io) = &node.io_specification {
        collect_base_documentation_ids(io.base.as_ref(), ids);
        if let Some(id) = &io.id {
            ids.insert(id.selector());
        }
        ids.extend(
            io.inputs
                .iter()
                .chain(&io.outputs)
                .filter_map(|p| p.id.as_ref().map(|id| id.selector())),
        );
        for port in io.inputs.iter().chain(&io.outputs) {
            collect_base_documentation_ids(port.base.as_ref(), ids);
        }
        for set in io.input_sets.iter().chain(&io.output_sets) {
            collect_base_documentation_ids(set.base.as_ref(), ids);
        }
        ids.extend(
            io.input_sets
                .iter()
                .chain(&io.output_sets)
                .filter_map(|set| set.id.as_ref().map(|id| id.selector())),
        );
        ids.extend(io.inputs.iter().chain(&io.outputs).filter_map(|port| {
            port.item_aware
                .as_ref()
                .and_then(|m| m.data_state.as_ref())
                .and_then(|s| s.id.as_ref())
                .map(|id| id.selector())
        }));
    }
    if let Some(crate::bpmn_model::NodeDetails::Event {
        definitions, io, ..
    }) = &node.details
    {
        for definition in definitions {
            collect_base_documentation_ids(definition.base(), ids);
            if let Some(id) = definition.id() {
                ids.insert(id.selector());
            }
            let expression = match definition {
                crate::bpmn_model::EventDefinition::Conditional { expression, .. } => {
                    Some(expression)
                }
                crate::bpmn_model::EventDefinition::Timer { expression, .. } => expression.as_ref(),
                _ => None,
            };
            if let Some(expression) = expression {
                collect_expression_ids(expression, ids);
            }
        }
        if let Some(io) = io {
            let (ports, set) = match io {
                crate::bpmn_model::EventIo::Catch {
                    outputs,
                    output_set,
                } => (outputs, output_set),
                crate::bpmn_model::EventIo::Throw { inputs, input_set } => (inputs, input_set),
            };
            for port in ports {
                collect_base_documentation_ids(port.base.as_ref(), ids);
                if let Some(id) = &port.id {
                    ids.insert(id.selector());
                }
                if let Some(id) = port
                    .item_aware
                    .as_ref()
                    .and_then(|m| m.data_state.as_ref())
                    .and_then(|s| s.id.as_ref())
                {
                    ids.insert(id.selector());
                }
            }
            if let Some(id) = set.as_ref().and_then(|s| s.id.as_ref()) {
                ids.insert(id.selector());
            }
            if let Some(set) = set {
                collect_base_documentation_ids(set.base.as_ref(), ids);
            }
        }
    }
    if depth >= 32 {
        return;
    }
    if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
        for child in &content.nodes {
            collect_node_ids(child, ids, depth + 1);
        }
        ids.extend(content.lanes.iter().map(|l| l.id.selector()));
        ids.extend(content.flows.iter().map(|f| f.id.selector()));
        ids.extend(content.groups.iter().map(|g| g.id.selector()));
        collect_base_documentation_ids(content.lane_set_base.as_ref(), ids);
        for lane in &content.lanes {
            collect_base_documentation_ids(lane.base.as_ref(), ids);
            collect_base_documentation_ids(lane.child_lane_set_base.as_ref(), ids);
        }
        for group in &content.groups {
            collect_base_documentation_ids(group.base.as_ref(), ids);
        }
        for object in &content.data_objects {
            collect_base_documentation_ids(object.base.as_ref(), ids);
        }
        for flow in &content.flows {
            if let Some(expression) = &flow.condition {
                collect_expression_ids(expression, ids);
            }
            if let crate::bpmn_metadata::DocumentationSet::Structured(items) = &flow.documentation {
                for item in items {
                    if let Some(id) = &item.id {
                        ids.insert(id.selector());
                    }
                }
            }
        }
        if let Some(id) = &content.lane_set_id {
            ids.insert(id.selector());
        }
        ids.extend(
            content
                .lanes
                .iter()
                .filter_map(|lane| lane.child_lane_set_id.as_ref().map(|id| id.selector())),
        );
        ids.extend(
            content
                .data_objects
                .iter()
                .filter_map(|o| o.id.as_ref().map(|id| id.selector())),
        );
        for association in &content.data_associations {
            collect_association_ids(association, ids);
        }
    }
}
/// Clear gateway and activity default flows that point at deleted flows.
fn clear_removed_defaults(
    nodes: &mut [Node],
    removed: &std::collections::HashSet<ElementSelector>,
) {
    for node in nodes {
        if let Some(activity) = &mut node.activity
            && activity
                .default_flow
                .as_ref()
                .is_some_and(|flow| removed.contains(flow))
        {
            activity.default_flow = None;
        }
        match &mut node.details {
            Some(crate::bpmn_model::NodeDetails::Gateway { default_flow, .. })
                if default_flow
                    .as_ref()
                    .is_some_and(|flow| removed.contains(flow)) =>
            {
                *default_flow = None;
            }
            Some(crate::bpmn_model::NodeDetails::SubProcess { content }) => {
                clear_removed_defaults(&mut content.nodes, removed);
            }
            _ => {}
        }
    }
}
fn delete_nested(
    nodes: &mut Vec<Node>,
    lanes: &mut Vec<Lane>,
    flows: &mut Vec<Flow>,
    associations: &mut Vec<crate::bpmn_data::DataAssociation>,
    id: &ElementSelector,
    removed: &mut std::collections::HashSet<ElementSelector>,
    depth: usize,
) -> bool {
    if depth > 32 {
        return false;
    }
    if let Some(index) = nodes.iter().position(|n| &n.id == id) {
        collect_node_ids(&nodes[index], removed, depth);
        nodes.remove(index);
        associations.retain(|association| {
            if association.owner == *id {
                collect_association_ids(association, removed);
                false
            } else {
                true
            }
        });
        for lane in lanes {
            lane.members.retain(|member| member != id);
        }
        return true;
    }
    if lanes.iter().any(|l| &l.id == id) {
        // A lane goes with its child lanes at any depth (bpmn-js removes the lane's children).
        let mut doomed = vec![id.clone()];
        for _ in 0..32 {
            let before = doomed.len();
            for lane in lanes.iter() {
                if lane.parent.as_ref().is_some_and(|p| doomed.contains(p))
                    && !doomed.contains(&lane.id.selector())
                {
                    doomed.push(lane.id.selector());
                }
            }
            if doomed.len() == before {
                break;
            }
        }
        for lane in lanes.iter().filter(|l| doomed.contains(&l.id.selector())) {
            collect_base_documentation_ids(lane.base.as_ref(), removed);
            collect_base_documentation_ids(lane.child_lane_set_base.as_ref(), removed);
            if let Some(child) = &lane.child_lane_set_id {
                removed.insert(child.selector());
            }
        }
        lanes.retain(|l| !doomed.contains(&l.id.selector()));
        removed.extend(doomed);
        return true;
    }
    if let Some(index) = flows.iter().position(|f| &f.id == id) {
        if let Some(expression) = &flows[index].condition {
            collect_expression_ids(expression, removed);
        }
        if let crate::bpmn_metadata::DocumentationSet::Structured(items) =
            &flows[index].documentation
        {
            for item in items {
                if let Some(id) = &item.id {
                    removed.insert(id.selector());
                }
            }
        }
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
                &mut content.data_associations,
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
    id: &ElementSelector,
) -> Result<&'a mut crate::bpmn_collaboration::Choreography, CommandError> {
    document
        .choreographies
        .iter_mut()
        .find(|c| &c.id == id)
        .ok_or_else(|| invalid("choreography not found"))
}

fn choreography_element_id(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
) -> ElementSelector {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    match element {
        ChoreographyFlowElement::Activity(a) => a.id.selector(),
        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => n.id.selector(),
        ChoreographyFlowElement::SequenceFlow(f) => f.id.selector(),
    }
}
fn choreography_artifact_id(
    artifact: &crate::bpmn_collaboration::CollaborationArtifact,
) -> ElementSelector {
    use crate::bpmn_collaboration::CollaborationArtifact;
    match artifact {
        CollaborationArtifact::Group(g) => g.id.selector(),
        CollaborationArtifact::TextAnnotation(n) => n.id.selector(),
        CollaborationArtifact::Association(f) => f.id.selector(),
    }
}
fn choreography_artifacts_mut<'a>(
    c: &'a mut crate::bpmn_collaboration::Choreography,
    scope: &ElementSelector,
    create_root: bool,
) -> Result<&'a mut Vec<crate::bpmn_collaboration::CollaborationArtifact>, CommandError> {
    fn nested<'a>(
        graph: &'a mut crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
    ) -> Option<&'a mut Vec<crate::bpmn_collaboration::CollaborationArtifact>> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &mut graph.flow_elements {
            if let ChoreographyFlowElement::Activity(activity) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) =
                    &mut activity.details
            {
                if activity.id == *scope {
                    return Some(artifacts);
                }
                if let Some(found) = nested(graph, scope) {
                    return Some(found);
                }
            }
        }
        None
    }
    if c.id == *scope {
        if create_root {
            return Ok(&mut c.extras.get_or_insert_with(Default::default).artifacts);
        }
        return c
            .extras
            .as_mut()
            .map(|e| &mut e.artifacts)
            .ok_or_else(|| invalid("choreography artifact not found"));
    }
    nested(
        c.graph
            .as_mut()
            .ok_or_else(|| invalid("choreography scope not found"))?,
        scope,
    )
    .ok_or_else(|| invalid("choreography scope not found"))
}

fn choreography_subtree_ids(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
    ids: &mut std::collections::HashSet<ElementSelector>,
) {
    use crate::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, CollaborationArtifact,
    };
    ids.insert(choreography_element_id(element));
    if let ChoreographyFlowElement::Activity(activity) = element
        && let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) = &activity.details
    {
        for child in &graph.flow_elements {
            choreography_subtree_ids(child, ids);
        }
        for association in &graph.data_associations {
            if let Some(id) = &association.id {
                ids.insert(id.selector());
            }
        }
        for artifact in artifacts {
            ids.insert(match artifact {
                CollaborationArtifact::Group(group) => group.id.selector(),
                CollaborationArtifact::TextAnnotation(node) => node.id.selector(),
                CollaborationArtifact::Association(flow) => flow.id.selector(),
            });
        }
    }
}

// A narrow geometry-preserving move into an expanded Sub is safe when every view proves a
// unique containing representation and no connected DI edge needs rerouting. All coordinates,
// labels and participant bands remain untouched.
fn choreography_reparent_geometry_safe(
    document: &BpmnDocument,
    root: &ElementSelector,
    source: &ElementSelector,
    target: &ElementSelector,
    moved: &ElementSelector,
    subtree: &std::collections::HashSet<ElementSelector>,
) -> bool {
    let scopes = crate::queries::choreography_scopes(document);
    let scope_ids: std::collections::HashSet<_> = scopes
        .iter()
        .filter(|s| s.root.id == *root && s.owner != *root)
        .map(|s| s.owner.clone())
        .collect();
    let contains = |outer: &Bounds, inner: &Bounds| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.width <= outer.x + outer.width
            && inner.y + inner.height <= outer.y + outer.height
    };
    for diagram in &document.diagrams {
        let represented: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| subtree.contains(&s.element))
            .collect();
        if represented.is_empty() && !diagram.edges.iter().any(|e| subtree.contains(&e.element)) {
            continue;
        }
        if *source != *root
            || subtree.len() != 1
            || diagram.plane != *root
            || represented.len() != 1
            || represented[0].element != *moved
            || diagram.edges.iter().any(|e| subtree.contains(&e.element))
        {
            return false;
        }
        let shape = represented[0];
        let targets: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| s.element == *target)
            .collect();
        if targets.len() != 1
            || targets[0].is_expanded != Some(true)
            || !contains(&targets[0].bounds, &shape.bounds)
        {
            return false;
        }
        if diagram.shapes.iter().any(|s| {
            scope_ids.contains(&s.element)
                && s.element != *target
                && s.element != *moved
                && contains(&s.bounds, &shape.bounds)
        }) {
            return false;
        }
        if diagram.edges.iter().any(|e| {
            e.source_element.as_ref() == Some(&shape.id.selector())
                || e.target_element.as_ref() == Some(&shape.id.selector())
        }) {
            return false;
        }
        if diagram.shapes.iter().any(|band| matches!(&band.choreography_activity_shape, Some(DiReference::Local(owner)) if *owner == shape.id.selector()) && !contains(&targets[0].bounds, &band.bounds)) { return false; }
        let namespace = document
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.as_str())
            .or_else(|| {
                document
                    .provenance
                    .is_none()
                    .then_some(GENERATED_TARGET_NAMESPACE)
            });
        if diagram.shapes.iter().any(|band| {
            match (&band.choreography_activity_shape, shape.id.xml_id()) {
                (Some(DiReference::Qualified(reference)), Some(id)) if &reference.local == id => {
                    namespace.is_none_or(|ns| ns == reference.namespace)
                        && !contains(&targets[0].bounds, &band.bounds)
                }
                _ => false,
            }
        }) {
            return false;
        }
    }
    true
}

fn choreography_scope_mut<'a>(
    c: &'a mut crate::bpmn_collaboration::Choreography,
    scope: &ElementSelector,
) -> Result<&'a mut crate::bpmn_collaboration::ChoreographyGraph, CommandError> {
    fn nested<'a>(
        graph: &'a mut crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
    ) -> Option<&'a mut crate::bpmn_collaboration::ChoreographyGraph> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &mut graph.flow_elements {
            if let ChoreographyFlowElement::Activity(activity) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &mut activity.details
            {
                if activity.id == *scope {
                    return Some(graph);
                }
                if let Some(found) = nested(graph, scope) {
                    return Some(found);
                }
            }
        }
        None
    }
    let root = c.id == *scope;
    let graph = c
        .graph
        .as_mut()
        .ok_or_else(|| invalid("scoped graph command requires a promoted graph"))?;
    if root {
        Ok(graph)
    } else {
        nested(graph, scope).ok_or_else(|| invalid("choreography scope not found"))
    }
}
fn choreography_element_identity(
    element: &crate::bpmn_collaboration::ChoreographyFlowElement,
) -> crate::ids::ElementIdentity {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    match element {
        ChoreographyFlowElement::Activity(a) => a.id.clone(),
        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => n.id.clone(),
        ChoreographyFlowElement::SequenceFlow(f) => f.id.clone(),
    }
}
/// Scope that directly contains a graph-mode flow element, searching nested Sub graphs.
fn choreography_graph_owner(
    document: &BpmnDocument,
    id: &ElementSelector,
) -> Option<(ElementSelector, ElementSelector)> {
    fn walk(
        graph: &crate::bpmn_collaboration::ChoreographyGraph,
        scope: &ElementSelector,
        id: &ElementSelector,
    ) -> Option<ElementSelector> {
        use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
        for element in &graph.flow_elements {
            if choreography_element_id(element) == *id {
                return Some(scope.clone());
            }
            if let ChoreographyFlowElement::Activity(a) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &a.details
                && let Some(found) = walk(graph, &a.id.selector(), id)
            {
                return Some(found);
            }
        }
        None
    }
    document.choreographies.iter().find_map(|c| {
        walk(c.graph.as_ref()?, &c.id.selector(), id).map(|scope| (c.id.selector(), scope))
    })
}
fn rename_choreography_element(
    graph: &mut crate::bpmn_collaboration::ChoreographyGraph,
    id: &ElementSelector,
    name: &Option<String>,
) -> bool {
    use crate::bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement};
    let mut found = false;
    for element in &mut graph.flow_elements {
        match element {
            ChoreographyFlowElement::Activity(a) => {
                if a.id == *id {
                    a.name = name.clone();
                    found = true;
                }
                if let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &mut a.details {
                    found |= rename_choreography_element(graph, id, name);
                }
            }
            ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                if n.id == *id {
                    n.name = name.clone();
                    found = true;
                }
            }
            ChoreographyFlowElement::SequenceFlow(f) => {
                if f.id == *id {
                    f.name = name.clone();
                    found = true;
                }
            }
        }
    }
    found
}
/// Removes an element plus what cannot outlive it: connected sequence flows, events attached to
/// it, and data associations it owns (recursively for removed dependents).
fn cascade_choreography_removal(
    graph: &mut crate::bpmn_collaboration::ChoreographyGraph,
    element_id: &ElementSelector,
    namespace: Option<&str>,
) -> Result<(), CommandError> {
    use crate::bpmn_collaboration::ChoreographyFlowElement;
    use crate::bpmn_model::NodeDetails;
    fn depends(
        element: &ChoreographyFlowElement,
        doomed: &[crate::ids::ElementIdentity],
        namespace: Option<&str>,
    ) -> bool {
        let hits = |r: &crate::bpmn_model::EventReference| {
            doomed.iter().any(|d| r.matches_identity(d, namespace))
        };
        match element {
            ChoreographyFlowElement::SequenceFlow(f) => {
                [&f.source, &f.target].into_iter().flatten().any(hits)
            }
            ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                match &n.details {
                    Some(NodeDetails::Event {
                        attached_to: Some(r),
                        ..
                    })
                    | Some(NodeDetails::BoundaryEvent { attached_to: r, .. }) => hits(r),
                    _ => false,
                }
            }
            ChoreographyFlowElement::Activity(_) => false,
        }
    }
    let mut doomed = vec![
        graph
            .flow_elements
            .iter()
            .find(|e| choreography_element_id(e) == *element_id)
            .map(choreography_element_identity)
            .ok_or_else(|| invalid("choreography flow element not found"))?,
    ];
    while let Some(next) = graph
        .flow_elements
        .iter()
        .find(|e| {
            !doomed.iter().any(|d| *d == choreography_element_id(e))
                && depends(e, &doomed, namespace)
        })
        .map(choreography_element_identity)
    {
        doomed.push(next);
    }
    graph
        .flow_elements
        .retain(|e| !doomed.iter().any(|d| *d == choreography_element_id(e)));
    graph
        .data_associations
        .retain(|a| !doomed.iter().any(|d| *d == a.owner));
    Ok(())
}
fn delete_choreography_flow_element(
    p: &mut ProjectData,
    document_id: DocumentId,
    choreography_id: &ElementSelector,
    scope_id: &ElementSelector,
    element_id: &ElementSelector,
) -> Result<(), CommandError> {
    let document = doc(p, document_id, true)?;
    let before: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.clone())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then(|| GENERATED_TARGET_NAMESPACE.to_owned())
        });
    let graph = choreography_scope_mut(choreography_mut(document, choreography_id)?, scope_id)?;
    cascade_choreography_removal(graph, element_id, namespace.as_deref())?;
    let after: std::collections::HashSet<_> = document
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    let removed: std::collections::HashSet<_> = before.difference(&after).cloned().collect();
    for diagram in &mut document.diagrams {
        let owners = diagram
            .shapes
            .iter()
            .filter(|shape| removed.contains(&shape.element))
            .map(|shape| shape.id.selector())
            .collect();
        remove_di_shapes(diagram, &owners, namespace.as_deref());
        diagram
            .edges
            .retain(|edge| !removed.contains(&edge.element));
    }
    prune_removed_element_refs(p, document_id, Some(&removed));
    Ok(())
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
    id: &ElementSelector,
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
    id: &ElementSelector,
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
    id: &ElementSelector,
) -> Result<&'a mut Option<crate::ids::ElementIdentity>, CommandError> {
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
fn lane_set_name_mut<'a>(
    document: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Option<String>, CommandError> {
    if let Some(index) = document.processes.iter().position(|p| &p.id == id) {
        return Ok(&mut document.processes[index].lane_set_name);
    }
    for process in &mut document.processes {
        if let Some(scope) = nested_scope_mut(&mut process.nodes, id, 0) {
            return Ok(&mut scope.lane_set_name);
        }
    }
    Err(invalid("process or subprocess not found"))
}
/// tLaneSet@name is an xsd:string: only XML 1.0 characters, bounded length.
fn check_lane_set_name(name: &Option<String>) -> Result<(), CommandError> {
    match name {
        Some(n)
            if n.len() > 16_384
                || n.chars().any(|c| {
                    !matches!(c as u32, 9 | 10 | 13 | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
                }) =>
        {
            Err(invalid("invalid lane set name"))
        }
        _ => Ok(()),
    }
}
fn flow_mut<'a>(
    d: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Flow, CommandError> {
    for process in &mut d.processes {
        if let Some(flow) = process.flows.iter_mut().find(|f| &f.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = nested_flow_mut(&mut process.nodes, id, 0) {
            return Ok(flow);
        }
    }
    for choreography in &mut d.choreographies {
        if let Some(flow) = choreography
            .sequence_flows
            .iter_mut()
            .chain(&mut choreography.message_flows)
            .find(|f| &f.id == id)
        {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut choreography.extras, id) {
            return Ok(flow);
        }
    }
    for collaboration in &mut d.additional_collaborations {
        if let Some(flow) = collaboration.flows.iter_mut().find(|flow| &flow.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut collaboration.extras, id) {
            return Ok(flow);
        }
    }
    if let Some(collaboration) = &mut d.collaboration {
        if let Some(flow) = collaboration.flows.iter_mut().find(|flow| &flow.id == id) {
            return Ok(flow);
        }
        if let Some(flow) = artifact_flow_mut(&mut collaboration.extras, id) {
            return Ok(flow);
        }
    }
    Err(invalid("flow not found"))
}
fn artifact_flow_mut<'a>(
    extras: &'a mut Option<crate::bpmn_collaboration::CollaborationExtras>,
    id: &ElementSelector,
) -> Option<&'a mut Flow> {
    use crate::bpmn_collaboration::CollaborationArtifact;
    extras
        .as_mut()?
        .artifacts
        .iter_mut()
        .find_map(|artifact| match artifact {
            CollaborationArtifact::Association(flow) if &flow.id == id => Some(flow.as_mut()),
            _ => None,
        })
}
fn remove_collaboration_artifact(
    extras: &mut Option<crate::bpmn_collaboration::CollaborationExtras>,
    id: &ElementSelector,
) -> bool {
    use crate::bpmn_collaboration::CollaborationArtifact;
    let Some(extras) = extras else { return false };
    let Some(index) = extras.artifacts.iter().position(|artifact| match artifact {
        CollaborationArtifact::Group(value) => &value.id == id,
        CollaborationArtifact::TextAnnotation(value) => &value.id == id,
        CollaborationArtifact::Association(value) => &value.id == id,
    }) else {
        return false;
    };
    extras.artifacts.remove(index);
    true
}
fn diagram<'a>(
    d: &'a mut BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a mut Diagram, CommandError> {
    d.diagrams
        .iter_mut()
        .find(|p| &p.id == id)
        .ok_or_else(|| invalid("diagram not found"))
}
fn remove_di_shapes(
    diagram: &mut Diagram,
    owners: &std::collections::HashSet<ElementSelector>,
    namespace: Option<&str>,
) {
    let owner_xml_ids: std::collections::HashSet<_> = diagram
        .shapes
        .iter()
        .filter(|shape| owners.contains(&shape.id.selector()))
        .filter_map(|shape| shape.id.xml_id().cloned())
        .collect();
    diagram.shapes.retain(|shape| {
        if owners.contains(&shape.id.selector()) {
            return false;
        }
        match &shape.choreography_activity_shape {
            Some(DiReference::Local(id)) => !owners.contains(id),
            Some(DiReference::Qualified(qname)) => {
                !(namespace == Some(qname.namespace.as_str())
                    && owner_xml_ids.contains(&qname.local))
            }
            None => true,
        }
    });
}
/// Auto-resize the containers of `elements` (and then their containers) after a create, move,
/// or expand (see `containers::auto_resize_step`); lane membership follows the grown pools.
fn auto_resize(
    p: &mut ProjectData,
    document_id: DocumentId,
    diagram_id: &ElementSelector,
    mut elements: Vec<ElementSelector>,
    old: &BpmnDocument,
) -> Result<(), CommandError> {
    let mut resized = false;
    for _ in 0..32 {
        let steps =
            crate::containers::auto_resize_step(doc(p, document_id, false)?, diagram_id, &elements);
        if steps.is_empty() {
            break;
        }
        elements.clear();
        for (element, shape_id, bounds) in steps {
            let batch = crate::manipulate::resize(
                doc(p, document_id, false)?,
                diagram_id,
                &shape_id,
                bounds,
            )
            .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
            elements.push(element);
            resized = true;
        }
    }
    let d = doc(p, document_id, false)?;
    if resized && d.capability.allows_structural_editing() && !d.blocks_structural_edits() {
        crate::containers::sync_lanes(old, d, diagram_id);
    }
    Ok(())
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
        } => {
            let document = doc(p, document_id, true)?;
            if let Some(existing) = document
                .collaboration
                .as_mut()
                .filter(|root| root.id == collaboration.id)
            {
                *existing = collaboration;
            } else if let Some(existing) = document
                .additional_collaborations
                .iter_mut()
                .find(|root| root.id == collaboration.id)
            {
                *existing = collaboration;
            } else if document.collaboration.is_none() {
                document.collaboration = Some(collaboration);
            } else {
                document.additional_collaborations.push(collaboration);
            }
        }
        Mutation::DeleteCollaborationRoot {
            document_id,
            collaboration_id,
        } => {
            let document = doc(p, document_id, true)?;
            if document
                .collaboration
                .as_ref()
                .is_some_and(|root| root.id == collaboration_id)
            {
                document.collaboration = None;
            } else {
                let index = document
                    .additional_collaborations
                    .iter()
                    .position(|root| root.id == collaboration_id)
                    .ok_or_else(|| invalid("collaboration root not found"))?;
                document.additional_collaborations.remove(index);
            }
            document
                .diagrams
                .retain(|diagram| diagram.plane != collaboration_id);
        }
        Mutation::SetPartnerCatalog {
            document_id,
            partner_entities,
            partner_roles,
            endpoints,
        } => {
            let document = doc(p, document_id, true)?;
            document.partner_entities = partner_entities;
            document.partner_roles = partner_roles;
            document.endpoints = endpoints;
        }
        Mutation::SetDocumentRootOrder { document_id, order } => {
            doc(p, document_id, true)?.root_order = order;
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
        } => {
            diagram(doc(p, document_id, true)?, &diagram_id)?.plane_metadata = metadata;
        }
        Mutation::SetDiagramPlaneOrder {
            document_id,
            diagram_id,
            order,
        } => {
            diagram(doc(p, document_id, true)?, &diagram_id)?.plane_order = order;
        }
        Mutation::SetShapeRepresentationMetadata {
            document_id,
            diagram_id,
            shape_id,
            metadata,
            is_message_visible,
            participant_band_kind,
            choreography_activity_shape,
        } => {
            let shape = diagram(doc(p, document_id, true)?, &diagram_id)?
                .shapes
                .iter_mut()
                .find(|shape| shape.id == shape_id)
                .ok_or_else(|| invalid("shape not found"))?;
            shape.di_metadata = metadata;
            shape.is_message_visible = is_message_visible;
            shape.participant_band_kind = participant_band_kind;
            shape.choreography_activity_shape = choreography_activity_shape;
        }
        Mutation::SetEdgeRepresentationMetadata {
            document_id,
            diagram_id,
            edge_id,
            metadata,
            message_visible_kind,
        } => {
            let edge = diagram(doc(p, document_id, true)?, &diagram_id)?
                .edges
                .iter_mut()
                .find(|edge| edge.id == edge_id)
                .ok_or_else(|| invalid("edge not found"))?;
            edge.di_metadata = metadata;
            edge.message_visible_kind = message_visible_kind;
        }
        Mutation::SetParticipantMultiplicity {
            document_id,
            participant_id,
            multiplicity,
        } => {
            let document = doc(p, document_id, true)?;
            let participant = document
                .collaboration
                .iter_mut()
                .flat_map(|c| &mut c.participants)
                .chain(
                    document
                        .choreographies
                        .iter_mut()
                        .flat_map(|c| &mut c.participants),
                )
                .find(|participant| participant.id == participant_id)
                .ok_or_else(|| invalid("participant not found"))?;
            participant.multiplicity = multiplicity;
        }
        Mutation::SetMessageFlowReference {
            document_id,
            flow_id,
            message_ref,
        } => {
            let flow = flow_mut(doc(p, document_id, true)?, &flow_id)?;
            if flow.kind != FlowKind::Message {
                return Err(invalid("message reference requires a message flow"));
            }
            flow.message_ref = message_ref;
        }
        Mutation::SetGlobalTasks {
            document_id,
            global_tasks,
        } => {
            doc(p, document_id, true)?.global_tasks = global_tasks;
        }
        Mutation::SetCorrelationProperties {
            document_id,
            correlation_properties,
        } => {
            doc(p, document_id, true)?.correlation_properties = correlation_properties;
        }
        Mutation::SetExpression {
            document_id,
            expression_id,
            expression,
        } => {
            let d = doc(p, document_id, false)?;
            if !matches!(d.capability, Capability::Editable) {
                return Err(CommandError::CapabilityDenied);
            }
            let opaque = d.has_opaque_xml();
            let mut found = 0;
            let mut changed_identity = false;
            let mut changed_opaque = false;
            crate::metadata_visit::visit_expressions_mut(d, |existing| {
                if let crate::bpmn_model::Expression::Structured(data) = existing
                    && data
                        .id
                        .as_ref()
                        .is_some_and(|id| id.selector() == expression_id)
                {
                    found += 1;
                    let new = match &expression {
                        crate::bpmn_model::Expression::Structured(value) => value,
                        _ => {
                            changed_identity = true;
                            return;
                        }
                    };
                    if serde_json::to_value(&data.id).ok() != serde_json::to_value(&new.id).ok() {
                        changed_identity = true;
                    } else if opaque
                        && (data.text != new.text
                            || data.namespaces != new.namespaces
                            || data.foreign_attributes != new.foreign_attributes)
                    {
                        changed_opaque = true;
                    } else {
                        *existing = expression.clone();
                    }
                }
            });
            if found != 1 || changed_identity {
                return Err(invalid("expression not found or identity changed"));
            }
            if changed_opaque {
                return Err(invalid("opaque XML prevents replacing expression content"));
            }
        }
        Mutation::SetDefinitionsExtensions {
            document_id,
            extensions,
        } => {
            doc(p, document_id, true)?.extensions = extensions;
        }
        Mutation::SetDefinitionsRelationships {
            document_id,
            relationships,
        } => {
            let document = doc(p, document_id, true)?;
            let namespace = document
                .definitions
                .as_ref()
                .map(|d| d.target_namespace.as_str())
                .or_else(|| {
                    document
                        .provenance
                        .is_none()
                        .then_some(GENERATED_TARGET_NAMESPACE)
                });
            if let Some(namespace) = namespace {
                let ids: std::collections::HashSet<_> = document
                    .element_identities()
                    .into_iter()
                    .filter_map(|id| id.xml_id())
                    .collect();
                let previous: std::collections::HashSet<_> = document
                    .relationships
                    .iter()
                    .flat_map(|r| r.sources.iter().chain(&r.targets))
                    .filter(|reference| {
                        reference.namespace == namespace && !ids.contains(&reference.local)
                    })
                    .collect();
                if relationships
                    .iter()
                    .flat_map(|r| r.sources.iter().chain(&r.targets))
                    .any(|reference| {
                        reference.namespace == namespace
                            && !ids.contains(&reference.local)
                            && !previous.contains(reference)
                    })
                {
                    return Err(invalid("new local relationship reference does not resolve"));
                }
            } else if relationships
                .iter()
                .flat_map(|r| r.sources.iter().chain(&r.targets))
                .any(|reference| {
                    !document
                        .relationships
                        .iter()
                        .flat_map(|r| r.sources.iter().chain(&r.targets))
                        .any(|old| old == reference)
                })
            {
                return Err(invalid(
                    "cannot determine namespace for new relationship references",
                ));
            }
            document.relationships = relationships;
        }
        Mutation::SetDiagramMetadata {
            document_id,
            diagram_id,
            documentation,
            resolution,
        } => {
            let diagram = diagram(doc(p, document_id, false)?, &diagram_id)?;
            diagram.documentation = documentation;
            diagram.resolution = resolution;
        }
        Mutation::SetDiagramLabelStyles {
            document_id,
            diagram_id,
            label_styles,
        } => {
            diagram(doc(p, document_id, true)?, &diagram_id)?.label_styles = label_styles;
        }
        Mutation::SetShapeLabel {
            document_id,
            diagram_id,
            shape_id,
            label,
            label_visibility,
        } => {
            let shape = diagram(doc(p, document_id, true)?, &diagram_id)?
                .shapes
                .iter_mut()
                .find(|shape| shape.id == shape_id)
                .ok_or_else(|| invalid("shape not found"))?;
            shape.label = label;
            shape.label_visibility = label_visibility;
        }
        Mutation::SetEdgeLabel {
            document_id,
            diagram_id,
            edge_id,
            label,
            label_visibility,
        } => {
            let edge = diagram(doc(p, document_id, true)?, &diagram_id)?
                .edges
                .iter_mut()
                .find(|edge| edge.id == edge_id)
                .ok_or_else(|| invalid("edge not found"))?;
            edge.label = label;
            edge.label_visibility = label_visibility;
        }
        Mutation::SetDocumentationEntry {
            document_id,
            documentation_id,
            documentation,
        } => {
            let d = doc(p, document_id, false)?;
            let opaque = d.has_opaque_xml();
            let mut found = 0;
            let mut invalid_id = false;
            let mut unsafe_content_change = false;
            let mut update = |item: &mut crate::bpmn_metadata::Documentation| {
                if item
                    .id
                    .as_ref()
                    .is_some_and(|id| id.selector() == documentation_id)
                {
                    found += 1;
                    if serde_json::to_value(&item.id).ok()
                        != serde_json::to_value(&documentation.id).ok()
                    {
                        invalid_id = true;
                    } else if opaque && item.content != documentation.content {
                        unsafe_content_change = true;
                    } else {
                        *item = documentation.clone();
                    }
                }
            };
            for extension in &mut d.extensions {
                for item in &mut extension.documentation {
                    update(item);
                }
            }
            crate::metadata_visit::visit_base_metadata_mut(d, |_, metadata| {
                if let Some(metadata) = metadata {
                    for item in &mut metadata.documentation {
                        update(item);
                    }
                }
            });
            crate::metadata_visit::visit_legacy_metadata_mut(d, |_, documentation, _| {
                if let crate::bpmn_metadata::DocumentationSet::Structured(items) = documentation {
                    for item in items {
                        update(item);
                    }
                }
            });
            crate::metadata_visit::visit_expressions_mut(d, |expression| {
                if let crate::bpmn_model::Expression::Structured(data) = expression
                    && let crate::bpmn_model::ExpressionBody::Mixed { parts } = &mut data.text
                {
                    for part in parts {
                        if let crate::bpmn_model::ExpressionPart::Documentation { documentation } =
                            part
                        {
                            update(documentation);
                        }
                    }
                }
            });
            if found != 1 || invalid_id {
                return Err(invalid("documentation not found or identity changed"));
            }
            if unsafe_content_change {
                return Err(invalid(
                    "opaque XML prevents replacing documentation content",
                ));
            }
        }
        Mutation::SetBaseMetadata {
            document_id,
            element_id,
            metadata,
        } => {
            let d = doc(p, document_id, true)?;
            let mut found = false;
            crate::metadata_visit::visit_base_metadata_mut(d, |id, slot| {
                if id.is_some_and(|id| id.selector() == element_id) {
                    *slot = metadata.clone();
                    found = true;
                }
            });
            if !found {
                return Err(invalid("element does not support common base metadata"));
            }
        }
        Mutation::SetBaseExtras {
            document_id,
            element_id,
            extras,
        } => {
            let d = doc(p, document_id, true)?;
            let mut found = false;
            crate::metadata_visit::visit_legacy_metadata_mut(d, |id, _, slot| {
                if id.selector() == element_id {
                    *slot = extras.clone();
                    found = true;
                }
            });
            if !found {
                return Err(invalid("element does not support base extras"));
            }
        }
        Mutation::SetScriptBody {
            document_id,
            element_id,
            body,
        } => {
            let node = node_mut(doc(p, document_id, true)?, &element_id)?;
            if node.kind != NodeKind::ScriptTask {
                return Err(invalid("element is not a script task"));
            }
            node.activity.get_or_insert_with(Default::default).script = body;
        }
        Mutation::SetAnnotationBody {
            document_id,
            element_id,
            body,
        } => {
            let node = node_mut(doc(p, document_id, true)?, &element_id)?;
            if node.kind != NodeKind::TextAnnotation {
                return Err(invalid("element is not a text annotation"));
            }
            match &mut node.details {
                Some(crate::bpmn_model::NodeDetails::TextAnnotation { text, .. }) => {
                    if text.is_none() {
                        node.name = None;
                    }
                    *text = body;
                }
                None => {
                    node.name = None;
                    node.details = Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                        text_format: None,
                        text: body,
                    });
                }
                _ => return Err(invalid("invalid text annotation details")),
            }
        }
        Mutation::ReplaceDocumentation {
            document_id,
            element_id,
            documentation,
        } => {
            let d = doc(p, document_id, true)?;
            let mut found = false;
            for process in &mut d.processes {
                if process.id == element_id {
                    process.documentation = documentation.clone();
                    found = true;
                }
                found |=
                    replace_nested_documentation(&mut process.nodes, &element_id, &documentation);
                if let Some(flow) = process.flows.iter_mut().find(|flow| flow.id == element_id) {
                    flow.documentation = documentation.clone();
                    found = true;
                }
            }
            if let Some(collaboration) = &mut d.collaboration
                && let Some(flow) = collaboration
                    .flows
                    .iter_mut()
                    .find(|flow| flow.id == element_id)
            {
                flow.documentation = documentation.clone();
                found = true;
            }
            for collaboration in &mut d.additional_collaborations {
                for flow in &mut collaboration.flows {
                    if flow.id == element_id {
                        flow.documentation = documentation.clone();
                        found = true;
                    }
                }
            }
            for choreography in &mut d.choreographies {
                for flow in choreography
                    .sequence_flows
                    .iter_mut()
                    .chain(&mut choreography.message_flows)
                {
                    if flow.id == element_id {
                        flow.documentation = documentation.clone();
                        found = true;
                    }
                }
            }
            crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
                if id == &element_id {
                    *value = documentation.clone();
                    found = true;
                }
            });
            if !found {
                return Err(invalid("element has no legacy documentation field"));
            }
        }
        Mutation::SetEventResources {
            document_id,
            resources,
        } => {
            let document = doc(p, document_id, true)?;
            for scope in crate::queries::scopes(document) {
                for node in scope.nodes {
                    if let Some(reference) = node
                        .activity
                        .as_ref()
                        .and_then(|activity| activity.message_ref.as_ref())
                        && local_message_exists(document, reference, &document.event_resources)
                        && !local_message_exists(document, reference, &resources)
                    {
                        return Err(invalid(
                            "Event resource change would remove a referenced local message",
                        ));
                    }
                }
            }
            document.event_resources = resources;
        }
        Mutation::SetRootEventDefinitions {
            document_id,
            definitions,
        } => {
            doc(p, document_id, true)?.root_event_definitions = definitions;
        }
        Mutation::SetShapeDiAttributes {
            document_id,
            diagram_id,
            shape_id,
            is_horizontal,
            is_marker_visible,
        } => {
            let shape = diagram(doc(p, document_id, false)?, &diagram_id)?
                .shapes
                .iter_mut()
                .find(|shape| shape.id == shape_id)
                .ok_or_else(|| invalid("shape not found"))?;
            shape.is_horizontal = is_horizontal;
            shape.is_marker_visible = is_marker_visible;
        }
        Mutation::SetEdgeDiEndpoints {
            document_id,
            diagram_id,
            edge_id,
            source_element,
            target_element,
        } => {
            let edge = diagram(doc(p, document_id, false)?, &diagram_id)?
                .edges
                .iter_mut()
                .find(|edge| edge.id == edge_id)
                .ok_or_else(|| invalid("edge not found"))?;
            edge.source_element = source_element;
            edge.target_element = target_element;
        }
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
        } => {
            let node = node_mut(doc(p, document_id, true)?, &element_id)?;
            if node.kind != NodeKind::TextAnnotation {
                return Err(invalid("element is not a text annotation"));
            }
            match &mut node.details {
                Some(crate::bpmn_model::NodeDetails::TextAnnotation { text: value, .. }) => {
                    if value.as_ref().is_some_and(|value| {
                        matches!(value, crate::bpmn_xml::MixedXml::Structured(_))
                    }) {
                        return Err(invalid(
                            "structured annotation requires an explicit mixed-content edit",
                        ));
                    }
                    if value.is_none() {
                        node.name = None;
                    }
                    *value = text.map(Into::into)
                }
                None => {
                    node.name = None;
                    node.details = Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                        text_format: None,
                        text: text.map(Into::into),
                    })
                }
                _ => return Err(invalid("invalid text annotation details")),
            }
        }
        Mutation::SetProcessExecutable {
            document_id,
            process_id,
            is_executable,
        } => {
            let document = doc(p, document_id, true)?;
            let process = document
                .processes
                .iter_mut()
                .find(|process| process.id == process_id)
                .ok_or_else(|| invalid("process not found"))?;
            process.is_executable = is_executable;
        }
        Mutation::SetProcessActivityMetadata {
            document_id,
            process_id,
            metadata,
        } => {
            let process = doc(p, document_id, true)?
                .processes
                .iter_mut()
                .find(|process| process.id == process_id)
                .ok_or_else(|| invalid("process not found"))?;
            process.activity_metadata = metadata;
        }
        Mutation::SetNodeFlowMetadata {
            document_id,
            element_id,
            metadata,
        } => {
            node_mut(doc(p, document_id, true)?, &element_id)?.flow_metadata = metadata;
        }
        Mutation::SetFlowMetadata {
            document_id,
            element_id,
            metadata,
        } => {
            flow_mut(doc(p, document_id, true)?, &element_id)?.flow_metadata = metadata;
        }
        Mutation::SetCollaborationName { document_id, name } => {
            collaboration_mut(doc(p, document_id, false)?)?.name = name;
        }
        Mutation::SetCollaborationCorrelationKeys {
            document_id,
            correlation_keys,
        } => {
            collaboration_mut(doc(p, document_id, true)?)?.correlation_keys = correlation_keys;
        }
        Mutation::SetDiagramName {
            document_id,
            diagram_id,
            name,
        } => {
            diagram(doc(p, document_id, false)?, &diagram_id)?.name = name;
        }
        Mutation::SetNodeItemAware {
            document_id,
            element_id,
            item_aware,
        } => node_mut(doc(p, document_id, true)?, &element_id)?.item_aware = item_aware,
        Mutation::SetProcessIoSpecification {
            document_id,
            process_id,
            io_specification,
        } => {
            let process = doc(p, document_id, true)?
                .processes
                .iter_mut()
                .find(|p| p.id == process_id)
                .ok_or_else(|| invalid("process not found"))?;
            process.io_specification = io_specification;
        }
        Mutation::SetDataStores {
            document_id,
            data_stores,
        } => doc(p, document_id, true)?.data_stores = data_stores,
        Mutation::SetScopeDataObjects {
            document_id,
            scope_id,
            data_objects,
        } => *data_parts_mut(doc(p, document_id, true)?, &scope_id)?.0 = data_objects,
        Mutation::SetScopeDataAssociations {
            document_id,
            scope_id,
            data_associations,
        } => *data_parts_mut(doc(p, document_id, true)?, &scope_id)?.1 = data_associations,
        Mutation::SetSupportingCatalog {
            document_id,
            catalog,
        } => {
            let d = doc(p, document_id, true)?;
            for scope in crate::queries::scopes(d) {
                for node in scope.nodes {
                    if let Some(reference) = node
                        .activity
                        .as_ref()
                        .and_then(|a| a.operation_ref.as_ref())
                        && local_operation_exists(d, reference, &d.catalog)
                        && !local_operation_exists(d, reference, &catalog)
                    {
                        return Err(invalid(
                            "Catalog change would remove a referenced local operation",
                        ));
                    }
                }
            }
            d.catalog = catalog;
        }
        Mutation::SetActivityProperties {
            document_id,
            element_id,
            activity,
        } => {
            let d = doc(p, document_id, true)?;
            let old_script = crate::queries::find_node(d, &element_id)
                .and_then(|n| n.activity.as_ref())
                .and_then(|a| a.script.as_ref());
            if old_script.is_some_and(|s| matches!(s, crate::bpmn_xml::MixedXml::Structured(_)))
                && old_script != activity.as_ref().and_then(|a| a.script.as_ref())
            {
                return Err(invalid(
                    "structured script requires an explicit mixed-content edit",
                ));
            }
            if let Some(activity) = &activity {
                let previous =
                    crate::queries::find_node(d, &element_id).and_then(|n| n.activity.as_ref());
                check_activity_references(d, activity, previous)?;
            }
            let node = node_mut(d, &element_id)?;
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
        Mutation::SetLaneSetName {
            document_id,
            scope_id,
            lane_set_name,
        } => {
            check_lane_set_name(&lane_set_name)?;
            *lane_set_name_mut(doc(p, document_id, true)?, &scope_id)? = lane_set_name;
        }
        Mutation::SetChildLaneSetName {
            document_id,
            scope_id,
            lane_id,
            child_lane_set_name,
        } => {
            check_lane_set_name(&child_lane_set_name)?;
            scope_parts_mut(doc(p, document_id, true)?, &scope_id)?
                .1
                .iter_mut()
                .find(|lane| lane.id == lane_id)
                .ok_or_else(|| invalid("lane not found"))?
                .child_lane_set_name = child_lane_set_name;
        }
        Mutation::SetLanePartition {
            document_id,
            scope_id,
            lane_id,
            partition_element,
            partition_element_ref,
        } => {
            check_lane_set_name(&partition_element.as_ref().and_then(|e| e.value.clone()))?;
            let lane = scope_parts_mut(doc(p, document_id, true)?, &scope_id)?
                .1
                .iter_mut()
                .find(|lane| lane.id == lane_id)
                .ok_or_else(|| invalid("lane not found"))?;
            lane.partition_element = partition_element;
            lane.partition_element_ref = partition_element_ref;
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
                .bounds = Some(bounds);
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
            if data_store.id.is_none() {
                return Err(invalid("Use SetDataStores for a store without an ID"));
            }
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
                .position(|v| v.id.as_ref().is_some_and(|id| id == &data_store_id))
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
            if data_object.id.is_none() {
                return Err(invalid(
                    "Use SetScopeDataObjects for an object without an ID",
                ));
            }
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
                .position(|v| v.id.as_ref().is_some_and(|id| id == &data_object_id))
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
            if association.id.is_none() {
                return Err(invalid(
                    "Use SetScopeDataAssociations for an association without an ID",
                ));
            }
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
                .position(|v| v.id.as_ref().is_some_and(|id| id == &association_id))
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
            if d.choreographies[index].graph.is_some() || choreography.graph.is_some() {
                return Err(invalid("legacy choreography update cannot replace a graph"));
            }
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
            if d.choreographies[index].graph.is_some() {
                return Err(invalid("legacy choreography delete cannot remove a graph"));
            }
            d.choreographies.remove(index);
        }
        Mutation::PromoteChoreographyGraph {
            document_id,
            choreography_id,
        } => {
            use crate::bpmn_collaboration::{ChoreographyFlowElement, ChoreographyGraph};
            let c = choreography_mut(doc(p, document_id, true)?, &choreography_id)?;
            if c.graph.is_some() {
                return Err(invalid("choreography already has a graph"));
            }
            let mut flow_elements = Vec::with_capacity(c.activities.len() + c.sequence_flows.len());
            flow_elements.extend(
                std::mem::take(&mut c.activities)
                    .into_iter()
                    .map(|a| ChoreographyFlowElement::Activity(Box::new(a))),
            );
            flow_elements.extend(
                std::mem::take(&mut c.sequence_flows)
                    .into_iter()
                    .map(|f| ChoreographyFlowElement::SequenceFlow(Box::new(f))),
            );
            c.graph = Some(ChoreographyGraph {
                flow_elements,
                data_associations: vec![],
            });
        }
        Mutation::CreateChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            index,
            element,
        } => {
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            if index > graph.flow_elements.len() {
                return Err(invalid("choreography insertion index out of range"));
            }
            graph.flow_elements.insert(index, element);
        }
        Mutation::UpdateChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element,
        } => {
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            let id = choreography_element_id(&element);
            let old = graph
                .flow_elements
                .iter_mut()
                .find(|e| choreography_element_id(e) == id)
                .ok_or_else(|| invalid("choreography flow element not found"))?;
            if std::mem::discriminant(old) != std::mem::discriminant(&element) {
                return Err(invalid("flow element kind cannot be changed by update"));
            }
            // Structural subtree changes need their own scoped operations so descendants cannot disappear implicitly.
            if let (
                crate::bpmn_collaboration::ChoreographyFlowElement::Activity(a),
                crate::bpmn_collaboration::ChoreographyFlowElement::Activity(b),
            ) = (&*old, &element)
            {
                use crate::bpmn_collaboration::ChoreographyActivityDetails::Sub;
                let has_contents = |details: &Option<
                    crate::bpmn_collaboration::ChoreographyActivityDetails,
                >| matches!(details, Some(Sub { graph, artifacts }) if !graph.flow_elements.is_empty() || !graph.data_associations.is_empty() || !artifacts.is_empty());
                if (has_contents(&a.details) || has_contents(&b.details))
                    && serde_json::to_value(&a.details).ok()
                        != serde_json::to_value(&b.details).ok()
                {
                    return Err(invalid("sub-choreography structure requires scoped edits"));
                }
            }
            *old = element;
        }
        Mutation::DeleteChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element_id,
        } => {
            delete_choreography_flow_element(
                p,
                document_id,
                &choreography_id,
                &scope_id,
                &element_id,
            )?;
        }
        Mutation::ReorderChoreographyFlowElement {
            document_id,
            choreography_id,
            scope_id,
            element_id,
            index,
        } => {
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            if index >= graph.flow_elements.len() {
                return Err(invalid("choreography reorder index out of range"));
            }
            let old = graph
                .flow_elements
                .iter()
                .position(|e| choreography_element_id(e) == element_id)
                .ok_or_else(|| invalid("choreography flow element not found"))?;
            let element = graph.flow_elements.remove(old);
            graph.flow_elements.insert(index, element);
        }
        Mutation::ReparentChoreographyFlowElement {
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            element_id,
            index,
        } => {
            if source_scope_id == target_scope_id {
                return Err(invalid("use reorder within one scope"));
            }
            let document = doc(p, document_id, true)?;
            let c = choreography_mut(document, &choreography_id)?;
            let source = choreography_scope_mut(c, &source_scope_id)?;
            let old = source
                .flow_elements
                .iter()
                .position(|e| choreography_element_id(e) == element_id)
                .ok_or_else(|| invalid("choreography flow element not found"))?;
            let mut subtree = std::collections::HashSet::new();
            choreography_subtree_ids(&source.flow_elements[old], &mut subtree);
            if !choreography_reparent_geometry_safe(
                document,
                &choreography_id,
                &source_scope_id,
                &target_scope_id,
                &element_id,
                &subtree,
            ) {
                return Err(invalid(
                    "reparenting represented choreography elements requires qualified geometry",
                ));
            }
            let c = choreography_mut(document, &choreography_id)?;
            let source = choreography_scope_mut(c, &source_scope_id)?;
            let element = source.flow_elements.remove(old);
            // Data associations are owned by their element and move with it.
            let (moved, kept) = std::mem::take(&mut source.data_associations)
                .into_iter()
                .partition(|a| a.owner == element_id);
            source.data_associations = kept;
            let target = choreography_scope_mut(c, &target_scope_id)?;
            if index > target.flow_elements.len() {
                return Err(invalid("choreography insertion index out of range"));
            }
            target.flow_elements.insert(index, element);
            target.data_associations.extend::<Vec<_>>(moved);
        }
        Mutation::CreateChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            index,
            artifact,
        } => {
            let artifacts = choreography_artifacts_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
                true,
            )?;
            if index > artifacts.len() {
                return Err(invalid(
                    "choreography artifact insertion index out of range",
                ));
            }
            artifacts.insert(index, artifact);
        }
        Mutation::UpdateChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact,
        } => {
            let artifacts = choreography_artifacts_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
                false,
            )?;
            let id = choreography_artifact_id(&artifact);
            let old = artifacts
                .iter_mut()
                .find(|a| choreography_artifact_id(a) == id)
                .ok_or_else(|| invalid("choreography artifact not found"))?;
            if std::mem::discriminant(old) != std::mem::discriminant(&artifact) {
                return Err(invalid("artifact kind cannot be changed by update"));
            }
            *old = artifact;
        }
        Mutation::DeleteChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
        } => {
            let document = doc(p, document_id, true)?;
            let before: std::collections::HashSet<_> = document
                .element_identities()
                .into_iter()
                .map(|id| id.selector())
                .collect();
            let artifacts = choreography_artifacts_mut(
                choreography_mut(document, &choreography_id)?,
                &scope_id,
                false,
            )?;
            let index = artifacts
                .iter()
                .position(|a| choreography_artifact_id(a) == artifact_id)
                .ok_or_else(|| invalid("choreography artifact not found"))?;
            artifacts.remove(index);
            let after: std::collections::HashSet<_> = document
                .element_identities()
                .into_iter()
                .map(|id| id.selector())
                .collect();
            let removed: std::collections::HashSet<_> =
                before.difference(&after).cloned().collect();
            let namespace = document
                .definitions
                .as_ref()
                .map(|d| d.target_namespace.clone())
                .or_else(|| {
                    document
                        .provenance
                        .is_none()
                        .then(|| GENERATED_TARGET_NAMESPACE.to_owned())
                });
            for diagram in &mut document.diagrams {
                let owners = diagram
                    .shapes
                    .iter()
                    .filter(|shape| removed.contains(&shape.element))
                    .map(|shape| shape.id.selector())
                    .collect();
                remove_di_shapes(diagram, &owners, namespace.as_deref());
                diagram
                    .edges
                    .retain(|edge| !removed.contains(&edge.element));
            }
            prune_removed_element_refs(p, document_id, Some(&removed));
        }
        Mutation::ReorderChoreographyArtifact {
            document_id,
            choreography_id,
            scope_id,
            artifact_id,
            index,
        } => {
            let artifacts = choreography_artifacts_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
                false,
            )?;
            if index >= artifacts.len() {
                return Err(invalid("choreography artifact reorder index out of range"));
            }
            let old = artifacts
                .iter()
                .position(|a| choreography_artifact_id(a) == artifact_id)
                .ok_or_else(|| invalid("choreography artifact not found"))?;
            let artifact = artifacts.remove(old);
            artifacts.insert(index, artifact);
        }
        Mutation::ReparentChoreographyArtifact {
            document_id,
            choreography_id,
            source_scope_id,
            target_scope_id,
            artifact_id,
            index,
        } => {
            if source_scope_id == target_scope_id {
                return Err(invalid("use reorder within one scope"));
            }
            let document = doc(p, document_id, true)?;
            if !choreography_reparent_geometry_safe(
                document,
                &choreography_id,
                &source_scope_id,
                &target_scope_id,
                &artifact_id,
                &std::collections::HashSet::from([artifact_id.clone()]),
            ) {
                return Err(invalid(
                    "reparenting represented choreography artifacts requires qualified geometry",
                ));
            }
            let c = choreography_mut(document, &choreography_id)?;
            let source = choreography_artifacts_mut(c, &source_scope_id, false)?;
            let old = source
                .iter()
                .position(|a| choreography_artifact_id(a) == artifact_id)
                .ok_or_else(|| invalid("choreography artifact not found"))?;
            let artifact = source.remove(old);
            let target = choreography_artifacts_mut(c, &target_scope_id, true)?;
            if index > target.len() {
                return Err(invalid(
                    "choreography artifact insertion index out of range",
                ));
            }
            target.insert(index, artifact);
        }
        Mutation::CreateChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            index,
            association,
        } => {
            if association.id.is_none() {
                return Err(invalid(
                    "new choreography data association requires a stable identity",
                ));
            }
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            if index > graph.data_associations.len() {
                return Err(invalid("data association insertion index out of range"));
            }
            graph.data_associations.insert(index, association);
        }
        Mutation::UpdateChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association,
        } => {
            let id = association
                .id
                .as_ref()
                .ok_or_else(|| invalid("data association requires a stable identity"))?
                .selector();
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            let old = graph
                .data_associations
                .iter_mut()
                .find(|a| a.id.as_ref().is_some_and(|value| value.selector() == id))
                .ok_or_else(|| invalid("data association not found"))?;
            *old = association;
        }
        Mutation::DeleteChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association_id,
        } => {
            let document = doc(p, document_id, true)?;
            let before: std::collections::HashSet<_> = document
                .element_identities()
                .into_iter()
                .map(|id| id.selector())
                .collect();
            let graph =
                choreography_scope_mut(choreography_mut(document, &choreography_id)?, &scope_id)?;
            let old = graph
                .data_associations
                .iter()
                .position(|a| {
                    a.id.as_ref()
                        .is_some_and(|value| value.selector() == association_id)
                })
                .ok_or_else(|| invalid("data association not found"))?;
            graph.data_associations.remove(old);
            let after: std::collections::HashSet<_> = document
                .element_identities()
                .into_iter()
                .map(|id| id.selector())
                .collect();
            let removed: std::collections::HashSet<_> =
                before.difference(&after).cloned().collect();
            for diagram in &mut document.diagrams {
                diagram
                    .edges
                    .retain(|edge| !removed.contains(&edge.element));
            }
            prune_removed_element_refs(p, document_id, Some(&removed));
        }
        Mutation::ReorderChoreographyDataAssociation {
            document_id,
            choreography_id,
            scope_id,
            association_id,
            index,
        } => {
            let graph = choreography_scope_mut(
                choreography_mut(doc(p, document_id, true)?, &choreography_id)?,
                &scope_id,
            )?;
            if index >= graph.data_associations.len() {
                return Err(invalid("data association reorder index out of range"));
            }
            let old = graph
                .data_associations
                .iter()
                .position(|a| {
                    a.id.as_ref()
                        .is_some_and(|value| value.selector() == association_id)
                })
                .ok_or_else(|| invalid("data association not found"))?;
            let association = graph.data_associations.remove(old);
            graph.data_associations.insert(index, association);
        }
        Mutation::CreateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => {
            let c = choreography_mut(doc(p, document_id, true)?, &choreography_id)?;
            if c.graph.is_some() {
                return Err(invalid(
                    "legacy activity command requires a legacy choreography",
                ));
            }
            c.activities.push(activity);
        }
        Mutation::UpdateChoreographyActivity {
            document_id,
            choreography_id,
            activity,
        } => {
            let c = choreography_mut(doc(p, document_id, true)?, &choreography_id)?;
            if c.graph.is_some() {
                return Err(invalid(
                    "legacy activity command requires a legacy choreography",
                ));
            }
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
            let document = doc(p, document_id, true)?;
            let c = choreography_mut(document, &choreography_id)?;
            if c.graph.is_some() {
                return Err(invalid(
                    "legacy activity command requires a legacy choreography",
                ));
            }
            let index = c
                .activities
                .iter()
                .position(|a| a.id == activity_id)
                .ok_or_else(|| invalid("choreography activity not found"))?;
            c.activities.remove(index);
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
            for diagram in &mut document.diagrams {
                let owners = diagram
                    .shapes
                    .iter()
                    .filter(|shape| shape.element == activity_id)
                    .map(|shape| shape.id.selector())
                    .collect();
                remove_di_shapes(diagram, &owners, namespace.as_deref());
            }
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
                        removed.insert(node.id.selector());
                    }
                }
                if removed.len() == old_len {
                    break;
                }
            }
            c.conversation_nodes
                .retain(|n| !removed.contains(&n.id.selector()));
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
            if matches!(
                &node.details,
                Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                    text: Some(crate::bpmn_xml::MixedXml::Structured(_)),
                    ..
                })
            ) && node.details != details
            {
                return Err(invalid(
                    "structured annotation requires an explicit mixed-content edit",
                ));
            }
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
            let attribute_used = |project: &ProjectData| {
                project.attribute_values.iter().any(|assignment| {
                    matches!(
                        &assignment.value,
                        crate::attributes::AttributeValue::DictionaryRefs(refs)
                            if refs.iter().any(|r| r == &DictionaryTarget::Entry(entry_id))
                    )
                })
            };
            let used = p
                .dictionary_links
                .iter()
                .any(|l| l.target == DictionaryTarget::Entry(entry_id))
                || attribute_used(p);
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
                for assignment in &mut p.attribute_values {
                    if let crate::attributes::AttributeValue::DictionaryRefs(refs) =
                        &mut assignment.value
                    {
                        for reference in refs.iter_mut() {
                            if *reference == DictionaryTarget::Entry(entry_id) {
                                *reference = replacement.clone();
                            }
                        }
                        // Retargeting can merge two references anywhere in the list.
                        let mut seen = std::collections::HashSet::new();
                        refs.retain(|reference| seen.insert(reference.clone()));
                    }
                }
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
            doc(p, source.document_id, false)?;
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
            if f.condition
                .as_ref()
                .is_some_and(|value| value.plain_text().is_none())
                && f.condition != condition
            {
                return Err(invalid("mixed expression requires an explicit replacement"));
            }
            f.condition = condition;
        }
        Mutation::ReplaceFlowCondition {
            document_id,
            flow_id,
            condition,
        } => {
            let d = doc(p, document_id, true)?;
            let f = flow_mut(d, &flow_id)?;
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
            prune_removed_element_refs(p, document_id, None);
            p.links.retain(|l| l.target != document_id);
        }
        Mutation::CreateShape {
            document_id,
            diagram_id,
            shape,
        } => {
            let diagram = diagram(doc(p, document_id, true)?, &diagram_id)?;
            if let Some(order) = &mut diagram.plane_order {
                order.push(shape.id.selector());
            }
            diagram.shapes.push(shape);
        }
        Mutation::DeleteShape {
            document_id,
            diagram_id,
            shape_id,
        } => {
            let document = doc(p, document_id, true)?;
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
        }
        Mutation::CreateEdge {
            document_id,
            diagram_id,
            edge,
        } => {
            let diagram = diagram(doc(p, document_id, true)?, &diagram_id)?;
            if let Some(order) = &mut diagram.plane_order {
                order.push(edge.id.selector());
            }
            diagram.edges.push(edge);
        }
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
        } => {
            let d = doc(p, document_id, true)?;
            let mut pending = vec![&element];
            while let Some(node) = pending.pop() {
                if let Some(activity) = &node.activity {
                    check_activity_references(d, activity, None)?;
                }
                if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details
                {
                    pending.extend(&content.nodes);
                }
            }
            scope_parts_mut(d, &process_id)?.0.push(element);
        }
        Mutation::CreateFlow {
            document_id,
            process_id,
            flow,
        } => {
            let d = doc(p, document_id, true)?;
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
                && (d.catalog.has_qname_refs()
                    || d.has_data_qname_refs()
                    || d.has_event_qname_refs()
                    || d.has_activity_qname_refs()
                    || d.has_di_choreography_qname_refs()
                    || d.has_collaboration_qname_refs()
                    || d.extensions.iter().any(|e| e.definition.is_some())
                    || d.relationships
                        .iter()
                        .any(|r| !r.sources.is_empty() || !r.targets.is_empty())
                    || d.processes
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
            if crate::queries::find_node(d, &element_id)
                .is_some_and(|node| node.kind == NodeKind::TextAnnotation)
                || d.collaboration.iter().chain(&d.additional_collaborations).filter_map(|root| root.extras.as_ref()).chain(d.choreographies.iter().filter_map(|root| root.extras.as_ref())).flat_map(|extras| &extras.artifacts).any(|artifact| matches!(artifact, crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) if node.id == element_id))
            {
                return Err(invalid("use SetTextAnnotation to edit annotation text"));
            }
            if crate::queries::scopes(d)
                .iter()
                .flat_map(|scope| scope.flows)
                .chain(d.collaboration.iter().flat_map(|c| &c.flows))
                .any(|flow| flow.id == element_id && flow.kind == FlowKind::Association)
                || d.collaboration.iter().chain(&d.additional_collaborations).filter_map(|root| root.extras.as_ref()).chain(d.choreographies.iter().filter_map(|root| root.extras.as_ref())).flat_map(|extras| &extras.artifacts).any(|artifact| matches!(artifact, crate::bpmn_collaboration::CollaborationArtifact::Association(flow) if flow.id == element_id))
            {
                return Err(invalid(
                    "association has no BPMN name; use its native label where available",
                ));
            }
            let mut found = false;
            for pr in &mut d.processes {
                if pr.id == element_id {
                    pr.name = name.clone();
                    found = true;
                }
                found |= edit_nested(&mut pr.nodes, &element_id, name.as_deref(), false, 0);
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
                if c.id == element_id {
                    c.name = name.clone();
                    found = true;
                }
                for node in &mut c.conversation_nodes {
                    if node.id == element_id {
                        node.name = name.clone();
                        found = true;
                    }
                }
                for link in &mut c.conversation_links {
                    if link.id == element_id {
                        link.name = name.clone();
                        found = true;
                    }
                }
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
            for c in &mut d.additional_collaborations {
                if c.id == element_id {
                    c.name = name.clone();
                    found = true;
                }
                for participant in &mut c.participants {
                    if participant.id == element_id {
                        participant.name = name.clone();
                        found = true;
                    }
                }
                for node in &mut c.conversation_nodes {
                    if node.id == element_id {
                        node.name = name.clone();
                        found = true;
                    }
                }
                for link in &mut c.conversation_links {
                    if link.id == element_id {
                        link.name = name.clone();
                        found = true;
                    }
                }
                for flow in &mut c.flows {
                    if flow.id == element_id {
                        flow.name = name.clone();
                        found = true;
                    }
                }
            }
            for partner in &mut d.partner_entities {
                if partner.id.as_ref().is_some_and(|id| id == &element_id) {
                    partner.name = name.clone();
                    found = true;
                }
            }
            for partner in &mut d.partner_roles {
                if partner.id.as_ref().is_some_and(|id| id == &element_id) {
                    partner.name = name.clone();
                    found = true;
                }
            }
            for choreography in &mut d.choreographies {
                if choreography.id == element_id {
                    choreography.name = name.clone();
                    found = true;
                }
                for participant in &mut choreography.participants {
                    if participant.id == element_id {
                        participant.name = name.clone();
                        found = true;
                    }
                }
                for activity in &mut choreography.activities {
                    if activity.id == element_id {
                        activity.name = name.clone();
                        found = true;
                    }
                }
                if let Some(graph) = &mut choreography.graph {
                    found |= rename_choreography_element(graph, &element_id, &name);
                }
                for node in &mut choreography.conversation_nodes {
                    if node.id == element_id {
                        node.name = name.clone();
                        found = true;
                    }
                }
                for link in &mut choreography.conversation_links {
                    if link.id == element_id {
                        link.name = name.clone();
                        found = true;
                    }
                }
                for flow in choreography
                    .sequence_flows
                    .iter_mut()
                    .chain(&mut choreography.message_flows)
                {
                    if flow.id == element_id {
                        flow.name = name.clone();
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
            let mut structured = documentation_is_structured(d, &element_id);
            crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
                if id == &element_id
                    && matches!(value, crate::bpmn_metadata::DocumentationSet::Structured(_))
                {
                    structured = true;
                }
            });
            if structured {
                return Err(invalid(
                    "structured documentation requires an explicit documentation edit",
                ));
            }
            let mut found = false;
            for pr in &mut d.processes {
                if pr.id == element_id {
                    pr.documentation = documentation.clone().into();
                    found = true;
                }
                found |= edit_nested(&mut pr.nodes, &element_id, Some(&documentation), true, 0);
                for n in &mut pr.nodes {
                    if n.id == element_id {
                        n.documentation = documentation.clone().into();
                        found = true;
                    }
                }
                for f in &mut pr.flows {
                    if f.id == element_id {
                        f.documentation = documentation.clone().into();
                        found = true;
                    }
                }
            }
            if let Some(c) = &mut d.collaboration {
                for f in &mut c.flows {
                    if f.id == element_id {
                        f.documentation = documentation.clone().into();
                        found = true;
                    }
                }
            }
            for collaboration in &mut d.additional_collaborations {
                for flow in &mut collaboration.flows {
                    if flow.id == element_id {
                        flow.documentation = documentation.clone().into();
                        found = true;
                    }
                }
            }
            for choreography in &mut d.choreographies {
                for flow in choreography
                    .sequence_flows
                    .iter_mut()
                    .chain(&mut choreography.message_flows)
                {
                    if flow.id == element_id {
                        flow.documentation = documentation.clone().into();
                        found = true;
                    }
                }
            }
            crate::metadata_visit::visit_legacy_metadata_mut(d, |id, value, _| {
                if id == &element_id {
                    *value = documentation.clone().into();
                    found = true;
                }
            });
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
        }
        Mutation::Connect {
            document_id,
            diagram_id,
            source_id,
            target_id,
            flow_id,
            edge_id,
        } => {
            doc(p, document_id, true)?;
            let d = p
                .documents
                .iter()
                .find(|d| d.id == document_id)
                .cloned()
                .ok_or_else(|| invalid("document not found"))?;
            let batch =
                crate::modeling::connect(&d, &diagram_id, &source_id, &target_id, flow_id, edge_id)
                    .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
        }
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
        } => {
            doc(p, document_id, true)?;
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
        }
        Mutation::ReplaceElement {
            document_id,
            element_id,
            target,
        } => {
            let d = doc(p, document_id, true)?;
            if let Some(diagram_id) =
                crate::containers::collapse_toggle_target(d, &element_id, &target)
            {
                return mutate(
                    p,
                    Mutation::ToggleCollapse {
                        document_id,
                        diagram_id,
                        element_id,
                    },
                );
            }
            let batch = crate::replace::replace(d, &element_id, &target)
                .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
        }
        Mutation::MoveElements {
            document_id,
            diagram_id,
            shapes,
            flow_id,
            edge_id,
        } => {
            let d = doc(p, document_id, false)?;
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
            let batch = crate::manipulate::move_elements(
                d,
                &diagram_id,
                shapes,
                flow_id.zip(edge_id),
                semantic,
            )
            .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
            auto_resize(p, document_id, &diagram_id, moved, &old)?;
        }
        Mutation::ResizeShape {
            document_id,
            diagram_id,
            shape_id,
            bounds,
        } => {
            let d = doc(p, document_id, false)?;
            let old = d.clone();
            let batch = crate::manipulate::resize(d, &diagram_id, &shape_id, bounds)
                .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
            let d = doc(p, document_id, false)?;
            if d.capability.allows_structural_editing() && !d.blocks_structural_edits() {
                crate::containers::sync_lanes(&old, d, &diagram_id);
            }
        }
        Mutation::Reconnect {
            document_id,
            diagram_id,
            flow_id,
            source_id,
            target_id,
        } => {
            let d = doc(p, document_id, true)?;
            let batch =
                crate::manipulate::reconnect(d, &diagram_id, &flow_id, &source_id, &target_id)
                    .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
        }
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
        } => {
            let d = doc(p, document_id, true)?;
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
        }
        Mutation::AddLane {
            document_id,
            diagram_id,
            element_id,
            location,
        } => crate::containers::add_lane(
            doc(p, document_id, true)?,
            &diagram_id,
            &element_id,
            location,
        )
        .map_err(CommandError::InvalidParams)?,
        Mutation::SplitLane {
            document_id,
            diagram_id,
            element_id,
            count,
        } => crate::containers::split_lane(
            doc(p, document_id, true)?,
            &diagram_id,
            &element_id,
            count,
        )
        .map_err(CommandError::InvalidParams)?,
        Mutation::CreatePool {
            document_id,
            diagram_id,
            name,
            center,
        } => {
            let batch = crate::containers::create_pool(
                doc(p, document_id, true)?,
                &diagram_id,
                name,
                center,
            )
            .map_err(CommandError::InvalidParams)?;
            for m in batch {
                mutate(p, m)?;
            }
        }
        Mutation::ToggleCollapse {
            document_id,
            diagram_id,
            element_id,
        } => {
            let d = doc(p, document_id, true)?;
            let old = d.clone();
            if crate::containers::toggle_collapse(d, &diagram_id, &element_id)
                .map_err(CommandError::InvalidParams)?
            {
                auto_resize(p, document_id, &diagram_id, vec![element_id], &old)?;
            }
        }
        Mutation::MakeSpace {
            document_id,
            diagram_id,
            axis,
            start,
            delta,
            before,
            at,
        } => {
            let d = doc(p, document_id, false)?;
            let semantic = d.capability.allows_structural_editing() && !d.blocks_structural_edits();
            crate::containers::make_space(d, &diagram_id, axis, start, delta, !before, semantic, at)
                .map_err(CommandError::InvalidParams)?
        }
        Mutation::Align {
            document_id,
            diagram_id,
            shape_ids,
            alignment,
        } => {
            let prepared = crate::arrange::prepare_align(
                doc(p, document_id, false)?,
                &diagram_id,
                &shape_ids,
                alignment,
            )
            .map_err(invalid)?;
            apply_prepared(p, prepared)?;
        }
        Mutation::Distribute {
            document_id,
            diagram_id,
            shape_ids,
            distribution,
        } => {
            let prepared = crate::arrange::prepare_distribute(
                doc(p, document_id, false)?,
                &diagram_id,
                &shape_ids,
                distribution,
            )
            .map_err(invalid)?;
            apply_prepared(p, prepared)?;
        }
        Mutation::Duplicate {
            document_id,
            diagram_id,
            element_ids,
            dx,
            dy,
            target_diagram_id,
        } => {
            let d = doc(p, document_id, true)?;
            let clipboard =
                crate::clipboard::copy(d, &diagram_id, &element_ids).map_err(invalid)?;
            let target = target_diagram_id.unwrap_or(diagram_id);
            let fallback = crate::clipboard::default_scope(d, &target)
                .ok_or_else(|| invalid("Create a process before pasting"))?;
            let pasted = crate::clipboard::paste(d, &target, &fallback, &clipboard, dx, dy)
                .map_err(invalid)?;
            for m in pasted.mutations {
                mutate(p, m)?;
            }
        }
        Mutation::Delete {
            document_id,
            element_id,
        } => {
            if let Some((choreography_id, scope_id)) =
                choreography_graph_owner(doc(p, document_id, true)?, &element_id)
            {
                return delete_choreography_flow_element(
                    p,
                    document_id,
                    &choreography_id,
                    &scope_id,
                    &element_id,
                );
            }
            let d = doc(p, document_id, true)?;
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
                    !removed.contains(&f.id.selector())
                        && (touches(&f.source) || touches(&f.target))
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
    descriptor!("reroute","Reroute edge","document_id":"DocumentId","diagram_id":"ElementSelector","edge_id":"ElementSelector","waypoints":"Point[]"),
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
