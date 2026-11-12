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
fn activity_commands_use_real_stdio_pages_and_exact_history() {
    use processcraft_core::{
        bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind},
        ids::ElementIdentity,
    };
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_processcraft-cli"))
            .args(["mcp", "--new", "Activities"])
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
        assert!(output.read_line(&mut line).unwrap() > 0);
        serde_json::from_str::<Value>(&line).unwrap()
    };
    assert!(call(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"activity-test","version":"1"}
    }}))["result"].is_object());
    let tools = call(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let apply = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "apply")
        .unwrap();
    assert!(apply["inputSchema"]["$defs"]["GlobalTask"].is_object());
    let inspect = |id| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"inspect","arguments":{}}});
    let session = call(inspect(3))["result"]["structuredContent"]["sessionId"].clone();
    let mut doc = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    doc.processes.push(
        serde_json::from_value(json!({
            "id":"Process","name":null,"documentation":"","nodes":[],"lanes":[],"flows":[]
        }))
        .unwrap(),
    );
    doc.diagrams.push(serde_json::from_value(json!({
        "id":"Diagram","plane_id":"Plane","plane":"Process","plane_order":[],"shapes":[],"edges":[]
    })).unwrap());
    let document_id = doc.id;
    let envelope = |id, request_id: &str, generation, request| {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":request_id,"expectedGeneration":generation,"request":request
        }}})
    };
    let created = call(envelope(
        4,
        "create",
        0,
        json!({"type":"batch","mutations":[{"type":"createDocument","document":doc}]}),
    ));
    assert_eq!(created["result"]["structuredContent"]["ok"], true);
    let before = call(inspect(5));
    let identity = ElementIdentity::anonymous();
    let task = GlobalTask {
        id: Some(identity.clone()),
        base: None,
        name: Some(String::new()),
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::Script {
            script_language: Some("urn:inert".into()),
            script: Some("inert & preserved".into()),
        },
    };
    let request = json!({"type":"batch","mutations":[{"type":"setGlobalTasks","document_id":document_id,"global_tasks":[task]}]});
    let changed = call(envelope(6, "activity", 1, request.clone()));
    assert_eq!(
        changed["result"]["structuredContent"]["ok"], true,
        "{changed}"
    );
    assert_eq!(
        call(envelope(7, "activity", 1, request))["result"]["structuredContent"],
        changed["result"]["structuredContent"]
    );
    let after = call(inspect(8));
    assert_ne!(
        before["result"]["structuredContent"]["contentId"],
        after["result"]["structuredContent"]["contentId"]
    );
    let mut offset = 0;
    let mut rows = std::collections::BTreeMap::new();
    loop {
        let page = call(
            json!({"jsonrpc":"2.0","id":100+offset,"method":"tools/call","params":{"name":"query","arguments":{
                "kind":"model","document_id":document_id,"offset":offset,"limit":4
            }}}),
        );
        let page = &page["result"]["structuredContent"];
        assert_eq!(page["generation"], 2);
        assert_eq!(
            page["contentId"],
            after["result"]["structuredContent"]["contentId"]
        );
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 4);
        for row in items {
            assert!(
                rows.insert(
                    row["path"].as_str().unwrap().to_owned(),
                    row["value"].clone()
                )
                .is_none()
            );
        }
        if let Some(next) = page["nextOffset"].as_u64() {
            assert!(next as usize > offset);
            offset = next as usize;
        } else {
            assert_eq!(page["total"], rows.len());
            break;
        }
    }
    assert_eq!(
        rows["/global_tasks/0/id/native_id"],
        serde_json::to_value(&identity).unwrap()["native_id"]
    );
    assert_eq!(rows["/global_tasks/0/id/xml_id"], Value::Null);
    assert_eq!(rows["/global_tasks/0/name"], "");
    assert_eq!(rows["/global_tasks/0/kind/script"], "inert & preserved");
    for (id, generation, kind, expected) in [(2000, 2, "undo", &before), (2002, 3, "redo", &after)]
    {
        assert_eq!(
            call(envelope(
                id,
                &format!("{kind}-global"),
                generation,
                json!({"type":kind})
            ))["result"]["structuredContent"]["ok"],
            true
        );
        assert_eq!(
            call(inspect(id + 1))["result"]["structuredContent"]["contentId"],
            expected["result"]["structuredContent"]["contentId"]
        );
    }
    let checkpoint = processcraft_core::ids::CheckpointId::new();
    assert_eq!(
        call(envelope(
            2100,
            "checkpoint-di",
            4,
            json!({"type":"batch","mutations":[{
                "type":"createCheckpoint","id":checkpoint,"name":"Before DI metadata","created_unix_ms":1
            }]})
        ))["result"]["structuredContent"]["ok"],
        true
    );
    let before_di = call(inspect(2101));
    let changed_di = call(envelope(
        2102,
        "plane-metadata",
        5,
        json!({"type":"batch","mutations":[{
            "type":"setDiagramPlaneMetadata","document_id":document_id,"diagram_id":"Diagram",
            "metadata":{"namespaces":[{"prefix":"v","uri":"urn:vendor"}],
                "foreign_attributes":[{"namespace":"urn:vendor","local":"layer","prefix":"v","value":"review-layer"}]}
        }]}),
    ));
    assert_eq!(
        changed_di["result"]["structuredContent"]["ok"], true,
        "{changed_di}"
    );
    let after_di = call(inspect(2103));
    let mut offset = 0;
    let mut found = false;
    loop {
        let response = call(
            json!({"jsonrpc":"2.0","id":2200+offset,"method":"tools/call","params":{"name":"query","arguments":{
                "kind":"checkpointFields","checkpointId":checkpoint,"offset":offset,"limit":5
            }}}),
        );
        let page = &response["result"]["structuredContent"];
        assert_eq!(page["generation"], 6, "{response}");
        assert_eq!(
            page["contentId"],
            after_di["result"]["structuredContent"]["contentId"]
        );
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 5);
        found |= items.iter().any(|row| {
            row["path"]
                .as_str()
                .unwrap()
                .starts_with("/changed_diagrams/")
                && row["value"] == "review-layer"
        });
        if let Some(next) = page["nextOffset"].as_u64() {
            assert!(next as usize > offset);
            offset = next as usize;
        } else {
            break;
        }
    }
    assert!(found);
    for (id, generation, kind, expected) in
        [(3000, 6, "undo", &before_di), (3002, 7, "redo", &after_di)]
    {
        assert_eq!(
            call(envelope(
                id,
                &format!("{kind}-di"),
                generation,
                json!({"type":kind})
            ))["result"]["structuredContent"]["ok"],
            true
        );
        assert_eq!(
            call(inspect(id + 1))["result"]["structuredContent"]["contentId"],
            expected["result"]["structuredContent"]["contentId"]
        );
    }
    assert!(apply["inputSchema"]["$defs"]["PartnerEntity"].is_object());
    assert!(apply["inputSchema"]["$defs"]["Flow"]["properties"]["source"]["anyOf"].is_array());
    // The earlier DI fixture deliberately contains opaque foreign metadata.
    // Exercise independent root authoring in a new document, retaining its guard.
    let root_document =
        BpmnDocument::new("Collaboration draft".into(), ElementIdentity::anonymous());
    let root_document_id = root_document.id;
    let collaboration = ElementIdentity::anonymous();
    let message_flow = ElementIdentity::anonymous();
    let expanded = call(envelope(
        4000,
        "collaboration-draft",
        8,
        json!({
            "type":"batch","mutations":[
                {"type":"createDocument","document":root_document},
                {"type":"setCollaborationRoot","document_id":root_document_id,"collaboration":{
                    "id":collaboration,"name":"Reusable conversation","kind":"GlobalConversation",
                    "participants":[],"flows":[{"id":message_flow,"kind":"Message",
                        "source":null,"target":{"namespace":"urn:external","local":"Actor"},
                        "name":null,"documentation":"","condition":null}],
                    "conversation_nodes":[],"conversation_links":[]
                }},
                {"type":"setPartnerCatalog","document_id":root_document_id,
                    "partner_entities":[{"id":null,"base":null,"name":"Customers","participant_refs":[]}],
                    "partner_roles":[],"endpoints":[]}
            ]
        }),
    ));
    assert_eq!(
        expanded["result"]["structuredContent"]["ok"], true,
        "{expanded}"
    );
    let before_order = call(inspect(4001));
    let initialized = call(envelope(
        4002,
        "initialize-roots",
        9,
        json!({
            "type":"batch","mutations":[{"type":"initializeDocumentRootOrder","document_id":root_document_id}]
        }),
    ));
    assert_eq!(
        initialized["result"]["structuredContent"]["ok"], true,
        "{initialized}"
    );
    let after_order = call(inspect(4003));
    let mut fields = std::collections::BTreeMap::new();
    let mut offset = 0;
    loop {
        let response = call(
            json!({"jsonrpc":"2.0","id":4100+offset,"method":"tools/call",
            "params":{"name":"query","arguments":{"kind":"model","document_id":root_document_id,
                "offset":offset,"limit":7}}}),
        );
        let page = &response["result"]["structuredContent"];
        assert_eq!(page["generation"], 10);
        assert_eq!(
            page["contentId"],
            after_order["result"]["structuredContent"]["contentId"]
        );
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 7);
        for row in items {
            assert!(
                fields
                    .insert(
                        row["path"].as_str().unwrap().to_owned(),
                        row["value"].clone()
                    )
                    .is_none()
            );
        }
        if let Some(next) = page["nextOffset"].as_u64() {
            assert!(next as usize > offset);
            offset = next as usize;
        } else {
            assert_eq!(page["total"], fields.len());
            break;
        }
    }
    let promoted = &fields["/partner_entities/0/id/native_id"];
    assert!(promoted.is_string());
    assert_eq!(fields["/partner_entities/0/id/xml_id"], Value::Null);
    assert_eq!(fields["/collaboration/flows/0/source"], Value::Null);
    assert_eq!(
        fields["/collaboration/flows/0/target/namespace"],
        "urn:external"
    );
    assert!(
        fields
            .iter()
            .any(|(path, value)| path.starts_with("/root_order/") && value == promoted)
    );
    for (id, generation, kind, expected) in [
        (5000, 10, "undo", before_order),
        (5002, 11, "redo", after_order),
    ] {
        let response = call(envelope(
            id,
            &format!("{kind}-root-order"),
            generation,
            json!({"type":kind}),
        ));
        assert_eq!(response["result"]["structuredContent"]["ok"], true);
        assert_eq!(
            call(inspect(id + 1))["result"]["structuredContent"]["contentId"],
            expected["result"]["structuredContent"]["contentId"]
        );
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
        namespaces: vec![],
        foreign_attributes: vec![],
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
    let checkpoint = "22222222-2222-4222-8222-222222222222";
    let captured = call(
        json!({"jsonrpc":"2.0","id":401,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"before-common-metadata","expectedGeneration":16,
            "request":{"type":"batch","mutations":[{"type":"createCheckpoint","id":checkpoint,"name":"Before documentation","created_unix_ms":1}]}
        }}}),
    );
    let captured = captured["result"]["structuredContent"].clone();
    assert_eq!(captured["ok"], true, "{captured}");
    let documentation = json!([{"id":"StartDocumentation","text_format":"text/plain","content":{
        "namespaces":[],"nodes":[{"type":"text","text":"before"},{"type":"comment","text":"inert"},
            {"type":"processingInstruction","target":"inspect","data":"never executed"},{"type":"text","text":"after"}]
    }}]);
    let common = call(
        json!({"jsonrpc":"2.0","id":402,"method":"tools/call","params":{"name":"apply","arguments":{
            "sessionId":session,"requestId":"common-metadata","expectedGeneration":17,
            "request":{"type":"batch","mutations":[{"type":"replaceDocumentation",
                "document_id":"11111111-1111-4111-8111-111111111111","element_id":"start","documentation":documentation}]}
        }}}),
    );
    let common = common["result"]["structuredContent"].clone();
    assert_eq!(common["ok"], true, "{common}");
    let mut model_rows = vec![];
    for (kind, extra) in [
        (
            "model",
            json!({"document_id":"11111111-1111-4111-8111-111111111111"}),
        ),
        ("checkpointFields", json!({"checkpointId":checkpoint})),
    ] {
        let mut offset = 0;
        let mut rows = vec![];
        loop {
            let mut arguments = extra.clone();
            arguments["kind"] = json!(kind);
            arguments["offset"] = json!(offset);
            arguments["limit"] = json!(100);
            let response = call(
                json!({"jsonrpc":"2.0","id":500+offset,"method":"tools/call","params":{"name":"query","arguments":arguments}}),
            );
            let page = &response["result"]["structuredContent"];
            assert_eq!(page["generation"], 18, "{response}");
            assert_eq!(page["contentId"], common["contentId"]);
            rows.extend(page["items"].as_array().unwrap().iter().cloned());
            if let Some(next) = page["nextOffset"].as_u64() {
                assert!(next as usize > offset);
                offset = next as usize;
            } else {
                assert_eq!(page["total"].as_u64().unwrap() as usize, rows.len());
                break;
            }
        }
        assert!(!rows.is_empty());
        if kind == "model" {
            model_rows = rows;
        }
    }
    for (path, value) in [
        (
            "/processes/0/nodes/0/documentation/0/id",
            json!("StartDocumentation"),
        ),
        (
            "/processes/0/nodes/0/documentation/0/content/nodes/2/data",
            json!("never executed"),
        ),
    ] {
        assert!(
            model_rows
                .iter()
                .any(|row| row["path"] == path && row["value"] == value),
            "{model_rows:?}"
        );
    }
    for (index, kind, expected) in [(700, "undo", &captured), (701, "redo", &common)] {
        let response = call(
            json!({"jsonrpc":"2.0","id":index,"method":"tools/call","params":{"name":"apply","arguments":{
                "sessionId":session,"requestId":format!("common-{kind}"),"expectedGeneration":index-682,"request":{"type":kind}
            }}}),
        );
        assert_eq!(
            response["result"]["structuredContent"]["ok"], true,
            "{response}"
        );
        assert_eq!(
            response["result"]["structuredContent"]["contentId"],
            expected["contentId"]
        );
    }
}
