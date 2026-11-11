use crate::args::{Command, Query};
use processcraft_core::{
    commands::{CommandError, Envelope, Request, Session, command_descriptors},
    model::ProjectData,
};
use processcraft_io::{Access, FileGrant, FileSession, ProjectSaveOutcome, bpmn, native};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
};

pub struct Error {
    pub code: i32,
    pub message: String,
}
fn error(code: i32, message: impl fmt::Display) -> Error {
    Error {
        code,
        message: message.to_string(),
    }
}
fn path(p: &Path) -> Result<PathBuf, Error> {
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        Ok(std::env::current_dir().map_err(|e| error(1, e))?.join(p))
    }
}
fn grant(p: &Path, access: Access) -> Result<FileGrant, Error> {
    FileGrant::new(path(p)?, access).map_err(|e| {
        error(
            if e.kind() == std::io::ErrorKind::WouldBlock {
                4
            } else {
                1
            },
            e,
        )
    })
}
fn read(p: &Path) -> Result<Vec<u8>, Error> {
    grant(p, Access::ReadOnly)?.read().map_err(|e| error(1, e))
}
fn emit(v: &Value) {
    println!("{}", serde_json::to_string(v).expect("serializable JSON"));
}
fn bpmn_path(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bpmn"))
}
fn inspect(p: &Path, validate: bool) -> Result<(), Error> {
    let bytes = read(p)?;
    if bpmn_path(p) {
        let imported = bpmn::import_bpmn(&bytes).map_err(|e| error(3, e))?;
        let r = imported.report();
        emit(
            &json!({"format":"bpmn", "valid":true, "capability":"preservedSource", "semanticEditsAllowed":r.semantic_edits_allowed, "elementCount":r.element_count, "diagramCount":r.diagram_count, "limitations":r.limitations}),
        );
    } else {
        let loaded = native::load_bytes(&bytes)
            .map_err(|e| error(3, format!("invalid native project: {e}")))?;
        loaded.project.validate().map_err(|e| error(3, e))?;
        let session = Session::new(loaded.project)
            .map_err(|e| error(3, format!("invalid project: {e:?}")))?;
        let s = session.snapshot();
        emit(
            &json!({"format":"processcraft", "valid":true, "validation":if validate {"native codec and core structural invariants"} else {"not requested"}, "projectId":s.project.id, "name":s.project.name, "documents":s.project.documents.len(), "contentId":s.content_id, "requiresSaveAsOrBackup":loaded.requires_save_as_or_backup, "warnings":[]}),
        );
    }
    Ok(())
}
fn parse_batch(bytes: &[u8]) -> Result<Request, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(error(2, "batch exceeds 4 MiB"));
    }
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Check.deserialize(&mut d).map_err(|e| error(2, e))?;
    d.end().map_err(|e| error(2, e))?;
    let req: Request = serde_json::from_slice(bytes).map_err(|e| error(2, e))?;
    if !matches!(req, Request::Batch { .. }) {
        return Err(error(2, "--batch requires a batch request"));
    }
    Ok(req)
}
struct Check;
impl<'de> DeserializeSeed<'de> for Check {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
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
        while let Some(k) = a.next_key::<String>()? {
            if !keys.insert(k) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            a.next_value_seed(Check)?;
        }
        Ok(())
    }
}
fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
fn distinct_files(input: &Path, output: &Path) -> Result<(), Error> {
    let input = grant(input, Access::ReadOnly)?;
    let output = grant(output, Access::Write)?;
    if input.same_target(&output).map_err(|e| error(1, e))? {
        return Err(error(2, "input and output must differ"));
    }
    Ok(())
}
fn run(
    new: Option<String>,
    input: Option<PathBuf>,
    batch: PathBuf,
    out: PathBuf,
) -> Result<(), Error> {
    let batch_bytes = grant(&batch, Access::ReadOnly)?
        .read_limited(4 * 1024 * 1024)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                error(2, e)
            } else {
                error(1, e)
            }
        })?;
    let request = parse_batch(&batch_bytes)?;
    let out = path(&out)?;
    let (mut session, mut file, same) = if let Some(input) = input {
        if bpmn_path(&input) {
            return Err(error(2, "BPMN imports are read-only and cannot be run"));
        }
        let input = path(&input)?;
        let initial = native::load_bytes(&read(&input)?).map_err(|e| error(3, e))?;
        let core = Session::new(initial.project).map_err(|e| error(3, format!("{e:?}")))?;
        let (file, loaded) =
            FileSession::open(core.id(), grant(&input, Access::Write)?).map_err(|e| {
                error(
                    if e.kind() == std::io::ErrorKind::InvalidData {
                        3
                    } else {
                        1
                    },
                    e,
                )
            })?;
        if loaded.project.content_identity().map_err(|e| error(3, e))? != core.snapshot().content_id
        {
            return Err(error(4, "input changed while opening"));
        }
        if !file.is_writable() {
            return Err(error(4, "input is owned by another writer"));
        }
        let same = same_path(&input, &out);
        if same && loaded.requires_save_as_or_backup {
            return Err(error(4, "migrated project requires Save As"));
        }
        (core, Some(file), same)
    } else {
        (
            Session::new(ProjectData::new(new.expect("clap requires name")))
                .map_err(|e| error(2, format!("{e:?}")))?,
            None,
            false,
        )
    };
    if file.is_none() {
        session.mark_unsaved();
    }
    let id = session.id();
    let result = session
        .apply(
            Envelope {
                request_id: uuid::Uuid::new_v4().to_string(),
                session_id: id.clone(),
                expected_generation: Some(session.snapshot().generation),
            },
            request,
        )
        .map_err(|e| {
            let code = match e {
                CommandError::GenerationConflict | CommandError::SessionMismatch => 4,
                CommandError::InvalidParams(_)
                | CommandError::CapabilityDenied
                | CommandError::NothingToUndo
                | CommandError::NothingToRedo => 3,
            };
            error(code, format!("command failed: {e:?}"))
        })?;
    #[cfg(all(debug_assertions, feature = "test-fault"))]
    if std::env::var_os("PROCESSCRAFT_TEST_INDETERMINATE").is_some() {
        let attempted = processcraft_io::SaveReceipt {
            operation_id: uuid::Uuid::new_v4(),
            session_id: id.clone(),
            operation: processcraft_io::Operation::Save,
            target: out.clone(),
            saved_content_id: session.snapshot().content_id,
            disk_digest: [0; 32],
        };
        let outcome = ProjectSaveOutcome::Indeterminate {
            attempted,
            error: std::io::Error::other("injected indeterminate save"),
            stage: out.with_extension("test-stage"),
            backup: out.with_extension("test-backup"),
        };
        let (output, result) = save_result(outcome, &id, result.generation);
        if let Some(output) = output {
            emit(&output);
        }
        return result;
    }
    let snapshot = session.snapshot();
    let outcome = if let Some(f) = file.as_mut() {
        if same {
            f.save(&snapshot.project)
        } else {
            f.save_as(grant(&out, Access::Write)?, &snapshot.project)
        }
    } else {
        let mut f = FileSession::create(id.clone(), grant(&out, Access::Write)?).map_err(|e| {
            error(
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    4
                } else {
                    1
                },
                e,
            )
        })?;
        f.save(&snapshot.project)
    };
    let (output, result) = save_result(outcome, &id, result.generation);
    if let Some(output) = output {
        emit(&output);
    }
    result
}
fn save_result(
    outcome: ProjectSaveOutcome,
    id: &str,
    generation: u64,
) -> (Option<Value>, Result<(), Error>) {
    match outcome {
        ProjectSaveOutcome::Committed { receipt, warning } => {
            if let Some(w) = warning {
                eprintln!("save warning: {w}");
            }
            (
                Some(
                    json!({"ok":true,"sessionId":id,"generation":generation,"contentId":receipt.saved_content_id,"operationId":receipt.operation_id.to_string(),"target":receipt.target.to_string_lossy()}),
                ),
                Ok(()),
            )
        }
        ProjectSaveOutcome::NotCommitted { error: e } => (
            None,
            Err(error(
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.to_string().contains("writer")
                    || e.to_string().contains("changed")
                {
                    4
                } else {
                    1
                },
                e,
            )),
        ),
        ProjectSaveOutcome::Indeterminate {
            attempted,
            error: e,
            stage,
            backup,
        } => (
            Some(
                json!({"ok":false,"status":"indeterminate","operationId":attempted.operation_id.to_string(),"sessionId":attempted.session_id,"target":attempted.target.to_string_lossy(),"contentId":attempted.saved_content_id,"diskDigest":attempted.disk_digest,"stage":stage.to_string_lossy(),"backup":backup.to_string_lossy(),"reconciliationRequired":true,"reconciliation":"Inspect target and retained artifacts before retrying"}),
            ),
            Err(error(1, e)),
        ),
    }
}
pub fn execute(command: Command) -> Result<(), Error> {
    match command {
        Command::LayoutPreview {
            input,
            document_id,
            diagram_id,
            out,
        } => layout_preview_file(&input, &document_id, &diagram_id, &out),
        Command::LayoutApply {
            input,
            proposal,
            out,
        } => layout_apply_file(&input, &proposal, &out),
        Command::ExportImage {
            input,
            document_id,
            diagram_id,
            format,
            out,
            scale,
            dark,
            overwrite,
            acknowledge_diagnostics,
        } => export_image_cli(
            &input,
            &document_id,
            &diagram_id,
            format,
            &out,
            scale,
            dark,
            overwrite,
            acknowledge_diagnostics,
        ),
        Command::ImportBpmn {
            input,
            out,
            overwrite,
        } => import_bpmn_cli(&input, &out, overwrite),
        Command::ExportBpmn {
            input,
            document_id,
            out,
            overwrite,
            acknowledge_native_fields,
        } => export_bpmn_cli(
            &input,
            &document_id,
            &out,
            overwrite,
            acknowledge_native_fields,
        ),
        Command::Mcp {
            new,
            input,
            attach,
            token,
            session,
            read_roots,
            write_roots,
        } => crate::mcp::run(
            new,
            input,
            read_roots,
            write_roots,
            attach
                .zip(token)
                .zip(session)
                .map(|((endpoint, token), session)| (endpoint, token, session)),
        ),
        Command::Query { path, query } => query_project(&path, query),
        Command::Info { path, json: _ } => inspect(&path, false),
        Command::Validate { path, json: _ } => inspect(&path, true),
        Command::Commands { json: _ } => {
            emit(
                &json!({"commands":command_descriptors(), "requestSchema":processcraft_core::commands::request_schema()}),
            );
            Ok(())
        }
        Command::Run {
            new,
            input,
            batch,
            out,
        } => run(new, input, batch, out),
    }
}

