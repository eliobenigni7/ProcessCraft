use crate::ids::XmlId;
use crate::model::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

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
        .all(|n| n.is_finite() && n.abs() <= 1_000_000_000.0)
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
fn inspect(v: &Value) -> Result<(), String> {
    match v {
        Value::String(s) if s.len() > 1_048_576 => fail("string exceeds 1 MiB"),
        Value::Array(a) => {
            if a.len() > 100_000 {
                return fail("collection exceeds limit");
            }
            for x in a {
                inspect(x)?;
            }
            Ok(())
        }
        Value::Object(o) => {
            for x in o.values() {
                inspect(x)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
fn short(s: &str) -> Result<(), String> {
    if s.len() > 16_384 {
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
                .is_some_and(|e| e.plain_text().is_some_and(|s| s.len() > 16_384))
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
impl ProjectData {
    pub fn validate(&self) -> Result<(), String> {
        short(&self.name)?;
        if self.checkpoints.len() > 20 {
            return fail("checkpoint limit exceeded");
        }
        let mut checkpoint_ids = HashSet::new();
        let mut current_metadata = self.clone();
        current_metadata.checkpoints.clear();
        for resource in &mut current_metadata.resources {
            resource.bytes = Default::default();
        }
        // Apply serde_json's reader recursion budget to the combined document,
        // rather than separately bounding graph and mixed-XML depths. Native
        // archives deserialize the same document nesting through serde_json.
        let current_json = serde_json::to_vec(&current_metadata).map_err(|e| e.to_string())?;
        serde_json::from_slice::<serde_json::Value>(&current_json)
            .map_err(|e| format!("project JSON is not readable: {e}"))?;
        let mut checkpoint_bytes = current_json.len();
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
        for checkpoint in &self.checkpoints {
            if !checkpoint_ids.insert(checkpoint.id) {
                return fail("duplicate checkpoint ID");
            }
            short(&checkpoint.name)?;
            if checkpoint.content.id != self.id {
                return fail("checkpoint project identity mismatch");
            }
            checkpoint.content.project().validate()?;
            let mut metadata = checkpoint.content.clone();
            for resource in &mut metadata.resources {
                if unique_resource_hashes.insert(resource.content_hash) {
                    checkpoint_bytes = checkpoint_bytes
                        .checked_add(resource.bytes.len())
                        .ok_or("checkpoint size overflow")?;
                }
                resource.bytes = Default::default();
            }
            let checkpoint_json = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
            serde_json::from_slice::<serde_json::Value>(&checkpoint_json)
                .map_err(|e| format!("checkpoint JSON is not readable: {e}"))?;
            checkpoint_bytes = checkpoint_bytes
                .checked_add(checkpoint_json.len())
                .ok_or("checkpoint size overflow")?;
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
            if comment.text.len() > 1_048_576 {
                return fail("comment exceeds 1 MiB");
            }
        }
        if self.documents.len() > 500 {
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
            if <[u8; 32]>::from(Sha256::digest(&r.bytes)) != r.content_hash {
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
        let mut metadata = self.clone();
        for resource in &mut metadata.resources {
            resource.bytes = Default::default();
        }
        for checkpoint in &mut metadata.checkpoints {
            for resource in &mut checkpoint.content.resources {
                resource.bytes = Default::default();
            }
        }
        let value = serde_json::to_value(&metadata).map_err(|e| e.to_string())?;
        inspect(&value)?;
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
            return fail("project JSON exceeds 64 MiB");
        }
        let mut docs = HashSet::new();
        let mut total = 0usize;
        for d in &self.documents {
            if !docs.insert(d.id) {
                return fail("duplicate document ID");
            }
            if d.folder.as_ref().is_some_and(|f| !folders.contains_key(f)) {
                return fail("missing document folder");
            }
            total += validate_document(d, &resources)?;
        }
        if total > 100_000 {
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
        Ok(())
    }
    pub fn content_identity(&self) -> Result<[u8; 32], String> {
        self.identity_version(false)
    }
    pub fn legacy_content_identity(&self) -> Result<[u8; 32], String> {
        self.identity_version(true)
    }
    fn identity_version(&self, legacy: bool) -> Result<[u8; 32], String> {
        self.validate()?;
        let mut metadata = self.clone();
        let checkpoints = std::mem::take(&mut metadata.checkpoints);
        metadata.resources.sort_by(|a, b| a.id.cmp(&b.id));
        for resource in &mut metadata.resources {
            resource.bytes = Default::default();
        }
        let mut value = serde_json::to_value(&metadata).map_err(|e| e.to_string())?;
        if !checkpoints.is_empty() {
            let mut catalog = Vec::with_capacity(checkpoints.len());
            for checkpoint in checkpoints {
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
        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        if bytes.len() > 64 * 1024 * 1024 {
            return fail("project JSON exceeds 64 MiB");
        }
        let mut h = Sha256::new();
        h.update(b"ProcessCraft persistent content v1\0");
        h.update(bytes);
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
    short(&d.name)?;
    let mut metadata_error = Ok(());
    let collaboration = crate::collaboration_read::collaboration_read(d);
    for docs in collaboration.docs {
        crate::bpmn_metadata::validate_set(docs)?;
    }
    for extras in collaboration.extras {
        crate::bpmn_metadata::validate_extras(extras)?;
    }
    for base in collaboration.bases {
        crate::bpmn_metadata::validate_base(base)?;
    }
    for base in crate::activity_read::activity_read(d).bases {
        crate::bpmn_metadata::validate_base(base)?;
    }
    crate::metadata_read::visit_base_metadata(d, |_, value| {
        if let Some(value) = value {
            metadata_error = metadata_error
                .clone()
                .and_then(|_| crate::bpmn_metadata::validate_base(value));
        }
    });
    crate::metadata_read::visit_legacy_metadata(d, |_, docs, extras| {
        metadata_error = metadata_error
            .clone()
            .and_then(|_| crate::bpmn_metadata::validate_set(docs));
        if let Some(extras) = extras {
            metadata_error = metadata_error
                .clone()
                .and_then(|_| crate::bpmn_metadata::validate_extras(extras));
        }
    });
    metadata_error?;
    let identities = d.element_identities();
    let anonymous_multiplicities = d
        .collaboration
        .iter()
        .flat_map(|c| &c.participants)
        .chain(d.choreographies.iter().flat_map(|c| &c.participants))
        .filter(|p| p.multiplicity.as_ref().is_some_and(|m| m.id.is_none()))
        .count();
    if identities
        .len()
        .saturating_add(anonymous_multiplicities)
        .saturating_add(crate::collaboration_read::collaboration_read(d).anonymous)
        .saturating_add(crate::activity_read::activity_read(d).anonymous_owners)
        > 10_000
    {
        return fail("document element limit exceeded");
    }
    if let Some(order) = &d.root_order {
        let roots = d.root_element_identities();
        if roots.iter().any(|id| id.is_none()) {
            return fail(
                "Root order requires explicit native identity promotion for anonymous roots",
            );
        }
        let expected: HashSet<_> = roots
            .into_iter()
            .flatten()
            .map(|id| id.selector())
            .collect();
        let actual: HashSet<_> = order.iter().cloned().collect();
        if actual.len() != order.len() || actual != expected {
            return fail("Root order must contain each modeled root exactly once");
        }
    }
    let mut native_ids = HashSet::new();
    let mut xml_ids = HashSet::new();
    for identity in identities {
        if let crate::ids::ElementIdentity::Native { native_id, .. } = identity
            && !native_ids.insert(native_id)
        {
            return fail("duplicate native element ID");
        }
        if let Some(xml_id) = identity.xml_id()
            && !xml_ids.insert(xml_id)
        {
            return fail("duplicate XML ID");
        }
    }
    if let Some(metadata) = &d.definitions {
        definitions_text(&metadata.target_namespace)?;
        if !metadata.namespaces.is_empty() || !metadata.foreign_attributes.is_empty() {
            crate::bpmn_xml::validate_foreign_attributes(
                &metadata.foreign_attributes,
                &metadata.namespaces,
            )?;
        }
        if metadata.imports.len() > 256 {
            return fail("Definitions import limit exceeded");
        }
        for import in &metadata.imports {
            definitions_text(&import.namespace)?;
            definitions_text(&import.location)?;
            definitions_text(&import.import_type)?;
        }
        for value in [
            &metadata.name,
            &metadata.expression_language,
            &metadata.type_language,
            &metadata.exporter,
            &metadata.exporter_version,
        ]
        .into_iter()
        .flatten()
        {
            definitions_text(value)?;
        }
    }
    let mut ids = HashSet::new();
    add(&mut ids, &d.definitions_id)?;
    let local_namespace = d
        .definitions
        .as_ref()
        .map(|v| v.target_namespace.as_str())
        .or_else(|| {
            d.provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let check_qname = |name: &crate::bpmn_model::QNameRef| -> Result<(), String> {
        definitions_text(&name.namespace)?;
        if name.namespace.chars().any(char::is_whitespace) {
            return fail("Invalid QName namespace");
        }
        Ok(())
    };
    for name in crate::activity_read::activity_read(d).qnames {
        check_qname(name)?;
    }
    for name in crate::collaboration_read::collaboration_qnames(d) {
        check_qname(name)?;
    }
    let local =
        |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
    let reference_index = crate::reference_resolution::ReferenceIndex::new(d);
    let endpoint = |reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
        reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
    };

    if d.extensions.len() > 256 || d.relationships.len() > 256 {
        return fail("Definitions declaration limit exceeded");
    }
    for extension in &d.extensions {
        if extension.documentation.len() > 256 {
            return fail("Extension documentation limit exceeded");
        }
        for doc in &extension.documentation {
            crate::bpmn_metadata::validate_documentation(doc)?;
        }
        if let Some(reference) = &extension.definition {
            check_qname(reference)?;
        }
    }
    for relationship in &d.relationships {
        if let Some(id) = &relationship.id {
            add(&mut ids, id)?;
        }
        definitions_text(&relationship.relationship_type)?;
        if relationship.sources.len() > 256 || relationship.targets.len() > 256 {
            return fail("Relationship reference limit exceeded");
        }
        for reference in relationship.sources.iter().chain(&relationship.targets) {
            check_qname(reference)?;
        }
    }
    let check_flow_message = |flow: &Flow| -> Result<(), String> {
        if let Some(reference) = &flow.message_ref {
            if flow.kind != FlowKind::Message {
                return fail("messageRef requires a message flow");
            }
            if let crate::bpmn_activity::ModelReference::Qualified(qname) = reference {
                check_qname(qname)?;
            }
        }
        Ok(())
    };
    for scope in crate::queries::scopes(d) {
        for flow in scope.flows {
            check_flow_message(flow)?;
        }
    }
    for collaboration in d.collaborations() {
        for flow in &collaboration.flows {
            check_flow_message(flow)?;
        }
    }
    for choreography in &d.choreographies {
        for flow in choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
        {
            check_flow_message(flow)?;
        }
    }
    let catalog = &d.catalog;
    if catalog.item_definitions.len() > 256
        || catalog.interfaces.len() > 256
        || catalog.resources.len() > 256
    {
        return fail("Supporting catalog limit exceeded");
    }
    for item in &catalog.item_definitions {
        if let Some(id) = &item.id {
            add(&mut ids, id)?;
        }
        if let Some(name) = &item.structure_ref {
            check_qname(name)?;
        }
    }
    for interface in &catalog.interfaces {
        if let Some(id) = &interface.id {
            add(&mut ids, id)?;
        }
        definitions_text(&interface.name)?;
        if let Some(name) = &interface.implementation_ref {
            check_qname(name)?;
        }
        if interface.operations.is_empty() || interface.operations.len() > 256 {
            return fail("Interface requires one to 256 operations");
        }
        for operation in &interface.operations {
            if let Some(id) = &operation.id {
                add(&mut ids, id)?;
            }
            definitions_text(&operation.name)?;
            if let Some(name) = &operation.implementation_ref {
                check_qname(name)?;
            }
            if operation.error_refs.len() > 256 {
                return fail("Operation error reference limit exceeded");
            }
            for reference in
                std::iter::once(&operation.in_message_ref).chain(operation.out_message_ref.iter())
            {
                check_qname(reference)?;
                if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Message { id, .. } if id.xml_id() == Some(&reference.local))) {
                    return fail("Operation references a missing local message");
                }
            }
            for reference in &operation.error_refs {
                check_qname(reference)?;
                if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Error { id, .. } if id.xml_id() == Some(&reference.local))) {
                    return fail("Operation references a missing local error");
                }
            }
        }
    }
    for resource in &catalog.resources {
        if let Some(id) = &resource.id {
            add(&mut ids, id)?;
        }
        definitions_text(&resource.name)?;
        if resource.parameters.len() > 256 {
            return fail("Resource parameter limit exceeded");
        }
        for parameter in &resource.parameters {
            if let Some(id) = &parameter.id {
                add(&mut ids, id)?;
            }
            if let Some(name) = &parameter.name {
                definitions_text(name)?;
            }
            if let Some(reference) = &parameter.type_ref {
                check_qname(reference)?;
                if local(reference)
                    && !catalog.item_definitions.iter().any(|item| {
                        item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                    })
                {
                    return fail("Resource parameter references a missing local item definition");
                }
            }
        }
    }
    let mut item_metadata: Vec<(&crate::bpmn_data::ItemAwareMetadata, Option<bool>)> = Vec::new();
    for store in &d.data_stores {
        if let Some(meta) = &store.item_aware {
            item_metadata.push((meta, None));
        }
    }
    for process in &d.processes {
        if let Some(io) = &process.io_specification {
            for port in io.inputs.iter().chain(&io.outputs) {
                if let Some(meta) = &port.item_aware {
                    item_metadata.push((meta, Some(port.is_collection.unwrap_or(false))));
                }
            }
        }
    }
    let has_flow_node_refs = |metadata: &Option<crate::bpmn_activity::FlowElementMetadata>| {
        metadata
            .as_ref()
            .is_some_and(|value| !value.incoming.is_empty() || !value.outgoing.is_empty())
    };
    for scope in crate::queries::scopes(d) {
        for object in scope.data_objects {
            if has_flow_node_refs(&object.flow_metadata) {
                return fail("incoming/outgoing references require a flow node");
            }
            if object
                .flow_metadata
                .as_ref()
                .is_some_and(|m| m.is_immediate.is_some())
            {
                return fail("isImmediate requires a sequence flow");
            }
            if let Some(meta) = &object.item_aware {
                item_metadata.push((meta, Some(object.is_collection.unwrap_or(false))));
            }
        }
        for association in scope.data_associations {
            if association.assignments.len() > 256 {
                return fail("Data assignment limit exceeded");
            }
            for assignment in &association.assignments {
                if let Some(id) = &assignment.id {
                    add(&mut ids, id)?;
                }
            }
        }
        for node in scope.nodes {
            if let Some(crate::bpmn_model::LoopCharacteristics::MultiInstance {
                complex_behavior_definitions,
                ..
            }) = &node.loop_characteristics
            {
                for behavior in complex_behavior_definitions {
                    if behavior
                        .event
                        .as_ref()
                        .is_some_and(|event| has_flow_node_refs(&event.flow))
                    {
                        return fail("Implicit event flow references must use the event fields");
                    }
                    if behavior
                        .event
                        .as_ref()
                        .and_then(|event| event.flow.as_ref())
                        .is_some_and(|flow| flow.is_immediate.is_some())
                    {
                        return fail("isImmediate requires a sequence flow");
                    }
                }
            }
            if let Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) =
                &node.details
            {
                for property in properties {
                    if let Some(id) = &property.id {
                        add(&mut ids, id)?;
                    }
                    if let Some(name) = &property.name {
                        definitions_text(name)?;
                    }
                    if let Some(metadata) = &property.item_aware {
                        item_metadata.push((metadata, None));
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
                    if ports.len() > 256 {
                        return fail("Event port limit exceeded");
                    }
                    for port in ports {
                        if let Some(id) = &port.id {
                            add(&mut ids, id)?;
                        }
                        if let Some(name) = &port.name {
                            definitions_text(name)?;
                        }
                        if let Some(metadata) = &port.item_aware {
                            item_metadata
                                .push((metadata, Some(port.is_collection.unwrap_or(false))));
                        }
                    }
                    if let Some(set) = set {
                        if let Some(id) = &set.id {
                            add(&mut ids, id)?;
                        }
                        if let Some(name) = &set.name {
                            definitions_text(name)?;
                        }
                        let mut seen = HashSet::new();
                        for reference in &set.data_refs {
                            if !seen.insert(reference)
                                || !ports
                                    .iter()
                                    .any(|port| port.id.as_ref().is_some_and(|id| id == reference))
                            {
                                return fail("Invalid event set port reference");
                            }
                        }
                        if set
                            .optional_refs
                            .iter()
                            .chain(&set.while_executing_refs)
                            .any(|reference| !set.data_refs.contains(reference))
                            || !set.counterpart_refs.is_empty()
                        {
                            return fail("Invalid event set metadata");
                        }
                    }
                }
            }
            if let Some(meta) = &node.item_aware {
                if !matches!(
                    node.kind,
                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                ) {
                    return fail("Item-aware metadata requires a data reference");
                }
                item_metadata.push((meta, None));
            }
            if let Some(io) = &node.io_specification {
                for port in io.inputs.iter().chain(&io.outputs) {
                    if let Some(meta) = &port.item_aware {
                        item_metadata.push((meta, Some(port.is_collection.unwrap_or(false))));
                    }
                }
            }
        }
    }
    // Event-owned properties, ports, sets, and data states in choreography
    // graphs participate in the same global identity and QName checks as process events.
    for scope in crate::queries::choreography_scopes(d) {
        if scope.root.graph.is_none() {
            continue;
        }
        for node in scope.events {
            if let Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) =
                &node.details
            {
                if properties.len() > 256 {
                    return fail("Event property limit exceeded");
                }
                for property in properties {
                    if let Some(id) = &property.id {
                        add(&mut ids, id)?;
                    }
                    if let Some(name) = &property.name {
                        definitions_text(name)?;
                    }
                    if let Some(metadata) = &property.item_aware {
                        item_metadata.push((metadata, None));
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
                    if ports.len() > 256 {
                        return fail("Event port limit exceeded");
                    }
                    for port in ports {
                        if let Some(id) = &port.id {
                            add(&mut ids, id)?;
                        }
                        if let Some(name) = &port.name {
                            definitions_text(name)?;
                        }
                        if let Some(metadata) = &port.item_aware {
                            item_metadata
                                .push((metadata, Some(port.is_collection.unwrap_or(false))));
                        }
                    }
                    if let Some(set) = set {
                        if let Some(id) = &set.id {
                            add(&mut ids, id)?;
                        }
                        if let Some(name) = &set.name {
                            definitions_text(name)?;
                        }
                        let mut seen = HashSet::new();
                        for reference in &set.data_refs {
                            if !seen.insert(reference)
                                || !ports
                                    .iter()
                                    .any(|port| port.id.as_ref().is_some_and(|id| id == reference))
                            {
                                return fail("Invalid event set port reference");
                            }
                        }
                        if set
                            .optional_refs
                            .iter()
                            .chain(&set.while_executing_refs)
                            .any(|r| !set.data_refs.contains(r))
                            || !set.counterpart_refs.is_empty()
                        {
                            return fail("Invalid event set metadata");
                        }
                    }
                }
            }
        }
    }
    for (metadata, collection) in item_metadata {
        if let Some(state) = &metadata.data_state {
            if let Some(id) = &state.id {
                add(&mut ids, id)?;
            }
            if let Some(name) = &state.name {
                definitions_text(name)?;
            }
        }
        if let Some(reference) = &metadata.item_subject_ref {
            check_qname(reference)?;
            if local(reference) {
                let Some(item) = catalog.item_definitions.iter().find(|item| {
                    item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                }) else {
                    return fail("Missing local item definition");
                };
                if let Some(value) = collection
                    && value != item.is_collection.unwrap_or(false)
                {
                    return fail("Collection status differs from item definition");
                }
            }
        }
    }
    let transformations: Vec<_> = crate::queries::scopes(d)
        .into_iter()
        .flat_map(|scope| {
            scope
                .data_associations
                .iter()
                .filter_map(|association| association.transformation.as_ref())
        })
        .chain(
            crate::queries::choreography_scopes(d)
                .into_iter()
                .flat_map(|scope| {
                    scope
                        .data_associations
                        .into_iter()
                        .filter_map(|association| association.transformation.as_ref())
                }),
        )
        .collect();
    let formal_expressions = crate::activity_read::activity_read(d).formal_expressions;
    for expression in d.expressions() {
        use crate::bpmn_model::{Expression, ExpressionKind};
        let transformation = transformations
            .iter()
            .chain(&formal_expressions)
            .any(|candidate| std::ptr::eq(*candidate, expression));
        if transformation
            && !matches!(
                expression,
                Expression::Structured(crate::bpmn_model::ExpressionData {
                    kind: ExpressionKind::Formal { .. },
                    ..
                })
            )
        {
            return fail("Data transformation requires a formal expression");
        }
        if let Expression::Structured(data) = expression {
            if let Some(id) = &data.id {
                add(&mut ids, id)?;
            }
            if let Some(text) = match &data.text {
                crate::bpmn_model::ExpressionBody::Text(s) => Some(s.as_str()),
                _ => None,
            } {
                definitions_text(text)?;
            }
            if data.namespaces.len() > 256 {
                return fail("Expression namespace limit exceeded");
            }
            let mut prefixes = HashSet::new();
            for (prefix, uri) in &data.namespaces {
                definitions_text(prefix)?;
                definitions_text(uri)?;
                if !prefixes.insert(prefix)
                    || uri.chars().any(char::is_whitespace)
                    || prefix == "xmlns"
                    || (!prefix.is_empty() && (prefix.parse::<XmlId>().is_err() || uri.is_empty()))
                    || (prefix == "xml") != (uri == "http://www.w3.org/XML/1998/namespace")
                    || uri == "http://www.w3.org/2000/xmlns/"
                {
                    return fail("Invalid expression namespace binding");
                }
            }
            if !data
                .namespaces
                .iter()
                .any(|(_, uri)| uri == "http://www.omg.org/spec/BPMN/20100524/MODEL")
            {
                return fail("Expression namespace context cannot represent a BPMN element");
            }
            let explicit_type = match &data.kind {
                ExpressionKind::Base { explicit_type }
                | ExpressionKind::Formal { explicit_type, .. } => *explicit_type,
            };
            if explicit_type
                && !data.namespaces.iter().any(|(prefix, uri)| {
                    !prefix.is_empty() && uri == "http://www.w3.org/2001/XMLSchema-instance"
                })
            {
                return fail("Expression namespace context cannot represent xsi:type");
            }
            if let ExpressionKind::Formal {
                explicit_type,
                language,
                evaluates_to_type_ref,
                ..
            } = &data.kind
            {
                if !explicit_type && !transformation {
                    return fail(
                        "Formal expression requires an explicit xsi:type in this editable profile",
                    );
                }
                if let Some(language) = language {
                    definitions_text(language)?;
                }
                if let Some(name) = evaluates_to_type_ref {
                    definitions_text(&name.namespace)?;
                    if local(name)
                        && !catalog.item_definitions.iter().any(|item| {
                            item.id.as_ref().and_then(|id| id.xml_id()) == Some(&name.local)
                        })
                    {
                        return fail("Expression references a missing local item definition");
                    }
                    let default = data
                        .namespaces
                        .iter()
                        .find(|(prefix, _)| prefix.is_empty())
                        .map(|(_, uri)| uri.as_str())
                        .unwrap_or("");
                    if (name.namespace.is_empty() && !default.is_empty())
                        || (!name.namespace.is_empty()
                            && !data
                                .namespaces
                                .iter()
                                .any(|(_, uri)| uri == &name.namespace))
                    {
                        return fail(
                            "Expression namespace context cannot represent evaluatesToTypeRef",
                        );
                    }
                }
            }
        }
    }
    use crate::bpmn_model::EventResource;
    for resource in &d.event_resources {
        let (id, name) = match resource {
            EventResource::Signal { id, name, .. }
            | EventResource::Message { id, name, .. }
            | EventResource::Error { id, name, .. }
            | EventResource::Escalation { id, name, .. } => (id, name),
        };
        add(&mut ids, id)?;
        if let Some(name) = name {
            short(name)?;
        }
        let item = match resource {
            EventResource::Message { item_ref, .. } => item_ref,
            EventResource::Signal { structure_ref, .. }
            | EventResource::Error { structure_ref, .. }
            | EventResource::Escalation { structure_ref, .. } => structure_ref,
        };
        if let Some(reference) = item {
            check_qname(reference)?;
            if local(reference)
                && !catalog.item_definitions.iter().any(|item| {
                    item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                })
            {
                return fail("Event resource references a missing local item definition");
            }
        }
        match resource {
            EventResource::Error {
                code: Some(code), ..
            }
            | EventResource::Escalation {
                code: Some(code), ..
            } => short(code)?,
            _ => {}
        }
    }
    if d.root_event_definitions.len() > 256 {
        return fail("Root event definition limit exceeded");
    }
    for definition in &d.root_event_definitions {
        if let Some(id) = definition.id() {
            add(&mut ids, id)?;
        }
        validate_event_payload(d, definition, local_namespace)?;
    }
    let root_by_selector: HashMap<_, _> = d
        .root_event_definitions
        .iter()
        .filter_map(|definition| definition.id().map(|id| (id.selector(), definition)))
        .collect();
    let root_by_xml: HashMap<_, _> = d
        .root_event_definitions
        .iter()
        .filter_map(|definition| {
            definition
                .id()
                .and_then(|id| id.xml_id())
                .map(|id| (id, definition))
        })
        .collect();
    if let Some(p) = &d.provenance
        && !resources.contains(&p.source_resource_id)
    {
        return fail("missing provenance resource");
    }
    match &d.capability {
        Capability::Editable => {}
        Capability::PreservedSource { source_resource_id }
        | Capability::RetainedExtensions {
            source_resource_id, ..
        } => {
            if !resources.contains(source_resource_id) {
                return fail("missing source resource");
            }
        }
    }
    for category in &d.categories {
        add(&mut ids, &category.id)?;
        if let Some(name) = &category.name {
            short(name)?;
        }
        for value in &category.values {
            add(&mut ids, &value.id)?;
            if let Some(text) = &value.value {
                short(text)?;
            }
        }
    }
    for store in &d.data_stores {
        if let Some(id) = &store.id {
            add(&mut ids, id)?;
        }
        if let Some(name) = &store.name {
            short(name)?;
        }
        if let Some(capacity) = &store.capacity {
            let digits = capacity.strip_prefix(['+', '-']).unwrap_or(capacity);
            if capacity.len() > 16_384
                || digits.is_empty()
                || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return fail("invalid data store capacity");
            }
        }
    }
    // Each subprocess node owns a distinct flow scope. Iterative traversal bounds depth.
    struct Scope<'a> {
        id: crate::ids::ElementSelector,
        parent: Option<crate::ids::ElementSelector>,
        nodes: &'a [Node],
        lanes: &'a [Lane],
        flows: &'a [Flow],
        objects: &'a [crate::bpmn_data::DataObject],
        associations: &'a [crate::bpmn_data::DataAssociation],
        lane_set_id: Option<&'a crate::ids::ElementIdentity>,
        groups: &'a [crate::bpmn_model::Group],
    }
    let mut scopes: Vec<Scope<'_>> = Vec::new();
    let mut pending: Vec<(Scope<'_>, usize)> = d
        .processes
        .iter()
        .map(|p| {
            (
                Scope {
                    id: p.id.selector(),
                    parent: None,
                    nodes: &p.nodes,
                    lanes: &p.lanes,
                    flows: &p.flows,
                    objects: &p.data_objects,
                    associations: &p.data_associations,
                    lane_set_id: p.lane_set_id.as_ref(),
                    groups: &p.groups,
                },
                0,
            )
        })
        .collect();
    let mut count = 0usize;
    while let Some((scope, depth)) = pending.pop() {
        if depth > 32 {
            return fail("subprocess depth limit exceeded");
        }
        count = count
            .saturating_add(scope.nodes.len())
            .saturating_add(scope.flows.len())
            .saturating_add(scope.lanes.len())
            .saturating_add(scope.objects.len())
            .saturating_add(scope.associations.len())
            .saturating_add(scope.groups.len());
        if count > 10_000 {
            return fail("document element limit exceeded");
        }
        for n in scope.nodes {
            if let Some(crate::bpmn_model::NodeDetails::DataStoreReference {
                data_store_ref: Some(crate::bpmn_data::DataStoreRef::Qualified(reference)),
            }) = &n.details
            {
                check_qname(reference)?;
            }
            match (&n.kind, &n.details) {
                (
                    NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction,
                    Some(crate::bpmn_model::NodeDetails::SubProcess { content }),
                ) => pending.push((
                    Scope {
                        id: n.id.selector(),
                        parent: Some(scope.id.clone()),
                        nodes: &content.nodes,
                        lanes: &content.lanes,
                        flows: &content.flows,
                        objects: &content.data_objects,
                        associations: &content.data_associations,
                        lane_set_id: content.lane_set_id.as_ref(),
                        groups: &content.groups,
                    },
                    depth + 1,
                )),
                (
                    NodeKind::CallActivity,
                    Some(crate::bpmn_model::NodeDetails::CallActivity { called_element }),
                ) => {
                    if called_element
                        .as_ref()
                        .is_some_and(|value| value.namespace.len() > 16_384)
                    {
                        return fail("invalid call namespace");
                    }
                }
                (
                    NodeKind::SubProcess
                    | NodeKind::AdHocSubProcess
                    | NodeKind::Transaction
                    | NodeKind::CallActivity,
                    _,
                ) => return fail("activity details do not match kind"),
                (
                    NodeKind::DataObjectReference,
                    Some(crate::bpmn_model::NodeDetails::DataObjectReference { data_object_ref }),
                ) if data_object_ref.as_ref().is_none_or(|id| {
                    if scope
                        .objects
                        .iter()
                        .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                    {
                        return true;
                    }
                    let mut parent = scope.parent.clone();
                    while let Some(scope_id) = parent {
                        let Some(ancestor) =
                            scopes.iter().find(|candidate| candidate.id == scope_id)
                        else {
                            break;
                        };
                        if ancestor
                            .objects
                            .iter()
                            .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                        {
                            return true;
                        }
                        parent = ancestor.parent.clone();
                    }
                    false
                }) => {}
                (
                    NodeKind::DataStoreReference,
                    Some(crate::bpmn_model::NodeDetails::DataStoreReference { data_store_ref }),
                ) if data_store_ref
                    .as_ref()
                    .is_none_or(|reference| match reference {
                        crate::bpmn_data::DataStoreRef::Legacy(id) => d
                            .data_stores
                            .iter()
                            .any(|o| o.id.as_ref().is_some_and(|identity| identity == id)),
                        crate::bpmn_data::DataStoreRef::Qualified(name) => {
                            !local(name)
                                || d.data_stores.iter().any(|o| {
                                    o.id.as_ref().and_then(|identity| identity.xml_id())
                                        == Some(&name.local)
                                })
                        }
                    }) => {}
                (
                    NodeKind::ExclusiveGateway
                    | NodeKind::InclusiveGateway
                    | NodeKind::ParallelGateway
                    | NodeKind::ComplexGateway
                    | NodeKind::EventBasedGateway,
                    Some(crate::bpmn_model::NodeDetails::Gateway {
                        default_flow,
                        instantiate,
                        event_gateway_type,
                        activation_condition,
                        ..
                    }),
                ) => {
                    if default_flow.is_some()
                        && !matches!(
                            n.kind,
                            NodeKind::ExclusiveGateway
                                | NodeKind::InclusiveGateway
                                | NodeKind::ComplexGateway
                        )
                    {
                        return fail("gateway kind cannot have default flow");
                    }
                    if default_flow.as_ref().is_some_and(|id| {
                        !scope.flows.iter().any(|f| {
                            &f.id == id
                                && endpoint(&f.source).is_some_and(|source| source == n.id)
                                && f.kind == FlowKind::Sequence
                        })
                    }) {
                        return fail("gateway default must be an outgoing flow in the same scope");
                    }
                    if (instantiate.is_some() || event_gateway_type.is_some())
                        && !matches!(n.kind, NodeKind::EventBasedGateway)
                    {
                        return fail("event gateway attributes on wrong gateway kind");
                    }
                    if let Some(condition) = activation_condition
                        && (!matches!(n.kind, NodeKind::ComplexGateway)
                            || condition.plain_text().is_some_and(|s| s.len() > 16_384))
                    {
                        return fail("invalid gateway activation condition");
                    }
                }
                (_, Some(crate::bpmn_model::NodeDetails::Gateway { .. })) => {
                    return fail("gateway details require gateway");
                }
                (
                    NodeKind::TextAnnotation,
                    Some(crate::bpmn_model::NodeDetails::TextAnnotation { text_format, text }),
                ) => {
                    if let Some(format) = text_format {
                        short(format)?;
                    }
                    if let Some(text) = text {
                        crate::bpmn_xml::validate_mixed(text)?;
                    }
                }
                (_, Some(crate::bpmn_model::NodeDetails::TextAnnotation { .. })) => {
                    return fail("annotation details require text annotation");
                }
                (NodeKind::DataStoreReference, _) => {
                    return fail("data store reference requires matching details");
                }
                (
                    _,
                    Some(
                        crate::bpmn_model::NodeDetails::SubProcess { .. }
                        | crate::bpmn_model::NodeDetails::CallActivity { .. }
                        | crate::bpmn_model::NodeDetails::DataObjectReference { .. }
                        | crate::bpmn_model::NodeDetails::DataStoreReference { .. },
                    ),
                ) => return fail("activity details do not match kind"),
                _ => (),
            }
        }
        scopes.push(scope);
    }
    let mut node_process = HashMap::new();
    let mut kinds = HashMap::new();
    let mut flow_ids = HashSet::new();
    let mut shape_ids = HashSet::new();
    let mut group_ids = HashSet::new();
    let mut element_scope = HashMap::new();
    for store in &d.data_stores {
        if let Some(id) = &store.id {
            shape_ids.insert(id.selector());
        }
    }
    let collaboration_extras = d
        .collaborations()
        .map(|c| (&c.id, &c.extras))
        .chain(d.choreographies.iter().map(|c| (&c.id, &c.extras)));
    for (owner, extras) in collaboration_extras {
        if let Some(extras) = extras {
            for artifact in &extras.artifacts {
                use crate::bpmn_collaboration::CollaborationArtifact;
                match artifact {
                    CollaborationArtifact::Group(group) => {
                        add(&mut ids, &group.id)?;
                        group_ids.insert(group.id.selector());
                        shape_ids.insert(group.id.selector());
                        element_scope.insert(group.id.selector(), owner.selector());
                    }
                    CollaborationArtifact::TextAnnotation(node) => {
                        if node.kind != NodeKind::TextAnnotation
                            || node.activity.is_some()
                            || node.io_specification.is_some()
                            || node.loop_characteristics.is_some()
                            || node.item_aware.is_some()
                            || node.flow_metadata.is_some()
                            || node.details.as_ref().is_some_and(|details| {
                                !matches!(
                                    details,
                                    crate::bpmn_model::NodeDetails::TextAnnotation { .. }
                                )
                            })
                        {
                            return fail("Collaboration text annotation has incompatible metadata");
                        }
                        add(&mut ids, &node.id)?;
                        shape_ids.insert(node.id.selector());
                        element_scope.insert(node.id.selector(), owner.selector());
                    }
                    CollaborationArtifact::Association(flow) => {
                        if flow.kind != FlowKind::Association
                            || flow.condition.is_some()
                            || flow.message_ref.is_some()
                            || flow.flow_metadata.is_some()
                            || flow.name.is_some()
                        {
                            return fail("Collaboration association has incompatible metadata");
                        }
                        add(&mut ids, &flow.id)?;
                        flow_ids.insert(flow.id.selector());
                        element_scope.insert(flow.id.selector(), owner.selector());
                    }
                }
            }
        }
    }
    for process in &d.processes {
        add(&mut ids, &process.id)?;
        if let Some(name) = &process.name {
            short(name)?;
        }
        if let Some(io) = &process.io_specification {
            validate_io(
                io,
                &mut ids,
                &mut shape_ids,
                &mut element_scope,
                &process.id.selector(),
            )?;
        }
    }
    let scopes_by_id: HashMap<_, _> = scopes
        .iter()
        .map(|scope| (scope.id.clone(), scope))
        .collect();
    for p in &scopes {
        if let Some(id) = p.lane_set_id {
            add(&mut ids, id)?;
        }
        for group in p.groups {
            add(&mut ids, &group.id)?;
            shape_ids.insert(group.id.selector());
            group_ids.insert(group.id.selector());
            node_process.insert(group.id.selector(), p.id.clone());
            element_scope.insert(group.id.selector(), p.id.clone());
            if group.category_value_ref.as_ref().is_some_and(|id| {
                !id.is_external(local_namespace)
                    && !d.categories.iter().any(|c| {
                        c.values
                            .iter()
                            .any(|v| id.matches_identity(&v.id, local_namespace))
                    })
            }) {
                return fail("missing group category value");
            }
        }
        for object in p.objects {
            if let Some(id) = &object.id {
                add(&mut ids, id)?;
                shape_ids.insert(id.selector());
                element_scope.insert(id.selector(), p.id.clone());
            }
            if let Some(name) = &object.name {
                short(name)?;
            }
        }
        for association in p.associations {
            if let Some(id) = &association.id {
                add(&mut ids, id)?;
            }
            if let Some(id) = &association.id {
                flow_ids.insert(id.selector());
                element_scope.insert(id.selector(), p.id.clone());
            }
        }
        for n in p.nodes {
            if n.flow_metadata
                .as_ref()
                .is_some_and(|m| m.is_immediate.is_some())
            {
                return fail("isImmediate requires a sequence flow");
            }
            if matches!(
                n.kind,
                NodeKind::DataObjectReference | NodeKind::DataStoreReference
            ) && has_flow_node_refs(&n.flow_metadata)
            {
                return fail("incoming/outgoing references require a flow node");
            }
            if n.flow_metadata.is_some() && matches!(n.kind, NodeKind::TextAnnotation) {
                return fail("Flow-element metadata requires a flow element");
            }
            add(&mut ids, &n.id)?;
            if let Some(io) = &n.io_specification {
                if !n.kind.is_activity() {
                    return fail("I/O specification requires activity");
                }
                validate_io(io, &mut ids, &mut shape_ids, &mut element_scope, &p.id)?;
            }
            if let Some(name) = &n.name {
                short(name)?;
            }
            if let Some(activity) = &n.activity {
                if !n.kind.is_activity() {
                    return fail("activity properties require activity");
                }
                let check_text = |value: &Option<String>| -> Result<(), String> {
                    if value.as_ref().is_some_and(|v| v.len() > 16_384) {
                        return fail("activity property too long");
                    }
                    Ok(())
                };
                for value in [&activity.start_quantity, &activity.completion_quantity] {
                    check_text(value)?;
                    if let Some(value) = value {
                        let trimmed = value.trim();
                        let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
                        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                            return fail("invalid activity quantity");
                        }
                    }
                }
                if activity.default_flow.as_ref().is_some_and(|id| {
                    !p.flows.iter().any(|f| {
                        &f.id == id
                            && endpoint(&f.source).is_some_and(|source| source == n.id)
                            && f.kind == FlowKind::Sequence
                    })
                }) {
                    return fail("activity default must be outgoing flow in same scope");
                }
                if activity.triggered_by_event.is_some()
                    && !matches!(
                        n.kind,
                        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
                    )
                {
                    return fail("event trigger requires subprocess");
                }
                if (activity.ad_hoc_ordering.is_some()
                    || activity.cancel_remaining_instances.is_some()
                    || activity.completion_condition.is_some())
                    && !matches!(n.kind, NodeKind::AdHocSubProcess)
                {
                    return fail("ad hoc metadata requires ad hoc subprocess");
                }
                if activity.transaction_method.is_some() && !matches!(n.kind, NodeKind::Transaction)
                {
                    return fail("transaction method requires transaction");
                }
                if activity.implementation.is_some()
                    && !matches!(
                        n.kind,
                        NodeKind::ServiceTask
                            | NodeKind::SendTask
                            | NodeKind::ReceiveTask
                            | NodeKind::UserTask
                            | NodeKind::BusinessRuleTask
                    )
                {
                    return fail("implementation on unsupported activity");
                }
                if activity.message_ref.is_some()
                    && !matches!(n.kind, NodeKind::SendTask | NodeKind::ReceiveTask)
                {
                    return fail("message reference requires send or receive task");
                }
                if activity.operation_ref.is_some()
                    && !matches!(
                        n.kind,
                        NodeKind::ServiceTask | NodeKind::SendTask | NodeKind::ReceiveTask
                    )
                {
                    return fail("operation reference requires service, send or receive task");
                }
                if activity.instantiate.is_some() && !matches!(n.kind, NodeKind::ReceiveTask) {
                    return fail("instantiate requires receive task");
                }
                if (activity.script_format.is_some() || activity.script.is_some())
                    && !matches!(n.kind, NodeKind::ScriptTask)
                {
                    return fail("script metadata requires script task");
                }
                for value in [
                    &activity.transaction_method,
                    &activity.implementation,
                    &activity.script_format,
                ] {
                    check_text(value)?;
                }
                if let Some(script) = &activity.script {
                    match script {
                        crate::bpmn_xml::MixedXml::Text(text) => {
                            if text.len() > 16_384 {
                                return fail("activity property too long");
                            }
                        }
                        crate::bpmn_xml::MixedXml::Structured(_) => {
                            crate::bpmn_xml::validate_mixed(script)?
                        }
                    }
                }
                if let Some(value) = &activity.completion_condition
                    && let Some(text) = value.plain_text()
                {
                    short(text)?;
                }
                for name in [&activity.message_ref, &activity.operation_ref]
                    .into_iter()
                    .flatten()
                {
                    if name.namespace.len() > 16_384 {
                        return fail("invalid QName namespace");
                    }
                }
            }
            if let Some(characteristics) = &n.loop_characteristics {
                use crate::bpmn_model::LoopCharacteristics;
                if !n.kind.is_activity() {
                    return fail("loop characteristics require an activity");
                }
                let check = |value: Option<&str>| -> Result<(), String> {
                    if value.is_some_and(|s| s.len() > 16_384) {
                        return fail("invalid loop expression");
                    }
                    Ok(())
                };
                match characteristics {
                    LoopCharacteristics::Standard {
                        condition, maximum, ..
                    } => {
                        check(condition.as_ref().and_then(|e| e.plain_text()))?;
                        check(maximum.as_deref())?;
                        if let Some(value) = maximum {
                            let trimmed = value.trim();
                            let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
                            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                                return fail("invalid loop maximum");
                            }
                        }
                    }
                    LoopCharacteristics::MultiInstance {
                        cardinality,
                        completion_condition,
                        ..
                    } => {
                        check(cardinality.as_ref().and_then(|e| e.plain_text()))?;
                        check(completion_condition.as_ref().and_then(|e| e.plain_text()))?;
                    }
                }
            }
            if let Some(details) = &n.details
                && !matches!(
                    details,
                    crate::bpmn_model::NodeDetails::SubProcess { .. }
                        | crate::bpmn_model::NodeDetails::CallActivity { .. }
                        | crate::bpmn_model::NodeDetails::DataObjectReference { .. }
                        | crate::bpmn_model::NodeDetails::DataStoreReference { .. }
                        | crate::bpmn_model::NodeDetails::Gateway { .. }
                        | crate::bpmn_model::NodeDetails::TextAnnotation { .. }
                )
            {
                use crate::bpmn_model::{EventDefinition, NodeDetails};
                let (
                    attached,
                    definitions,
                    cancel,
                    definition_refs,
                    parallel_multiple,
                    is_interrupting,
                    properties,
                    event_io,
                ) = match details {
                    NodeDetails::BoundaryEvent {
                        attached_to,
                        cancel_activity,
                    } => (
                        Some(attached_to),
                        &[][..],
                        Some(*cancel_activity),
                        &[][..],
                        None,
                        None,
                        &[][..],
                        None,
                    ),
                    NodeDetails::SubProcess { .. }
                    | NodeDetails::CallActivity { .. }
                    | NodeDetails::DataObjectReference { .. }
                    | NodeDetails::DataStoreReference { .. }
                    | NodeDetails::Gateway { .. }
                    | NodeDetails::TextAnnotation { .. } => {
                        unreachable!()
                    }
                    NodeDetails::Event {
                        definitions,
                        attached_to,
                        cancel_activity,
                        definition_refs,
                        parallel_multiple,
                        is_interrupting,
                        properties,
                        io,
                    } => (
                        attached_to.as_ref(),
                        definitions.as_slice(),
                        *cancel_activity,
                        definition_refs.as_slice(),
                        *parallel_multiple,
                        *is_interrupting,
                        properties.as_slice(),
                        io.as_ref(),
                    ),
                };
                let event = matches!(
                    n.kind,
                    NodeKind::StartEvent
                        | NodeKind::EndEvent
                        | NodeKind::IntermediateCatchEvent
                        | NodeKind::IntermediateThrowEvent
                        | NodeKind::BoundaryEvent
                );
                if !event
                    || definitions.len() > 256
                    || definition_refs.len() > 256
                    || properties.len() > 256
                {
                    return fail("invalid event details");
                }
                let catch = matches!(
                    n.kind,
                    NodeKind::StartEvent
                        | NodeKind::IntermediateCatchEvent
                        | NodeKind::BoundaryEvent
                );
                let event_sub_start = n.kind == NodeKind::StartEvent
                    && crate::queries::find_node(d, &p.id).is_some_and(|owner| {
                        owner
                            .activity
                            .as_ref()
                            .is_some_and(|a| a.triggered_by_event == Some(true))
                    });
                if parallel_multiple.is_some() && !catch {
                    return fail("parallelMultiple requires a catch event");
                }
                if is_interrupting.is_some() && n.kind != NodeKind::StartEvent {
                    return fail("isInterrupting requires a start event");
                }
                if event_io.is_some_and(|io| {
                    !matches!(
                        (catch, io),
                        (true, crate::bpmn_model::EventIo::Catch { .. })
                            | (false, crate::bpmn_model::EventIo::Throw { .. })
                    )
                }) {
                    return fail("Event I/O direction does not match event kind");
                }
                if matches!(n.kind, NodeKind::BoundaryEvent) {
                    let Some(target) = attached else {
                        return fail("boundary event requires attachment");
                    };
                    if !target.is_external(local_namespace)
                        && !p.nodes.iter().any(|other| {
                            target.matches_identity(&other.id, local_namespace)
                                && other.kind.is_activity()
                        })
                    {
                        return fail("invalid boundary attachment");
                    }
                } else if attached.is_some() || cancel.is_some() {
                    return fail("attachment only allowed on boundary events");
                }
                let mut resolved = Vec::new();
                let mut seen_references = HashSet::new();
                let mut seen_local_roots = HashSet::new();
                for reference in definition_refs {
                    if !seen_references.insert(reference) {
                        return fail("Duplicate event definition reference");
                    }
                    if let crate::bpmn_model::EventReference::Qualified(name) = reference {
                        check_qname(name)?;
                    }
                    if reference.is_external(local_namespace) {
                        continue;
                    }
                    let definition = match reference {
                        crate::bpmn_model::EventReference::Local(selector) => {
                            root_by_selector.get(selector).copied()
                        }
                        crate::bpmn_model::EventReference::Qualified(name) => {
                            root_by_xml.get(&name.local).copied()
                        }
                    }
                    .ok_or("Missing or wrong-kind root event definition")?;
                    if let Some(id) = definition.id()
                        && !seen_local_roots.insert(id.selector())
                    {
                        return fail("Duplicate event definition reference");
                    }
                    resolved.push(definition);
                }
                for (definition, owned) in definitions
                    .iter()
                    .map(|d| (d, true))
                    .chain(resolved.into_iter().map(|d| (d, false)))
                {
                    validate_event_payload(d, definition, local_namespace)?;
                    match definition {
                        EventDefinition::Timer { id, .. } => {
                            if !catch {
                                return fail("timer definition requires a catch event");
                            }
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        EventDefinition::Signal { id, .. }
                        | EventDefinition::Message { id, .. } => {
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        EventDefinition::Error { id, .. } => {
                            if !matches!(n.kind, NodeKind::EndEvent | NodeKind::BoundaryEvent)
                                && !event_sub_start
                            {
                                return fail("invalid error event context");
                            }
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        EventDefinition::Escalation { id, .. } => {
                            if !matches!(
                                n.kind,
                                NodeKind::EndEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::BoundaryEvent
                            ) && !event_sub_start
                            {
                                return fail("invalid escalation event context");
                            }
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        EventDefinition::Conditional { id, expression, .. } => {
                            if !matches!(
                                n.kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::BoundaryEvent
                            ) {
                                return fail("conditional definition requires a catch event");
                            }
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if expression.plain_text().is_some_and(|s| s.len() > 16_384) {
                                return fail("invalid conditional expression");
                            }
                        }
                        EventDefinition::Link {
                            id,
                            name,
                            sources,
                            target,
                            ..
                        } => {
                            if !matches!(
                                n.kind,
                                NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
                            ) {
                                return fail(
                                    "Link definition requires an intermediate catch or throw event",
                                );
                            }
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            short(name)?;
                            if (n.kind == NodeKind::IntermediateCatchEvent && target.is_some())
                                || (n.kind == NodeKind::IntermediateThrowEvent
                                    && !sources.is_empty())
                            {
                                return fail("Invalid link definition direction");
                            }
                            let mut unique = HashSet::new();
                            for (reference, expected) in sources
                                .iter()
                                .map(|r| (r, NodeKind::IntermediateThrowEvent))
                                .chain(target.iter().map(|r| (r, NodeKind::IntermediateCatchEvent)))
                            {
                                if !unique.insert(reference) {
                                    return fail("Duplicate link reference");
                                }
                                let valid = p.nodes.iter().any(|other| {
                                    if other.kind != expected { return false; }
                                    let Some(NodeDetails::Event { definitions, definition_refs, .. }) = &other.details else { return false; };
                                    let matches_link = |definition: &EventDefinition| matches!(definition,
                                        EventDefinition::Link { id: Some(other_id), name: other_name, .. }
                                        if reference.matches_identity(other_id, local_namespace) && other_name == name);
                                    definitions.iter().any(matches_link) || definition_refs.iter().any(|use_ref|
                                        d.root_event_definitions.iter().any(|definition|
                                            definition.id().is_some_and(|id| use_ref.matches_identity(id, local_namespace)) && matches_link(definition)))
                                });
                                if !valid && !reference.is_external(local_namespace) {
                                    return fail("Invalid or cross-scope link reference");
                                }
                            }
                        }
                        EventDefinition::Cancel { id, .. } => {
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            let valid = match n.kind {
                                NodeKind::BoundaryEvent => {
                                    cancel != Some(false)
                                        && attached.is_some_and(|target| {
                                            target.is_external(local_namespace)
                                                || p.nodes.iter().any(|other| {
                                                    target.matches_identity(
                                                        &other.id,
                                                        local_namespace,
                                                    ) && other.kind == NodeKind::Transaction
                                                })
                                        })
                                }
                                NodeKind::EndEvent => crate::queries::find_node(d, &p.id)
                                    .is_some_and(|owner| owner.kind == NodeKind::Transaction),
                                _ => false,
                            };
                            if !valid {
                                return fail(
                                    "Cancel definition requires an interrupting transaction boundary or transaction end event",
                                );
                            }
                        }
                        EventDefinition::Compensate {
                            id, activity_ref, ..
                        } => {
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            let valid_kind = matches!(
                                n.kind,
                                NodeKind::BoundaryEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::EndEvent
                            ) || (n.kind == NodeKind::StartEvent
                                && crate::queries::find_node(d, &p.id).is_some_and(|owner| {
                                    owner
                                        .activity
                                        .as_ref()
                                        .is_some_and(|a| a.triggered_by_event == Some(true))
                                }));
                            if !valid_kind {
                                return fail("Invalid compensation event context");
                            }
                            if activity_ref.as_ref().is_some_and(|reference| {
                                !matches!(
                                    n.kind,
                                    NodeKind::IntermediateThrowEvent | NodeKind::EndEvent
                                ) || !p.nodes.iter().any(|other| {
                                    reference.matches_identity(&other.id, local_namespace)
                                        && other.kind.is_activity()
                                }) && !reference.is_external(local_namespace)
                            }) {
                                return fail("Invalid compensation activity reference");
                            }
                        }
                        EventDefinition::Terminate { id, .. }
                            if matches!(n.kind, NodeKind::EndEvent) =>
                        {
                            if owned && let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        _ => return fail("unsupported event definition"),
                    }
                }
            } else if matches!(n.kind, NodeKind::BoundaryEvent) {
                return fail("boundary event requires attachment");
            }
            shape_ids.insert(n.id.selector());
            element_scope.insert(n.id.selector(), p.id.clone());
            node_process.insert(n.id.selector(), p.id.clone());
            kinds.insert(n.id.selector(), n.kind);
        }
        for l in p.lanes {
            add(&mut ids, &l.id)?;
            if let Some(id) = &l.child_lane_set_id {
                add(&mut ids, id)?;
            }
            if let Some(name) = &l.name {
                short(name)?;
            }
            shape_ids.insert(l.id.selector());
            element_scope.insert(l.id.selector(), p.id.clone());
        }
        for f in p.flows {
            if has_flow_node_refs(&f.flow_metadata) {
                return fail("incoming/outgoing references require a flow node");
            }
            if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                return fail("Flow-element metadata requires a sequence flow");
            }
            if f.association_direction.is_some() && f.kind != FlowKind::Association {
                return fail("association direction requires association");
            }
            add(&mut ids, &f.id)?;
            flow_ids.insert(f.id.selector());
            element_scope.insert(f.id.selector(), p.id.clone());
            if let Some(name) = &f.name {
                short(name)?;
            }
        }
    }
    let target_namespace = d.definitions.as_ref().map(|v| v.target_namespace.as_str());
    for choreography in &d.choreographies {
        add(&mut ids, &choreography.id)?;
        if let Some(name) = &choreography.name {
            short(name)?;
        }
        let conversation_parents: HashMap<_, _> = choreography
            .conversation_nodes
            .iter()
            .map(|n| (n.id.selector(), n.parent.clone()))
            .collect();
        acyclic_selectors(&conversation_parents, "conversation")?;
        for node in &choreography.conversation_nodes {
            if node.call.is_some()
                && node.kind != crate::bpmn_collaboration::ConversationKind::CallConversation
            {
                return fail("Call conversation metadata requires a call conversation");
            }
            if node.parent.as_ref().is_some_and(|parent| {
                !choreography.conversation_nodes.iter().any(|n| {
                    &n.id == parent
                        && n.kind == crate::bpmn_collaboration::ConversationKind::SubConversation
                })
            }) {
                return fail("invalid parent conversation");
            }
            let mut seen = HashSet::new();
            for participant in &node.participant_refs {
                if !seen.insert(participant)
                    || (!participant.is_external(local_namespace)
                        && !choreography
                            .participants
                            .iter()
                            .any(|p| participant.matches_identity(&p.id, local_namespace)))
                {
                    return fail("invalid conversation participant");
                }
            }
            add(&mut ids, &node.id)?;
            shape_ids.insert(node.id.selector());
            element_scope.insert(node.id.selector(), choreography.id.selector());
        }
        for link in &choreography.conversation_links {
            add(&mut ids, &link.id)?;
            flow_ids.insert(link.id.selector());
            element_scope.insert(link.id.selector(), choreography.id.selector());
        }
        for participant in &choreography.participants {
            add(&mut ids, &participant.id)?;
            if let Some(name) = &participant.name {
                short(name)?;
            }
            shape_ids.insert(participant.id.selector());
            element_scope.insert(participant.id.selector(), choreography.id.selector());
            if participant.process.as_ref().is_some_and(|id| {
                !id.is_external(local_namespace)
                    && !d
                        .processes
                        .iter()
                        .any(|p| id.matches_identity(&p.id, local_namespace))
            }) {
                return fail("missing choreography participant process");
            }
        }
        for flow in &choreography.message_flows {
            if flow.flow_metadata.is_some() {
                return fail("Flow-element metadata requires a sequence flow");
            }
            add(&mut ids, &flow.id)?;
            if let Some(name) = &flow.name {
                short(name)?;
            }
            flow_ids.insert(flow.id.selector());
            element_scope.insert(flow.id.selector(), choreography.id.selector());
            let source = endpoint(&flow.source);
            let target = endpoint(&flow.target);
            if flow.kind != FlowKind::Message
                || flow.condition.is_some()
                || (source.is_some() && source == target)
                || source
                    .as_ref()
                    .is_some_and(|id| !choreography.participants.iter().any(|p| &p.id == id))
                || target
                    .as_ref()
                    .is_some_and(|id| !choreography.participants.iter().any(|p| &p.id == id))
            {
                return fail("invalid choreography message flow");
            }
        }
        if choreography.graph.is_some()
            && (!choreography.activities.is_empty() || !choreography.sequence_flows.is_empty())
        {
            return fail("mixed choreography graph representations");
        }
        for activity in &choreography.activities {
            if activity
                .flow_metadata
                .as_ref()
                .is_some_and(|m| m.is_immediate.is_some())
            {
                return fail("isImmediate requires a sequence flow");
            }
            add(&mut ids, &activity.id)?;
            if let Some(name) = &activity.name {
                short(name)?;
            }
            shape_ids.insert(activity.id.selector());
            element_scope.insert(activity.id.selector(), choreography.id.selector());
            let mut unique = HashSet::new();
            for participant in &activity.participant_refs {
                if !unique.insert(participant)
                    || (!participant.is_external(target_namespace)
                        && !choreography
                            .participants
                            .iter()
                            .any(|p| participant.matches_identity(&p.id, target_namespace)))
                {
                    return fail("invalid choreography participant");
                }
            }
            if activity
                .initiating_participant
                .as_ref()
                .is_some_and(|r| !unique.contains(r) && !r.is_external(target_namespace))
            {
                return fail("invalid initiating participant");
            }
            let mut messages = HashSet::new();
            for flow in &activity.message_flow_refs {
                if !messages.insert(flow)
                    || (!flow.is_external(target_namespace)
                        && !choreography
                            .message_flows
                            .iter()
                            .any(|f| flow.matches_identity(&f.id, target_namespace)))
                {
                    return fail("invalid choreography message reference");
                }
            }
        }
        for flow in &choreography.sequence_flows {
            if has_flow_node_refs(&flow.flow_metadata) {
                return fail("incoming/outgoing references require a flow node");
            }
            add(&mut ids, &flow.id)?;
            if let Some(name) = &flow.name {
                short(name)?;
            }
            flow_ids.insert(flow.id.selector());
            element_scope.insert(flow.id.selector(), choreography.id.selector());
            if flow.kind != FlowKind::Sequence
                || matches!(
                    &flow.source,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                )
                || matches!(
                    &flow.target,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                )
                || endpoint(&flow.source)
                    .as_ref()
                    .is_some_and(|id| !choreography.activities.iter().any(|a| &a.id == id))
                || endpoint(&flow.target)
                    .as_ref()
                    .is_some_and(|id| !choreography.activities.iter().any(|a| &a.id == id))
            {
                return fail("invalid choreography sequence flow");
            }
        }
    }
    // Graph membership and identity are scoped; never resolve an endpoint by local name alone.
    for root in &d.choreographies {
        use crate::bpmn_collaboration::{
            ChoreographyActivityDetails, ChoreographyFlowElement, CollaborationArtifact,
        };
        let Some(graph) = &root.graph else { continue };
        let mut pending = vec![(graph, root.id.selector(), 0usize)];
        while let Some((graph, owner, depth)) = pending.pop() {
            if depth > 32 {
                return fail("choreography depth limit exceeded");
            }
            count = count
                .saturating_add(graph.flow_elements.len())
                .saturating_add(graph.data_associations.len());
            if count > 10_000 {
                return fail("choreography element limit exceeded");
            }
            let nodes: HashSet<_> = graph
                .flow_elements
                .iter()
                .filter_map(|v| match v {
                    ChoreographyFlowElement::Activity(a) => Some(a.id.selector()),
                    ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                        Some(n.id.selector())
                    }
                    ChoreographyFlowElement::SequenceFlow(_) => None,
                })
                .collect();
            for association in &graph.data_associations {
                use crate::bpmn_data::DataAssociationKind;
                if let Some(id) = &association.id {
                    add(&mut ids, id)?;
                }
                if association.assignments.len() > 256 {
                    return fail("Data assignment limit exceeded");
                }
                for assignment in &association.assignments {
                    if let Some(id) = &assignment.id {
                        add(&mut ids, id)?;
                    }
                }
                let Some(event) = graph.flow_elements.iter().find_map(|item| match item {
                    ChoreographyFlowElement::Event(node) if node.id == association.owner => {
                        Some(node)
                    }
                    _ => None,
                }) else {
                    return fail("invalid choreography data association owner");
                };
                let Some(crate::bpmn_model::NodeDetails::Event {
                    io: Some(io),
                    properties,
                    ..
                }) = &event.details
                else {
                    return fail("data association requires compatible owner ports");
                };
                use crate::bpmn_model::EventIo;
                let (inputs, outputs): (
                    &[crate::bpmn_data::DataPort],
                    &[crate::bpmn_data::DataPort],
                ) = match io {
                    EventIo::Catch { outputs, .. } => (&[], outputs),
                    EventIo::Throw { inputs, .. } => (inputs, &[]),
                };
                let local = |id: &crate::ids::ElementSelector| {
                    properties
                        .iter()
                        .any(|p| p.id.as_ref().is_some_and(|identity| identity == id))
                        || d.data_stores
                            .iter()
                            .any(|store| store.id.as_ref().is_some_and(|identity| identity == id))
                };
                let mut unique = HashSet::new();
                if association.sources.iter().any(|id| !unique.insert(id)) {
                    return fail("invalid data association sources");
                }
                let valid = match association.kind {
                    DataAssociationKind::Input => {
                        inputs.iter().any(|p| {
                            p.id.as_ref()
                                .is_some_and(|identity| identity == &association.target)
                        }) && association.sources.iter().all(local)
                    }
                    DataAssociationKind::Output => {
                        local(&association.target)
                            && association.sources.iter().all(|id| {
                                outputs
                                    .iter()
                                    .any(|p| p.id.as_ref().is_some_and(|identity| identity == id))
                            })
                    }
                };
                if !valid {
                    return fail("invalid data association ports");
                }
            }
            for element in &graph.flow_elements {
                match element {
                    ChoreographyFlowElement::Activity(a) => {
                        add(&mut ids, &a.id)?;
                        shape_ids.insert(a.id.selector());
                        element_scope.insert(a.id.selector(), owner.clone());
                        if let Some(name) = &a.name {
                            short(name)?;
                        }
                        if a.details.is_some() && !a.message_flow_refs.is_empty() {
                            return fail(
                                "non-task choreography activity has task message references",
                            );
                        }
                        if let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) =
                            &a.details
                        {
                            count = count.saturating_add(artifacts.len());
                            if count > 10_000 {
                                return fail("choreography element limit exceeded");
                            }
                            for artifact in artifacts {
                                match artifact {
                                    CollaborationArtifact::Group(g) => {
                                        add(&mut ids, &g.id)?;
                                        shape_ids.insert(g.id.selector());
                                        element_scope.insert(g.id.selector(), a.id.selector());
                                    }
                                    CollaborationArtifact::TextAnnotation(n) => {
                                        add(&mut ids, &n.id)?;
                                        shape_ids.insert(n.id.selector());
                                        element_scope.insert(n.id.selector(), a.id.selector());
                                    }
                                    CollaborationArtifact::Association(f) => {
                                        add(&mut ids, &f.id)?;
                                        flow_ids.insert(f.id.selector());
                                        element_scope.insert(f.id.selector(), a.id.selector());
                                    }
                                }
                            }
                            pending.push((graph, a.id.selector(), depth + 1));
                        }
                    }
                    ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                        let event = matches!(
                            n.kind,
                            NodeKind::StartEvent
                                | NodeKind::EndEvent
                                | NodeKind::IntermediateCatchEvent
                                | NodeKind::IntermediateThrowEvent
                                | NodeKind::BoundaryEvent
                        );
                        let gateway = matches!(
                            n.kind,
                            NodeKind::ExclusiveGateway
                                | NodeKind::InclusiveGateway
                                | NodeKind::ParallelGateway
                                | NodeKind::ComplexGateway
                                | NodeKind::EventBasedGateway
                        );
                        if (matches!(element, ChoreographyFlowElement::Event(_)) && !event)
                            || (matches!(element, ChoreographyFlowElement::Gateway(_)) && !gateway)
                        {
                            return fail("invalid choreography flow node kind");
                        }
                        if event {
                            use crate::bpmn_model::NodeDetails;
                            let attachment = match &n.details {
                                Some(NodeDetails::BoundaryEvent { attached_to, .. }) => {
                                    Some(attached_to)
                                }
                                Some(NodeDetails::Event { attached_to, .. }) => {
                                    attached_to.as_ref()
                                }
                                None => None,
                                _ => return fail("invalid choreography event details"),
                            };
                            if n.kind == NodeKind::BoundaryEvent {
                                if attachment.is_some_and(|attachment| !attachment.is_external(local_namespace) && !graph.flow_elements.iter().any(|element| matches!(element, ChoreographyFlowElement::Activity(activity) if attachment.matches_identity(&activity.id, local_namespace)))) {
                                    return fail("invalid choreography boundary attachment");
                                }
                            } else if attachment.is_some() {
                                return fail("attachment only allowed on boundary events");
                            }
                        } else if !matches!(
                            n.details,
                            None | Some(crate::bpmn_model::NodeDetails::Gateway { .. })
                        ) {
                            return fail("invalid choreography gateway details");
                        }
                        add(&mut ids, &n.id)?;
                        if let Some(crate::bpmn_model::NodeDetails::Event {
                            io: Some(io), ..
                        }) = &n.details
                        {
                            let catch = matches!(
                                n.kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::BoundaryEvent
                            );
                            if !matches!(
                                (catch, io),
                                (true, crate::bpmn_model::EventIo::Catch { .. })
                                    | (false, crate::bpmn_model::EventIo::Throw { .. })
                            ) {
                                return fail("Event I/O direction does not match event kind");
                            }
                        }
                        if let Some(crate::bpmn_model::NodeDetails::Event { definitions, .. }) =
                            &n.details
                        {
                            if definitions.len() > 256 {
                                return fail("Event definition limit exceeded");
                            }
                            for definition in definitions {
                                validate_event_payload(d, definition, local_namespace)?;
                                if let Some(id) = definition.id() {
                                    add(&mut ids, id)?;
                                }
                            }
                        }
                        shape_ids.insert(n.id.selector());
                        element_scope.insert(n.id.selector(), owner.clone());
                    }
                    ChoreographyFlowElement::SequenceFlow(f) => {
                        add(&mut ids, &f.id)?;
                        flow_ids.insert(f.id.selector());
                        element_scope.insert(f.id.selector(), owner.clone());
                        if f.kind != FlowKind::Sequence
                            || matches!(
                                f.source,
                                Some(crate::bpmn_activity::ModelReference::Qualified(_))
                            )
                            || matches!(
                                f.target,
                                Some(crate::bpmn_activity::ModelReference::Qualified(_))
                            )
                            || endpoint(&f.source)
                                .as_ref()
                                .is_some_and(|id| !nodes.contains(id))
                            || endpoint(&f.target)
                                .as_ref()
                                .is_some_and(|id| !nodes.contains(id))
                        {
                            return fail("invalid choreography sequence flow");
                        }
                    }
                }
            }
        }
    }
    for c in d.collaboration.iter().chain(&d.additional_collaborations) {
        add(&mut ids, &c.id)?;
        if let Some(name) = &c.name {
            definitions_text(name)?;
        }

        for p in &c.participants {
            add(&mut ids, &p.id)?;
            shape_ids.insert(p.id.selector());
            // Older native projects placed their own participant on a process plane.
            // Retain those projects without accepting participants from other processes.
            if let Some(process) = endpoint(&p.process) {
                element_scope.insert(p.id.selector(), process);
            }
            if let Some(name) = &p.name {
                short(name)?;
            }

            if p.process.as_ref().is_some_and(|id| {
                !id.is_external(local_namespace)
                    && !d
                        .processes
                        .iter()
                        .any(|x| id.matches_identity(&x.id, local_namespace))
            }) {
                return fail("missing participant process");
            }
        }
        for f in &c.flows {
            if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                return fail("Flow-element metadata requires a sequence flow");
            }
            add(&mut ids, &f.id)?;
            flow_ids.insert(f.id.selector());
            if let Some(name) = &f.name {
                short(name)?;
            }
        }
        let parents: HashMap<_, _> = c
            .conversation_nodes
            .iter()
            .map(|n| (n.id.selector(), n.parent.clone()))
            .collect();
        acyclic_selectors(&parents, "conversation")?;
        for node in &c.conversation_nodes {
            if node.call.is_some()
                && node.kind != crate::bpmn_collaboration::ConversationKind::CallConversation
            {
                return fail("Call conversation metadata requires a call conversation");
            }
            add(&mut ids, &node.id)?;
            if let Some(name) = &node.name {
                short(name)?;
            }
            shape_ids.insert(node.id.selector());
            if node.parent.as_ref().is_some_and(|parent| {
                !c.conversation_nodes.iter().any(|n| {
                    &n.id == parent
                        && matches!(
                            n.kind,
                            crate::bpmn_collaboration::ConversationKind::SubConversation
                        )
                })
            }) {
                return fail("invalid parent conversation");
            }
            let mut unique = HashSet::new();
            for participant in &node.participant_refs {
                if !unique.insert(participant)
                    || (!participant.is_external(local_namespace)
                        && !c
                            .participants
                            .iter()
                            .any(|p| participant.matches_identity(&p.id, local_namespace)))
                {
                    return fail("invalid conversation participant");
                }
            }
        }
        for link in &c.conversation_links {
            add(&mut ids, &link.id)?;
            if let Some(name) = &link.name {
                short(name)?;
            }
            flow_ids.insert(link.id.selector());
            let participant =
                |id: &crate::ids::ElementSelector| c.participants.iter().any(|p| &p.id == id);
            let conversation =
                |id: &crate::ids::ElementSelector| c.conversation_nodes.iter().any(|n| &n.id == id);
            if let (Some(source), Some(target)) = (endpoint(&link.source), endpoint(&link.target))
                && !((participant(&source) && conversation(&target))
                    || (conversation(&source) && participant(&target)))
            {
                return fail("invalid conversation link endpoints");
            }
        }
    }
    for p in &scopes {
        let lanes: HashMap<_, _> = p
            .lanes
            .iter()
            .map(|l| (l.id.selector(), l.parent.clone()))
            .collect();
        acyclic_selectors(&lanes, "lane")?;
        let mut members = HashSet::new();
        for l in p.lanes {
            for m in &l.members {
                if node_process.get(m) != Some(&p.id) || !members.insert(m) {
                    return fail("invalid or duplicate lane membership");
                }
            }
        }
        for association in p.associations {
            use crate::bpmn_data::DataAssociationKind;
            let Some(owner) = p.nodes.iter().find(|n| n.id == association.owner) else {
                return fail("invalid data association owner");
            };
            let (inputs, outputs): (&[crate::bpmn_data::DataPort], &[crate::bpmn_data::DataPort]) =
                if owner.kind.is_activity() {
                    let Some(io) = &owner.io_specification else {
                        return fail("data association requires activity ports");
                    };
                    (&io.inputs, &io.outputs)
                } else if let Some(crate::bpmn_model::NodeDetails::Event {
                    io: Some(event_io),
                    ..
                }) = &owner.details
                {
                    match event_io {
                        crate::bpmn_model::EventIo::Catch { outputs, .. } => (&[], outputs),
                        crate::bpmn_model::EventIo::Throw { inputs, .. } => (inputs, &[]),
                    }
                } else {
                    return fail("data association requires compatible owner ports");
                };
            let local = |id: &crate::ids::ElementSelector, input_source: bool| {
                if let Some(crate::bpmn_model::NodeDetails::Event { properties, .. }) =
                    &owner.details
                    && properties
                        .iter()
                        .any(|property| property.id.as_ref().is_some_and(|identity| identity == id))
                {
                    return true;
                }
                if d.data_stores
                    .iter()
                    .any(|store| store.id.as_ref().is_some_and(|identity| identity == id))
                {
                    return true;
                }
                let mut current = Some(p.id.clone());
                while let Some(scope_id) = current {
                    let Some(scope) = scopes_by_id.get(&scope_id) else {
                        break;
                    };
                    if scope
                        .objects
                        .iter()
                        .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                        || scope.nodes.iter().any(|n| {
                            n.id == *id
                                && matches!(
                                    n.kind,
                                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                                )
                        })
                        || d.processes
                            .iter()
                            .find(|process| process.id == scope_id)
                            .and_then(|process| process.io_specification.as_ref())
                            .is_some_and(|io| {
                                let ports = if input_source {
                                    &io.inputs
                                } else {
                                    &io.outputs
                                };
                                ports.iter().any(|port| {
                                    port.id.as_ref().is_some_and(|identity| identity == id)
                                })
                            })
                    {
                        return true;
                    }
                    current = scope.parent.clone();
                }
                false
            };
            let mut unique = HashSet::new();
            if association.sources.iter().any(|id| !unique.insert(id)) {
                return fail("invalid data association sources");
            }
            let valid = match association.kind {
                DataAssociationKind::Input => {
                    inputs.iter().any(|port| {
                        port.id
                            .as_ref()
                            .is_some_and(|identity| identity == &association.target)
                    }) && association.sources.iter().all(|id| local(id, true))
                }
                DataAssociationKind::Output => {
                    local(&association.target, false)
                        && association.sources.iter().all(|id| {
                            outputs
                                .iter()
                                .any(|port| port.id.as_ref().is_some_and(|identity| identity == id))
                        })
                }
            };
            if !valid {
                return fail("invalid data association ports");
            }
        }
        for f in p.flows {
            if f.kind != FlowKind::Sequence && f.condition.is_some() {
                return fail("condition requires sequence flow");
            }
            if f.kind == FlowKind::Sequence
                && (matches!(
                    &f.source,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                ) || matches!(
                    &f.target,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                ))
            {
                return fail("Sequence flow endpoints require IDREF selectors");
            }
            let source = endpoint(&f.source);
            let target = endpoint(&f.target);
            if f.kind == FlowKind::Message {
                return fail("invalid process flow endpoints");
            }
            if let (Some(source), Some(target)) = (&source, &target) {
                if f.kind == FlowKind::Association
                    && !group_ids.contains(source)
                    && !group_ids.contains(target)
                    && !matches!(
                        kinds.get(source),
                        Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                    )
                    && !matches!(
                        kinds.get(target),
                        Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                    )
                {
                    return fail("association requires an artifact endpoint");
                }
                let association_endpoint = |reference: &crate::ids::ElementSelector| {
                    node_process.get(reference) == Some(&p.id)
                        || p.flows
                            .iter()
                            .any(|flow| flow.id == *reference && flow.kind == FlowKind::Sequence)
                };
                if (f.kind == FlowKind::Association
                    && (!association_endpoint(source) || !association_endpoint(target)))
                    || (f.kind != FlowKind::Association
                        && (node_process.get(source) != Some(&p.id)
                            || node_process.get(target) != Some(&p.id)))
                {
                    return fail("invalid process flow endpoints");
                }
                if f.kind == FlowKind::Sequence
                    && (!kinds.get(source).is_some_and(|kind| {
                        is_flow_node(*kind) && !matches!(kind, NodeKind::EndEvent)
                    }) || !kinds.get(target).is_some_and(|kind| {
                        is_flow_node(*kind) && !matches!(kind, NodeKind::StartEvent)
                    }))
                {
                    return fail("invalid sequence endpoint");
                }
            }
        }
    }
    for c in d.collaboration.iter().chain(&d.additional_collaborations) {
        let participant_for = |id: &crate::ids::ElementSelector,
                               is_source: bool|
         -> Option<crate::ids::ElementSelector> {
            if let Some(p) = c.participants.iter().find(|p| &p.id == id) {
                return Some(p.id.selector());
            }
            // BPMN 2.0.2 §9.4: message flows leave activities and throwing
            // events, and reach activities and catching events.
            if !kinds.get(id).is_some_and(|kind| {
                kind.is_activity()
                    || if is_source {
                        matches!(kind, NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                    } else {
                        matches!(
                            kind,
                            NodeKind::StartEvent
                                | NodeKind::IntermediateCatchEvent
                                | NodeKind::BoundaryEvent
                        )
                    }
            }) {
                return None;
            }
            let process = node_process.get(id)?;
            let mut ps = c
                .participants
                .iter()
                .filter(|p| endpoint(&p.process).as_ref() == Some(process));
            let first = ps.next()?;
            if ps.next().is_some() {
                None
            } else {
                Some(first.id.selector())
            }
        };
        for f in &c.flows {
            if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                return fail("Flow-element metadata requires a sequence flow");
            }
            let source = endpoint(&f.source);
            let target = endpoint(&f.target);
            if f.condition.is_some()
                || f.kind != FlowKind::Message
                || matches!((&source, &target), (Some(source), Some(target)) if participant_for(source, true).is_none() || participant_for(target, false).is_none() || participant_for(source, true) == participant_for(target, false))
            {
                return fail("invalid message flow endpoints");
            }
        }
    }
    for diagram in &d.diagrams {
        add(&mut ids, &diagram.id)?;
        if let Some(name) = &diagram.name {
            definitions_text(name)?;
        }
        add(&mut ids, &diagram.plane_id)?;
        di_metadata(&diagram.plane_metadata)?;
        if let Some(order) = &diagram.plane_order {
            let expected: HashSet<_> = diagram
                .shapes
                .iter()
                .map(|s| s.id.selector())
                .chain(diagram.edges.iter().map(|e| e.id.selector()))
                .collect();
            let actual: HashSet<_> = order.iter().cloned().collect();
            if actual.len() != order.len() || actual != expected {
                return fail("Plane order must contain each representation exactly once");
            }
        }
        for style in &diagram.label_styles {
            if let Some(id) = &style.id {
                add(&mut ids, id)?;
            }
        }
        let check_label = |label: &Option<Label>| -> Result<(), String> {
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Local(target)) = &label.style_ref
                && !diagram
                    .label_styles
                    .iter()
                    .any(|style| style.id.as_ref().is_some_and(|id| id == target))
            {
                return fail("Label style reference is outside its diagram");
            }
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Qualified(target)) = &label.style_ref
            {
                check_qname(target)?;
                if local(target)
                    && !diagram.label_styles.iter().any(|style| {
                        style.id.as_ref().and_then(|id| id.xml_id()) == Some(&target.local)
                    })
                {
                    return fail("Label style reference is outside its diagram");
                }
            }
            Ok(())
        };
        let choreography_scopes = crate::queries::choreography_scopes(d);
        if !scopes.iter().any(|p| p.id == diagram.plane)
            && !choreography_scopes
                .iter()
                .any(|scope| scope.owner == diagram.plane)
            && !d.collaborations().any(|c| c.id == diagram.plane)
        {
            return fail("invalid diagram plane");
        }
        let mut contained = HashSet::new();
        if choreography_scopes
            .iter()
            .any(|scope| scope.owner == diagram.plane)
        {
            contained.insert(diagram.plane.clone());
            // Scopes are ordered parent-before-child. Only the selected scope and
            // descendants belong to this plane, never siblings or ancestors.
            for scope in &choreography_scopes {
                if scope
                    .parent
                    .as_ref()
                    .is_some_and(|parent| contained.contains(parent))
                {
                    contained.insert(scope.owner.clone());
                }
            }
        }
        if let Some(start) = scopes.iter().find(|p| p.id == diagram.plane) {
            let mut pending = vec![start];
            while let Some(scope) = pending.pop() {
                contained.insert(scope.id.clone());
                for node in scope.nodes {
                    if matches!(
                        node.details,
                        Some(crate::bpmn_model::NodeDetails::SubProcess { .. })
                    ) && let Some(child) = scopes.iter().find(|s| s.id == node.id.selector())
                    {
                        pending.push(child);
                    }
                }
            }
        }
        for s in &diagram.shapes {
            di_metadata(&s.di_metadata)?;
            let mut scoped_participant_band = false;
            if let Some(reference) = &s.choreography_activity_shape {
                let owner = match reference {
                    crate::model::DiReference::Local(selector) => {
                        diagram.shapes.iter().find(|shape| shape.id == *selector)
                    }
                    crate::model::DiReference::Qualified(qname) => {
                        check_qname(qname)?;
                        if local(qname) {
                            diagram
                                .shapes
                                .iter()
                                .find(|shape| shape.id.xml_id() == Some(&qname.local))
                        } else {
                            None
                        }
                    }
                };
                let external = matches!(reference, crate::model::DiReference::Qualified(qname) if !local(qname));
                if let Some(shape) = owner {
                    scoped_participant_band = choreography_scopes.iter().any(|scope| {
                        contained.contains(&scope.owner)
                            && scope.activities.iter().any(|activity| {
                                activity.id == shape.element
                                    && scope.root.participants.iter().any(|participant| {
                                        participant.id == s.element
                                            && activity.participant_refs.iter().any(|reference| {
                                                reference.matches_identity(
                                                    &participant.id,
                                                    local_namespace,
                                                )
                                            })
                                    })
                            })
                    });
                }
                if !external && !scoped_participant_band {
                    return fail(
                        "Participant band must reference a participant of a choreography activity shape in its diagram",
                    );
                }
            }
            add(&mut ids, &s.id)?;
            let global_store = d
                .data_stores
                .iter()
                .any(|store| store.id.as_ref().is_some_and(|id| id == &s.element));
            if !global_store
                && !scoped_participant_band
                && !contained.is_empty()
                && !element_scope
                    .get(&s.element)
                    .is_some_and(|scope| contained.contains(scope))
            {
                return Err(format!("shape {} is outside diagram scope", s.element));
            }
            if !shape_ids.contains(&s.element) {
                return fail("invalid shape reference");
            }
            bounds(&s.bounds)?;
            label(&s.label)?;
            check_label(&s.label)?;
            if let Some(id) = s.label.as_ref().and_then(|l| l.id.as_ref()) {
                add(&mut ids, id)?;
            }
        }
        let representations: HashSet<_> = diagram
            .shapes
            .iter()
            .map(|shape| shape.id.selector())
            .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
            .collect();
        for e in &diagram.edges {
            di_metadata(&e.di_metadata)?;
            add(&mut ids, &e.id)?;
            if e.source_element
                .iter()
                .chain(e.target_element.iter())
                .any(|reference| !representations.contains(reference))
            {
                return fail("Edge endpoint references a representation outside its diagram");
            }
            let scoped_message_flow = choreography_scopes.iter().any(|scope| {
                contained.contains(&scope.owner)
                    && scope.root.message_flows.iter().any(|flow| {
                        flow.id == e.element
                            && scope.activities.iter().any(|activity| {
                                activity.message_flow_refs.iter().any(|reference| {
                                    reference.matches_identity(&flow.id, local_namespace)
                                })
                            })
                    })
            });
            if !scoped_message_flow
                && !contained.is_empty()
                && !element_scope
                    .get(&e.element)
                    .is_some_and(|scope| contained.contains(scope))
            {
                return fail("edge outside diagram scope");
            }
            if !flow_ids.contains(&e.element) || e.waypoints.len() < 2 {
                return fail("invalid edge reference or route");
            }
            for p in &e.waypoints {
                if !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x.abs() > 1_000_000_000.0
                    || p.y.abs() > 1_000_000_000.0
                {
                    return fail("invalid waypoint");
                }
            }
            label(&e.label)?;
            check_label(&e.label)?;
            if let Some(id) = e.label.as_ref().and_then(|l| l.id.as_ref()) {
                add(&mut ids, id)?;
            }
        }
    }
    if let Capability::RetainedExtensions { extensions, .. } = &d.capability {
        let semantics = semantic_ids(d);
        for e in extensions {
            if !semantics.contains(&e.owner) && !ids.iter().any(|id| id.selector() == e.owner) {
                return fail("missing extension owner");
            }
        }
    }
    if ids.len() > 10_000 {
        return fail("document element limit exceeded");
    }
    Ok(ids.len())
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
