//! M5.5 prompt pack: the documents under `docs/prompts/` are checked without any model or network.
//!
//! 1. Every fenced JSON example claims what it is in its info string (`json tool=apply`,
//!    `tool=query`, `tool=request`, or `json output` for a reply sample) and is validated against
//!    the live schema from `commands --json` / MCP `tools/list`.
//! 2. Each recorded transcript (`tests/fixtures/prompts/*.transcript.json`, the tool calls of one
//!    real agent run) replays through a fresh `processcraft-cli mcp --new` stdio session and ends
//!    in a project that validates, has no `error` diagnostics under the recommended conventions
//!    profile and matches the recorded element and attribute counts.
use processcraft_core::{
    DiagnosticSeverity, commands::Request, conventions::ConventionsProfile, model::ProjectData,
};
use processcraft_io::native;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

const PROMPTS: [&str; 4] = [
    "text-to-model",
    "review-conventions",
    "explain-process",
    "fill-attributes",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Mcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
    dir: PathBuf,
}
impl Mcp {
    fn start() -> Self {
        let dir =
            std::env::temp_dir().join(format!("processcraft-prompts-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
            .args(["mcp", "--new", "Prompt pack"])
            .arg("--write-root")
            .arg(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Self {
            child,
            input,
            output,
            next: 1,
            dir,
        };
        mcp.rpc(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"prompt-pack","version":"1"}}),
        );
        mcp
    }
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    /// The `structuredContent` of a tool call; a JSON-RPC error fails the caller.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let reply = self.rpc("tools/call", json!({"name":tool,"arguments":arguments}));
        let content = reply["result"]["structuredContent"].clone();
        assert!(
            !content.is_null(),
            "{tool} returned no structured content: {reply}"
        );
        content
    }
}
impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

struct Block {
    file: String,
    line: usize,
    info: String,
    body: String,
}
/// Every fenced code block whose language is `json`, with its info string.
fn json_blocks(file: &Path) -> Vec<Block> {
    let text = fs::read_to_string(file).unwrap();
    let name = file.strip_prefix(root()).unwrap().display().to_string();
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    for (index, line) in text.lines().enumerate() {
        match open.take() {
            Some(mut block) if line.trim_end() == "```" => {
                blocks.push(Block {
                    body: std::mem::take(&mut block.body),
                    ..block
                });
            }
            Some(mut block) => {
                block.body.push_str(line);
                block.body.push('\n');
                open = Some(block);
            }
            None => {
                if let Some(info) = line.strip_prefix("```json") {
                    open = Some(Block {
                        file: name.clone(),
                        line: index + 1,
                        info: info.trim().to_owned(),
                        body: String::new(),
                    });
                } else if line.starts_with("```") {
                    // A non-JSON fence: skip to its end.
                    open = Some(Block {
                        file: String::new(),
                        line: 0,
                        info: "skip".into(),
                        body: String::new(),
                    });
                }
            }
        }
    }
    assert!(open.is_none(), "{name}: unterminated code fence");
    blocks.retain(|b| !b.file.is_empty());
    blocks
}
fn prompt_files() -> Vec<PathBuf> {
    let base = root().join("docs/prompts");
    let mut files = vec![base.join("README.md")];
    files.extend(PROMPTS.iter().map(|p| base.join(p).join("SKILL.md")));
    files
}

/// Top-level check of an object against a JSON Schema object node: no unknown
/// property, every required property present.
fn check_object(what: &str, value: &Value, schema: &Value) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{what}: not an object"));
    let properties = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{what}: schema has no properties"));
    for key in object.keys() {
        assert!(
            properties.contains_key(key),
            "{what}: unknown field `{key}`"
        );
    }
    for required in schema["required"].as_array().into_iter().flatten() {
        let required = required.as_str().unwrap();
        assert!(
            object.contains_key(required),
            "{what}: missing `{required}`"
        );
    }
}

