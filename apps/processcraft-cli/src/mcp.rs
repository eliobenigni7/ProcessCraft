//! Explicit, standalone MCP session. No server is started by other commands.
use processcraft_core::{
    commands::{Envelope, Request, Session},
    model::ProjectData,
};
use processcraft_io::{
    Access, FileSession, OpenSession, ProjectSaveOutcome, SaveOutcome, WriterSession,
    automation_ledger::{Admission, RequestLedger},
    root_grants::RootGrant,
};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
    transport::async_rw::AsyncRwTransport,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

struct State {
    session: Session,
    ledger: RequestLedger,
    reconcile_ledger: RequestLedger,
    _read: Vec<RootGrant>,
    write: Vec<RootGrant>,
    file: Option<FileSession>,
    pending: Option<PendingEffect>,
    settled: Option<(uuid::Uuid, Value)>,
}
enum PendingEffect {
    Native {
        receipt: processcraft_io::SaveReceipt,
        stage: PathBuf,
        backup: PathBuf,
    },
    Export {
        operation_id: uuid::Uuid,
        session_id: String,
        target: PathBuf,
        disk_digest: [u8; 32],
        content_id: [u8; 32],
        stage: PathBuf,
        backup: PathBuf,
        writer: WriterSession,
        bytes: Vec<u8>,
    },
}
impl PendingEffect {
    fn blocked(&self) -> Value {
        match self {
            Self::Native {
                receipt,
                stage,
                backup,
            } => {
                json!({"ok":false,"status":"reconciliationRequired","sessionId":receipt.session_id,"target":receipt.target,"operationId":receipt.operation_id,"contentId":receipt.saved_content_id,"diskDigest":receipt.disk_digest,"stage":stage,"backup":backup,"reconciliationRequired":true})
            }
            Self::Export {
                operation_id,
                session_id,
                target,
                disk_digest,
                content_id,
                stage,
                backup,
                ..
            } => {
                json!({"ok":false,"status":"reconciliationRequired","sessionId":session_id,"target":target,"operationId":operation_id,"contentId":content_id,"diskDigest":disk_digest,"stage":stage,"backup":backup,"reconciliationRequired":true})
            }
        }
    }
}
#[cfg_attr(not(windows), allow(dead_code))]
struct Attachment {
    endpoint: String,
    token: String,
    session: String,
}
#[derive(Clone)]
struct Server {
    state: Option<Arc<Mutex<State>>>,
    attach: Option<Arc<Attachment>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Apply {
    session_id: String,
    request_id: String,
    expected_generation: u64,
    request: Request,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Save {
    session_id: String,
    request_id: String,
    expected_generation: u64,
    path: PathBuf,
    overwrite: bool,
}
#[derive(Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reconcile {
    session_id: String,
    request_id: String,
    expected_generation: u64,
    #[schemars(with = "String")]
    operation_id: uuid::Uuid,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Export {
    session_id: String,
    request_id: String,
    expected_generation: u64,
    path: PathBuf,
    document_id: processcraft_core::ids::DocumentId,
    overwrite: bool,
    acknowledge_native_fields: bool,
    #[serde(default)]
    format: ExportKind,
    #[serde(default)]
    diagram_id: Option<processcraft_core::ids::ElementSelector>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    dark: bool,
    #[serde(default)]
    acknowledge_diagnostics: bool,
}
#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
enum ExportKind {
    #[default]
    Bpmn,
    Svg,
    Png,
    Pdf,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Query {
    Model {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    CheckpointFields {
        #[serde(rename = "checkpointId")]
        checkpoint_id: processcraft_core::ids::CheckpointId,
        offset: usize,
        limit: usize,
    },
    Events {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Diagrams {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Identities {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Data {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Catalog {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Comments {
        offset: usize,
        limit: usize,
    },
    Checkpoints {
        offset: usize,
        limit: usize,
    },
    CompareCheckpoint {
        #[serde(rename = "checkpointId")]
        checkpoint_id: processcraft_core::ids::CheckpointId,
        offset: usize,
        limit: usize,
    },
    LayoutPreview {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "expectedGeneration")]
        expected_generation: u64,
        #[serde(rename = "documentId")]
        document_id: processcraft_core::ids::DocumentId,
        #[serde(rename = "diagramId")]
        diagram_id: processcraft_core::ids::ElementSelector,
        #[serde(default, rename = "choreographyScopeShape")]
        choreography_scope_shape: Option<processcraft_core::ids::ElementSelector>,
    },
    Library {
        text: String,
        offset: usize,
        limit: usize,
    },
    Dictionary {
        offset: usize,
        limit: usize,
    },
    Diagnostics {
        offset: usize,
        limit: usize,
    },
    Usages {
        entry_id: processcraft_core::ids::DictionaryEntryId,
        offset: usize,
        limit: usize,
    },
    Table {
        document_id: processcraft_core::ids::DocumentId,
        process_id: processcraft_core::ids::ElementSelector,
        offset: usize,
        limit: usize,
    },
    Attributes {
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    ReplaceOptions {
        document_id: processcraft_core::ids::DocumentId,
        element_id: processcraft_core::ids::ElementSelector,
        offset: usize,
        limit: usize,
    },
}

fn schema(value: Value) -> Arc<serde_json::Map<String, Value>> {
    Arc::new(value.as_object().unwrap().clone())
}
fn tools() -> Vec<Tool> {
    let empty = schema(json!({"type":"object","properties":{},"additionalProperties":false}));
    let mut request = processcraft_core::commands::request_schema();
    let defs = request
        .as_object_mut()
        .and_then(|o| o.remove("$defs"))
        .unwrap_or(json!({}));
    let apply = schema(
        json!({"$defs":defs,"type":"object","properties":{"sessionId":{"type":"string"},"requestId":{"type":"string"},"expectedGeneration":{"type":"integer","minimum":0},"request":request},"required":["sessionId","requestId","expectedGeneration","request"],"additionalProperties":false}),
    );
    let save = schema(
        json!({"type":"object","properties":{"sessionId":{"type":"string"},"requestId":{"type":"string"},"expectedGeneration":{"type":"integer","minimum":0},"path":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["sessionId","requestId","expectedGeneration","path","overwrite"],"additionalProperties":false}),
    );
    // MCP requires an object-typed inputSchema; the generated tagged enum is a bare oneOf.
    let mut query = serde_json::to_value(schemars::schema_for!(Query)).expect("query schema");
    query["type"] = json!("object");
    let query = schema(query);
    let export =
        schema(serde_json::to_value(schemars::schema_for!(Export)).expect("export schema"));
    let reconcile =
        schema(serde_json::to_value(schemars::schema_for!(Reconcile)).expect("reconcile schema"));
    vec![
        Tool::new(
            "inspect",
            "Inspect current session identity and project summary",
            empty.clone(),
        )
        .annotate(ToolAnnotations::new().read_only(true)),
        Tool::new("validate", "Validate current project", empty)
            .annotate(ToolAnnotations::new().read_only(true)),
        Tool::new(
            "query",
            "Read paginated catalog fields, library, dictionary, usages, replace options, or review data",
            query,
        )
        .annotate(ToolAnnotations::new().read_only(true)),
        Tool::new("apply", "Apply one atomic request", apply)
            .annotate(ToolAnnotations::new().read_only(false)),
        Tool::new("save", "Save to an explicitly authorized write root", save)
            .annotate(ToolAnnotations::new().read_only(false)),
        Tool::new(
            "export",
            "Export a qualified BPMN document to a write root",
            export,
        )
        .annotate(ToolAnnotations::new().read_only(false)),
        Tool::new(
            "reconcile",
            "Reconcile an indeterminate file operation",
            reconcile,
        )
        .annotate(ToolAnnotations::new().read_only(false)),
    ]
}
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            rmcp::model::Implementation::new("processcraft", env!("CARGO_PKG_VERSION")),
        )
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }
    async fn call_tool(
        &self,
        params: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(params.arguments.unwrap_or_default());
        if let Some(attachment) = &self.attach {
            return forward_attachment(attachment, params.name.as_ref(), args);
        }
        let mut state = self
            .state
            .as_ref()
            .expect("standalone state")
            .lock()
            .map_err(|_| ErrorData::internal_error("session lock poisoned", None))?;
        let response = match params.name.as_ref() {
            "inspect" if args == json!({}) => {
                let s = state.session.snapshot();
                json!({"sessionId":state.session.id(),"generation":s.generation,"contentId":s.content_id,"projectId":s.project.id,"name":s.project.name,"documents":s.project.documents.len()})
            }
            "validate" if args == json!({}) => {
                let s = state.session.snapshot();
                match s.project.validate() {
                    Ok(()) => {
                        let diagnostics = s.project.diagnostics();
                        let errors = diagnostics
                            .iter()
                            .filter(|d| d.severity == processcraft_core::DiagnosticSeverity::Error)
                            .count();
                        json!({"valid":true,"errorCount":errors,"warningCount":diagnostics.len() - errors})
                    }
                    Err(e) => json!({"valid":false,"error":e.to_string()}),
                }
            }
            "query" => {
                let query: Query = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let snapshot = state.session.snapshot();
                let catalog_query = matches!(
                    &query,
                    Query::Model { .. }
                        | Query::CheckpointFields { .. }
                        | Query::Catalog { .. }
                        | Query::Data { .. }
                        | Query::Identities { .. }
                        | Query::Events { .. }
                        | Query::Diagrams { .. }
                );
                let (items, offset, limit) = match query {
                    Query::Model { document_id, offset, limit } => {
                        let page = processcraft_core::queries::document_fields_page(&snapshot.project, document_id, offset, limit)
                            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?;
                        let mut result = json!(page);
                        result["generation"] = json!(snapshot.generation);
                        result["contentId"] = json!(snapshot.content_id);
                        return Ok(CallToolResult::structured(result).into());
                    },
                    Query::CheckpointFields { checkpoint_id, offset, limit } => {
                        let page = processcraft_core::queries::checkpoint_fields_page(&snapshot.project, checkpoint_id, offset, limit)
                            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
                            .ok_or_else(|| ErrorData::invalid_params("checkpoint not found", None))?;
                        let mut result = json!(page);
                        result["generation"] = json!(snapshot.generation);
                        result["contentId"] = json!(snapshot.content_id);
                        return Ok(CallToolResult::structured(result).into());
                    },
                    Query::Events { document_id, offset, limit } => (
                        processcraft_core::queries::event_fields(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?
                            .into_iter().map(|row| json!(row)).collect(), offset, limit),
                    Query::Diagrams { document_id, offset, limit } => (
                        processcraft_core::queries::diagram_fields(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?
                            .into_iter().map(|row| json!(row)).collect(), offset, limit),
                    Query::Identities { document_id, offset, limit } => (
                        processcraft_core::queries::identified_elements(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?
                            .into_iter().map(|row| json!(row)).collect(), offset, limit),
                    Query::Data { document_id, offset, limit } => (
                        processcraft_core::queries::data_fields(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?
                            .into_iter().map(|row| json!(row)).collect(), offset, limit),
                    Query::Catalog { document_id, offset, limit } => (
                        processcraft_core::queries::supporting_catalog_fields(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?
                            .into_iter().map(|row| json!(row)).collect(), offset, limit),
                    Query::Comments { offset, limit } => (snapshot.project.comments.iter().map(|c| json!({"comment":c,"orphan":processcraft_core::queries::comment_is_orphan(&snapshot.project,c)})).collect(), offset, limit),
                    Query::Checkpoints { offset, limit } => (snapshot.project.checkpoints.iter().map(|c| json!({"id":c.id,"name":c.name,"createdUnixMs":c.created_unix_ms,"reference":snapshot.project.reference_checkpoint == Some(c.id)})).collect(), offset, limit),
                    Query::CompareCheckpoint { checkpoint_id, offset, limit } => (vec![json!(processcraft_core::queries::compare_checkpoint(&snapshot.project, checkpoint_id).ok_or_else(|| ErrorData::invalid_params("checkpoint not found", None))?)], offset, limit),
                    Query::LayoutPreview { session_id, expected_generation, document_id, diagram_id, choreography_scope_shape } => {
                        if session_id != state.session.id() || expected_generation != snapshot.generation { return Err(ErrorData::invalid_params("session or generation mismatch", None)); }
                        let doc = snapshot.project.documents.iter().find(|d| d.id == document_id).ok_or_else(|| ErrorData::invalid_params("document not found",None))?;
                        let cancel = std::sync::atomic::AtomicBool::new(false);
                        let proposal = match choreography_scope_shape {
                            Some(shape) => processcraft_core::layout::propose_choreography_scope_layout(doc, &diagram_id, &shape, &session_id, expected_generation, &cancel),
                            None => processcraft_core::layout::propose_layout(doc, &diagram_id, &session_id, expected_generation, &cancel),
                        }.map_err(|e| ErrorData::invalid_params(e.to_string(),None))?;
                        let value = json!({"proposal":proposal});
                        if serde_json::to_vec(&value).map_or(true, |v| v.len() > 3 * 1024 * 1024) { return Err(ErrorData::invalid_params("proposal exceeds response limit",None)); }
                        return Ok(CallToolResult::structured(value).into());
                    }
                    Query::Library {
                        text,
                        offset,
                        limit,
                    } => (
                        processcraft_core::queries::library_search(&snapshot.project, &text)
                            .into_iter()
                            .map(|v| json!(v))
                            .collect::<Vec<_>>(),
                        offset,
                        limit,
                    ),
                    Query::Diagnostics { offset, limit } => (
                        snapshot.project.diagnostics().iter().map(|d| json!(d)).collect(),
                        offset,
                        limit,
                    ),
                    Query::Dictionary { offset, limit } => (
                        snapshot
                            .project
                            .dictionary_entries
                            .iter()
                            .map(|e| json!({"id":e.id,"kind":e.kind,"name":e.name}))
                            .collect(),
                        offset,
                        limit,
                    ),
                    Query::Usages {
                        entry_id,
                        offset,
                        limit,
                    } => (
                        processcraft_core::queries::dictionary_usages(&snapshot.project, entry_id)
                            .into_iter()
                            .map(|v| json!(v))
                            .collect(),
                        offset,
                        limit,
                    ),
                    Query::Table { document_id, process_id, offset, limit } => (
                        processcraft_core::queries::activity_table(&snapshot.project, document_id, &process_id)
                            .ok_or_else(|| ErrorData::invalid_params("process not found", None))?
                            .into_iter().map(|row| json!({"key":row.key,"name":row.name,"lane":row.lane,"role":row.role,"systems":row.systems,"documents":row.documents,"attributes":row.attributes,"outgoing":row.outgoing.into_iter().map(|f| json!({"id":f.id,"target":f.target,"condition":f.condition})).collect::<Vec<_>>() })).collect(), offset, limit),
                    Query::Attributes { document_id, offset, limit } => {
                        let definitions =
                            processcraft_core::queries::attribute_definitions(&snapshot.project);
                        let rows = processcraft_core::queries::element_attributes(&snapshot.project, document_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?;
                        let total = rows.len();
                        let definitions = json!(definitions);
                        let reserved = serde_json::to_vec(&definitions)
                            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
                            .len();
                        let mut result = crate::run::bounded_page_reserved(
                            rows.into_iter().skip(offset).map(|row| json!(row)),
                            total,
                            offset,
                            limit,
                            reserved,
                        )
                        .map_err(|error| ErrorData::invalid_params(error.message, None))?;
                        result["definitions"] = definitions;
                        return Ok(CallToolResult::structured(result).into());
                    }
                    Query::ReplaceOptions { document_id, element_id, offset, limit } => (
                        crate::run::replace_options(&snapshot.project, document_id, &element_id)
                            .ok_or_else(|| ErrorData::invalid_params("document not found", None))?,
                        offset, limit),
                };
                let total = items.len();
                let mut result =
                    crate::run::bounded_page(items.into_iter().skip(offset), total, offset, limit)
                        .map_err(|error| ErrorData::invalid_params(error.message, None))?;
                if catalog_query {
                    result["generation"] = json!(snapshot.generation);
                    result["contentId"] = json!(snapshot.content_id);
                }
                result
            }
            "apply" => {
                let parsed: Apply = serde_json::from_value(args.clone())
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let bytes =
                    serde_json::to_vec(&(params.name.as_ref(), &args)).expect("JSON serializes");
                match admit_write(&mut state, &parsed.session_id, &parsed.request_id, &bytes) {
                    Admission::Completed(v) => v,
                    Admission::New => {
                        let result = state.session.apply(
                            Envelope {
                                request_id: parsed.request_id.clone(),
                                session_id: parsed.session_id,
                                expected_generation: Some(parsed.expected_generation),
                            },
                            parsed.request,
                        );
                        let value = match result {
                            Ok(r) => {
                                let mut value =
                                    serde_json::to_value(r).expect("core change set serializes");
                                value["ok"] = json!(true);
                                value
                            }
                            Err(e) => json!({"ok":false,"error":format!("{e:?}")}),
                        };
                        state.ledger.complete(&parsed.request_id, value.clone());
                        value
                    }
                    other => {
                        json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true})
                    }
                }
            }
            "save" => {
                let parsed: Save = serde_json::from_value(args.clone())
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let bytes =
                    serde_json::to_vec(&(params.name.as_ref(), &args)).expect("JSON serializes");
                match admit_write(&mut state, &parsed.session_id, &parsed.request_id, &bytes) {
                    Admission::Completed(v) => v,
                    Admission::New => {
                        let value = if !state.reconcile_ledger.has_capacity() {
                            json!({"ok":false,"status":"manualInvestigationRequired","manualInvestigationRequired":true})
                        } else if parsed.expected_generation != state.session.snapshot().generation
                        {
                            json!({"ok":false,"status":"generationConflict"})
                        } else if let Some(pending) = &state.pending {
                            pending.blocked()
                        } else {
                            let same = state
                                .file
                                .as_ref()
                                .is_some_and(|f| f.target() == parsed.path);
                            // Authorize before touching the path, so existence never leaks
                            // outside the write roots.
                            let grant = state
                                .write
                                .iter()
                                .find_map(|root| root.grant(&parsed.path, Access::Write).ok());
                            if grant.is_some() && !same && parsed.path.exists() && !parsed.overwrite
                            {
                                json!({"ok":false,"status":"overwriteRequiresAuthorization"})
                            } else if let Some(grant) = grant {
                                let project = state.session.snapshot().project;
                                let outcome = if let Some(file) = state.file.as_mut() {
                                    if same {
                                        file.save(&project)
                                    } else {
                                        file.save_as(grant, &project)
                                    }
                                } else {
                                    match FileSession::create(parsed.session_id.clone(), grant) {
                                        Ok(mut file) => {
                                            let outcome = file.save(&project);
                                            state.file = Some(file);
                                            outcome
                                        }
                                        Err(e) => ProjectSaveOutcome::NotCommitted { error: e },
                                    }
                                };
                                if let ProjectSaveOutcome::Indeterminate {
                                    attempted,
                                    stage,
                                    backup,
                                    ..
                                } = &outcome
                                {
                                    state.pending = Some(PendingEffect::Native {
                                        receipt: attempted.clone(),
                                        stage: stage.clone(),
                                        backup: backup.clone(),
                                    });
                                }
                                if let ProjectSaveOutcome::Committed { receipt, .. } = &outcome {
                                    state.session.mark_saved(receipt.saved_content_id);
                                }
                                save_outcome(outcome)
                            } else {
                                json!({"ok":false,"status":"pathNotAuthorized"})
                            }
                        };
                        state.ledger.complete(&parsed.request_id, value.clone());
                        value
                    }
                    other => {
                        json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true})
                    }
                }
            }
            "export" => {
                let parsed: Export = serde_json::from_value(args.clone())
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let bytes =
                    serde_json::to_vec(&(params.name.as_ref(), &args)).expect("JSON serializes");
                match admit_write(&mut state, &parsed.session_id, &parsed.request_id, &bytes) {
                    Admission::Completed(v) => v,
                    Admission::New => {
                        let value = if !state.reconcile_ledger.has_capacity() {
                            json!({"ok":false,"status":"manualInvestigationRequired","manualInvestigationRequired":true})
                        } else {
                            let snapshot = state.session.snapshot();
                            let grant = state
                                .write
                                .iter()
                                .find_map(|root| root.grant(&parsed.path, Access::Write).ok());
                            if parsed.expected_generation != snapshot.generation {
                                json!({"ok":false,"status":"generationConflict"})
                            } else if let Some(pending) = &state.pending {
                                pending.blocked()
                            } else if grant.is_some() && parsed.path.exists() && !parsed.overwrite {
                                json!({"ok":false,"status":"overwriteRequiresAuthorization"})
                            } else if let Some(grant) = grant {
                                if state
                                    .file
                                    .as_ref()
                                    .is_some_and(|file| file.same_target(&grant).unwrap_or(true))
                                {
                                    json!({"ok":false,"status":"exportTargetsActiveNativeFile"})
                                } else {
                                    match prepare_export(&snapshot.project, &parsed) {
                                        Err(e) => e,
                                        Ok(export) => {
                                            let operation = uuid::Uuid::new_v4();
                                            let disk_digest: [u8; 32] =
                                                Sha256::digest(&export.0).into();
                                            match WriterSession::open(&grant) {
                                                Ok(OpenSession::Writable(mut writer)) => {
                                                    match writer.save(&export.0) {
                                                        SaveOutcome::Committed { warning } => {
                                                            json!({"ok":true,"status":"committed","operationId":operation,"contentId":snapshot.content_id,"diskDigest":disk_digest,"warning":warning.map(|e|e.to_string()),"report":export.1})
                                                        }
                                                        SaveOutcome::NotCommitted { error } => {
                                                            json!({"ok":false,"status":"notCommitted","operationId":operation,"error":error.to_string()})
                                                        }
                                                        SaveOutcome::Indeterminate {
                                                            error,
                                                            stage,
                                                            backup,
                                                        } => {
                                                            let result = json!({"ok":false,"status":"indeterminate","operationId":operation,"sessionId":parsed.session_id,"target":parsed.path,"contentId":snapshot.content_id,"diskDigest":disk_digest,"error":error.to_string(),"stage":stage,"backup":backup,"reconciliationRequired":true});
                                                            state.pending =
                                                                Some(PendingEffect::Export {
                                                                    operation_id: operation,
                                                                    session_id: parsed
                                                                        .session_id
                                                                        .clone(),
                                                                    target: parsed.path.clone(),
                                                                    disk_digest,
                                                                    content_id: snapshot.content_id,
                                                                    stage,
                                                                    backup,
                                                                    writer,
                                                                    bytes: export.0,
                                                                });
                                                            result
                                                        }
                                                    }
                                                }
                                                Ok(OpenSession::ReadOnly) => {
                                                    json!({"ok":false,"status":"notCommitted","error":"writer unavailable"})
                                                }
                                                Err(e) => {
                                                    json!({"ok":false,"status":"notCommitted","error":e.to_string()})
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                json!({"ok":false,"status":"pathNotAuthorized"})
                            }
                        };
                        state.ledger.complete(&parsed.request_id, value.clone());
                        value
                    }
                    other => {
                        json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true})
                    }
                }
            }
            "reconcile" => {
                let parsed: Reconcile = serde_json::from_value(args.clone())
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                handle_reconcile(&mut state, &parsed)
            }
            _ => {
                return Err(ErrorData::invalid_params(
                    "unknown tool or invalid arguments",
                    None,
                ));
            }
        };
        if serde_json::to_vec(&response).map_or(true, |v| v.len() > 1024 * 1024) {
            return Ok(CallToolResult::structured_error(
                json!({"ok":false,"status":"responseTooLarge","message":"Use a smaller page"}),
            )
            .into());
        }
        Ok(CallToolResult::structured(response).into())
    }
}

fn prepare_export(project: &ProjectData, args: &Export) -> Result<(Vec<u8>, Value), Value> {
    use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine};
    if matches!(args.format, ExportKind::Bpmn) {
        let result = processcraft_io::interchange::export_bpmn(project, args.document_id, args.acknowledge_native_fields)
            .map_err(|e| json!({"ok":false,"status":"exportBlocked","blocked":e.report.blocked,"omitted":e.report.omitted}))?;
        return Ok((
            result.bytes,
            json!({"retained":result.report.retained,"transformed":result.report.transformed,"omitted":result.report.omitted}),
        ));
    }
    let document = project
        .documents
        .iter()
        .find(|d| d.id == args.document_id)
        .ok_or_else(|| json!({"ok":false,"status":"documentNotFound"}))?;
    let diagram = args
        .diagram_id
        .as_ref()
        .ok_or_else(|| json!({"ok":false,"status":"diagramRequired"}))?;
    let appearance = if args.dark {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    let mut fonts = TextEngine::new();
    let diagnostics = processcraft_render::preflight_diagram(
        document,
        diagram,
        appearance,
        &mut fonts,
        "Noto Sans",
    )
    .map_err(|e| json!({"ok":false,"status":"exportBlocked","error":e.to_string()}))?;
    if !args.acknowledge_diagnostics && !diagnostics.is_empty() {
        return Err(
            json!({"ok":false,"status":"acknowledgementRequired","diagnostics":diagnostics}),
        );
    }
    let output = processcraft_render::export_diagram(
        document,
        diagram,
        ExportOptions {
            format: if matches!(args.format, ExportKind::Svg) {
                ExportFormat::Svg
            } else if matches!(args.format, ExportKind::Pdf) {
                ExportFormat::Pdf
            } else {
                ExportFormat::Png
            },
            appearance,
            scale: args.scale.unwrap_or(1.0),
            acknowledge_diagnostics: args.acknowledge_diagnostics,
        },
        &mut fonts,
        "Noto Sans",
    )
    .map_err(|e| json!({"ok":false,"status":"exportBlocked","error":e.to_string()}))?;
    Ok((
        output.bytes,
        json!({"origin":output.origin,"logicalSize":output.logical_size,"pixelSize":output.pixel_size,"diagnostics":output.diagnostics}),
    ))
}

fn admit_write(state: &mut State, session: &str, request_id: &str, payload: &[u8]) -> Admission {
    if state.reconcile_ledger.contains_request_id(request_id) {
        Admission::Conflict
    } else {
        state.ledger.admit(session, request_id, payload)
    }
}

fn handle_reconcile(state: &mut State, request: &Reconcile) -> Value {
    if request.session_id != state.session.id() {
        return json!({"ok":false,"status":"SessionMismatch"});
    }
    if request.request_id.is_empty()
        || request.request_id.len() > 256
        || state.ledger.contains_request_id(&request.request_id)
    {
        return json!({"ok":false,"status":"Conflict"});
    }
    let bytes = serde_json::to_vec(request).expect("reconcile request serializes");
    if state
        .reconcile_ledger
        .contains_request_id(&request.request_id)
    {
        return match state
            .reconcile_ledger
            .admit(&request.session_id, &request.request_id, &bytes)
        {
            Admission::Completed(value) => value,
            other => {
                json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true})
            }
        };
    }
    let matches_pending = state.pending.as_ref().is_some_and(|pending| match pending {
        PendingEffect::Native { receipt, .. } => receipt.operation_id == request.operation_id,
        PendingEffect::Export { operation_id, .. } => *operation_id == request.operation_id,
    });
    let matches_settled = state
        .settled
        .as_ref()
        .is_some_and(|(id, _)| *id == request.operation_id);
    if !matches_pending && !matches_settled {
        return json!({"ok":false,"status":"unknownOperation","sessionId":request.session_id,"operationId":request.operation_id,"reconciliationRequired":true});
    }
    if request.expected_generation != state.session.snapshot().generation {
        return json!({"ok":false,"status":"generationConflict"});
    }
    match state
        .reconcile_ledger
        .admit(&request.session_id, &request.request_id, &bytes)
    {
        Admission::New => {
            let value = reconcile_effect(state, request.operation_id);
            state
                .reconcile_ledger
                .complete(&request.request_id, value.clone());
            value
        }
        other => {
            json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true,"manualInvestigationRequired":true})
        }
    }
}

fn reconcile_effect(state: &mut State, operation_id: uuid::Uuid) -> Value {
    if let Some((id, value)) = &state.settled
        && *id == operation_id
    {
        return value.clone();
    }
    let Some(pending) = state.pending.as_mut() else {
        return json!({"ok":false,"status":"unknownOperation","sessionId":state.session.id(),"operationId":operation_id,"reconciliationRequired":true});
    };
    let (actual, session, target, digest, content, stage, backup) = match pending {
        PendingEffect::Native {
            receipt,
            stage,
            backup,
        } => (
            receipt.operation_id,
            receipt.session_id.clone(),
            receipt.target.clone(),
            receipt.disk_digest,
            receipt.saved_content_id,
            stage.clone(),
            backup.clone(),
        ),
        PendingEffect::Export {
            operation_id,
            session_id,
            target,
            disk_digest,
            content_id,
            stage,
            backup,
            ..
        } => (
            *operation_id,
            session_id.clone(),
            target.clone(),
            *disk_digest,
            *content_id,
            stage.clone(),
            backup.clone(),
        ),
    };
    if actual != operation_id {
        return json!({"ok":false,"status":"unknownOperation","sessionId":state.session.id(),"operationId":operation_id,"reconciliationRequired":true});
    }
    let resolved = match pending {
        PendingEffect::Native { .. } => state
            .file
            .as_mut()
            .expect("pending native retains file")
            .reconcile()
            .map(|receipt| receipt.is_some()),
        PendingEffect::Export { writer, bytes, .. } => writer.reconcile(bytes),
    };
    match resolved {
        Ok(committed) => {
            if committed && matches!(pending, PendingEffect::Native { .. }) {
                state.session.mark_saved(content);
            }
            state.pending = None;
            let value = json!({"ok":true,"status":if committed {"committed"} else {"notCommitted"},"operationId":actual,"sessionId":session,"target":target,"contentId":content,"diskDigest":digest,"stage":stage,"backup":backup});
            state.settled = Some((actual, value.clone()));
            value
        }
        Err(e) => {
            json!({"ok":false,"status":"indeterminate","operationId":actual,"sessionId":session,"target":target,"contentId":content,"diskDigest":digest,"stage":stage,"backup":backup,"error":e.to_string(),"reconciliationRequired":true})
        }
    }
}

fn save_outcome(outcome: ProjectSaveOutcome) -> Value {
    match outcome {
        ProjectSaveOutcome::Committed { receipt, warning } => {
            json!({"ok":true,"status":"committed","operationId":receipt.operation_id,"sessionId":receipt.session_id,"contentId":receipt.saved_content_id,"diskDigest":receipt.disk_digest,"warning":warning.map(|e| e.to_string())})
        }
        ProjectSaveOutcome::NotCommitted { error } => {
            json!({"ok":false,"status":"notCommitted","error":error.to_string()})
        }
        ProjectSaveOutcome::Indeterminate {
            attempted,
            error,
            stage,
            backup,
        } => {
            json!({"ok":false,"status":"indeterminate","operationId":attempted.operation_id,"sessionId":attempted.session_id,"target":attempted.target,"contentId":attempted.saved_content_id,"diskDigest":attempted.disk_digest,"stage":stage,"backup":backup,"error":error.to_string(),"reconciliationRequired":true})
        }
    }
}

pub fn run(
    name: Option<String>,
    input: Option<PathBuf>,
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
    attach: Option<(String, String, String)>,
) -> Result<(), crate::run::Error> {
    let failure = |e: String| crate::run::Error {
        code: 1,
        message: e,
    };
    if attach.is_some() && (!read.is_empty() || !write.is_empty()) {
        return Err(failure(
            "attached sessions use GUI-authorized roots only".into(),
        ));
    }
    if let Some((endpoint, token, session)) = attach {
        return serve(Server {
            state: None,
            attach: Some(Arc::new(Attachment {
                endpoint,
                token,
                session,
            })),
        });
    }
    let read = read
        .into_iter()
        .map(|p| RootGrant::new(p, Access::ReadOnly))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| failure(e.to_string()))?;
    let write = write
        .into_iter()
        .map(|p| RootGrant::new(p, Access::Write))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| failure(e.to_string()))?;
    let (session, file) = if let Some(path) = input {
        let grant = read
            .iter()
            .chain(write.iter())
            .find_map(|root| root.grant(&path, Access::ReadOnly).ok())
            .ok_or_else(|| failure("input path is outside authorized roots".into()))?;
        let bytes = grant.read().map_err(|e| failure(e.to_string()))?;
        let loaded =
            processcraft_io::native::load_bytes(&bytes).map_err(|e| failure(e.to_string()))?;
        let session = Session::new(loaded.project).map_err(|e| failure(format!("{e:?}")))?;
        let file = if let Some(grant) = write
            .iter()
            .find_map(|root| root.grant(&path, Access::Write).ok())
        {
            let (file, reopened) =
                FileSession::open(session.id(), grant).map_err(|e| failure(e.to_string()))?;
            if reopened
                .project
                .content_identity()
                .map_err(|e| failure(e.to_string()))?
                != session.snapshot().content_id
            {
                return Err(failure("input changed during open".into()));
            }
            Some(file)
        } else {
            None
        };
        (session, file)
    } else {
        let mut session = Session::new(ProjectData::new(name.expect("clap requires new or in")))
            .map_err(|e| failure(format!("{e:?}")))?;
        session.mark_unsaved();
        (session, None)
    };
    let ledger = RequestLedger::new(session.id(), 128);
    let server = Server {
        state: Some(Arc::new(Mutex::new(State {
            reconcile_ledger: RequestLedger::new(session.id(), 8),
            session,
            ledger,
            _read: read,
            write,
            file,
            pending: None,
            settled: None,
        }))),
        attach: None,
    };
    serve(server)
}

fn serve(server: Server) -> Result<(), crate::run::Error> {
    let failure = |e: String| crate::run::Error {
        code: 1,
        message: e,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| failure(e.to_string()))?;
    rt.block_on(serve_lines(server, tokio::io::stdin(), tokio::io::stdout()))
        .map_err(failure)
}

/// Run the SDK over newline-delimited JSON-RPC. Lines the SDK would drop silently
/// (malformed JSON, duplicate keys, over 4 MiB) are answered with a JSON-RPC error
/// instead. Both the SDK's output and these errors reach `output` through one writer
/// task, one complete line at a time, so an error never interleaves with a response.
async fn serve_lines<I, O>(server: Server, input: I, output: O) -> Result<(), String>
where
    I: tokio::io::AsyncRead + Unpin + Send + 'static,
    O: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let (lines, mut queue) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let writer = tokio::spawn(async move {
        let mut output = output;
        // An empty line marks the end of the SDK's output; real lines are never empty.
        while let Some(line) = queue.recv().await.filter(|line| !line.is_empty()) {
            if output.write_all(&line).await.is_err() || output.flush().await.is_err() {
                return;
            }
        }
    });
    let (sdk_input, filter_output) = tokio::io::duplex(8192);
    tokio::spawn(filter_input(input, filter_output, lines.clone()));
    let (sdk_output, pipe) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let mut pipe = tokio::io::BufReader::new(pipe);
        loop {
            let mut line = Vec::new();
            match pipe.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => {
                    let _ = lines.send(Vec::new()).await;
                    return;
                }
                Ok(_) => {
                    if lines.send(line).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    let transport = AsyncRwTransport::<RoleServer, _, _>::new(sdk_input, sdk_output)
        .with_max_line_length(4 * 1024 * 1024);
    server
        .serve(transport)
        .await
        .map_err(|e| e.to_string())?
        .waiting()
        .await
        .map_err(|e| e.to_string())?;
    // The SDK's writer is dropped: drain what it wrote before returning.
    let _ = writer.await;
    Ok(())
}

/// Apply the same duplicate-key policy as batch input before SDK deserialization;
/// rejected lines are answered through `errors` (JSON-RPC, `id: null`).
async fn filter_input<I: tokio::io::AsyncRead + Unpin>(
    mut input: I,
    mut output: tokio::io::DuplexStream,
    errors: tokio::sync::mpsc::Sender<Vec<u8>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let reject = |code: i32, message: &str| {
        let mut line = json!({"jsonrpc":"2.0","id":null,"error":{"code":code,"message":message}})
            .to_string()
            .into_bytes();
        line.push(b'\n');
        line
    };
    let mut chunk = [0_u8; 8192];
    let mut line = Vec::new();
    let mut oversized = false;
    while let Ok(count) = input.read(&mut chunk).await {
        if count == 0 {
            break;
        }
        for byte in &chunk[..count] {
            if *byte == b'\n' {
                let rejection = if oversized {
                    Some(reject(-32600, "Invalid Request: line exceeds 4 MiB"))
                } else if line.trim_ascii().is_empty() {
                    None
                } else if serde_json::from_slice::<serde::de::IgnoredAny>(&line).is_err() {
                    Some(reject(-32700, "Parse error"))
                } else if processcraft_io::strict_json::has_duplicate_keys(&line) {
                    Some(reject(-32600, "Invalid Request: duplicate JSON key"))
                } else {
                    None
                };
                let failed = match rejection {
                    Some(error) => errors.send(error).await.is_err(),
                    None if !line.trim_ascii().is_empty() => {
                        output.write_all(&line).await.is_err()
                            || output.write_all(b"\n").await.is_err()
                    }
                    None => false,
                };
                if failed {
                    return;
                }
                line.clear();
                oversized = false;
            } else if !oversized {
                if line.len() == 4 * 1024 * 1024 {
                    line.clear();
                    oversized = true;
                } else {
                    line.push(*byte);
                }
            }
        }
    }
}

#[cfg(windows)]
fn forward_attachment(
    attachment: &Attachment,
    method: &str,
    args: Value,
) -> Result<CallToolResponse, ErrorData> {
    use processcraft_io::local_ipc::AttachmentRequest;
    if !matches!(
        method,
        "inspect" | "query" | "validate" | "apply" | "save" | "export" | "reconcile"
    ) {
        return Err(ErrorData::invalid_params("unknown tool", None));
    }
    let invalid = |e: serde_json::Error| ErrorData::invalid_params(e.to_string(), None);
    match method {
        "inspect" | "validate" if args != json!({}) => {
            return Err(ErrorData::invalid_params("arguments must be empty", None));
        }
        "query" => {
            let _: Query = serde_json::from_value(args.clone()).map_err(invalid)?;
        }
        "apply" => {
            let _: Apply = serde_json::from_value(args.clone()).map_err(invalid)?;
        }
        "save" => {
            let _: Save = serde_json::from_value(args.clone()).map_err(invalid)?;
        }
        "export" => {
            let _: Export = serde_json::from_value(args.clone()).map_err(invalid)?;
        }
        "reconcile" => {
            let _: Reconcile = serde_json::from_value(args.clone()).map_err(invalid)?;
        }
        _ => {}
    }
    let effect = matches!(method, "apply" | "save" | "export" | "reconcile");
    let (session_id, request_id, expected_generation) = if effect {
        let session = args
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorData::invalid_params("sessionId required", None))?;
        let request = args
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| ErrorData::invalid_params("requestId required", None))?;
        let generation = args
            .get("expectedGeneration")
            .and_then(Value::as_u64)
            .ok_or_else(|| ErrorData::invalid_params("expectedGeneration required", None))?;
        if session != attachment.session {
            return Err(ErrorData::invalid_params("session mismatch", None));
        }
        (session.to_owned(), request.to_owned(), generation)
    } else {
        (
            attachment.session.clone(),
            uuid::Uuid::new_v4().to_string(),
            0,
        )
    };
    let request = AttachmentRequest {
        session_id,
        request_id,
        expected_generation,
        method: method.to_owned(),
        params: args,
    };
    match processcraft_io::windows_pipe::call(&attachment.endpoint, &attachment.token, &request) {
        Ok(value) => Ok(CallToolResult::structured(value).into()),
        Err(_) => Ok(CallToolResult::structured_error(
            json!({"ok":false,"status":"unknown","sessionId":request.session_id,"requestId":request.request_id,"target":request.params.get("path"),"operationId":request.params.get("operationId"),"reconciliationRequired":effect}),
        )
        .into()),
    }
}

#[cfg(not(windows))]
fn forward_attachment(_: &Attachment, _: &str, _: Value) -> Result<CallToolResponse, ErrorData> {
    Err(ErrorData::internal_error(
        "attachment is supported only on Windows",
        None,
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn uncertain_export_retains_writer_until_explicit_settlement() {
        let dir =
            std::env::temp_dir().join(format!("processcraft-reconcile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("export.svg");
        std::fs::write(&target, b"old").unwrap();
        let grant = processcraft_io::FileGrant::new(&target, Access::Write).unwrap();
        let OpenSession::Writable(writer) = WriterSession::open(&grant).unwrap() else {
            panic!("writer");
        };
        // Deterministically simulate a replacement that installed the intended bytes
        // before reporting failure; IO's fault test covers the native failure itself.
        std::fs::write(&target, b"new").unwrap();
        let session = Session::new(ProjectData::new("test".into())).unwrap();
        let id = session.id();
        let operation = uuid::Uuid::new_v4();
        let mut state = State {
            ledger: RequestLedger::new(id.clone(), 1),
            reconcile_ledger: RequestLedger::new(id.clone(), 8),
            session,
            _read: vec![],
            write: vec![],
            file: None,
            pending: Some(PendingEffect::Export {
                operation_id: operation,
                session_id: id.clone(),
                target: target.clone(),
                disk_digest: Sha256::digest(b"new").into(),
                content_id: [0; 32],
                stage: dir.join("stage"),
                backup: dir.join("backup"),
                writer,
                bytes: b"new".to_vec(),
            }),
            settled: None,
        };
        let payload = b"export request";
        assert_eq!(state.ledger.admit(&id, "original", payload), Admission::New);
        let uncertain = json!({"status":"indeterminate","operationId":operation});
        assert!(state.ledger.complete("original", uncertain.clone()));
        assert_eq!(
            state.ledger.admit(&id, "another-write", b"x"),
            Admission::UnknownReconcile
        );
        for index in 0..10 {
            let unknown = Reconcile {
                session_id: id.clone(),
                request_id: format!("unknown-{index}"),
                expected_generation: 0,
                operation_id: uuid::Uuid::new_v4(),
            };
            assert_eq!(
                handle_reconcile(&mut state, &unknown)["status"],
                "unknownOperation"
            );
        }
        assert!(state.pending.is_some());
        let request = Reconcile {
            session_id: id.clone(),
            request_id: "reconcile-new".into(),
            expected_generation: 0,
            operation_id: operation,
        };
        let settled = handle_reconcile(&mut state, &request);
        assert_eq!(settled["status"], "committed");
        assert_eq!(
            admit_write(&mut state, &id, "reconcile-new", b"apply"),
            Admission::Conflict
        );
        let changed = Reconcile {
            session_id: id.clone(),
            request_id: "reconcile-new".into(),
            expected_generation: 0,
            operation_id: uuid::Uuid::new_v4(),
        };
        assert_eq!(handle_reconcile(&mut state, &changed)["status"], "Conflict");
        std::fs::write(&target, b"later change").unwrap();
        assert_eq!(handle_reconcile(&mut state, &request), settled);
        assert_eq!(std::fs::read(&target).unwrap(), b"later change");
        assert!(state.pending.is_none());
        assert_eq!(
            state.ledger.admit(&id, "original", payload),
            Admission::Completed(uncertain)
        );
        drop(state);
        drop(grant);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(all(test, windows))]
mod budget_tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    #[test]
    fn rejected_lines_get_json_rpc_errors_and_the_session_continues() {
        let session = Session::new(ProjectData::new("test".into())).unwrap();
        let id = session.id();
        let server = Server {
            state: Some(Arc::new(Mutex::new(State {
                ledger: RequestLedger::new(id.clone(), 8),
                reconcile_ledger: RequestLedger::new(id, 8),
                session,
                _read: vec![],
                write: vec![],
                file: None,
                pending: None,
                settled: None,
            }))),
            attach: None,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut write, input) = tokio::io::duplex(8192);
            let (output, read) = tokio::io::duplex(8192);
            let task = tokio::spawn(serve_lines(server, input, output));
            let mut read = BufReader::new(read);
            let mut next = async |line: &[u8]| {
                write.write_all(line).await.unwrap();
                let mut reply = String::new();
                read.read_line(&mut reply).await.unwrap();
                serde_json::from_str::<Value>(&reply).unwrap()
            };
            let parse = next(b"{\"jsonrpc\":\"2.0\",\"id\":1,\n").await;
            assert_eq!(parse["error"]["code"], -32700);
            assert_eq!(parse["id"], Value::Null);
            let duplicate = next(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":2}\n").await;
            assert_eq!(duplicate["error"]["code"], -32600);
            let init = json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
            let init = next(format!("\n{init}\n").as_bytes()).await;
            assert_eq!(init["id"], 3);
            assert!(init["result"].is_object(), "{init}");
            drop(write);
            task.await.unwrap().unwrap();
        });
    }
    #[test]
    fn exhausted_reconciliation_budget_blocks_file_effects_before_io() {
        let dir =
            std::env::temp_dir().join(format!("processcraft-budget-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let session = Session::new(ProjectData::new("test".into())).unwrap();
        let id = session.id();
        let mut reconcile_ledger = RequestLedger::new(id.clone(), 8);
        for n in 0..8 {
            let key = format!("reconcile-{n}");
            assert_eq!(reconcile_ledger.admit(&id, &key, b"valid"), Admission::New);
            assert!(reconcile_ledger.complete(&key, json!({"ok":true})));
        }
        let state = State {
            session,
            ledger: RequestLedger::new(id.clone(), 128),
            reconcile_ledger,
            _read: vec![],
            write: vec![RootGrant::new(&dir, Access::Write).unwrap()],
            file: None,
            pending: None,
            settled: None,
        };
        let server = Server {
            state: Some(Arc::new(Mutex::new(state))),
            attach: None,
        };
        let target = dir.join("result.processcraft");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (client, service) = tokio::io::duplex(8192);
            let (read, write) = tokio::io::split(service);
            let task = tokio::spawn(async move { let running = server.serve(AsyncRwTransport::<RoleServer,_,_>::new(read, write)).await.unwrap(); let _ = running.waiting().await; });
            let (read, mut write) = tokio::io::split(client); let mut read = BufReader::new(read);
            async fn call(write: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, read: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>, value: Value) -> Value {
                write.write_all(format!("{value}\n").as_bytes()).await.unwrap();
                let mut line=String::new(); read.read_line(&mut line).await.unwrap(); serde_json::from_str(&line).unwrap()
            }
            call(&mut write, &mut read, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await;
            let save=call(&mut write,&mut read,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":id,"requestId":"save","expectedGeneration":0,"path":target,"overwrite":false}}})).await;
            assert_eq!(save["result"]["structuredContent"]["status"], "manualInvestigationRequired");
            let export=call(&mut write,&mut read,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"export","arguments":{"sessionId":id,"requestId":"export","expectedGeneration":0,"path":target,"documentId":uuid::Uuid::new_v4().to_string(),"overwrite":false,"acknowledgeNativeFields":false}}})).await;
            assert_eq!(export["result"]["structuredContent"]["status"], "manualInvestigationRequired");
            let conflict=call(&mut write,&mut read,json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":id,"requestId":"save","expectedGeneration":0,"path":target,"overwrite":true}}})).await;
            assert_eq!(conflict["result"]["structuredContent"]["status"], "Conflict");
            let apply=call(&mut write,&mut read,json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"apply","arguments":{"sessionId":id,"requestId":"apply","expectedGeneration":0,"request":{"type":"batch","mutations":[{"type":"renameProject","name":"Changed"}]}}}})).await;
            assert_eq!(apply["result"]["structuredContent"]["ok"], true);
            drop(write); drop(read); task.await.unwrap();
        });
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
