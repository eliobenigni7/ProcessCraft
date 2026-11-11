use processcraft_core::model::{Capability, ProjectData};
use std::sync::Arc;
fn export(
    b: &processcraft_io::interchange::PreparedImport,
) -> processcraft_io::interchange::ExportedBpmn {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap()
}
use processcraft_io::interchange::{export_bpmn, prepare_import};
const FLAT: &str = r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" xmlns:tns="urn:test" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><startEvent id="start"/><task id="task" name="Review"/><endEvent id="end"/><sequenceFlow id="f1" sourceRef="tns:start" targetRef="tns:task"/><sequenceFlow id="f2" sourceRef="tns:task" targetRef="tns:end"/></process><bpmndi:BPMNDiagram id="d"><bpmndi:BPMNPlane id="plane" bpmnElement="tns:p"><bpmndi:BPMNShape id="s" bpmnElement="tns:task"><dc:Bounds x="-10" y="20" width="100" height="80"/></bpmndi:BPMNShape><bpmndi:BPMNEdge id="e" bpmnElement="tns:f1"><di:waypoint x="0" y="1"/><di:waypoint x="10" y="20"/></bpmndi:BPMNEdge></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></definitions>"#;
#[test]
fn editable_profile_round_trips_and_keeps_source() {
    let mut b = prepare_import(FLAT.as_bytes(), "flat").unwrap();
    assert!(b.report.editable);
    assert_eq!(&*b.resources[0].bytes, FLAT.as_bytes());
    b.document.processes[0].nodes[1].name = "A & B".into();
    let out = export(&b);
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].nodes[1].name, "A & B");
    assert_eq!(again.document.diagrams[0].shapes[0].bounds.x, -10.0);
    assert_eq!(again.document.diagrams[0].edges[0].waypoints.len(), 2);
}
#[test]
fn unknown_attributes_force_exact_preservation() {
    let input = FLAT.replace("id=\"task\"", "id=\"task\" arbitrary=\"x\"");
    let b = prepare_import(input.as_bytes(), "source").unwrap();
    assert!(matches!(
        b.document.capability,
        Capability::PreservedSource { .. }
    ));
    assert_eq!(export(&b).bytes, input.as_bytes());
}
#[test]
fn documentation_labels_and_multiple_diagrams_round_trip() {
    let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", "<task id=\"task\" name=\"Review\"><documentation>Notes &amp; detail</documentation></task>").replace("</bpmndi:BPMNShape>", "<bpmndi:BPMNLabel><dc:Bounds x=\"-5\" y=\"10\" width=\"70\" height=\"20\"/></bpmndi:BPMNLabel></bpmndi:BPMNShape>").replace("</definitions>", "<bpmndi:BPMNDiagram id=\"d2\"><bpmndi:BPMNPlane id=\"plane2\" bpmnElement=\"tns:p\"/></bpmndi:BPMNDiagram></definitions>");
    let b = prepare_import(input.as_bytes(), "source").unwrap();
    assert!(b.report.editable);
    let again = prepare_import(&export(&b).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[1].documentation,
        "Notes & detail"
    );
    assert_eq!(again.document.diagrams.len(), 2);
    assert_eq!(
        again.document.diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .x,
        -5.
    );
}
#[test]
fn ambiguous_or_foreign_content_is_preserved() {
    for input in [
        FLAT.replace("<task id=", "<task xmlns:v=\"urn:vendor\" v:flag=\"x\" id="),
        FLAT.replace("<task id=", "<task custom=\"x\" id="),
    ] {
        let b = prepare_import(input.as_bytes(), "source").unwrap();
        assert!(!b.report.editable);
        assert_eq!(export(&b).bytes, input.as_bytes());
    }
}
#[test]
fn project_links_require_acknowledgement() {
    let b = prepare_import(FLAT.as_bytes(), "source").unwrap();
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources;
    p.links.push(processcraft_core::model::NavigationLink {
        source: processcraft_core::ids::ElementKey {
            document_id: b.document.id,
            xml_id: "task".parse().unwrap(),
        },
        target: b.document.id,
        label: "link".into(),
    });
    assert!(export_bpmn(&p, b.document.id, false).is_err());
    assert!(
        !export_bpmn(&p, b.document.id, true)
            .unwrap()
            .report
            .omitted
            .is_empty()
    );
}
#[test]
fn lane_and_formal_condition_survive_edit_export_reimport() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn");
    let mut b = prepare_import(input, "fixture").unwrap();
    assert!(b.report.editable);
    assert_eq!(b.document.processes[0].lanes[0].members.len(), 2);
    assert_eq!(
        b.document.processes[0].flows[0].condition.as_deref(),
        Some("x > 3")
    );
    b.document.processes[0].nodes[1].name = "Edited".into();
    let out = export(&b);
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].lanes[0].members.len(), 2);
    assert_eq!(
        again.document.processes[0].flows[0].condition.as_deref(),
        Some("x > 3")
    );
    assert_eq!(again.document.processes[0].nodes[1].name, "Edited");
    assert!(String::from_utf8(out.bytes).unwrap().contains("laneSet1"));
}
#[test]
fn prefixed_formal_expression_is_preserved() {
    let input = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn").to_vec(),
    )
    .unwrap()
    .replace("xmlns:bpmn=", "xmlns:foo=\"urn:foreign\" xmlns:bpmn=")
    .replace("x &gt; 3", "foo:value");
    let b = prepare_import(input.as_bytes(), "source").unwrap();
    assert!(!b.report.editable);
    assert_eq!(export(&b).bytes, input.as_bytes());
    let mut editable = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn"),
        "source",
    )
    .unwrap();
    editable.document.processes[0].flows[0].condition = Some("foo:value".into());
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(editable.document.clone()));
    p.resources = editable.resources;
    assert!(export_bpmn(&p, editable.document.id, true).is_err());
}

