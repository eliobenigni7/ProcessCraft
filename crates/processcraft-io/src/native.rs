//! Strict, bounded ZIP/JSON native project format.
//! Schema 0 is a synthetic migration exercise: it lacks `description`.
use processcraft_core::{
    ProjectId,
    ids::{CheckpointId, XmlId},
    model::{
        BpmnDocument, Checkpoint, Comment, DictionaryEntry, DictionaryLink, Folder, FrozenProject,
        NavigationLink, ProjectData, Resource,
    },
};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MIME: &[u8] = b"application/vnd.processcraft+zip";
const JSON_LIMIT: usize = 64 * 1024 * 1024;
const RESOURCE_LIMIT: usize = 32 * 1024 * 1024;
const EXPANDED_LIMIT: usize = 256 * 1024 * 1024;
const ZIP_LIMIT: usize = 128 * 1024 * 1024;
const ENTRY_LIMIT: usize = 4096;
const DEFINITIONS_CAPABILITY: &str = "bpmn-definitions-metadata";
const EXPRESSIONS_CAPABILITY: &str = "bpmn-structured-expressions";
const EVENT_CAPABILITY: &str = "bpmn-link-cancel-compensation";
const CATALOG_CAPABILITY: &str = "bpmn-supporting-catalog";
const EMPTY_ACTIVITY_BINDING_CAPABILITY: &str = "bpmn-empty-namespace-activity-bindings";
const EXPANDED_DATA_CAPABILITY: &str = "bpmn-expanded-data";
const NATIVE_IDENTITY_CAPABILITY: &str = "bpmn-native-element-identity";

fn has_native_identity_features(
    documents: &[Arc<BpmnDocument>],
    links: &[NavigationLink],
    dictionary_links: &[DictionaryLink],
    comments: &[Comment],
) -> bool {
    documents.iter().any(|d| d.has_native_identity_features())
        || links.iter().any(|l| l.source.xml_id.is_native())
        || dictionary_links.iter().any(|l| l.source.xml_id.is_native())
        || comments.iter().any(|c| c.target.xml_id.is_native())
}

fn has_empty_activity_bindings(document: &BpmnDocument) -> bool {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut pending: Vec<_> = document
        .processes
        .iter()
        .map(|p| p.nodes.as_slice())
        .collect();
    while let Some(nodes) = pending.pop() {
        for node in nodes {
            if node.activity.as_ref().is_some_and(|a| {
                [&a.message_ref, &a.operation_ref]
                    .into_iter()
                    .flatten()
                    .any(|r| r.namespace.is_empty())
            }) {
                return true;
            }
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                pending.push(&content.nodes);
            }
        }
    }
    false
}