fn bounded_page(
    items: impl Iterator<Item = Value>,
    total: usize,
    offset: usize,
    limit: usize,
) -> Result<Value, Error> {
    if !(1..=100).contains(&limit) {
        return Err(error(2, "limit must be 1..100"));
    }
    let mut page = Vec::new();
    let mut bytes = 0usize;
    for item in items {
        let size = serde_json::to_vec(&item).map_err(|e| error(3, e))?.len();
        if bytes.saturating_add(size) > 3 * 1024 * 1024 {
            break;
        }
        bytes += size;
        page.push(item);
    }
    let next = offset.saturating_add(page.len());
    Ok(
        json!({"total":total,"items":page,"nextOffset":if next < total { Some(next) } else { None }}),
    )
}

fn query_project(path: &Path, query: Query) -> Result<(), Error> {
    use processcraft_core::{
        ids::{DictionaryEntryId, DocumentId, XmlId},
        queries,
    };
    let loaded = native::load_bytes(&read(path)?).map_err(|e| error(3, e))?;
    let project = &loaded.project;
    let result = match query {
        Query::Comments { offset, limit } => {
            let total = project.comments.len();
            let values = project
                .comments
                .iter()
                .skip(offset)
                .take(limit)
                .map(|c| json!({"comment":c,"orphan":queries::comment_is_orphan(project,c)}));
            bounded_page(values, total, offset, limit)?
        }
        Query::Checkpoints { offset, limit } => {
            let total = project.checkpoints.len();
            let values = project.checkpoints.iter().skip(offset).take(limit).map(|c| json!({"id":c.id,"name":c.name,"createdUnixMs":c.created_unix_ms,"reference":project.reference_checkpoint == Some(c.id)}));
            bounded_page(values, total, offset, limit)?
        }
        Query::CompareCheckpoint { checkpoint_id } => {
            let id = checkpoint_id.parse().map_err(|e| error(2, e))?;
            json!({"comparison":queries::compare_checkpoint(project,id).ok_or_else(|| error(3,"checkpoint not found"))?})
        }
        Query::Library { text } => json!({"documents": queries::library_search(project, &text)}),
        Query::Dictionary => json!({"entries": project.dictionary_entries}),
        Query::Usages { entry_id } => {
            let id: DictionaryEntryId = entry_id.parse().map_err(|e| error(2, e))?;
            json!({"usages": queries::dictionary_usages(project, id)})
        }
        Query::Table {
            document_id,
            process_id,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(2, e))?;
            let process: XmlId = process_id.parse().map_err(|e| error(2, e))?;
            let rows = queries::activity_table(project, document, &process)
                .ok_or_else(|| error(3, "process not found"))?;
            let rows: Vec<_> = rows.into_iter().map(|row| json!({"key":row.key,"name":row.name,"lane":row.lane,"role":row.role,"systems":row.systems,"documents":row.documents,"outgoing":row.outgoing.into_iter().map(|flow| json!({"id":flow.id,"target":flow.target,"condition":flow.condition})).collect::<Vec<_>>() })).collect();
            json!({"rows":rows})
        }
    };
    emit(&result);
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLayoutProposal {
    base_content_id: [u8; 32],
    proposal: processcraft_core::layout::LayoutProposal,
}