fn tool_schemas(mcp: &mut Mcp) -> Value {
    let listed = mcp.rpc("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap().clone();
    json!(
        tools
            .into_iter()
            .map(|t| (
                t["name"].as_str().unwrap().to_owned(),
                t["inputSchema"].clone()
            ))
            .collect::<serde_json::Map<_, _>>()
    )
}

#[test]
fn prompt_examples_match_live_schemas() {
    let mut mcp = Mcp::start();
    let schemas = tool_schemas(&mut mcp);
    // The request protocol the CLI prints must be the schema the apply tool embeds.
    let described = Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
        .args(["commands", "--json"])
        .output()
        .unwrap();
    assert!(described.status.success());
    let described: Value = serde_json::from_slice(&described.stdout).unwrap();
    assert!(described["requestSchema"]["$defs"]["Mutation"].is_object());
    assert_eq!(
        described["requestSchema"]["$defs"], schemas["apply"]["$defs"],
        "commands --json and tools/list disagree"
    );
    // Every tool the README names exists, and the README names every tool.
    let readme = fs::read_to_string(root().join("docs/prompts/README.md")).unwrap();
    for tool in schemas.as_object().unwrap().keys() {
        assert!(
            readme.contains(&format!("`{tool}`")),
            "README omits tool {tool}"
        );
    }
    let query_variants = schemas["query"]["oneOf"].as_array().unwrap().clone();

    let mut checked = 0;
    for file in prompt_files() {
        let blocks = json_blocks(&file);
        let mut in_file = 0;
        for block in blocks {
            let here = format!("{}:{}", block.file, block.line);
            let value: Value = serde_json::from_str(&block.body)
                .unwrap_or_else(|e| panic!("{here}: invalid JSON: {e}"));
            let claim = block.info.as_str();
            match claim {
                "output" => {} // a reply sample: must parse, nothing more to check
                "tool=apply" => {
                    check_object(&here, &value, &schemas["apply"]);
                    serde_json::from_value::<Request>(value["request"].clone())
                        .unwrap_or_else(|e| panic!("{here}: not a Request: {e}"));
                    assert!(value["expectedGeneration"].is_u64(), "{here}");
                }
                "tool=request" => {
                    serde_json::from_value::<Request>(value)
                        .unwrap_or_else(|e| panic!("{here}: not a Request: {e}"));
                }
                "tool=query" => {
                    let kind = value["kind"].as_str().unwrap_or_default();
                    let variant = query_variants
                        .iter()
                        .find(|v| {
                            v["properties"]["kind"]["const"] == kind
                                || v["properties"]["kind"]["enum"]
                                    .as_array()
                                    .is_some_and(|e| e.iter().any(|x| x == kind))
                        })
                        .unwrap_or_else(|| panic!("{here}: unknown query kind `{kind}`"));
                    check_object(&here, &value, variant);
                    assert!(
                        value["offset"].is_u64() && value["limit"].is_u64(),
                        "{here}"
                    );
                }
                other => panic!(
                    "{here}: info string `{other}` must be `output` or `tool=apply|request|query`"
                ),
            }
            if claim != "output" {
                in_file += 1;
            }
            checked += 1;
        }
        if file.ends_with("SKILL.md") {
            assert!(
                in_file >= 2,
                "{}: needs worked request examples",
                file.display()
            );
            let text = fs::read_to_string(&file).unwrap();
            assert!(
                text.starts_with("---\nname: processcraft-"),
                "{}",
                file.display()
            );
            assert!(text.contains("\ndescription: "), "{}", file.display());
            for needed in [
                "## Tool sequence",
                "GenerationConflict",
                "## Must not",
                "BPMN",
            ] {
                assert!(
                    text.contains(needed),
                    "{}: missing `{needed}`",
                    file.display()
                );
            }
        }
    }
    assert!(checked >= 15, "only {checked} examples found");
}

/// An apply example that shares a `requestId` with a recorded call must carry the same request:
/// the documents show what the agents actually sent.
#[test]
fn documented_apply_examples_equal_recorded_requests() {
    let mut recorded = std::collections::HashMap::new();
    for prompt in PROMPTS {
        let transcript = read_transcript(prompt);
        for call in transcript["calls"].as_array().unwrap() {
            if call["tool"] == "apply" {
                recorded.insert(
                    call["arguments"]["requestId"].as_str().unwrap().to_owned(),
                    call["arguments"].clone(),
                );
            }
        }
    }
    let mut matched = 0;
    for file in prompt_files() {
        for block in json_blocks(&file) {
            if block.info != "tool=apply" {
                continue;
            }
            let value: Value = serde_json::from_str(&block.body).unwrap();
            let id = value["requestId"].as_str().unwrap();
            if let Some(call) = recorded.get(id) {
                assert_eq!(
                    value["request"], call["request"],
                    "{}:{}: example `{id}` differs from the recorded call",
                    block.file, block.line
                );
                assert_eq!(value["expectedGeneration"], call["expectedGeneration"]);
                matched += 1;
            }
        }
    }
    assert!(matched >= 4, "only {matched} examples tie to a transcript");
}

fn read_transcript(prompt: &str) -> Value {
    let path = root().join(format!("tests/fixtures/prompts/{prompt}.transcript.json"));
    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap()
}

fn substitute(value: &Value, session: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace("${sessionId}", session)),
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, session)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), substitute(v, session)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn replay(prompt: &str) {
    let transcript = read_transcript(prompt);
    assert_eq!(transcript["prompt"], prompt);
    let mut mcp = Mcp::start();
    let inspected = mcp.call("inspect", json!({}));
    let session = inspected["sessionId"].as_str().unwrap().to_owned();
    // The input project, if the task starts from one: a pinned fixture applied at generation 0.
    if let Some(setup) = transcript["setup"].as_object() {
        let fixture = root()
            .join("tests/fixtures")
            .join(setup["fixture"].as_str().unwrap());
        let request: Value = serde_json::from_slice(&fs::read(fixture).unwrap()).unwrap();
        let applied = mcp.call(
            "apply",
            json!({"sessionId":session,"requestId":setup["requestId"],"expectedGeneration":0,"request":request}),
        );
        assert_eq!(applied["ok"], true, "{prompt}: setup failed: {applied}");
    }
    let calls = transcript["calls"].as_array().unwrap();
    assert!(!calls.is_empty());
    for (index, call) in calls.iter().enumerate() {
        let tool = call["tool"].as_str().unwrap();
        assert!(
            !matches!(tool, "save" | "export" | "reconcile"),
            "{prompt}: recorded file effects are not replayed"
        );
        if prompt == "explain-process" {
            assert!(!matches!(tool, "apply"), "explain-process must not write");
        }
        let result = mcp.call(tool, substitute(&call["arguments"], &session));
        for (key, want) in call["expect"].as_object().unwrap() {
            assert_eq!(
                &result[key], want,
                "{prompt} call {index} ({tool}): `{key}` in {result}"
            );
        }
    }

    // End state: save, reload, and check it independently of the tool replies.
    let after = mcp.call("inspect", json!({}));
    let target = mcp.dir.join("final.processcraft");
    let saved = mcp.call(
        "save",
        json!({"sessionId":session,"requestId":"replay-save","expectedGeneration":after["generation"],"path":target,"overwrite":false}),
    );
    assert_eq!(saved["ok"], true, "{prompt}: {saved}");
    let validated = mcp.call("validate", json!({}));
    assert_eq!(validated["valid"], true, "{prompt}: {validated}");
    assert_eq!(validated["errorCount"], 0, "{prompt}: {validated}");
    let loaded = native::load_bytes(&fs::read(&target).unwrap()).unwrap();
    let project: &ProjectData = &loaded.project;
    project.validate().unwrap();
    let findings = project.diagnostics_with(&ConventionsProfile::recommended());
    assert!(
        findings
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error),
        "{prompt}: error diagnostics under the recommended profile: {findings:?}"
    );
    let expected = &transcript["expected"];
    let count = |f: fn(&processcraft_core::model::Process) -> usize| -> usize {
        project
            .documents
            .iter()
            .flat_map(|d| &d.processes)
            .map(f)
            .sum()
    };
    let actual = json!({
        "documents": project.documents.len(),
        "nodes": count(|p| p.nodes.len()),
        "flows": count(|p| p.flows.len()),
        "lanes": count(|p| p.lanes.len()),
        "attributeValues": project.attribute_values.len(),
        "attributeDefinitions": project.attribute_definitions.len(),
        "recommendedWarnings": findings.len(),
    });
    assert_eq!(
        &actual, expected,
        "{prompt}: end state differs from the recorded counts"
    );
}

