//! Implementation of the one-shot CLI subcommands (and the shared [`Error`] type that carries
//! the exit status). Every mutation goes through `processcraft_core::commands::Session`.

use crate::args::{Command, FailOn, Query};
use processcraft_core::{
    commands::{CommandError, Envelope, Request, Session, command_descriptors},
    model::ProjectData,
};
use processcraft_io::{Access, FileGrant, FileSession, ProjectSaveOutcome, bpmn, native};
use serde_json::{Value, json};
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// Font family used for labels in rendered exports and preflight checks (bundled font).
pub const FONT_FAMILY: &str = "Noto Sans";

/// Process exit codes of the CLI; success is 0.
pub mod exit {
    /// File system, storage or other I/O failure.
    pub const IO: i32 = 1;
    /// Bad arguments or unusable request input (including BPMN XML where a project is required).
    pub const USAGE: i32 = 2;
    /// The data is invalid or refused: failed validation, unsupported project, blocked export,
    /// rejected command, missing document.
    pub const DATA: i32 = 3;
    /// A conflict: output exists without `--overwrite`, another writer, or a stale preview.
    pub const CONFLICT: i32 = 4;
}
/// A failed CLI command: the message goes to stderr and `code` (see [`exit`]) is the exit status.
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
        Ok(std::env::current_dir()
            .map_err(|e| error(exit::IO, e))?
            .join(p))
    }
}
fn grant(p: &Path, access: Access) -> Result<FileGrant, Error> {
    FileGrant::new(path(p)?, access).map_err(|e| {
        error(
            if e.kind() == std::io::ErrorKind::WouldBlock {
                exit::CONFLICT
            } else {
                exit::IO
            },
            format!("{}: {e}", p.display()),
        )
    })
}
fn read(p: &Path) -> Result<Vec<u8>, Error> {
    grant(p, Access::ReadOnly)?
        .read()
        .map_err(|e| error(exit::IO, format!("{}: {e}", p.display())))
}
/// Reads a file that must be a native project, refusing BPMN XML by content, not name.
fn read_native(p: &Path) -> Result<Vec<u8>, Error> {
    let bytes = read(p)?;
    if is_xml(&bytes) {
        return Err(error(
            exit::USAGE,
            format!(
                "{}: is BPMN XML, not a .processcraft project; convert it with import-bpmn first",
                p.display()
            ),
        ));
    }
    Ok(bytes)
}
fn emit(v: &Value) {
    println!("{}", serde_json::to_string(v).expect("serializable JSON"));
}
fn bpmn_path(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bpmn"))
}
/// Native projects are ZIP packages; anything starting like XML is treated as BPMN.
fn is_xml(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'<')
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn diagram_ids(document: &processcraft_core::model::BpmnDocument) -> Vec<Value> {
    document
        .diagrams
        .iter()
        .map(|d| json!(d.id.selector()))
        .collect()
}
/// `diagnostics` array, error and warning counts of one evaluation.
fn diagnostics_fields(
    diagnostics: &[processcraft_core::validation::Diagnostic],
) -> (Value, usize, usize) {
    use processcraft_core::validation::DiagnosticSeverity::Error;
    let errors = diagnostics.iter().filter(|d| d.severity == Error).count();
    (json!(diagnostics), errors, diagnostics.len() - errors)
}
fn parse_profile(
    bytes: &[u8],
) -> Result<processcraft_core::conventions::ConventionsProfile, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(error(exit::USAGE, "profile exceeds 4 MiB"));
    }
    processcraft_io::strict_json::check(bytes).map_err(|e| error(exit::USAGE, e))?;
    let profile: processcraft_core::conventions::ConventionsProfile =
        serde_json::from_slice(bytes).map_err(|e| error(exit::USAGE, e))?;
    profile
        .validate()
        .map_err(|e| error(exit::USAGE, format!("{e:?}")))?;
    Ok(profile)
}
/// Result of `validate`: diagnostics fields to merge into the output and the exit decision.
struct Checks {
    profile: Option<processcraft_core::conventions::ConventionsProfile>,
    fail_on: FailOn,
}
fn inspect(p: &Path, validate: bool) -> Result<(), Error> {
    inspect_with(p, validate, None)
}
fn inspect_with(p: &Path, validate: bool, checks: Option<Checks>) -> Result<(), Error> {
    let bytes = read(p)?;
    let mut diagnostics = None;
    let mut output = if is_xml(&bytes) {
        let imported = bpmn::import_bpmn(&bytes).map_err(|e| error(exit::DATA, e))?;
        let r = imported.report();
        let name = p
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Imported BPMN");
        let prepared = processcraft_io::interchange::prepare_import(&bytes, name)
            .map_err(|e| error(exit::DATA, e))?;
        let editable = prepared.report.editable;
        let diagrams = diagram_ids(&prepared.document);
        let mut limitations = prepared.report.limitations;
        limitations.extend(r.limitations.iter().map(|l| l.to_string()));
        if validate {
            let mut project = ProjectData::new(name.to_owned());
            project
                .documents
                .push(std::sync::Arc::new(prepared.document));
            project.resources = prepared.resources;
            project.validate().map_err(|e| error(exit::DATA, e))?;
            if let Some(checks) = &checks {
                diagnostics = Some(match &checks.profile {
                    Some(profile) => project.diagnostics_with(profile),
                    None => project.diagnostics(),
                });
            }
        }
        json!({"format":"bpmn", "valid":true, "validation":if validate {"structural import checks and core invariants of the prepared import"} else {"not requested"}, "capability":if editable {"editable"} else {"preservedSource"}, "semanticEditsAllowed":editable, "elementCount":r.element_count, "diagramCount":r.diagram_count, "diagrams":diagrams, "limitations":limitations})
    } else {
        let loaded = native::load_bytes(&bytes)
            .map_err(|e| error(exit::DATA, format!("invalid native project: {e}")))?;
        loaded
            .project
            .validate()
            .map_err(|e| error(exit::DATA, e))?;
        let session = Session::new(loaded.project)
            .map_err(|e| error(exit::DATA, format!("invalid project: {e:?}")))?;
        let s = session.snapshot();
        let documents: Vec<_> = s
            .project
            .documents
            .iter()
            .map(|d| json!({"id":d.id, "name":d.name, "diagrams":diagram_ids(d)}))
            .collect();
        if let Some(checks) = &checks {
            diagnostics = Some(match &checks.profile {
                Some(profile) => s.project.diagnostics_with(profile),
                None => s.project.diagnostics(),
            });
        }
        json!({"format":"processcraft", "valid":true, "validation":if validate {"native codec and core structural invariants"} else {"not requested"}, "projectId":s.project.id, "name":s.project.name, "documentCount":s.project.documents.len(), "documents":documents, "contentId":hex(&s.content_id), "requiresSaveAsOrBackup":loaded.requires_save_as_or_backup, "warnings":[]})
    };
    let (Some(diagnostics), Some(checks)) = (diagnostics, checks) else {
        emit(&output);
        return Ok(());
    };
    let (list, errors, warnings) = diagnostics_fields(&diagnostics);
    output["diagnostics"] = list;
    output["errorCount"] = json!(errors);
    output["warningCount"] = json!(warnings);
    emit(&output);
    if errors > 0 || (checks.fail_on == FailOn::Warning && warnings > 0) {
        return Err(error(
            exit::DATA,
            format!("{errors} error and {warnings} warning diagnostics"),
        ));
    }
    Ok(())
}
fn parse_batch(bytes: &[u8]) -> Result<Request, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(error(exit::USAGE, "batch exceeds 4 MiB"));
    }
    processcraft_io::strict_json::check(bytes).map_err(|e| error(exit::USAGE, e))?;
    let req: Request = serde_json::from_slice(bytes).map_err(|e| error(exit::USAGE, e))?;
    if !matches!(req, Request::Batch { .. }) {
        return Err(error(exit::USAGE, "--batch requires a batch request"));
    }
    Ok(req)
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
    if input.same_target(&output).map_err(|e| error(exit::IO, e))? {
        return Err(error(exit::USAGE, "input and output must differ"));
    }
    Ok(())
}
fn run(
    new: Option<String>,
    input: Option<PathBuf>,
    batch: PathBuf,
    out: PathBuf,
    overwrite: bool,
) -> Result<(), Error> {
    let batch_bytes = grant(&batch, Access::ReadOnly)?
        .read_limited(4 * 1024 * 1024)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                error(exit::USAGE, e)
            } else {
                error(exit::IO, e)
            }
        })?;
    let request = parse_batch(&batch_bytes)?;
    let out = path(&out)?;
    let (mut session, mut file, same) = if let Some(input) = input {
        let input = path(&input)?;
        let initial =
            native::load_bytes(&read_native(&input)?).map_err(|e| error(exit::DATA, e))?;
        let core =
            Session::new(initial.project).map_err(|e| error(exit::DATA, format!("{e:?}")))?;
        let (file, loaded) =
            FileSession::open(core.id(), grant(&input, Access::Write)?).map_err(|e| {
                error(
                    if e.kind() == std::io::ErrorKind::InvalidData {
                        exit::DATA
                    } else {
                        exit::IO
                    },
                    e,
                )
            })?;
        if loaded
            .project
            .content_identity()
            .map_err(|e| error(exit::DATA, e))?
            != core.snapshot().content_id
        {
            return Err(error(exit::CONFLICT, "input changed while opening"));
        }
        if !file.is_writable() {
            return Err(error(exit::CONFLICT, "input is owned by another writer"));
        }
        let same = same_path(&input, &out);
        if same && loaded.requires_save_as_or_backup {
            return Err(error(exit::CONFLICT, "migrated project requires Save As"));
        }
        (core, Some(file), same)
    } else {
        (
            Session::new(ProjectData::new(new.expect("clap requires name")))
                .map_err(|e| error(exit::USAGE, format!("{e:?}")))?,
            None,
            false,
        )
    };
    if !same && !overwrite && out.exists() {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
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
                CommandError::GenerationConflict | CommandError::SessionMismatch => exit::CONFLICT,
                CommandError::InvalidParams(_)
                | CommandError::CapabilityDenied
                | CommandError::NothingToUndo
                | CommandError::NothingToRedo => exit::DATA,
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
                    exit::CONFLICT
                } else {
                    exit::IO
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
                    exit::CONFLICT
                } else {
                    exit::IO
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
            Err(error(exit::IO, e)),
        ),
    }
}
pub fn execute(command: Command) -> Result<(), Error> {
    match command {
        Command::LayoutPreview {
            input,
            document_id,
            diagram_id,
            choreography_scope_shape,
            out,
            overwrite,
        } => layout_preview_file(
            &input,
            &document_id,
            &diagram_id,
            choreography_scope_shape.as_deref(),
            &out,
            overwrite,
        ),
        Command::LayoutApply {
            input,
            proposal,
            out,
            overwrite,
        } => layout_apply_file(&input, &proposal, &out, overwrite),
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
            overlay_attribute,
            overlay_mode,
        } => export_image_cli(ImageCli {
            input: &input,
            document_id: &document_id,
            diagram_id: &diagram_id,
            format,
            out: &out,
            scale,
            dark,
            overwrite,
            acknowledge_diagnostics,
            overlay_attribute: overlay_attribute.as_deref(),
            overlay_mode,
        }),
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
        Command::ExportDrawio {
            input,
            document_id,
            diagram_id,
            out,
            overwrite,
        } => export_drawio_cli(&input, &document_id, &diagram_id, &out, overwrite),
        Command::ExportRaci {
            input,
            document_id,
            process_id,
            out,
            overwrite,
        } => export_raci_cli(&input, &document_id, &process_id, &out, overwrite),
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
        Command::Validate {
            path,
            json: _,
            profile,
            fail_on,
        } => {
            let profile = profile
                .map(|file| read(&file).and_then(|bytes| parse_profile(&bytes)))
                .transpose()?;
            inspect_with(&path, true, Some(Checks { profile, fail_on }))
        }
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
            overwrite,
        } => run(new, input, batch, out, overwrite),
    }
}