fn layout_preview_file(
    input: &Path,
    document_id: &str,
    diagram_id: &str,
    out: &Path,
) -> Result<(), Error> {
    if out.exists() {
        return Err(error(4, "output exists"));
    }
    distinct_files(input, out)?;
    let project = native::load_bytes(&read(input)?)
        .map_err(|e| error(3, e))?
        .project;
    let session = Session::new(project).map_err(|e| error(3, format!("{e:?}")))?;
    let snapshot = session.snapshot();
    let id = document_id.parse().map_err(|e| error(2, e))?;
    let diagram = diagram_id.parse().map_err(|e| error(2, e))?;
    let doc = snapshot
        .project
        .documents
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| error(3, "document not found"))?;
    let proposal = processcraft_core::layout::propose_layout(
        doc,
        &diagram,
        &session.id(),
        snapshot.generation,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| error(3, e))?;
    let bytes = serde_json::to_vec(&FileLayoutProposal {
        base_content_id: snapshot.content_id,
        proposal,
    })
    .map_err(|e| error(3, e))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(error(3, "proposal exceeds 4 MiB"));
    }
    write_export(out, &bytes, json!({"kind":"layoutPreview"}), true)
}
fn layout_apply_file(input: &Path, proposal: &Path, out: &Path) -> Result<(), Error> {
    if out.exists() {
        return Err(error(4, "output exists"));
    }
    distinct_files(input, out)?;
    distinct_files(proposal, out)?;
    let bytes = grant(proposal, Access::ReadOnly)?
        .read_limited(4 * 1024 * 1024)
        .map_err(|e| error(2, e))?;
    let mut check = serde_json::Deserializer::from_slice(&bytes);
    Check.deserialize(&mut check).map_err(|e| error(2, e))?;
    check.end().map_err(|e| error(2, e))?;
    let mut file: FileLayoutProposal = serde_json::from_slice(&bytes).map_err(|e| error(2, e))?;
    let loaded = native::load_bytes(&read(input)?).map_err(|e| error(3, e))?;
    let mut session = Session::new(loaded.project).map_err(|e| error(3, format!("{e:?}")))?;
    let snapshot = session.snapshot();
    if file.base_content_id != snapshot.content_id {
        return Err(error(4, "project changed since preview"));
    }
    let document = snapshot
        .project
        .documents
        .iter()
        .find(|d| d.id == file.proposal.document_id)
        .ok_or_else(|| error(3, "document not found"))?;
    processcraft_core::layout::validate_proposal(document, &file.proposal)
        .map_err(|e| error(3, e))?;
    // A file opens in a fresh ephemeral session. Only after checking the full content identity
    // and unchanged geometry do we rebind the proposal to this process's session epoch.
    file.proposal.session_id = session.id();
    file.proposal.generation = snapshot.generation;
    session
        .apply(
            Envelope {
                request_id: "file-layout".into(),
                session_id: session.id(),
                expected_generation: Some(snapshot.generation),
            },
            Request::Batch {
                mutations: vec![processcraft_core::commands::Mutation::ApplyLayout {
                    proposal: file.proposal,
                }],
            },
        )
        .map_err(|e| error(3, format!("{e:?}")))?;
    let id = session.id();
    let mut target =
        FileSession::create(id.clone(), grant(out, Access::Write)?).map_err(|e| error(1, e))?;
    if out.exists() {
        return Err(error(4, "output exists"));
    }
    let (result, status) = save_result(
        target.save(&session.snapshot().project),
        &id,
        session.snapshot().generation,
    );
    if let Some(result) = result {
        emit(&result);
    }
    status
}