#[test]
fn additional_basic_node_kinds_round_trip_or_preserve_specialized_content() {
    for tag in [
        "serviceTask",
        "sendTask",
        "receiveTask",
        "scriptTask",
        "businessRuleTask",
        "inclusiveGateway",
        "complexGateway",
        "eventBasedGateway",
        "intermediateThrowEvent",
    ] {
        let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", &format!("<{tag} id=\"task\" name=\"Review\"><documentation>Important</documentation></{tag}>"));
        let b = prepare_import(input.as_bytes(), "source").unwrap();
        assert!(b.report.editable, "{tag}");
        let out = export(&b);
        let again = prepare_import(&out.bytes, "again").unwrap();
        assert!(again.report.editable, "{tag}");
        assert_eq!(
            format!("{:?}", again.document.processes[0].nodes[1].kind),
            format!("{:?}", b.document.processes[0].nodes[1].kind),
            "{tag}"
        );
        assert_eq!(
            again.document.processes[0].nodes[1].documentation,
            "Important"
        );
        assert_eq!(again.document.processes[0].flows.len(), 2);
        let specialized = input.replace(
            &format!("<{tag} id="),
            &format!("<{tag} custom=\"unrepresented\" id="),
        );
        let preserved = prepare_import(specialized.as_bytes(), "specialized").unwrap();
        assert!(!preserved.report.editable, "{tag}");
        assert_eq!(export(&preserved).bytes, specialized.as_bytes());
    }
}

#[test]
fn event_definitions_and_boundary_events_remain_preserved() {
    for replacement in [
        "<intermediateCatchEvent id=\"task\"><messageEventDefinition id=\"msg\"/></intermediateCatchEvent>",
        "<boundaryEvent id=\"task\" attachedToRef=\"tns:start\"><timerEventDefinition id=\"timer\"/></boundaryEvent>",
    ] {
        let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", replacement);
        let result = prepare_import(input.as_bytes(), "event").unwrap();
        assert!(!result.report.editable);
        assert_eq!(export(&result).bytes, input.as_bytes());
    }
}

#[test]
fn catch_timer_details_round_trip() {
    for (name, value) in [
        ("timeDate", "2026-11-12"),
        ("timeDuration", "PT5M"),
        ("timeCycle", "R3/PT5M"),
    ] {
        let node = format!(
            "<intermediateCatchEvent id=\"task\" name=\"Wait\"><timerEventDefinition id=\"timer\"><{name}>{value}</{name}></timerEventDefinition></intermediateCatchEvent>"
        );
        let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", &node);
        let parsed = prepare_import(input.as_bytes(), "timer").unwrap();
        assert!(parsed.report.editable, "{name}");
        let output = export(&parsed);
        let again = prepare_import(&output.bytes, "again").unwrap();
        assert!(again.report.editable, "{name}");
        assert_eq!(
            again.document.processes[0].nodes[1].details,
            parsed.document.processes[0].nodes[1].details
        );
        assert!(String::from_utf8(output.bytes).unwrap().contains(value));
    }
}

