//! Ownership and commit coordination for a native project file.
use crate::{Access, FileGrant, OpenSession, SaveOutcome, SavePoint, WriterSession, native};
use processcraft_core::model::ProjectData;
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Save,
    SaveAs,
}
#[derive(Debug, Clone)]
pub struct SaveReceipt {
    pub operation_id: uuid::Uuid,
    pub session_id: String,
    pub operation: Operation,
    pub target: PathBuf,
    pub saved_content_id: [u8; 32],
    pub disk_digest: [u8; 32],
}
#[derive(Debug)]
pub enum ProjectSaveOutcome {
    NotCommitted {
        error: io::Error,
    },
    Committed {
        receipt: SaveReceipt,
        warning: Option<io::Error>,
    },
    Indeterminate {
        attempted: SaveReceipt,
        error: io::Error,
        stage: PathBuf,
        backup: PathBuf,
    },
}
struct Binding {
    grant: FileGrant,
    writer: Option<WriterSession>,
    migrated: bool,
}
struct Pending {
    binding: Option<Binding>,
    bytes: Vec<u8>,
    receipt: SaveReceipt,
}
pub struct FileSession {
    session_id: String,
    binding: Binding,
    pending: Option<Pending>,
    observed_digest: Option<[u8; 32]>,
}

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
fn binding(grant: FileGrant) -> io::Result<Binding> {
    let writer = match WriterSession::open(&grant)? {
        OpenSession::Writable(w) => Some(w),
        OpenSession::ReadOnly => None,
    };
    Ok(Binding {
        grant,
        writer,
        migrated: false,
    })
}
impl FileSession {
    /// Bind an authorized first-save target without writing the project file.
    /// Existing targets require the caller to have obtained overwrite authorization.
    /// A cooperative writer already holding the target yields WouldBlock.
    pub fn create(session_id: impl Into<String>, grant: FileGrant) -> io::Result<Self> {
        if grant.access != Access::Write {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "write grant required",
            ));
        }
        let binding = binding(grant)?;
        if binding.writer.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "target already has a writer",
            ));
        }
        let observed_digest = binding
            .writer
            .as_ref()
            .and_then(|w| w.snapshot())
            .map(|b| Sha256::digest(b).into());
        Ok(Self {
            session_id: session_id.into(),
            binding,
            pending: None,
            observed_digest,
        })
    }

    /// The writer lock is obtained before the snapshot used for the editable load.
    pub fn open(
        session_id: impl Into<String>,
        grant: FileGrant,
    ) -> io::Result<(Self, native::LoadedProject)> {
        let mut binding = binding(grant)?;
        let bytes = if let Some(w) = &binding.writer {
            w.snapshot()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "project missing"))?
                .to_vec()
        } else {
            binding.grant.read()?
        };
        let observed_digest = Some(Sha256::digest(&bytes).into());
        let loaded = native::load_bytes(&bytes).map_err(invalid)?;
        binding.migrated = loaded.requires_save_as_or_backup;
        Ok((
            Self {
                session_id: session_id.into(),
                binding,
                pending: None,
                observed_digest,
            },
            loaded,
        ))
    }
    /// Digest of the last observed or confirmed bound file bytes, not a live disk probe.
    pub fn observed_disk_digest(&self) -> Option<[u8; 32]> {
        self.observed_digest
    }
    pub fn same_target(&self, grant: &FileGrant) -> io::Result<bool> {
        self.binding.grant.same_target(grant)
    }
    pub fn target(&self) -> &Path {
        self.binding.grant.path()
    }
    pub fn is_writable(&self) -> bool {
        self.binding.writer.is_some()
    }
    pub fn save(&mut self, project: &ProjectData) -> ProjectSaveOutcome {
        self.save_with_hook(project, |_, _| Ok(()))
    }
    pub fn save_with_hook(
        &mut self,
        project: &ProjectData,
        hook: impl FnMut(SavePoint, &Path) -> io::Result<()>,
    ) -> ProjectSaveOutcome {
        if self.binding.migrated {
            return fail("migrated project requires Save As");
        }
        self.perform(project, None, hook)
    }
    pub fn save_as(&mut self, grant: FileGrant, project: &ProjectData) -> ProjectSaveOutcome {
        self.save_as_with_hook(grant, project, |_, _| Ok(()))
    }
    pub fn save_as_with_hook(
        &mut self,
        grant: FileGrant,
        project: &ProjectData,
        hook: impl FnMut(SavePoint, &Path) -> io::Result<()>,
    ) -> ProjectSaveOutcome {
        if self.pending.is_some() {
            return fail("reconciliation required");
        }
        if grant.canonical_path == self.binding.grant.canonical_path {
            return fail("Save As requires a different target");
        }
        if grant.access != Access::Write {
            return fail("write grant required");
        }
        let binding = match binding(grant) {
            Ok(b) if b.writer.is_some() => b,
            Ok(_) => return fail("target already has a writer"),
            Err(error) => return ProjectSaveOutcome::NotCommitted { error },
        };
        self.perform(project, Some(binding), hook)
    }
    fn perform(
        &mut self,
        project: &ProjectData,
        mut destination: Option<Binding>,
        mut hook: impl FnMut(SavePoint, &Path) -> io::Result<()>,
    ) -> ProjectSaveOutcome {
        if self.pending.is_some() {
            return fail("reconciliation required");
        }
        let content_id = match project.content_identity() {
            Ok(v) => v,
            Err(e) => return ProjectSaveOutcome::NotCommitted { error: invalid(e) },
        };
        let bytes = match native::write_bytes(project) {
            Ok(v) => v,
            Err(e) => return ProjectSaveOutcome::NotCommitted { error: invalid(e) },
        };
        let operation = if destination.is_some() {
            Operation::SaveAs
        } else {
            Operation::Save
        };
        let b = destination.as_mut().unwrap_or(&mut self.binding);
        let receipt = SaveReceipt {
            operation_id: uuid::Uuid::new_v4(),
            session_id: self.session_id.clone(),
            operation,
            target: b.grant.path().to_path_buf(),
            saved_content_id: content_id,
            disk_digest: Sha256::digest(&bytes).into(),
        };
        let Some(writer) = &mut b.writer else {
            return fail("read-only session");
        };
        let outcome = writer.save_with_hook(&bytes, |point, path| {
            hook(point, path)?;
            if point == SavePoint::BeforeReplace {
                let staged = crate::save::read_target(path)?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "stage missing"))?;
                let loaded = native::load_bytes(&staged).map_err(invalid)?;
                if loaded.project.content_identity().map_err(invalid)? != content_id {
                    return Err(invalid("stage identity mismatch"));
                }
            }
            Ok(())
        });
        self.finish_save(outcome, destination, bytes, receipt)
    }
    fn finish_save(
        &mut self,
        outcome: SaveOutcome,
        destination: Option<Binding>,
        bytes: Vec<u8>,
        receipt: SaveReceipt,
    ) -> ProjectSaveOutcome {
        match outcome {
            SaveOutcome::NotCommitted { error } => ProjectSaveOutcome::NotCommitted { error },
            SaveOutcome::Committed { warning } => {
                if let Some(b) = destination {
                    self.binding = b;
                }
                self.observed_digest = Some(receipt.disk_digest);
                ProjectSaveOutcome::Committed { receipt, warning }
            }
            SaveOutcome::Indeterminate {
                error,
                stage,
                backup,
            } => {
                self.pending = Some(Pending {
                    binding: destination,
                    bytes,
                    receipt: receipt.clone(),
                });
                ProjectSaveOutcome::Indeterminate {
                    attempted: receipt,
                    error,
                    stage,
                    backup,
                }
            }
        }
    }
    /// Compare the target to both intended and prior bytes. Never retry blindly.
    pub fn reconcile(&mut self) -> io::Result<Option<SaveReceipt>> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| io::Error::other("no pending save"))?;
        let binding = pending.binding.as_mut().unwrap_or(&mut self.binding);
        let committed = binding
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("missing writer"))?
            .reconcile(&pending.bytes)?;
        let pending = self.pending.take().expect("pending exists");
        if committed {
            if let Some(b) = pending.binding {
                self.binding = b;
            }
            self.observed_digest = Some(pending.receipt.disk_digest);
            Ok(Some(pending.receipt))
        } else {
            Ok(None)
        }
    }
}
fn fail(message: &str) -> ProjectSaveOutcome {
    ProjectSaveOutcome::NotCommitted {
        error: io::Error::other(message.to_owned()),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn indeterminate_save_requires_reconciliation_and_keeps_operation_id() {
        let dir = std::env::temp_dir().join(format!("processcraft-coord-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("project.pc");
        let original = ProjectData::new("original".into());
        fs::write(&path, native::write_bytes(&original).unwrap()).unwrap();
        let (mut session, _) =
            FileSession::open("session", FileGrant::new(&path, Access::Write).unwrap()).unwrap();
        let mut changed = original;
        changed.name = "changed".into();
        let bytes = native::write_bytes(&changed).unwrap();
        let receipt = SaveReceipt {
            operation_id: uuid::Uuid::new_v4(),
            session_id: "session".into(),
            operation: Operation::Save,
            target: path.clone(),
            saved_content_id: changed.content_identity().unwrap(),
            disk_digest: Sha256::digest(&bytes).into(),
        };
        let outcome = session.binding.writer.as_mut().unwrap().save_impl(
            &bytes,
            &mut |_, _| Ok(()),
            |stage, target, backup, _| {
                fs::rename(target, backup)?;
                fs::rename(stage, target)?;
                Err(io::Error::other("injected late failure"))
            },
        );
        let ProjectSaveOutcome::Indeterminate { attempted, .. } =
            session.finish_save(outcome, None, bytes, receipt.clone())
        else {
            panic!("expected uncertainty")
        };
        assert_eq!(attempted.operation_id, receipt.operation_id);
        assert!(matches!(
            session.save(&changed),
            ProjectSaveOutcome::NotCommitted { .. }
        ));
        let reconciled = session.reconcile().unwrap().unwrap();
        assert_eq!(reconciled.operation_id, attempted.operation_id);
        assert_eq!(reconciled.session_id, attempted.session_id);
        assert_eq!(reconciled.target, attempted.target);
        drop(session);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn identical_bytes_cannot_reconcile_or_rebind_save_as() {
        let dir = std::env::temp_dir().join(format!("processcraft-equal-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let source = dir.join("source.pc");
        let target = dir.join("target.pc");
        let project = ProjectData::new("same".into());
        let bytes = native::write_bytes(&project).unwrap();
        fs::write(&source, &bytes).unwrap();
        fs::write(&target, &bytes).unwrap();
        let (mut session, _) =
            FileSession::open("session", FileGrant::new(&source, Access::Write).unwrap()).unwrap();
        let mut destination = binding(FileGrant::new(&target, Access::Write).unwrap()).unwrap();
        let receipt = SaveReceipt {
            operation_id: uuid::Uuid::new_v4(),
            session_id: "session".into(),
            operation: Operation::SaveAs,
            target: target.clone(),
            saved_content_id: project.content_identity().unwrap(),
            disk_digest: Sha256::digest(&bytes).into(),
        };
        let outcome = destination.writer.as_mut().unwrap().save_impl(
            &bytes,
            &mut |_, _| Ok(()),
            |stage, target, backup, _| {
                fs::copy(target, backup)?;
                let _ = stage;
                Err(io::Error::other("injected ambiguous failure"))
            },
        );
        assert!(matches!(
            session.finish_save(outcome, Some(destination), bytes, receipt),
            ProjectSaveOutcome::Indeterminate { .. }
        ));
        assert!(session.reconcile().is_err());
        assert_eq!(session.target(), source);
        assert!(matches!(
            session.save(&project),
            ProjectSaveOutcome::NotCommitted { .. }
        ));
        drop(session);
        fs::remove_dir_all(dir).unwrap();
    }
}