fn import_bpmn_cli(input: &Path, out: &Path, overwrite: bool) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(4, "output exists; --overwrite is required"));
    }
    distinct_files(input, out)?;
    let bytes = read(input)?;
    let name = input
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Imported BPMN");
    let prepared =
        processcraft_io::interchange::prepare_import(&bytes, name).map_err(|e| error(3, e))?;
    let editable = prepared.report.editable;
    let limitations = prepared.report.limitations;
    let document_id = prepared.document.id;
    let mut project = ProjectData::new(name.to_owned());
    project
        .documents
        .push(std::sync::Arc::new(prepared.document));
    project.resources = prepared.resources;
    let mut session = Session::new(project).map_err(|e| error(3, format!("{e:?}")))?;
    session.mark_unsaved();
    let id = session.id();
    let mut file =
        FileSession::create(id.clone(), grant(out, Access::Write)?).map_err(|e| error(1, e))?;
    let outcome = file.save(&session.snapshot().project);
    let (result, status) = save_result(outcome, &id, session.snapshot().generation);
    if let Some(mut result) = result {
        result["documentId"] = json!(document_id);
        result["editable"] = json!(editable);
        result["limitations"] = json!(limitations);
        emit(&result);
    }
    status
}

fn export_bpmn_cli(
    input: &Path,
    document_id: &str,
    out: &Path,
    overwrite: bool,
    acknowledge: bool,
) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(4, "output exists; --overwrite is required"));
    }
    distinct_files(input, out)?;
    let loaded = native::load_bytes(&read(input)?).map_err(|e| error(3, e))?;
    let id = document_id.parse().map_err(|e| error(2, e))?;
    let result = processcraft_io::interchange::export_bpmn(&loaded.project, id, acknowledge)
        .map_err(|e| {
            error(
                3,
                format!(
                    "export blocked: {:?}; omitted: {:?}",
                    e.report.blocked, e.report.omitted
                ),
            )
        })?;
    write_export(
        out,
        &result.bytes,
        json!({"retained":result.report.retained,"transformed":result.report.transformed,"omitted":result.report.omitted}),
        false,
    )
}