#[test]
fn transcript_text_to_model_replays() {
    replay("text-to-model");
}
#[test]
fn transcript_review_conventions_replays() {
    replay("review-conventions");
}
#[test]
fn transcript_explain_process_replays() {
    replay("explain-process");
}
#[test]
fn transcript_fill_attributes_replays() {
    replay("fill-attributes");
}

/// The transcripts are pinned fixtures: each is registered in the manifest with its hash and an
/// honest provenance, and none was written by the test run.
#[test]
fn transcripts_are_registered_in_the_manifest() {
    use sha2::{Digest, Sha256};
    let manifest: Value =
        serde_json::from_slice(&fs::read(root().join("tests/fixtures/manifest.json")).unwrap())
            .unwrap();
    for prompt in PROMPTS {
        let path = format!("prompts/{prompt}.transcript.json");
        let entry = manifest["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["path"] == path)
            .unwrap_or_else(|| panic!("{path} is not in the manifest"));
        let bytes = fs::read(root().join("tests/fixtures").join(&path)).unwrap();
        let hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(entry["sha256"], hash.as_str(), "{path}");
        assert_eq!(entry["license"], "CC0-1.0");
        let provenance = entry["provenance"].as_str().unwrap();
        assert!(provenance.contains("recorded by a Claude Sonnet subagent on 2026-11-13"));
        assert!(provenance.contains(prompt));
    }
}