#[test]
fn end_terminate_definition_round_trips() {
    let input = FLAT.replace(
        "<endEvent id=\"end\"/>",
        "<endEvent id=\"end\"><terminateEventDefinition id=\"terminate\"/></endEvent>",
    );
    let parsed = prepare_import(input.as_bytes(), "terminate").unwrap();
    assert!(parsed.report.editable);
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[2].details,
        parsed.document.processes[0].nodes[2].details
    );
}

#[test]
fn boundary_timer_attachment_and_cancel_round_trip() {
    for cancel in ["", " cancelActivity=\"false\"", " cancelActivity=\"true\""] {
        let node = format!(
            "<boundaryEvent id=\"boundary\" attachedToRef=\"tns:task\"{cancel}><timerEventDefinition id=\"timer\"><timeDuration>PT5M</timeDuration></timerEventDefinition></boundaryEvent>"
        );
        let input = FLAT.replace("</process>", &format!("{node}</process>"));
        let parsed = prepare_import(input.as_bytes(), "boundary").unwrap();
        assert!(parsed.report.editable);
        let out = export(&parsed);
        let again = prepare_import(&out.bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(
            again.document.processes[0].nodes.last().unwrap().details,
            parsed.document.processes[0].nodes.last().unwrap().details
        );
        let invalid = input.replace("attachedToRef=\"tns:task\"", "attachedToRef=\"tns:start\"");
        let fallback = prepare_import(invalid.as_bytes(), "invalid").unwrap();
        assert!(!fallback.report.editable);
        assert_eq!(export(&fallback).bytes, invalid.as_bytes());
    }
}

#[test]
fn start_timer_round_trips() {
    let input = FLAT.replace("<startEvent id=\"start\"/>", "<startEvent id=\"start\"><timerEventDefinition><timeCycle>R3/PT5M</timeCycle></timerEventDefinition></startEvent>");
    let parsed = prepare_import(input.as_bytes(), "start timer").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[0].details,
        parsed.document.processes[0].nodes[0].details
    );
}

#[test]
fn normative_idrefs_import_and_export_unqualified() {
    let input = include_bytes!("../../../tests/fixtures/bpmn20-schema/timer-events.bpmn");
    let parsed = prepare_import(input, "normative timer fixture").unwrap();
    assert!(parsed.report.editable);
    let output = export(&parsed);
    let xml = String::from_utf8(output.bytes).unwrap();
    assert!(xml.contains("sourceRef=\"start\" targetRef=\"work\""));
    assert!(
        prepare_import(xml.as_bytes(), "roundtrip")
            .unwrap()
            .report
            .editable
    );
}

#[test]
fn message_and_signal_references_round_trip_or_preserve() {
    for (root, def, attr) in [
        ("message", "messageEventDefinition", "messageRef"),
        ("signal", "signalEventDefinition", "signalRef"),
    ] {
        let resource = format!("<{root} id=\"resource\" name=\"Alert\"/>");
        let event = format!(
            "<intermediateCatchEvent id=\"task\"><{def} id=\"eventdef\" {attr}=\"tns:resource\"/></intermediateCatchEvent>"
        );
        let input = FLAT
            .replace("<process id=", &format!("{resource}<process id="))
            .replace("<task id=\"task\" name=\"Review\"/>", &event);
        let parsed = prepare_import(input.as_bytes(), "event").unwrap();
        assert!(parsed.report.editable, "{root}");
        let output = export(&parsed);
        let again = prepare_import(&output.bytes, "again").unwrap();
        assert!(again.report.editable, "{root}");
        assert_eq!(
            again.document.event_resources,
            parsed.document.event_resources
        );
        assert_eq!(
            again.document.processes[0].nodes[1].details,
            parsed.document.processes[0].nodes[1].details
        );
        let dangling = input.replace("tns:resource", "tns:missing");
        if let Ok(preserved) = prepare_import(dangling.as_bytes(), "dangling") {
            assert!(!preserved.report.editable);
            assert_eq!(export(&preserved).bytes, dangling.as_bytes());
        }
    }
}

