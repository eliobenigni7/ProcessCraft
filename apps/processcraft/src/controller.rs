use processcraft_core::{
    commands::{ChangeSet, Envelope, Mutation, Request, Session},
    ids::XmlId,
    model::*,
};
use processcraft_io::{
    Access, FileGrant,
    project::{FileSession, ProjectSaveOutcome},
};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};

pub fn id(prefix: &str) -> XmlId {
    format!("{}_{}", prefix, uuid::Uuid::new_v4().simple())
        .parse()
        .unwrap()
}
pub fn blank_document() -> BpmnDocument {
    let mut d = BpmnDocument::new("Untitled process".into(), id("Definitions").into());
    let process = Process {
        activity_metadata: None,
        lane_set_base: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("Process").into(),
        name: Some("Process".into()),
        documentation: String::new().into(),
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    };
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        label_styles: Vec::new(),
        resolution: None,
        name: None,
        id: id("Diagram").into(),
        plane_id: id("Plane").into(),
        plane: process.id.selector(),
        shapes: vec![],
        edges: vec![],
    });
    d.processes.push(process);
    d
}
pub fn blank_project() -> ProjectData {
    let mut p = ProjectData::new("Untitled project".into());
    p.documents.push(Arc::new(blank_document()));
    p
}
struct SaveContext {
    job: uuid::Uuid,
    session_id: String,
    target: PathBuf,
    operation: processcraft_io::project::Operation,
    content_id: [u8; 32],
}
struct SaveResult {
    job: uuid::Uuid,
    digest: Option<[u8; 32]>,
    session_id: String,
    file: Option<FileSession>,
    saved: Option<processcraft_io::project::SaveReceipt>,
    details: Option<UncertainSave>,
    error: String,
    uncertain: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct UncertainSave {
    pub attempted: processcraft_io::project::SaveReceipt,
    pub stage: PathBuf,
    pub backup: PathBuf,
}
#[derive(Debug, Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct SaveReport {
    pub receipt: Option<processcraft_io::project::SaveReceipt>,
    pub uncertain: bool,
    pub error: String,
    pub details: Option<UncertainSave>,
}
pub struct Controller {
    pub session: Session,
    last_save_report: Option<SaveReport>,
    pub file: Option<FileSession>,
    pub migration: bool,
    pub error: String,
    pub uncertain: bool,
    pending: Option<std::sync::mpsc::Receiver<SaveResult>>,
    unmatched: Vec<SaveResult>,
    pending_context: Option<SaveContext>,
    pending_origin: Option<processcraft_io::recovery::RecoveryOrigin>,
}
impl Default for Controller {
    fn default() -> Self {
        let mut session = Session::new(blank_project()).unwrap();
        session.mark_unsaved();
        Self {
            session,
            last_save_report: None,
            file: None,
            migration: false,
            error: String::new(),
            uncertain: false,
            pending: None,
            unmatched: Vec::new(),
            pending_context: None,
            pending_origin: None,
        }
    }
}
impl Controller {
    pub fn apply_changes(&mut self, request: Request) -> Option<ChangeSet> {
        self.apply_changes_at_generation(self.session.snapshot().generation, request)
    }
    pub fn apply_changes_at_generation(
        &mut self,
        generation: u64,
        request: Request,
    ) -> Option<ChangeSet> {
        let env = Envelope {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: self.session.id(),
            expected_generation: Some(generation),
        };
        match self.session.apply(env, request) {
            Ok(changes) => Some(changes),
            Err(e) => {
                self.error = format!("Change could not be applied: {e:?}");
                None
            }
        }
    }
    pub fn apply(&mut self, request: Request) -> bool {
        self.apply_changes(request).is_some()
    }
    pub fn mutate_changes(&mut self, mutations: Vec<Mutation>) -> Option<ChangeSet> {
        self.apply_changes(Request::Batch { mutations })
    }
    pub fn mutate(&mut self, mutations: Vec<Mutation>) -> bool {
        self.mutate_changes(mutations).is_some()
    }
    pub fn mutate_at_generation(&mut self, generation: u64, mutations: Vec<Mutation>) -> bool {
        self.apply_changes_at_generation(generation, Request::Batch { mutations })
            .is_some()
    }
    pub fn open(&mut self, path: PathBuf) {
        let grant = match FileGrant::new(&path, Access::Write) {
            Ok(g) => g,
            Err(e) => {
                self.error = e.to_string();
                return;
            }
        };
        let temporary = Session::new(blank_project()).unwrap();
        match FileSession::open(temporary.id(), grant) {
            Ok((file, loaded)) => match Session::new(loaded.project) {
                Ok(session) => {
                    // FileSession receipts must carry the active session identity; reopen with that identity.
                    drop(file);
                    match FileGrant::new(&path, Access::Write)
                        .and_then(|g| FileSession::open(session.id(), g))
                    {
                        Ok((file, reloaded))
                            if reloaded.project.content_identity().ok()
                                == Some(session.snapshot().content_id) =>
                        {
                            self.session = session;
                            self.file = Some(file);
                            self.migration = reloaded.requires_save_as_or_backup;
                            self.error.clear();
                        }
                        Ok(_) => {
                            self.error =
                                "The project changed while opening. Please try again.".into()
                        }
                        Err(e) => self.error = e.to_string(),
                    }
                }
                Err(e) => self.error = format!("Invalid project: {e:?}"),
            },
            Err(e) => self.error = format!("Could not open project: {e}"),
        }
    }
    pub fn reconcile(&mut self) {
        let Some(file) = &mut self.file else { return };
        match file.reconcile() {
            Ok(Some(receipt)) => {
                self.session.mark_saved(receipt.saved_content_id);
                self.uncertain = false;
                self.migration = false;
                self.error = "Save confirmed.".into();
            }
            Ok(None) => {
                self.uncertain = false;
                self.error = "The save did not complete. Please save again.".into();
            }
            Err(e) => self.error = format!("Save outcome remains uncertain: {e}"),
        }
    }
    pub fn recovery_origin(&self) -> processcraft_io::recovery::RecoveryOrigin {
        self.file
            .as_ref()
            .map(|f| processcraft_io::recovery::RecoveryOrigin {
                original_path: Some(f.target().to_path_buf()),
                observed_disk_digest: f.observed_disk_digest(),
            })
            .or_else(|| self.pending_origin.clone())
            .unwrap_or(processcraft_io::recovery::RecoveryOrigin {
                original_path: None,
                observed_disk_digest: None,
            })
    }
    pub fn can_transition(&self) -> bool {
        self.pending.is_none() && !self.uncertain && self.unmatched.is_empty()
    }
    pub fn is_saving(&self) -> bool {
        self.pending.is_some()
    }
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn take_save_report(&mut self) -> Option<SaveReport> {
        self.last_save_report.take()
    }
    pub fn poll_save(&mut self) -> Option<bool> {
        let received = self.pending.as_ref()?.try_recv();
        match received {
            Ok(result) => {
                self.pending = None;
                let mismatch = match (&self.pending_context, &result.saved) {
                    (Some(c), Some(r)) => {
                        result.job != c.job
                            || result.session_id != c.session_id
                            || c.session_id != self.session.id()
                            || r.session_id != c.session_id
                            || r.operation_id.is_nil()
                            || r.operation != c.operation
                            || r.target != c.target
                            || r.saved_content_id != c.content_id
                            || Some(r.disk_digest) != result.digest
                            || result.file.as_ref().is_none_or(|f| f.target() != r.target)
                    }
                    (Some(c), None) => {
                        result.job != c.job
                            || result.session_id != c.session_id
                            || c.session_id != self.session.id()
                    }
                    _ => true,
                };
                if mismatch {
                    self.error="A save acknowledgement does not match its origin; the write record is retained.".into();
                    self.uncertain = true;
                    self.unmatched.push(result);
                    self.last_save_report = Some(SaveReport {
                        receipt: None,
                        uncertain: true,
                        error: self.error.clone(),
                        details: None,
                    });
                    return Some(false);
                }
                self.pending_context = None;
                self.pending_origin = None;
                self.file = result.file;
                self.uncertain = result.uncertain;
                self.error = result.error;
                self.last_save_report = Some(SaveReport {
                    receipt: result.saved.clone(),
                    uncertain: self.uncertain,
                    error: self.error.clone(),
                    details: result.details.clone(),
                });
                if let Some(receipt) = result.saved {
                    self.session.mark_saved(receipt.saved_content_id);
                    self.migration = false;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.error = "Save worker stopped unexpectedly; save outcome is unknown.".into();
                self.uncertain = true;
                self.last_save_report = Some(SaveReport {
                    receipt: None,
                    uncertain: true,
                    error: self.error.clone(),
                    details: None,
                });
                Some(false)
            }
        }
    }
    pub fn save(&mut self, as_path: Option<PathBuf>) -> bool {
        self.save_inner(as_path, None)
    }
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn save_granted(&mut self, grant: FileGrant) -> bool {
        self.save_inner(Some(grant.path().to_path_buf()), Some(grant))
    }
    fn save_inner(&mut self, as_path: Option<PathBuf>, granted: Option<FileGrant>) -> bool {
        if self.pending.is_some() || self.uncertain {
            self.error = "Wait for the current save to settle.".into();
            return false;
        }
        self.last_save_report = None;
        let snapshot = self.session.snapshot();
        let content_id = snapshot.content_id;
        let project = snapshot.project;
        let session_id = self.session.id();
        self.pending_origin =
            self.file
                .as_ref()
                .map(|f| processcraft_io::recovery::RecoveryOrigin {
                    original_path: Some(f.target().to_path_buf()),
                    observed_disk_digest: f.observed_disk_digest(),
                });
        let mut file = self.file.take();
        let initial = file.is_none();
        if initial && as_path.is_none() {
            self.error = "Choose Save As to save this project.".into();
            return false;
        }
        let job = uuid::Uuid::new_v4();
        let expected_target = as_path
            .clone()
            .or_else(|| file.as_ref().map(|f| f.target().to_path_buf()))
            .expect("target available");
        let operation = if !initial && as_path.is_some() {
            processcraft_io::project::Operation::SaveAs
        } else {
            processcraft_io::project::Operation::Save
        };
        self.pending_context = Some(SaveContext {
            job,
            session_id: session_id.clone(),
            target: expected_target,
            operation,
            content_id,
        });
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let digest = processcraft_io::native::write_bytes(&project)
                .ok()
                .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)));
            let mut target = as_path;
            let mut granted = granted;
            if initial {
                let path = target.take().unwrap();
                let grant = match granted.take() {
                    Some(grant) => Ok(grant),
                    None => FileGrant::new(path, Access::Write),
                };
                let result = grant.and_then(|g| FileSession::create(session_id.clone(), g));
                match result {
                    Ok(f) => file = Some(f),
                    Err(e) => {
                        let _ = tx.send(SaveResult {
                            job,
                            digest,
                            session_id,
                            file: None,
                            saved: None,
                            details: None,
                            error: format!("Save failed: {e}"),
                            uncertain: false,
                        });
                        return;
                    }
                }
            }
            let f = file.as_mut().unwrap();
            let outcome = if let Some(path) = target {
                match granted
                    .take()
                    .map_or_else(|| FileGrant::new(path, Access::Write), Ok)
                {
                    Ok(g) => f.save_as(g, &project),
                    Err(e) => ProjectSaveOutcome::NotCommitted { error: e },
                }
            } else {
                f.save(&project)
            };
            let mut details = None;
            let (saved, error, uncertain) = match outcome {
                ProjectSaveOutcome::Committed { receipt, warning } => (
                    Some(receipt),
                    warning
                        .map(|w| format!("Saved with warning: {w}"))
                        .unwrap_or_default(),
                    false,
                ),
                ProjectSaveOutcome::NotCommitted { error } => {
                    (None, format!("Save failed: {error}"), false)
                }
                ProjectSaveOutcome::Indeterminate {
                    error,
                    attempted,
                    stage,
                    backup,
                } => match f.reconcile() {
                    Ok(Some(r)) => (
                        Some(r),
                        format!("Save confirmed after uncertainty: {error}"),
                        false,
                    ),
                    Ok(None) => (None, format!("Save did not complete: {error}"), false),
                    Err(e) => {
                        details = Some(UncertainSave {
                            attempted,
                            stage,
                            backup,
                        });
                        (None, format!("Save outcome uncertain: {error}; {e}"), true)
                    }
                },
            };
            if initial && saved.is_none() && !uncertain {
                file = None;
            }
            let _ = tx.send(SaveResult {
                job,
                digest,
                session_id,
                file,
                saved,
                details,
                error,
                uncertain,
            });
        });
        true
    }
}

