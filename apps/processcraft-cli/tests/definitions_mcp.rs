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
    assert!(apply["inputSchema"]["$defs"]["Expression"].is_object());
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
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    let created = call(
        json!({"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"fixture","expectedGeneration":4,"request":fixture
        }}}),
    );
    assert_eq!(created["result"]["structuredContent"]["ok"], true);
    let expression = json!({
        "id":"ConditionMetadata","kind":{"type":"formal","explicit_type":true,
            "language":"urn:expression-language","evaluates_to_type_ref":{"namespace":"urn:item-type","local":"Amount"}},
        "text":"order:amount > 0", "namespaces":[["","http://www.omg.org/spec/BPMN/20100524/MODEL"],["xsi","http://www.w3.org/2001/XMLSchema-instance"],["order","urn:orders"],["item","urn:item-type"]]
    });
    let changed = call(
        json!({"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"expression","expectedGeneration":5,"request":{"type":"batch","mutations":[{
                "type":"setFlowCondition","document_id":"11111111-1111-4111-8111-111111111111",
                "flow_id":"flow_review_decision","condition":expression
            }]}
        }}}),
    );
    assert_eq!(changed["result"]["structuredContent"]["ok"], true);
    let table = call(
        json!({"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"query","arguments":{
            "kind":"table","document_id":"11111111-1111-4111-8111-111111111111","process_id":"procurement","offset":0,"limit":100
        }}}),
    );
    let outgoing = table["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["outgoing"].as_array().unwrap())
        .find(|flow| flow["id"] == "flow_review_decision")
        .unwrap();
    assert_eq!(outgoing["condition"], expression);
    let catalog = json!({"item_definitions":[{"id":"ResultType","structure_ref":{"namespace":"http://www.w3.org/2001/XMLSchema","local":"string"},"is_collection":false,"item_kind":"Information"}],
    "interfaces":[{"id":"ServiceInterface","name":"Lookup","implementation_ref":null,"operations":[{
        "id":"LookupOperation","name":"Run","implementation_ref":null,"in_message_ref":{"namespace":"urn:external","local":"Request"},"out_message_ref":null,"error_refs":[]
    }]}]});
    let updated = call(
        json!({"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"catalog","expectedGeneration":6,"request":{"type":"batch","mutations":[{
                "type":"setSupportingCatalog","document_id":document_id,"catalog":catalog
            }]}
        }}}),
    );
    assert_eq!(updated["result"]["structuredContent"]["ok"], true);
    let normalized = serde_json::to_value(
        serde_json::from_value::<processcraft_core::bpmn_catalog::SupportingCatalog>(catalog)
            .unwrap(),
    )
    .unwrap();
    let mut offset = 0;
    let mut seen = 0;
    let mut identity = None;
    loop {
        let page = call(
            json!({"jsonrpc":"2.0","id":17+offset,"method":"tools/call","params":{"name":"query","arguments":{
                "kind":"catalog","document_id":document_id,"offset":offset,"limit":2
            }}}),
        );
        let page = &page["result"]["structuredContent"];
        assert_eq!(page["generation"], 7);
        if let Some(previous) = &identity {
            assert_eq!(previous, &page["contentId"]);
        } else {
            identity = Some(page["contentId"].clone());
        }
        for row in page["items"].as_array().unwrap() {
            assert_eq!(
                normalized.pointer(row["path"].as_str().unwrap()),
                Some(&row["value"])
            );
            seen += 1;
        }
        if let Some(next) = page["nextOffset"].as_u64() {
            assert!(next as usize > offset);
            offset = next as usize;
        } else {
            assert_eq!(page["total"], seen);
            break;
        }
    }
    assert!(seen > 2);
    let stores = json!([{"id":null,"name":null,"capacity":"184467440737095516161234","is_unlimited":false,
        "item_aware":{"item_subject_ref":{"namespace":"urn:processcraft:test","local":"ResultType"},
            "data_state":{"id":"AnonymousStoreState","name":""}}}]);
    let updated = call(
        json!({"jsonrpc":"2.0","id":100,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"data","expectedGeneration":7,"request":{"type":"batch","mutations":[{
                "type":"setDataStores","document_id":document_id,"data_stores":stores
            }]}
        }}}),
    );
    assert_eq!(updated["result"]["structuredContent"]["ok"], true);
    let page = call(
        json!({"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"query","arguments":{
            "kind":"data","document_id":document_id,"offset":0,"limit":100
        }}}),
    );
    let page = &page["result"]["structuredContent"];
    assert_eq!(page["generation"], 8);
    let values = page["items"].as_array().unwrap();
    for (path, value) in [
        ("/data_stores/0/id", Value::Null),
        ("/data_stores/0/name", Value::Null),
        ("/data_stores/0/capacity", json!("184467440737095516161234")),
        ("/data_stores/0/item_aware/data_state/name", json!("")),
    ] {
        assert!(
            values
                .iter()
                .any(|row| row["path"] == path && row["value"] == value)
        );
    }
    let promoted = call(
        json!({"jsonrpc":"2.0","id":102,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"promote-store","expectedGeneration":8,
            "request":{"type":"batch","mutations":[{"type":"promoteElementIdentity","document_id":document_id,
                "model_path":"/data_stores/0/id","xml_id":null}]}
        }}}),
    );
    let promoted = promoted["result"]["structuredContent"].clone();
    assert_eq!(promoted["ok"], true, "{promoted}");
    assert_eq!(promoted["generation"], 9);
    let store_selector = promoted["promotedIdentities"][0]["selector"].clone();
    assert!(store_selector["native_id"].is_string());
    let arguments = json!({"sessionId":session,"requestId":"assign-store","expectedGeneration":9,
        "request":{"type":"batch","mutations":[{"type":"setElementXmlId","document_id":document_id,
            "element_id":store_selector,"xml_id":"AssignedStore"}]}});
    let assigned = call(
        json!({"jsonrpc":"2.0","id":103,"method":"tools/call","params":{"name":"apply","arguments":arguments}}),
    );
    let assigned = assigned["result"]["structuredContent"].clone();
    assert_eq!(assigned["ok"], true, "{assigned}");
    assert_eq!(assigned["generation"], 10);
    let retry = call(
        json!({"jsonrpc":"2.0","id":104,"method":"tools/call","params":{"name":"apply","arguments":arguments}}),
    );
    assert_eq!(retry["result"]["structuredContent"], assigned);
    let refused = call(
        json!({"jsonrpc":"2.0","id":105,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"store-collision","expectedGeneration":10,
            "request":{"type":"batch","mutations":[{"type":"setElementXmlId","document_id":document_id,
                "element_id":store_selector,"xml_id":"Definitions"}]}
        }}}),
    );
    assert_eq!(refused["result"]["structuredContent"]["ok"], false);
    for (id, generation, kind, expected) in
        [(106, 10, "undo", &promoted), (107, 11, "redo", &assigned)]
    {
        let result = call(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":{
                "sessionId":session,"requestId":format!("store-{kind}"),"expectedGeneration":generation,"request":{"type":kind}
            }}}),
        );
        let result = &result["result"]["structuredContent"];
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["contentId"], expected["contentId"]);
    }
    let changed = call(
        json!({"jsonrpc":"2.0","id":108,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"rename-review-id","expectedGeneration":12,
            "request":{"type":"batch","mutations":[{"type":"setElementXmlId",
                "document_id":"11111111-1111-4111-8111-111111111111","element_id":"review","xml_id":"ReviewRenamed"}]}
        }}}),
    );
    let changed = &changed["result"]["structuredContent"];
    assert_eq!(changed["ok"], true, "{changed}");
    let mapping = &changed["selectorMappings"][0];
    assert_eq!(mapping["from"], "review");
    assert!(mapping["to"]["native_id"].is_string());
    let table = call(
        json!({"jsonrpc":"2.0","id":109,"method":"tools/call","params":{"name":"query","arguments":{
            "kind":"table","document_id":"11111111-1111-4111-8111-111111111111","process_id":"procurement","offset":0,"limit":100
        }}}),
    );
    assert!(
        table["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["key"]["xml_id"] == mapping["to"])
    );
    for (id, generation, kind, from, to, expected_content) in [
        (
            110,
            13,
            "undo",
            &mapping["to"],
            &mapping["from"],
            &assigned["contentId"],
        ),
        (
            111,
            14,
            "redo",
            &mapping["from"],
            &mapping["to"],
            &changed["contentId"],
        ),
    ] {
        let result = call(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":{
                "sessionId":session,"requestId":format!("review-{kind}"),"expectedGeneration":generation,"request":{"type":kind}
            }}}),
        );
        let result = &result["result"]["structuredContent"];
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(&result["contentId"], expected_content);
        assert_eq!(&result["selectorMappings"][0]["from"], from);
        assert_eq!(&result["selectorMappings"][0]["to"], to);
    }
    let identities = call(
        json!({"jsonrpc":"2.0","id":112,"method":"tools/call","params":{"name":"query","arguments":{
            "kind":"identities","document_id":"11111111-1111-4111-8111-111111111111","offset":0,"limit":100
        }}}),
    );
    let identities = &identities["result"]["structuredContent"];
    assert_eq!(identities["generation"], 15);
    assert_eq!(identities["contentId"], changed["contentId"]);
    assert!(
        identities["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["selector"] == mapping["to"] && row["xmlId"] == "ReviewRenamed")
    );
    let extended = call(
        json!({"jsonrpc":"2.0","id":113,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"events-and-di","expectedGeneration":15,
            "request":{"type":"batch","mutations":[
                {"type":"setEventResources","document_id":"11111111-1111-4111-8111-111111111111",
                    "resources":[{"type":"message","id":"IncomingMessage","name":null}]},
                {"type":"setRootEventDefinitions","document_id":"11111111-1111-4111-8111-111111111111",
                    "definitions":[{"type":"message","id":"SharedMessage","message_ref":"IncomingMessage","operation_ref":null}]},
                {"type":"setNodeDetails","document_id":"11111111-1111-4111-8111-111111111111","element_id":"start",
                    "details":{"type":"event","definitions":[],"definition_refs":["SharedMessage"],
                        "parallel_multiple":false,"is_interrupting":true,"attached_to":null,"cancel_activity":null}},
                {"type":"setShapeDiAttributes","document_id":"11111111-1111-4111-8111-111111111111",
                    "diagram_id":"procurement_diagram","shape_id":"pc_shape_decision","is_horizontal":null,"is_marker_visible":false},
                {"type":"setEdgeDiEndpoints","document_id":"11111111-1111-4111-8111-111111111111",
                    "diagram_id":"procurement_diagram","edge_id":"pc_edge_flow_start_review",
                    "source_element":"pc_shape_start","target_element":"pc_shape_review"}
            ]}
        }}}),
    );
    let extended = &extended["result"]["structuredContent"];
    assert_eq!(extended["ok"], true, "{extended}");
    for (kind, expected) in [
        (
            "events",
            vec![
                ("/root_definitions/0/message_ref", json!("IncomingMessage")),
                (
                    "/events/0/details/definition_refs/0",
                    json!("SharedMessage"),
                ),
                ("/events/0/details/parallel_multiple", json!(false)),
                ("/events/0/details/is_interrupting", json!(true)),
            ],
        ),
        (
            "diagrams",
            vec![
                ("/0/shapes/4/is_marker_visible", json!(false)),
                ("/0/edges/0/source_element", json!("pc_shape_start")),
                ("/0/edges/0/target_element", json!("pc_shape_review")),
            ],
        ),
    ] {
        let mut offset = 0;
        let mut rows = vec![];
        loop {
            let result = call(
                json!({"jsonrpc":"2.0","id":200+offset,"method":"tools/call","params":{"name":"query","arguments":{
                    "kind":kind,"document_id":"11111111-1111-4111-8111-111111111111","offset":offset,"limit":10
                }}}),
            );
            let result = &result["result"]["structuredContent"];
            assert_eq!(result["generation"], 16);
            assert_eq!(result["contentId"], extended["contentId"]);
            rows.extend(result["items"].as_array().unwrap().iter().cloned());
            if let Some(next) = result["nextOffset"].as_u64() {
                assert!(next as usize > offset);
                offset = next as usize;
            } else {
                assert_eq!(result["total"].as_u64().unwrap() as usize, rows.len());
                break;
            }
        }
        for (path, value) in expected {
            assert!(
                rows.iter()
                    .any(|row| row["path"] == path && row["value"] == value),
                "{kind}: {path}"
            );
        }
    }
    let unchanged = call(
        json!({"jsonrpc":"2.0","id":400,"method":"tools/call","params":{"name":"inspect","arguments":{}}}),
    );
    assert_eq!(unchanged["result"]["structuredContent"]["generation"], 16);
    assert_eq!(
        unchanged["result"]["structuredContent"]["contentId"],
        extended["contentId"]
    );
}
