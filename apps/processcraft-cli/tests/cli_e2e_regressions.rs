use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli(dir: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(args)
        .env("APPDATA", dir)
        .env("LOCALAPPDATA", dir)
        .env("USERPROFILE", dir)
        .output()
        .unwrap()
}
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn run_refuses_an_existing_output_without_overwrite() {
    let dir = std::env::temp_dir().join(format!("processcraft-cli-e2e-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let _guard = Temp(dir.clone());
    let empty = dir.join("empty.json");
    fs::write(&empty, br#"{"type":"batch","mutations":[]}"#).unwrap();
    let (a, b) = (dir.join("a.processcraft"), dir.join("b.processcraft"));
    for (name, out) in [("A", &a), ("B", &b)] {
        let made = cli(
            &dir,
            &[
                "run".as_ref(),
                "--new".as_ref(),
                name.as_ref(),
                "--batch".as_ref(),
                empty.as_os_str(),
                "--out".as_ref(),
                out.as_os_str(),
            ],
        );
        assert!(made.status.success(), "{}", stderr(&made));
    }
    let before = fs::read(&b).unwrap();
    let run_in = |extra: &[&str]| {
        let mut args: Vec<&std::ffi::OsStr> = vec![
            "run".as_ref(),
            "--in".as_ref(),
            a.as_os_str(),
            "--batch".as_ref(),
            empty.as_os_str(),
            "--out".as_ref(),
            b.as_os_str(),
        ];
        args.extend(extra.iter().map(std::ffi::OsStr::new));
        cli(&dir, &args)
    };
    let refused = run_in(&[]);
    assert_eq!(refused.status.code(), Some(4), "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("--overwrite"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(
        fs::read(&b).unwrap(),
        before,
        "existing output must be untouched"
    );
    let allowed = run_in(&["--overwrite"]);
    assert!(allowed.status.success(), "{}", stderr(&allowed));
    assert_ne!(fs::read(&b).unwrap(), before);
    // Saving back onto the input itself never needs --overwrite.
    let in_place = cli(
        &dir,
        &[
            "run".as_ref(),
            "--in".as_ref(),
            a.as_os_str(),
            "--batch".as_ref(),
            empty.as_os_str(),
            "--out".as_ref(),
            a.as_os_str(),
        ],
    );
    assert!(in_place.status.success(), "{}", stderr(&in_place));
    // New-project runs onto an existing file are refused as well.
    let fresh = cli(
        &dir,
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "C".as_ref(),
            "--batch".as_ref(),
            empty.as_os_str(),
            "--out".as_ref(),
            a.as_os_str(),
        ],
    );
    assert_eq!(fresh.status.code(), Some(4), "{}", stderr(&fresh));
}

#[test]
fn layout_commands_name_the_overwrite_flag() {
    let dir = std::env::temp_dir().join(format!("processcraft-cli-e2e-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let _guard = Temp(dir.clone());
    let (input, out) = (dir.join("in.processcraft"), dir.join("proposal.json"));
    fs::write(&input, b"x").unwrap();
    fs::write(&out, b"y").unwrap();
    let preview = cli(
        &dir,
        &[
            "layout-preview".as_ref(),
            input.as_os_str(),
            "--document-id".as_ref(),
            "22222222-2222-4222-8222-222222222222".as_ref(),
            "--diagram-id".as_ref(),
            "d".as_ref(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
    );
    assert_eq!(preview.status.code(), Some(4), "{}", stderr(&preview));
    assert!(
        stderr(&preview).contains("--overwrite"),
        "{}",
        stderr(&preview)
    );
    let other = dir.join("other.processcraft");
    fs::write(&other, b"z").unwrap();
    let apply_existing = cli(
        &dir,
        &[
            "layout-apply".as_ref(),
            input.as_os_str(),
            "--proposal".as_ref(),
            out.as_os_str(),
            "--out".as_ref(),
            other.as_os_str(),
        ],
    );
    assert_eq!(
        apply_existing.status.code(),
        Some(4),
        "{}",
        stderr(&apply_existing)
    );
    assert!(
        stderr(&apply_existing).contains("--overwrite"),
        "{}",
        stderr(&apply_existing)
    );
}
