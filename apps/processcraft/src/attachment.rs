//! GUI-owned attachment dispatcher. No worker accesses or copies the live session.
use crate::controller::Controller;
use processcraft_core::commands::{Envelope, Request};
use processcraft_io::{
    Access,
    automation_ledger::{Admission, RequestLedger},
    local_ipc::{AttachmentRequest, OwnerQueue, PendingRequest, owner_queue},
    root_grants::RootGrant,
    windows_pipe::Server,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
struct ExportContext {
    operation: uuid::Uuid,
    session_id: String,
    target: PathBuf,
    content_id: [u8; 32],
    disk_digest: Option<[u8; 32]>,
}
impl ExportContext {
    fn disconnected(&self) -> Value {
        json!({"ok":false,"status":"indeterminate","operationId":self.operation,"sessionId":self.session_id,"target":self.target,"contentId":self.content_id,"diskDigest":self.disk_digest,"stage":null,"backup":null,"error":"export worker stopped unexpectedly","reconciliationRequired":true})
    }
}
type SharedExportContext = Arc<Mutex<ExportContext>>;
type ExportResult = (Value, Option<(processcraft_io::WriterSession, Vec<u8>)>);

pub struct Attachment {
    session_id: String,
    server: Server,
    owner: OwnerQueue,
    ledger: RequestLedger,
    roots: Vec<RootGrant>,
    uncertain_export: Option<(processcraft_io::WriterSession, Vec<u8>)>,
    lost_export: Option<SharedExportContext>,
    pending_save: Option<PendingRequest>,
    pending_export: Option<(
        PendingRequest,
        std::sync::mpsc::Receiver<ExportResult>,
        SharedExportContext,
    )>,
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
#[derive(Deserialize)]
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
    /// Image formats only: draw this attribute definition as an overlay with a legend.
    #[serde(default)]
    overlay_attribute: Option<processcraft_core::ids::AttributeDefinitionId>,
    #[serde(default)]
    overlay_mode: Option<processcraft_core::overlay::OverlayMode>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportKind {
    #[default]
    Bpmn,
    Svg,
    Png,
    Pdf,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Query {
    Model {
        #[serde(alias = "documentId", alias = "docId")]
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
        #[serde(alias = "documentId", alias = "docId")]
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Diagrams {
        #[serde(alias = "documentId", alias = "docId")]
        document_id: processcraft_core::ids::DocumentId,
        offset: usize,
        limit: usize,
    },
    Identities {
        #[serde(alias = "documentId", alias = "docId")]
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
    },
    Library {
        text: String,
        offset: usize,
        limit: usize,
    },
    Documents {
        offset: usize,
        limit: usize,
    },
    Conventions {},
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
    Raci {
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
}
impl Attachment {
    pub fn start(session_id: String, wake: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        let (sender, owner) = owner_queue(session_id.clone(), 16);
        let server = Server::start(sender.with_wake(wake))?;
        Ok(Self {
            ledger: RequestLedger::new(session_id.clone(), 128),
            session_id,
            server,
            owner,
            roots: Vec::new(),
            uncertain_export: None,
            lost_export: None,
            pending_save: None,
            pending_export: None,
        })
    }
    pub fn endpoint(&self) -> &str {
        self.server.endpoint()
    }
    pub fn token(&self) -> &str {
        self.server.token()
    }
    pub fn authorize_root(&mut self, path: PathBuf) -> io::Result<()> {
        self.roots.push(RootGrant::new(path, Access::Write)?);
        Ok(())
    }
    pub fn has_pending_save(&self) -> bool {
        self.pending_save.is_some()
    }
    pub fn reconcile_export(&mut self) -> io::Result<Option<bool>> {
        if self.lost_export.is_some() {
            return Err(io::Error::other(
                "export worker stopped unexpectedly; manual reconciliation required",
            ));
        }
        let Some((writer, bytes)) = self.uncertain_export.as_mut() else {
            return Ok(None);
        };
        let committed = writer.reconcile(bytes)?;
        self.uncertain_export = None;
        Ok(Some(committed))
    }
    pub fn has_pending_effect(&self) -> bool {
        self.pending_save.is_some()
            || self.pending_export.is_some()
            || self.uncertain_export.is_some()
            || self.lost_export.is_some()
    }
    /// Returns false after the owner session has changed and the attachment was revoked.
    pub fn poll(&mut self, controller: &mut Controller, accept_requests: bool) -> bool {
        if controller.session.id() != self.session_id {
            self.owner.revoke();
            return false;
        }
        if let Some((_, receiver, _)) = &self.pending_export {
            match receiver.try_recv() {
                Ok((value, uncertain)) => {
                    self.uncertain_export = uncertain;
                    let (pending, _, _) = self.pending_export.take().unwrap();
                    self.ledger
                        .complete(&pending.request.request_id, value.clone());
                    pending.reply(value);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let (pending, _, context) = self.pending_export.take().unwrap();
                    let value = context
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .disconnected();
                    self.lost_export = Some(context);
                    self.ledger
                        .complete(&pending.request.request_id, value.clone());
                    pending.reply(value);
                }
            }
        }
        if self.pending_save.is_some() {
            if controller.poll_save().is_none() {
                return true;
            }
            let pending = self.pending_save.take().unwrap();
            let response = match controller.take_save_report() {
                Some(report) => match report.receipt {
                    Some(receipt) => {
                        json!({"ok":true,"status":"committed","operationId":receipt.operation_id,"sessionId":receipt.session_id,"contentId":receipt.saved_content_id,"diskDigest":receipt.disk_digest,"warning":report.error})
                    }
                    None if report.uncertain => {
                        if let Some(details) = report.details {
                            json!({"ok":false,"status":"indeterminate","operationId":details.attempted.operation_id,"sessionId":details.attempted.session_id,"contentId":details.attempted.saved_content_id,"diskDigest":details.attempted.disk_digest,"target":details.attempted.target,"stage":details.stage,"backup":details.backup,"error":report.error,"reconciliationRequired":true})
                        } else {
                            json!({"ok":false,"status":"indeterminate","error":report.error,"reconciliationRequired":true})
                        }
                    }
                    None => json!({"ok":false,"status":"notCommitted","error":report.error}),
                },
                None => json!({"ok":false,"status":"indeterminate","reconciliationRequired":true}),
            };
            self.ledger
                .complete(&pending.request.request_id, response.clone());
            pending.reply(response);
        }
        if !accept_requests {
            return true;
        }
        while let Some(pending) = self.owner.try_recv() {
            if pending.request.method == "export" {
                match self.begin_export(controller, &pending.request) {
                    Ok((receiver, context)) => {
                        self.pending_export = Some((pending, receiver, context));
                        break;
                    }
                    Err(value) => {
                        pending.reply(value);
                        continue;
                    }
                }
            }
            if pending.request.method == "save" {
                match self.begin_save(controller, &pending.request) {
                    Ok(()) => {
                        self.pending_save = Some(pending);
                        break;
                    }
                    Err(response) => {
                        pending.reply(response);
                        continue;
                    }
                }
            }
            let response = self.dispatch(controller, &pending.request);
            pending.reply(response);
        }
        true
    }
    fn begin_export(
        &mut self,
        controller: &Controller,
        req: &AttachmentRequest,
    ) -> Result<(std::sync::mpsc::Receiver<ExportResult>, SharedExportContext), Value> {
        let args: Export = serde_json::from_value(req.params.clone())
            .map_err(|e| json!({"ok":false,"error":e.to_string()}))?;
        if args.session_id != req.session_id
            || args.request_id != req.request_id
            || args.expected_generation != req.expected_generation
        {
            return Err(json!({"ok":false,"status":"envelopeMismatch"}));
        }
        let bytes =
            serde_json::to_vec(&(req.method.as_str(), &req.params)).expect("JSON serializes");
        match self
            .ledger
            .admit(&args.session_id, &args.request_id, &bytes)
        {
            Admission::Completed(value) => return Err(value),
            Admission::New => {}
            other => {
                return Err(
                    json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true}),
                );
            }
        }
        let snapshot = controller.session.snapshot();
        let grant = self
            .roots
            .iter()
            .find_map(|root| root.grant(&args.path, Access::Write).ok());
        let failure = if args.expected_generation != snapshot.generation {
            Some("generationConflict")
        } else if self.uncertain_export.is_some()
            || self.lost_export.is_some()
            || !controller.can_transition()
        {
            Some("busy")
        } else if let (Some(file), Some(target_grant)) = (&controller.file, &grant) {
            match file.same_target(target_grant) {
                Ok(true) => Some("nativeTargetForbidden"),
                Err(_) => Some("nativeTargetComparisonFailed"),
                Ok(false) if args.path.exists() && !args.overwrite => {
                    Some("overwriteRequiresAuthorization")
                }
                Ok(false) => None,
            }
        } else if grant.is_none() {
            Some("pathNotAuthorized")
        } else if args.path.exists() && !args.overwrite {
            Some("overwriteRequiresAuthorization")
        } else {
            None
        };
        if let Some(status) = failure {
            let value = json!({"ok":false,"status":status});
            self.ledger.complete(&args.request_id, value.clone());
            return Err(value);
        }
        let grant = grant.unwrap();
        let context = Arc::new(Mutex::new(ExportContext {
            operation: uuid::Uuid::new_v4(),
            session_id: args.session_id.clone(),
            target: args.path.clone(),
            content_id: snapshot.content_id,
            disk_digest: None,
        }));
        let worker_context = context.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            use processcraft_io::{OpenSession, SaveOutcome, WriterSession};
            use sha2::{Digest, Sha256};
            let mut uncertain = None;
            let value = match prepare_export(&snapshot.project, &args) {
                Err(value) => value,
                Ok((data, report)) => {
                    let disk_digest: [u8; 32] = Sha256::digest(&data).into();
                    let operation = {
                        let mut c = worker_context.lock().unwrap_or_else(|e| e.into_inner());
                        c.disk_digest = Some(disk_digest);
                        c.operation
                    };
                    match WriterSession::open(&grant) {
                        Ok(OpenSession::Writable(mut writer)) => {
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                writer.save(&data)
                            })) {
                                Ok(SaveOutcome::Committed { warning }) => {
                                    json!({"ok":true,"status":"committed","operationId":operation,"sessionId":args.session_id,"target":args.path,"contentId":snapshot.content_id,"diskDigest":disk_digest,"warning":warning.map(|e|e.to_string()),"report":report})
                                }
                                Ok(SaveOutcome::NotCommitted { error }) => {
                                    json!({"ok":false,"status":"notCommitted","operationId":operation,"error":error.to_string()})
                                }
                                Ok(SaveOutcome::Indeterminate {
                                    error,
                                    stage,
                                    backup,
                                }) => {
                                    uncertain = Some((writer, data));
                                    json!({"ok":false,"status":"indeterminate","operationId":operation,"sessionId":args.session_id,"target":args.path,"contentId":snapshot.content_id,"diskDigest":disk_digest,"error":error.to_string(),"stage":stage,"backup":backup,"reconciliationRequired":true})
                                }
                                Err(_) => {
                                    uncertain = Some((writer, data));
                                    json!({"ok":false,"status":"indeterminate","operationId":operation,"sessionId":args.session_id,"target":args.path,"contentId":snapshot.content_id,"diskDigest":disk_digest,"error":"export worker panicked during write","stage":null,"backup":null,"reconciliationRequired":true})
                                }
                            }
                        }
                        Ok(OpenSession::ReadOnly) => {
                            json!({"ok":false,"status":"notCommitted","error":"writer unavailable"})
                        }
                        Err(e) => json!({"ok":false,"status":"notCommitted","error":e.to_string()}),
                    }
                }
            };
            let _ = tx.send((value, uncertain));
        });
        Ok((rx, context))
    }
    fn begin_save(
        &mut self,
        controller: &mut Controller,
        req: &AttachmentRequest,
    ) -> Result<(), Value> {
        let parsed: Save = serde_json::from_value(req.params.clone())
            .map_err(|e| json!({"ok":false,"error":e.to_string()}))?;
        if parsed.session_id != req.session_id
            || parsed.request_id != req.request_id
            || parsed.expected_generation != req.expected_generation
        {
            return Err(json!({"ok":false,"status":"envelopeMismatch"}));
        }
        let bytes =
            serde_json::to_vec(&(req.method.as_str(), &req.params)).expect("JSON serializes");
        match self
            .ledger
            .admit(&parsed.session_id, &parsed.request_id, &bytes)
        {
            Admission::Completed(value) => return Err(value),
            Admission::New => {}
            other => {
                return Err(
                    json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true}),
                );
            }
        }
        let grant = self
            .roots
            .iter()
            .find_map(|root| root.grant(&parsed.path, Access::Write).ok());
        let error = if parsed.expected_generation != controller.session.snapshot().generation {
            Some("generationConflict")
        } else if parsed.path.exists()
            && !parsed.overwrite
            && controller
                .file
                .as_ref()
                .is_none_or(|f| f.target() != parsed.path)
        {
            Some("overwriteRequiresAuthorization")
        } else if grant.is_none() {
            Some("pathNotAuthorized")
        } else if self.uncertain_export.is_some()
            || self.lost_export.is_some()
            || !controller.can_transition()
        {
            Some("busy")
        } else {
            None
        };
        if let Some(status) = error {
            let value = json!({"ok":false,"status":status});
            self.ledger.complete(&parsed.request_id, value.clone());
            return Err(value);
        }
        if !controller.save_granted(grant.unwrap()) {
            let value = json!({"ok":false,"status":"notCommitted","error":controller.error});
            self.ledger.complete(&parsed.request_id, value.clone());
            return Err(value);
        }
        Ok(())
    }
    fn dispatch(&mut self, controller: &mut Controller, req: &AttachmentRequest) -> Value {
        if req.session_id != self.session_id {
            return json!({"ok":false,"status":"sessionMismatch"});
        }
        match req.method.as_str() {
            "inspect" if req.params == json!({}) => {
                let s = controller.session.snapshot();
                json!({"sessionId":controller.session.id(),"generation":s.generation,"contentId":s.content_id,"projectId":s.project.id,"name":s.project.name,"documents":s.project.documents.len()})
            }
            "validate" if req.params == json!({}) => {
                let s = controller.session.snapshot();
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
                let q: Query = match serde_json::from_value(req.params.clone()) {
                    Ok(q) => q,
                    Err(e) => return json!({"ok":false,"error":e.to_string()}),
                };
                let snapshot = controller.session.snapshot();
                let catalog_query = matches!(
                    &q,
                    Query::Catalog { .. }
                        | Query::Data { .. }
                        | Query::Identities { .. }
                        | Query::Events { .. }
                        | Query::Diagrams { .. }
                        | Query::Model { .. }
                        | Query::CheckpointFields { .. }
                );
                let (items, offset, limit): (Vec<Value>, usize, usize) = match q {
                    Query::Model { document_id, offset, limit } => {
                        let page = match processcraft_core::queries::document_fields_page(&snapshot.project, document_id, offset, limit) {
                            Ok(Some(page)) => page,
                            Ok(None) => return json!({"ok":false,"error":"document not found"}),
                            Err(error) => return json!({"ok":false,"error":error.to_string()}),
                        };
                        return json!({"total":page.total,"items":page.items,"nextOffset":page.next_offset,"generation":snapshot.generation,"contentId":snapshot.content_id});
                    },
                    Query::CheckpointFields { checkpoint_id, offset, limit } => {
                        let page = match processcraft_core::queries::checkpoint_fields_page(&snapshot.project, checkpoint_id, offset, limit) {
                            Ok(Some(page)) => page,
                            Ok(None) => return json!({"ok":false,"error":"checkpoint not found"}),
                            Err(error) => return json!({"ok":false,"error":error.to_string()}),
                        };
                        return json!({"total":page.total,"items":page.items,"nextOffset":page.next_offset,"generation":snapshot.generation,"contentId":snapshot.content_id});
                    },
                    Query::Events { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::event_fields(&snapshot.project, document_id) else { return json!({"ok":false,"error":"document not found"}); };
                        (rows.into_iter().map(|row| json!(row)).collect(), offset, limit)
                    },
                    Query::Diagrams { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::diagram_fields(&snapshot.project, document_id) else { return json!({"ok":false,"error":"document not found"}); };
                        (rows.into_iter().map(|row| json!(row)).collect(), offset, limit)
                    },
                    Query::Identities { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::identified_elements(&snapshot.project, document_id) else {
                            return json!({"ok":false,"error":"document not found"});
                        };
                        (rows.into_iter().map(|row| json!(row)).collect(), offset, limit)
                    },
                    Query::Data { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::data_fields(&snapshot.project, document_id) else {
                            return json!({"ok":false,"error":"document not found"});
                        };
                        (rows.into_iter().map(|row| json!(row)).collect(), offset, limit)
                    },
                    Query::Catalog { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::supporting_catalog_fields(&snapshot.project, document_id) else {
                            return json!({"ok":false,"error":"document not found"});
                        };
                        (rows.into_iter().map(|row| json!(row)).collect(), offset, limit)
                    },
                    Query::Comments { offset, limit } => (snapshot.project.comments.iter().map(|c| json!({"comment":c,"orphan":processcraft_core::queries::comment_is_orphan(&snapshot.project,c)})).collect(), offset, limit),
                    Query::Checkpoints { offset, limit } => (snapshot.project.checkpoints.iter().map(|c| json!({"id":c.id,"name":c.name,"createdUnixMs":c.created_unix_ms,"reference":snapshot.project.reference_checkpoint == Some(c.id)})).collect(), offset, limit),
                    Query::CompareCheckpoint { checkpoint_id, offset, limit } => {
                        let Some(comparison) = processcraft_core::queries::compare_checkpoint(&snapshot.project, checkpoint_id) else { return json!({"ok":false,"error":"checkpoint not found"}); };
                        (vec![json!(comparison)], offset, limit)
                    },
                    Query::LayoutPreview { session_id, expected_generation, document_id, diagram_id } => {
                        if session_id != controller.session.id() || expected_generation != snapshot.generation { return json!({"ok":false,"error":"session or generation mismatch"}); }
                        let Some(doc) = snapshot.project.documents.iter().find(|d| d.id == document_id) else { return json!({"ok":false,"error":"document not found"}); };
                        let proposal = match processcraft_core::layout::propose_layout(doc, &diagram_id, &session_id, expected_generation, &std::sync::atomic::AtomicBool::new(false)) { Ok(v) => v, Err(e) => return json!({"ok":false,"error":e.to_string()}) };
                        let value = json!({"proposal":proposal});
                        if serde_json::to_vec(&value).map_or(true, |v| v.len() > 3 * 1024 * 1024) { return json!({"ok":false,"error":"proposal exceeds response limit"}); }
                        return value;
                    }
                    Query::Library {
                        text,
                        offset,
                        limit,
                    } => (
                        processcraft_core::queries::library_search(&snapshot.project, &text)
                            .into_iter()
                            .map(|v| json!(v))
                            .collect(),
                        offset,
                        limit,
                    ),
                    Query::Documents { offset, limit } => (
                        processcraft_core::queries::document_overview(&snapshot.project),
                        offset,
                        limit,
                    ),
                    Query::Conventions {} => {
                        return json!({"conventions":snapshot.project.conventions});
                    }
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
                    Query::Table {
                        document_id,
                        process_id,
                        offset,
                        limit,
                    } => {
                        let Some(rows) = processcraft_core::queries::activity_table(
                            &snapshot.project,
                            document_id,
                            &process_id,
                        ) else {
                            return json!({"ok":false,"error":"process not found"});
                        };
                        (rows.into_iter().map(|row| json!({"key":row.key,"name":row.name,"lane":row.lane,"role":row.role,"systems":row.systems,"documents":row.documents,"attributes":row.attributes,"outgoing":row.outgoing.into_iter().map(|f| json!({"id":f.id,"target":f.target,"condition":f.condition})).collect::<Vec<_>>() })).collect(),offset,limit)
                    }
                    Query::Raci { document_id, process_id, offset, limit } => {
                        let Some(matrix) = processcraft_core::queries::raci_matrix(&snapshot.project, document_id, &process_id) else {
                            return json!({"ok":false,"error":"process not found"});
                        };
                        if !(1..=100).contains(&limit) {
                            return json!({"ok":false,"error":"limit must be 1..100"});
                        }
                        let columns = json!(matrix.columns);
                        // The columns share the 3 MiB response budget with the page.
                        let mut bytes = serde_json::to_vec(&columns).map_or(3 * 1024 * 1024 + 1, |v| v.len());
                        if bytes > 3 * 1024 * 1024 {
                            return json!({"ok":false,"error":"query header exceeds response limit"});
                        }
                        let total = matrix.rows.len();
                        let mut page = Vec::new();
                        for item in matrix.rows.into_iter().skip(offset).take(limit).map(|row| json!(row)) {
                            let size = serde_json::to_vec(&item).map_or(3 * 1024 * 1024 + 1, |v| v.len());
                            if bytes.saturating_add(size) > 3 * 1024 * 1024 {
                                if page.is_empty() {
                                    return json!({"ok":false,"error":"item exceeds response limit"});
                                }
                                break;
                            }
                            bytes += size;
                            page.push(item);
                        }
                        let next = offset.saturating_add(page.len());
                        return json!({"columns":columns,"total":total,"items":page,"nextOffset":if next < total { Some(next) } else { None }});
                    }
                    Query::Attributes { document_id, offset, limit } => {
                        let Some(rows) = processcraft_core::queries::element_attributes(&snapshot.project, document_id) else {
                            return json!({"ok":false,"error":"document not found"});
                        };
                        if !(1..=100).contains(&limit) {
                            return json!({"ok":false,"error":"limit must be 1..100"});
                        }
                        let definitions = json!(processcraft_core::queries::attribute_definitions(&snapshot.project));
                        // The definitions share the 3 MiB response budget with the page.
                        let mut bytes = serde_json::to_vec(&definitions).map_or(3 * 1024 * 1024 + 1, |v| v.len());
                        if bytes > 3 * 1024 * 1024 {
                            return json!({"ok":false,"error":"query header exceeds response limit"});
                        }
                        let total = rows.len();
                        let mut page = Vec::new();
                        for item in rows.into_iter().skip(offset).take(limit).map(|row| json!(row)) {
                            let size = serde_json::to_vec(&item).map_or(3 * 1024 * 1024 + 1, |v| v.len());
                            if bytes.saturating_add(size) > 3 * 1024 * 1024 {
                                if page.is_empty() {
                                    return json!({"ok":false,"error":"item exceeds response limit"});
                                }
                                break;
                            }
                            bytes += size;
                            page.push(item);
                        }
                        let next = offset.saturating_add(page.len());
                        return json!({"definitions":definitions,"total":total,"items":page,"nextOffset":if next < total { Some(next) } else { None }});
                    }
                };
                if !(1..=100).contains(&limit) {
                    return json!({"ok":false,"error":"limit must be 1..100"});
                }
                let total = items.len();
                let mut page = Vec::new();
                let mut bytes = 0usize;
                for item in items.into_iter().skip(offset).take(limit) {
                    let size = serde_json::to_vec(&item).map_or(3 * 1024 * 1024 + 1, |v| v.len());
                    if bytes.saturating_add(size) > 3 * 1024 * 1024 {
                        if page.is_empty() {
                            return json!({"ok":false,"error":"item exceeds response limit"});
                        }
                        break;
                    }
                    bytes += size;
                    page.push(item);
                }
                let next = offset.saturating_add(page.len());
                let mut result = json!({"total":total,"items":page,"nextOffset":if next < total { Some(next) } else { None }});
                if catalog_query {
                    result["generation"] = json!(snapshot.generation);
                    result["contentId"] = json!(snapshot.content_id);
                }
                result
            }
            "apply" => {
                let parsed: Apply = match serde_json::from_value(req.params.clone()) {
                    Ok(v) => v,
                    Err(e) => return json!({"ok":false,"error":e.to_string()}),
                };
                if parsed.session_id != req.session_id
                    || parsed.request_id != req.request_id
                    || parsed.expected_generation != req.expected_generation
                {
                    return json!({"ok":false,"status":"envelopeMismatch"});
                }
                let bytes = serde_json::to_vec(&(req.method.as_str(), &req.params))
                    .expect("JSON serializes");
                match self
                    .ledger
                    .admit(&parsed.session_id, &parsed.request_id, &bytes)
                {
                    Admission::Completed(value) => value,
                    Admission::New => {
                        let result = controller.session.apply(
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
                                    serde_json::to_value(r).expect("change set serializes");
                                value
                                    .as_object_mut()
                                    .expect("change set is an object")
                                    .insert("ok".into(), json!(true));
                                value
                            }
                            Err(e) => json!({"ok":false,"error":format!("{e:?}")}),
                        };
                        self.ledger.complete(&parsed.request_id, value.clone());
                        value
                    }
                    other => {
                        json!({"ok":false,"status":format!("{other:?}"),"reconciliationRequired":true})
                    }
                }
            }
            "reconcile" => {
                json!({"ok":false,"status":"reconciliationViaGui","reconciliationRequired":true})
            }
            "save" | "export" => json!({"ok":false,"status":"noAuthorizedWriteRoot"}),
            _ => json!({"ok":false,"error":"unknown method or invalid arguments"}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::commands::Request;
    #[test]
    fn attached_layout_preview_rejects_stale_session_and_generation() {
        let mut controller = Controller::default();
        let session = controller.session.id();
        let snapshot = controller.session.snapshot();
        let doc = &snapshot.project.documents[0];
        let document_id = doc.id;
        let diagram_id = doc.diagrams[0].id.clone();
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        let mut request = AttachmentRequest {
            session_id: session.clone(),
            request_id: "preview".into(),
            expected_generation: snapshot.generation,
            method: "query".into(),
            params: json!({"kind":"layoutPreview","sessionId":session,"expectedGeneration":snapshot.generation,"documentId":document_id,"diagramId":diagram_id}),
        };
        let result = attachment.dispatch(&mut controller, &request);
        assert!(result.get("proposal").is_some(), "{result}");
        request.params["expectedGeneration"] = json!(snapshot.generation + 1);
        assert_eq!(attachment.dispatch(&mut controller, &request)["ok"], false);
        request.params["expectedGeneration"] = json!(snapshot.generation);
        request.params["sessionId"] = json!("wrong-session");
        assert_eq!(attachment.dispatch(&mut controller, &request)["ok"], false);
    }
    #[test]
    fn apply_retry_is_idempotent_and_visible_undo_works() {
        let mut controller = Controller::default();
        let session = controller.session.id();
        let generation = controller.session.snapshot().generation;
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        let request = AttachmentRequest {
            session_id: session.clone(),
            request_id: "one".into(),
            expected_generation: generation,
            method: "apply".into(),
            params: json!({"sessionId":session,"requestId":"one","expectedGeneration":generation,"request":{"type":"batch","mutations":[{"type":"renameProject","name":"Attached"}]}}),
        };
        let first = attachment.dispatch(&mut controller, &request);
        assert_eq!(first["ok"], true);
        assert_eq!(controller.session.snapshot().project.name, "Attached");
        assert_eq!(attachment.dispatch(&mut controller, &request), first);
        let mut changed = request.clone();
        changed.params["request"]["mutations"][0]["name"] = json!("Changed");
        assert_eq!(
            attachment.dispatch(&mut controller, &changed)["reconciliationRequired"],
            true
        );
        assert!(controller.apply(Request::Undo));
        assert_ne!(controller.session.snapshot().project.name, "Attached");
        controller.session = Controller::default().session;
        assert!(!attachment.poll(&mut controller, true));
    }
}

fn prepare_export(
    project: &processcraft_core::model::ProjectData,
    args: &Export,
) -> Result<(Vec<u8>, Value), Value> {
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
    let overlay = args
        .overlay_attribute
        .map(|definition| {
            processcraft_core::overlay::export_overlay(
                project,
                args.document_id,
                definition,
                args.overlay_mode,
            )
        })
        .transpose()
        .map_err(|e| json!({"ok":false,"status":"overlayBlocked","error":e}))?;
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
    let output = processcraft_render::export_diagram_with_overlay(
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
        overlay.as_ref(),
    )
    .map_err(|e| json!({"ok":false,"status":"exportBlocked","error":e.to_string()}))?;
    Ok((
        output.bytes,
        json!({"origin":output.origin,"logicalSize":output.logical_size,"pixelSize":output.pixel_size,"diagnostics":output.diagnostics}),
    ))
}
#[cfg(test)]
mod effect_tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn attached_save_requires_root_and_replies_after_commit() {
        let dir =
            std::env::temp_dir().join(format!("processcraft-attachment-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let mut ctl = Controller::default();
        let session = ctl.session.id();
        let generation = ctl.session.snapshot().generation;
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        let path = dir.join("attached.pc");
        let request = AttachmentRequest {
            session_id: session.clone(),
            request_id: "save-1".into(),
            expected_generation: generation,
            method: "save".into(),
            params: json!({"sessionId":session,"requestId":"save-1","expectedGeneration":generation,"path":path,"overwrite":false}),
        };
        assert_eq!(
            attachment.begin_save(&mut ctl, &request).unwrap_err()["status"],
            "pathNotAuthorized"
        );
        attachment.authorize_root(dir.clone()).unwrap();
        let mut next = request.clone();
        next.request_id = "save-2".into();
        next.params["requestId"] = json!("save-2");
        let endpoint = attachment.endpoint().to_owned();
        let token = attachment.token().to_owned();
        let client = std::thread::spawn(move || {
            processcraft_io::windows_pipe::call(&endpoint, &token, &next)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !client.is_finished() {
            attachment.poll(&mut ctl, true);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let result = client.join().unwrap().unwrap();
        assert_eq!(result["status"], "committed");
        assert!(path.is_file());
        drop(attachment);
        drop(ctl);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn export_is_root_bound_and_writes_after_authorization() {
        let dir = std::env::temp_dir().join(format!(
            "processcraft-attachment-export-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&dir).unwrap();
        let ctl = Controller::default();
        let session = ctl.session.id();
        let snap = ctl.session.snapshot();
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        let path = dir.join("diagram.bpmn");
        let mut req = AttachmentRequest {
            session_id: session.clone(),
            request_id: "export-1".into(),
            expected_generation: snap.generation,
            method: "export".into(),
            params: json!({"sessionId":session,"requestId":"export-1","expectedGeneration":snap.generation,"path":path,"documentId":snap.project.documents[0].id,"overwrite":false,"acknowledgeNativeFields":true}),
        };
        assert_eq!(
            attachment.begin_export(&ctl, &req).unwrap_err()["status"],
            "pathNotAuthorized"
        );
        attachment.authorize_root(dir.clone()).unwrap();
        req.request_id = "export-2".into();
        req.params["requestId"] = json!("export-2");
        let (result, retained) = attachment
            .begin_export(&ctl, &req)
            .unwrap()
            .0
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(result["status"], "committed", "{result}");
        assert!(retained.is_none());
        assert!(path.is_file());
        drop(attachment);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(test)]
mod native_export_guard_tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn rejects_open_native_target_and_hardlink_without_modification() {
        let dir = std::env::temp_dir().join(format!(
            "processcraft-export-guard-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("native.pc");
        let alias = dir.join("alias.pc");
        let mut ctl = Controller::default();
        assert!(ctl.save(Some(path.clone())));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(ok) = ctl.poll_save() {
                assert!(ok);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let original = std::fs::read(&path).unwrap();

        let snapshot = ctl.session.snapshot();
        let session = ctl.session.id();
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        attachment.authorize_root(dir.clone()).unwrap();
        for (i, target) in [path.clone(), alias.clone()].into_iter().enumerate() {
            if i == 1 {
                std::fs::hard_link(&path, &alias).unwrap();
            }
            let id = format!("export-{i}");
            let req = AttachmentRequest {
                session_id: session.clone(),
                request_id: id.clone(),
                expected_generation: snapshot.generation,
                method: "export".into(),
                params: json!({"sessionId":session,"requestId":id,"expectedGeneration":snapshot.generation,"path":target,"documentId":snapshot.project.documents[0].id,"overwrite":true,"acknowledgeNativeFields":true}),
            };
            let status = attachment.begin_export(&ctl, &req).unwrap_err();
            assert_eq!(
                status["status"],
                if i == 0 {
                    "nativeTargetForbidden"
                } else {
                    "pathNotAuthorized"
                }
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        drop(attachment);
        drop(ctl);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(test)]
mod disconnect_context_tests {
    use super::*;
    #[test]
    fn disconnected_export_retains_origin_and_blocks_transitions() {
        let mut ctl = Controller::default();
        let session = ctl.session.id();
        let mut attachment = Attachment::start(session.clone(), Arc::new(|| {})).unwrap();
        let (sender, owner) = owner_queue(session.clone(), 1);
        let request = AttachmentRequest {
            session_id: session.clone(),
            request_id: "broken".into(),
            expected_generation: 0,
            method: "export".into(),
            params: json!({}),
        };
        let reply = sender.submit(request).unwrap();
        let pending = owner.try_recv().unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        drop(tx);
        let operation = uuid::Uuid::new_v4();
        let target = PathBuf::from(r"C:\example\out.bpmn");
        let context = Arc::new(Mutex::new(ExportContext {
            operation,
            session_id: session.clone(),
            target: target.clone(),
            content_id: [1; 32],
            disk_digest: Some([2; 32]),
        }));
        attachment.pending_export = Some((pending, rx, context));
        assert!(attachment.poll(&mut ctl, true));
        let result = reply.recv().unwrap();
        assert_eq!(result["operationId"], operation.to_string());
        assert_eq!(result["sessionId"], session);
        assert_eq!(result["target"], json!(target));
        assert_eq!(result["diskDigest"], json!(vec![2; 32]));
        assert!(result["stage"].is_null());
        assert!(result["backup"].is_null());
        assert!(attachment.has_pending_effect());
    }
}