fn write_export(out: &Path, bytes: &[u8], report: Value, must_be_new: bool) -> Result<(), Error> {
    use processcraft_io::{OpenSession, SaveOutcome, WriterSession};
    use sha2::{Digest, Sha256};
    let operation = uuid::Uuid::new_v4();
    let session = uuid::Uuid::new_v4();
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let grant = grant(out, Access::Write)?;
    let mut writer = match WriterSession::open(&grant).map_err(|e| error(1, e))? {
        OpenSession::Writable(w) => w,
        OpenSession::ReadOnly => return Err(error(4, "output has another writer")),
    };
    if must_be_new && out.exists() {
        return Err(error(4, "output exists"));
    }
    match writer.save(bytes) {
        SaveOutcome::Committed { warning } => {
            emit(
                &json!({"ok":true,"status":"committed","operationId":operation,"diskDigest":digest,"report":report,"warning":warning.map(|e|e.to_string())}),
            );
            Ok(())
        }
        SaveOutcome::NotCommitted { error: e } => Err(error(1, e)),
        SaveOutcome::Indeterminate {
            error: e,
            stage,
            backup,
        } => {
            emit(
                &json!({"ok":false,"status":"indeterminate","operationId":operation,"sessionId":session,"target":out,"diskDigest":digest,"stage":stage,"backup":backup,"reconciliationRequired":true,"reconciliation":"Inspect target and retained artifacts before retrying"}),
            );
            Err(error(1, e))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn export_image_cli(
    input: &Path,
    document_id: &str,
    diagram_id: &str,
    format: crate::args::ImageFormat,
    out: &Path,
    scale: f32,
    dark: bool,
    overwrite: bool,
    acknowledge_diagnostics: bool,
) -> Result<(), Error> {
    use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine};
    if out.exists() && !overwrite {
        return Err(error(4, "output exists; --overwrite is required"));
    }
    distinct_files(input, out)?;
    let loaded = native::load_bytes(&read(input)?).map_err(|e| error(3, e))?;
    let id = document_id.parse().map_err(|e| error(2, e))?;
    let diagram = diagram_id.parse().map_err(|e| error(2, e))?;
    let doc = loaded
        .project
        .documents
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| error(3, "document not found"))?;
    let mut fonts = TextEngine::new();
    let appearance = if dark {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    let diagnostics =
        processcraft_render::preflight_diagram(doc, &diagram, appearance, &mut fonts, "Noto Sans")
            .map_err(|e| error(3, e))?;
    if !diagnostics.is_empty() && !acknowledge_diagnostics {
        return Err(error(
            3,
            format!("export diagnostics require acknowledgement: {diagnostics:?}"),
        ));
    }
    let output = processcraft_render::export_diagram(
        doc,
        &diagram,
        ExportOptions {
            format: match format {
                crate::args::ImageFormat::Svg => ExportFormat::Svg,
                crate::args::ImageFormat::Png => ExportFormat::Png,
                crate::args::ImageFormat::Pdf => ExportFormat::Pdf,
            },
            appearance,
            scale,
            acknowledge_diagnostics,
        },
        &mut fonts,
        "Noto Sans",
    )
    .map_err(|e| error(3, e))?;
    write_export(
        out,
        &output.bytes,
        json!({"origin":output.origin,"logicalSize":output.logical_size,"pixelSize":output.pixel_size,"diagnostics":output.diagnostics}),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_io::{Operation, SaveReceipt};

    #[test]
    fn indeterminate_save_returns_machine_data_and_exit_one() {
        let operation_id = uuid::Uuid::new_v4();
        let target = PathBuf::from(r"C:\work\target.processcraft");
        let outcome = ProjectSaveOutcome::Indeterminate {
            attempted: SaveReceipt {
                operation_id,
                session_id: "session-1".into(),
                operation: Operation::Save,
                target: target.clone(),
                saved_content_id: [0; 32],
                disk_digest: [0; 32],
            },
            error: std::io::Error::other("uncertain"),
            stage: PathBuf::from(r"C:\work\stage"),
            backup: PathBuf::from(r"C:\work\backup"),
        };
        let (json, result) = save_result(outcome, "ignored", 0);
        let value = json.unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        let decoded: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded["operationId"], operation_id.to_string());
        assert_eq!(decoded["sessionId"], "session-1");
        assert_eq!(decoded["target"], target.to_string_lossy().as_ref());
        assert_eq!(decoded["reconciliationRequired"], true);
        assert_eq!(result.err().unwrap().code, 1);
    }
}
