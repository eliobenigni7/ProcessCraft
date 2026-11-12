use processcraft_core::{
    commands::{Envelope, Request, Session},
    model::ProjectData,
};
use processcraft_io::{Access, FileGrant, FileSession, native};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p =
            std::env::temp_dir().join(format!("processcraft-cli-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli(args: &[&std::ffi::OsStr], profile: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(args)
        .env("APPDATA", profile)
        .env("LOCALAPPDATA", profile)
        .env("USERPROFILE", profile)
        .output()
        .unwrap()
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/commands/procurement.json")
}
fn run_new(t: &Temp, out: &Path) -> Output {
    cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "Procurement".as_ref(),
            "--batch".as_ref(),
            fixture().as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    )
}
#[test]
fn cli_matches_engine_content() {
    let t = Temp::new();
    let out = t.path("output.processcraft");
    let result = run_new(&t, &out);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let loaded = native::load_bytes(&fs::read(out).unwrap()).unwrap();
    let mut expected = ProjectData::new("Procurement".into());
    expected.id = loaded.project.id;
    let mut session = Session::new(expected).unwrap();
    let request: Request = serde_json::from_slice(&fs::read(fixture()).unwrap()).unwrap();
    session
        .apply(
            Envelope {
                request_id: "test".into(),
                session_id: session.id(),
                expected_generation: Some(0),
            },
            request,
        )
        .unwrap();
    assert_eq!(
        loaded.project.content_identity().unwrap(),
        session.snapshot().content_id
    );
}
#[test]
fn batch_failure_writes_nothing() {
    let t = Temp::new();
    let batch = t.path("bad.json");
    fs::write(&batch,r#"{"type":"batch","mutations":[{"type":"deleteDocument","document_id":"11111111-1111-4111-8111-111111111111"}]}"#).unwrap();
    let out = t.path("out.pc");
    let r = cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            batch.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(r.status.code(), Some(3));
    assert!(!out.exists());
}
#[test]
fn gui_owned_file_refuses_headless_writer() {
    let t = Temp::new();
    let source = t.path("source.pc");
    assert!(run_new(&t, &source).status.success());
    let (_owner, _) =
        FileSession::open("gui", FileGrant::new(&source, Access::Write).unwrap()).unwrap();
    let out = t.path("out.pc");
    let r = cli(
        &[
            "run".as_ref(),
            "--in".as_ref(),
            source.as_os_str(),
            "--batch".as_ref(),
            fixture().as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(r.status.code(), Some(4));
    assert!(!out.exists());
}
#[test]
fn paths_are_explicit_grants() {
    let t = Temp::new();
    let source = t.path("source.pc");
    assert!(run_new(&t, &source).status.success());
    let before = fs::read(&source).unwrap();
    let batch = t.path("batch.json");
    fs::write(
        &batch,
        r#"{"type":"batch","mutations":[{"type":"renameProject","name":"Changed"}]}"#,
    )
    .unwrap();
    let out = t.path("other.pc");
    let r = cli(
        &[
            "run".as_ref(),
            "--in".as_ref(),
            source.as_os_str(),
            "--batch".as_ref(),
            batch.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(before, fs::read(source).unwrap());
    assert!(out.exists());
}
#[test]
fn headless_run_creates_no_recovery_or_preferences() {
    let t = Temp::new();
    let profile = t.path("profile");
    fs::create_dir(&profile).unwrap();
    let out = t.path("out.pc");
    let r = cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            fixture().as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &profile,
    );
    assert!(r.status.success());
    assert_eq!(fs::read_dir(profile).unwrap().count(), 0);
}
#[test]
fn schema_and_validation_exit_codes() {
    let t = Temp::new();
    let b = t.path("bad.json");
    fs::write(&b, r#"{"type":"batch","mutations":[],"unexpected":1}"#).unwrap();
    let out = t.path("out.pc");
    let r = cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            b.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(r.status.code(), Some(2));
    fs::write(&out, b"invalid").unwrap();
    let r = cli(
        &["validate".as_ref(), out.as_os_str(), "--json".as_ref()],
        &t.0,
    );
    assert_eq!(r.status.code(), Some(3));
}

#[test]
fn exit_codes_and_nested_schema_are_explicit() {
    let t = Temp::new();
    let missing = t.path("missing.pc");
    let result = cli(
        &["info".as_ref(), missing.as_os_str(), "--json".as_ref()],
        &t.0,
    );
    assert_eq!(result.status.code(), Some(1));
    let batch = t.path("unknown.json");
    fs::write(
        &batch,
        r#"{"type":"batch","mutations":[{"type":"renameProject","name":"x","unknown":true}]}"#,
    )
    .unwrap();
    let out = t.path("out.pc");
    let result = cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            batch.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
    let result = cli(&["commands".as_ref(), "--json".as_ref()], &t.0);
    assert_eq!(result.status.code(), Some(0));
    let data: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        data["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["serde_tag"] == "setFlowCondition")
    );
}

#[test]
fn bpmn_limitations_are_reported_without_validation_failure() {
    let t = Temp::new();
    let source = t.path("minimal.bpmn");
    fs::write(&source, r#"<?xml version="1.0" encoding="UTF-8"?><definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:test"/>"#).unwrap();
    let result = cli(
        &["validate".as_ref(), source.as_os_str(), "--json".as_ref()],
        &t.0,
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    // The real prepared-import outcome is reported, not a fixed preserved-source claim.
    assert_eq!(data["capability"], "editable");
    assert_eq!(data["semanticEditsAllowed"], true);
    assert!(!data["limitations"].as_array().unwrap().is_empty());
    // Format is sniffed from content: BPMN XML under a native name is still BPMN.
    let misnamed = t.path("misnamed.processcraft");
    fs::copy(&source, &misnamed).unwrap();
    let result = cli(&["info".as_ref(), misnamed.as_os_str()], &t.0);
    let data: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data["format"], "bpmn");
    // import-bpmn writes a ZIP package, so a .bpmn output name is refused.
    let result = cli(
        &[
            "import-bpmn".as_ref(),
            source.as_os_str(),
            "--out".as_ref(),
            t.path("out.bpmn").as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(!t.path("out.bpmn").exists());
}

#[test]
fn info_lists_document_and_diagram_ids_with_hex_content_id() {
    let t = Temp::new();
    let source = t.path("source.bpmn");
    fs::write(&source, r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><task id="task" name="Review"/></process><bpmndi:BPMNDiagram id="d"><bpmndi:BPMNPlane id="plane" bpmnElement="p"><bpmndi:BPMNShape id="s" bpmnElement="task"><dc:Bounds x="0" y="0" width="100" height="80"/></bpmndi:BPMNShape></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></definitions>"#).unwrap();
    let project = t.path("p.processcraft");
    let imported = cli(
        &[
            "import-bpmn".as_ref(),
            source.as_os_str(),
            "--out".as_ref(),
            project.as_os_str(),
        ],
        &t.0,
    );
    let imported: serde_json::Value = serde_json::from_slice(&imported.stdout).unwrap();
    // Unprefixed bpmnElement values name local IDs even without a default namespace.
    assert_eq!(imported["editable"], true, "{imported}");
    let info = cli(&["info".as_ref(), project.as_os_str()], &t.0);
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(info["documents"][0]["id"], imported["documentId"]);
    assert_eq!(info["documents"][0]["diagrams"][0], "d");
    let content = info["contentId"].as_str().unwrap();
    assert!(content.len() == 64 && content.bytes().all(|b| b.is_ascii_hexdigit()));
    // The scale range is checked before render diagnostics.
    let result = cli(
        &[
            "export-image".as_ref(),
            project.as_os_str(),
            "--document-id".as_ref(),
            imported["documentId"].as_str().unwrap().as_ref(),
            "--diagram-id".as_ref(),
            "d".as_ref(),
            "--format".as_ref(),
            "svg".as_ref(),
            "--out".as_ref(),
            t.path("x.svg").as_os_str(),
            "--scale".as_ref(),
            "0".as_ref(),
        ],
        &t.0,
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--scale"));
    // Missing inputs name the path.
    let missing = t.path("nope.processcraft");
    let result = cli(&["info".as_ref(), missing.as_os_str()], &t.0);
    assert!(String::from_utf8_lossy(&result.stderr).contains("nope.processcraft"));
}

#[test]
fn oversized_batch_is_rejected_before_output() {
    let t = Temp::new();
    let batch = t.path("large.json");
    fs::write(&batch, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    let out = t.path("out.pc");
    let result = cli(
        &[
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            batch.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ],
        &t.0,
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(!out.exists());
}

#[cfg(all(debug_assertions, feature = "test-fault"))]
#[test]
fn indeterminate_save_subprocess_emits_reconciliation_and_exit_one() {
    let t = Temp::new();
    let out = t.path("out.pc");
    let output = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args([
            "run".as_ref(),
            "--new".as_ref(),
            "x".as_ref(),
            "--batch".as_ref(),
            fixture().as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ])
        .env("PROCESSCRAFT_TEST_INDETERMINATE", "1")
        .env("APPDATA", &t.0)
        .env("LOCALAPPDATA", &t.0)
        .env("USERPROFILE", &t.0)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("injected indeterminate save"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "indeterminate");
    assert!(uuid::Uuid::parse_str(json["operationId"].as_str().unwrap()).is_ok());
    assert!(uuid::Uuid::parse_str(json["sessionId"].as_str().unwrap()).is_ok());
    assert_eq!(json["target"], out.to_string_lossy().as_ref());
    assert_eq!(json["reconciliationRequired"], true);
    assert!(!out.exists());
}

#[test]
fn query_library_and_dictionary_return_json_without_side_effects() {
    let t = Temp::new();
    let out = t.path("project.processcraft");
    assert!(run_new(&t, &out).status.success());
    let library = cli(
        &["query".as_ref(), out.as_os_str(), "library".as_ref()],
        &t.0,
    );
    assert!(
        library.status.success(),
        "{}",
        String::from_utf8_lossy(&library.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&library.stdout).unwrap();
    assert!(value["documents"].is_array());
    let dictionary = cli(
        &["query".as_ref(), out.as_os_str(), "dictionary".as_ref()],
        &t.0,
    );
    assert!(dictionary.status.success());
    let value: serde_json::Value = serde_json::from_slice(&dictionary.stdout).unwrap();
    assert!(value["entries"].is_array());
}

#[test]
fn mcp_stdio_uses_real_handshake_and_shared_schema() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let t = Temp::new();
    let target = t.path("result.processcraft");
    let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(["mcp", "--new", "MCP project"])
        .arg("--write-root")
        .arg(&t.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut call = |value: serde_json::Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    let initialized = call(
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "processcraft");
    let listed =
        call(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    // MCP requires every inputSchema to be an object schema.
    assert!(tools.iter().all(|v| v["inputSchema"]["type"] == "object"));
    let apply = tools.iter().find(|v| v["name"] == "apply").unwrap();
    assert!(apply["inputSchema"]["$defs"].is_object());
    let reconcile = tools.iter().find(|v| v["name"] == "reconcile").unwrap();
    assert!(
        reconcile["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "operationId")
    );
    let inspected = call(
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    assert_eq!(
        inspected["result"]["structuredContent"]["name"],
        "MCP project"
    );
    let session = inspected["result"]["structuredContent"]["sessionId"]
        .as_str()
        .unwrap();
    let arguments = serde_json::json!({"sessionId":session,"requestId":"rename-1","expectedGeneration":0,"request":{"type":"batch","mutations":[{"type":"renameProject","name":"Renamed"}]}});
    let request = |id| serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":arguments}});
    let applied = call(request(4));
    assert_eq!(applied["result"]["structuredContent"]["ok"], true);
    let retried = call(request(5));
    assert_eq!(
        retried["result"]["structuredContent"],
        applied["result"]["structuredContent"]
    );
    let conflict = call(
        serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"apply","arguments":{"sessionId":session,"requestId":"rename-1","expectedGeneration":0,"request":{"type":"undo"}}}}),
    );
    assert_eq!(
        conflict["result"]["structuredContent"]["status"],
        "Conflict"
    );
    let stale = call(
        serde_json::json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":session,"requestId":"stale","expectedGeneration":0,"path":target,"overwrite":false}}}),
    );
    assert_eq!(
        stale["result"]["structuredContent"]["status"],
        "generationConflict"
    );
    let outside =
        t.0.parent()
            .unwrap()
            .join(format!("outside-{}.processcraft", uuid::Uuid::new_v4()));
    let denied = call(
        serde_json::json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":session,"requestId":"outside","expectedGeneration":1,"path":outside,"overwrite":false}}}),
    );
    assert_eq!(
        denied["result"]["structuredContent"]["status"],
        "pathNotAuthorized"
    );
    assert!(!outside.exists());
    // An existing file outside the roots must not be distinguishable from a missing one.
    let other = Temp::new();
    let existing = other.path("existing.processcraft");
    fs::write(&existing, b"x").unwrap();
    let hidden = call(
        serde_json::json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":session,"requestId":"existing-outside","expectedGeneration":1,"path":existing,"overwrite":false}}}),
    );
    assert_eq!(
        hidden["result"]["structuredContent"]["status"],
        "pathNotAuthorized"
    );
    let saved = call(
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"save","arguments":{"sessionId":session,"requestId":"save-1","expectedGeneration":1,"path":target,"overwrite":false}}}),
    );
    assert_eq!(saved["result"]["structuredContent"]["status"], "committed");
    let unknown = call(
        serde_json::json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"reconcile","arguments":{"sessionId":session,"requestId":"reconcile-unknown","expectedGeneration":1,"operationId":uuid::Uuid::new_v4().to_string()}}}),
    );
    assert_eq!(
        unknown["result"]["structuredContent"]["status"],
        "unknownOperation"
    );
    assert_eq!(
        native::load_bytes(&fs::read(&target).unwrap())
            .unwrap()
            .project
            .name,
        "Renamed"
    );
    drop(input);
    assert!(child.wait().unwrap().success());
}

#[test]
fn cli_import_and_export_bpmn_svg_png() {
    let t = Temp::new();
    let source = t.path("source.bpmn");
    fs::write(&source, r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:tns="urn:test" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><task id="task" name="Review"/></process><bpmndi:BPMNDiagram id="d"><bpmndi:BPMNPlane id="plane" bpmnElement="tns:p"><bpmndi:BPMNShape id="s" bpmnElement="tns:task"><dc:Bounds x="0" y="0" width="100" height="80"/></bpmndi:BPMNShape></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></definitions>"#).unwrap();
    let project = t.path("imported.processcraft");
    let imported = cli(
        &[
            "import-bpmn".as_ref(),
            source.as_os_str(),
            "--out".as_ref(),
            project.as_os_str(),
        ],
        &t.0,
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&imported.stdout).unwrap();
    assert_eq!(result["editable"], true);
    let id = result["documentId"].as_str().unwrap();
    let alias = t.path("alias.processcraft");
    fs::hard_link(&project, &alias).unwrap();
    let before = fs::read(&project).unwrap();
    let rejected = cli(
        &[
            "export-bpmn".as_ref(),
            project.as_os_str(),
            "--document-id".as_ref(),
            id.as_ref(),
            "--out".as_ref(),
            alias.as_os_str(),
            "--overwrite".as_ref(),
        ],
        &t.0,
    );
    assert!(!rejected.status.success());
    assert_eq!(fs::read(&project).unwrap(), before);
    fs::remove_file(&alias).unwrap();
    let output = t.path("export.bpmn");
    let exported = cli(
        &[
            "export-bpmn".as_ref(),
            project.as_os_str(),
            "--document-id".as_ref(),
            id.as_ref(),
            "--out".as_ref(),
            output.as_os_str(),
        ],
        &t.0,
    );
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(processcraft_io::bpmn::import_bpmn(&fs::read(output).unwrap()).is_ok());
    for (format, extension) in [("svg", "svg"), ("png", "png")] {
        let output = t.path(&format!("image.{extension}"));
        let exported = cli(
            &[
                "export-image".as_ref(),
                project.as_os_str(),
                "--document-id".as_ref(),
                id.as_ref(),
                "--diagram-id".as_ref(),
                "d".as_ref(),
                "--format".as_ref(),
                format.as_ref(),
                "--out".as_ref(),
                output.as_os_str(),
                "--acknowledge-diagnostics".as_ref(),
            ],
            &t.0,
        );
        assert!(
            exported.status.success(),
            "{}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let bytes = fs::read(output).unwrap();
        assert!(if format == "png" {
            bytes.starts_with(b"\x89PNG")
        } else {
            String::from_utf8_lossy(&bytes).contains("<svg")
        });
    }
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .arg("mcp")
        .arg("--in")
        .arg(&project)
        .arg("--read-root")
        .arg(&t.0)
        .arg("--write-root")
        .arg(&t.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut call = |v: serde_json::Value| {
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    call(
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    let inspected = call(
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    let session = inspected["result"]["structuredContent"]["sessionId"]
        .as_str()
        .unwrap();
    let rejected = call(
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"export","arguments":{"sessionId":session,"requestId":"same-target","expectedGeneration":0,"path":project,"documentId":id,"overwrite":true,"acknowledgeNativeFields":false}}}),
    );
    assert_eq!(
        rejected["result"]["structuredContent"]["status"],
        "exportTargetsActiveNativeFile"
    );
    assert_eq!(fs::read(&project).unwrap(), before);
    let target = t.path("mcp.svg");
    let exported = call(
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"export","arguments":{"sessionId":session,"requestId":"export-1","expectedGeneration":0,"path":target,"documentId":id,"overwrite":false,"acknowledgeNativeFields":false,"format":"svg","diagramId":"d","acknowledgeDiagnostics":true}}}),
    );
    assert_eq!(
        exported["result"]["structuredContent"]["status"], "committed",
        "{exported}"
    );
    assert!(String::from_utf8_lossy(&fs::read(target).unwrap()).contains("<svg"));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn mcp_discards_oversized_input_before_parsing() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(["mcp", "--new", "bounded"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    writeln!(input, "{}", "x".repeat(4 * 1024 * 1024 + 1)).unwrap();
    input.flush().unwrap();
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let result: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(result["id"], 1);
    assert!(result.get("result").is_some());
    drop(input);
    assert!(child.wait().unwrap().success());
}

#[cfg(windows)]
#[test]
fn mcp_attach_forwards_to_owner_session() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let mut session = Session::new(ProjectData::new("Visible".into())).unwrap();
    let id = session.id();
    let (sender, owner) = processcraft_io::local_ipc::owner_queue(id.clone(), 4);
    let pipe = processcraft_io::windows_pipe::Server::start(sender).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args([
            "mcp",
            "--attach",
            pipe.endpoint(),
            "--token",
            pipe.token(),
            "--session",
            &id,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let owner_thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut count = 0;
        while count < 2 && std::time::Instant::now() < deadline {
            if let Some(pending) = owner.try_recv() {
                let req = &pending.request;
                let result = if req.method == "inspect" {
                    serde_json::json!({"sessionId":session.id(),"generation":session.snapshot().generation,"name":session.snapshot().project.name})
                } else {
                    let request: Request =
                        serde_json::from_value(req.params["request"].clone()).unwrap();
                    let result = session
                        .apply(
                            Envelope {
                                session_id: req.session_id.clone(),
                                request_id: req.request_id.clone(),
                                expected_generation: Some(req.expected_generation),
                            },
                            request,
                        )
                        .unwrap();
                    serde_json::json!({"ok":true,"generation":result.generation})
                };
                assert!(pending.reply(result));
                count += 1;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        (count, session.snapshot().project.name.clone())
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut call = |v: serde_json::Value| {
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    call(
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    let inspect = call(
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    assert_eq!(inspect["result"]["structuredContent"]["name"], "Visible");
    let renamed = call(
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"apply","arguments":{"sessionId":id,"requestId":"rename","expectedGeneration":0,"request":{"type":"batch","mutations":[{"type":"renameProject","name":"Remote rename"}]}}}}),
    );
    assert_eq!(
        renamed["result"]["structuredContent"]["ok"], true,
        "{renamed}"
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
    assert_eq!(owner_thread.join().unwrap(), (2, "Remote rename".into()));
}

#[test]
fn mcp_discards_duplicate_keys_before_sdk_parsing() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(["mcp", "--new", "strict"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
    writeln!(input, "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{{\"name\":\"inspect\",\"arguments\":{{\"x\":1,\"x\":2}}}}}}").unwrap();
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect","arguments":{}}})).unwrap();
    input.flush().unwrap();
    let mut first = String::new();
    output.read_line(&mut first).unwrap();
    let mut second = String::new();
    output.read_line(&mut second).unwrap();
    let first: serde_json::Value = serde_json::from_str(&first).unwrap();
    let second: serde_json::Value = serde_json::from_str(&second).unwrap();
    assert_eq!(first["id"], 1);
    assert_eq!(second["id"], 3);
    drop(input);
    assert!(child.wait().unwrap().success());
}

#[test]
fn layout_file_preview_apply_and_stale_guard() {
    let t = Temp::new();
    let input = t.path("input.processcraft");
    let mut p = ProjectData::new("Layout".into());
    let mut doc =
        processcraft_core::model::BpmnDocument::new("Model".into(), "defs".parse().unwrap());
    let document_id = doc.id.to_string();
    doc.processes.push(processcraft_core::model::Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: "process".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![processcraft_core::model::Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            id: "task".parse().unwrap(),
            name: Some("Task".into()),
            documentation: String::new().into(),
            kind: processcraft_core::model::NodeKind::Task,
            details: None,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    doc.diagrams.push(processcraft_core::model::Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: "diagram".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "process".parse().unwrap(),
        shapes: vec![processcraft_core::model::Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: "shape".parse().unwrap(),
            element: "task".parse().unwrap(),
            bounds: processcraft_core::model::Bounds {
                x: 200.0,
                y: 100.0,
                width: 100.0,
                height: 60.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    p.documents.push(std::sync::Arc::new(doc));
    fs::write(&input, native::write_bytes(&p).unwrap()).unwrap();
    let proposal = t.path("layout.json");
    let result = cli(
        &[
            "layout-preview".as_ref(),
            input.as_os_str(),
            "--document-id".as_ref(),
            document_id.as_ref(),
            "--diagram-id".as_ref(),
            "diagram".as_ref(),
            "--out".as_ref(),
            proposal.as_os_str(),
        ],
        &t.0,
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = t.path("layout.processcraft");
    let args = [
        "layout-apply".as_ref(),
        input.as_os_str(),
        "--proposal".as_ref(),
        proposal.as_os_str(),
        "--out".as_ref(),
        output.as_os_str(),
    ];
    let result = cli(&args, &t.0);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(native::load_bytes(&fs::read(&output).unwrap()).is_ok());
    assert!(!cli(&args, &t.0).status.success());
    let mut changed = native::load_bytes(&fs::read(&input).unwrap())
        .unwrap()
        .project;
    changed.name = "Changed after preview".into();
    fs::write(&input, native::write_bytes(&changed).unwrap()).unwrap();
    let stale_out = t.path("stale.processcraft");
    let result = cli(
        &[
            "layout-apply".as_ref(),
            input.as_os_str(),
            "--proposal".as_ref(),
            proposal.as_os_str(),
            "--out".as_ref(),
            stale_out.as_os_str(),
        ],
        &t.0,
    );
    assert!(!result.status.success());
    assert!(!stale_out.exists());
}

#[test]
fn cli_queries_persisted_comments_and_checkpoint_reference() {
    use processcraft_core::{
        ids::{CheckpointId, CommentId, DocumentId, ElementKey},
        model::{Checkpoint, Comment, FrozenProject},
    };
    let t = Temp::new();
    let input = t.path("m3.processcraft");
    let mut p = ProjectData::new("M3".into());
    let comment_id = CommentId::new();
    p.comments.push(Comment {
        id: comment_id,
        target: ElementKey {
            document_id: DocumentId::new(),
            xml_id: "deleted".parse().unwrap(),
        },
        author: "Analyst".into(),
        text: "Retained".into(),
    });
    let id = CheckpointId::new();
    p.checkpoints.push(Checkpoint {
        id,
        name: "Baseline".into(),
        created_unix_ms: 5,
        content: FrozenProject::capture(&p),
    });
    p.reference_checkpoint = Some(id);
    fs::write(&input, native::write_bytes(&p).unwrap()).unwrap();
    let comments = cli(
        &["query".as_ref(), input.as_os_str(), "comments".as_ref()],
        &t.0,
    );
    assert!(comments.status.success());
    let value: serde_json::Value = serde_json::from_slice(&comments.stdout).unwrap();
    assert_eq!(value["items"][0]["comment"]["id"], comment_id.to_string());
    assert_eq!(value["items"][0]["orphan"], true);
    let checkpoints = cli(
        &["query".as_ref(), input.as_os_str(), "checkpoints".as_ref()],
        &t.0,
    );
    assert!(checkpoints.status.success());
    let value: serde_json::Value = serde_json::from_slice(&checkpoints.stdout).unwrap();
    assert_eq!(value["items"][0]["id"], id.to_string());
    assert_eq!(value["items"][0]["reference"], true);
}

#[test]
fn cli_discovery_matches_all_typed_mutations() {
    let t = Temp::new();
    let result = cli(&["commands".as_ref(), "--json".as_ref()], &t.0);
    assert!(result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let mut schema_tags: Vec<_> = value["requestSchema"]["$defs"]["Mutation"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| {
            variant["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let mut descriptor_tags: Vec<_> = value["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["serde_tag"].as_str().unwrap().to_owned())
        .filter(|tag| !matches!(tag.as_str(), "batch" | "undo" | "redo"))
        .collect();
    schema_tags.sort();
    descriptor_tags.sort();
    assert_eq!(descriptor_tags, schema_tags);
}