#[test]
fn conditional_catch_expression_round_trips() {
    let event = "<intermediateCatchEvent id=\"task\"><conditionalEventDefinition id=\"conditionDef\"><condition>amount &gt; 10</condition></conditionalEventDefinition></intermediateCatchEvent>";
    let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", event);
    let parsed = prepare_import(input.as_bytes(), "condition").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[1].details,
        parsed.document.processes[0].nodes[1].details
    );
    let advanced = input.replace("<condition>", "<condition language=\"urn:expr\">");
    let fallback = prepare_import(advanced.as_bytes(), "advanced").unwrap();
    assert!(!fallback.report.editable);
    assert_eq!(export(&fallback).bytes, advanced.as_bytes());
}

#[test]
fn error_and_escalation_end_events_keep_typed_root_references() {
    for (root, def, attr, code) in [
        ("error", "errorEventDefinition", "errorRef", "errorCode"),
        (
            "escalation",
            "escalationEventDefinition",
            "escalationRef",
            "escalationCode",
        ),
    ] {
        let resource = format!("<{root} id=\"resource\" name=\"Problem\" {code}=\"E1\"/>");
        let end = format!(
            "<endEvent id=\"end\"><{def} id=\"eventdef\" {attr}=\"tns:resource\"/></endEvent>"
        );
        let input = FLAT
            .replace("<process id=", &format!("{resource}<process id="))
            .replace("<endEvent id=\"end\"/>", &end);
        let parsed = prepare_import(input.as_bytes(), "event").unwrap();
        assert!(parsed.report.editable, "{root}");
        let out = export(&parsed);
        let again = prepare_import(&out.bytes, "again").unwrap();
        assert!(again.report.editable, "{root}");
        assert_eq!(
            again.document.event_resources,
            parsed.document.event_resources
        );
        assert_eq!(
            again.document.processes[0].nodes[2].details,
            parsed.document.processes[0].nodes[2].details
        );
    }
}

#[test]
fn event_reference_to_wrong_root_kind_cannot_become_editable() {
    let input = FLAT.replace("<process id=", "<message id=\"resource\"/><process id=").replace("<task id=\"task\" name=\"Review\"/>", "<intermediateCatchEvent id=\"task\"><signalEventDefinition signalRef=\"tns:resource\"/></intermediateCatchEvent>");
    if let Ok(parsed) = prepare_import(input.as_bytes(), "wrong-kind") {
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, input.as_bytes());
    }
}

#[test]
fn nested_subprocess_call_and_expanded_geometry_round_trip() {
    let bytes = include_bytes!("../../../tests/fixtures/bpmn20-schema/nested.bpmn");
    let parsed = prepare_import(bytes, "nested").unwrap();
    assert!(parsed.report.editable);
    let sub = &parsed.document.processes[0].nodes[1];
    let Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { content }) = &sub.details
    else {
        panic!("missing nested scope")
    };
    assert_eq!(content.nodes.len(), 2);
    assert_eq!(content.flows.len(), 1);
    assert_eq!(
        parsed.document.diagrams[0].shapes[0].is_expanded,
        Some(true)
    );
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].nodes[1].details, sub.details);
    assert_eq!(
        again.document.processes[0].nodes[2].details,
        parsed.document.processes[0].nodes[2].details
    );
    assert_eq!(again.document.diagrams[0].shapes[0].is_expanded, Some(true));
}

#[test]
fn cross_scope_flows_and_foreign_plane_shapes_do_not_become_editable() {
    let input = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested.bpmn").to_vec(),
    )
    .unwrap();
    for changed in [
        input.replace(
            "sourceRef=\"innerStart\" targetRef=\"innerTask\"",
            "sourceRef=\"innerStart\" targetRef=\"start\"",
        ),
        input.replace(
            "bpmnElement=\"tns:innerTask\"",
            "bpmnElement=\"tns:calledTask\"",
        ),
    ] {
        if let Ok(parsed) = prepare_import(changed.as_bytes(), "invalid") {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, changed.as_bytes());
        }
    }
}

