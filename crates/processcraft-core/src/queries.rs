//! Read-only projections of the current in-memory project.
use crate::{
    ids::{
        CheckpointId, DictionaryEntryId, DocumentId, ElementIdentity, ElementKey, ElementSelector,
        XmlId,
    },
    model::*,
};

/// Scalar model fields use JSON Pointer paths so bounded pages do not need to
/// truncate large nested reference lists or lose absent values.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelField {
    pub path: String,
    pub value: serde_json::Value,
}
pub type CatalogField = ModelField;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementIdentityRow {
    pub selector: ElementSelector,
    pub xml_id: Option<XmlId>,
}

/// Existing semantic and DI identities. Legacy null IDs are deliberately not
/// promoted by this projection; scoped data/catalog queries retain those rows.
pub fn identified_elements(
    project: &ProjectData,
    document_id: DocumentId,
) -> Option<Vec<ElementIdentityRow>> {
    let document = project.documents.iter().find(|d| d.id == document_id)?;
    Some(
        document
            .element_identities()
            .into_iter()
            .map(|identity| ElementIdentityRow {
                selector: identity.selector(),
                xml_id: identity.xml_id().cloned(),
            })
            .collect(),
    )
}

fn model_fields(value: serde_json::Value) -> Vec<ModelField> {
    fn fields(value: serde_json::Value, path: String, out: &mut Vec<ModelField>) {
        match value {
            serde_json::Value::Object(object) if !object.is_empty() => {
                for (key, value) in object {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    fields(value, format!("{path}/{key}"), out);
                }
            }
            serde_json::Value::Array(array) if !array.is_empty() => {
                for (index, value) in array.into_iter().enumerate() {
                    fields(value, format!("{path}/{index}"), out);
                }
            }
            value => out.push(ModelField { path, value }),
        }
    }
    let mut result = Vec::new();
    fields(value, String::new(), &mut result);
    result
}

/// Complete modeled document content as scalar JSON Pointer fields. Ordered XML,
/// optional metadata and native identities are observed without normalization.
/// Preserved source bytes are not interpreted or executed.
pub fn document_fields(project: &ProjectData, document_id: DocumentId) -> Option<Vec<ModelField>> {
    let document = project
        .documents
        .iter()
        .find(|document| document.id == document_id)?;
    Some(model_fields(serde_json::to_value(document).ok()?))
}

/// Detailed checkpoint changes can be paged without bundling a large comparison
/// into a single response item. Paths refer to the comparison JSON document.
pub fn checkpoint_fields(
    project: &ProjectData,
    checkpoint_id: CheckpointId,
) -> Option<Vec<ModelField>> {
    Some(model_fields(
        serde_json::to_value(compare_checkpoint(project, checkpoint_id)?).ok()?,
    ))
}

/// Bounded response memory: serializes the document into the requested scalar
/// page directly, without allocating a full JSON document or all field rows.
pub fn document_fields_page(
    project: &ProjectData,
    document_id: DocumentId,
    offset: usize,
    limit: usize,
) -> Result<Option<crate::field_projection::FieldPage>, crate::field_projection::ProjectionError> {
    let Some(document) = project
        .documents
        .iter()
        .find(|document| document.id == document_id)
    else {
        return Ok(None);
    };
    crate::field_projection::project_fields_page(document, offset, limit).map(Some)
}

/// Projects comparison detail directly into a bounded page. Computing the
/// comparison still uses the existing bounded model/checkpoint comparison path.
pub fn checkpoint_fields_page(
    project: &ProjectData,
    checkpoint_id: CheckpointId,
    offset: usize,
    limit: usize,
) -> Result<Option<crate::field_projection::FieldPage>, crate::field_projection::ProjectionError> {
    let Some(comparison) = compare_checkpoint(project, checkpoint_id) else {
        return Ok(None);
    };
    crate::field_projection::project_fields_page(&comparison, offset, limit).map(Some)
}

pub fn supporting_catalog_fields(
    project: &ProjectData,
    document_id: DocumentId,
) -> Option<Vec<CatalogField>> {
    let document = project.documents.iter().find(|d| d.id == document_id)?;
    Some(model_fields(serde_json::to_value(&document.catalog).ok()?))
}

