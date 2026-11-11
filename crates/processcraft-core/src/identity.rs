use crate::ids::XmlId;
use crate::model::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

fn fail<T>(message: &str) -> Result<T, String> {
    Err(message.into())
}
fn add<'a>(set: &mut HashSet<&'a XmlId>, id: &'a XmlId) -> Result<(), String> {
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
fn label(l: &Option<Label>) -> Result<(), String> {
    if let Some(l) = l {
        bounds(&l.bounds)?;
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
fn validate_io<'a>(
    io: &'a crate::bpmn_data::IoSpecification,
    ids: &mut HashSet<&'a XmlId>,
    shape_ids: &mut HashSet<&'a XmlId>,
    element_scope: &mut HashMap<&'a XmlId, &'a XmlId>,
    scope_id: &'a XmlId,
) -> Result<(), String> {
    if let Some(id) = &io.id {
        add(ids, id)?;
    }
    for port in io.inputs.iter().chain(&io.outputs) {
        if let Some(id) = &port.id {
            add(ids, id)?;
            shape_ids.insert(id);
            element_scope.insert(id, scope_id);
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
                    !unique.insert(id) || !ports.iter().any(|p| p.id.as_ref() == Some(id))
                }) {
                    return fail("invalid I/O set port reference");
                }
            }
            let mut unique = HashSet::new();
            if set
                .counterpart_refs
                .iter()
                .any(|id| !unique.insert(id) || !other.iter().any(|s| s.id.as_ref() == Some(id)))
            {
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
        let mut checkpoint_bytes = serde_json::to_vec(&current_metadata)
            .map_err(|e| e.to_string())?
            .len();
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
            checkpoint_bytes = checkpoint_bytes
                .checked_add(
                    serde_json::to_vec(&metadata)
                        .map_err(|e| e.to_string())?
                        .len(),
                )
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
                    role_sources_index.insert((document.id, &lane.id));
                }
                for node in nodes {
                    if node.kind.is_activity() {
                        activity_sources_index.insert((document.id, &node.id));
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
            let source = (link.source.document_id, &link.source.xml_id);
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
pub(crate) fn semantic_ids(d: &BpmnDocument) -> HashSet<&XmlId> {
    let mut s = HashSet::new();
    s.insert(&d.definitions_id);
    let mut metadata = Vec::new();
    for store in &d.data_stores {
        metadata.extend(store.item_aware.iter());
    }
    for process in &d.processes {
        if let Some(io) = &process.io_specification {
            if let Some(id) = &io.id {
                s.insert(id);
            }
            for set in io.input_sets.iter().chain(&io.output_sets) {
                if let Some(id) = &set.id {
                    s.insert(id);
                }
            }
            for port in io.inputs.iter().chain(&io.outputs) {
                if let Some(id) = &port.id {
                    s.insert(id);
                }
                metadata.extend(port.item_aware.iter());
            }
        }
    }
    for scope in crate::queries::scopes(d) {
        for object in scope.data_objects {
            metadata.extend(object.item_aware.iter());
        }
        for association in scope.data_associations {
            for assignment in &association.assignments {
                if let Some(id) = &assignment.id {
                    s.insert(id);
                }
            }
        }
        for node in scope.nodes {
            metadata.extend(node.item_aware.iter());
            if let Some(io) = &node.io_specification {
                for port in io.inputs.iter().chain(&io.outputs) {
                    metadata.extend(port.item_aware.iter());
                }
            }
        }
    }
    for item in metadata {
        if let Some(id) = item.data_state.as_ref().and_then(|state| state.id.as_ref()) {
            s.insert(id);
        }
    }
    for item in &d.catalog.item_definitions {
        if let Some(id) = &item.id {
            s.insert(id);
        }
    }
    for interface in &d.catalog.interfaces {
        if let Some(id) = &interface.id {
            s.insert(id);
        }
        for operation in &interface.operations {
            if let Some(id) = &operation.id {
                s.insert(id);
            }
        }
    }
    for resource in &d.catalog.resources {
        if let Some(id) = &resource.id {
            s.insert(id);
        }
        for parameter in &resource.parameters {
            if let Some(id) = &parameter.id {
                s.insert(id);
            }
        }
    }
    for expression in d.expressions() {
        if let crate::bpmn_model::Expression::Structured(data) = expression
            && let Some(id) = &data.id
        {
            s.insert(id);
        }
    }
    for category in &d.categories {
        s.insert(&category.id);
        for value in &category.values {
            s.insert(&value.id);
        }
    }
    for store in &d.data_stores {
        if let Some(id) = &store.id {
            s.insert(id);
        }
    }
    let mut pending: Vec<(&[Node], &[Lane], &[Flow])> = Vec::new();
    for p in &d.processes {
        s.insert(&p.id);
        if let Some(id) = &p.lane_set_id {
            s.insert(id);
        }
        for group in &p.groups {
            s.insert(&group.id);
        }
        pending.push((&p.nodes, &p.lanes, &p.flows));
    }
    while let Some((nodes, lanes, flows)) = pending.pop() {
        for n in nodes {
            s.insert(&n.id);
            if let Some(crate::bpmn_model::NodeDetails::Event { definitions, .. }) = &n.details {
                use crate::bpmn_model::EventDefinition;
                for definition in definitions {
                    let id = match definition {
                        EventDefinition::Message { id, .. }
                        | EventDefinition::Signal { id, .. }
                        | EventDefinition::Error { id, .. }
                        | EventDefinition::Escalation { id, .. }
                        | EventDefinition::Conditional { id, .. }
                        | EventDefinition::Link { id, .. }
                        | EventDefinition::Cancel { id }
                        | EventDefinition::Compensate { id, .. }
                        | EventDefinition::Terminate { id }
                        | EventDefinition::Timer { id, .. } => id,
                    };
                    if let Some(id) = id {
                        s.insert(id);
                    }
                }
            }
            if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &n.details {
                if let Some(id) = &content.lane_set_id {
                    s.insert(id);
                }
                for group in &content.groups {
                    s.insert(&group.id);
                }
                pending.push((&content.nodes, &content.lanes, &content.flows));
            }
        }
        for l in lanes {
            s.insert(&l.id);
            if let Some(id) = &l.child_lane_set_id {
                s.insert(id);
            }
        }
        for f in flows {
            s.insert(&f.id);
        }
    }
    for choreography in &d.choreographies {
        s.insert(&choreography.id);
        for participant in &choreography.participants {
            s.insert(&participant.id);
        }
        for flow in &choreography.message_flows {
            s.insert(&flow.id);
        }
        for activity in &choreography.activities {
            s.insert(&activity.id);
        }
        for flow in &choreography.sequence_flows {
            s.insert(&flow.id);
        }
    }
    let mut data_pending: Vec<(
        &[Node],
        &[crate::bpmn_data::DataObject],
        &[crate::bpmn_data::DataAssociation],
    )> = d
        .processes
        .iter()
        .map(|p| {
            (
                p.nodes.as_slice(),
                p.data_objects.as_slice(),
                p.data_associations.as_slice(),
            )
        })
        .collect();
    while let Some((nodes, objects, associations)) = data_pending.pop() {
        for o in objects {
            if let Some(id) = &o.id {
                s.insert(id);
            }
        }
        for a in associations {
            if let Some(id) = &a.id {
                s.insert(id);
            }
        }
        for node in nodes {
            if let Some(io) = &node.io_specification {
                if let Some(id) = &io.id {
                    s.insert(id);
                }
                for set in io.input_sets.iter().chain(&io.output_sets) {
                    if let Some(id) = &set.id {
                        s.insert(id);
                    }
                }
                for port in io.inputs.iter().chain(&io.outputs) {
                    if let Some(id) = &port.id {
                        s.insert(id);
                    }
                }
            }
            if let Some(crate::bpmn_model::NodeDetails::SubProcess { content }) = &node.details {
                data_pending.push((
                    &content.nodes,
                    &content.data_objects,
                    &content.data_associations,
                ));
            }
        }
    }
    if let Some(c) = &d.collaboration {
        s.insert(&c.id);
        for p in &c.participants {
            s.insert(&p.id);
        }
        for f in &c.flows {
            s.insert(&f.id);
        }
        for n in &c.conversation_nodes {
            s.insert(&n.id);
        }
        for link in &c.conversation_links {
            s.insert(&link.id);
        }
    }
    s
}
fn validate_document(d: &BpmnDocument, resources: &HashSet<&XmlId>) -> Result<usize, String> {
    short(&d.name)?;
    if let Some(metadata) = &d.definitions {
        if metadata.target_namespace.is_empty() {
            return fail("Empty target namespace is outside the qualified editable profile");
        }
        definitions_text(&metadata.target_namespace)?;
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
    let local =
        |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
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
                if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Message { id, .. } if id == &reference.local)) {
                    return fail("Operation references a missing local message");
                }
            }
            for reference in &operation.error_refs {
                check_qname(reference)?;
                if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Error { id, .. } if id == &reference.local)) {
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
                    && !catalog
                        .item_definitions
                        .iter()
                        .any(|item| item.id.as_ref() == Some(&reference.local))
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
    for scope in crate::queries::scopes(d) {
        for object in scope.data_objects {
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
                let Some(item) = catalog
                    .item_definitions
                    .iter()
                    .find(|item| item.id.as_ref() == Some(&reference.local))
                else {
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
        .collect();
    for expression in d.expressions() {
        use crate::bpmn_model::{Expression, ExpressionKind};
        let transformation = transformations
            .iter()
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
            definitions_text(&data.text)?;
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
                        && !catalog
                            .item_definitions
                            .iter()
                            .any(|item| item.id.as_ref() == Some(&name.local))
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
            EventResource::Signal { id, name }
            | EventResource::Message { id, name }
            | EventResource::Error { id, name, .. }
            | EventResource::Escalation { id, name, .. } => (id, name),
        };
        add(&mut ids, id)?;
        if let Some(name) = name {
            short(name)?;
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
        id: &'a XmlId,
        parent: Option<&'a XmlId>,
        nodes: &'a [Node],
        lanes: &'a [Lane],
        flows: &'a [Flow],
        objects: &'a [crate::bpmn_data::DataObject],
        associations: &'a [crate::bpmn_data::DataAssociation],
        lane_set_id: Option<&'a XmlId>,
        groups: &'a [crate::bpmn_model::Group],
    }
    let mut scopes: Vec<Scope<'_>> = Vec::new();
    let mut pending: Vec<(Scope<'_>, usize)> = d
        .processes
        .iter()
        .map(|p| {
            (
                Scope {
                    id: &p.id,
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
                        id: &n.id,
                        parent: Some(scope.id),
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
                ) if !called_element.namespace.is_empty() => (),
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
                    if scope.objects.iter().any(|o| o.id.as_ref() == Some(id)) {
                        return true;
                    }
                    let mut parent = scope.parent;
                    while let Some(scope_id) = parent {
                        let Some(ancestor) =
                            scopes.iter().find(|candidate| candidate.id == scope_id)
                        else {
                            break;
                        };
                        if ancestor.objects.iter().any(|o| o.id.as_ref() == Some(id)) {
                            return true;
                        }
                        parent = ancestor.parent;
                    }
                    false
                }) => {}
                (
                    NodeKind::DataStoreReference,
                    Some(crate::bpmn_model::NodeDetails::DataStoreReference { data_store_ref }),
                ) if data_store_ref
                    .as_ref()
                    .is_none_or(|reference| match reference {
                        crate::bpmn_data::DataStoreRef::Legacy(id) => {
                            d.data_stores.iter().any(|o| o.id.as_ref() == Some(id))
                        }
                        crate::bpmn_data::DataStoreRef::Qualified(name) => {
                            !local(name)
                                || d.data_stores
                                    .iter()
                                    .any(|o| o.id.as_ref() == Some(&name.local))
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
                            &f.id == id && f.source == n.id && f.kind == FlowKind::Sequence
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
                        && (!matches!(n.kind, NodeKind::ComplexGateway) || condition.len() > 16_384)
                    {
                        return fail("invalid gateway activation condition");
                    }
                }
                (_, Some(crate::bpmn_model::NodeDetails::Gateway { .. })) => {
                    return fail("gateway details require gateway");
                }
                (
                    NodeKind::TextAnnotation,
                    Some(crate::bpmn_model::NodeDetails::TextAnnotation {
                        text_format: Some(format),
                    }),
                ) => short(format)?,
                (
                    NodeKind::TextAnnotation,
                    Some(crate::bpmn_model::NodeDetails::TextAnnotation { text_format: None }),
                ) => {}
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
            shape_ids.insert(id);
        }
    }
    for process in &d.processes {
        add(&mut ids, &process.id)?;
        short(&process.name)?;
        if let Some(io) = &process.io_specification {
            validate_io(
                io,
                &mut ids,
                &mut shape_ids,
                &mut element_scope,
                &process.id,
            )?;
        }
    }
    let scopes_by_id: HashMap<_, _> = scopes.iter().map(|scope| (scope.id, scope)).collect();
    for p in &scopes {
        if let Some(id) = p.lane_set_id {
            add(&mut ids, id)?;
        }
        for group in p.groups {
            add(&mut ids, &group.id)?;
            shape_ids.insert(&group.id);
            group_ids.insert(&group.id);
            node_process.insert(&group.id, p.id);
            element_scope.insert(&group.id, p.id);
            if group.category_value_ref.as_ref().is_some_and(|id| {
                !d.categories
                    .iter()
                    .any(|c| c.values.iter().any(|v| &v.id == id))
            }) {
                return fail("missing group category value");
            }
        }
        for object in p.objects {
            if let Some(id) = &object.id {
                add(&mut ids, id)?;
                shape_ids.insert(id);
                element_scope.insert(id, p.id);
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
                flow_ids.insert(id);
                element_scope.insert(id, p.id);
            }
        }
        for n in p.nodes {
            add(&mut ids, &n.id)?;
            if let Some(io) = &n.io_specification {
                if !n.kind.is_activity() {
                    return fail("I/O specification requires activity");
                }
                validate_io(io, &mut ids, &mut shape_ids, &mut element_scope, p.id)?;
            }
            short(&n.name)?;
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
                    !p.flows
                        .iter()
                        .any(|f| &f.id == id && f.source == n.id && f.kind == FlowKind::Sequence)
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
                    &activity.script,
                ] {
                    check_text(value)?;
                }
                if let Some(value) = &activity.completion_condition {
                    short(value.text())?;
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
                        check(condition.as_deref())?;
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
                        check(cardinality.as_deref())?;
                        check(completion_condition.as_deref())?;
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
                let (attached, definitions, cancel) = match details {
                    NodeDetails::BoundaryEvent {
                        attached_to,
                        cancel_activity,
                    } => (Some(attached_to), &[][..], Some(*cancel_activity)),
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
                    } => (
                        attached_to.as_ref(),
                        definitions.as_slice(),
                        *cancel_activity,
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
                if !event || definitions.len() > 1 {
                    return fail("invalid event details");
                }
                if matches!(n.kind, NodeKind::BoundaryEvent) {
                    let Some(target) = attached else {
                        return fail("boundary event requires attachment");
                    };
                    if !p
                        .nodes
                        .iter()
                        .any(|other| &other.id == target && other.kind.is_activity())
                    {
                        return fail("invalid boundary attachment");
                    }
                } else if attached.is_some() || cancel.is_some() {
                    return fail("attachment only allowed on boundary events");
                }
                for definition in definitions {
                    match definition {
                        EventDefinition::Timer { id, expression, .. } => {
                            if !matches!(
                                n.kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::BoundaryEvent
                            ) {
                                return fail("timer definition requires a catch event");
                            }
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if expression.len() > 16_384 {
                                return fail("invalid timer expression");
                            }
                        }
                        EventDefinition::Signal { id, signal_ref } => {
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if signal_ref.as_ref().is_some_and(|r| {
                                !d.event_resources.iter().any(
                                    |e| matches!(e, EventResource::Signal { id, .. } if id == r),
                                )
                            }) {
                                return fail("missing signal declaration");
                            }
                        }
                        EventDefinition::Message {
                            id,
                            message_ref,
                            operation_ref,
                        } => {
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if operation_ref.is_some() {
                                return fail("unsupported operation reference");
                            }
                            if message_ref.as_ref().is_some_and(|r| {
                                !d.event_resources.iter().any(
                                    |e| matches!(e, EventResource::Message { id, .. } if id == r),
                                )
                            }) {
                                return fail("missing message declaration");
                            }
                        }
                        EventDefinition::Error { id, error_ref } => {
                            if !matches!(n.kind, NodeKind::EndEvent | NodeKind::BoundaryEvent) {
                                return fail("error event requires end or boundary event");
                            }
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if error_ref.as_ref().is_some_and(|r| {
                                !d.event_resources.iter().any(
                                    |e| matches!(e, EventResource::Error { id, .. } if id == r),
                                )
                            }) {
                                return fail("missing error declaration");
                            }
                        }
                        EventDefinition::Escalation { id, escalation_ref } => {
                            if !matches!(
                                n.kind,
                                NodeKind::EndEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::BoundaryEvent
                            ) {
                                return fail("invalid escalation event");
                            }
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if escalation_ref.as_ref().is_some_and(|r| !d.event_resources.iter().any(|e| matches!(e, EventResource::Escalation { id, .. } if id == r))) { return fail("missing escalation declaration"); }
                        }
                        EventDefinition::Conditional { id, expression } => {
                            if !matches!(
                                n.kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::BoundaryEvent
                            ) {
                                return fail("conditional definition requires a catch event");
                            }
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            if expression.len() > 16_384 {
                                return fail("invalid conditional expression");
                            }
                        }
                        EventDefinition::Link {
                            id,
                            name,
                            sources,
                            target,
                        } => {
                            if !matches!(
                                n.kind,
                                NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
                            ) {
                                return fail(
                                    "Link definition requires an intermediate catch or throw event",
                                );
                            }
                            if let Some(id) = id {
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
                                let valid = p.nodes.iter().any(|other| other.kind == expected && matches!(&other.details,
                                    Some(NodeDetails::Event { definitions, .. }) if definitions.iter().any(|definition|
                                        matches!(definition, EventDefinition::Link { id: Some(other_id), name: other_name, .. }
                                            if other_id == reference && other_name == name))));
                                if !valid {
                                    return fail("Invalid or cross-scope link reference");
                                }
                            }
                        }
                        EventDefinition::Cancel { id } => {
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            let valid = match n.kind {
                                NodeKind::BoundaryEvent => {
                                    cancel != Some(false)
                                        && attached.is_some_and(|target| {
                                            p.nodes.iter().any(|other| {
                                                &other.id == target
                                                    && other.kind == NodeKind::Transaction
                                            })
                                        })
                                }
                                NodeKind::EndEvent => crate::queries::find_node(d, p.id)
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
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                            let valid_kind = matches!(
                                n.kind,
                                NodeKind::BoundaryEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::EndEvent
                            ) || (n.kind == NodeKind::StartEvent
                                && crate::queries::find_node(d, p.id).is_some_and(|owner| {
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
                                ) || !p
                                    .nodes
                                    .iter()
                                    .any(|other| &other.id == reference && other.kind.is_activity())
                            }) {
                                return fail("Invalid compensation activity reference");
                            }
                        }
                        EventDefinition::Terminate { id }
                            if matches!(n.kind, NodeKind::EndEvent) =>
                        {
                            if let Some(id) = id {
                                add(&mut ids, id)?;
                            }
                        }
                        _ => return fail("unsupported event definition"),
                    }
                }
            } else if matches!(n.kind, NodeKind::BoundaryEvent) {
                return fail("boundary event requires attachment");
            }
            shape_ids.insert(&n.id);
            element_scope.insert(&n.id, p.id);
            node_process.insert(&n.id, p.id);
            kinds.insert(&n.id, n.kind);
        }
        for l in p.lanes {
            add(&mut ids, &l.id)?;
            if let Some(id) = &l.child_lane_set_id {
                add(&mut ids, id)?;
            }
            short(&l.name)?;
            shape_ids.insert(&l.id);
            element_scope.insert(&l.id, p.id);
        }
        for f in p.flows {
            if f.association_direction.is_some() && f.kind != FlowKind::Association {
                return fail("association direction requires association");
            }
            add(&mut ids, &f.id)?;
            flow_ids.insert(&f.id);
            element_scope.insert(&f.id, p.id);
            short(&f.name)?;
        }
    }
    for choreography in &d.choreographies {
        add(&mut ids, &choreography.id)?;
        short(&choreography.name)?;
        for participant in &choreography.participants {
            add(&mut ids, &participant.id)?;
            short(&participant.name)?;
            shape_ids.insert(&participant.id);
            element_scope.insert(&participant.id, &choreography.id);
            if participant
                .process
                .as_ref()
                .is_some_and(|id| !d.processes.iter().any(|p| &p.id == id))
            {
                return fail("missing choreography participant process");
            }
        }
        for flow in &choreography.message_flows {
            add(&mut ids, &flow.id)?;
            short(&flow.name)?;
            flow_ids.insert(&flow.id);
            element_scope.insert(&flow.id, &choreography.id);
            if flow.kind != FlowKind::Message
                || flow.condition.is_some()
                || flow.source == flow.target
                || !choreography
                    .participants
                    .iter()
                    .any(|p| p.id == flow.source)
                || !choreography
                    .participants
                    .iter()
                    .any(|p| p.id == flow.target)
            {
                return fail("invalid choreography message flow");
            }
        }
        for activity in &choreography.activities {
            add(&mut ids, &activity.id)?;
            short(&activity.name)?;
            shape_ids.insert(&activity.id);
            element_scope.insert(&activity.id, &choreography.id);
            let mut unique = HashSet::new();
            for participant in &activity.participant_refs {
                if !unique.insert(participant)
                    || !choreography
                        .participants
                        .iter()
                        .any(|p| &p.id == participant)
                {
                    return fail("invalid choreography participant");
                }
            }
            if !unique.contains(&activity.initiating_participant) {
                return fail("invalid initiating participant");
            }
            let mut messages = HashSet::new();
            for flow in &activity.message_flow_refs {
                if !messages.insert(flow)
                    || !choreography.message_flows.iter().any(|f| &f.id == flow)
                {
                    return fail("invalid choreography message reference");
                }
            }
        }
        for flow in &choreography.sequence_flows {
            add(&mut ids, &flow.id)?;
            short(&flow.name)?;
            flow_ids.insert(&flow.id);
            element_scope.insert(&flow.id, &choreography.id);
            if flow.kind != FlowKind::Sequence
                || !choreography.activities.iter().any(|a| a.id == flow.source)
                || !choreography.activities.iter().any(|a| a.id == flow.target)
            {
                return fail("invalid choreography sequence flow");
            }
        }
    }
    if let Some(c) = &d.collaboration {
        add(&mut ids, &c.id)?;

        for p in &c.participants {
            add(&mut ids, &p.id)?;
            shape_ids.insert(&p.id);
            // Older native projects placed their own participant on a process plane.
            // Retain those projects without accepting participants from other processes.
            if let Some(process) = &p.process {
                element_scope.insert(&p.id, process);
            }
            short(&p.name)?;

            if p.process
                .as_ref()
                .is_some_and(|id| !d.processes.iter().any(|x| &x.id == id))
            {
                return fail("missing participant process");
            }
        }
        for f in &c.flows {
            add(&mut ids, &f.id)?;
            flow_ids.insert(&f.id);
            short(&f.name)?;
        }
        let parents: HashMap<_, _> = c
            .conversation_nodes
            .iter()
            .map(|n| (&n.id, n.parent.as_ref()))
            .collect();
        acyclic(&parents, "conversation")?;
        for node in &c.conversation_nodes {
            add(&mut ids, &node.id)?;
            short(&node.name)?;
            shape_ids.insert(&node.id);
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
                    || !c.participants.iter().any(|p| &p.id == participant)
                {
                    return fail("invalid conversation participant");
                }
            }
        }
        for link in &c.conversation_links {
            add(&mut ids, &link.id)?;
            short(&link.name)?;
            flow_ids.insert(&link.id);
            let participant = |id: &XmlId| c.participants.iter().any(|p| &p.id == id);
            let conversation = |id: &XmlId| c.conversation_nodes.iter().any(|n| &n.id == id);
            if !((participant(&link.source) && conversation(&link.target))
                || (conversation(&link.source) && participant(&link.target)))
            {
                return fail("invalid conversation link endpoints");
            }
        }
    }
    for p in &scopes {
        let lanes: HashMap<_, _> = p.lanes.iter().map(|l| (&l.id, l.parent.as_ref())).collect();
        acyclic(&lanes, "lane")?;
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
            let Some(owner) = p
                .nodes
                .iter()
                .find(|n| n.id == association.owner && n.kind.is_activity())
            else {
                return fail("invalid data association owner");
            };
            let Some(io) = &owner.io_specification else {
                return fail("data association requires activity ports");
            };
            let local = |id: &XmlId, input_source: bool| {
                if d.data_stores
                    .iter()
                    .any(|store| store.id.as_ref() == Some(id))
                {
                    return true;
                }
                let mut current = Some(p.id);
                while let Some(scope_id) = current {
                    let Some(scope) = scopes_by_id.get(scope_id) else {
                        break;
                    };
                    if scope.objects.iter().any(|o| o.id.as_ref() == Some(id))
                        || scope.nodes.iter().any(|n| {
                            &n.id == id
                                && matches!(
                                    n.kind,
                                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                                )
                        })
                        || d.processes
                            .iter()
                            .find(|process| &process.id == scope_id)
                            .and_then(|process| process.io_specification.as_ref())
                            .is_some_and(|io| {
                                let ports = if input_source {
                                    &io.inputs
                                } else {
                                    &io.outputs
                                };
                                ports.iter().any(|port| port.id.as_ref() == Some(id))
                            })
                    {
                        return true;
                    }
                    current = scope.parent;
                }
                false
            };
            let mut unique = HashSet::new();
            if association.sources.iter().any(|id| !unique.insert(id)) {
                return fail("invalid data association sources");
            }
            let valid = match association.kind {
                DataAssociationKind::Input => {
                    io.inputs
                        .iter()
                        .any(|port| port.id.as_ref() == Some(&association.target))
                        && association.sources.iter().all(|id| local(id, true))
                }
                DataAssociationKind::Output => {
                    local(&association.target, false)
                        && association
                            .sources
                            .iter()
                            .all(|id| io.outputs.iter().any(|port| port.id.as_ref() == Some(id)))
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
            if f.kind == FlowKind::Association
                && !group_ids.contains(&f.source)
                && !group_ids.contains(&f.target)
                && !matches!(
                    kinds.get(&f.source),
                    Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                )
                && !matches!(
                    kinds.get(&f.target),
                    Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                )
            {
                return fail("association requires an artifact endpoint");
            }
            if f.kind == FlowKind::Message
                || node_process.get(&f.source) != Some(&p.id)
                || node_process.get(&f.target) != Some(&p.id)
            {
                return fail("invalid process flow endpoints");
            }
            if f.kind == FlowKind::Sequence
                && (!kinds
                    .get(&f.source)
                    .is_some_and(|kind| is_flow_node(*kind) && !matches!(kind, NodeKind::EndEvent))
                    || !kinds.get(&f.target).is_some_and(|kind| {
                        is_flow_node(*kind) && !matches!(kind, NodeKind::StartEvent)
                    }))
            {
                return fail("invalid sequence endpoint");
            }
        }
    }
    if let Some(c) = &d.collaboration {
        let participant_for = |id: &XmlId| -> Option<&XmlId> {
            if let Some(p) = c.participants.iter().find(|p| &p.id == id) {
                return Some(&p.id);
            }
            if !kinds.get(id).is_some_and(|kind| kind.is_activity()) {
                return None;
            }
            let process = node_process.get(id)?;
            let mut ps = c
                .participants
                .iter()
                .filter(|p| p.process.as_ref() == Some(*process));
            let first = ps.next()?;
            if ps.next().is_some() {
                None
            } else {
                Some(&first.id)
            }
        };
        for f in &c.flows {
            if f.condition.is_some()
                || f.kind != FlowKind::Message
                || participant_for(&f.source).is_none()
                || participant_for(&f.target).is_none()
                || participant_for(&f.source) == participant_for(&f.target)
            {
                return fail("invalid message flow endpoints");
            }
        }
    }
    for diagram in &d.diagrams {
        add(&mut ids, &diagram.id)?;
        add(&mut ids, &diagram.plane_id)?;
        if !scopes.iter().any(|p| *p.id == diagram.plane)
            && !d.choreographies.iter().any(|c| c.id == diagram.plane)
            && !d
                .collaboration
                .as_ref()
                .is_some_and(|c| c.id == diagram.plane)
        {
            return fail("invalid diagram plane");
        }
        let mut contained = HashSet::new();
        if let Some(choreography) = d.choreographies.iter().find(|c| c.id == diagram.plane) {
            contained.insert(&choreography.id);
        }
        if let Some(start) = scopes.iter().find(|p| *p.id == diagram.plane) {
            let mut pending = vec![start];
            while let Some(scope) = pending.pop() {
                contained.insert(scope.id);
                for node in scope.nodes {
                    if matches!(
                        node.details,
                        Some(crate::bpmn_model::NodeDetails::SubProcess { .. })
                    ) && let Some(child) = scopes.iter().find(|s| *s.id == node.id)
                    {
                        pending.push(child);
                    }
                }
            }
        }
        for s in &diagram.shapes {
            add(&mut ids, &s.id)?;
            let global_store = d
                .data_stores
                .iter()
                .any(|store| store.id.as_ref() == Some(&s.element));
            if !global_store
                && !contained.is_empty()
                && !element_scope
                    .get(&s.element)
                    .is_some_and(|scope| contained.contains(scope))
            {
                return fail("shape outside diagram scope");
            }
            if !shape_ids.contains(&s.element) {
                return fail("invalid shape reference");
            }
            bounds(&s.bounds)?;
            label(&s.label)?;
        }
        for e in &diagram.edges {
            add(&mut ids, &e.id)?;
            if !contained.is_empty()
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
        }
    }
    if let Capability::RetainedExtensions { extensions, .. } = &d.capability {
        let semantics = semantic_ids(d);
        for e in extensions {
            if !semantics.contains(&e.owner) && !ids.contains(&e.owner) {
                return fail("missing extension owner");
            }
        }
    }
    if ids.len() > 10_000 {
        return fail("document element limit exceeded");
    }
    Ok(ids.len())
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