#[test]
fn activity_loop_characteristics_round_trip() {
    for loop_xml in [
        "<standardLoopCharacteristics testBefore=\"true\" loopMaximum=\"5\"><loopCondition>x &lt; 3</loopCondition></standardLoopCharacteristics>",
        "<multiInstanceLoopCharacteristics isSequential=\"true\"><loopCardinality>3</loopCardinality><completionCondition>x &gt; 1</completionCondition></multiInstanceLoopCharacteristics>",
    ] {
        let node = format!("<task id=\"task\" name=\"Review\">{loop_xml}</task>");
        let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", &node);
        let parsed = prepare_import(input.as_bytes(), "loop").unwrap();
        assert!(parsed.report.editable);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(
            again.document.processes[0].nodes[1].loop_characteristics,
            parsed.document.processes[0].nodes[1].loop_characteristics
        );
    }
}

#[test]
fn nested_duplicate_ids_and_cross_scope_edges_are_rejected_by_core() {
    let parsed = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested.bpmn"),
        "nested",
    )
    .unwrap();
    let mut doc = parsed.document.clone();
    let outer_id = doc.processes[0].nodes[0].id.clone();
    let Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { content }) =
        &mut doc.processes[0].nodes[1].details
    else {
        panic!("nested")
    };
    content.nodes[0].id = outer_id;
    let mut project = ProjectData::new("test".into());
    project.documents.push(Arc::new(doc));
    project.resources = parsed.resources.clone();
    assert!(project.validate().is_err());
    let mut doc = parsed.document;
    let outer_id = doc.processes[0].nodes[0].id.clone();
    let Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { content }) =
        &mut doc.processes[0].nodes[1].details
    else {
        panic!("nested")
    };
    content.flows[0].target = outer_id;
    let mut project = ProjectData::new("test".into());
    project.documents.push(Arc::new(doc));
    project.resources = parsed.resources;
    assert!(project.validate().is_err());
}

#[test]
fn collaboration_conversation_and_links_round_trip() {
    let bytes = include_bytes!("../../../tests/fixtures/bpmn20-schema/conversation.bpmn");
    let parsed = prepare_import(bytes, "conversation").unwrap();
    assert!(parsed.report.editable);
    let c = parsed.document.collaboration.as_ref().unwrap();
    assert_eq!(
        (
            c.participants.len(),
            c.conversation_nodes.len(),
            c.conversation_links.len()
        ),
        (2, 1, 2)
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    let c2 = again.document.collaboration.as_ref().unwrap();
    assert_eq!(c.conversation_nodes, c2.conversation_nodes);
    assert_eq!(c.conversation_links, c2.conversation_links);
    let named = String::from_utf8(bytes.to_vec()).unwrap().replace(
        "<collaboration id=",
        "<collaboration name=\"not yet modeled\" id=",
    );
    let preserved = prepare_import(named.as_bytes(), "named").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, named.as_bytes());
}

#[test]
fn choreography_task_and_data_references_round_trip() {
    for fixture in [
        include_bytes!("../../../tests/fixtures/bpmn20-schema/choreography.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/data.bpmn").as_slice(),
    ] {
        let parsed = prepare_import(fixture, "fixture").unwrap();
        assert!(parsed.report.editable);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(
            again.document.choreographies.len(),
            parsed.document.choreographies.len()
        );
        assert_eq!(again.document.data_stores, parsed.document.data_stores);
        assert_eq!(
            again.document.processes.len(),
            parsed.document.processes.len()
        );
    }
}

#[test]
fn advanced_data_metadata_stays_preserved() {
    let source = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/data.bpmn").to_vec(),
    )
    .unwrap();
    let modified = source.replace(
        "<dataObject id=",
        "<dataObject itemSubjectRef=\"tns:unknown\" id=",
    );
    if let Ok(parsed) = prepare_import(modified.as_bytes(), "advanced") {
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, modified.as_bytes());
    }
}

#[test]
fn data_io_sets_ports_and_associations_round_trip() {
    let bytes = include_bytes!("../../../tests/fixtures/bpmn20-schema/data-io.bpmn");
    let parsed = prepare_import(bytes, "data I/O").unwrap();
    assert!(parsed.report.editable);
    let p = &parsed.document.processes[0];
    assert_eq!(p.data_associations.len(), 2);
    let io = p
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "work")
        .unwrap()
        .io_specification
        .as_ref()
        .unwrap();
    assert_eq!((io.input_sets.len(), io.output_sets.len()), (1, 1));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].data_associations,
        p.data_associations
    );
    assert_eq!(
        again.document.processes[0]
            .nodes
            .iter()
            .find(|n| n.id.to_string() == "work")
            .unwrap()
            .io_specification
            .as_ref(),
        Some(io)
    );
}