/// Ordered event catalogs and owned event metadata as bounded scalar fields.
/// Qualified references retain their namespace and are never dereferenced here.
pub fn event_fields(project: &ProjectData, document_id: DocumentId) -> Option<Vec<ModelField>> {
    let document = project.documents.iter().find(|d| d.id == document_id)?;
    let events: Vec<_> = scopes(document)
        .into_iter()
        .flat_map(|scope| {
            scope
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        NodeKind::StartEvent
                            | NodeKind::EndEvent
                            | NodeKind::IntermediateCatchEvent
                            | NodeKind::IntermediateThrowEvent
                            | NodeKind::BoundaryEvent
                    )
                })
                .map(move |node| {
                    serde_json::json!({"scope":scope.id,"id":node.id,"kind":node.kind,
                    "name":node.name,"documentation":node.documentation,"details":node.details})
                })
        })
        .collect();
    Some(model_fields(serde_json::json!({
        "resources":document.event_resources,"root_definitions":document.root_event_definitions,
        "events":events
    })))
}

/// Independent DI identities, representation references and explicit attributes.
pub fn diagram_fields(project: &ProjectData, document_id: DocumentId) -> Option<Vec<ModelField>> {
    let document = project.documents.iter().find(|d| d.id == document_id)?;
    Some(model_fields(serde_json::to_value(&document.diagrams).ok()?))
}

/// Read every typed data family, including anonymous declarations and scoped
/// association expressions. References retain expanded namespace identity.
pub fn data_fields(project: &ProjectData, document_id: DocumentId) -> Option<Vec<ModelField>> {
    let document = project.documents.iter().find(|d| d.id == document_id)?;
    let processes: Vec<_> = document
        .processes
        .iter()
        .map(|p| serde_json::json!({"id":p.id,"io_specification":p.io_specification}))
        .collect();
    let scopes: Vec<_> = scopes(document).into_iter().map(|scope| {
        let nodes: Vec<_> = scope.nodes.iter().filter(|node|
            node.item_aware.is_some() || node.io_specification.is_some()
                || matches!(&node.details, Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) if !properties.is_empty() || io.is_some())
                || matches!(node.kind, NodeKind::DataObjectReference | NodeKind::DataStoreReference)
        ).map(|node| {
            let reference = match &node.details {
                Some(crate::bpmn_model::NodeDetails::DataObjectReference { .. }
                    | crate::bpmn_model::NodeDetails::DataStoreReference { .. }) => serde_json::to_value(&node.details).ok()?,
                _ => serde_json::Value::Null,
            };
            let event_data = match &node.details {
                Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) =>
                    serde_json::json!({"properties":properties,"io":io}),
                _ => serde_json::Value::Null,
            };
            Some(serde_json::json!({"id":node.id,"name":node.name,"kind":node.kind,
                "item_aware":node.item_aware,"io_specification":node.io_specification,"reference":reference,"event_data":event_data}))
        }).collect::<Option<Vec<_>>>()?;
        Some(serde_json::json!({"id":scope.id,"data_objects":scope.data_objects,
            "data_associations":scope.data_associations,"nodes":nodes}))
    }).collect::<Option<Vec<_>>>()?;
    Some(model_fields(
        serde_json::json!({"data_stores":document.data_stores,"processes":processes,"scopes":scopes}),
    ))
}

