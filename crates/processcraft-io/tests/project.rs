#![cfg(windows)]
use processcraft_core::{
    commands::{Envelope, Mutation, Request, Session},
    model::ProjectData,
};
use processcraft_io::{Access, FileGrant, FileSession, ProjectSaveOutcome, SavePoint, native};
use std::{fs, io, path::PathBuf};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("pc-project-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> PathBuf {
        self.0.join("project.pc")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn open(t: &Temp) -> (FileSession, ProjectData) {
    let p = ProjectData::new("original".into());
    fs::write(t.path(), native::write_bytes(&p).unwrap()).unwrap();
    let (s, l) =
        FileSession::open("session", FileGrant::new(t.path(), Access::Write).unwrap()).unwrap();
    (s, l.project)
}
#[test]
fn save_old_generation_keeps_new_edits_dirty() {
    let t = Temp::new();
    let (mut f, p) = open(&t);
    let mut s = Session::new(p).unwrap();
    let old = s.snapshot();
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: Some(0),
        },
        Request::Batch {
            mutations: vec![Mutation::RenameProject { name: "new".into() }],
        },
    )
    .unwrap();
    let ProjectSaveOutcome::Committed { receipt, .. } = f.save(&old.project) else {
        panic!()
    };
    s.mark_saved(receipt.saved_content_id);
    assert!(s.snapshot().dirty);
}
#[test]
fn external_change_blocks_replace() {
    let t = Temp::new();
    let (mut f, p) = open(&t);
    fs::write(t.path(), b"external").unwrap();
    assert!(matches!(
        f.save(&p),
        ProjectSaveOutcome::NotCommitted { .. }
    ));
    assert_eq!(fs::read(t.path()).unwrap(), b"external");
}
#[test]
fn save_as_failure_keeps_source_binding() {
    let t = Temp::new();
    let (mut f, p) = open(&t);
    let dst = t.0.join("new.pc");
    let out = f.save_as_with_hook(
        FileGrant::new(&dst, Access::Write).unwrap(),
        &p,
        |point, _| {
            if point == SavePoint::BeforeReplace {
                Err(io::Error::other("cancel"))
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(out, ProjectSaveOutcome::NotCommitted { .. }));
    assert_eq!(
        f.target(),
        FileGrant::new(t.path(), Access::ReadOnly).unwrap().path()
    );
    assert!(!dst.exists());
}
#[test]
fn cancel_before_commit_keeps_original() {
    let t = Temp::new();
    let (mut f, mut p) = open(&t);
    let original = fs::read(t.path()).unwrap();
    p.name = "changed".into();
    for point in [
        SavePoint::Written,
        SavePoint::Flushed,
        SavePoint::BeforeReplace,
    ] {
        assert!(matches!(
            f.save_with_hook(&p, |at, _| if point == at {
                Err(io::Error::other("cancel"))
            } else {
                Ok(())
            }),
            ProjectSaveOutcome::NotCommitted { .. }
        ));
        assert_eq!(fs::read(t.path()).unwrap(), original);
    }
}
#[test]
fn invalid_stage_never_replaces_original() {
    let t = Temp::new();
    let (mut f, p) = open(&t);
    let original = fs::read(t.path()).unwrap();
    let out = f.save_with_hook(&p, |at, path| {
        if at == SavePoint::BeforeReplace {
            fs::write(path, b"corrupt")?;
        }
        Ok(())
    });
    assert!(matches!(out, ProjectSaveOutcome::NotCommitted { .. }));
    assert_eq!(fs::read(t.path()).unwrap(), original);
}
#[test]
fn read_only_and_writer_ownership() {
    let t = Temp::new();
    let (mut first, p) = open(&t);
    let (second, _) =
        FileSession::open("second", FileGrant::new(t.path(), Access::Write).unwrap()).unwrap();
    assert!(!second.is_writable());
    assert_eq!(
        FileGrant::new(t.path(), Access::ReadOnly)
            .unwrap()
            .read()
            .unwrap(),
        native::write_bytes(&p).unwrap()
    );
    assert!(matches!(
        first.save(&p),
        ProjectSaveOutcome::Committed { .. }
    ));
}
#[test]
fn not_content_indexed_is_preserved() {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NOT_CONTENT_INDEXED;
    let t = Temp::new();
    let p = ProjectData::new("nci".into());
    fs::write(t.path(), native::write_bytes(&p).unwrap()).unwrap();
    assert!(
        std::process::Command::new("attrib")
            .arg("+I")
            .arg(t.path())
            .status()
            .unwrap()
            .success()
    );
    let (mut f, loaded) =
        FileSession::open("nci", FileGrant::new(t.path(), Access::Write).unwrap()).unwrap();
    assert!(matches!(
        f.save(&loaded.project),
        ProjectSaveOutcome::Committed { .. }
    ));
    assert_ne!(
        fs::metadata(t.path()).unwrap().file_attributes() & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
        0
    );
}
#[test]
fn migrated_source_requires_save_as() {
    use std::io::{Cursor, Read, Write};
    let t = Temp::new();
    let p = ProjectData::new("v0".into());
    let bytes = native::write_bytes(&p).unwrap();
    let mut source = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..source.len() {
        let mut entry = source.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "manifest.json" {
            let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            v["required_capabilities"] = serde_json::json!([]);
            v["schema"] = 0.into();
            v["min_reader"] = 0.into();
            v["content_identity"] =
                serde_json::to_value(p.legacy_content_identity().unwrap()).unwrap();
            bytes = serde_json::to_vec(&v).unwrap();
        }
        if name == "project.json" {
            let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            v.as_object_mut().unwrap().remove("description");
            v.as_object_mut().unwrap().remove("dictionary_entries");
            v.as_object_mut().unwrap().remove("dictionary_links");
            bytes = serde_json::to_vec(&v).unwrap();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(&bytes).unwrap();
    }
    fs::write(t.path(), out.finish().unwrap().into_inner()).unwrap();
    let original = fs::read(t.path()).unwrap();
    let (mut f, loaded) =
        FileSession::open("migrate", FileGrant::new(t.path(), Access::Write).unwrap()).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert!(matches!(
        f.save(&loaded.project),
        ProjectSaveOutcome::NotCommitted { .. }
    ));
    assert_eq!(fs::read(t.path()).unwrap(), original);
    assert!(matches!(
        f.save_as(
            FileGrant::new(t.0.join("new.pc"), Access::Write).unwrap(),
            &loaded.project
        ),
        ProjectSaveOutcome::Committed { .. }
    ));
    assert!(matches!(
        f.save(&loaded.project),
        ProjectSaveOutcome::Committed { .. }
    ));
}
#[test]
fn operation_ids_are_unique() {
    let t = Temp::new();
    let (mut f, p) = open(&t);
    let ProjectSaveOutcome::Committed { receipt: first, .. } = f.save(&p) else {
        panic!()
    };
    let ProjectSaveOutcome::Committed {
        receipt: second, ..
    } = f.save(&p)
    else {
        panic!()
    };
    assert_ne!(first.operation_id, second.operation_id);
}
#[test]
fn create_binds_without_writing_and_refuses_other_writer() {
    let t = Temp::new();
    let target = t.path();
    let mut session =
        FileSession::create("new", FileGrant::new(&target, Access::Write).unwrap()).unwrap();
    assert!(!target.exists());
    let competing = FileSession::create("other", FileGrant::new(&target, Access::Write).unwrap());
    assert!(matches!(competing, Err(ref e) if e.kind() == io::ErrorKind::WouldBlock));
    let project = ProjectData::new("new".into());
    assert!(matches!(
        session.save(&project),
        ProjectSaveOutcome::Committed { .. }
    ));
    assert_eq!(
        native::load_bytes(&fs::read(target).unwrap())
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        project.content_identity().unwrap()
    );
}
#[test]
fn create_existing_target_preserves_conflict_guard() {
    let t = Temp::new();
    let target = t.path();
    fs::write(&target, b"old").unwrap();
    let mut session =
        FileSession::create("new", FileGrant::new(&target, Access::Write).unwrap()).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"old");
    fs::write(&target, b"external").unwrap();
    assert!(matches!(
        session.save(&ProjectData::new("new".into())),
        ProjectSaveOutcome::NotCommitted { .. }
    ));
    assert_eq!(fs::read(target).unwrap(), b"external");
}
