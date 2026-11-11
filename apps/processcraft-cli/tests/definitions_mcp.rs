use processcraft_core::model::{BpmnDocument, DefinitionsMetadata};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn definitions_metadata_uses_discovered_mcp_command_and_exact_history() {
    // No file grants: this exercises the shared in-memory engine on every platform.
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
            .args(["mcp", "--new", "Definitions"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = server.0.stdin.take().unwrap();
    let mut output = BufReader::new(server.0.stdout.take().unwrap());
    let mut call = |value: Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "MCP closed before responding"
        );
        serde_json::from_str::<Value>(&line).unwrap()
    };
    let initialized = call(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"definitions-test","version":"1"}
        }}),
    );
    assert!(initialized["result"].is_object());
    let listed = call(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let apply = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "apply")
        .unwrap();
    assert!(apply["inputSchema"]["$defs"]["DefinitionsMetadata"].is_object());
    let inspected = call(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    let session = inspected["result"]["structuredContent"]["sessionId"].clone();
    let document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    let document_id = document.id;
    let create = call(
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"create","expectedGeneration":0,
            "request":{"type":"batch","mutations":[{"type":"createDocument","document":document}]}
        }}}),
    );
    assert_eq!(create["result"]["structuredContent"]["ok"], true);
    let before = call(
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    let metadata = DefinitionsMetadata {
        imports: vec![processcraft_core::model::DefinitionsImport {
            namespace: "urn:external".into(),
            location: "file:///not-accessed.wsdl".into(),
            import_type: "http://schemas.xmlsoap.org/wsdl/".into(),
        }],
        target_namespace: "urn:processcraft:test".into(),
        name: Some("Standard metadata".into()),
        expression_language: None,
        type_language: None,
        exporter: Some(String::new()),
        exporter_version: Some("1".into()),
    };
    let arguments = json!({"sessionId":session,"requestId":"metadata","expectedGeneration":1,
        "request":{"type":"batch","mutations":[{"type":"setDefinitionsMetadata","document_id":document_id,"metadata":metadata}]}});
    let request = |id| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":arguments}});
    let applied = call(request(6));
    assert_eq!(applied["result"]["structuredContent"]["ok"], true);
    assert_eq!(
        call(request(7))["result"]["structuredContent"],
        applied["result"]["structuredContent"]
    );
    let after = call(
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    assert_eq!(after["result"]["structuredContent"]["generation"], 2);
    assert_ne!(
        before["result"]["structuredContent"]["contentId"],
        after["result"]["structuredContent"]["contentId"]
    );
    for (id, generation, kind, expected) in [(9, 2, "undo", &before), (11, 3, "redo", &after)] {
        let result = call(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":{
                "sessionId":session,"requestId":kind,"expectedGeneration":generation,"request":{"type":kind}
            }}}),
        );
        assert_eq!(result["result"]["structuredContent"]["ok"], true);
        let current = call(
            json!({"jsonrpc":"2.0","id":id+1,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
        );
        assert_eq!(
            current["result"]["structuredContent"]["contentId"],
            expected["result"]["structuredContent"]["contentId"]
        );
    }
}
