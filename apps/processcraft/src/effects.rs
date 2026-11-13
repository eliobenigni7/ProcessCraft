use processcraft_core::{
    ids::{DocumentId, ElementSelector},
    model::ProjectData,
};
use processcraft_io::{Access, FileGrant, OpenSession, SaveOutcome, WriterSession, interchange};
use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
pub enum ExportKind {
    Bpmn,
    Drawio,
    Image(ExportFormat),
}
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

pub enum ResultValue {
    Import {
        session: String,
        generation: u64,
        prepared: Box<interchange::PreparedImport>,
    },
    Export {
        bytes: Vec<u8>,
        report: Vec<String>,
        extension: &'static str,
    },
    Opened {
        session: String,
        generation: u64,
        opened: Box<crate::controller::OpenedProject>,
    },
    Written(String),
    Indeterminate(Box<UncertainExport>),
    LostWrite(Box<WriteContext>),
    Failed(String),
}
/// Largest BPMN input read for an import.
const IMPORT_LIMIT: usize = 32 * 1024 * 1024;
fn import_result(
    result: Result<interchange::PreparedImport, String>,
    session: String,
    generation: u64,
) -> ResultValue {
    match result {
        Ok(prepared) => ResultValue::Import {
            session,
            generation,
            prepared: Box::new(prepared),
        },
        Err(error) => ResultValue::Failed(format!("Import failed: {error}")),
    }
}
pub struct UncertainExport {
    writer: WriterSession,
    bytes: Vec<u8>,
    session: String,
    operation: uuid::Uuid,
    pub path: PathBuf,
    pub stage: PathBuf,
    pub backup: PathBuf,
    pub error: String,
}
pub struct WriteContext {
    pub session: String,
    pub path: PathBuf,
    pub operation: uuid::Uuid,
    pub digest: [u8; 32],
    pub bytes: Vec<u8>,
}
pub struct Effects {
    pending: Option<mpsc::Receiver<ResultValue>>,
    write_context: Option<Box<WriteContext>>,
    /// What the running worker does, shown in the status bar.
    pub label: &'static str,
}
impl Effects {
    pub fn new() -> Self {
        Self {
            pending: None,
            write_context: None,
            label: "",
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    fn start(
        &mut self,
        ctx: egui::Context,
        label: &'static str,
        work: impl FnOnce() -> ResultValue + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.label = label;
        std::thread::spawn(move || {
            let result = work();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self) -> Option<ResultValue> {
        match self.pending.as_ref()?.try_recv() {
            Ok(value) => {
                self.pending = None;
                self.write_context = None;
                Some(value)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                Some(match self.write_context.take() {
                    Some(context) => ResultValue::LostWrite(context),
                    None => ResultValue::Failed("Interchange worker stopped unexpectedly.".into()),
                })
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
    pub fn open(&mut self, ctx: egui::Context, path: PathBuf, session: String, generation: u64) {
        self.start(
            ctx,
            "Opening project…",
            move || match crate::controller::load_project(&path) {
                Ok(opened) => ResultValue::Opened {
                    session,
                    generation,
                    opened: Box::new(opened),
                },
                Err(error) => ResultValue::Failed(error),
            },
        );
    }
    pub fn import(&mut self, ctx: egui::Context, path: PathBuf, session: String, generation: u64) {
        self.start(ctx, "Importing BPMN…", move || {
            let result = FileGrant::new(&path, Access::ReadOnly)
                .and_then(|g| g.read_limited(IMPORT_LIMIT))
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    interchange::prepare_import(
                        &bytes,
                        &path.file_stem().unwrap_or_default().to_string_lossy(),
                    )
                    .map_err(|e| e.detail)
                });
            import_result(result, session, generation)
        });
    }
    /// Import BPMN XML held in memory (pasted from the OS clipboard) through the same path.
    pub fn import_bytes(
        &mut self,
        ctx: egui::Context,
        bytes: Vec<u8>,
        name: String,
        session: String,
        generation: u64,
    ) {
        self.start(ctx, "Importing BPMN…", move || {
            let result = if bytes.len() > IMPORT_LIMIT {
                Err("the BPMN text is larger than 32 MiB".into())
            } else {
                interchange::prepare_import(&bytes, &name).map_err(|e| e.detail)
            };
            import_result(result, session, generation)
        });
    }
    pub fn export(
        &mut self,
        ctx: egui::Context,
        project: Arc<ProjectData>,
        doc: DocumentId,
        diagram: Option<ElementSelector>,
        format: ExportKind,
        appearance: Appearance,
    ) {
        self.start(ctx, "Preparing export…", move || {
            if let ExportKind::Drawio = format {
                let Some(document) = project.documents.iter().find(|d| d.id == doc) else {
                    return ResultValue::Failed("Document not found.".into());
                };
                let Some(diagram) = diagram else {
                    return ResultValue::Failed("No diagram selected.".into());
                };
                return match processcraft_io::drawio::export_drawio(document, &diagram) {
                    Ok(out) => ResultValue::Export {
                        bytes: out.bytes,
                        report: out
                            .approximated
                            .into_iter()
                            .map(|e| format!("Drawn as plain geometry: {e}"))
                            .collect(),
                        extension: "drawio",
                    },
                    Err(e) => ResultValue::Failed(format!("Export failed: {e}")),
                };
            }
            if let ExportKind::Image(format) = format {
                let Some(document) = project.documents.iter().find(|d| d.id == doc) else {
                    return ResultValue::Failed("Document not found.".into());
                };
                let Some(diagram) = diagram else {
                    return ResultValue::Failed("No diagram selected.".into());
                };
                let mut fonts = TextEngine::new();
                match processcraft_render::export_diagram(
                    document,
                    &diagram,
                    ExportOptions {
                        format,
                        appearance: if format == ExportFormat::Pdf {
                            Appearance::Light
                        } else {
                            appearance
                        },
                        scale: 1.0,
                        acknowledge_diagnostics: true,
                    },
                    &mut fonts,
                    "Noto Sans",
                ) {
                    Ok(out) => ResultValue::Export {
                        bytes: out.bytes,
                        report: out.diagnostics,
                        extension: match format {
                            ExportFormat::Svg => "svg",
                            ExportFormat::Png => "png",
                            ExportFormat::Pdf => "pdf",
                        },
                    },
                    Err(e) => ResultValue::Failed(format!("Export failed: {e}")),
                }
            } else {
                match interchange::export_bpmn(&project, doc, true) {
                    Ok(out) => {
                        let mut report = out.report.omitted;
                        report.extend(out.report.transformed);
                        ResultValue::Export {
                            bytes: out.bytes,
                            report,
                            extension: "bpmn",
                        }
                    }
                    Err(e) => ResultValue::Failed(format!(
                        "Export blocked: {}",
                        e.report.blocked.join("; ")
                    )),
                }
            }
        });
    }
    pub fn write(&mut self, ctx: egui::Context, grant: FileGrant, bytes: Vec<u8>, session: String) {
        let path = grant.path().to_path_buf();
        let operation = uuid::Uuid::new_v4();
        self.write_context = Some(Box::new(WriteContext {
            session: session.clone(),
            path: path.clone(),
            operation,
            digest: Sha256::digest(&bytes).into(),
            bytes: bytes.clone(),
        }));
        self.start(ctx, "Writing export…", move || {
            let mut writer = match WriterSession::open(&grant) {
                Ok(OpenSession::Writable(writer)) => writer,
                Ok(OpenSession::ReadOnly) => {
                    return ResultValue::Failed("Target is locked by another writer.".into());
                }
                Err(e) => return ResultValue::Failed(format!("Export failed: {e}")),
            };
            let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                writer.save(&bytes)
            })) {
                Ok(outcome) => outcome,
                Err(_) => {
                    return ResultValue::Indeterminate(Box::new(UncertainExport {
                        writer,
                        bytes,
                        session,
                        operation,
                        path,
                        stage: PathBuf::from("unknown"),
                        backup: PathBuf::from("unknown"),
                        error: "Export worker panicked during the write; outcome unknown".into(),
                    }));
                }
            };
            match outcome {
                SaveOutcome::Committed { warning } => ResultValue::Written(format!(
                    "Exported {}{}",
                    path.display(),
                    warning.map_or(String::new(), |w| format!("; warning: {w}"))
                )),
                SaveOutcome::NotCommitted { error } => {
                    ResultValue::Failed(format!("Export was not committed: {error}"))
                }
                SaveOutcome::Indeterminate {
                    error,
                    stage,
                    backup,
                } => ResultValue::Indeterminate(Box::new(UncertainExport {
                    writer,
                    bytes,
                    session,
                    operation,
                    path,
                    stage,
                    backup,
                    error: error.to_string(),
                })),
            }
        });
    }
    pub fn reconcile(&mut self, ctx: egui::Context, mut pending: Box<UncertainExport>) {
        self.write_context = Some(Box::new(WriteContext {
            session: pending.session.clone(),
            path: pending.path.clone(),
            operation: pending.operation,
            digest: Sha256::digest(&pending.bytes).into(),
            bytes: pending.bytes.clone(),
        }));
        self.start(
            ctx,
            "Checking export outcome…",
            move || match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pending.writer.reconcile(&pending.bytes)
            })) {
                Ok(Ok(true)) => {
                    ResultValue::Written(format!("Export confirmed: {}", pending.path.display()))
                }
                Ok(Ok(false)) => ResultValue::Failed(format!(
                    "Export was not committed: {}",
                    pending.path.display()
                )),
                Ok(Err(error)) => {
                    pending.error = error.to_string();
                    ResultValue::Indeterminate(pending)
                }
                Err(_) => {
                    pending.error = "Checking the export outcome failed unexpectedly".into();
                    ResultValue::Indeterminate(pending)
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_write_retains_origin_and_intended_bytes() {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let path = PathBuf::from("test-export.bpmn");
        let mut effects = Effects {
            pending: Some(receiver),
            write_context: Some(Box::new(WriteContext {
                session: "session".into(),
                path: path.clone(),
                operation: uuid::Uuid::new_v4(),
                digest: [7; 32],
                bytes: vec![1, 2, 3],
            })),
            label: "",
        };
        let Some(ResultValue::LostWrite(context)) = effects.poll() else {
            panic!("write outcome must remain unknown")
        };
        assert_eq!(context.path, path);
        assert_eq!(context.session, "session");
        assert_eq!(context.bytes, vec![1, 2, 3]);
        assert_eq!(context.digest, [7; 32]);
    }
    #[test]
    fn failed_import_releases_worker_and_reports_error() {
        let mut effects = Effects::new();
        let path = std::env::temp_dir().join(format!("missing-{}.bpmn", uuid::Uuid::new_v4()));
        effects.import(egui::Context::default(), path, "session".into(), 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let result = loop {
            if let Some(result) = effects.poll() {
                break result;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert!(matches!(result, ResultValue::Failed(_)));
        assert!(!effects.busy());
    }
}
