//! Structural validity and content identity of a project.
//!
//! `ProjectData::validate` is the hard structural check (unique IDs, resolvable references,
//! bounded sizes) that every mutation must preserve; it is distinct from the non-blocking
//! editing diagnostics in `validation.rs`. This module also computes the SHA-256 content identity
//! of canonical persistent content and holds `CheckpointCache`.
use crate::attributes::MAX_TEXT;
use crate::ids::XmlId;
use crate::model::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

mod document_checks;

/// Maximum entries in a per-element list (imports, ports, operations, assignments, ...).
const MAX_COLLECTION_ITEMS: usize = 256;
/// Maximum flow/graph elements counted in one document.
const MAX_DOCUMENT_ELEMENTS: usize = 10_000;
/// Maximum elements counted across the whole project.
const MAX_PROJECT_ELEMENTS: usize = 100_000;
/// Maximum documents in one project.
const MAX_DOCUMENTS: usize = 500;
/// Maximum items in a serialized JSON array.
const MAX_JSON_ARRAY_ITEMS: usize = 100_000;
/// Maximum bytes of a serialized JSON string or comment text (1 MiB).
const MAX_LONG_TEXT_BYTES: usize = 1_048_576;
/// JSON nesting limit, matching serde_json's reader.
const MAX_JSON_DEPTH: usize = 128;
/// Largest absolute diagram coordinate or extent accepted.
const MAX_COORDINATE: f64 = 1_000_000_000.0;

