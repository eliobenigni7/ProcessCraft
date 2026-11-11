//! Private, bounded recovery snapshots. Metadata is the publication marker.
use crate::{
    Access, FileGrant, FileSession, OpenSession, ProjectSaveOutcome, SaveOutcome, WriterSession,
    native,
};
use processcraft_core::model::ProjectData;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

const LIMIT: u64 = 512 * 1024 * 1024;
const META_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryOrigin {
    pub original_path: Option<PathBuf>,
    pub observed_disk_digest: Option<[u8; 32]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRecord {
    pub id: uuid::Uuid,
    pub project_id: String,
    pub content_id: [u8; 32],
    pub generation: u64,
    #[serde(default)]
    pub written_unix_ms: u64,
    pub origin: RecoveryOrigin,
    pub payload_digest: [u8; 32],
}
pub struct RecoveryStore {
    root: PathBuf,
    _pins: Vec<File>,
    limit: u64,
}
#[cfg(windows)]
fn is_reparse(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}
fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
impl RecoveryStore {
    pub fn default_location() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("ProcessCraft")
            .join("Recovery")
    }
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let root = path.as_ref().to_path_buf();
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute recovery path required",
            ));
        }
        private::create_or_verify(&root)?;
        let pins = crate::save::windows::pin_directories(&root.join("pin"))?;
        Ok(Self {
            root,
            _pins: pins,
            limit: LIMIT,
        })
    }
    fn lock(&self) -> io::Result<File> {
        let lock = crate::save::windows::open_lock(&self.root.join("recovery.lock"))?;
        match lock.try_lock() {
            Ok(()) => Ok(lock),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "recovery store busy",
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
    fn payload(&self, id: uuid::Uuid) -> PathBuf {
        self.root.join(format!("{id}.processcraft"))
    }
    fn metadata(&self, id: uuid::Uuid) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
    fn usage(&self) -> io::Result<u64> {
        let mut total = 0u64;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            if is_reparse(&meta) || !meta.is_file() {
                return Err(invalid("unexpected recovery directory entry"));
            }
            total = total
                .checked_add(meta.len())
                .ok_or_else(|| invalid("recovery size overflow"))?;
        }
        Ok(total)
    }
    pub fn write(
        &self,
        project: &ProjectData,
        origin: RecoveryOrigin,
        generation: u64,
    ) -> io::Result<RecoveryRecord> {
        self.write_with_hook(project, origin, generation, |_, _| Ok(()))
    }
    fn write_with_hook(
        &self,
        project: &ProjectData,
        origin: RecoveryOrigin,
        generation: u64,
        hook: impl FnMut(crate::SavePoint, &Path) -> io::Result<()>,
    ) -> io::Result<RecoveryRecord> {
        let bytes = native::write_bytes(project).map_err(invalid)?;
        let record = RecoveryRecord {
            id: uuid::Uuid::new_v4(),
            project_id: project.id.to_string(),
            content_id: project.content_identity().map_err(invalid)?,
            generation,
            written_unix_ms: u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(invalid)?
                    .as_millis(),
            )
            .map_err(invalid)?,
            origin,
            payload_digest: Sha256::digest(&bytes).into(),
        };
        let metadata = serde_json::to_vec(&record).map_err(invalid)?;
        if metadata.len() > META_LIMIT {
            return Err(invalid("recovery metadata too large"));
        }
        let _lock = self.lock()?;
        let required = (bytes.len() as u64)
            .checked_add(metadata.len() as u64)
            .and_then(|v| v.checked_add(self.usage().ok()?))
            .ok_or_else(|| invalid("recovery quota accounting failed"))?;
        if required > self.limit {
            return Err(io::Error::other("recovery quota exceeded"));
        }
        let path = self.payload(record.id);
        let grant = FileGrant::new(&path, Access::Write)?;
        let mut session = FileSession::create("recovery", grant)?;
        match session.save_with_hook(project, hook) {
            ProjectSaveOutcome::Committed { .. } => {}
            ProjectSaveOutcome::NotCommitted { error }
            | ProjectSaveOutcome::Indeterminate { error, .. } => return Err(error),
        }
        drop(session);
        let grant = FileGrant::new(self.metadata(record.id), Access::Write)?;
        let OpenSession::Writable(mut writer) = WriterSession::open(&grant)? else {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "metadata busy"));
        };
        match writer.save(&metadata) {
            SaveOutcome::Committed { .. } => {}
            SaveOutcome::NotCommitted { error } | SaveOutcome::Indeterminate { error, .. } => {
                return Err(error);
            }
        }
        drop(writer);
        drop(grant);
        self.open_locked(&record)?;
        let mut matching: Vec<_> = self
            .list_locked()?
            .into_iter()
            .filter(|r| r.project_id == record.project_id)
            .collect();
        matching.retain(|r| r.id != record.id);
        matching.sort_by_key(|r| (r.written_unix_ms, r.id));
        while matching.len() > 1 {
            let old = matching.remove(0);
            self.discard_locked(&old)?;
        }
        Ok(record)
    }
    fn open_locked(&self, record: &RecoveryRecord) -> io::Result<ProjectData> {
        let grant = FileGrant::new(self.metadata(record.id), Access::ReadOnly)?;
        let stored: RecoveryRecord =
            serde_json::from_slice(&grant.read_limited(META_LIMIT)?).map_err(invalid)?;
        if &stored != record {
            return Err(invalid("recovery metadata mismatch"));
        }
        let payload = FileGrant::new(self.payload(record.id), Access::ReadOnly)?.read()?;
        if <[u8; 32]>::from(Sha256::digest(&payload)) != record.payload_digest {
            return Err(invalid("recovery digest mismatch"));
        }
        let loaded = native::load_bytes(&payload).map_err(invalid)?;
        let project = loaded.project;
        if project.id.to_string() != record.project_id
            || loaded.source_content_identity != record.content_id
        {
            return Err(invalid("recovery identity mismatch"));
        }
        Ok(project)
    }
    fn list_locked(&self) -> io::Result<Vec<RecoveryRecord>> {
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let Ok(id) = uuid::Uuid::parse_str(stem) else {
                continue;
            };
            if id.to_string() != stem {
                continue;
            }
            let Ok(grant) = FileGrant::new(self.metadata(id), Access::ReadOnly) else {
                continue;
            };
            let Ok(bytes) = grant.read_limited(META_LIMIT) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<RecoveryRecord>(&bytes) else {
                continue;
            };
            if record.id == id && self.open_locked(&record).is_ok() {
                result.push(record);
            }
        }
        result.sort_by_key(|r| std::cmp::Reverse((r.written_unix_ms, r.id)));
        Ok(result)
    }
    pub fn list(&self) -> io::Result<Vec<RecoveryRecord>> {
        let _lock = self.lock()?;
        self.list_locked()
    }
    pub fn open(&self, record: &RecoveryRecord) -> io::Result<ProjectData> {
        let _lock = self.lock()?;
        self.open_locked(record)
    }
    fn discard_locked(&self, record: &RecoveryRecord) -> io::Result<()> {
        self.open_locked(record)?;
        fs::remove_file(self.metadata(record.id))?;
        fs::remove_file(self.payload(record.id))?;
        Ok(())
    }
    pub fn discard(&self, record: &RecoveryRecord) -> io::Result<()> {
        let _lock = self.lock()?;
        self.discard_locked(record)
    }
}
#[cfg(not(windows))]
mod private {
    use std::{io, path::Path};
    pub fn create_or_verify(_: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "recovery requires Windows NTFS",
        ))
    }
}
#[cfg(windows)]
#[allow(unsafe_code)]
mod private;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn store() -> RecoveryStore {
        RecoveryStore::new(
            std::env::temp_dir().join(format!("processcraft-recovery-{}", uuid::Uuid::new_v4())),
        )
        .unwrap()
    }
    fn origin() -> RecoveryOrigin {
        RecoveryOrigin {
            original_path: None,
            observed_disk_digest: None,
        }
    }
    #[test]
    fn snapshots_are_bounded_and_corruption_is_ignored() {
        let mut store = store();
        let mut project = ProjectData::new("one".into());
        let first = store.write(&project, origin(), 1).unwrap();
        project.name = "two".into();
        let second = store.write(&project, origin(), 2).unwrap();
        store.limit = store.usage().unwrap();
        assert!(store.write(&project, origin(), 3).is_err());
        assert_eq!(store.open(&first).unwrap().name, "one");
        store.limit = LIMIT;
        project.name = "three".into();
        let third = store.write(&project, origin(), 3).unwrap();
        assert!(store.open(&first).is_err());
        assert_eq!(store.list().unwrap().len(), 2);
        fs::write(store.payload(third.id), b"corrupt").unwrap();
        assert_eq!(store.list().unwrap(), vec![second]);
        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn busy_and_incomplete_payload_preserve_previous() {
        let store = store();
        let project = ProjectData::new("one".into());
        let record = store.write(&project, origin(), 1).unwrap();
        let lock = store.lock().unwrap();
        assert_eq!(store.list().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(lock);
        fs::write(store.payload(uuid::Uuid::new_v4()), b"interrupted").unwrap();
        assert_eq!(store.list().unwrap(), vec![record.clone()]);
        assert_eq!(store.open(&record).unwrap().name, "one");
        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn original_is_untouched_and_invalid_metadata_is_ignored() {
        let store = store();
        let original =
            std::env::temp_dir().join(format!("processcraft-original-{}", uuid::Uuid::new_v4()));
        fs::write(&original, b"original").unwrap();
        let project = ProjectData::new("snapshot".into());
        let record = store
            .write(
                &project,
                RecoveryOrigin {
                    original_path: Some(original.clone()),
                    observed_disk_digest: None,
                },
                1,
            )
            .unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"original");
        fs::write(store.metadata(record.id), b"{invalid").unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(store.open(&record).is_err());
        fs::remove_file(original).unwrap();
        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn session_generation_reset_and_legacy_timestamp() {
        let store = store();
        let project = ProjectData::new("snapshot".into());
        let first = store.write(&project, origin(), 100).unwrap();
        let second = store.write(&project, origin(), 101).unwrap();
        // Deterministic timestamps avoid relying on clock resolution in the test.
        for (record, timestamp) in [(&first, 1), (&second, 2)] {
            let path = store.metadata(record.id);
            let mut json: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            json["written_unix_ms"] = timestamp.into();
            fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
        }
        let third = store.write(&project, origin(), 0).unwrap();
        let records = store.list().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, third.id);
        assert_eq!(records[1].id, second.id);
        let path = store.metadata(second.id);
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("written_unix_ms");
        fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
        let legacy = store
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == second.id)
            .unwrap();
        assert_eq!(legacy.written_unix_ms, 0);
        assert_eq!(store.open(&legacy).unwrap().name, "snapshot");
        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovery_child_writer() {
        let Some(path) = std::env::var_os("PC_RECOVERY_CHILD_ROOT") else {
            return;
        };
        let ready = PathBuf::from(std::env::var_os("PC_RECOVERY_CHILD_READY").unwrap());
        let store = RecoveryStore::new(path).unwrap();
        let previous = store.list().unwrap().pop().unwrap();
        let mut project = store.open(&previous).unwrap();
        project.name = "unpublished".into();
        let _ = store.write_with_hook(&project, origin(), 2, |point, _| {
            if point == crate::SavePoint::Written {
                fs::write(&ready, b"ready").unwrap();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Ok(())
        });
    }
    #[test]
    fn killed_recovery_writer_preserves_previous_and_original() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let store = store();
        let original = std::env::temp_dir().join(format!("pc-original-{}", uuid::Uuid::new_v4()));
        let ready = std::env::temp_dir().join(format!("pc-ready-{}", uuid::Uuid::new_v4()));
        fs::write(&original, b"original").unwrap();
        let project = ProjectData::new("previous".into());
        let record = store
            .write(
                &project,
                RecoveryOrigin {
                    original_path: Some(original.clone()),
                    observed_disk_digest: None,
                },
                1,
            )
            .unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "recovery::tests::recovery_child_writer",
                "--nocapture",
            ])
            .env("PC_RECOVERY_CHILD_ROOT", &store.root)
            .env("PC_RECOVERY_CHILD_READY", &ready)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "child exited before write barrier"
            );
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child write barrier timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            fs::read_dir(&store.root).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".stage"))
        );
        assert_eq!(store.list().unwrap(), vec![record.clone()]);
        assert_eq!(store.open(&record).unwrap().name, "previous");
        assert_eq!(fs::read(&original).unwrap(), b"original");
        fs::remove_file(ready).unwrap();
        fs::remove_file(original).unwrap();
        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_recovery_remains_discoverable_after_schema_migration() {
        use std::io::{Cursor, Read, Write};
        use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

        let store = store();
        let project = ProjectData::new("M1 unsaved work".into());
        let mut record = store.write(&project, origin(), 7).unwrap();
        let current = fs::read(store.payload(record.id)).unwrap();
        let mut reader = ZipArchive::new(Cursor::new(current)).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let legacy_id = project.legacy_content_identity().unwrap();
        for index in 0..reader.len() {
            let mut entry = reader.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name.ends_with(".json") {
                let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                if name == "manifest.json" {
                    json["required_capabilities"] = serde_json::json!([]);
                    json["schema"] = 1.into();
                    json["min_reader"] = 1.into();
                    json["content_identity"] = serde_json::json!(legacy_id);
                } else if name == "project.json" {
                    json.as_object_mut().unwrap().remove("dictionary_entries");
                    json.as_object_mut().unwrap().remove("dictionary_links");
                }
                bytes = serde_json::to_vec(&json).unwrap();
            }
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        let legacy_bytes = writer.finish().unwrap().into_inner();
        record.content_id = legacy_id;
        record.payload_digest = Sha256::digest(&legacy_bytes).into();
        fs::write(store.payload(record.id), &legacy_bytes).unwrap();
        fs::write(
            store.metadata(record.id),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();

        assert_eq!(store.list().unwrap(), vec![record.clone()]);
        assert_eq!(store.open(&record).unwrap().name, project.name);
        assert_eq!(fs::read(store.payload(record.id)).unwrap(), legacy_bytes);
        record.content_id = project.content_identity().unwrap();
        fs::write(
            store.metadata(record.id),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert!(store.open(&record).is_err());

        let root = store.root.clone();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_existing_untrusted_directory() {
        let path =
            std::env::temp_dir().join(format!("processcraft-untrusted-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        assert!(RecoveryStore::new(&path).is_err());
        fs::remove_dir(path).unwrap();
    }
}