/// One semantic scope; subprocess contents remain separate from their parent.
#[derive(Clone, Copy, Debug)]
pub struct ScopeRef<'a> {
    pub id: &'a ElementIdentity,
    pub name: &'a str,
    pub nodes: &'a [Node],
    pub flows: &'a [Flow],
    pub lanes: &'a [Lane],
    pub groups: &'a [crate::bpmn_model::Group],
    pub data_objects: &'a [crate::bpmn_data::DataObject],
    pub data_associations: &'a [crate::bpmn_data::DataAssociation],
}
pub fn scopes(document: &BpmnDocument) -> Vec<ScopeRef<'_>> {
    fn visit<'a>(nodes: &'a [Node], depth: usize, out: &mut Vec<ScopeRef<'a>>) {
        if depth >= 32 {
            return;
        }
        for node in nodes {
            if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
                out.push(ScopeRef {
                    id: &node.id,
                    name: node.name.as_deref().unwrap_or(""),
                    nodes: &content.nodes,
                    flows: &content.flows,
                    lanes: &content.lanes,
                    groups: &content.groups,
                    data_objects: &content.data_objects,
                    data_associations: &content.data_associations,
                });
                visit(&content.nodes, depth + 1, out);
            }
        }
    }
    let mut result = Vec::new();
    for process in &document.processes {
        result.push(ScopeRef {
            id: &process.id,
            name: process.name.as_deref().unwrap_or(""),
            nodes: &process.nodes,
            flows: &process.flows,
            lanes: &process.lanes,
            groups: &process.groups,
            data_objects: &process.data_objects,
            data_associations: &process.data_associations,
        });
        visit(&process.nodes, 0, &mut result);
    }
    result
}
pub fn find_node<'a>(document: &'a BpmnDocument, id: &ElementSelector) -> Option<&'a Node> {
    scopes(document)
        .into_iter()
        .find_map(|scope| scope.nodes.iter().find(|node| &node.id.selector() == id))
        .or_else(|| {
            document
                .collaborations()
                .filter_map(|c| c.extras.as_ref())
                .chain(
                    document
                        .choreographies
                        .iter()
                        .filter_map(|c| c.extras.as_ref()),
                )
                .flat_map(|extras| &extras.artifacts)
                .find_map(|artifact| match artifact {
                    crate::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node)
                        if &node.id.selector() == id =>
                    {
                        Some(node.as_ref())
                    }
                    _ => None,
                })
        })
}
/// Resolve a persistent semantic or DI selector without converting its native
/// UUID into an XML identifier or allocating an identity for a legacy null.
pub fn element_identity<'a>(
    document: &'a BpmnDocument,
    selector: &ElementSelector,
) -> Option<&'a ElementIdentity> {
    document
        .element_identities()
        .into_iter()
        .find(|identity| identity.selector() == *selector)
}
pub fn find_flow<'a>(document: &'a BpmnDocument, id: &ElementSelector) -> Option<&'a Flow> {
    scopes(document)
        .into_iter()
        .find_map(|scope| scope.flows.iter().find(|flow| &flow.id.selector() == id))
        .or_else(|| {
            document
                .collaborations()
                .flat_map(|c| &c.flows)
                .find(|flow| &flow.id.selector() == id)
        })
        .or_else(|| {
            document.choreographies.iter().find_map(|choreography| {
                choreography
                    .message_flows
                    .iter()
                    .chain(&choreography.sequence_flows)
                    .find(|flow| &flow.id.selector() == id)
            })
        })
        .or_else(|| {
            document
                .collaborations()
                .filter_map(|c| c.extras.as_ref())
                .chain(
                    document
                        .choreographies
                        .iter()
                        .filter_map(|c| c.extras.as_ref()),
                )
                .flat_map(|extras| &extras.artifacts)
                .find_map(|artifact| match artifact {
                    crate::bpmn_collaboration::CollaborationArtifact::Association(flow)
                        if &flow.id.selector() == id =>
                    {
                        Some(flow.as_ref())
                    }
                    _ => None,
                })
        })
}
pub fn element_name<'a>(document: &'a BpmnDocument, id: &ElementSelector) -> Option<&'a str> {
    if let Some(task) = document.global_tasks.iter().find(|task| {
        task.id
            .as_ref()
            .is_some_and(|identity| &identity.selector() == id)
    }) {
        return Some(task.name.as_deref().unwrap_or(""));
    }
    if let Some(property) = document.correlation_properties.iter().find(|property| {
        property
            .id
            .as_ref()
            .is_some_and(|identity| &identity.selector() == id)
    }) {
        return Some(property.name.as_deref().unwrap_or(""));
    }
    if let Some(partner) = document.partner_entities.iter().find(|p| {
        p.id.as_ref()
            .is_some_and(|identity| &identity.selector() == id)
    }) {
        return Some(partner.name.as_deref().unwrap_or(""));
    }
    if let Some(partner) = document.partner_roles.iter().find(|p| {
        p.id.as_ref()
            .is_some_and(|identity| &identity.selector() == id)
    }) {
        return Some(partner.name.as_deref().unwrap_or(""));
    }
    if document.endpoints.iter().any(|p| {
        p.id.as_ref()
            .is_some_and(|identity| &identity.selector() == id)
    }) {
        return Some("");
    }
    for scope in scopes(document) {
        if &scope.id.selector() == id {
            return Some(scope.name);
        }
        if let Some(object) = scope.data_objects.iter().find(|o| {
            o.id.as_ref()
                .is_some_and(|identity| &identity.selector() == id)
        }) {
            return Some(object.name.as_deref().unwrap_or(""));
        }
        if let Some(node) = scope.nodes.iter().find(|n| &n.id.selector() == id) {
            return Some(node.name.as_deref().unwrap_or(""));
        }
        if let Some(lane) = scope.lanes.iter().find(|l| &l.id.selector() == id) {
            return Some(lane.name.as_deref().unwrap_or(""));
        }
        if let Some(flow) = scope.flows.iter().find(|f| &f.id.selector() == id) {
            return Some(flow.name.as_deref().unwrap_or(""));
        }
    }
    for choreography in &document.choreographies {
        if &choreography.id.selector() == id {
            return Some(choreography.name.as_deref().unwrap_or(""));
        }
        if let Some(node) = choreography
            .conversation_nodes
            .iter()
            .find(|n| &n.id.selector() == id)
        {
            return Some(node.name.as_deref().unwrap_or(""));
        }
        if let Some(link) = choreography
            .conversation_links
            .iter()
            .find(|n| &n.id.selector() == id)
        {
            return Some(link.name.as_deref().unwrap_or(""));
        }
        if let Some(participant) = choreography
            .participants
            .iter()
            .find(|participant| &participant.id.selector() == id)
        {
            return Some(participant.name.as_deref().unwrap_or(""));
        }
        if let Some(activity) = choreography
            .activities
            .iter()
            .find(|activity| &activity.id.selector() == id)
        {
            return Some(activity.name.as_deref().unwrap_or(""));
        }
        if let Some(flow) = choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
            .find(|flow| &flow.id.selector() == id)
        {
            return Some(flow.name.as_deref().unwrap_or(""));
        }
    }
    for collaboration in document.collaborations() {
        if &collaboration.id.selector() == id {
            return Some(collaboration.name.as_deref().unwrap_or(""));
        }
        if let Some(node) = collaboration
            .conversation_nodes
            .iter()
            .find(|node| &node.id.selector() == id)
        {
            return Some(node.name.as_deref().unwrap_or(""));
        }
        if let Some(link) = collaboration
            .conversation_links
            .iter()
            .find(|link| &link.id.selector() == id)
        {
            return Some(link.name.as_deref().unwrap_or(""));
        }
    }
    if let Some(node) = find_node(document, id) {
        return Some(node.name.as_deref().unwrap_or(""));
    }
    if let Some(flow) = find_flow(document, id) {
        return Some(flow.name.as_deref().unwrap_or(""));
    }
    document.collaborations().find_map(|c| {
        c.participants
            .iter()
            .find(|p| &p.id.selector() == id)
            .and_then(|p| p.name.as_deref())
            .or_else(|| {
                c.flows
                    .iter()
                    .find(|f| &f.id.selector() == id)
                    .and_then(|f| f.name.as_deref())
            })
    })
}
/// Resolve conversation elements within this document's collaboration only.
pub fn find_conversation_node<'a>(
    document: &'a BpmnDocument,
    id: &ElementSelector,
) -> Option<&'a crate::bpmn_collaboration::ConversationNode> {
    document
        .collaborations()
        .flat_map(|c| &c.conversation_nodes)
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| &c.conversation_nodes),
        )
        .find(|node| &node.id.selector() == id)
}
pub fn find_conversation_link<'a>(
    document: &'a BpmnDocument,
    id: &ElementSelector,
) -> Option<&'a crate::bpmn_collaboration::ConversationLink> {
    document
        .collaborations()
        .flat_map(|c| &c.conversation_links)
        .chain(
            document
                .choreographies
                .iter()
                .flat_map(|c| &c.conversation_links),
        )
        .find(|link| &link.id.selector() == id)
}
/// Resolve a choreography or task without crossing its owning document/scope.
pub fn find_choreography<'a>(
    document: &'a BpmnDocument,
    id: &ElementSelector,
) -> Option<&'a crate::bpmn_collaboration::Choreography> {
    document
        .choreographies
        .iter()
        .find(|c| &c.id.selector() == id)
}
pub fn find_choreography_activity<'a>(
    document: &'a BpmnDocument,
    choreography_id: &ElementSelector,
    activity_id: &ElementSelector,
) -> Option<&'a crate::bpmn_collaboration::ChoreographyActivity> {
    find_choreography(document, choreography_id)?
        .activities
        .iter()
        .find(|a| &a.id == activity_id)
}
pub fn library_search(project: &ProjectData, text: &str) -> Vec<DocumentId> {
    let text = text.to_lowercase();
    let mut docs: Vec<_> = project
        .documents
        .iter()
        .filter(|d| d.name.to_lowercase().contains(&text))
        .collect();
    docs.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.to_string().cmp(&b.id.to_string()))
    });
    docs.into_iter().map(|d| d.id).collect()
}
pub fn dictionary_usages(project: &ProjectData, id: DictionaryEntryId) -> Vec<ElementKey> {
    let mut keys: Vec<_> = project
        .dictionary_links
        .iter()
        .filter(|l| l.target == DictionaryTarget::Entry(id))
        .map(|l| l.source.clone())
        .collect();
    keys.sort_by(|a, b| {
        a.document_id
            .to_string()
            .cmp(&b.document_id.to_string())
            .then(a.xml_id.cmp(&b.xml_id))
    });
    keys
}
pub fn target_name<'a>(project: &'a ProjectData, target: &'a DictionaryTarget) -> Option<&'a str> {
    match target {
        DictionaryTarget::Literal(s) => Some(s),
        DictionaryTarget::Entry(id) => project
            .dictionary_entries
            .iter()
            .find(|e| e.id == *id)
            .map(|e| e.name.as_str()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoleLaneChoices {
    CreateLane,
    Assign(ElementSelector),
    Choose(Vec<ElementSelector>),
}
pub fn role_lane_choices(
    project: &ProjectData,
    document_id: DocumentId,
    process_id: &ElementSelector,
    role: DictionaryEntryId,
) -> Option<RoleLaneChoices> {
    let d = project.documents.iter().find(|d| d.id == document_id)?;
    let p = scopes(d)
        .into_iter()
        .find(|p| &p.id.selector() == process_id)?;
    let mut ids: Vec<_> = p
        .lanes
        .iter()
        .filter(|lane| {
            project.dictionary_links.iter().any(|l| {
                l.source.document_id == document_id
                    && l.source.xml_id == lane.id.selector()
                    && l.kind == DictionaryKind::Role
                    && l.target == DictionaryTarget::Entry(role)
            })
        })
        .map(|l| l.id.selector())
        .collect();
    ids.sort();
    Some(match ids.len() {
        0 => RoleLaneChoices::CreateLane,
        1 => RoleLaneChoices::Assign(ids.remove(0)),
        _ => RoleLaneChoices::Choose(ids),
    })
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TableFlow {
    pub id: ElementSelector,
    pub target: ElementSelector,
    pub condition: Option<crate::bpmn_model::Expression>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ActivityRow {
    pub key: ElementKey,
    pub name: Option<String>,
    pub lane: Option<ElementSelector>,
    pub role: Option<String>,
    pub systems: Vec<String>,
    pub documents: Vec<String>,
    pub outgoing: Vec<TableFlow>,
}
pub fn activity_table(
    project: &ProjectData,
    document_id: DocumentId,
    process_id: &ElementSelector,
) -> Option<Vec<ActivityRow>> {
    let d = project.documents.iter().find(|d| d.id == document_id)?;
    let p = scopes(d)
        .into_iter()
        .find(|p| &p.id.selector() == process_id)?;
    let index = crate::reference_resolution::ReferenceIndex::new(d);
    let resolve = |r: &Option<crate::bpmn_activity::ModelReference>| {
        r.as_ref().and_then(|r| match index.resolve(r) {
            crate::reference_resolution::ReferenceResolution::Resolved(id) => Some(id),
            _ => None,
        })
    };
    let mut rows = Vec::new();
    for n in p.nodes {
        if !n.kind.is_activity() {
            continue;
        }
        let key = ElementKey {
            document_id,
            xml_id: n.id.selector(),
        };
        let lane = p
            .lanes
            .iter()
            .find(|l| l.members.contains(&n.id.selector()));
        let labels = |source: &ElementSelector, kind| {
            project
                .dictionary_links
                .iter()
                .filter(|l| {
                    l.source.document_id == document_id
                        && &l.source.xml_id == source
                        && l.kind == kind
                })
                .filter_map(|l| target_name(project, &l.target).map(str::to_owned))
                .collect::<Vec<_>>()
        };
        let outgoing = p
            .flows
            .iter()
            .filter(|f| {
                f.kind == FlowKind::Sequence
                    && resolve(&f.source).as_ref() == Some(&n.id.selector())
            })
            .filter_map(|f| {
                Some(TableFlow {
                    id: f.id.selector(),
                    target: resolve(&f.target)?,
                    condition: f.condition.clone(),
                })
            })
            .collect();
        rows.push(ActivityRow {
            key,
            name: n.name.clone(),
            lane: lane.map(|l| l.id.selector()),
            role: lane.and_then(|l| {
                labels(&l.id.selector(), DictionaryKind::Role)
                    .into_iter()
                    .next()
            }),
            systems: labels(&n.id.selector(), DictionaryKind::System),
            documents: labels(&n.id.selector(), DictionaryKind::Document),
            outgoing,
        });
    }
    Some(rows)
}

/// True when a retained comment no longer points at an existing model element.
pub fn comment_is_orphan(project: &ProjectData, comment: &Comment) -> bool {
    !project
        .documents
        .iter()
        .find(|doc| doc.id == comment.target.document_id)
        .is_some_and(|doc| crate::identity::semantic_ids(doc).contains(&comment.target.xml_id))
}

/// Changes are grouped by semantic IDs independently of diagram geometry and frozen context.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct PropertyDifference {
    pub property: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ElementChange {
    pub key: ElementKey,
    pub before_name: Option<String>,
    pub after_name: Option<String>,
    pub properties: Vec<PropertyDifference>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiagramChange {
    pub document_id: DocumentId,
    pub diagram_id: ElementSelector,
    #[serde(default)]
    pub properties: Vec<PropertyDifference>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiagramElementChange {
    pub document_id: DocumentId,
    pub diagram_id: ElementSelector,
    pub element_id: ElementSelector,
    pub representation_id: ElementSelector,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CheckpointComparison {
    pub added_documents: Vec<DocumentId>,
    pub removed_documents: Vec<DocumentId>,
    pub changed_documents: Vec<DocumentId>,
    pub dictionary_changed: bool,
    pub resources_changed: bool,
    pub comments_changed: bool,
    pub links_changed: bool,
    pub added_links: Vec<NavigationLink>,
    pub removed_links: Vec<NavigationLink>,
    pub added_dictionary_links: Vec<DictionaryLink>,
    pub removed_dictionary_links: Vec<DictionaryLink>,
    pub project_properties_changed: bool,
    pub added_elements: Vec<ElementChange>,
    pub removed_elements: Vec<ElementChange>,
    pub renamed_elements: Vec<ElementChange>,
    pub changed_properties: Vec<ElementChange>,
    pub changed_diagrams: Vec<DiagramChange>,
    pub moved_shapes: Vec<DiagramElementChange>,
    pub rerouted_edges: Vec<DiagramElementChange>,
    pub changed_document_properties: Vec<DocumentId>,
}
pub fn compare_checkpoint(project: &ProjectData, id: CheckpointId) -> Option<CheckpointComparison> {
    let frozen = &project.checkpoints.iter().find(|c| c.id == id)?.content;
    let added_documents = project
        .documents
        .iter()
        .filter(|d| !frozen.documents.iter().any(|old| old.id == d.id))
        .map(|d| d.id)
        .collect();
    let removed_documents = frozen
        .documents
        .iter()
        .filter(|d| !project.documents.iter().any(|current| current.id == d.id))
        .map(|d| d.id)
        .collect();
    let changed_documents = project
        .documents
        .iter()
        .filter(|d| {
            frozen
                .documents
                .iter()
                .find(|old| old.id == d.id)
                .is_some_and(|old| serde_json::to_value(old).ok() != serde_json::to_value(d).ok())
        })
        .map(|d| d.id)
        .collect();
    let dictionary_changed =
        serde_json::to_value((&project.dictionary_entries, &project.dictionary_links)).ok()
            != serde_json::to_value((&frozen.dictionary_entries, &frozen.dictionary_links)).ok();
    let resource_keys = |resources: &[Resource]| {
        resources
            .iter()
            .map(|r| (r.id.clone(), r.content_hash, r.media_type.clone()))
            .collect::<Vec<_>>()
    };
    let resources_changed = resource_keys(&project.resources) != resource_keys(&frozen.resources);
    let mut added_elements = vec![];
    let mut removed_elements = vec![];
    let mut renamed_elements = vec![];
    let mut changed_properties = vec![];
    let mut changed_diagrams = vec![];
    let mut moved_shapes = vec![];
    let mut rerouted_edges = vec![];
    let mut changed_document_properties = vec![];
    for document in &project.documents {
        let Some(before) = frozen.documents.iter().find(|d| d.id == document.id) else {
            continue;
        };
        if before.name != document.name
            || before.folder != document.folder
            || serde_json::to_value(&before.definitions_id).ok()
                != serde_json::to_value(&document.definitions_id).ok()
            || before.definitions != document.definitions
            || before.extensions != document.extensions
            || before.relationships != document.relationships
        {
            changed_document_properties.push(document.id);
        }
        let old = semantic_properties(before)?;
        let new = semantic_properties(document)?;
        for (element_id, value) in &new {
            let previous = old.get(element_id);
            let change = ElementChange {
                properties: vec![],
                key: ElementKey {
                    document_id: document.id,
                    xml_id: element_id.clone(),
                },
                before_name: previous.and_then(|v| v.get("name")?.as_str().map(str::to_owned)),
                after_name: value
                    .get("name")
                    .and_then(|v| v.as_str().map(str::to_owned)),
            };
            if let Some(previous) = previous {
                if change.before_name != change.after_name {
                    renamed_elements.push(change.clone());
                }
                let mut old_props = previous.clone();
                let mut new_props = value.clone();
                old_props.as_object_mut()?.remove("name");
                new_props.as_object_mut()?.remove("name");
                if old_props != new_props {
                    let mut change = change;
                    property_differences(&old_props, &new_props, "", &mut change.properties);
                    changed_properties.push(change);
                }
            } else {
                added_elements.push(change);
            }
        }
        for (element_id, previous) in old.iter().filter(|(id, _)| !new.contains_key(*id)) {
            removed_elements.push(ElementChange {
                properties: vec![],
                key: ElementKey {
                    document_id: document.id,
                    xml_id: element_id.clone(),
                },
                before_name: previous
                    .get("name")
                    .and_then(|v| v.as_str().map(str::to_owned)),
                after_name: None,
            });
        }
        for diagram in &document.diagrams {
            if let Some(old) = before.diagrams.iter().find(|d| d.id == diagram.id) {
                for shape in &diagram.shapes {
                    if old
                        .shapes
                        .iter()
                        .find(|s| s.id == shape.id)
                        .is_some_and(|s| {
                            (s.bounds.x, s.bounds.y, s.bounds.width, s.bounds.height)
                                != (
                                    shape.bounds.x,
                                    shape.bounds.y,
                                    shape.bounds.width,
                                    shape.bounds.height,
                                )
                        })
                    {
                        moved_shapes.push(DiagramElementChange {
                            document_id: document.id,
                            diagram_id: diagram.id.selector(),
                            element_id: shape.element.clone(),
                            representation_id: shape.id.selector(),
                        });
                    }
                }
                for edge in &diagram.edges {
                    if old.edges.iter().find(|e| e.id == edge.id).is_some_and(|e| {
                        e.waypoints
                            .iter()
                            .map(|p| (p.x, p.y))
                            .ne(edge.waypoints.iter().map(|p| (p.x, p.y)))
                    }) {
                        rerouted_edges.push(DiagramElementChange {
                            document_id: document.id,
                            diagram_id: diagram.id.selector(),
                            element_id: edge.element.clone(),
                            representation_id: edge.id.selector(),
                        });
                    }
                }
            }
            if before
                .diagrams
                .iter()
                .find(|d| d.id == diagram.id)
                .is_none_or(|old| {
                    serde_json::to_value(old).ok() != serde_json::to_value(diagram).ok()
                })
            {
                let mut properties = vec![];
                let old = before
                    .diagrams
                    .iter()
                    .find(|old| old.id == diagram.id)
                    .map(serde_json::to_value)
                    .transpose()
                    .ok()?
                    .unwrap_or(serde_json::Value::Null);
                let new = serde_json::to_value(diagram).ok()?;
                diagram_property_differences(&old, &new, "", &mut properties);
                changed_diagrams.push(DiagramChange {
                    document_id: document.id,
                    diagram_id: diagram.id.selector(),
                    properties,
                });
            }
        }
        for diagram in before
            .diagrams
            .iter()
            .filter(|old| !document.diagrams.iter().any(|d| d.id == old.id))
        {
            changed_diagrams.push(DiagramChange {
                document_id: document.id,
                diagram_id: diagram.id.selector(),
                properties: vec![PropertyDifference {
                    property: String::new(),
                    before: serde_json::to_value(diagram).ok()?,
                    after: serde_json::Value::Null,
                }],
            });
        }
    }
    let navigation_equal = |a: &NavigationLink, b: &NavigationLink| {
        a.source == b.source && a.target == b.target && a.label == b.label
    };
    let dictionary_equal = |a: &DictionaryLink, b: &DictionaryLink| {
        a.source == b.source && a.kind == b.kind && a.target == b.target
    };
    let added_links = project
        .links
        .iter()
        .filter(|link| !frozen.links.iter().any(|old| navigation_equal(link, old)))
        .cloned()
        .collect();
    let removed_links = frozen
        .links
        .iter()
        .filter(|link| {
            !project
                .links
                .iter()
                .any(|current| navigation_equal(link, current))
        })
        .cloned()
        .collect();
    let added_dictionary_links = project
        .dictionary_links
        .iter()
        .filter(|link| {
            !frozen
                .dictionary_links
                .iter()
                .any(|old| dictionary_equal(link, old))
        })
        .cloned()
        .collect();
    let removed_dictionary_links = frozen
        .dictionary_links
        .iter()
        .filter(|link| {
            !project
                .dictionary_links
                .iter()
                .any(|current| dictionary_equal(link, current))
        })
        .cloned()
        .collect();
    Some(CheckpointComparison {
        added_links,
        removed_links,
        added_dictionary_links,
        removed_dictionary_links,
        comments_changed: serde_json::to_value(&project.comments).ok()
            != serde_json::to_value(&frozen.comments).ok(),
        links_changed: serde_json::to_value(&project.links).ok()
            != serde_json::to_value(&frozen.links).ok(),
        project_properties_changed: project.name != frozen.name
            || project.description != frozen.description
            || serde_json::to_value(&project.folders).ok()
                != serde_json::to_value(&frozen.folders).ok(),
        added_elements,
        removed_elements,
        renamed_elements,
        changed_properties,
        changed_diagrams,
        moved_shapes,
        rerouted_edges,
        changed_document_properties,
        added_documents,
        removed_documents,
        changed_documents,
        dictionary_changed,
        resources_changed,
    })
}

// The native typed projection is the source of truth. Child elements are references here so a
// child's rename is reported on that child, not as a property edit on every ancestor.
fn semantic_properties(
    document: &BpmnDocument,
) -> Option<std::collections::BTreeMap<ElementSelector, serde_json::Value>> {
    use serde_json::Value;
    fn selector(value: &Value) -> Option<ElementSelector> {
        serde_json::from_value::<ElementIdentity>(value.clone())
            .ok()
            .map(|identity| identity.selector())
    }
    fn shallow(value: &Value, root: bool) -> Value {
        match value {
            Value::Object(object) if !root && object.get("id").and_then(selector).is_some() => {
                serde_json::json!({ "ref": selector(&object["id"]) })
            }
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), shallow(value, false)))
                    .collect(),
            ),
            Value::Array(values) => {
                Value::Array(values.iter().map(|v| shallow(v, false)).collect())
            }
            value => value.clone(),
        }
    }
    let mut value = serde_json::to_value(document).ok()?;
    let object = value.as_object_mut()?;
    for key in [
        "id",
        "name",
        "folder",
        "definitions_id",
        "definitions",
        "diagrams",
        "provenance",
        "capability",
    ] {
        object.remove(key);
    }
    let mut result = std::collections::BTreeMap::new();
    result.insert(
        document.definitions_id.selector(),
        serde_json::json!({
            "id": document.definitions_id,
            "definitions": document.definitions,
            "content": shallow(&value, true),
        }),
    );
    let mut pending = vec![&value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                if let Some(id) = object.get("id").and_then(selector) {
                    result.insert(id, shallow(value, true));
                }
                pending.extend(object.values());
            }
            Value::Array(values) => pending.extend(values),
            _ => {}
        }
    }
    Some(result)
}

fn diagram_property_differences(
    before: &serde_json::Value,
    after: &serde_json::Value,
    path: &str,
    result: &mut Vec<PropertyDifference>,
) {
    use serde_json::Value;
    match (before, after) {
        (Value::Object(old), Value::Object(new)) => {
            let keys: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
            for key in keys {
                let pointer = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                diagram_property_differences(
                    old.get(key).unwrap_or(&Value::Null),
                    new.get(key).unwrap_or(&Value::Null),
                    &pointer,
                    result,
                );
            }
        }
        (Value::Array(old), Value::Array(new)) => {
            for index in 0..old.len().max(new.len()) {
                diagram_property_differences(
                    old.get(index).unwrap_or(&Value::Null),
                    new.get(index).unwrap_or(&Value::Null),
                    &format!("{path}/{index}"),
                    result,
                );
            }
        }
        _ if before != after => result.push(PropertyDifference {
            property: path.into(),
            before: before.clone(),
            after: after.clone(),
        }),
        _ => {}
    }
}

fn property_differences(
    before: &serde_json::Value,
    after: &serde_json::Value,
    path: &str,
    result: &mut Vec<PropertyDifference>,
) {
    if let (Some(old), Some(new)) = (before.as_object(), after.as_object()) {
        let keys: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
        for key in keys {
            let name = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            property_differences(
                old.get(key).unwrap_or(&serde_json::Value::Null),
                new.get(key).unwrap_or(&serde_json::Value::Null),
                &name,
                result,
            );
        }
    } else if before != after {
        result.push(PropertyDifference {
            property: path.into(),
            before: before.clone(),
            after: after.clone(),
        });
    }
}