#[test]
fn invalid_data_ports_and_unmodeled_assignments_cannot_become_editable() {
    let source = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/data-io.bpmn").to_vec(),
    )
    .unwrap();
    for invalid in [
        source.replace("<targetRef>in</targetRef>", "<targetRef>out</targetRef>"),
        source.replace(
            "</dataInputAssociation>",
            "<assignment id=\"assign\"/></dataInputAssociation>",
        ),
        source.replace(
            "<inputSet id=\"inputs\">",
            "<inputSet id=\"inputs\" custom=\"x\">",
        ),
    ] {
        if let Ok(parsed) = prepare_import(invalid.as_bytes(), "invalid") {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, invalid.as_bytes());
        }
    }
}

#[test]
fn gateway_metadata_and_default_flow_round_trip() {
    let bytes = include_bytes!("../../../tests/fixtures/bpmn20-schema/gateways.bpmn");
    let parsed = prepare_import(bytes, "gateways").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    for (a, b) in parsed.document.processes[0]
        .nodes
        .iter()
        .zip(&again.document.processes[0].nodes)
    {
        assert_eq!(a.details, b.details);
    }
    let bad = String::from_utf8(bytes.to_vec())
        .unwrap()
        .replace("default=\"fallback\"", "default=\"missing\"");
    if let Ok(preserved) = prepare_import(bad.as_bytes(), "bad") {
        assert!(!preserved.report.editable);
        assert_eq!(export(&preserved).bytes, bad.as_bytes());
    }
}

#[test]
fn descriptive_artifacts_and_nested_lanes_round_trip() {
    for fixture in [
        include_bytes!("../../../tests/fixtures/bpmn20-schema/artifacts.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/groups.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested-lanes.bpmn").as_slice(),
    ] {
        let parsed = prepare_import(fixture, "descriptive").unwrap();
        assert!(parsed.report.editable);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(again.document.categories, parsed.document.categories);
        assert_eq!(
            again.document.processes[0].groups,
            parsed.document.processes[0].groups
        );
        assert_eq!(
            again.document.processes[0].lane_set_id,
            parsed.document.processes[0].lane_set_id
        );
        assert_eq!(
            again.document.processes[0].lanes,
            parsed.document.processes[0].lanes
        );
        assert_eq!(
            again.document.processes[0].flows,
            parsed.document.processes[0].flows
        );
    }
}

#[test]
fn unsupported_artifact_metadata_and_dangling_category_are_safe() {
    let input = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/groups.bpmn").to_vec(),
    )
    .unwrap();
    for modified in [
        input.replace(
            "categoryValueRef=\"tns:value\"",
            "categoryValueRef=\"tns:missing\"",
        ),
        input.replace("<group id=", "<group custom=\"x\" id="),
    ] {
        if let Ok(parsed) = prepare_import(modified.as_bytes(), "unsupported") {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, modified.as_bytes());
        }
    }
}

#[test]
fn descriptive_artifact_identity_survives_native_round_trip() {
    for fixture in [
        include_bytes!("../../../tests/fixtures/bpmn20-schema/artifacts.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/groups.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested-lanes.bpmn").as_slice(),
    ] {
        let parsed = prepare_import(fixture, "native descriptive").unwrap();
        let mut project = ProjectData::new("native descriptive".into());
        project.documents.push(Arc::new(parsed.document));
        project.resources = parsed.resources;
        let identity = project.content_identity().unwrap();
        let bytes = processcraft_io::native::write_bytes(&project).unwrap();
        let reopened = processcraft_io::native::load_bytes(&bytes).unwrap().project;
        assert_eq!(reopened.content_identity().unwrap(), identity);
    }
}

#[test]
fn activity_attributes_and_plain_script_round_trip() {
    let bytes = include_bytes!("../../../tests/fixtures/bpmn20-schema/activity.bpmn");
    let parsed = prepare_import(bytes, "activity").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        parsed.document.processes[0].nodes[0].activity,
        again.document.processes[0].nodes[0].activity
    );
    let input = String::from_utf8(bytes.to_vec()).unwrap();
    for modified in [
        input.replace("default=\"next\"", "default=\"missing\""),
        input.replace("<script>", "<script custom=\"x\">"),
    ] {
        if let Ok(parsed) = prepare_import(modified.as_bytes(), "invalid") {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, modified.as_bytes());
        }
    }
}