fn fail<T>(message: &str) -> Result<T, String> {
    Err(message.into())
}
fn add<'a, T: Eq + std::hash::Hash>(set: &mut HashSet<&'a T>, id: &'a T) -> Result<(), String> {
    if set.insert(id) {
        Ok(())
    } else {
        fail("duplicate XML ID")
    }
}
fn bounds(b: &Bounds) -> Result<(), String> {
    if [b.x, b.y, b.width, b.height]
        .iter()
        .all(|n| n.is_finite() && n.abs() <= MAX_COORDINATE)
        && b.width >= 0.0
        && b.height >= 0.0
    {
        Ok(())
    } else {
        fail("invalid geometry")
    }
}
fn di_metadata(metadata: &Option<crate::model::DiElementMetadata>) -> Result<(), String> {
    if let Some(metadata) = metadata {
        crate::bpmn_xml::validate_di_foreign_attributes(
            &metadata.foreign_attributes,
            &metadata.namespaces,
        )?;
        if let Some(extension) = &metadata.extension {
            crate::bpmn_xml::validate_di_extension(extension)?;
        }
    }
    Ok(())
}
fn label(l: &Option<Label>) -> Result<(), String> {
    if let Some(value) = l {
        di_metadata(&value.di_metadata)?;
    }
    if let Some(l) = l
        && let Some(b) = &l.bounds
    {
        bounds(b)?;
    }
    Ok(())
}
/// `depth` counts the arrays and objects enclosing `v` in the whole project JSON.
/// Nesting is bounded exactly like serde_json's reader (127 levels), so a value
/// serialized from the model is readable back without reparsing it here.
fn inspect(v: &Value, depth: usize) -> Result<(), String> {
    if matches!(v, Value::Array(_) | Value::Object(_)) && depth + 1 >= MAX_JSON_DEPTH {
        return fail("project JSON is not readable: recursion limit exceeded");
    }
    match v {
        Value::String(s) if s.len() > MAX_LONG_TEXT_BYTES => fail("string exceeds 1 MiB"),
        Value::Array(a) => {
            if a.len() > MAX_JSON_ARRAY_ITEMS {
                return fail("collection exceeds limit");
            }
            for x in a {
                inspect(x, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(o) => {
            for x in o.values() {
                inspect(x, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
fn short(s: &str) -> Result<(), String> {
    if s.len() > MAX_TEXT {
        fail("label exceeds 16 KiB")
    } else {
        Ok(())
    }
}
fn definitions_text(s: &str) -> Result<(), String> {
    short(s)?;
    if s.chars().any(|c| !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
        return fail("Definitions metadata contains a character forbidden in XML 1.0");
    }
    Ok(())
}
fn validate_event_payload(
    d: &BpmnDocument,
    definition: &crate::bpmn_model::EventDefinition,
    namespace: Option<&str>,
) -> Result<(), String> {
    use crate::bpmn_model::{EventDefinition as E, EventReference, EventResource};
    let valid_qname = |r: &EventReference| -> Result<(), String> {
        if let EventReference::Qualified(name) = r {
            definitions_text(&name.namespace)?;
            if name.namespace.chars().any(char::is_whitespace) {
                return fail("Invalid event QName namespace");
            }
        }
        Ok(())
    };
    let resource = |reference: &Option<EventReference>, kind: &str| -> Result<(), String> {
        let Some(reference) = reference else {
            return Ok(());
        };
        valid_qname(reference)?;
        if reference.is_external(namespace) {
            return Ok(());
        }
        if !d.event_resources.iter().any(|item| match (kind, item) {
            ("message", EventResource::Message { id, .. })
            | ("signal", EventResource::Signal { id, .. })
            | ("error", EventResource::Error { id, .. })
            | ("escalation", EventResource::Escalation { id, .. }) => {
                reference.matches_identity(id, namespace)
            }
            _ => false,
        }) {
            return fail("Missing or wrong-kind local event resource");
        }
        Ok(())
    };
    match definition {
        E::Message {
            message_ref,
            operation_ref,
            ..
        } => {
            resource(message_ref, "message")?;
            if let Some(reference) = operation_ref {
                valid_qname(reference)?;
                if !reference.is_external(namespace)
                    && !d
                        .catalog
                        .interfaces
                        .iter()
                        .flat_map(|i| &i.operations)
                        .any(|o| {
                            o.id.as_ref()
                                .is_some_and(|id| reference.matches_identity(id, namespace))
                        })
                {
                    return fail("Missing or wrong-kind local event operation");
                }
            }
        }
        E::Signal { signal_ref, .. } => resource(signal_ref, "signal")?,
        E::Error { error_ref, .. } => resource(error_ref, "error")?,
        E::Escalation { escalation_ref, .. } => resource(escalation_ref, "escalation")?,
        E::Timer {
            kind, expression, ..
        } => {
            if kind.is_some() != expression.is_some() {
                return fail("Incomplete timer choice");
            }
            if expression
                .as_ref()
                .is_some_and(|e| e.plain_text().is_some_and(|s| s.len() > MAX_TEXT))
            {
                return fail("Invalid timer expression");
            }
        }
        E::Conditional { expression, .. } => {
            if let Some(text) = expression.plain_text() {
                short(text)?;
            }
        }
        E::Link {
            name,
            sources,
            target,
            ..
        } => {
            short(name)?;
            for reference in sources.iter().chain(target.iter()) {
                valid_qname(reference)?;
            }
        }
        E::Compensate {
            activity_ref: Some(reference),
            ..
        } => valid_qname(reference)?,
        _ => {}
    }
    Ok(())
}
fn validate_io<'a>(
    io: &'a crate::bpmn_data::IoSpecification,
    ids: &mut HashSet<&'a crate::ids::ElementIdentity>,
    shape_ids: &mut HashSet<crate::ids::ElementSelector>,
    element_scope: &mut HashMap<crate::ids::ElementSelector, crate::ids::ElementSelector>,
    scope_id: &crate::ids::ElementSelector,
) -> Result<(), String> {
    if let Some(id) = &io.id {
        add(ids, id)?;
    }
    for port in io.inputs.iter().chain(&io.outputs) {
        if let Some(id) = &port.id {
            add(ids, id)?;
            shape_ids.insert(id.selector());
            element_scope.insert(id.selector(), scope_id.clone());
        }
        if let Some(name) = &port.name {
            short(name)?;
        }
    }
    for (sets, ports, other) in [
        (&io.input_sets, &io.inputs, &io.output_sets),
        (&io.output_sets, &io.outputs, &io.input_sets),
    ] {
        for set in sets {
            if let Some(id) = &set.id {
                add(ids, id)?;
            }
            if let Some(name) = &set.name {
                short(name)?;
            }
            for refs in [
                &set.data_refs,
                &set.optional_refs,
                &set.while_executing_refs,
            ] {
                let mut unique = HashSet::new();
                if refs.iter().any(|id| {
                    !unique.insert(id)
                        || !ports
                            .iter()
                            .any(|p| p.id.as_ref().is_some_and(|identity| identity == id))
                }) {
                    return fail("invalid I/O set port reference");
                }
            }
            let mut unique = HashSet::new();
            if set.counterpart_refs.iter().any(|id| {
                !unique.insert(id)
                    || !other
                        .iter()
                        .any(|s| s.id.as_ref().is_some_and(|identity| identity == id))
            }) {
                return fail("invalid I/O set reference");
            }
        }
    }
    Ok(())
}
/// Facts proven about one immutable checkpoint. They stay valid while a later
/// checkpoint is pointer-identical: the cached clone keeps the shared `Arc`s
/// alive, and `Arc::make_mut` never mutates content another holder shares.
#[derive(Clone)]
struct CheckpointFacts {
    checkpoint: Checkpoint,
    content_len: usize,
    entry_len: usize,
    digest: Option<[u8; 32]>,
}
/// Per-session memo so commits, undo and redo do not re-validate and re-hash
/// unchanged checkpoints and resource bytes. Content identity is unchanged.
#[derive(Clone, Default)]
pub(crate) struct CheckpointCache {
    checkpoints: Vec<CheckpointFacts>,
    resources: Vec<Resource>,
    documents: Vec<DocumentFacts>,
}
/// Facts proven about one immutable document `Arc` (validated against the listed
/// resource IDs): its element count and canonical JSON (sorted keys, normalized
/// numbers). An unchanged document is neither re-validated nor re-serialized.
#[derive(Clone)]
struct DocumentFacts {
    document: std::sync::Arc<BpmnDocument>,
    resource_ids: Vec<XmlId>,
    elements: usize,
    canonical: std::sync::Arc<[u8]>,
}
struct Validated {
    metadata: Value,
    documents: Vec<std::sync::Arc<[u8]>>,
}
fn same_checkpoint(a: &Checkpoint, b: &Checkpoint) -> bool {
    let (x, y) = (&a.content, &b.content);
    let small = |p: &FrozenProject| {
        serde_json::to_vec(&(
            &p.folders,
            &p.links,
            &p.dictionary_entries,
            &p.dictionary_links,
            &p.comments,
            &p.attribute_definitions,
            &p.attribute_values,
            &p.conventions,
        ))
        .ok()
    };
    a.id == b.id
        && a.name == b.name
        && a.created_unix_ms == b.created_unix_ms
        && x.id == y.id
        && x.name == y.name
        && x.description == y.description
        && x.documents.len() == y.documents.len()
        && x.documents
            .iter()
            .zip(&y.documents)
            .all(|(d, e)| std::sync::Arc::ptr_eq(d, e))
        && x.resources.len() == y.resources.len()
        && x.resources
            .iter()
            .zip(&y.resources)
            .all(|(r, s)| same_resource(r, s))
        && small(x) == small(y)
}
fn same_resource(r: &Resource, s: &Resource) -> bool {
    r.id == s.id
        && r.media_type == s.media_type
        && r.content_hash == s.content_hash
        && std::sync::Arc::ptr_eq(&r.bytes, &s.bytes)
}
/// Serialized project length once the checkpoint catalog is inserted into a
/// project serialized without it: `,"checkpoints":[e1,...,en]`.
fn json_len_with_checkpoints(without: usize, checkpoints: &[CheckpointFacts]) -> usize {
    if checkpoints.is_empty() {
        return without;
    }
    without
        + ",\"checkpoints\":[]".len()
        + checkpoints.iter().map(|c| c.entry_len).sum::<usize>()
        + checkpoints.len()
        - 1
}
impl ProjectData {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_cached(&mut CheckpointCache::default())
            .map(drop)
    }
    /// Validates and returns the project JSON value without documents, checkpoints
    /// and resource bytes, plus each document's canonical JSON, so identity reuses
    /// them instead of serializing again.
    fn validate_cached(&self, cache: &mut CheckpointCache) -> Result<Validated, String> {
        short(&self.name)?;
        if self.checkpoints.len() > crate::MAX_CHECKPOINTS {
            return fail("checkpoint limit exceeded");
        }
        let mut checkpoint_ids = HashSet::new();
        let mut current_metadata = self.clone();
        current_metadata.checkpoints.clear();
        current_metadata.documents.clear();
        for resource in &mut current_metadata.resources {
            resource.bytes = Default::default();
        }
        // Apply serde_json's reader recursion budget to the combined document
        // (see `inspect`), rather than separately bounding graph and mixed-XML
        // depths. Native archives deserialize the same nesting through serde_json.
        let current_value = serde_json::to_value(&current_metadata).map_err(|e| e.to_string())?;
        let resource_ids: Vec<XmlId> = self.resources.iter().map(|r| r.id.clone()).collect();
        let mut documents = Vec::with_capacity(self.documents.len());
        for d in &self.documents {
            let cached = cache.documents.iter().find(|facts| {
                std::sync::Arc::ptr_eq(&facts.document, d) && facts.resource_ids == resource_ids
            });
            documents.push(match cached {
                Some(facts) => (facts.canonical.clone(), Some(facts.elements)),
                None => {
                    let mut value = serde_json::to_value(&**d).map_err(|e| e.to_string())?;
                    // Nested in the project object and its `documents` array.
                    inspect(&value, 2)?;
                    normalize(&mut value);
                    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
                    (bytes.into(), None)
                }
            });
        }
        // Serialized length of the project without checkpoints: `"documents":[]`
        // filled with the comma-separated documents.
        let current_len = serde_json::to_vec(&current_value)
            .map_err(|e| e.to_string())?
            .len()
            + documents
                .iter()
                .map(|(bytes, _)| bytes.len())
                .sum::<usize>()
            + documents.len().saturating_sub(1);
        let mut checkpoint_bytes = current_len;
        checkpoint_bytes = checkpoint_bytes
            .checked_add(1024 + self.checkpoints.len() * 512)
            .ok_or("checkpoint size overflow")?;
        let mut unique_resource_hashes = HashSet::new();
        for resource in &self.resources {
            if unique_resource_hashes.insert(resource.content_hash) {
                checkpoint_bytes = checkpoint_bytes
                    .checked_add(resource.bytes.len())
                    .ok_or("checkpoint size overflow")?;
            }
        }
        let mut validated = Vec::with_capacity(self.checkpoints.len());
        for checkpoint in &self.checkpoints {
            if !checkpoint_ids.insert(checkpoint.id) {
                return fail("duplicate checkpoint ID");
            }
            short(&checkpoint.name)?;
            if checkpoint.content.id != self.id {
                return fail("checkpoint project identity mismatch");
            }
            let cached = cache
                .checkpoints
                .iter()
                .find(|facts| same_checkpoint(&facts.checkpoint, checkpoint));
            let facts = match cached {
                Some(facts) => facts.clone(),
                None => {
                    checkpoint.content.project().validate()?;
                    let mut stripped = checkpoint.clone();
                    for resource in &mut stripped.content.resources {
                        resource.bytes = Default::default();
                    }
                    let checkpoint_json =
                        serde_json::to_vec(&stripped.content).map_err(|e| e.to_string())?;
                    serde_json::from_slice::<serde_json::Value>(&checkpoint_json)
                        .map_err(|e| format!("checkpoint JSON is not readable: {e}"))?;
                    let entry = serde_json::to_value(&stripped).map_err(|e| e.to_string())?;
                    // Content nesting was bounded by reading `checkpoint_json` back.
                    for field in entry.as_object().into_iter().flat_map(|o| o.values()) {
                        inspect(field, 0)?;
                    }
                    CheckpointFacts {
                        checkpoint: checkpoint.clone(),
                        content_len: checkpoint_json.len(),
                        entry_len: serde_json::to_vec(&entry).map_err(|e| e.to_string())?.len(),
                        digest: None,
                    }
                }
            };
            for resource in &checkpoint.content.resources {
                if unique_resource_hashes.insert(resource.content_hash) {
                    checkpoint_bytes = checkpoint_bytes
                        .checked_add(resource.bytes.len())
                        .ok_or("checkpoint size overflow")?;
                }
            }
            checkpoint_bytes = checkpoint_bytes
                .checked_add(facts.content_len)
                .ok_or("checkpoint size overflow")?;
            validated.push(facts);
        }
        if checkpoint_bytes > 256 * 1024 * 1024 {
            return fail("checkpoint expanded size limit exceeded");
        }
        if self
            .reference_checkpoint
            .is_some_and(|id| !checkpoint_ids.contains(&id))
        {
            return fail("reference checkpoint not found");
        }
        let mut comment_ids = HashSet::new();
        for comment in &self.comments {
            if !comment_ids.insert(comment.id) {
                return fail("duplicate comment ID");
            }
            short(&comment.author)?;
            if comment.text.len() > MAX_LONG_TEXT_BYTES {
                return fail("comment exceeds 1 MiB");
            }
        }
        if self.documents.len() > MAX_DOCUMENTS {
            return fail("document limit exceeded");
        }

        let mut folders = HashMap::new();
        for f in &self.folders {
            short(&f.name)?;
            if folders.insert(&f.id, f.parent.as_ref()).is_some() {
                return fail("duplicate folder ID");
            }
        }
        acyclic(&folders, "folder")?;
        let mut resources = HashSet::new();
        let mut resource_bytes = 0usize;
        for r in &self.resources {
            add(&mut resources, &r.id)?;
            if !cache.resources.iter().any(|v| same_resource(v, r))
                && <[u8; 32]>::from(Sha256::digest(&r.bytes)) != r.content_hash
            {
                return fail("resource content hash mismatch");
            }
            if r.bytes.len() > 32 * 1024 * 1024 {
                return fail("resource exceeds 32 MiB");
            }
            resource_bytes = resource_bytes
                .checked_add(r.bytes.len())
                .ok_or("resource size overflow")?;
        }
        if resource_bytes > 256 * 1024 * 1024 {
            return fail("resources exceed package limit");
        }
        // `current_value` is this project without documents, checkpoints or
        // resource bytes (documents were inspected above); `current_len` is the
        // serialized length with documents. Checkpoint entries were inspected and
        // measured individually above.
        inspect(&current_value, 0)?;
        if json_len_with_checkpoints(current_len, &validated) > 64 * 1024 * 1024 {
            return fail("project JSON exceeds 64 MiB");
        }
        let mut docs = HashSet::new();
        let mut total = 0usize;
        let mut document_facts = Vec::with_capacity(self.documents.len());
        for (d, (canonical, elements)) in self.documents.iter().zip(&documents) {
            if !docs.insert(d.id) {
                return fail("duplicate document ID");
            }
            if d.folder.as_ref().is_some_and(|f| !folders.contains_key(f)) {
                return fail("missing document folder");
            }
            let elements = match elements {
                Some(elements) => *elements,
                None => validate_document(d, &resources)?,
            };
            total += elements;
            document_facts.push(DocumentFacts {
                document: d.clone(),
                resource_ids: resource_ids.clone(),
                elements,
                canonical: canonical.clone(),
            });
        }
        if total > MAX_PROJECT_ELEMENTS {
            return fail("project element limit exceeded");
        }
        for link in &self.links {
            short(&link.label)?;
            let d = self
                .documents
                .iter()
                .find(|d| d.id == link.source.document_id)
                .ok_or("missing link source document")?;
            if !docs.contains(&link.target) || !semantic_ids(d).contains(&link.source.xml_id) {
                return fail("invalid navigation link");
            }
        }
        let mut entries = HashMap::new();
        for entry in &self.dictionary_entries {
            short(&entry.name)?;
            if entries.insert(entry.id, entry.kind).is_some() {
                return fail("duplicate dictionary entry ID");
            }
        }
        // Build this index once per validation; it is never retained across edits.
        let mut role_sources_index = HashSet::new();
        let mut activity_sources_index = HashSet::new();
        for document in &self.documents {
            let mut pending: Vec<(&[Node], &[Lane])> = document
                .processes
                .iter()
                .map(|p| (p.nodes.as_slice(), p.lanes.as_slice()))
                .collect();
            while let Some((nodes, lanes)) = pending.pop() {
                for lane in lanes {
                    role_sources_index.insert((document.id, lane.id.selector()));
                }
                for node in nodes {
                    if node.kind.is_activity() {
                        activity_sources_index.insert((document.id, node.id.selector()));
                    }
                    if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) =
                        &node.details
                    {
                        pending.push((&content.nodes, &content.lanes));
                    }
                }
            }
        }
        let mut relations = HashSet::new();
        let mut role_sources = HashSet::new();
        for link in &self.dictionary_links {
            if !relations.insert((&link.source, link.kind, &link.target)) {
                return fail("duplicate dictionary link");
            }
            if link.kind == DictionaryKind::Role && !role_sources.insert(&link.source) {
                return fail("lane has multiple roles");
            }
            match &link.target {
                DictionaryTarget::Entry(id) if entries.get(id) != Some(&link.kind) => {
                    return fail("missing or wrong kind dictionary entry");
                }
                DictionaryTarget::Literal(label) => short(label)?,
                _ => {}
            }
            if !docs.contains(&link.source.document_id) {
                return fail("missing dictionary source document");
            }
            let source = (link.source.document_id, link.source.xml_id.clone());
            let valid = match link.kind {
                DictionaryKind::Role => role_sources_index.contains(&source),
                DictionaryKind::System | DictionaryKind::Document => {
                    activity_sources_index.contains(&source)
                }
            };
            if !valid {
                return fail("invalid dictionary source");
            }
        }
        crate::attributes::validate_project(self)?;
        crate::conventions::validate_project(self)?;
        cache.checkpoints = validated;
        cache.resources = self.resources.clone();
        // Keep the current documents plus a few earlier versions, so undo and redo
        // of recent edits find their documents again.
        let keep = document_facts.len() + 8;
        for facts in std::mem::take(&mut cache.documents) {
            if !document_facts
                .iter()
                .any(|kept| std::sync::Arc::ptr_eq(&kept.document, &facts.document))
            {
                document_facts.push(facts);
            }
        }
        document_facts.truncate(keep);
        cache.documents = document_facts;
        Ok(Validated {
            metadata: current_value,
            documents: documents.into_iter().map(|(bytes, _)| bytes).collect(),
        })
    }
    pub fn content_identity(&self) -> Result<[u8; 32], String> {
        self.identity_version(false, &mut CheckpointCache::default())
    }
    pub fn legacy_content_identity(&self) -> Result<[u8; 32], String> {
        self.identity_version(true, &mut CheckpointCache::default())
    }
    /// Same value as [Self::content_identity], reusing facts about unchanged
    /// checkpoints and resources proven by earlier calls with the same cache.
    pub(crate) fn content_identity_cached(
        &self,
        cache: &mut CheckpointCache,
    ) -> Result<[u8; 32], String> {
        self.identity_version(false, cache)
    }
    fn identity_version(
        &self,
        legacy: bool,
        cache: &mut CheckpointCache,
    ) -> Result<[u8; 32], String> {
        // Equal to serializing the project without checkpoints, resource bytes
        // stripped and resources stably sorted by ID.
        let Validated {
            metadata: mut value,
            documents,
        } = self.validate_cached(cache)?;
        if let Some(Value::Array(resources)) = value.get_mut("resources") {
            resources.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        }
        let checkpoints = self.checkpoints.clone();
        if !checkpoints.is_empty() {
            let mut catalog = Vec::with_capacity(checkpoints.len());
            for (checkpoint, facts) in checkpoints.into_iter().zip(&mut cache.checkpoints) {
                if let Some(digest) = facts.digest {
                    catalog.push(serde_json::json!({"id":checkpoint.id,"name":checkpoint.name,"created_unix_ms":checkpoint.created_unix_ms,"content_digest":digest}));
                    continue;
                }
                let mut content = checkpoint.content;
                content.resources.sort_by(|a, b| a.id.cmp(&b.id));
                for resource in &mut content.resources {
                    resource.bytes = Default::default();
                }
                let mut payload = serde_json::to_value(&content).map_err(|e| e.to_string())?;
                normalize(&mut payload);
                let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
                if bytes.len() > 64 * 1024 * 1024 {
                    return fail("checkpoint JSON exceeds 64 MiB");
                }
                let digest: [u8; 32] = Sha256::digest(bytes).into();
                facts.digest = Some(digest);
                catalog.push(serde_json::json!({"id":checkpoint.id,"name":checkpoint.name,"created_unix_ms":checkpoint.created_unix_ms,"content_digest":digest}));
            }
            value
                .as_object_mut()
                .unwrap()
                .insert("checkpoints".into(), Value::Array(catalog));
        }
        if legacy {
            let object = value.as_object_mut().unwrap();
            object.remove("dictionary_entries");
            object.remove("dictionary_links");
        }
        normalize(&mut value);
        let mut h = Sha256::new();
        h.update(b"ProcessCraft persistent content v1\0");
        // Same bytes as serializing the whole normalized project value: its keys
        // are sorted, and `documents` is filled with the cached canonical documents.
        let Value::Object(fields) = &value else {
            return fail("project JSON is not an object");
        };
        let mut len = 0usize;
        let mut put = |bytes: &[u8]| {
            h.update(bytes);
            len += bytes.len();
        };
        put(b"{");
        for (i, (key, field)) in fields.iter().enumerate() {
            if i > 0 {
                put(b",");
            }
            put(&serde_json::to_vec(key).map_err(|e| e.to_string())?);
            put(b":");
            if key == "documents" {
                put(b"[");
                for (j, document) in documents.iter().enumerate() {
                    if j > 0 {
                        put(b",");
                    }
                    put(document);
                }
                put(b"]");
            } else {
                put(&serde_json::to_vec(field).map_err(|e| e.to_string())?);
            }
        }
        put(b"}");
        if len > 64 * 1024 * 1024 {
            return fail("project JSON exceeds 64 MiB");
        }
        let mut resources: Vec<_> = self.resources.iter().collect();
        resources.sort_by(|a, b| a.id.cmp(&b.id));
        for resource in resources {
            h.update((resource.bytes.len() as u64).to_be_bytes());
            h.update(&*resource.bytes);
        }
        Ok(h.finalize().into())
    }
}
fn normalize(v: &mut Value) {
    match v {
        Value::Number(n) if n.as_f64() == Some(0.0) && n.is_f64() => *v = Value::from(0.0),
        Value::Array(a) => {
            for x in a {
                normalize(x)
            }
        }
        Value::Object(o) => {
            for x in o.values_mut() {
                normalize(x)
            }
        }
        _ => {}
    }
}
pub(crate) fn semantic_ids(d: &BpmnDocument) -> HashSet<crate::ids::ElementSelector> {
    let mut ids: HashSet<_> = d
        .element_identities()
        .into_iter()
        .map(|id| id.selector())
        .collect();
    for diagram in &d.diagrams {
        ids.remove(&diagram.id.selector());
        ids.remove(&diagram.plane_id.selector());
        for shape in &diagram.shapes {
            ids.remove(&shape.id.selector());
        }
        for edge in &diagram.edges {
            ids.remove(&edge.id.selector());
        }
    }
    ids
}

fn validate_document(d: &BpmnDocument, resources: &HashSet<&XmlId>) -> Result<usize, String> {
    use document_checks as c;
    short(&d.name)?;
    let collaboration = crate::collaboration_read::collaboration_read(d);
    // One read per document: each walks the whole model.
    let activity = crate::activity_read::activity_read(d);
    c::check_metadata(d, &collaboration, &activity)?;
    let identities = d.element_identities();
    c::check_element_limit(d, &identities, &collaboration, &activity)?;
    c::check_root_order(d)?;
    c::check_unique_identities(identities)?;
    c::check_definitions_metadata(d)?;
    let mut checks = c::DocumentChecks::new(d, resources)?;
    c::check_qnames(d, &activity.qnames)?;
    checks.check_definitions_declarations()?;
    c::check_flow_message_refs(d)?;
    checks.check_item_definitions()?;
    checks.check_interfaces()?;
    checks.check_resources()?;
    checks.collect_item_metadata()?;
    checks.check_flow_scopes()?;
    checks.check_choreography_scope_events()?;
    checks.check_item_aware_metadata()?;
    checks.check_expressions(activity.formal_expressions)?;
    checks.check_event_resources()?;
    checks.check_root_event_definitions()?;
    checks.check_provenance()?;
    checks.check_capability()?;
    checks.check_categories()?;
    checks.check_data_stores()?;
    let scopes = checks.collect_scopes()?;
    checks.register_data_store_shapes()?;
    checks.check_collaboration_extras()?;
    checks.check_processes()?;
    let scopes_by_id: HashMap<_, _> = scopes
        .iter()
        .map(|scope| (scope.id.clone(), scope))
        .collect();
    checks.check_scopes(&scopes)?;
    checks.check_choreographies()?;
    checks.check_choreography_graphs()?;
    checks.check_collaborations()?;
    checks.check_scope_endpoints(&scopes, &scopes_by_id)?;
    checks.check_collaboration_flows(&scopes_by_id)?;
    checks.check_diagrams(&scopes)?;
    checks.check_retained_extensions()?;
    if checks.id_count() > MAX_DOCUMENT_ELEMENTS {
        return fail("document element limit exceeded");
    }
    Ok(checks.id_count())
}
fn acyclic_selectors(
    parents: &HashMap<crate::ids::ElementSelector, Option<crate::ids::ElementSelector>>,
    kind: &str,
) -> Result<(), String> {
    let mut completed = HashSet::new();
    for start in parents.keys() {
        let mut visiting = HashSet::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if completed.contains(id) {
                break;
            }
            if !visiting.insert(id.clone()) {
                return Err(format!("{kind} cycle"));
            }
            current = parents
                .get(id)
                .ok_or_else(|| format!("missing parent {kind}"))?
                .as_ref();
        }
        completed.extend(visiting);
    }
    Ok(())
}
fn is_flow_node(k: NodeKind) -> bool {
    !matches!(
        k,
        NodeKind::TextAnnotation | NodeKind::DataObjectReference | NodeKind::DataStoreReference
    )
}

fn acyclic<'a>(parents: &HashMap<&'a XmlId, Option<&'a XmlId>>, kind: &str) -> Result<(), String> {
    let mut completed = HashSet::new();
    let mut visiting = HashSet::new();
    for &start in parents.keys() {
        if completed.contains(start) {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if completed.contains(id) {
                break;
            }
            if !visiting.insert(id) {
                return Err(format!("{kind} cycle"));
            }
            path.push(id);
            current = *parents
                .get(id)
                .ok_or_else(|| format!("missing parent {kind}"))?;
        }
        for id in path {
            visiting.remove(id);
            completed.insert(id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod checkpoint_cache_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn cached_identity_and_size_accounting_match_uncached() {
        let mut p = ProjectData::new("p".into());
        p.resources.push(Resource::new(
            "r".parse().unwrap(),
            "text/plain".into(),
            Arc::from(&b"abc"[..]),
        ));
        for i in 0..3 {
            p.checkpoints.push(Checkpoint {
                id: crate::ids::CheckpointId::new(),
                name: format!("c\"{i}"),
                created_unix_ms: i,
                content: FrozenProject::capture(&p),
            });
        }
        p.reference_checkpoint = Some(p.checkpoints[0].id);
        let mut cache = CheckpointCache::default();
        assert_eq!(
            p.content_identity_cached(&mut cache).unwrap(),
            p.content_identity().unwrap()
        );
        assert!(cache.checkpoints.iter().all(|c| c.digest.is_some()));
        p.name = "renamed".into();
        assert_eq!(
            p.content_identity_cached(&mut cache).unwrap(),
            p.content_identity().unwrap()
        );
        let mut stripped = p.clone();
        for resource in stripped.resources.iter_mut().chain(
            stripped
                .checkpoints
                .iter_mut()
                .flat_map(|c| &mut c.content.resources),
        ) {
            resource.bytes = Default::default();
        }
        let len = |p: &ProjectData| {
            serde_json::to_vec(&serde_json::to_value(p).unwrap())
                .unwrap()
                .len()
        };
        let full = len(&stripped);
        stripped.checkpoints.clear();
        assert_eq!(
            json_len_with_checkpoints(len(&stripped), &cache.checkpoints),
            full
        );
        // Reusing the validated JSON keeps the original formula: serialize the
        // project without checkpoints, bytes stripped and resources sorted.
        p.checkpoints.clear();
        p.reference_checkpoint = None;
        p.resources.push(Resource::new(
            "a".parse().unwrap(),
            "text/plain".into(),
            Arc::from(&b"z"[..]),
        ));
        let mut metadata = p.clone();
        metadata.resources.sort_by(|a, b| a.id.cmp(&b.id));
        for resource in &mut metadata.resources {
            resource.bytes = Default::default();
        }
        let mut value = serde_json::to_value(&metadata).unwrap();
        normalize(&mut value);
        let mut h = Sha256::new();
        h.update(b"ProcessCraft persistent content v1\0");
        h.update(serde_json::to_vec(&value).unwrap());
        let mut sorted: Vec<_> = p.resources.iter().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        for resource in sorted {
            h.update((resource.bytes.len() as u64).to_be_bytes());
            h.update(&*resource.bytes);
        }
        assert_eq!(
            <[u8; 32]>::from(h.finalize()),
            p.content_identity().unwrap()
        );
        // Bytes not proven by the cache are still hashed.
        p.resources[0].bytes = Arc::from(&b"abd"[..]);
        assert!(p.content_identity_cached(&mut cache).is_err());
    }
}