fn has_extended_event_definitions(document: &BpmnDocument) -> bool {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut pending: Vec<_> = document
        .processes
        .iter()
        .map(|p| p.nodes.as_slice())
        .collect();
    while let Some(nodes) = pending.pop() {
        for node in nodes {
            match &node.details {
                Some(NodeDetails::SubProcess { content }) => pending.push(&content.nodes),
                Some(NodeDetails::Event { definitions, .. })
                    if definitions.iter().any(|d| {
                        matches!(
                            d,
                            EventDefinition::Link { .. }
                                | EventDefinition::Cancel { .. }
                                | EventDefinition::Compensate { .. }
                        )
                    }) =>
                {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

#[derive(Debug)]
pub struct NativeError(pub String);
impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NativeError {}
fn err(e: impl fmt::Display) -> NativeError {
    NativeError(e.to_string())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    schema: u32,
    min_reader: u32,
    project_id: ProjectId,
    content_identity: [u8; 32],
    required_capabilities: Vec<String>,
    writer_version: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectDto {
    id: ProjectId,
    name: String,
    description: String,
    folders: Vec<Folder>,
    documents: Vec<Arc<BpmnDocument>>,
    resources: Vec<ResourceDto>,
    links: Vec<NavigationLink>,
    dictionary_entries: Vec<DictionaryEntry>,
    dictionary_links: Vec<DictionaryLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    checkpoints: Vec<CheckpointMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference_checkpoint: Option<CheckpointId>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointMeta {
    id: CheckpointId,
    name: String,
    created_unix_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenDto {
    id: ProjectId,
    name: String,
    description: String,
    folders: Vec<Folder>,
    documents: Vec<Arc<BpmnDocument>>,
    resources: Vec<ResourceDto>,
    links: Vec<NavigationLink>,
    dictionary_entries: Vec<DictionaryEntry>,
    dictionary_links: Vec<DictionaryLink>,
    comments: Vec<Comment>,
}
fn resource_dto(r: &Resource) -> ResourceDto {
    ResourceDto {
        id: r.id.clone(),
        media_type: r.media_type.clone(),
        content_hash: r.content_hash,
        size: r.bytes.len() as u64,
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectV1 {
    id: ProjectId,
    name: String,
    description: String,
    folders: Vec<Folder>,
    documents: Vec<Arc<BpmnDocument>>,
    resources: Vec<ResourceDto>,
    links: Vec<NavigationLink>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectV0 {
    id: ProjectId,
    name: String,
    folders: Vec<Folder>,
    documents: Vec<Arc<BpmnDocument>>,
    resources: Vec<ResourceDto>,
    links: Vec<NavigationLink>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceDto {
    id: XmlId,
    media_type: String,
    content_hash: [u8; 32],
    size: u64,
}

/// A migrated archive must be saved to a new location or backed up before overwrite.
pub struct LoadedProject {
    pub project: ProjectData,
    /// Verified identity recorded by the source schema, before in-memory migration.
    pub source_content_identity: [u8; 32],
    pub requires_save_as_or_backup: bool,
}
fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Serialize a validated project to a complete native archive.
pub fn write_bytes(project: &ProjectData) -> Result<Vec<u8>, NativeError> {
    let identity = project.content_identity().map_err(err)?;
    let dto = ProjectDto {
        id: project.id,
        name: project.name.clone(),
        description: project.description.clone(),
        folders: project.folders.clone(),
        documents: project.documents.clone(),
        links: project.links.clone(),
        dictionary_entries: project.dictionary_entries.clone(),
        dictionary_links: project.dictionary_links.clone(),
        comments: project.comments.clone(),
        checkpoints: project
            .checkpoints
            .iter()
            .map(|c| CheckpointMeta {
                id: c.id,
                name: c.name.clone(),
                created_unix_ms: c.created_unix_ms,
            })
            .collect(),
        reference_checkpoint: project.reference_checkpoint,
        resources: project
            .resources
            .iter()
            .map(|r| ResourceDto {
                id: r.id.clone(),
                media_type: r.media_type.clone(),
                content_hash: r.content_hash,
                size: r.bytes.len() as u64,
            })
            .collect(),
    };
    let mut required_capabilities = vec!["comments".into(), "checkpoints".into()];
    if project.documents.iter().any(|d| d.definitions.is_some())
        || project
            .checkpoints
            .iter()
            .any(|c| c.content.documents.iter().any(|d| d.definitions.is_some()))
    {
        required_capabilities.push(DEFINITIONS_CAPABILITY.into());
    }
    if project
        .documents
        .iter()
        .any(|d| d.has_structured_expressions())
        || project.checkpoints.iter().any(|c| {
            c.content
                .documents
                .iter()
                .any(|d| d.has_structured_expressions())
        })
    {
        required_capabilities.push(EXPRESSIONS_CAPABILITY.into());
    }
    if project
        .documents
        .iter()
        .any(|d| has_extended_event_definitions(d))
        || project.checkpoints.iter().any(|c| {
            c.content
                .documents
                .iter()
                .any(|d| has_extended_event_definitions(d))
        })
    {
        required_capabilities.push(EVENT_CAPABILITY.into());
    }
    if project.documents.iter().any(|d| !d.catalog.is_empty())
        || project
            .checkpoints
            .iter()
            .any(|c| c.content.documents.iter().any(|d| !d.catalog.is_empty()))
    {
        required_capabilities.push(CATALOG_CAPABILITY.into());
    }
    if project
        .documents
        .iter()
        .any(|d| has_empty_activity_bindings(d))
        || project.checkpoints.iter().any(|c| {
            c.content
                .documents
                .iter()
                .any(|d| has_empty_activity_bindings(d))
        })
    {
        required_capabilities.push(EMPTY_ACTIVITY_BINDING_CAPABILITY.into());
    }
    if project.documents.iter().any(|d| d.has_expanded_data())
        || project
            .checkpoints
            .iter()
            .any(|c| c.content.documents.iter().any(|d| d.has_expanded_data()))
    {
        required_capabilities.push(EXPANDED_DATA_CAPABILITY.into());
    }
    if has_native_identity_features(
        &project.documents,
        &project.links,
        &project.dictionary_links,
        &project.comments,
    ) || project.checkpoints.iter().any(|c| {
        has_native_identity_features(
            &c.content.documents,
            &c.content.links,
            &c.content.dictionary_links,
            &c.content.comments,
        )
    }) {
        required_capabilities.push(NATIVE_IDENTITY_CAPABILITY.into());
    }
    let manifest = Manifest {
        format: "processcraft".into(),
        schema: 3,
        min_reader: 3,
        project_id: project.id,
        content_identity: identity,
        required_capabilities,
        writer_version: env!("CARGO_PKG_VERSION").into(),
    };
    let manifest = serde_json::to_vec(&manifest).map_err(err)?;
    let json = serde_json::to_vec(&dto).map_err(err)?;
    if json.len() > JSON_LIMIT || manifest.len() > JSON_LIMIT {
        return Err(err("JSON exceeds limit"));
    }
    let mut payloads = Vec::new();
    for checkpoint in &project.checkpoints {
        let c = &checkpoint.content;
        let dto = FrozenDto {
            id: c.id,
            name: c.name.clone(),
            description: c.description.clone(),
            folders: c.folders.clone(),
            documents: c.documents.clone(),
            resources: c.resources.iter().map(resource_dto).collect(),
            links: c.links.clone(),
            dictionary_entries: c.dictionary_entries.clone(),
            dictionary_links: c.dictionary_links.clone(),
            comments: c.comments.clone(),
        };
        let bytes = serde_json::to_vec(&dto).map_err(err)?;
        if bytes.len() > JSON_LIMIT {
            return Err(err("checkpoint JSON exceeds limit"));
        }
        payloads.push((format!("checkpoints/{}.json", checkpoint.id), bytes));
    }
    let mut unique = HashMap::new();
    for r in project.resources.iter().chain(
        project
            .checkpoints
            .iter()
            .flat_map(|c| c.content.resources.iter()),
    ) {
        if r.bytes.len() > RESOURCE_LIMIT || digest(&r.bytes) != r.content_hash {
            return Err(err("invalid resource"));
        }
        unique.insert(r.content_hash, &r.bytes);
    }
    if unique.len() + payloads.len() + 3 > ENTRY_LIMIT {
        return Err(err("entry limit exceeded"));
    }
    let expanded = unique
        .values()
        .try_fold(MIME.len() + manifest.len() + json.len(), |n, b| {
            n.checked_add(b.len())
        })
        .ok_or_else(|| err("size overflow"))?;
    let expanded = payloads
        .iter()
        .try_fold(expanded, |n, (_, bytes)| n.checked_add(bytes.len()))
        .ok_or_else(|| err("size overflow"))?;
    if expanded > EXPANDED_LIMIT {
        return Err(err("expanded limit exceeded"));
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(1));
    writer
        .start_file(
            "mimetype",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .map_err(err)?;
    writer.write_all(MIME).map_err(err)?;
    for (name, bytes) in [
        ("manifest.json", manifest.as_slice()),
        ("project.json", json.as_slice()),
    ] {
        writer.start_file(name, opts).map_err(err)?;
        writer.write_all(bytes).map_err(err)?;
    }
    for (name, payload) in payloads {
        writer.start_file(name, opts).map_err(err)?;
        writer.write_all(&payload).map_err(err)?;
    }
    let mut resources: Vec<_> = unique.into_iter().collect();
    resources.sort_by_key(|(h, _)| *h);
    for (hash, bytes) in resources {
        writer
            .start_file(format!("resources/{}", hex(&hash)), opts)
            .map_err(err)?;
        writer.write_all(bytes).map_err(err)?;
    }
    let bytes = writer.finish().map_err(err)?.into_inner();
    if bytes.len() > ZIP_LIMIT {
        return Err(err("compressed limit exceeded"));
    }
    Ok(bytes)
}

/// Load a project without extracting paths to the filesystem.
pub fn load_bytes(bytes: &[u8]) -> Result<LoadedProject, NativeError> {
    load_with_limits(bytes, JSON_LIMIT, EXPANDED_LIMIT)
}

fn load_with_limits(
    bytes: &[u8],
    json_limit: usize,
    expanded_limit: usize,
) -> Result<LoadedProject, NativeError> {
    if bytes.len() > ZIP_LIMIT {
        return Err(err("compressed limit exceeded"));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(err)?;
    if archive.len() > ENTRY_LIMIT {
        return Err(err("entry limit exceeded"));
    }
    let mut entries = HashMap::new();
    let mut expanded = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(err)?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || entry.enclosed_name().is_none()
            || name.contains('\\')
            || name.starts_with('/')
            || name
                .split('/')
                .any(|x| x.is_empty() || x == "." || x == "..")
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err(err("unsafe archive entry"));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(err("unsupported compression"));
        }
        if entries.contains_key(&name) {
            return Err(err("duplicate entry"));
        }
        let cap = if name == "mimetype" {
            128
        } else if name == "manifest.json"
            || name == "project.json"
            || (name.starts_with("checkpoints/") && name.ends_with(".json"))
        {
            json_limit
        } else if name.starts_with("resources/") || name == "preview.png" {
            RESOURCE_LIMIT
        } else {
            return Err(err("unmodeled archive entry"));
        };
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((cap as u64) + 1)
            .read_to_end(&mut data)
            .map_err(err)?;
        if data.len() > cap {
            return Err(err("entry exceeds limit"));
        }
        expanded = expanded
            .checked_add(data.len())
            .ok_or_else(|| err("size overflow"))?;
        if expanded > expanded_limit {
            return Err(err("expanded limit exceeded"));
        }
        entries.insert(name, data);
    }
    if entries.remove("mimetype").as_deref() != Some(MIME) {
        return Err(err("missing or invalid mimetype"));
    }
    if let Some(preview) = entries.remove("preview.png") {
        validate_preview(&preview)?;
    }
    let manifest: Manifest = parse_json(
        &entries
            .remove("manifest.json")
            .ok_or_else(|| err("missing manifest"))?,
    )?;
    if manifest.format != "processcraft"
        || manifest.schema > 3
        || manifest.min_reader > 3
        || manifest.required_capabilities.iter().any(|cap| {
            cap != "comments"
                && cap != "checkpoints"
                && cap != DEFINITIONS_CAPABILITY
                && cap != EXPRESSIONS_CAPABILITY
                && cap != EVENT_CAPABILITY
                && cap != CATALOG_CAPABILITY
                && cap != EMPTY_ACTIVITY_BINDING_CAPABILITY
                && cap != EXPANDED_DATA_CAPABILITY
                && cap != NATIVE_IDENTITY_CAPABILITY
        })
        || (manifest.schema < 3 && !manifest.required_capabilities.is_empty())
        || (manifest.schema == 3
            && !manifest
                .required_capabilities
                .iter()
                .any(|cap| cap == "comments"))
    {
        return Err(err("unsupported format or required capability"));
    }
    if manifest.schema == 3
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == "checkpoints")
    {
        return Err(err("unsupported format or required capability"));
    }
    let raw = entries
        .remove("project.json")
        .ok_or_else(|| err("missing project"))?;
    let dto = if manifest.schema == 0 {
        let old: ProjectV0 = parse_json(&raw)?;
        ProjectDto {
            id: old.id,
            name: old.name,
            description: String::new(),
            folders: old.folders,
            documents: old.documents,
            resources: old.resources,
            links: old.links,
            dictionary_entries: vec![],
            dictionary_links: vec![],
            comments: vec![],
            checkpoints: vec![],
            reference_checkpoint: None,
        }
    } else if manifest.schema == 1 {
        let old: ProjectV1 = parse_json(&raw)?;
        ProjectDto {
            id: old.id,
            name: old.name,
            description: old.description,
            folders: old.folders,
            documents: old.documents,
            resources: old.resources,
            links: old.links,
            dictionary_entries: vec![],
            dictionary_links: vec![],
            comments: vec![],
            checkpoints: vec![],
            reference_checkpoint: None,
        }
    } else {
        parse_json(&raw)?
    };
    if manifest.schema < 3
        && (!dto.comments.is_empty()
            || !dto.checkpoints.is_empty()
            || dto.reference_checkpoint.is_some())
    {
        return Err(err("comments require schema 3"));
    }
    if dto.checkpoints.len() > 20 {
        return Err(err("checkpoint limit exceeded"));
    }
    let mut frozen = Vec::new();
    for meta in dto.checkpoints {
        let path = format!("checkpoints/{}.json", meta.id);
        let raw = entries
            .remove(&path)
            .ok_or_else(|| err("missing checkpoint payload"))?;
        let payload: FrozenDto = parse_json(&raw)?;
        frozen.push((meta, payload));
    }
    if (dto.documents.iter().any(|d| d.definitions.is_some())
        || frozen
            .iter()
            .any(|(_, c)| c.documents.iter().any(|d| d.definitions.is_some())))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == DEFINITIONS_CAPABILITY)
    {
        return Err(err("definitions metadata requires capability"));
    }
    if (dto.documents.iter().any(|d| d.has_structured_expressions())
        || frozen
            .iter()
            .any(|(_, c)| c.documents.iter().any(|d| d.has_structured_expressions())))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == EXPRESSIONS_CAPABILITY)
    {
        return Err(err("structured expressions require capability"));
    }
    if (dto
        .documents
        .iter()
        .any(|d| has_extended_event_definitions(d))
        || frozen.iter().any(|(_, c)| {
            c.documents
                .iter()
                .any(|d| has_extended_event_definitions(d))
        }))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == EVENT_CAPABILITY)
    {
        return Err(err(
            "link, cancel and compensation events require capability",
        ));
    }
    if (dto.documents.iter().any(|d| !d.catalog.is_empty())
        || frozen
            .iter()
            .any(|(_, c)| c.documents.iter().any(|d| !d.catalog.is_empty())))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == CATALOG_CAPABILITY)
    {
        return Err(err("supporting catalog requires capability"));
    }
    if (dto.documents.iter().any(|d| has_empty_activity_bindings(d))
        || frozen
            .iter()
            .any(|(_, c)| c.documents.iter().any(|d| has_empty_activity_bindings(d))))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == EMPTY_ACTIVITY_BINDING_CAPABILITY)
    {
        return Err(err("empty namespace activity bindings require capability"));
    }
    if (dto.documents.iter().any(|d| d.has_expanded_data())
        || frozen
            .iter()
            .any(|(_, c)| c.documents.iter().any(|d| d.has_expanded_data())))
        && !manifest
            .required_capabilities
            .iter()
            .any(|cap| cap == EXPANDED_DATA_CAPABILITY)
    {
        return Err(err("expanded data requires capability"));
    }
    if (has_native_identity_features(
        &dto.documents,
        &dto.links,
        &dto.dictionary_links,
        &dto.comments,
    ) || frozen.iter().any(|(_, c)| {
        has_native_identity_features(&c.documents, &c.links, &c.dictionary_links, &c.comments)
    })) && !manifest
        .required_capabilities
        .iter()
        .any(|cap| cap == NATIVE_IDENTITY_CAPABILITY)
    {
        return Err(err("native element identities require capability"));
    }
    for resources in std::iter::once(&dto.resources).chain(frozen.iter().map(|(_, c)| &c.resources))
    {
        let mut declared_size = 0u64;
        for r in resources {
            declared_size = declared_size
                .checked_add(r.size)
                .ok_or_else(|| err("resource size overflow"))?;
            if r.size > RESOURCE_LIMIT as u64 || declared_size > EXPANDED_LIMIT as u64 {
                return Err(err("logical resources exceed limit"));
            }
        }
    }
    let mut cache: HashMap<[u8; 32], Arc<[u8]>> = HashMap::new();
    let mut hydrate = |items: Vec<ResourceDto>| -> Result<Vec<Resource>, NativeError> {
        let mut result = Vec::with_capacity(items.len());
        for r in items {
            let bytes = if let Some(bytes) = cache.get(&r.content_hash) {
                Arc::clone(bytes)
            } else {
                let path = format!("resources/{}", hex(&r.content_hash));
                let data = entries
                    .remove(&path)
                    .ok_or_else(|| err("missing resource"))?;
                if data.len() as u64 != r.size || digest(&data) != r.content_hash {
                    return Err(err("resource digest or size mismatch"));
                }
                let bytes: Arc<[u8]> = Arc::from(data);
                cache.insert(r.content_hash, Arc::clone(&bytes));
                bytes
            };
            if bytes.len() as u64 != r.size {
                return Err(err("resource size mismatch"));
            }
            result.push(Resource {
                id: r.id,
                media_type: r.media_type,
                content_hash: r.content_hash,
                bytes,
            });
        }
        Ok(result)
    };
    let resources = hydrate(dto.resources)?;
    let mut checkpoints = Vec::new();
    for (meta, c) in frozen {
        let resources = hydrate(c.resources)?;
        checkpoints.push(Checkpoint {
            id: meta.id,
            name: meta.name,
            created_unix_ms: meta.created_unix_ms,
            content: FrozenProject {
                id: c.id,
                name: c.name,
                description: c.description,
                folders: c.folders,
                documents: c.documents,
                resources,
                links: c.links,
                dictionary_entries: c.dictionary_entries,
                dictionary_links: c.dictionary_links,
                comments: c.comments,
            },
        });
    }
    if !entries.is_empty() {
        return Err(err("unmodeled archive entry"));
    }
    let project = ProjectData {
        id: dto.id,
        name: dto.name,
        description: dto.description,
        folders: dto.folders,
        documents: dto.documents,
        resources,
        links: dto.links,
        dictionary_entries: dto.dictionary_entries,
        dictionary_links: dto.dictionary_links,
        comments: dto.comments,
        checkpoints,
        reference_checkpoint: dto.reference_checkpoint,
    };
    if project.id != manifest.project_id
        || (if manifest.schema < 2 {
            project.legacy_content_identity()
        } else {
            project.content_identity()
        })
        .map_err(err)?
            != manifest.content_identity
    {
        return Err(err("project identity mismatch"));
    }
    Ok(LoadedProject {
        project,
        source_content_identity: manifest.content_identity,
        requires_save_as_or_backup: manifest.schema < 3,
    })
}

// Traverse every object before typed deserialization: serde_json::Value alone loses duplicate keys.
fn parse_json<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, NativeError> {
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Check.deserialize(&mut d).map_err(err)?;
    d.end().map_err(err)?;
    let input: serde_json::Value = serde_json::from_slice(bytes).map_err(err)?;
    let parsed: T = serde_json::from_slice(bytes).map_err(err)?;
    if serde_json::to_value(&parsed).map_err(err)? != input {
        return Err(err("unmodeled JSON content"));
    }
    Ok(parsed)
}
struct Check;
impl<'de> DeserializeSeed<'de> for Check {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(Check)
    }
}
impl<'de> Visitor<'de> for Check {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON value")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_string<E: de::Error>(self, _: String) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while a.next_element_seed(Check)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut keys = HashSet::new();
        while let Some(key) = a.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            a.next_value_seed(Check)?;
        }
        Ok(())
    }
}

fn validate_preview(data: &[u8]) -> Result<(), NativeError> {
    let decoder = png::Decoder::new(Cursor::new(data));
    let mut reader = decoder.read_info().map_err(err)?;
    let info = reader.info();
    if u64::from(info.width) * u64::from(info.height) > 4_000_000 {
        return Err(err("preview exceeds pixel limit"));
    }
    let size = reader.output_buffer_size();
    if size > 32 * 1024 * 1024 {
        return Err(err("preview exceeds decoded limit"));
    }
    let mut buffer = vec![0; size];
    reader.next_frame(&mut buffer).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    #[test]
    fn expanded_data_traversal_covers_independent_nested_representations() {
        use processcraft_core::{
            bpmn_data::{
                DataAssignment, DataAssociation, DataAssociationKind, DataObject, DataPort,
                DataStoreRef, IoSpecification,
            },
            bpmn_model::{Expression, ExpressionData, ExpressionKind, NodeDetails, QNameRef},
            model::{FlowContainer, Node, NodeKind, Process},
        };
        let id = |s: &str| s.parse().unwrap();
        let node = |name: &str, kind, details| Node {
            item_aware: None,
            id: id(name),
            name: Some(String::new()),
            documentation: String::new(),
            kind,
            details,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        };
        let document = || {
            let mut d = BpmnDocument::new("document".into(), id("defs"));
            d.processes.push(Process {
                is_executable: Some(false),
                io_specification: None,
                lane_set_id: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                id: id("process"),
                name: Some(String::new()),
                documentation: String::new(),
                nodes: vec![node("task", NodeKind::Task, None)],
                lanes: vec![],
                flows: vec![],
            });
            d
        };
        let port = |id_value| DataPort {
            id: id_value,
            name: Some(String::new()),
            item_aware: None,
            is_collection: None,
        };
        let association = || DataAssociation {
            id: Some(id("association")),
            owner: "task".parse().unwrap(),
            kind: DataAssociationKind::Input,
            sources: vec!["source".parse().unwrap()],
            target: "target".parse().unwrap(),
            transformation: None,
            assignments: vec![],
        };
        let formal = || {
            Expression::Structured(ExpressionData {
                id: Some(id("expression")),
                kind: ExpressionKind::Formal {
                    explicit_type: false,
                    language: None,
                    evaluates_to_type_ref: None,
                },
                text: "x".into(),
                namespaces: vec![],
            })
        };
        let mut d = document();
        assert!(!d.has_expanded_data());
        d.processes[0].io_specification = Some(IoSpecification::default());
        assert!(d.has_expanded_data());
        let mut d = document();
        d.processes[0].data_objects.push(DataObject {
            id: None,
            name: Some(String::new()),
            is_collection: None,
            item_aware: None,
        });
        assert!(d.has_expanded_data());
        let mut d = document();
        let mut a = association();
        a.id = None;
        d.processes[0].data_associations.push(a);
        assert!(d.has_expanded_data());
        let mut d = document();
        d.processes[0].nodes[0].io_specification = Some(IoSpecification {
            inputs: vec![port(None)],
            ..Default::default()
        });
        assert!(d.has_expanded_data());
        let mut d = document();
        d.processes[0].nodes.push(node(
            "ref",
            NodeKind::DataStoreReference,
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(DataStoreRef::Qualified(QNameRef {
                    namespace: "urn:external".into(),
                    local: "store".parse().unwrap(),
                })),
            }),
        ));
        assert!(d.has_expanded_data());
        let mut d = document();
        let mut a = association();
        a.transformation = Some(formal());
        d.processes[0].data_associations.push(a);
        assert!(d.has_expanded_data() && d.has_structured_expressions());
        assert!(
            matches!(d.expressions()[0], Expression::Structured(ExpressionData { id: Some(value), .. }) if value == &id("expression"))
        );
        let mut d = document();
        let mut a = association();
        a.assignments.push(DataAssignment {
            id: None,
            from: formal(),
            to: Expression::Legacy("target".into()),
        });
        d.processes[0].data_associations.push(a);
        assert!(d.has_expanded_data() && d.has_structured_expressions());
        let mut d = document();
        let mut inner = node("inner", NodeKind::Task, None);
        inner.io_specification = Some(IoSpecification {
            outputs: vec![port(None)],
            ..Default::default()
        });
        d.processes[0].nodes.push(node(
            "sub",
            NodeKind::SubProcess,
            Some(NodeDetails::SubProcess {
                content: Box::new(FlowContainer {
                    nodes: vec![inner],
                    ..Default::default()
                }),
            }),
        ));
        assert!(d.has_expanded_data());
    }

    #[test]
    fn event_capability_detects_all_variants_in_nested_scopes() {
        use processcraft_core::{
            bpmn_model::{EventDefinition, NodeDetails},
            model::{FlowContainer, Node, NodeKind, Process},
        };
        let node = |id: &str, kind, details| Node {
            item_aware: None,
            id: id.parse().unwrap(),
            name: Some(String::new()),
            documentation: String::new(),
            kind,
            details: Some(details),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        };
        for event in [
            EventDefinition::Link {
                id: None,
                name: "link".into(),
                sources: vec![],
                target: None,
            },
            EventDefinition::Cancel { id: None },
            EventDefinition::Compensate {
                id: None,
                wait_for_completion: None,
                activity_ref: None,
            },
        ] {
            let inner = node(
                "event",
                NodeKind::IntermediateThrowEvent,
                NodeDetails::Event {
                    definitions: vec![event],
                    attached_to: None,
                    cancel_activity: None,
                },
            );
            let outer = node(
                "sub",
                NodeKind::SubProcess,
                NodeDetails::SubProcess {
                    content: Box::new(FlowContainer {
                        nodes: vec![inner],
                        ..Default::default()
                    }),
                },
            );
            let mut document = BpmnDocument::new("document".into(), "defs".parse().unwrap());
            document.processes.push(Process {
                is_executable: Some(false),
                io_specification: None,
                id: "process".parse().unwrap(),
                name: Some(String::new()),
                documentation: String::new(),
                lane_set_id: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                nodes: vec![outer],
                lanes: vec![],
                flows: vec![],
            });
            assert!(has_extended_event_definitions(&document));
        }
    }

    fn compressed(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn actual_decompressed_json_is_bounded() {
        let data = vec![b' '; 16_384];
        let bytes = compressed(&[("project.json", &data)]);
        assert!(bytes.len() < 1024);
        let error = load_with_limits(&bytes, 1024, 32_768).err().unwrap();
        assert!(error.to_string().contains("entry exceeds limit"));
    }

    #[test]
    fn actual_decompressed_entries_share_aggregate_budget() {
        let a = vec![b' '; 2048];
        let bytes = compressed(&[("project.json", &a), ("manifest.json", &a)]);
        assert!(bytes.len() < 1024);
        let error = load_with_limits(&bytes, 4096, 3000).err().unwrap();
        assert!(error.to_string().contains("expanded limit exceeded"));
    }

    #[test]
    fn highly_compressible_valid_project_loads() {
        let mut project = ProjectData::new("compressible".into());
        project.description = "a".repeat(500_000);
        let bytes = write_bytes(&project).unwrap();
        assert!(bytes.len() < 20_000);
        let result = load_bytes(&bytes).unwrap();
        assert_eq!(
            result.project.content_identity().unwrap(),
            project.content_identity().unwrap()
        );
    }

    #[test]
    fn oversized_preview_is_rejected_before_pixel_allocation() {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2001, 2000);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![0; 2001 * 2000]).unwrap();
        }
        let bytes = compressed(&[("mimetype", MIME), ("preview.png", &png)]);
        let error = load_bytes(&bytes).err().unwrap();
        assert!(error.to_string().contains("preview exceeds pixel limit"));
    }
}