#[test]
fn generated_lane_set_ids_avoid_collisions_and_preserve_nested_lanes() {
    let mut parsed = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/advanced-subprocess.bpmn"),
        "nested lane",
    )
    .unwrap();
    assert!(parsed.report.editable);
    let Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { content }) =
        &mut parsed.document.processes[0].nodes[1].details
    else {
        panic!("transaction")
    };
    content.lane_set_id = None;
    let work: processcraft_core::ids::XmlId = "laneSetAuto0".parse().unwrap();
    content.nodes[0].id = work.clone();
    content.lanes[0].members[0] = work;
    let output = export(&parsed);
    let xml = String::from_utf8(output.bytes.clone()).unwrap();
    assert!(xml.contains("<laneSet id=\"laneSetAuto1\">"));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    let Some(processcraft_core::bpmn_model::NodeDetails::SubProcess { content }) =
        &again.document.processes[0].nodes[1].details
    else {
        panic!("transaction")
    };
    assert_eq!(content.lanes.len(), 1);
    assert_eq!(content.lanes[0].members[0].to_string(), "laneSetAuto0");
}

#[test]
fn generated_root_and_child_lane_sets_are_distinct() {
    let mut parsed = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested-lanes.bpmn"),
        "lanes",
    )
    .unwrap();
    parsed.document.processes[0].lane_set_id = None;
    for lane in &mut parsed.document.processes[0].lanes {
        lane.child_lane_set_id = None;
    }
    let output = export(&parsed);
    let xml = String::from_utf8(output.bytes.clone()).unwrap();
    assert!(xml.contains("<childLaneSet id=\"laneSetAuto"));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].lanes.len(), 2);
    assert_ne!(
        again.document.processes[0].lane_set_id,
        again.document.processes[0]
            .lanes
            .iter()
            .find(|l| l.id.to_string() == "parent")
            .unwrap()
            .child_lane_set_id
    );
}

#[test]
fn ad_hoc_and_transaction_properties_round_trip() {
    let fixture = include_bytes!("../../../tests/fixtures/bpmn20-schema/advanced-subprocess.bpmn");
    let parsed = prepare_import(fixture, "advanced subprocess").unwrap();
    assert!(parsed.report.editable);
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    for (a, b) in parsed.document.processes[0]
        .nodes
        .iter()
        .zip(&again.document.processes[0].nodes)
    {
        assert_eq!(a.activity, b.activity);
        assert_eq!(a.details, b.details);
    }
    let modified = String::from_utf8(fixture.to_vec())
        .unwrap()
        .replace("ordering=\"Sequential\"", "ordering=\"Unknown\"");
    if let Ok(fallback) = prepare_import(modified.as_bytes(), "unsupported") {
        assert!(!fallback.report.editable);
        assert_eq!(export(&fallback).bytes, modified.as_bytes());
    }
}

#[test]
fn definitions_metadata_roundtrips_with_presence_and_namespace_identity() {
    let input = FLAT.replace(
        "id=\"defs\" targetNamespace=\"urn:test\"",
        "id=\"defs\" targetNamespace=\"urn:test\" name=\"A &amp; B\" expressionLanguage=\"http://www.w3.org/1999/XPath\" typeLanguage=\"urn:types\" exporter=\"Tool\" exporterVersion=\"1\"",
    );
    let parsed = prepare_import(input.as_bytes(), "metadata").unwrap();
    assert!(parsed.report.editable);
    let metadata = parsed.document.definitions.as_ref().unwrap();
    assert_eq!(metadata.target_namespace, "urn:test");
    assert_eq!(metadata.name.as_deref(), Some("A & B"));
    assert_eq!(
        metadata.expression_language.as_deref(),
        Some("http://www.w3.org/1999/XPath")
    );
    assert_eq!(metadata.type_language.as_deref(), Some("urn:types"));
    assert_eq!(metadata.exporter.as_deref(), Some("Tool"));
    assert_eq!(metadata.exporter_version.as_deref(), Some("1"));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.definitions, parsed.document.definitions);

    let omitted = prepare_import(FLAT.as_bytes(), "omitted").unwrap();
    let output = export(&omitted);
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(!xml.contains("expressionLanguage="));
    assert!(!xml.contains("typeLanguage="));
}

