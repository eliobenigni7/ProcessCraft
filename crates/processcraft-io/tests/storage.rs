#![cfg(windows)]

use processcraft_io::{Access, FileGrant, OpenSession, SaveOutcome, SavePoint, WriterSession};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("processcraft-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn target(&self) -> PathBuf {
        self.0.join("modello à.processcraft")
    }
    fn writer(&self) -> WriterSession {
        let grant = FileGrant::new(self.target(), Access::Write).unwrap();
        match WriterSession::open(&grant).unwrap() {
            OpenSession::Writable(writer) => writer,
            OpenSession::ReadOnly => panic!("expected writer"),
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn failed_stage_keeps_original() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old complete bytes").unwrap();
    let mut writer = temp.writer();
    for point in [
        SavePoint::StageCreated,
        SavePoint::Written,
        SavePoint::Flushed,
        SavePoint::BeforeReplace,
    ] {
        let outcome = writer.save_with_hook(b"new complete bytes", |at, _| {
            if at == point {
                Err(io::Error::other("injected failure"))
            } else {
                Ok(())
            }
        });
        assert!(
            matches!(outcome, SaveOutcome::NotCommitted { .. }),
            "{outcome:?}"
        );
        assert_eq!(fs::read(temp.target()).unwrap(), b"old complete bytes");
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2);
    }
}

#[test]
fn second_writer_is_read_only() {
    let temp = Temp::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_process", "--nocapture"])
        .env("PROCESSCRAFT_CHILD_DIR", &temp.0)
        .env("PROCESSCRAFT_CHILD_MODE", "lock")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&temp.0.join("ready"), &mut child);
    let grant = FileGrant::new(temp.target(), Access::Write).unwrap();
    assert!(matches!(
        WriterSession::open(&grant).unwrap(),
        OpenSession::ReadOnly
    ));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(matches!(
        WriterSession::open(&grant).unwrap(),
        OpenSession::Writable(_)
    ));
}

#[test]
fn replacement_outcome_is_explicit() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut writer = temp.writer();
    let outcome = writer.save_with_hook(b"new", |point, _| {
        if point == SavePoint::Replaced {
            Err(io::Error::other("post-commit warning"))
        } else {
            Ok(())
        }
    });
    assert!(
        matches!(outcome, SaveOutcome::Committed { warning: Some(_) }),
        "{outcome:?}"
    );
    assert_eq!(fs::read(temp.target()).unwrap(), b"new");
    assert!(matches!(
        writer.save(b"newer"),
        SaveOutcome::Committed { .. }
    ));
}