pub(crate) fn bounded_page(
    items: impl Iterator<Item = Value>,
    total: usize,
    offset: usize,
    limit: usize,
) -> Result<Value, Error> {
    bounded_page_reserved(items, total, offset, limit, 0)
}
/// `bounded_page` with `reserved` bytes of the 3 MiB response already spent on
/// fields sent beside the page (the attribute definitions).
pub(crate) fn bounded_page_reserved(
    items: impl Iterator<Item = Value>,
    total: usize,
    offset: usize,
    limit: usize,
    reserved: usize,
) -> Result<Value, Error> {
    if !(1..=100).contains(&limit) {
        return Err(error(exit::USAGE, "limit must be 1..100"));
    }
    if reserved > 3 * 1024 * 1024 {
        return Err(error(exit::DATA, "query header exceeds response limit"));
    }
    let mut page = Vec::new();
    let mut bytes = reserved;
    for item in items.take(limit) {
        let size = serde_json::to_vec(&item)
            .map_err(|e| error(exit::DATA, e))?
            .len();
        if bytes.saturating_add(size) > 3 * 1024 * 1024 {
            if page.is_empty() {
                return Err(error(
                    exit::DATA,
                    "query item exceeds response limit; use a field projection query",
                ));
            }
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
        ids::{DictionaryEntryId, DocumentId, ElementSelector},
        queries,
    };
    let loaded = native::load_bytes(&read_native(path)?).map_err(|e| error(exit::DATA, e))?;
    let project = &loaded.project;
    let result = match query {
        Query::Model {
            document_id,
            offset,
            limit,
        } => {
            let id = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::document_fields_page(project, id, offset, limit)
                .map_err(|e| error(exit::DATA, e))?
                .ok_or_else(|| error(exit::DATA, "document not found"))?;
            let mut page = json!(rows);
            page["contentId"] = json!(
                project
                    .content_identity()
                    .map_err(|e| error(exit::DATA, e))?
            );
            page
        }
        Query::CheckpointFields {
            checkpoint_id,
            offset,
            limit,
        } => {
            let id = checkpoint_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::checkpoint_fields_page(project, id, offset, limit)
                .map_err(|e| error(exit::DATA, e))?
                .ok_or_else(|| error(exit::DATA, "checkpoint not found"))?;
            let mut page = json!(rows);
            page["contentId"] = json!(
                project
                    .content_identity()
                    .map_err(|e| error(exit::DATA, e))?
            );
            page
        }
        Query::Events {
            document_id,
            offset,
            limit,
        } => query_fields_page(project, &document_id, offset, limit, queries::event_fields)?,
        Query::Diagrams {
            document_id,
            offset,
            limit,
        } => query_fields_page(
            project,
            &document_id,
            offset,
            limit,
            queries::diagram_fields,
        )?,
        Query::Identities {
            document_id,
            offset,
            limit,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::identified_elements(project, document)
                .ok_or_else(|| error(exit::DATA, "document not found"))?;
            content_page(project, rows, offset, limit)?
        }
        Query::Data {
            document_id,
            offset,
            limit,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::data_fields(project, document)
                .ok_or_else(|| error(exit::DATA, "document not found"))?;
            content_page(project, rows, offset, limit)?
        }
        Query::Catalog {
            document_id,
            offset,
            limit,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::supporting_catalog_fields(project, document)
                .ok_or_else(|| error(exit::DATA, "document not found"))?;
            content_page(project, rows, offset, limit)?
        }
        Query::Comments { offset, limit } => {
            let total = project.comments.len();
            let values = project
                .comments
                .iter()
                .skip(offset)
                .take(limit)
                .map(|c| queries::comment_row(project, c));
            bounded_page(values, total, offset, limit)?
        }
        Query::Checkpoints { offset, limit } => {
            let total = project.checkpoints.len();
            let values = project
                .checkpoints
                .iter()
                .skip(offset)
                .take(limit)
                .map(|c| queries::checkpoint_row(project, c));
            bounded_page(values, total, offset, limit)?
        }
        Query::CompareCheckpoint { checkpoint_id } => {
            let id = checkpoint_id.parse().map_err(|e| error(exit::USAGE, e))?;
            json!({"comparison":queries::compare_checkpoint(project,id).ok_or_else(|| error(exit::DATA,"checkpoint not found"))?})
        }
        Query::Library { text } => json!({"documents": queries::library_search(project, &text)}),
        Query::Dictionary => json!({"entries": project.dictionary_entries}),
        Query::Documents => json!({"documents": queries::document_overview(project)}),
        Query::Conventions => json!({"conventions": project.conventions}),
        Query::Usages { entry_id } => {
            let id: DictionaryEntryId = entry_id.parse().map_err(|e| error(exit::USAGE, e))?;
            json!({"usages": queries::dictionary_usages(project, id)})
        }
        Query::Table {
            document_id,
            process_id,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let process: ElementSelector = process_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let rows = queries::activity_table(project, document, &process)
                .ok_or_else(|| error(exit::DATA, "process not found"))?;
            let rows: Vec<_> = rows.into_iter().map(queries::activity_row).collect();
            json!({"rows":rows})
        }
        Query::Raci {
            document_id,
            process_id,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let process: ElementSelector = process_id.parse().map_err(|e| error(exit::USAGE, e))?;
            json!({"raci": queries::raci_matrix(project, document, &process)
                .ok_or_else(|| error(exit::DATA,"process not found"))?})
        }
        Query::Diagnostics => {
            let (list, errors, warnings) = diagnostics_fields(&project.diagnostics());
            json!({"diagnostics": list, "errorCount": errors, "warningCount": warnings})
        }
        Query::Attributes { document_id } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let definitions = queries::attribute_definitions(project);
            let rows = queries::element_attributes(project, document)
                .ok_or_else(|| error(exit::DATA, "document not found"))?;
            json!({"definitions": definitions, "rows": rows})
        }
        Query::ReplaceOptions {
            document_id,
            element_id,
        } => {
            let document: DocumentId = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
            let element: ElementSelector = element_id.parse().map_err(|e| error(exit::USAGE, e))?;
            json!({"options": replace_options(project, document, &element)
                .ok_or_else(|| error(exit::DATA,"document not found"))?})
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

/// Replace targets for an element, each with its menu label (shared by CLI and MCP).
pub(crate) fn replace_options(
    project: &ProjectData,
    document: processcraft_core::ids::DocumentId,
    element: &processcraft_core::ids::ElementSelector,
) -> Option<Vec<Value>> {
    let d = project.documents.iter().find(|d| d.id == document)?;
    Some(
        processcraft_core::replace::replace_options(d, element)
            .into_iter()
            .map(|target| json!({"label": target.label(), "target": target}))
            .collect(),
    )
}

/// A bounded page of `rows` tagged with the project's content identity.
fn content_page(
    project: &ProjectData,
    rows: Vec<impl serde::Serialize>,
    offset: usize,
    limit: usize,
) -> Result<Value, Error> {
    let total = rows.len();
    let mut page = bounded_page(
        rows.into_iter()
            .skip(offset)
            .take(limit)
            .map(|row| json!(row)),
        total,
        offset,
        limit,
    )?;
    page["contentId"] = json!(
        project
            .content_identity()
            .map_err(|e| error(exit::DATA, e))?
    );
    Ok(page)
}
fn query_fields_page(
    project: &ProjectData,
    document_id: &str,
    offset: usize,
    limit: usize,
    projection: fn(
        &ProjectData,
        processcraft_core::ids::DocumentId,
    ) -> Option<Vec<processcraft_core::queries::ModelField>>,
) -> Result<Value, Error> {
    let id = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let rows = projection(project, id).ok_or_else(|| error(exit::DATA, "document not found"))?;
    content_page(project, rows, offset, limit)
}

fn layout_preview_file(
    input: &Path,
    document_id: &str,
    diagram_id: &str,
    choreography_scope_shape: Option<&str>,
    out: &Path,
    overwrite: bool,
) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    let project = native::load_bytes(&read_native(input)?)
        .map_err(|e| error(exit::DATA, e))?
        .project;
    let session = Session::new(project).map_err(|e| error(exit::DATA, format!("{e:?}")))?;
    let snapshot = session.snapshot();
    let id = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let diagram = diagram_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let doc = snapshot
        .project
        .documents
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| error(exit::DATA, "document not found"))?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let proposal = match choreography_scope_shape {
        Some(shape) => processcraft_core::layout::propose_choreography_scope_layout(
            doc,
            &diagram,
            &shape.parse().map_err(|e| error(exit::USAGE, e))?,
            &session.id(),
            snapshot.generation,
            &cancel,
        ),
        None => processcraft_core::layout::propose_layout(
            doc,
            &diagram,
            &session.id(),
            snapshot.generation,
            &cancel,
        ),
    }
    .map_err(|e| error(exit::DATA, e))?;
    let report = json!({"kind":"layoutPreview","warnings":proposal.warnings.len(),"metrics":proposal.metrics});
    let bytes = serde_json::to_vec(&FileLayoutProposal {
        base_content_id: snapshot.content_id,
        proposal,
    })
    .map_err(|e| error(exit::DATA, e))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(error(exit::DATA, "proposal exceeds 4 MiB"));
    }
    write_export(out, &bytes, report, !overwrite)
}
fn layout_apply_file(
    input: &Path,
    proposal: &Path,
    out: &Path,
    overwrite: bool,
) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    distinct_files(proposal, out)?;
    let bytes = grant(proposal, Access::ReadOnly)?
        .read_limited(4 * 1024 * 1024)
        .map_err(|e| error(exit::USAGE, e))?;
    processcraft_io::strict_json::check(&bytes).map_err(|e| error(exit::USAGE, e))?;
    let mut file: FileLayoutProposal =
        serde_json::from_slice(&bytes).map_err(|e| error(exit::USAGE, e))?;
    let loaded = native::load_bytes(&read_native(input)?).map_err(|e| error(exit::DATA, e))?;
    let mut session =
        Session::new(loaded.project).map_err(|e| error(exit::DATA, format!("{e:?}")))?;
    let snapshot = session.snapshot();
    if file.base_content_id != snapshot.content_id {
        return Err(error(exit::CONFLICT, "project changed since preview"));
    }
    let document = snapshot
        .project
        .documents
        .iter()
        .find(|d| d.id == file.proposal.document_id)
        .ok_or_else(|| error(exit::DATA, "document not found"))?;
    processcraft_core::layout::validate_proposal(document, &file.proposal)
        .map_err(|e| error(exit::DATA, e))?;
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
        .map_err(|e| error(exit::DATA, format!("{e:?}")))?;
    let id = session.id();
    let mut target = FileSession::create(id.clone(), grant(out, Access::Write)?)
        .map_err(|e| error(exit::IO, e))?;
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
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
    if bpmn_path(out) {
        return Err(error(
            exit::USAGE,
            format!(
                "{}: import-bpmn writes a .processcraft package (ZIP), not BPMN XML; choose a .processcraft output",
                out.display()
            ),
        ));
    }
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    let bytes = read(input)?;
    let name = input
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Imported BPMN");
    let mut prepared = processcraft_io::interchange::prepare_import(&bytes, name)
        .map_err(|e| error(exit::DATA, e))?;
    let editable = prepared.report.editable;
    let mut limitations = prepared.report.limitations;
    let document_id = prepared.document.id;
    let mut project = ProjectData::new(name.to_owned());
    // The plan sees the fresh project, so every imported definition is created;
    // its mutations run through the session like any other change.
    let plan = processcraft_io::interchange::plan_attribute_import(
        &project,
        &mut prepared.document,
        &prepared.attributes,
    );
    project
        .documents
        .push(std::sync::Arc::new(prepared.document));
    project.resources = prepared.resources;
    let mut session = Session::new(project).map_err(|e| error(exit::DATA, format!("{e:?}")))?;
    if !plan.mutations.is_empty() {
        let envelope = processcraft_core::commands::Envelope {
            request_id: "import-attributes".into(),
            session_id: session.id(),
            expected_generation: None,
        };
        session
            .apply(
                envelope,
                processcraft_core::commands::Request::Batch {
                    mutations: plan.mutations,
                },
            )
            .map_err(|e| error(exit::DATA, format!("attribute import failed: {e:?}")))?;
    }
    limitations.extend(plan.notes);
    session.mark_unsaved();
    let id = session.id();
    let mut file = FileSession::create(id.clone(), grant(out, Access::Write)?)
        .map_err(|e| error(exit::IO, e))?;
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
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    let loaded = native::load_bytes(&read_native(input)?).map_err(|e| error(exit::DATA, e))?;
    let id = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let result = processcraft_io::interchange::export_bpmn(&loaded.project, id, acknowledge)
        .map_err(|e| {
            error(
                exit::DATA,
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

fn export_drawio_cli(
    input: &Path,
    document_id: &str,
    diagram_id: &str,
    out: &Path,
    overwrite: bool,
) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    let loaded = native::load_bytes(&read_native(input)?).map_err(|e| error(exit::DATA, e))?;
    let id: processcraft_core::ids::DocumentId =
        document_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let diagram = diagram_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let document = loaded
        .project
        .documents
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| error(exit::USAGE, "document not found"))?;
    let result = processcraft_io::drawio::export_drawio(document, &diagram)
        .map_err(|e| error(exit::DATA, e))?;
    write_export(
        out,
        &result.bytes,
        json!({"approximated":result.approximated}),
        false,
    )
}

fn export_raci_cli(
    input: &Path,
    document_id: &str,
    process_id: &str,
    out: &Path,
    overwrite: bool,
) -> Result<(), Error> {
    if out.exists() && !overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(input, out)?;
    let loaded = native::load_bytes(&read_native(input)?).map_err(|e| error(exit::DATA, e))?;
    let document = document_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let process = process_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let matrix = processcraft_core::queries::raci_matrix(&loaded.project, document, &process)
        .ok_or_else(|| error(exit::DATA, "process not found"))?;
    write_export(
        out,
        processcraft_core::queries::raci_csv(&matrix).as_bytes(),
        json!({"columns":matrix.columns.len(),"rows":matrix.rows.len()}),
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
    let mut writer = match WriterSession::open(&grant).map_err(|e| error(exit::IO, e))? {
        OpenSession::Writable(w) => w,
        OpenSession::ReadOnly => return Err(error(exit::CONFLICT, "output has another writer")),
    };
    if must_be_new && out.exists() {
        return Err(error(exit::CONFLICT, "output exists"));
    }
    match writer.save(bytes) {
        SaveOutcome::Committed { warning } => {
            emit(
                &json!({"ok":true,"status":"committed","operationId":operation,"diskDigest":digest,"report":report,"warning":warning.map(|e|e.to_string())}),
            );
            Ok(())
        }
        SaveOutcome::NotCommitted { error: e } => Err(error(exit::IO, e)),
        SaveOutcome::Indeterminate {
            error: e,
            stage,
            backup,
        } => {
            emit(
                &json!({"ok":false,"status":"indeterminate","operationId":operation,"sessionId":session,"target":out,"diskDigest":digest,"stage":stage,"backup":backup,"reconciliationRequired":true,"reconciliation":"Inspect target and retained artifacts before retrying"}),
            );
            Err(error(exit::IO, e))
        }
    }
}

struct ImageCli<'a> {
    input: &'a Path,
    document_id: &'a str,
    diagram_id: &'a str,
    format: crate::args::ImageFormat,
    out: &'a Path,
    scale: f32,
    dark: bool,
    overwrite: bool,
    acknowledge_diagnostics: bool,
    overlay_attribute: Option<&'a str>,
    overlay_mode: Option<crate::args::OverlayModeArg>,
}

fn export_image_cli(o: ImageCli) -> Result<(), Error> {
    use crate::export::{ExportFailure, ImageExport, render_export};
    use processcraft_render::ExportFormat;
    // Mirrors the renderer's limit so a bad scale is reported before diagnostics.
    if !o.scale.is_finite() || !processcraft_render::ZOOM_RANGE.contains(&o.scale) {
        return Err(error(exit::USAGE, "--scale must be finite within 0.125..8"));
    }
    if o.out.exists() && !o.overwrite {
        return Err(error(
            exit::CONFLICT,
            "output exists; --overwrite is required",
        ));
    }
    distinct_files(o.input, o.out)?;
    let loaded = native::load_bytes(&read_native(o.input)?).map_err(|e| error(exit::DATA, e))?;
    let diagram = o.diagram_id.parse().map_err(|e| error(exit::USAGE, e))?;
    let options = ImageExport {
        document_id: o.document_id.parse().map_err(|e| error(exit::USAGE, e))?,
        diagram_id: Some(&diagram),
        format: match o.format {
            crate::args::ImageFormat::Svg => ExportFormat::Svg,
            crate::args::ImageFormat::Png => ExportFormat::Png,
            crate::args::ImageFormat::Pdf => ExportFormat::Pdf,
        },
        dark: o.dark,
        scale: o.scale,
        acknowledge_diagnostics: o.acknowledge_diagnostics,
        overlay: o.overlay_attribute.map(|attribute| {
            let definition = attribute
                .parse()
                .map_err(|e: processcraft_core::ids::InvalidId| e.to_string())?;
            Ok((definition, o.overlay_mode.map(Into::into)))
        }),
    };
    let (bytes, report) =
        render_export(&loaded.project, options).map_err(|failure| match failure {
            ExportFailure::DocumentNotFound => error(exit::DATA, "document not found"),
            // Unreachable: the CLI always passes a diagram.
            ExportFailure::DiagramRequired => error(exit::USAGE, "--diagram-id is required"),
            ExportFailure::Overlay(e) => error(exit::USAGE, e),
            ExportFailure::Render(e) => error(exit::DATA, e),
            ExportFailure::AcknowledgementRequired(diagnostics) => error(
                exit::DATA,
                format!(
                    "export diagnostics require acknowledgement (pass --acknowledge-diagnostics): {diagnostics:?}"
                ),
            ),
        })?;
    write_export(o.out, &bytes, report, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_io::{Operation, SaveReceipt};

    #[test]
    fn oversized_query_item_refuses_instead_of_returning_nonadvancing_page() {
        let result = bounded_page(
            std::iter::once(json!("x".repeat(3 * 1024 * 1024))),
            1,
            0,
            100,
        );
        assert!(
            result
                .err()
                .unwrap()
                .message
                .contains("exceeds response limit")
        );
    }

    #[test]
    fn byte_limited_page_advances_and_next_item_can_be_read() {
        let items = vec![
            json!("x".repeat(2 * 1024 * 1024)),
            json!("y".repeat(2 * 1024 * 1024)),
        ];
        let first = bounded_page(items.clone().into_iter(), 2, 0, 100)
            .ok()
            .unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 1);
        assert_eq!(first["nextOffset"], 1);
        let second = bounded_page(items.into_iter().skip(1), 2, 1, 100)
            .ok()
            .unwrap();
        assert_eq!(second["items"].as_array().unwrap().len(), 1);
        assert!(second["nextOffset"].is_null());
    }

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