#[test]
fn definitions_namespace_survives_legacy_provenance_and_local_calls() {
    let input = include_bytes!("../../../tests/fixtures/bpmn20-schema/nested.bpmn");
    let mut parsed = prepare_import(input, "nested").unwrap();
    assert!(parsed.report.editable);
    parsed.document.definitions = None;
    let output = export(&parsed);
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.contains("targetNamespace=\"urn:nested\""));
    assert!(xml.contains("calledElement=\"tns:called\""));
    assert!(
        prepare_import(&output.bytes, "again")
            .unwrap()
            .report
            .editable
    );

    let unprefixed = std::str::from_utf8(input)
        .unwrap()
        .replace("calledElement=\"tns:called\"", "calledElement=\"called\"");
    let parsed = prepare_import(unprefixed.as_bytes(), "unprefixed").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, unprefixed.as_bytes());
}

#[test]
fn definitions_outside_qualified_metadata_limits_are_preserved() {
    let oversized = FLAT.replace(
        "id=\"defs\"",
        &format!("id=\"defs\" exporter=\"{}\"", "x".repeat(16_385)),
    );
    let empty_namespace = r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace=""/>"#.to_string();
    for input in [oversized, empty_namespace] {
        let parsed = prepare_import(input.as_bytes(), "source").unwrap();
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, input.as_bytes());
    }
}

#[test]
fn definitions_attribute_whitespace_and_empty_values_roundtrip() {
    let input = FLAT.replace("id=\"defs\"", "id=\"defs\" name=\"A&#9;B&#10;C&#13;D &amp; &quot;E&quot;\" exporter=\"\" exporterVersion=\"&#9;&#10;&#13;\"");
    let parsed = prepare_import(input.as_bytes(), "metadata").unwrap();
    assert!(parsed.report.editable);
    let metadata = parsed.document.definitions.as_ref().unwrap();
    assert_eq!(metadata.name.as_deref(), Some("A\tB\nC\rD & \"E\""));
    assert_eq!(metadata.exporter.as_deref(), Some(""));
    assert_eq!(metadata.exporter_version.as_deref(), Some("\t\n\r"));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert_eq!(again.document.definitions, parsed.document.definitions);
}

#[test]
fn definitions_imports_roundtrip_without_loading_locations() {
    let input = FLAT.replace("<process id=", "<import namespace=\"urn:external&amp;x\" location=\"file:///nonexistent/processcraft-import\" importType=\"urn:type\"/><import namespace=\"\" location=\"\" importType=\"\"/><process id=");
    let parsed = prepare_import(input.as_bytes(), "imports").unwrap();
    assert!(parsed.report.editable);
    let imports = &parsed.document.definitions.as_ref().unwrap().imports;
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0].namespace, "urn:external&x");
    assert_eq!(
        imports[0].location,
        "file:///nonexistent/processcraft-import"
    );
    assert_eq!(imports[1].import_type, "");
    let output = export(&parsed);
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.find("<import ").unwrap() < xml.find("<process ").unwrap());
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.definitions, parsed.document.definitions);
}

#[test]
fn unrepresented_import_content_is_preserved() {
    let imports = [
        "<import namespace=\"urn:x\" location=\"x\"/>",
        "<import namespace=\"urn:x\" location=\"x\" importType=\"t\" extra=\"x\"/>",
        "<import namespace=\"urn:x\" location=\"x\" importType=\"t\"><other/></import>",
        "<import namespace=\"urn:x\" location=\"x\" importType=\"t\">unexpected text</import>",
    ];
    for import in imports {
        let input = FLAT.replace("<process id=", &format!("{import}<process id="));
        let parsed = prepare_import(input.as_bytes(), "source").unwrap();
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, input.as_bytes());
    }
    let import = "<import namespace=\"urn:x\" location=\"x\" importType=\"t\"/>".repeat(257);
    let input = FLAT.replace("<process id=", &format!("{import}<process id="));
    let parsed = prepare_import(input.as_bytes(), "source").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, input.as_bytes());
}