#[test]
fn denied_grant_writes_nothing() {
    let temp = Temp::new();
    let grant = FileGrant::new(temp.target(), Access::ReadOnly).unwrap();
    assert!(matches!(
        WriterSession::open(&grant).unwrap(),
        OpenSession::ReadOnly
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

fn wait_for(path: &std::path::Path, child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "child exited before barrier"
        );
        assert!(Instant::now() < deadline, "child barrier timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn child_process() {
    let Ok(dir) = std::env::var("PROCESSCRAFT_CHILD_DIR") else {
        return;
    };
    let temp = std::mem::ManuallyDrop::new(Temp(PathBuf::from(dir)));
    let mut writer = temp.writer();
    let mode = std::env::var("PROCESSCRAFT_CHILD_MODE").unwrap();
    if mode == "lock" {
        fs::write(temp.0.join("ready"), b"ready").unwrap();
    } else {
        let stop = if mode == "before" {
            SavePoint::BeforeReplace
        } else {
            SavePoint::Replaced
        };
        writer.save_with_hook(&vec![0x6e; 1024 * 1024], |at, _| {
            if at == stop {
                fs::write(temp.0.join("ready"), b"ready").unwrap();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            Ok(())
        });
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn killed_writer_leaves_complete_old_or_new_bytes() {
    for mode in ["before", "after"] {
        let temp = Temp::new();
        let old = vec![0x6f; 1024 * 1024];
        fs::write(temp.target(), &old).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_process", "--nocapture"])
            .env("PROCESSCRAFT_CHILD_DIR", &temp.0)
            .env("PROCESSCRAFT_CHILD_MODE", mode)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        wait_for(&temp.0.join("ready"), &mut child);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(
            fs::read(temp.target()).unwrap(),
            if mode == "before" {
                old
            } else {
                vec![0x6e; 1024 * 1024]
            }
        );
        let grant = FileGrant::new(temp.target(), Access::Write).unwrap();
        assert!(matches!(
            WriterSession::open(&grant).unwrap(),
            OpenSession::Writable(_)
        ));
    }
}

#[test]
fn actual_sharing_violation_keeps_original() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut writer = temp.writer();
    let _blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(temp.target())
        .unwrap();
    let outcome = writer.save(b"new");
    match outcome {
        SaveOutcome::NotCommitted { error } => assert_eq!(error.raw_os_error(), Some(32)),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(temp.target()).unwrap(), b"old");
}

#[test]
fn external_change_and_hardlinks_refuse_replacement() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut writer = temp.writer();
    fs::write(temp.target(), b"external").unwrap();
    assert!(matches!(
        writer.save(b"new"),
        SaveOutcome::NotCommitted { .. }
    ));
    assert_eq!(fs::read(temp.target()).unwrap(), b"external");
    drop(writer);
    fs::hard_link(temp.target(), temp.0.join("alias")).unwrap();
    assert!(FileGrant::new(temp.target(), Access::Write).is_err());
    assert_eq!(fs::read(temp.0.join("alias")).unwrap(), b"external");
}

fn powershell(script: &str, temp: &Temp, stage: Option<&std::path::Path>) -> String {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("PC_TARGET", temp.target())
        .env("PC_DIR", &temp.0)
        .env("PC_STAGE", stage.unwrap_or(&temp.0))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn dacl_stream_and_creation_time_survive_replacement() {
    use std::os::windows::fs::MetadataExt;
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    fs::write(
        format!("{}:review", temp.target().display()),
        b"retained ADS",
    )
    .unwrap();
    powershell(
        "$ErrorActionPreference='Stop'; $acl=New-Object Security.AccessControl.FileSecurity; $acl.SetAccessRuleProtection($true,$false); $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','Allow'); $acl.AddAccessRule($rule); [IO.File]::SetAccessControl($env:PC_TARGET,$acl)",
        &temp,
        None,
    );
    let before = powershell(
        "[IO.File]::GetAccessControl($env:PC_TARGET).Sddl",
        &temp,
        None,
    );
    let created = fs::metadata(temp.target()).unwrap().creation_time();
    let mut writer = temp.writer();
    let outcome = writer.save_with_hook(b"new", |point, stage| {
        if point == SavePoint::StageCreated {
            let stage_dacl = powershell("$a=[IO.File]::GetAccessControl($env:PC_TARGET); $b=[IO.File]::GetAccessControl($env:PC_STAGE); if (($a.AccessToString -ne $b.AccessToString) -or !$b.AreAccessRulesProtected) {throw ('stage DACL mismatch: original=' + $a.Sddl + '; stage=' + $b.Sddl)}", &temp, Some(stage));
            assert!(stage_dacl.is_empty());
            assert_eq!(fs::metadata(stage).unwrap().len(), 0);
        }
        Ok(())
    });
    assert!(
        matches!(outcome, SaveOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        powershell(
            "[IO.File]::GetAccessControl($env:PC_TARGET).Sddl",
            &temp,
            None
        ),
        before
    );
    assert_eq!(
        fs::read(format!("{}:review", temp.target().display())).unwrap(),
        b"retained ADS"
    );
    assert_eq!(
        fs::metadata(temp.target()).unwrap().creation_time(),
        created
    );
}

#[test]
fn actual_access_denied_during_stage_keeps_original() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut writer = temp.writer();
    let sid = powershell(
        "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        &temp,
        None,
    );
    let deny = Command::new("icacls.exe")
        .arg(&temp.0)
        .args(["/deny", &format!("*{sid}:(WD)")])
        .output()
        .unwrap();
    assert!(
        deny.status.success(),
        "{}",
        String::from_utf8_lossy(&deny.stderr)
    );
    let outcome = writer.save(b"new");
    let restore = Command::new("icacls.exe")
        .arg(&temp.0)
        .args(["/remove:d", &format!("*{sid}")])
        .output()
        .unwrap();
    assert!(restore.status.success());
    match outcome {
        SaveOutcome::NotCommitted { error } => assert_eq!(error.raw_os_error(), Some(5)),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(temp.target()).unwrap(), b"old");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2);
}

#[test]
fn injected_partial_stage_is_rejected() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut writer = temp.writer();
    let outcome = writer.save_with_hook(b"new complete", |point, stage| {
        if point == SavePoint::BeforeReplace {
            fs::write(stage, b"partial")?;
        }
        Ok(())
    });
    assert!(matches!(outcome, SaveOutcome::NotCommitted { .. }));
    assert_eq!(fs::read(temp.target()).unwrap(), b"old");
}

#[test]
fn file_aliases_share_one_lock_or_are_refused() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let _writer = temp.writer();
    let case_alias = temp.target().to_string_lossy().to_uppercase();
    let grant = FileGrant::new(case_alias, Access::Write).unwrap();
    assert!(matches!(
        WriterSession::open(&grant).unwrap(),
        OpenSession::ReadOnly
    ));
    let trailing_dot = format!("{}.", temp.target().display());
    assert!(FileGrant::new(trailing_dot, Access::Write).is_err());
    let ads = format!("{}:extra", temp.target().display());
    assert!(FileGrant::new(ads, Access::Write).is_err());
    // Windows may have 8.3 creation disabled. If it supplies a short name,
    // FileGrant must still resolve the same sidecar.
    let short = powershell(
        "$f=New-Object -ComObject Scripting.FileSystemObject; $f.GetFile($env:PC_TARGET).ShortPath",
        &temp,
        None,
    );
    let grant = FileGrant::new(short, Access::Write).unwrap();
    assert!(matches!(
        WriterSession::open(&grant).unwrap(),
        OpenSession::ReadOnly
    ));
}

#[test]
fn superscript_device_names_are_refused_without_writes() {
    let temp = Temp::new();
    for name in ["COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³"] {
        for suffix in ["", ".processcraft"] {
            let result = FileGrant::new(temp.0.join(format!("{name}{suffix}")), Access::Write);
            assert!(
                matches!(result, Err(error) if error.kind() == io::ErrorKind::Unsupported),
                "device name was not refused: {name}{suffix}"
            );
        }
    }
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn junction_grant_is_refused() {
    let temp = Temp::new();
    let real = temp.0.join("real");
    fs::create_dir(&real).unwrap();
    powershell(
        "$ErrorActionPreference='Stop'; [void](New-Item -ItemType Junction -Path ($env:PC_DIR+'\\alias') -Target ($env:PC_DIR+'\\real'))",
        &temp,
        None,
    );
    assert!(FileGrant::new(temp.0.join("alias").join("target"), Access::Write).is_err());
}

#[test]
fn new_file_is_invisible_until_complete_and_disk_full_is_not_committed() {
    let temp = Temp::new();
    let mut writer = temp.writer();
    let outcome = writer.save_with_hook(b"complete", |point, _| {
        if point != SavePoint::Replaced {
            assert!(!temp.target().exists());
        }
        Ok(())
    });
    assert!(matches!(outcome, SaveOutcome::Committed { .. }));
    let outcome = writer.save_with_hook(b"replacement", |point, _| {
        if point == SavePoint::Written {
            Err(io::Error::from_raw_os_error(112))
        } else {
            Ok(())
        }
    });
    assert!(
        matches!(outcome, SaveOutcome::NotCommitted { error } if error.raw_os_error() == Some(112))
    );
    assert_eq!(fs::read(temp.target()).unwrap(), b"complete");
}

#[test]
fn unsupported_attributes_are_refused() {
    let temp = Temp::new();
    fs::write(temp.target(), b"old").unwrap();
    let mut permissions = fs::metadata(temp.target()).unwrap().permissions();
    let original_permissions = permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(temp.target(), permissions).unwrap();
    let result = FileGrant::new(temp.target(), Access::Write);
    fs::set_permissions(temp.target(), original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(temp.target()).unwrap(), b"old");
}

#[test]
fn grant_pins_authorized_directory_until_release() {
    let temp = Temp::new();
    let directory = temp.0.join("authorized");
    let moved = temp.0.join("moved");
    fs::create_dir(&directory).unwrap();
    let grant = FileGrant::new(directory.join("file.processcraft"), Access::Write).unwrap();
    assert_eq!(
        fs::rename(&directory, &moved).unwrap_err().raw_os_error(),
        Some(32)
    );
    drop(grant);
    fs::rename(directory, moved).unwrap();
}