#[cfg(test)]
mod lifecycle {
    use super::*;
    #[test]
    fn newer_edits_stay_dirty() {
        let mut ctl = Controller::default();
        assert!(ctl.mutate(vec![Mutation::RenameProject { name: "one".into() }]));
        let old = ctl.session.snapshot().content_id;
        assert!(ctl.mutate(vec![Mutation::RenameProject { name: "two".into() }]));
        ctl.session.mark_saved(old);
        assert!(ctl.session.snapshot().dirty);
    }
    #[cfg(windows)]
    #[test]
    fn old_save_acknowledgement_is_reconciled() {
        let dir = std::env::temp_dir().join(format!("processcraft-old-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let mut ctl = Controller::default();
        assert!(ctl.mutate(vec![Mutation::RenameProject {
            name: "first".into()
        }]));
        assert!(ctl.save(Some(dir.join("test.pc"))));
        assert!(ctl.mutate(vec![Mutation::RenameProject {
            name: "second".into()
        }]));
        let reply = ctl
            .pending
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(reply).unwrap();
        ctl.pending = Some(rx);
        assert_eq!(ctl.poll_save(), Some(true));
        assert!(ctl.session.snapshot().dirty);
        drop(ctl);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn creating_pool_and_shape_is_valid() {
        let mut ctl = Controller::default();
        let snap = ctl.session.snapshot();
        let d = &snap.project.documents[0];
        let p = &d.processes[0];
        let diagram = &d.diagrams[0];
        let participant = id("Participant");
        let ok = ctl.mutate(vec![
            Mutation::CreateCollaboration {
                document_id: d.id,
                collaboration: Collaboration {
                    kind: None,
                    extras: None,
                    correlation_keys: Vec::new(),
                    base: None,
                    name: None,
                    conversation_nodes: vec![],
                    conversation_links: vec![],
                    id: id("Collaboration").into(),
                    participants: vec![],
                    flows: vec![],
                },
            },
            Mutation::CreateParticipant {
                document_id: d.id,
                participant: Participant {
                    interface_refs: vec![],
                    endpoint_refs: vec![],
                    multiplicity: None,
                    base: None,
                    id: participant.clone().into(),
                    name: Some("Pool".into()),
                    process: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                        p.id.selector(),
                    )),
                },
            },
            Mutation::CreateShape {
                document_id: d.id,
                diagram_id: diagram.id.selector(),
                shape: Shape {
                    di_metadata: None,
                    is_message_visible: None,
                    participant_band_kind: None,
                    choreography_activity_shape: None,
                    label_visibility: None,
                    is_horizontal: None,
                    is_marker_visible: None,
                    is_expanded: None,
                    id: id("Shape").into(),
                    element: participant.into(),
                    bounds: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: 700.0,
                        height: 350.0,
                    },
                    label: None,
                    style: None,
                },
            },
        ]);
        assert!(ok, "{}", ctl.error);
    }
    #[cfg(windows)]
    #[test]
    fn asynchronous_first_save_writes_captured_snapshot() {
        let dir =
            std::env::temp_dir().join(format!("processcraft-desktop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("test.pc");
        let mut ctl = Controller::default();
        assert!(ctl.save(Some(path.clone())));
        let reply = ctl
            .pending
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(reply).unwrap();
        ctl.pending = Some(rx);
        assert_eq!(ctl.poll_save(), Some(true));
        assert!(!ctl.session.snapshot().dirty);
        assert!(path.exists());
        let origin = ctl.recovery_origin();
        assert_eq!(origin.original_path.as_ref(), Some(&path));
        assert_eq!(
            origin.observed_disk_digest,
            Some(Sha256::digest(std::fs::read(&path).unwrap()).into())
        );
        drop(ctl);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn closed_session_keeps_pending_write_record() {
        let mut ctl = Controller::default();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        ctl.pending = Some(rx);
        assert!(!ctl.can_transition());
        tx.send(SaveResult {
            job: uuid::Uuid::new_v4(),
            digest: None,
            session_id: "different-session".into(),
            file: None,
            saved: None,
            details: None,
            error: String::new(),
            uncertain: false,
        })
        .unwrap();
        assert_eq!(ctl.poll_save(), Some(false));
        assert_eq!(ctl.unmatched.len(), 1);
        assert!(!ctl.can_transition());
    }
    #[cfg(windows)]
    #[test]
    fn forged_receipt_is_retained_and_does_not_mark_saved() {
        let dir =
            std::env::temp_dir().join(format!("processcraft-forged-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let mut ctl = Controller::default();
        assert!(ctl.save(Some(dir.join("test.pc"))));
        let mut result = ctl
            .pending
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap();
        result.saved.as_mut().unwrap().saved_content_id = [7; 32];
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(result).unwrap();
        ctl.pending = Some(rx);
        assert_eq!(ctl.poll_save(), Some(false));
        assert!(ctl.session.snapshot().dirty);
        assert_eq!(ctl.unmatched.len(), 1);
        assert!(!ctl.can_transition());
        drop(ctl);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn blank_document_has_valid_process_plane() {
        let document = blank_document();
        assert_eq!(document.diagrams[0].plane, document.processes[0].id);
        assert!(Session::new(blank_project()).is_ok());
    }
}
