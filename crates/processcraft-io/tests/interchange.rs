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
const FLAT: &str = r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" xmlns:tns="urn:test" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><startEvent id="start"/><task id="task" name="Review"/><endEvent id="end"/><sequenceFlow id="f1" sourceRef="start" targetRef="task"/><sequenceFlow id="f2" sourceRef="task" targetRef="end"/></process><bpmndi:BPMNDiagram id="d"><bpmndi:BPMNPlane id="plane" bpmnElement="tns:p"><bpmndi:BPMNShape id="s" bpmnElement="tns:task"><dc:Bounds x="-10" y="20" width="100" height="80"/></bpmndi:BPMNShape><bpmndi:BPMNEdge id="e" bpmnElement="tns:f1"><di:waypoint x="0" y="1"/><di:waypoint x="10" y="20"/></bpmndi:BPMNEdge></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></definitions>"#;
#[test]
fn editable_profile_round_trips_and_keeps_source() {
    let mut b = prepare_import(FLAT.as_bytes(), "flat").unwrap();
    assert!(b.report.editable);
    assert_eq!(&*b.resources[0].bytes, FLAT.as_bytes());
    b.document.processes[0].nodes[1].name = Some("A & B".into());
    let out = export(&b);
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[1].name.as_deref(),
        Some("A & B")
    );
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
        "Notes & detail".into()
    );
    assert_eq!(again.document.diagrams.len(), 2);
    assert_eq!(
        again.document.diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .x,
        -5.
    );
}
#[test]
fn ambiguous_or_foreign_content_is_preserved() {
    for (index, input) in [
        FLAT.replace("<task id=", "<task xmlns:v=\"urn:vendor\" v:flag=\"x\" id="),
        FLAT.replace("<task id=", "<task custom=\"x\" id="),
    ]
    .into_iter()
    .enumerate()
    {
        let b = prepare_import(input.as_bytes(), "source").unwrap();
        if index == 0 {
            assert!(b.report.editable);
            assert!(
                prepare_import(&export(&b).bytes, "again")
                    .unwrap()
                    .report
                    .editable
            );
        } else {
            assert!(!b.report.editable);
            assert_eq!(export(&b).bytes, input.as_bytes());
        }
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
fn node_and_folder_omissions_require_acknowledgement_without_mutation() {
    let mut imported = prepare_import(FLAT.as_bytes(), "native fields").unwrap();
    imported.document.folder = Some("workspace_folder".parse().unwrap());
    imported.document.processes[0].nodes[0].owner = "Owner".into();
    imported.document.processes[0].nodes[0].contact = "Contact".into();
    let mut project = ProjectData::new("native fields".into());
    project.folders.push(processcraft_core::model::Folder {
        id: "workspace_folder".parse().unwrap(),
        name: "Workspace".into(),
        parent: None,
    });
    project.documents.push(Arc::new(imported.document.clone()));
    project.resources = imported.resources;
    let before = project.content_identity().unwrap();
    let failure = export_bpmn(&project, imported.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .blocked
            .iter()
            .any(|value| value.contains("acknowledgement"))
    );
    assert!(
        failure
            .report
            .omitted
            .iter()
            .any(|value| value.contains("owner and contact"))
    );
    assert!(
        failure
            .report
            .omitted
            .iter()
            .any(|value| value.contains("folder"))
    );
    let exported = export_bpmn(&project, imported.document.id, true).unwrap();
    assert!(
        exported
            .report
            .omitted
            .iter()
            .any(|value| value.contains("owner and contact"))
    );
    assert!(
        exported
            .report
            .omitted
            .iter()
            .any(|value| value.contains("folder"))
    );
    assert_eq!(project.content_identity().unwrap(), before);
    assert_eq!(
        project.documents[0].folder.as_ref().unwrap().to_string(),
        "workspace_folder"
    );
    assert_eq!(project.documents[0].processes[0].nodes[0].owner, "Owner");
    assert_eq!(
        project.documents[0].processes[0].nodes[0].contact,
        "Contact"
    );
}

#[test]
fn preserved_source_folder_omission_requires_acknowledgement() {
    let input = FLAT.replace("<task id=", "<task custom=\"unknown\" id=");
    let mut imported = prepare_import(input.as_bytes(), "preserved").unwrap();
    assert!(!imported.report.editable);
    imported.document.folder = Some("workspace_folder".parse().unwrap());
    let mut project = ProjectData::new("preserved".into());
    project.folders.push(processcraft_core::model::Folder {
        id: "workspace_folder".parse().unwrap(),
        name: "Workspace".into(),
        parent: None,
    });
    project.documents.push(Arc::new(imported.document.clone()));
    project.resources = imported.resources;
    let failure = export_bpmn(&project, imported.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .omitted
            .iter()
            .any(|value| value.contains("folder"))
    );
    let output = export_bpmn(&project, imported.document.id, true).unwrap();
    assert_eq!(output.bytes, input.as_bytes());
    assert!(
        output
            .report
            .omitted
            .iter()
            .any(|value| value.contains("folder"))
    );
}
#[test]
fn lane_and_formal_condition_survive_edit_export_reimport() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn");
    let mut b = prepare_import(input, "fixture").unwrap();
    assert!(b.report.editable);
    assert_eq!(b.document.processes[0].lanes[0].members.len(), 2);
    assert_eq!(
        b.document.processes[0].flows[0]
            .condition
            .as_ref()
            .and_then(|e| e.plain_text()),
        Some("x > 3")
    );
    b.document.processes[0].nodes[1].name = Some("Edited".into());
    let out = export(&b);
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].lanes[0].members.len(), 2);
    assert_eq!(
        again.document.processes[0].flows[0]
            .condition
            .as_ref()
            .and_then(|e| e.plain_text()),
        Some("x > 3")
    );
    assert_eq!(
        again.document.processes[0].nodes[1].name.as_deref(),
        Some("Edited")
    );
    assert!(String::from_utf8(out.bytes).unwrap().contains("laneSet1"));
}
#[test]
fn prefixed_formal_expression_retains_namespace() {
    let input = String::from_utf8(
        include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn").to_vec(),
    )
    .unwrap()
    .replace("xmlns:bpmn=", "xmlns:foo=\"urn:foreign\" xmlns:bpmn=")
    .replace("x &gt; 3", "foo:value");
    let b = prepare_import(input.as_bytes(), "source").unwrap();
    assert!(b.report.editable);
    let again = prepare_import(&export(&b).bytes, "again").unwrap();
    assert_eq!(
        again.document.processes[0].flows[0].condition,
        b.document.processes[0].flows[0].condition
    );
    // `foo` is bound in the source, so the text may be a QName: refused.
    let mut editable = prepare_import(input.as_bytes(), "source").unwrap();
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
            "Important".into()
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
fn unbound_message_event_roundtrips_and_invalid_boundary_stays_preserved() {
    let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", "<intermediateCatchEvent id=\"task\"><messageEventDefinition id=\"msg\"/></intermediateCatchEvent>");
    let parsed = prepare_import(input.as_bytes(), "unbound message").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].nodes[1].details,
        again.document.processes[0].nodes[1].details
    );
    let invalid = FLAT.replace("<task id=\"task\" name=\"Review\"/>", "<boundaryEvent id=\"task\" attachedToRef=\"tns:start\"><timerEventDefinition id=\"timer\"/></boundaryEvent>");
    let preserved = prepare_import(invalid.as_bytes(), "invalid boundary").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, invalid.as_bytes());
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
    let Some(processcraft_core::bpmn_model::NodeDetails::Event { definitions, .. }) =
        &parsed.document.processes[0].nodes[1].details
    else {
        panic!("event expected")
    };
    assert!(matches!(
        &definitions[0],
        processcraft_core::bpmn_model::EventDefinition::Conditional {
            expression: processcraft_core::bpmn_model::Expression::Structured(
                processcraft_core::bpmn_model::ExpressionData {
                    kind: processcraft_core::bpmn_model::ExpressionKind::Base {
                        explicit_type: false
                    },
                    ..
                }
            ),
            ..
        }
    ));
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[1].details,
        parsed.document.processes[0].nodes[1].details
    );
    let invalid = input.replace("<condition>", "<condition language=\"urn:expr\">");
    let preserved = prepare_import(invalid.as_bytes(), "invalid").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, invalid.as_bytes());
    let advanced = input.replace("<condition>", "<condition xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:bpmn=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" xsi:type=\"bpmn:tFormalExpression\" language=\"urn:expr\">");
    let parsed = prepare_import(advanced.as_bytes(), "advanced").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.processes[0].nodes[1].details,
        parsed.document.processes[0].nodes[1].details
    );
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
    content.flows[0].target = Some(processcraft_core::bpmn_model::EventReference::Local(
        outer_id.selector(),
    ));
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
    let named = prepare_import(named.as_bytes(), "named").unwrap();
    assert!(named.report.editable);
    assert_eq!(
        named
            .document
            .collaboration
            .as_ref()
            .unwrap()
            .name
            .as_deref(),
        Some("not yet modeled")
    );
    let again = prepare_import(&export(&named).bytes, "again").unwrap();
    assert_eq!(
        again.document.collaboration.as_ref().unwrap().name,
        named.document.collaboration.as_ref().unwrap().name
    );
}

#[test]
fn choreography_task_and_data_references_round_trip() {
    for fixture in [
        include_bytes!("../../../tests/fixtures/bpmn20-schema/choreography.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn20-schema/data.bpmn").as_slice(),
    ] {
        let input = std::str::from_utf8(fixture)
            .unwrap()
            .replace(
                "<dataObjectReference id=",
                "<dataObjectReference name=\"\" id=",
            )
            .replace(
                "<dataStoreReference id=",
                "<dataStoreReference name=\"\" id=",
            );
        let parsed = prepare_import(input.as_bytes(), "fixture").unwrap();
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
    let bytes = include_str!("../../../tests/fixtures/bpmn20-schema/data-io.bpmn").replace(
        "<dataObjectReference id=",
        "<dataObjectReference name=\"\" id=",
    );
    let parsed = prepare_import(bytes.as_bytes(), "data I/O").unwrap();
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
fn omitted_lane_set_ids_stay_omitted_and_preserve_nested_lanes() {
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
    content.nodes[0].id = work.clone().into();
    content.lanes[0].members[0] = work.into();
    let output = export(&parsed);
    let xml = String::from_utf8(output.bytes.clone()).unwrap();
    assert!(xml.contains("<laneSet>"));
    assert!(!xml.contains("id=\"laneSetAuto1\""));
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
fn omitted_root_and_child_lane_set_ids_stay_omitted() {
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
    assert!(xml.contains("<childLaneSet>"));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(again.document.processes[0].lanes.len(), 2);
    assert!(again.document.processes[0].lane_set_id.is_none());
    assert!(
        again.document.processes[0]
            .lanes
            .iter()
            .all(|lane| lane.child_lane_set_id.is_none())
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
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert!(again.document.processes.iter().flat_map(|p| &p.nodes).any(|node| matches!(
        &node.details,
        Some(processcraft_core::bpmn_model::NodeDetails::CallActivity { called_element: Some(q) })
            if q.namespace == "urn:nested" && q.local.to_string() == "called"
    )));

    let unprefixed = std::str::from_utf8(input)
        .unwrap()
        .replace("calledElement=\"tns:called\"", "calledElement=\"called\"");
    let parsed = prepare_import(unprefixed.as_bytes(), "unprefixed").unwrap();
    assert!(parsed.report.editable);
    let roundtrip = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(roundtrip.document.processes.iter().flat_map(|p| &p.nodes).any(|node| matches!(
        &node.details,
        Some(processcraft_core::bpmn_model::NodeDetails::CallActivity { called_element: Some(q) })
            if q.namespace == "http://www.omg.org/spec/BPMN/20100524/MODEL" && q.local.to_string() == "called"
    )));
}

#[test]
fn definitions_outside_qualified_metadata_limits_are_preserved() {
    let oversized = FLAT.replace(
        "id=\"defs\"",
        &format!("id=\"defs\" exporter=\"{}\"", "x".repeat(16_385)),
    );
    let parsed = prepare_import(oversized.as_bytes(), "source").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, oversized.as_bytes());
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

#[test]
fn expression_metadata_preserves_scoped_prefixes_and_default_namespace() {
    use processcraft_core::bpmn_model::{Expression, ExpressionKind};
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn"
    ))
    .unwrap();
    let input = source.replace("<conditionExpression xsi:type=\"bpmn:tFormalExpression\">x &gt; 3</conditionExpression>", "<bpmn:conditionExpression xmlns=\"urn:body\" xmlns:xsi=\"urn:shadow\" xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:q=\"urn:types\" i:type=\"bpmn:tFormalExpression\" id=\"expr1\" language=\"urn:lang\" evaluatesToTypeRef=\"q:Item\">q:value &gt; 3</bpmn:conditionExpression>");
    let parsed = prepare_import(input.as_bytes(), "expression").unwrap();
    assert!(parsed.report.editable);
    let Some(Expression::Structured(data)) = &parsed.document.processes[0].flows[0].condition
    else {
        panic!("structured expression expected")
    };
    assert_eq!(data.id.as_ref().unwrap().to_string(), "expr1");
    assert!(data.namespaces.contains(&("".into(), "urn:body".into())));
    assert!(
        data.namespaces
            .contains(&("xsi".into(), "urn:shadow".into()))
    );
    assert!(
        matches!(&data.kind, ExpressionKind::Formal { explicit_type: true, language: Some(language), evaluates_to_type_ref: Some(q) } if language == "urn:lang" && q.namespace == "urn:types" && q.local.to_string() == "Item")
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].flows[0].condition,
        again.document.processes[0].flows[0].condition
    );
}

#[test]
fn boundary_expression_keeps_noncanonical_owner_prefix_context() {
    let node = "<boundaryEvent xmlns:owner=\"urn:test\" id=\"boundary\" attachedToRef=\"owner:task\"><timerEventDefinition id=\"timer\"><timeDuration id=\"duration\" xmlns:tns=\"urn:shadow\">PT5M</timeDuration></timerEventDefinition></boundaryEvent>";
    let input = FLAT.replace("</process>", &format!("{node}</process>"));
    let parsed = prepare_import(input.as_bytes(), "boundary").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].nodes.last().unwrap().details,
        again.document.processes[0].nodes.last().unwrap().details
    );
}

#[test]
fn expression_mixed_content_roundtrips() {
    let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", "<task id=\"task\" name=\"Review\"><standardLoopCharacteristics><loopCondition id=\"expr\">before<documentation>note</documentation>after</loopCondition></standardLoopCharacteristics></task>");
    let parsed = prepare_import(input.as_bytes(), "mixed").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(parsed.document.expressions(), again.document.expressions());
}

#[test]
fn structured_expression_metadata_roundtrips_across_loop_and_adhoc_owners() {
    let cases = [
        (
            include_str!("../../../tests/fixtures/bpmn20-schema/loops.bpmn")
                .replace("<loopCondition>", "<loopCondition id=\"exprA\">")
                .replace("<loopCardinality>", "<loopCardinality id=\"exprB\">")
                .replace(
                    "<completionCondition>",
                    "<completionCondition id=\"exprC\">",
                ),
            3,
        ),
        (
            include_str!("../../../tests/fixtures/bpmn20-schema/advanced-subprocess.bpmn").replace(
                "<completionCondition>",
                "<completionCondition id=\"exprD\">",
            ),
            1,
        ),
    ];
    for (input, count) in cases {
        let parsed = prepare_import(input.as_bytes(), "expressions").unwrap();
        assert!(parsed.report.editable);
        assert_eq!(parsed.document.expressions().len(), count);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(parsed.document.expressions(), again.document.expressions());
    }
}

#[test]
fn structured_expression_native_namespace_order_does_not_block_export() {
    use processcraft_core::bpmn_model::Expression;
    let mut parsed = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn"),
        "condition",
    )
    .unwrap();
    let Some(Expression::Structured(data)) = &mut parsed.document.processes[0].flows[0].condition
    else {
        panic!("structured expression expected")
    };
    data.namespaces.reverse();
    let output = export(&parsed);
    assert!(
        prepare_import(&output.bytes, "again")
            .unwrap()
            .report
            .editable
    );
}

#[test]
fn expression_type_and_unqualified_type_reference_preserve_omission_and_empty_namespace() {
    use processcraft_core::bpmn_model::{Expression, ExpressionKind};
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn"
    ))
    .unwrap();
    let old =
        "<conditionExpression xsi:type=\"bpmn:tFormalExpression\">x &gt; 3</conditionExpression>";
    for (replacement, expected_formal) in [
        (
            "<bpmn:conditionExpression xmlns=\"\" xsi:type=\"bpmn:tFormalExpression\" evaluatesToTypeRef=\"Item\">x</bpmn:conditionExpression>",
            true,
        ),
        (
            "<conditionExpression xsi:type=\"bpmn:tExpression\">x</conditionExpression>",
            false,
        ),
        ("<conditionExpression>x</conditionExpression>", false),
    ] {
        let input = source.replace(old, replacement);
        let parsed = prepare_import(input.as_bytes(), "type").unwrap();
        assert!(parsed.report.editable);
        let Some(Expression::Structured(data)) = &parsed.document.processes[0].flows[0].condition
        else {
            panic!("structured expression expected")
        };
        if expected_formal {
            assert!(
                matches!(&data.kind, ExpressionKind::Formal { explicit_type: true, evaluates_to_type_ref: Some(q), .. } if q.namespace.is_empty() && q.local.to_string() == "Item")
            );
        } else {
            assert!(
                matches!(&data.kind, ExpressionKind::Base { explicit_type } if *explicit_type == replacement.contains("xsi:type"))
            );
        }
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            parsed.document.processes[0].flows[0].condition,
            again.document.processes[0].flows[0].condition
        );
    }
}

#[test]
fn empty_expression_is_preserved_as_editable_model_data() {
    let input = FLAT.replace("<task id=\"task\" name=\"Review\"/>", "<task id=\"task\" name=\"Review\"><standardLoopCharacteristics><loopCondition id=\"expr\"/></standardLoopCharacteristics></task>");
    let parsed = prepare_import(input.as_bytes(), "empty expression").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(parsed.document.expressions(), again.document.expressions());
}

#[test]
fn inline_link_cancel_and_compensation_roundtrip() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/inline-events.bpmn");
    let parsed = prepare_import(input, "inline events").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        parsed.document.processes[0]
            .nodes
            .iter()
            .map(|n| &n.details)
            .collect::<Vec<_>>(),
        again.document.processes[0]
            .nodes
            .iter()
            .map(|n| &n.details)
            .collect::<Vec<_>>()
    );
}

#[test]
fn inline_events_preserve_unrepresented_or_invalid_content() {
    let fixture = include_str!("../../../tests/fixtures/bpmn/inline-events.bpmn");
    for input in [
        fixture.replace(
            "id=\"throwLink\"",
            "id=\"throwLink\" custom=\"unsupported\"",
        ),
        fixture.replace(
            "<target>tns:catchLink</target>",
            "<target>tns:activity</target>",
        ),
        fixture.replace(
            "attachedToRef=\"tns:transaction\"",
            "attachedToRef=\"tns:activity\"",
        ),
    ] {
        let parsed = prepare_import(input.as_bytes(), "invalid").unwrap();
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, input.as_bytes());
    }
}

#[test]
fn compensation_wait_attribute_presence_roundtrips() {
    let fixture = include_str!("../../../tests/fixtures/bpmn/inline-events.bpmn");
    for attribute in [
        "",
        " waitForCompletion=\"true\"",
        " waitForCompletion=\"false\"",
    ] {
        let input = fixture.replace(" waitForCompletion=\"false\"", attribute);
        let parsed = prepare_import(input.as_bytes(), "compensation").unwrap();
        assert!(parsed.report.editable);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            parsed.document.processes[0].nodes[4].details,
            again.document.processes[0].nodes[4].details
        );
    }
}

#[test]
fn supporting_catalog_qnames_and_presence_roundtrip() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    let parsed = prepare_import(input, "catalog").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let catalog = &parsed.document.catalog;
    assert_eq!(catalog.item_definitions.len(), 2);
    assert_eq!(catalog.item_definitions[0].is_collection, Some(false));
    assert!(
        catalog.item_definitions[1]
            .id
            .as_ref()
            .is_some_and(|id| id.xml_id().is_none())
    );
    assert_eq!(
        catalog.resources[0].parameters[1]
            .type_ref
            .as_ref()
            .unwrap()
            .namespace,
        ""
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn supporting_catalog_unrepresented_metadata_is_preserved() {
    let input = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    for variant in [
        input.replace(
            "<itemDefinition id=\"item\"",
            "<itemDefinition custom=\"x\" id=\"item\"",
        ),
        input.replace(
            "<resource id=\"analyst\" name=\"Analyst\">",
            "<resource id=\"analyst\" name=\"Analyst\"><documentation>note</documentation>",
        ),
        input.replace("<inMessageRef>tns:request</inMessageRef>", ""),
    ] {
        let parsed = prepare_import(variant.as_bytes(), "unsupported").unwrap();
        if parsed.report.editable {
            assert!(parsed.report.editable);
            assert!(
                prepare_import(&export(&parsed).bytes, "again")
                    .unwrap()
                    .report
                    .editable
            );
        } else {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, variant.as_bytes());
        }
    }
}

#[test]
fn supporting_catalog_external_operation_references_are_retained_without_resolution() {
    let input = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn")
        .replace(
            "<outMessageRef>tns:response</outMessageRef>",
            "<outMessageRef>svc:RemoteMessage</outMessageRef>",
        )
        .replace(
            "<errorRef>tns:failure</errorRef>",
            "<errorRef>svc:RemoteError</errorRef>",
        );
    let parsed = prepare_import(input.as_bytes(), "external references").unwrap();
    assert!(parsed.report.editable);
    let op = &parsed.document.catalog.interfaces[0].operations[0];
    assert_eq!(
        op.out_message_ref.as_ref().unwrap().namespace,
        "urn:service"
    );
    assert_eq!(op.error_refs[0].local.to_string(), "RemoteError");
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn catalog_only_prefixed_document_keeps_unqualified_message_qname() {
    let input = br#"<b:definitions xmlns:b="http://www.omg.org/spec/BPMN/20100524/MODEL" id="D" targetNamespace="urn:catalog"><b:interface name="Service"><b:operation name="Run"><b:inMessageRef>ExternalMessage</b:inMessageRef></b:operation></b:interface></b:definitions>"#;
    let parsed = prepare_import(input, "catalog").unwrap();
    assert!(parsed.report.editable);
    assert!(
        parsed.document.catalog.interfaces[0].operations[0]
            .in_message_ref
            .namespace
            .is_empty()
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn activity_message_and_operation_qnames_roundtrip_with_loop_context() {
    use processcraft_core::bpmn_model::Expression;
    let input = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn").replace(
        "<process id=\"process\" isExecutable=\"false\"/>",
        "<process id=\"process\" isExecutable=\"false\"><serviceTask id=\"serviceTask\" operationRef=\"tns:lookup\"><standardLoopCharacteristics><loopCondition id=\"loopExpr\" xmlns:tns=\"urn:shadow\" xmlns:body=\"urn:body\">body:value</loopCondition></standardLoopCharacteristics></serviceTask><receiveTask id=\"receive\" messageRef=\"tns:request\" operationRef=\"tns:lookup\"/><sendTask id=\"send\" messageRef=\"svc:RemoteMessage\" operationRef=\"svc:RemoteOperation\"/></process>",
    );
    let parsed = prepare_import(input.as_bytes(), "bindings").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let nodes = &parsed.document.processes[0].nodes;
    assert_eq!(
        nodes[0]
            .activity
            .as_ref()
            .unwrap()
            .operation_ref
            .as_ref()
            .unwrap()
            .namespace,
        "urn:catalog"
    );
    assert_eq!(
        nodes[2]
            .activity
            .as_ref()
            .unwrap()
            .message_ref
            .as_ref()
            .unwrap()
            .namespace,
        "urn:service"
    );
    assert!(matches!(
        parsed.document.expressions()[0],
        Expression::Structured(_)
    ));
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    let after = &again.document.processes[0].nodes;
    for index in 0..3 {
        assert_eq!(nodes[index].activity, after[index].activity);
    }
    assert_eq!(parsed.document.expressions(), again.document.expressions());
}

#[test]
fn activity_unqualified_qname_and_wrong_subtype_are_safe() {
    let source = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    let input = source.replace("<process id=\"process\" isExecutable=\"false\"/>", "<process id=\"process\" isExecutable=\"false\"><b:sendTask xmlns=\"\" id=\"send\" messageRef=\"ExternalMessage\" operationRef=\"ExternalOperation\"/></process>");
    let parsed = prepare_import(input.as_bytes(), "unqualified").unwrap();
    assert!(parsed.report.editable);
    let refs = parsed.document.processes[0].nodes[0]
        .activity
        .as_ref()
        .unwrap();
    assert!(refs.message_ref.as_ref().unwrap().namespace.is_empty());
    assert!(refs.operation_ref.as_ref().unwrap().namespace.is_empty());
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].nodes[0].activity,
        again.document.processes[0].nodes[0].activity
    );
    let wrong = source.replace("<process id=\"process\" isExecutable=\"false\"/>", "<process id=\"process\" isExecutable=\"false\"><manualTask id=\"wrong\" messageRef=\"tns:request\"/></process>");
    let preserved = prepare_import(wrong.as_bytes(), "wrong").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, wrong.as_bytes());
}

#[test]
fn activity_unqualified_operation_with_loop_and_documentation_roundtrips() {
    let source = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    let input = source.replace("<process id=\"process\" isExecutable=\"false\"/>", "<process id=\"process\" isExecutable=\"false\"><b:serviceTask xmlns=\"\" id=\"serviceTask\" operationRef=\"ExternalOperation\"><b:documentation>notes</b:documentation><b:standardLoopCharacteristics><b:loopCondition id=\"condition\">counter</b:loopCondition></b:standardLoopCharacteristics></b:serviceTask></process>");
    let parsed = prepare_import(input.as_bytes(), "unqualified loop").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].nodes[0].activity,
        again.document.processes[0].nodes[0].activity
    );
    assert_eq!(parsed.document.expressions(), again.document.expressions());
    assert_eq!(
        again.document.processes[0].nodes[0].documentation,
        "notes".into()
    );
}

#[test]
fn unqualified_activity_operation_with_io_and_data_associations_roundtrips() {
    let source = include_str!("../../../tests/fixtures/bpmn20-schema/data-io.bpmn").replace(
        "<dataObjectReference id=",
        "<dataObjectReference name=\"\" id=",
    );
    let input = source.replace("<task id=\"work\" name=\"Review\">", "<b:serviceTask xmlns:b=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" xmlns=\"\" id=\"work\" name=\"Review\" operationRef=\"ExternalOperation\">")
        .replace("</task>", "</b:serviceTask>")
        .replace("<ioSpecification id=", "<b:ioSpecification xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=")
        .replace("</ioSpecification>", "</b:ioSpecification>")
        .replace("<dataInputAssociation id=", "<b:dataInputAssociation xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=")
        .replace("</dataInputAssociation>", "</b:dataInputAssociation>")
        .replace("<dataOutputAssociation id=", "<b:dataOutputAssociation xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=")
        .replace("</dataOutputAssociation>", "</b:dataOutputAssociation>");
    let parsed = prepare_import(input.as_bytes(), "unqualified io").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        parsed.document.processes[0].data_associations,
        again.document.processes[0].data_associations
    );
    assert_eq!(
        parsed.document.processes[0].nodes.last().unwrap().activity,
        again.document.processes[0].nodes.last().unwrap().activity
    );
}

#[test]
fn activity_binding_to_wrong_local_kind_never_becomes_editable() {
    let source = include_str!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    for node in [
        "<serviceTask id=\"wrong\" operationRef=\"tns:request\"/>",
        "<sendTask id=\"wrong\" messageRef=\"tns:failure\"/>",
    ] {
        let input = source.replace(
            "<process id=\"process\" isExecutable=\"false\"/>",
            &format!("<process id=\"process\" isExecutable=\"false\">{node}</process>"),
        );
        if let Ok(parsed) = prepare_import(input.as_bytes(), "wrong kind") {
            assert!(!parsed.report.editable);
            assert_eq!(export(&parsed).bytes, input.as_bytes());
        }
    }
}

#[test]
fn item_aware_metadata_transformations_and_assignments_roundtrip() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/data-metadata.bpmn");
    let parsed = prepare_import(input, "data metadata").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let process = &parsed.document.processes[0];
    assert_eq!(
        process.data_objects[0]
            .item_aware
            .as_ref()
            .unwrap()
            .item_subject_ref
            .as_ref()
            .unwrap()
            .namespace,
        "urn:data-metadata"
    );
    let Some(processcraft_core::bpmn_model::NodeDetails::DataStoreReference {
        data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Qualified(reference)),
    }) = &process.nodes[1].details
    else {
        panic!("local store reference expected")
    };
    assert_eq!(reference.namespace, "urn:data-metadata");
    assert_eq!(reference.local.to_string(), "store");
    assert_eq!(process.data_associations[0].assignments.len(), 1);
    assert!(process.data_associations[0].transformation.is_some());
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    assert_eq!(
        process.data_objects,
        again.document.processes[0].data_objects
    );
    assert_eq!(
        process.data_associations,
        again.document.processes[0].data_associations
    );
    assert_eq!(parsed.document.expressions(), again.document.expressions());
}

#[test]
fn data_elements_allow_optional_xml_ids_and_process_io() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/data-optional.bpmn");
    let parsed = prepare_import(input, "optional data ids").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let process = &parsed.document.processes[0];
    assert!(
        parsed.document.data_stores[0]
            .id
            .as_ref()
            .is_some_and(|id| id.xml_id().is_none())
    );
    assert!(process.data_objects[0].id.is_none());
    assert!(
        process.io_specification.as_ref().unwrap().inputs[0]
            .id
            .is_none()
    );
    assert!(process.data_associations[0].id.is_none());
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    assert_eq!(
        process.data_objects,
        again.document.processes[0].data_objects
    );
    assert_eq!(
        process.io_specification,
        again.document.processes[0].io_specification
    );
    assert_eq!(
        process.data_associations,
        again.document.processes[0].data_associations
    );
}

#[test]
fn data_store_qualified_and_no_namespace_references_keep_identity() {
    let source = include_str!("../../../tests/fixtures/bpmn/data-metadata.bpmn");
    for replacement in [
        "<dataStoreReference name=\"Archive reference\" id=\"storeRef\" dataStoreRef=\"ext:ExternalStore\" itemSubjectRef=\"ext:RemoteType\"><dataState name=\"ready\"/></dataStoreReference>",
        "<b:dataStoreReference xmlns=\"\" name=\"Archive reference\" id=\"storeRef\" dataStoreRef=\"ExternalStore\" itemSubjectRef=\"RemoteType\"><b:dataState name=\"ready\"/></b:dataStoreReference>",
    ] {
        let input = source.replace("<dataStoreReference name=\"Archive reference\" id=\"storeRef\" dataStoreRef=\"tns:store\" itemSubjectRef=\"ext:RemoteType\"><dataState name=\"ready\"/></dataStoreReference>", replacement);
        let parsed = prepare_import(input.as_bytes(), "store ref").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let Some(processcraft_core::bpmn_model::NodeDetails::DataStoreReference {
            data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Qualified(reference)),
        }) = &parsed.document.processes[0].nodes[1].details
        else {
            panic!("qualified store reference expected")
        };
        assert_eq!(
            reference.namespace,
            if replacement.contains("ext:ExternalStore") {
                "urn:types"
            } else {
                ""
            }
        );
        assert_eq!(reference.local.to_string(), "ExternalStore");
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            parsed.document.processes[0].nodes[1].details,
            again.document.processes[0].nodes[1].details
        );
        assert_eq!(
            parsed.document.processes[0].nodes[1].item_aware,
            again.document.processes[0].nodes[1].item_aware
        );
    }
}

#[test]
fn unnamed_data_reference_preserves_name_omission() {
    let source = include_str!("../../../tests/fixtures/bpmn/data-metadata.bpmn");
    let input = source.replace(" name=\"Invoice reference\"", "");
    let parsed = prepare_import(input.as_bytes(), "unnamed reference").unwrap();
    assert!(parsed.report.editable);
    assert_eq!(parsed.document.processes[0].nodes[0].name, None);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(again.document.processes[0].nodes[0].name, None);
}

#[test]
fn unsupported_data_state_metadata_and_invalid_assignments_are_preserved() {
    let source = include_str!("../../../tests/fixtures/bpmn/data-metadata.bpmn");
    for input in [
        source.replace("<dataState id=\"storeState\" name=\"stored\"/>", "<dataState id=\"storeState\" name=\"stored\"><documentation>note</documentation></dataState>"),
        source.replace("<to id=\"toExpr\" xsi:type=\"b:tFormalExpression\" language=\"urn:expression\">ext:target</to>", ""),
        source.replace("<transformation id=\"transform\"", "<transformation xsi:type=\"b:tExpression\" id=\"transform\""),
    ] {
        if let Ok(parsed) = prepare_import(input.as_bytes(), "unsupported data") {
            if parsed.report.editable { assert!(parsed.report.editable); assert!(prepare_import(&export(&parsed).bytes, "again").unwrap().report.editable); }
            else { assert!(!parsed.report.editable); assert_eq!(export(&parsed).bytes, input.as_bytes()); }
        }
    }
}

#[test]
fn subprocess_io_is_never_silently_discarded() {
    let input = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><subProcess id="sub"><ioSpecification><dataInput id="input"/><inputSet/><outputSet/></ioSpecification><task id="inner"/></subProcess></process></definitions>"#;
    let parsed = prepare_import(input, "subprocess io").unwrap();
    if parsed.report.editable {
        assert!(
            parsed.document.processes[0].nodes[0]
                .io_specification
                .is_some()
        );
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            parsed.document.processes[0].nodes[0].io_specification,
            again.document.processes[0].nodes[0].io_specification
        );
    } else {
        assert_eq!(export(&parsed).bytes, input);
    }
}

#[test]
fn definitions_without_process_graph_roundtrip() {
    let inputs: &[&[u8]] = &[
        include_bytes!("../../../tests/fixtures/bpmn/root-only.bpmn"),
        br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:empty"/>"#,
        br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:roots"><message id="m"/><category id="c"/></definitions>"#,
    ];
    for input in inputs {
        let parsed = prepare_import(input, "definitions only").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        assert!(parsed.document.processes.is_empty());
        let output = export(&parsed);
        let again = prepare_import(&output.bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(
            processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
            processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
        );
        assert_eq!(
            parsed.document.event_resources,
            again.document.event_resources
        );
        assert_eq!(parsed.document.categories, again.document.categories);
    }
}

#[test]
fn empty_target_namespace_keeps_local_qnames_and_di() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/empty-namespace.bpmn");
    let parsed = prepare_import(input, "empty namespace").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert_eq!(
        parsed
            .document
            .definitions
            .as_ref()
            .unwrap()
            .target_namespace,
        ""
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again
            .document
            .definitions
            .as_ref()
            .unwrap()
            .target_namespace,
        ""
    );
    assert_eq!(parsed.document.expressions(), again.document.expressions());
}

#[test]
fn anonymous_elements_keep_native_identity_without_invented_xml_ids() {
    use processcraft_core::ids::ElementIdentity;
    let input = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" targetNamespace="urn:anonymous"><process isExecutable="false"><task/><task name=""/></process></definitions>"#;
    let parsed = prepare_import(input, "anonymous").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert!(matches!(
        parsed.document.definitions_id,
        ElementIdentity::Native { xml_id: None, .. }
    ));
    let process = &parsed.document.processes[0];
    assert!(matches!(
        process.id,
        ElementIdentity::Native { xml_id: None, .. }
    ));
    assert!(matches!(
        process.nodes[0].id,
        ElementIdentity::Native { xml_id: None, .. }
    ));
    assert_eq!(process.nodes[0].name, None);
    assert_eq!(process.nodes[1].name.as_deref(), Some(""));
    let output = export(&parsed);
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(!xml.contains(" id="));
    assert!(!xml.contains("native:"));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert_eq!(again.document.processes[0].nodes[0].name, None);
    assert_eq!(
        again.document.processes[0].nodes[1].name.as_deref(),
        Some("")
    );
}

#[test]
fn anonymous_di_target_requires_explicit_xml_id() {
    use processcraft_core::ids::ElementIdentity;
    let input = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:d="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:t="urn:test" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><task id="task"/></process><d:BPMNDiagram><d:BPMNPlane bpmnElement="t:p"><d:BPMNShape bpmnElement="t:task"><dc:Bounds x="0" y="0" width="100" height="80"/></d:BPMNShape></d:BPMNPlane></d:BPMNDiagram></definitions>"#;
    let mut parsed = prepare_import(input, "anonymous target").unwrap();
    assert!(parsed.report.editable);
    let identity = ElementIdentity::anonymous();
    parsed.document.processes[0].nodes[0].id = identity.clone();
    parsed.document.diagrams[0].shapes[0].element = identity.selector();
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources.clone();
    let failure = export_bpmn(&project, parsed.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .blocked
            .iter()
            .any(|s| s.contains("Assign an XML ID"))
    );
    if let ElementIdentity::Native { xml_id, .. } = &mut parsed.document.processes[0].nodes[0].id {
        *xml_id = Some("assigned".parse().unwrap());
    }
    let output = export(&parsed);
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.contains("id=\"assigned\""));
    assert!(xml.contains("bpmnElement=\"assigned\""));
    assert!(!xml.contains("native:"));
}

#[test]
fn process_is_executable_omission_and_explicit_values_survive() {
    for (attribute, expected) in [
        ("", None),
        (" isExecutable=\"false\"", Some(false)),
        (" isExecutable=\"true\"", Some(true)),
    ] {
        let input = format!(
            "<definitions xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=\"defs\" targetNamespace=\"urn:test\"><process id=\"p\"{attribute}/></definitions>"
        );
        let parsed = prepare_import(input.as_bytes(), "executable flag").unwrap();
        assert!(parsed.report.editable);
        assert_eq!(parsed.document.processes[0].is_executable, expected);
        let output = export(&parsed);
        let text = std::str::from_utf8(&output.bytes).unwrap();
        assert_eq!(text.contains("isExecutable="), expected.is_some());
        let again = prepare_import(&output.bytes, "again").unwrap();
        assert_eq!(again.document.processes[0].is_executable, expected);
    }
}

#[test]
fn legacy_implicit_and_explicit_hidden_di_labels_export_distinctly() {
    use processcraft_core::model::DiLabelVisibility;
    let mut imported = prepare_import(FLAT.as_bytes(), "legacy diagram labels").unwrap();
    let diagram = &mut imported.document.diagrams[0];
    diagram.shapes[0].label_visibility = None;
    diagram.shapes[0].label = None;
    diagram.edges[0].label_visibility = Some(DiLabelVisibility::Explicit);
    diagram.edges[0].label = None;
    let mut project = ProjectData::new("legacy diagram labels".into());
    project.documents.push(Arc::new(imported.document.clone()));
    project.resources = imported.resources;
    let before = project.content_identity().unwrap();
    let bytes = processcraft_io::native::write_bytes(&project).unwrap();
    let reopened = processcraft_io::native::load_bytes(&bytes).unwrap().project;
    assert_eq!(reopened.content_identity().unwrap(), before);
    let output = export_bpmn(&reopened, imported.document.id, false).unwrap();
    assert!(
        output
            .report
            .transformed
            .iter()
            .any(|entry| entry.contains("implicit diagram labels"))
    );
    assert_eq!(reopened.content_identity().unwrap(), before);
    let again = prepare_import(&output.bytes, "again").unwrap();
    let label = again.document.diagrams[0].shapes[0].label.as_ref().unwrap();
    assert!(label.id.is_none() && label.bounds.is_none() && label.style_ref.is_none());
    assert!(again.document.diagrams[0].edges[0].label.is_none());
    assert_eq!(
        again.document.diagrams[0].edges[0].label_visibility,
        Some(DiLabelVisibility::Explicit)
    );
}

#[test]
fn legacy_implicit_di_label_keeps_existing_geometry() {
    use processcraft_core::model::{Bounds, Label};
    let mut imported = prepare_import(FLAT.as_bytes(), "legacy label geometry").unwrap();
    let shape = &mut imported.document.diagrams[0].shapes[0];
    shape.label_visibility = None;
    shape.label = Some(Label {
        di_metadata: None,
        id: None,
        bounds: Some(Bounds {
            x: 12.0,
            y: 34.0,
            width: 56.0,
            height: 20.0,
        }),
        style: None,
        style_ref: None,
    });
    let output = export(&imported);
    let again = prepare_import(&output.bytes, "again").unwrap();
    let label = again.document.diagrams[0].shapes[0].label.as_ref().unwrap();
    assert!(label.id.is_none());
    assert_eq!(label.bounds.as_ref().unwrap().x, 12.0);
    assert_eq!(label.bounds.as_ref().unwrap().width, 56.0);
}

#[test]
fn legacy_annotation_edit_and_undo_export_correct_body() {
    use processcraft_core::bpmn_model::NodeDetails;
    use processcraft_core::commands::{Envelope, Mutation, Request, Session};
    let input = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:test"><process id="p" isExecutable="false"><textAnnotation id="note" textFormat="text/plain"><text>old body</text></textAnnotation></process></definitions>"#;
    let mut parsed = prepare_import(input, "annotation").unwrap();
    let node = &mut parsed.document.processes[0].nodes[0];
    node.name = Some("old body".into());
    let Some(NodeDetails::TextAnnotation { text, .. }) = &mut node.details else {
        panic!("annotation expected")
    };
    *text = None;
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources.clone();
    let mut session = Session::new(project).unwrap();
    let envelope = |session: &Session| Envelope {
        request_id: "annotation".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session
        .apply(
            envelope(&session),
            Request::Batch {
                mutations: vec![Mutation::SetTextAnnotation {
                    document_id: parsed.document.id,
                    element_id: parsed.document.processes[0].nodes[0].id.selector(),
                    text: Some("new body".into()),
                }],
            },
        )
        .unwrap();
    let output = export_bpmn(&session.snapshot().project, parsed.document.id, false).unwrap();
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.contains("<text>new body</text>"));
    assert!(!xml.contains("old body"));
    session.apply(envelope(&session), Request::Undo).unwrap();
    let output = export_bpmn(&session.snapshot().project, parsed.document.id, false).unwrap();
    assert!(
        std::str::from_utf8(&output.bytes)
            .unwrap()
            .contains("<text>old body</text>")
    );
}

#[test]
fn empty_target_namespace_event_qnames_keep_local_identity() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/empty-namespace-events.bpmn");
    let parsed = prepare_import(input, "empty namespace events").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        parsed.document.processes[0]
            .nodes
            .iter()
            .map(|node| &node.details)
            .collect::<Vec<_>>(),
        again.document.processes[0]
            .nodes
            .iter()
            .map(|node| &node.details)
            .collect::<Vec<_>>()
    );
}

#[test]
fn anonymous_di_representation_ids_are_not_invented() {
    let input = include_bytes!("../../../tests/fixtures/bpmn/anonymous-identities.bpmn");
    let parsed = prepare_import(input, "anonymous identity").unwrap();
    assert!(parsed.report.editable);
    assert!(parsed.document.definitions_id.xml_id().is_none());
    assert!(parsed.document.diagrams[0].id.xml_id().is_none());
    assert!(parsed.document.diagrams[0].plane_id.xml_id().is_none());
    assert!(parsed.document.diagrams[0].shapes[0].id.xml_id().is_none());
    let output = export(&parsed);
    let text = std::str::from_utf8(&output.bytes).unwrap();
    assert!(!text.contains("native:"));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.document.definitions_id.xml_id().is_none());
    assert!(again.document.diagrams[0].plane_id.xml_id().is_none());
    assert!(again.document.diagrams[0].shapes[0].id.xml_id().is_none());
    assert_eq!(again.document.processes[0].nodes[1].name, None);
    assert_eq!(
        again.document.processes[0].nodes[2].name.as_deref(),
        Some("")
    );
}

#[test]
fn collaboration_and_diagram_names_preserve_absence_and_explicit_empty() {
    let source = include_str!("../../../tests/fixtures/bpmn20-schema/conversation.bpmn");
    for (name_attribute, expected) in [
        ("", None),
        (" name=\"\"", Some("")),
        (" name=\"Collaboration\"", Some("Collaboration")),
    ] {
        let input = source.replacen(
            "<collaboration ",
            &format!("<collaboration{name_attribute} "),
            1,
        );
        let parsed = prepare_import(input.as_bytes(), "collaboration name").unwrap();
        assert!(parsed.report.editable);
        assert_eq!(
            parsed
                .document
                .collaboration
                .as_ref()
                .unwrap()
                .name
                .as_deref(),
            expected
        );
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            again
                .document
                .collaboration
                .as_ref()
                .unwrap()
                .name
                .as_deref(),
            expected
        );
    }
    let source = include_str!("../../../tests/fixtures/bpmn/anonymous-identities.bpmn");
    for (name_attribute, expected) in [
        ("", None),
        (" name=\"\"", Some("")),
        (" name=\"Diagram\"", Some("Diagram")),
    ] {
        let input = source.replacen(
            "<di:BPMNDiagram",
            &format!("<di:BPMNDiagram{name_attribute}"),
            1,
        );
        let parsed = prepare_import(input.as_bytes(), "diagram name").unwrap();
        assert!(parsed.report.editable);
        assert_eq!(parsed.document.diagrams[0].name.as_deref(), expected);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(again.document.diagrams[0].name.as_deref(), expected);
    }
}

#[test]
fn empty_target_namespace_collaboration_and_choreography_qnames_roundtrip() {
    for input in [
        include_bytes!("../../../tests/fixtures/bpmn/empty-namespace-conversation.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn/empty-namespace-choreography.bpmn").as_slice(),
    ] {
        let parsed = prepare_import(input, "empty namespace collaboration").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(
            parsed.document.collaboration.is_some(),
            again.document.collaboration.is_some()
        );
        assert_eq!(
            parsed.document.choreographies.len(),
            again.document.choreographies.len()
        );
    }
}

#[test]
fn annotation_absent_and_empty_body_remain_distinct() {
    use processcraft_core::bpmn_model::NodeDetails;
    for (child, expected) in [("", None), ("<text/>", Some(""))] {
        let input = format!(
            "<definitions xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=\"defs\" targetNamespace=\"urn:test\"><process id=\"p\" isExecutable=\"false\"><textAnnotation id=\"note\">{child}</textAnnotation></process></definitions>"
        );
        let parsed = prepare_import(input.as_bytes(), "annotation presence").unwrap();
        assert!(parsed.report.editable);
        let Some(NodeDetails::TextAnnotation { text, .. }) =
            &parsed.document.processes[0].nodes[0].details
        else {
            panic!("annotation expected")
        };
        assert_eq!(text.as_ref().and_then(|x| x.plain_text()), expected);
        let output = export(&parsed);
        let xml = std::str::from_utf8(&output.bytes).unwrap();
        assert_eq!(xml.contains("<text>"), expected.is_some());
        let again = prepare_import(&output.bytes, "again").unwrap();
        let Some(NodeDetails::TextAnnotation { text, .. }) =
            &again.document.processes[0].nodes[0].details
        else {
            panic!("annotation expected")
        };
        assert_eq!(text.as_ref().and_then(|x| x.plain_text()), expected);
    }
}

#[test]
fn empty_target_namespace_groups_and_event_resources_roundtrip() {
    for input in [
        include_bytes!("../../../tests/fixtures/bpmn/empty-namespace-groups.bpmn").as_slice(),
        include_bytes!("../../../tests/fixtures/bpmn/empty-namespace-message-signal.bpmn")
            .as_slice(),
    ] {
        let parsed = prepare_import(input, "empty namespace roots").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert_eq!(parsed.document.categories, again.document.categories);
        assert_eq!(
            parsed.document.event_resources,
            again.document.event_resources
        );
        assert_eq!(
            parsed.document.processes[0].groups,
            again.document.processes[0].groups
        );
        assert_eq!(
            parsed.document.processes[0]
                .nodes
                .iter()
                .map(|n| &n.details)
                .collect::<Vec<_>>(),
            again.document.processes[0]
                .nodes
                .iter()
                .map(|n| &n.details)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn call_activity_resolves_xml_id_of_native_process_identity() {
    use processcraft_core::ids::ElementIdentity;
    let mut parsed = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn20-schema/nested.bpmn"),
        "nested",
    )
    .unwrap();
    let called = &mut parsed.document.processes[1];
    let mut identity = ElementIdentity::anonymous();
    if let ElementIdentity::Native { xml_id, .. } = &mut identity {
        *xml_id = Some("called".parse().unwrap());
    }
    let old_selector = called.id.selector();
    called.id = identity;
    let new_selector = called.id.selector();
    for selector in parsed.document.root_order.as_mut().unwrap() {
        if *selector == old_selector {
            *selector = new_selector.clone();
        }
    }
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
}

#[test]
fn activity_port_di_is_preserved_or_refused_while_process_port_di_roundtrips() {
    use processcraft_core::model::{Bounds, Diagram, Shape};
    let source = include_str!("../../../tests/fixtures/bpmn/data-metadata.bpmn");
    let with_activity_port = source.replace("<definitions ", "<definitions xmlns:di=\"http://www.omg.org/spec/BPMN/20100524/DI\" xmlns:dc=\"http://www.omg.org/spec/DD/20100524/DC\" ")
        .replace("</definitions>", "<di:BPMNDiagram id=\"diagram\"><di:BPMNPlane id=\"plane\" bpmnElement=\"tns:process\"><di:BPMNShape id=\"portShape\" bpmnElement=\"tns:input\"><dc:Bounds x=\"0\" y=\"0\" width=\"50\" height=\"50\"/></di:BPMNShape></di:BPMNPlane></di:BPMNDiagram></definitions>");
    let preserved = prepare_import(with_activity_port.as_bytes(), "activity port").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, with_activity_port.as_bytes());

    let mut parsed = prepare_import(source.as_bytes(), "native activity port").unwrap();
    parsed.document.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: Vec::new(),
        id: "diagram".parse().unwrap(),
        name: None,
        plane_id: "plane".parse().unwrap(),
        plane: "process".parse().unwrap(),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            id: "portShape".parse().unwrap(),
            element: "input".parse().unwrap(),
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    let mut project = ProjectData::new("test".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources;
    let failure = export_bpmn(&project, parsed.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .blocked
            .iter()
            .any(|message| message.contains("data input/output ports"))
    );

    let process_port = include_str!("../../../tests/fixtures/bpmn/data-optional.bpmn")
        .replace("<definitions ", "<definitions xmlns:tns=\"urn:data-optional\" xmlns:di=\"http://www.omg.org/spec/BPMN/20100524/DI\" xmlns:dc=\"http://www.omg.org/spec/DD/20100524/DC\" ")
        .replacen("<dataInput/>", "<dataInput id=\"processPort\"/>", 1)
        .replace("</definitions>", "<di:BPMNDiagram id=\"diagram\"><di:BPMNPlane id=\"plane\" bpmnElement=\"tns:process\"><di:BPMNShape id=\"portShape\" bpmnElement=\"tns:processPort\"><dc:Bounds x=\"0\" y=\"0\" width=\"50\" height=\"50\"/></di:BPMNShape></di:BPMNPlane></di:BPMNDiagram></definitions>");
    let parsed = prepare_import(process_port.as_bytes(), "process port").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert!(
        prepare_import(&export(&parsed).bytes, "again")
            .unwrap()
            .report
            .editable
    );
}

#[test]
fn diagram_local_qnames_keep_expanded_names_with_unqualified_lexical_values() {
    use processcraft_io::bpmn::{ExpandedName, import_bpmn};
    for (input, namespace, names) in [
        (
            include_bytes!("../../../tests/fixtures/bpmn/m2-flat-lanes.bpmn").as_slice(),
            Some("urn:test"),
            vec!["p", "task", "f1"],
        ),
        (
            include_bytes!("../../../tests/fixtures/bpmn/empty-namespace.bpmn").as_slice(),
            None,
            vec!["outer", "task"],
        ),
    ] {
        let original = import_bpmn(input).unwrap();
        let parsed = prepare_import(input, "diagram namespace").unwrap();
        assert!(parsed.report.editable);
        let output = export(&parsed);
        let reopened = import_bpmn(&output.bytes).unwrap();
        let xml = std::str::from_utf8(&output.bytes).unwrap();
        for name in names {
            let expanded = ExpandedName {
                namespace_uri: namespace.map(str::to_owned),
                local_name: name.into(),
            };
            assert!(original.report().qname_values.contains(&expanded));
            assert!(reopened.report().qname_values.contains(&expanded));
            assert!(xml.contains(&format!("bpmnElement=\"{name}\"")));
            assert!(!xml.contains(&format!("bpmnElement=\"tns:{name}\"")));
        }
        assert!(
            prepare_import(&output.bytes, "again")
                .unwrap()
                .report
                .editable
        );
    }
}

#[test]
fn event_root_refs_flags_properties_and_owned_io_roundtrip() {
    use processcraft_core::bpmn_model::{EventDefinition, EventIo, EventReference, NodeDetails};
    let input = include_bytes!("../../../tests/fixtures/bpmn/event-ownership.bpmn");
    let parsed = prepare_import(input, "event ownership").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert_eq!(parsed.document.root_event_definitions.len(), 2);
    let nodes = &parsed.document.processes[0].nodes;
    let Some(NodeDetails::Event {
        is_interrupting, ..
    }) = &nodes[1].details
    else {
        panic!("start event expected")
    };
    assert_eq!(*is_interrupting, Some(false));
    let Some(NodeDetails::Event {
        definitions,
        definition_refs,
        parallel_multiple,
        properties,
        io,
        ..
    }) = &nodes[2].details
    else {
        panic!("catch event expected")
    };
    assert_eq!(*parallel_multiple, Some(true));
    assert_eq!(properties.len(), 1);
    assert!(matches!(
        definitions.as_slice(),
        [EventDefinition::Timer { kind: Some(_), .. }]
    ));
    assert!(matches!(
        definition_refs.as_slice(),
        [EventReference::Local(_), EventReference::Qualified(_)]
    ));
    assert!(
        matches!(io, Some(EventIo::Catch { outputs, output_set: Some(_) }) if outputs.len() == 1)
    );
    let Some(NodeDetails::Event {
        definitions, io, ..
    }) = &nodes[3].details
    else {
        panic!("throw event expected")
    };
    assert!(matches!(
        definitions.as_slice(),
        [EventDefinition::Message {
            operation_ref: Some(EventReference::Local(_)),
            ..
        }]
    ));
    assert!(matches!(io, Some(EventIo::Throw { inputs, input_set: Some(_) }) if inputs.len() == 1));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        parsed.document.root_event_definitions,
        again.document.root_event_definitions
    );
    assert_eq!(
        parsed.document.event_resources,
        again.document.event_resources
    );
    assert_eq!(
        parsed.document.processes[0].data_associations,
        again.document.processes[0].data_associations
    );
    assert_eq!(
        parsed.document.processes[0]
            .nodes
            .iter()
            .map(|node| &node.details)
            .collect::<Vec<_>>(),
        again.document.processes[0]
            .nodes
            .iter()
            .map(|node| &node.details)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_timer_choice_and_event_flags_preserve_omission() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    for (start_attr, expected_interrupting) in [
        ("", None),
        (" isInterrupting=\"false\"", Some(false)),
        (" isInterrupting=\"true\"", Some(true)),
    ] {
        for (catch_attr, expected_parallel) in [
            ("", None),
            (" parallelMultiple=\"false\"", Some(false)),
            (" parallelMultiple=\"true\"", Some(true)),
        ] {
            let input = format!(
                "<definitions xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=\"defs\" targetNamespace=\"urn:test\"><timerEventDefinition id=\"rootTimer\"/><process id=\"process\" isExecutable=\"false\"><startEvent id=\"start\"{start_attr}><timerEventDefinition/></startEvent><intermediateCatchEvent id=\"catch\"{catch_attr}><timerEventDefinition/></intermediateCatchEvent></process></definitions>"
            );
            let parsed = prepare_import(input.as_bytes(), "empty timer").unwrap();
            assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
            assert!(matches!(
                parsed.document.root_event_definitions.as_slice(),
                [EventDefinition::Timer {
                    kind: None,
                    expression: None,
                    ..
                }]
            ));
            let Some(NodeDetails::Event {
                is_interrupting, ..
            }) = &parsed.document.processes[0].nodes[0].details
            else {
                panic!("start event")
            };
            assert_eq!(*is_interrupting, expected_interrupting);
            let Some(NodeDetails::Event {
                parallel_multiple, ..
            }) = &parsed.document.processes[0].nodes[1].details
            else {
                panic!("catch event")
            };
            assert_eq!(*parallel_multiple, expected_parallel);
            let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
            assert_eq!(
                parsed.document.root_event_definitions,
                again.document.root_event_definitions
            );
            assert_eq!(
                parsed.document.processes[0].nodes[0].details,
                again.document.processes[0].nodes[0].details
            );
            assert_eq!(
                parsed.document.processes[0].nodes[1].details,
                again.document.processes[0].nodes[1].details
            );
        }
    }
}

#[test]
fn event_definition_reference_must_target_root_and_opaque_metadata_is_preserved() {
    let source = include_str!("../../../tests/fixtures/bpmn/event-ownership.bpmn");
    for (index, input) in [
        source.replacen("<eventDefinitionRef>tns:sharedSignal</eventDefinitionRef>", "<eventDefinitionRef>tns:inlineTimer</eventDefinitionRef>", 1),
        source.replacen("<eventDefinitionRef>tns:sharedSignal</eventDefinitionRef>", "<eventDefinitionRef>tns:message</eventDefinitionRef>", 1),
        source.replace("<signalEventDefinition id=\"sharedSignal\" signalRef=\"tns:signal\"/>", "<signalEventDefinition id=\"sharedSignal\" signalRef=\"tns:signal\"><documentation>opaque metadata</documentation></signalEventDefinition>"),
    ].into_iter().enumerate() {
        if let Ok(parsed) = prepare_import(input.as_bytes(), "unsupported event") {
            if index == 2 { assert!(parsed.report.editable); assert!(prepare_import(&export(&parsed).bytes, "again").unwrap().report.editable); }
            else { assert!(!parsed.report.editable); assert_eq!(export(&parsed).bytes, input.as_bytes()); }
        }
    }
}

#[test]
fn explicit_di_representation_endpoints_and_shape_flags_roundtrip() {
    let source = include_str!("../../../tests/fixtures/bpmn/di-endpoints.bpmn");
    let input = source
        .replace("targetRef=\"tns:flow\"", "targetRef=\"tns:a\"")
        .replace(
            "targetElement=\"tns:edgeFlow\"",
            "targetElement=\"tns:shapeA\"",
        );
    for (input, expected_target) in [(source, "edgeFlow"), (input.as_str(), "shapeA")] {
        let parsed = prepare_import(input.as_bytes(), "DI endpoints").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let diagram = &parsed.document.diagrams[0];
        assert_eq!(diagram.shapes[0].is_horizontal, Some(false));
        assert_eq!(diagram.shapes[0].is_marker_visible, Some(true));
        assert_eq!(
            diagram.edges[1]
                .source_element
                .as_ref()
                .unwrap()
                .to_string(),
            "shapeNote"
        );
        assert_eq!(
            diagram.edges[1]
                .target_element
                .as_ref()
                .unwrap()
                .to_string(),
            expected_target
        );
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert_eq!(
            again.document.diagrams[0].edges[1].source_element,
            diagram.edges[1].source_element
        );
        assert_eq!(
            again.document.diagrams[0].edges[1].target_element,
            diagram.edges[1].target_element
        );
        assert_eq!(
            again.document.diagrams[0].shapes[0].is_horizontal,
            Some(false)
        );
        assert_eq!(
            again.document.diagrams[0].shapes[0].is_marker_visible,
            Some(true)
        );
    }
}

#[test]
fn event_owned_port_shape_remains_preserved() {
    let source = include_str!("../../../tests/fixtures/bpmn/event-ownership.bpmn");
    let input = source.replace("<definitions ", "<definitions xmlns:di=\"http://www.omg.org/spec/BPMN/20100524/DI\" xmlns:dc=\"http://www.omg.org/spec/DD/20100524/DC\" ")
        .replace("</definitions>", "<di:BPMNDiagram id=\"diagram\"><di:BPMNPlane id=\"plane\" bpmnElement=\"tns:process\"><di:BPMNShape id=\"portShape\" bpmnElement=\"tns:output\"><dc:Bounds x=\"0\" y=\"0\" width=\"50\" height=\"50\"/></di:BPMNShape></di:BPMNPlane></di:BPMNDiagram></definitions>");
    let parsed = prepare_import(input.as_bytes(), "event port").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, input.as_bytes());
}

#[test]
fn activity_port_association_requires_explicit_di_endpoint() {
    let source = include_str!("../../../tests/fixtures/bpmn20-schema/data-io.bpmn")
        .replace("<dataObjectReference id=", "<dataObjectReference name=\"\" id=")
        .replace("<definitions ", "<definitions xmlns:t=\"urn:io\" xmlns:bd=\"http://www.omg.org/spec/BPMN/20100524/DI\" xmlns:dc=\"http://www.omg.org/spec/DD/20100524/DC\" xmlns:di=\"http://www.omg.org/spec/DD/20100524/DI\" ")
        .replace("</definitions>", "<bd:BPMNDiagram id=\"diagram\"><bd:BPMNPlane id=\"plane\" bpmnElement=\"t:p\"><bd:BPMNShape id=\"workShape\" bpmnElement=\"t:work\"><dc:Bounds x=\"100\" y=\"0\" width=\"100\" height=\"80\"/></bd:BPMNShape><bd:BPMNShape id=\"dataShape\" bpmnElement=\"t:invoiceRef\"><dc:Bounds x=\"0\" y=\"0\" width=\"50\" height=\"50\"/></bd:BPMNShape><bd:BPMNEdge id=\"inputEdge\" bpmnElement=\"t:inputAssoc\" targetElement=\"t:workShape\"><di:waypoint x=\"50\" y=\"25\"/><di:waypoint x=\"100\" y=\"25\"/></bd:BPMNEdge></bd:BPMNPlane></bd:BPMNDiagram></definitions>");
    let parsed = prepare_import(source.as_bytes(), "explicit port endpoint").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.diagrams[0].edges[0]
            .target_element
            .as_ref()
            .unwrap()
            .to_string(),
        "workShape"
    );
    let mut project = ProjectData::new("missing endpoint".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources.clone();
    Arc::make_mut(&mut project.documents[0]).diagrams[0].edges[0].target_element = None;
    let failure = export_bpmn(&project, parsed.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .blocked
            .iter()
            .any(|reason| reason.contains("target")),
        "{:?}",
        failure.report.blocked
    );
}

#[test]
fn diagram_label_style_and_empty_label_roundtrip() {
    let source = include_str!("../../../tests/fixtures/bpmn/di-label-styles.bpmn");
    let parsed = prepare_import(source.as_bytes(), "label styles").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let diagram = &parsed.document.diagrams[0];
    assert_eq!(diagram.documentation.as_deref(), Some("Diagram notes"));
    assert_eq!(diagram.resolution, Some(96.0));
    assert_eq!(diagram.label_styles.len(), 1);
    assert_eq!(
        diagram.label_styles[0].font.as_ref().unwrap().is_bold,
        Some(true)
    );
    let label = diagram.shapes[0].label.as_ref().unwrap();
    assert!(label.bounds.is_none());
    assert_eq!(
        label.id.as_ref().unwrap().xml_id().unwrap().to_string(),
        "label"
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.diagrams[0].documentation,
        diagram.documentation
    );
    assert_eq!(again.document.diagrams[0].resolution, diagram.resolution);
    assert_eq!(
        again.document.diagrams[0].label_styles,
        diagram.label_styles
    );
    assert!(
        again.document.diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .is_none()
    );
}

#[test]
fn process_documentation_mixed_order_and_namespaces_roundtrip() {
    use processcraft_core::bpmn_metadata::DocumentationSet;
    let source = include_str!("../../../tests/fixtures/bpmn/process-documentation.bpmn");
    let parsed = prepare_import(source.as_bytes(), "mixed documentation").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let DocumentationSet::Structured(docs) = &parsed.document.processes[0].documentation else {
        panic!("structured docs expected")
    };
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].text_format.as_deref(), Some("text/plain"));
    assert!(docs[1].id.is_none());
    let output = export(&parsed).bytes;
    let again = prepare_import(&output, "again").unwrap();
    assert_eq!(
        again.document.processes[0].documentation,
        parsed.document.processes[0].documentation
    );
}

#[test]
fn identified_base_owner_metadata_roundtrips() {
    let source = include_str!("../../../tests/fixtures/bpmn/base-metadata.bpmn");
    let parsed = prepare_import(source.as_bytes(), "base metadata").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let process = &parsed.document.processes[0];
    assert_eq!(
        process
            .base_extras
            .as_ref()
            .unwrap()
            .foreign_attributes
            .len(),
        1
    );
    assert!(
        process
            .base_extras
            .as_ref()
            .unwrap()
            .extension_elements
            .is_some()
    );
    let task = process
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "next")
        .unwrap();
    assert!(
        task.base_extras
            .as_ref()
            .unwrap()
            .extension_elements
            .is_some()
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(again.document.processes[0].base_extras, process.base_extras);
    assert_eq!(
        again.document.processes[0]
            .nodes
            .iter()
            .find(|n| n.id.to_string() == "next")
            .unwrap()
            .base_extras,
        task.base_extras
    );
    assert_eq!(
        again.document.processes[0]
            .nodes
            .iter()
            .find(|n| n.id.to_string() == "next")
            .unwrap()
            .documentation,
        task.documentation
    );
}

#[test]
fn expression_inherited_documentation_mixed_order_roundtrips() {
    let source = include_str!("../../../tests/fixtures/bpmn/common-metadata.bpmn")
        .replace(" v:flag=\"v:value\"", "")
        .replace("    <extensionElements><v:settings v:ref=\"t:task\"><v:nested xmlns:t=\"urn:shadow\" value=\"t:item\"/></v:settings></extensionElements>\n", "")
        .replace("<script>pre<v:code expression=\"v:item\"/>post</script>", "")
        .replace("<text>pre<v:note/>post</text>", "<text>plain</text>");
    let parsed = prepare_import(source.as_bytes(), "mixed expression").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let original = parsed.document.expressions();
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(again.document.expressions(), original);
}

#[test]
fn script_and_annotation_mixed_xml_roundtrip_inertly() {
    let source = include_str!("../../../tests/fixtures/bpmn/common-metadata.bpmn");
    let parsed = prepare_import(source.as_bytes(), "mixed bodies").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.processes[0].nodes[0].activity,
        parsed.document.processes[0].nodes[0].activity
    );
    assert_eq!(
        again.document.processes[0].nodes.last().unwrap().details,
        parsed.document.processes[0].nodes.last().unwrap().details
    );
    assert_eq!(again.document.expressions(), parsed.document.expressions());
}

#[test]
fn definitions_declarations_and_required_unknown_extension_policy() {
    let source = include_str!("../../../tests/fixtures/bpmn/definitions-declarations.bpmn");
    let parsed = prepare_import(source.as_bytes(), "declarations").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert_eq!(parsed.document.extensions[0].must_understand, Some(false));
    assert_eq!(
        parsed.document.relationships[0].sources[0].namespace,
        "urn:definitions"
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(again.document.extensions, parsed.document.extensions);
    assert_eq!(again.document.relationships, parsed.document.relationships);
    let required = source.replace("mustUnderstand=\"false\"", "mustUnderstand=\"true\"");
    let protected = prepare_import(required.as_bytes(), "unknown required extension").unwrap();
    assert!(!protected.report.editable);
    assert_eq!(export(&protected).bytes, required.as_bytes());
}

#[test]
fn anonymous_metadata_stays_on_its_typed_owner() {
    let source = include_str!("../../../tests/fixtures/bpmn/anonymous-common-metadata.bpmn");
    let parsed = prepare_import(source.as_bytes(), "anonymous metadata").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let catalog = &parsed.document.catalog;
    assert!(
        catalog.item_definitions[0]
            .id
            .as_ref()
            .is_some_and(|id| id.xml_id().is_none())
    );
    assert!(
        catalog.interfaces[0]
            .id
            .as_ref()
            .is_some_and(|id| id.xml_id().is_none())
    );
    assert!(
        catalog.interfaces[0]
            .operations
            .iter()
            .all(|op| op.id.is_none() && op.base.is_some())
    );
    assert_ne!(
        catalog.interfaces[0].operations[0].base,
        catalog.interfaces[0].operations[1].base
    );
    let first = &catalog.interfaces[0].operations[0]
        .base
        .as_ref()
        .unwrap()
        .documentation[0]
        .content;
    let second = &catalog.interfaces[0].operations[1]
        .base
        .as_ref()
        .unwrap()
        .documentation[0]
        .content;
    let text = |body: &processcraft_core::bpmn_xml::MixedXml| match body {
        processcraft_core::bpmn_xml::MixedXml::Structured(content) => {
            match content.nodes.as_slice() {
                [processcraft_core::bpmn_xml::XmlNode::Text { text }] => text.clone(),
                _ => panic!("plain documentation expected"),
            }
        }
        _ => panic!("structured documentation expected"),
    };
    assert_eq!(text(first), "operation one");
    assert_eq!(text(second), "operation two");
    assert!(parsed.document.processes[2].lane_set_base.is_some());
    assert!(
        parsed.document.processes[1].lanes[0]
            .child_lane_set_base
            .is_some()
    );
    let task = parsed.document.processes[0]
        .nodes
        .iter()
        .find(|node| node.id.to_string() == "task")
        .unwrap();
    let Some(processcraft_core::bpmn_model::LoopCharacteristics::Standard {
        id: Some(loop_id),
        base: Some(_),
        ..
    }) = &task.loop_characteristics
    else {
        panic!("loop identity and metadata expected")
    };
    assert_eq!(loop_id.xml_id().unwrap().to_string(), "loop");
    let output = export(&parsed).bytes;
    let again = prepare_import(&output, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    assert_eq!(
        again.document.processes[2].lane_set_base,
        parsed.document.processes[2].lane_set_base
    );
}

#[test]
fn anonymous_owner_foreign_context_and_sibling_order_roundtrip() {
    let source = include_str!("../../../tests/fixtures/bpmn/anonymous-common-metadata.bpmn")
        .replace("<resourceParameter name=\"first\"><documentation>parameter first</documentation>", "<resourceParameter xmlns:v=\"urn:vendor\" v:flag=\"v:first\" name=\"first\"><documentation>before<v:detail data-processcraft-internal-context=\"5b5d\" ref=\"v:item\"/>after<!--note--><?inspect inert?></documentation><extensionElements><v:payload ref=\"v:first\"/></extensionElements>")
        .replace("<resourceParameter name=\"second\"><documentation>parameter second</documentation>", "<resourceParameter xmlns:v=\"urn:other\" v:flag=\"v:second\" name=\"second\"><documentation>second<v:detail/>tail</documentation><extensionElements/>");
    let parsed = prepare_import(source.as_bytes(), "anonymous scopes").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let parameters = &parsed.document.catalog.resources[0].parameters;
    assert_ne!(parameters[0].base, parameters[1].base);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(again.document.catalog.resources[0].parameters, *parameters);
    let mut reordered = parsed;
    let base = reordered.document.catalog.resources[0].parameters[0]
        .base
        .as_mut()
        .unwrap();
    base.namespaces.reverse();
    if let processcraft_core::bpmn_xml::MixedXml::Structured(content) =
        &mut base.documentation[0].content
    {
        content.namespaces.reverse();
    }
    assert!(
        prepare_import(&export(&reordered).bytes, "reordered contexts")
            .unwrap()
            .report
            .editable
    );
}

#[test]
fn anonymous_unreferenced_process_and_task_keep_metadata_without_xml_ids() {
    let source = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" targetNamespace="urn:anonymous"><process><documentation>process note</documentation><task><documentation>task note</documentation></task></process></definitions>"#;
    let parsed = prepare_import(source, "anonymous graph").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert!(parsed.document.processes[0].id.xml_id().is_none());
    assert!(parsed.document.processes[0].nodes[0].id.xml_id().is_none());
    let output = export(&parsed).bytes;
    let xml = std::str::from_utf8(&output).unwrap();
    assert!(!xml.contains(" id="));
    let again = prepare_import(&output, "again").unwrap();
    assert!(again.report.editable);
    assert!(again.document.processes[0].nodes[0].id.xml_id().is_none());
}

#[test]
fn anonymous_owner_keeps_empty_default_namespace_with_foreign_attributes() {
    let source = br#"<b:definitions xmlns:b="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:v="urn:vendor" targetNamespace="urn:anonymous"><b:process id="process"><b:task v:flag="v:opaque"><b:documentation>before<v:detail ref="v:item"/>after</b:documentation><b:extensionElements><v:payload/></b:extensionElements></b:task></b:process></b:definitions>"#;
    let parsed = prepare_import(source, "anonymous scoped owner").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert!(parsed.document.processes[0].nodes[0].id.xml_id().is_none());
    let output = export(&parsed).bytes;
    let xml = std::str::from_utf8(&output).unwrap();
    assert!(!xml.contains("data-processcraft-internal-context"));
    let again = prepare_import(&output, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[0].base_extras,
        parsed.document.processes[0].nodes[0].base_extras
    );
}

#[test]
fn anonymous_script_and_annotation_mixed_bodies_are_not_executed_or_given_ids() {
    let source = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:v="urn:vendor" targetNamespace="urn:anonymous"><process><scriptTask><script>before<v:code expr="v:value"/>after</script></scriptTask><textAnnotation><text>before<v:note/>after</text></textAnnotation></process></definitions>"#;
    let parsed = prepare_import(source, "anonymous bodies").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let output = export(&parsed).bytes;
    assert!(!std::str::from_utf8(&output).unwrap().contains(" id="));
    let again = prepare_import(&output, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes[0].activity,
        parsed.document.processes[0].nodes[0].activity
    );
    assert_eq!(
        again.document.processes[0].nodes[1].details,
        parsed.document.processes[0].nodes[1].details
    );
}

#[test]
fn anonymous_namespace_fixture_roundtrips() {
    let source = include_bytes!("../../../tests/fixtures/bpmn/anonymous-metadata-namespaces.bpmn");
    let parsed = prepare_import(source, "anonymous namespaces").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let output = export(&parsed).bytes;
    assert!(
        !std::str::from_utf8(&output)
            .unwrap()
            .contains("data-processcraft-internal-context")
    );
    assert!(prepare_import(&output, "again").unwrap().report.editable);
}

#[test]
fn internal_context_marker_collision_on_definitions_stays_source_protected() {
    let source = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" data-processcraft-internal-context="5b5d" targetNamespace="urn:anonymous"><process/></definitions>"#;
    let parsed = prepare_import(source, "marker collision").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, source);
}

#[test]
fn referenced_anonymous_data_port_requires_an_explicit_xml_id() {
    use processcraft_core::ids::ElementIdentity;
    let source = include_bytes!("../../../tests/fixtures/bpmn/anonymous-common-metadata.bpmn");
    let mut parsed = prepare_import(source, "anonymous referenced port").unwrap();
    let identity = ElementIdentity::anonymous();
    let process = &mut parsed.document.processes[0];
    let task = process
        .nodes
        .iter_mut()
        .find(|node| node.id.to_string() == "task")
        .unwrap();
    task.io_specification.as_mut().unwrap().inputs[1].id = Some(identity.clone());
    process.data_associations[0].target = identity.selector();
    let mut project = ProjectData::new("anonymous port".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources;
    let failure = export_bpmn(&project, parsed.document.id, false).unwrap_err();
    assert!(
        failure
            .report
            .blocked
            .iter()
            .any(|reason| reason.contains("Assign an XML ID")),
        "{:?}",
        failure.report.blocked
    );
}

#[test]
fn anonymous_event_property_output_and_set_keep_separate_metadata() {
    let source = include_str!("../../../tests/fixtures/bpmn/anonymous-common-metadata.bpmn").replace(
        "<intermediateCatchEvent id=\"event\"><timerEventDefinition>",
        "<intermediateCatchEvent id=\"event\"><property><documentation>property one</documentation></property><property><documentation>property two</documentation></property><dataOutput><documentation>event output</documentation></dataOutput><outputSet><documentation>event set</documentation></outputSet><timerEventDefinition>",
    );
    let parsed = prepare_import(source.as_bytes(), "anonymous event fields").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let event = parsed.document.processes[0]
        .nodes
        .iter()
        .find(|node| node.id.to_string() == "event")
        .unwrap();
    let Some(processcraft_core::bpmn_model::NodeDetails::Event { properties, io, .. }) =
        &event.details
    else {
        panic!("event expected")
    };
    assert_eq!(properties.len(), 2);
    assert_ne!(properties[0].base, properties[1].base);
    let Some(processcraft_core::bpmn_model::EventIo::Catch {
        outputs,
        output_set: Some(set),
    }) = io
    else {
        panic!("event output expected")
    };
    assert!(outputs[0].id.is_none() && outputs[0].base.is_some());
    assert!(set.id.is_none() && set.base.is_some());
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.processes[0]
            .nodes
            .iter()
            .find(|node| node.id.to_string() == "event")
            .unwrap()
            .details,
        event.details
    );
}

#[test]
fn activity_correlation_and_callable_fixtures_roundtrip_with_values() {
    use processcraft_core::bpmn_activity::{GlobalTaskKind, MultiInstanceFlowCondition};
    use processcraft_core::bpmn_model::LoopCharacteristics;
    let source = include_bytes!("../../../tests/fixtures/bpmn/activity-correlation.bpmn");
    let imported = prepare_import(source, "correlation").unwrap();
    assert!(imported.report.editable);
    assert!(!imported.document.correlation_properties.is_empty());
    let again = prepare_import(&export(&imported).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        imported.document.correlation_properties,
        again.document.correlation_properties
    );

    let source = include_bytes!("../../../tests/fixtures/bpmn/activity-callable-roles.bpmn");
    let imported = prepare_import(source, "callable").unwrap();
    assert!(imported.report.editable);
    assert!(
        imported
            .document
            .global_tasks
            .iter()
            .any(|task| matches!(task.kind, GlobalTaskKind::CallableElement))
    );
    let loop_value = imported
        .document
        .processes
        .iter()
        .flat_map(|p| &p.nodes)
        .find_map(|node| match &node.loop_characteristics {
            Some(LoopCharacteristics::MultiInstance {
                behavior: Some(MultiInstanceFlowCondition::Complex),
                complex_behavior_definitions,
                is_sequential: Some(false),
                ..
            }) => Some(complex_behavior_definitions),
            _ => None,
        })
        .unwrap();
    assert_eq!(loop_value.len(), 1);
    assert!(
        loop_value[0]
            .event
            .as_ref()
            .is_some_and(|event| event.input_associations.len() == 1)
    );
    let again = prepare_import(&export(&imported).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(imported.document.global_tasks, again.document.global_tasks);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&imported.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap(),
    );
}

#[test]
fn multi_instance_behavior_qnames_preserve_empty_namespace_and_shadowed_prefixes() {
    let imported = prepare_import(
        include_bytes!("../../../tests/fixtures/bpmn/activity-loop-qnames.bpmn"),
        "loops",
    )
    .unwrap();
    assert!(imported.report.editable);
    let output = export(&imported);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&imported.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(!xml.contains("xmlns:pc_loop_0=\"\""));
}

#[test]
fn choreography_di_representation_preserves_order_bands_and_metadata() {
    use processcraft_core::model::{DiStyleRef, MessageVisibleKind, ParticipantBandKind};
    let source = include_bytes!("../../../tests/fixtures/bpmn/di-choreography-representation.bpmn");
    let parsed = prepare_import(source, "choreography").unwrap();
    assert!(parsed.report.editable);
    let choreography = &parsed.document.choreographies[0];
    let multiplicity = choreography.participants[0].multiplicity.as_ref().unwrap();
    assert_eq!(
        (multiplicity.minimum, multiplicity.maximum),
        (Some(0), Some(3))
    );
    assert!(choreography.message_flows[0].message_ref.is_some());
    let diagram = &parsed.document.diagrams[0];
    assert_eq!(diagram.plane_order.as_ref().unwrap().len(), 4);
    assert!(diagram.plane_metadata.as_ref().unwrap().extension.is_some());
    assert_eq!(
        diagram.edges[0].message_visible_kind,
        Some(MessageVisibleKind::NonInitiating)
    );
    assert_eq!(
        diagram.shapes[1].participant_band_kind,
        Some(ParticipantBandKind::TopInitiating)
    );
    assert!(matches!(
        diagram.shapes[1].choreography_activity_shape,
        Some(DiStyleRef::Local(_))
    ));
    assert!(
        diagram.shapes[1]
            .label
            .as_ref()
            .unwrap()
            .di_metadata
            .is_some()
    );
    let output = export(&parsed);
    assert!(
        output
            .report
            .retained
            .iter()
            .any(|v| v.contains("foreign-schema validation"))
    );
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn invalid_di_extension_namespace_stays_preserved() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap();
    let bad = source.replace(
        "<dc:Bounds x=\"1\" y=\"2\" width=\"3\" height=\"4\"/>",
        "<di:waypoint x=\"1\" y=\"2\"/>",
    );
    let parsed = prepare_import(bad.as_bytes(), "invalid di extension").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, bad.as_bytes());
}

#[test]
fn diagram_metadata_keeps_noncanonical_namespace_context() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap()
    .replace("bpmndi:", "graph:")
    .replace("xmlns:bpmndi=", "xmlns:graph=")
    .replace("dc:", "geom:")
    .replace("xmlns:dc=", "xmlns:geom=")
    .replace("di:", "drawing:")
    .replace("xmlns:di=", "xmlns:drawing=");
    let parsed = prepare_import(source.as_bytes(), "custom prefixes").unwrap();
    assert!(parsed.report.editable);
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn diagram_owner_prefix_shadowing_is_preserved() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap();
    let start = source.find("<bpmndi:BPMNShape id=\"BuyerBand\"").unwrap();
    let end =
        source[start..].find("</bpmndi:BPMNShape>").unwrap() + start + "</bpmndi:BPMNShape>".len();
    let modified = source[start..end].replace("bpmndi:", "alt:").replacen("<alt:BPMNShape", "<alt:BPMNShape xmlns:alt=\"http://www.omg.org/spec/BPMN/20100524/DI\" xmlns:bpmndi=\"urn:shadow\"", 1);
    let source = format!("{}{}{}", &source[..start], modified, &source[end..]);
    let parsed = prepare_import(source.as_bytes(), "prefix shadow").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn choreography_band_foreign_same_local_reference_is_not_retargeted() {
    use processcraft_core::model::DiStyleRef;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap()
    .replacen(
        "choreographyActivityShape=\"tns:TaskShape\"",
        "xmlns:foreign=\"urn:foreign\" choreographyActivityShape=\"foreign:TaskShape\"",
        1,
    );
    let parsed = prepare_import(source.as_bytes(), "foreign band").unwrap();
    assert!(parsed.report.editable);
    assert!(
        matches!(&parsed.document.diagrams[0].shapes[1].choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace == "urn:foreign")
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert!(
        matches!(&again.document.diagrams[0].shapes[1].choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace == "urn:foreign")
    );
}

#[test]
fn choreography_band_empty_namespace_reference_stays_empty() {
    use processcraft_core::model::DiStyleRef;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap()
    .replacen(
        "<bpmndi:BPMNShape id=\"BuyerBand\"",
        "<bpmndi:BPMNShape xmlns=\"\" id=\"BuyerBand\"",
        1,
    )
    .replacen(
        "choreographyActivityShape=\"tns:TaskShape\"",
        "choreographyActivityShape=\"TaskShape\"",
        1,
    )
    .replacen(" v:band=\"B\"", "", 1);
    let parsed = prepare_import(source.as_bytes(), "empty band").unwrap();
    assert!(parsed.report.editable);
    assert!(
        matches!(&parsed.document.diagrams[0].shapes[1].choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace.is_empty())
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert!(
        matches!(&again.document.diagrams[0].shapes[1].choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace.is_empty())
    );
}

#[test]
fn choreography_message_reference_preserves_external_and_empty_namespaces() {
    use processcraft_core::bpmn_activity::ModelReference;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap();
    for (attribute, namespace) in [
        (
            "xmlns:external=\"urn:external\" messageRef=\"external:Message\"",
            "urn:external",
        ),
        ("xmlns=\"\" messageRef=\"Message\"", ""),
    ] {
        let source = source.replacen("messageRef=\"tns:Message\"", attribute, 1);
        let source = if namespace.is_empty() {
            source.replacen(
                "<messageFlow id=",
                "<b:messageFlow xmlns:b=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" id=",
                1,
            )
        } else {
            source
        };
        let parsed = prepare_import(source.as_bytes(), "foreign message").unwrap();
        assert!(parsed.report.editable);
        let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
        assert!(again.report.editable);
        assert!(
            matches!(&again.document.choreographies[0].message_flows[0].message_ref, Some(ModelReference::Qualified(q)) if q.namespace == namespace)
        );
    }
}

#[test]
fn all_participant_band_and_edge_message_visibility_values_roundtrip() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/di-choreography-representation.bpmn"
    ))
    .unwrap();
    for band in [
        "top_initiating",
        "middle_initiating",
        "bottom_initiating",
        "top_non_initiating",
        "middle_non_initiating",
        "bottom_non_initiating",
    ] {
        for visible in ["initiating", "non_initiating"] {
            let input = source
                .replacen(
                    "participantBandKind=\"top_initiating\"",
                    &format!("participantBandKind=\"{band}\""),
                    1,
                )
                .replacen(
                    "messageVisibleKind=\"non_initiating\"",
                    &format!("messageVisibleKind=\"{visible}\""),
                    1,
                );
            let parsed = prepare_import(input.as_bytes(), "band variants").unwrap();
            assert!(parsed.report.editable, "{band} {visible}");
            let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
            assert!(again.report.editable, "{band} {visible}");
            assert_eq!(
                parsed.document.diagrams[0].shapes[1].participant_band_kind,
                again.document.diagrams[0].shapes[1].participant_band_kind
            );
            assert_eq!(
                parsed.document.diagrams[0].edges[0].message_visible_kind,
                again.document.diagrams[0].edges[0].message_visible_kind
            );
        }
    }
}

#[test]
fn multiple_collaborations_and_anonymous_root_order_roundtrip() {
    use processcraft_core::bpmn_collaboration::{CollaborationRootKind, ConversationKind};
    let source = include_bytes!("../../../tests/fixtures/bpmn/collaboration-multiple.bpmn");
    let parsed = prepare_import(source, "collaborations").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert_eq!(
        parsed.document.additional_collaborations[0].kind,
        Some(CollaborationRootKind::GlobalConversation)
    );
    assert!(
        parsed.document.partner_entities[0]
            .id
            .as_ref()
            .is_some_and(|id| id.xml_id().is_none())
    );
    let collab = parsed.document.collaboration.as_ref().unwrap();
    assert_eq!(collab.extras.as_ref().unwrap().is_closed, Some(false));
    assert_eq!(collab.extras.as_ref().unwrap().artifacts.len(), 3);
    assert!(collab.conversation_nodes.iter().any(|node| {
        node.kind == ConversationKind::CallConversation
            && node
                .call
                .as_ref()
                .is_some_and(|call| call.participant_associations.len() == 1)
    }));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.find("<partnerEntity").unwrap() < xml.find("<collaboration").unwrap());
    assert!(xml.find("<process").unwrap() < xml.find("<endPoint").unwrap());
}

#[test]
fn invalid_collaboration_child_order_and_unsupported_children_are_protected() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/collaboration-multiple.bpmn"
    ))
    .unwrap();
    let order = source.replace(
        "<interfaceRef>ext:Interface</interfaceRef><endPointRef>t:Endpoint</endPointRef>",
        "<endPointRef>t:Endpoint</endPointRef><interfaceRef>ext:Interface</interfaceRef>",
    );
    let parsed = prepare_import(order.as_bytes(), "bad order").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, order.as_bytes());
    let unknown = source.replacen(
        "<globalConversation id=\"Global\" name=\"Shared\"/>",
        "<globalConversation id=\"Global\" name=\"Shared\"><unexpected/></globalConversation>",
        1,
    );
    let parsed = prepare_import(unknown.as_bytes(), "unknown child").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, unknown.as_bytes());
}

#[test]
fn collaboration_external_qnames_do_not_alias_local_names() {
    use processcraft_core::bpmn_model::EventReference;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/collaboration-multiple.bpmn"
    ))
    .unwrap()
    .replacen("processRef=\"t:Process\"", "processRef=\"ext:Process\"", 1)
    .replacen(
        "sourceRef=\"t:Buyer\" targetRef=\"t:Seller\"",
        "sourceRef=\"ext:Buyer\" targetRef=\"t:Seller\"",
        1,
    )
    .replacen(
        "sourceRef=\"t:Sub\" targetRef=\"t:Buyer\"",
        "sourceRef=\"ext:Sub\" targetRef=\"t:Buyer\"",
        1,
    );
    let parsed = prepare_import(source.as_bytes(), "external names").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    let collab = again.document.collaboration.as_ref().unwrap();
    assert!(
        matches!(&collab.participants[0].process, Some(EventReference::Qualified(q)) if q.namespace == "urn:external")
    );
    assert!(
        matches!(&collab.flows[0].source, Some(EventReference::Qualified(q)) if q.namespace == "urn:external")
    );
    assert!(
        matches!(&collab.conversation_links[0].source, Some(EventReference::Qualified(q)) if q.namespace == "urn:external")
    );
}

#[test]
fn collaboration_unqualified_qnames_keep_the_empty_namespace() {
    use processcraft_core::bpmn_model::EventReference;
    let input = br#"<b:definitions xmlns:b="http://www.omg.org/spec/BPMN/20100524/MODEL" targetNamespace="urn:target"><b:collaboration id="C"><b:participant id="P" processRef="Process"><b:interfaceRef>Interface</b:interfaceRef><b:endPointRef>Endpoint</b:endPointRef></b:participant><b:messageFlow id="M" sourceRef="P" targetRef="Remote"/><b:conversation id="N"><b:participantRef>P</b:participantRef><b:messageFlowRef>M</b:messageFlowRef></b:conversation></b:collaboration></b:definitions>"#;
    let parsed = prepare_import(input, "empty namespace").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let out = export(&parsed);
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    let collaboration = again.document.collaboration.as_ref().unwrap();
    assert!(
        matches!(&collaboration.participants[0].process, Some(EventReference::Qualified(q)) if q.namespace.is_empty())
    );
    assert!(
        matches!(&collaboration.flows[0].source, Some(EventReference::Qualified(q)) if q.namespace.is_empty())
    );
    assert!(
        matches!(&collaboration.conversation_nodes[0].participant_refs[0], EventReference::Qualified(q) if q.namespace.is_empty())
    );
}

#[test]
fn collaboration_qname_owners_keep_documentation_namespace_scope() {
    let source = std::str::from_utf8(include_bytes!("../../../tests/fixtures/bpmn/collaboration-multiple.bpmn")).unwrap()
        .replace("messageRef=\"ext:Message\"/>", "messageRef=\"ext:Message\"><documentation>Scoped message</documentation></messageFlow>")
        .replace("<participantMultiplicity minimum=\"0\" maximum=\"2\"/>", "<participantMultiplicity minimum=\"0\" maximum=\"2\"><documentation>Scoped multiplicity</documentation></participantMultiplicity>")
        .replace("<conversationAssociation innerConversationNodeRef=\"ext:Inner\" outerConversationNodeRef=\"t:Sub\"/>", "<conversationAssociation innerConversationNodeRef=\"ext:Inner\" outerConversationNodeRef=\"t:Sub\"><documentation>Scoped association</documentation></conversationAssociation>");
    let parsed = prepare_import(source.as_bytes(), "scoped docs").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn collaboration_group_external_category_qname_remains_external() {
    use processcraft_core::{
        bpmn_collaboration::CollaborationArtifact, bpmn_model::EventReference,
    };
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/collaboration-multiple.bpmn"
    ))
    .unwrap()
    .replacen(
        "categoryValueRef=\"ext:CategoryValue\"",
        "categoryValueRef=\"ext:Value\"",
        1,
    );
    let parsed = prepare_import(source.as_bytes(), "external group category").unwrap();
    assert!(parsed.report.editable);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    let extras = again
        .document
        .collaboration
        .as_ref()
        .unwrap()
        .extras
        .as_ref()
        .unwrap();
    assert!(
        matches!(&extras.artifacts[0], CollaborationArtifact::Group(group) if matches!(&group.category_value_ref, Some(EventReference::Qualified(q)) if q.namespace == "urn:external"))
    );
}

#[test]
fn choreography_inherited_collaboration_fields_roundtrip() {
    let source = include_bytes!("../../../tests/fixtures/bpmn/choreography-inherited.bpmn");
    let parsed = prepare_import(source, "inherited choreography").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let choreography = &parsed.document.choreographies[0];
    assert_eq!(choreography.conversation_nodes.len(), 1);
    assert_eq!(choreography.conversation_links.len(), 1);
    assert_eq!(choreography.correlation_keys.len(), 1);
    let extras = choreography.extras.as_ref().unwrap();
    assert_eq!(
        (
            extras.conversation_associations.len(),
            extras.participant_associations.len(),
            extras.message_flow_associations.len(),
            extras.choreography_refs.len()
        ),
        (1, 1, 1, 1)
    );
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&parsed.document).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
}

#[test]
fn collaboration_artifact_native_fields_require_acknowledgement() {
    use processcraft_core::bpmn_collaboration::CollaborationArtifact;
    for location in 0..3 {
        let source = if location == 2 {
            include_str!("../../../tests/fixtures/bpmn/choreography-inherited.bpmn").replace(
                "<conversation id=\"Conversation\">",
                "<textAnnotation id=\"NativeNote\"><text>Note</text></textAnnotation><conversation id=\"Conversation\">",
            )
        } else {
            include_str!("../../../tests/fixtures/bpmn/collaboration-multiple.bpmn").to_owned()
        };
        let mut imported = prepare_import(source.as_bytes(), "artifact").unwrap();
        assert!(imported.report.editable);
        if location == 1 {
            let mut node = imported
                .document
                .collaboration
                .as_ref()
                .unwrap()
                .extras
                .as_ref()
                .unwrap()
                .artifacts
                .iter()
                .find_map(|artifact| match artifact {
                    CollaborationArtifact::TextAnnotation(node) => Some(node.clone()),
                    _ => None,
                })
                .unwrap();
            node.id = "AdditionalNote".parse().unwrap();
            imported.document.additional_collaborations[0]
                .extras
                .get_or_insert_with(Default::default)
                .artifacts
                .push(CollaborationArtifact::TextAnnotation(node));
        }
        let extras = match location {
            0 => imported
                .document
                .collaboration
                .as_mut()
                .unwrap()
                .extras
                .as_mut()
                .unwrap(),
            1 => imported.document.additional_collaborations[0]
                .extras
                .as_mut()
                .unwrap(),
            _ => imported.document.choreographies[0].extras.as_mut().unwrap(),
        };
        let node = extras
            .artifacts
            .iter_mut()
            .find_map(|artifact| match artifact {
                CollaborationArtifact::TextAnnotation(node) => Some(node),
                _ => None,
            })
            .unwrap();
        node.owner = "Artifact owner".into();
        node.contact = "Artifact contact".into();
        let mut project = ProjectData::new("artifact fields".into());
        project.documents.push(Arc::new(imported.document.clone()));
        project.resources = imported.resources;
        let before = project.content_identity().unwrap();
        let failure = export_bpmn(&project, imported.document.id, false).unwrap_err();
        assert!(
            failure
                .report
                .blocked
                .iter()
                .any(|v| v.contains("acknowledgement"))
        );
        assert!(
            failure
                .report
                .omitted
                .iter()
                .any(|v| v.contains("owner and contact"))
        );
        let result = export_bpmn(&project, imported.document.id, true)
            .unwrap_or_else(|error| panic!("location {location}: {error:?}"));
        assert!(
            result
                .report
                .omitted
                .iter()
                .any(|v| v.contains("owner and contact"))
        );
        assert!(
            prepare_import(&result.bytes, "again")
                .unwrap()
                .report
                .editable
        );
        assert_eq!(project.content_identity().unwrap(), before);
        let actual = match location {
            0 => project.documents[0]
                .collaboration
                .as_ref()
                .unwrap()
                .extras
                .as_ref()
                .unwrap(),
            1 => project.documents[0].additional_collaborations[0]
                .extras
                .as_ref()
                .unwrap(),
            _ => project.documents[0].choreographies[0]
                .extras
                .as_ref()
                .unwrap(),
        }
        .artifacts
        .iter()
        .find_map(|artifact| match artifact {
            CollaborationArtifact::TextAnnotation(node) => Some(node),
            _ => None,
        })
        .unwrap();
        assert_eq!(actual.owner, "Artifact owner");
        assert_eq!(actual.contact, "Artifact contact");
    }
}

#[test]
fn reordered_collaboration_and_anonymous_same_family_roots_roundtrip() {
    let source = include_str!("../../../tests/fixtures/bpmn/collaboration-multiple.bpmn").replace(
        "<collaboration id=\"Collab\"",
        "<partnerEntity name=\"Second entity\"/><collaboration id=\"Collab\"",
    );
    let mut imported = prepare_import(source.as_bytes(), "reordered roots").unwrap();
    assert!(imported.report.editable);
    let first = imported
        .document
        .collaboration
        .as_ref()
        .unwrap()
        .id
        .selector();
    let second = imported.document.additional_collaborations[0].id.selector();
    let first_partner = imported.document.partner_entities[0]
        .id
        .as_ref()
        .unwrap()
        .selector();
    let second_partner = imported.document.partner_entities[1]
        .id
        .as_ref()
        .unwrap()
        .selector();
    assert!(
        imported.document.partner_entities.iter().all(|p| p
            .id
            .as_ref()
            .unwrap()
            .xml_id()
            .is_none())
    );
    let order = imported.document.root_order.as_mut().unwrap();
    let swap_ids = |order: &mut Vec<_>, a, b| {
        let i = order.iter().position(|s| *s == a).unwrap();
        let j = order.iter().position(|s| *s == b).unwrap();
        order.swap(i, j);
    };
    swap_ids(order, first, second);
    swap_ids(order, first_partner, second_partner);
    let mut project = ProjectData::new("reordered roots".into());
    project.documents.push(Arc::new(imported.document.clone()));
    project.resources = imported.resources.clone();
    let before = project.content_identity().unwrap();
    let output = export_bpmn(&project, imported.document.id, false).unwrap();
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    assert!(xml.find("<globalConversation").unwrap() < xml.find("<collaboration").unwrap());
    assert!(xml.find("name=\"Second entity\"").unwrap() < xml.find("name=\"Customers\"").unwrap());
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&project.documents[0]).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    assert_eq!(project.content_identity().unwrap(), before);
}

#[test]
fn reordered_mixed_di_elements_preserve_anonymous_representations() {
    let source = FLAT.replace("</bpmndi:BPMNPlane>", "<bpmndi:BPMNShape bpmnElement=\"tns:start\"><dc:Bounds x=\"110\" y=\"20\" width=\"40\" height=\"40\"/></bpmndi:BPMNShape><bpmndi:BPMNEdge bpmnElement=\"tns:f2\"><di:waypoint x=\"10\" y=\"20\"/><di:waypoint x=\"90\" y=\"20\"/></bpmndi:BPMNEdge></bpmndi:BPMNPlane>");
    let mut imported = prepare_import(source.as_bytes(), "reordered diagram").unwrap();
    assert!(imported.report.editable);
    let diagram = &mut imported.document.diagrams[0];
    assert!(diagram.shapes[1].id.xml_id().is_none());
    assert!(diagram.edges[1].id.xml_id().is_none());
    diagram.plane_order = Some(vec![
        diagram.edges[1].id.selector(),
        diagram.shapes[1].id.selector(),
        diagram.edges[0].id.selector(),
        diagram.shapes[0].id.selector(),
    ]);
    let mut project = ProjectData::new("reordered diagram".into());
    project.documents.push(Arc::new(imported.document.clone()));
    project.resources = imported.resources.clone();
    let before = project.content_identity().unwrap();
    let output = export_bpmn(&project, imported.document.id, false).unwrap();
    let xml = std::str::from_utf8(&output.bytes).unwrap();
    let ordered: Vec<_> = [
        "bpmnElement=\"f2\"",
        "bpmnElement=\"start\"",
        "bpmnElement=\"f1\"",
        "bpmnElement=\"task\"",
    ]
    .iter()
    .map(|v| xml.find(v).unwrap())
    .collect();
    assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    let again = prepare_import(&output.bytes, "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        processcraft_io::interchange_compare::xml_model_value(&project.documents[0]).unwrap(),
        processcraft_io::interchange_compare::xml_model_value(&again.document).unwrap()
    );
    assert_eq!(project.content_identity().unwrap(), before);
}

#[test]
fn choreography_task_loop_type_preserves_explicit_presence() {
    use processcraft_core::bpmn_collaboration::ChoreographyLoopType;
    let input = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn20-schema/choreography.bpmn"
    ))
    .unwrap();
    for (attribute, expected) in [
        ("", None),
        (" loopType=\"None\"", Some(ChoreographyLoopType::None)),
        (
            " loopType=\"Standard\"",
            Some(ChoreographyLoopType::Standard),
        ),
        (
            " loopType=\"MultiInstanceSequential\"",
            Some(ChoreographyLoopType::MultiInstanceSequential),
        ),
        (
            " loopType=\"MultiInstanceParallel\"",
            Some(ChoreographyLoopType::MultiInstanceParallel),
        ),
    ] {
        let xml = input.replace(
            "<choreographyTask id=",
            &format!("<choreographyTask{attribute} id="),
        );
        let parsed = prepare_import(xml.as_bytes(), "loop presence").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        assert_eq!(
            parsed.document.choreographies[0].activities[0].loop_type,
            expected
        );
        let again = prepare_import(&export(&parsed).bytes, "loop round trip").unwrap();
        assert!(again.report.editable, "{:?}", again.report.limitations);
        assert_eq!(
            again.document.choreographies[0].activities[0].loop_type,
            expected
        );
    }
}

#[test]
fn unimplemented_choreography_scopes_keep_original_source() {
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn20-schema/choreography.bpmn"
    ))
    .unwrap();
    for input in [
        source
            .replace("<choreography id=", "<globalChoreographyTask id=")
            .replace("</choreography>", "</globalChoreographyTask>"),
        source
            .replace("<choreographyTask id=", "<subChoreography id=")
            .replace("</choreographyTask>", "</subChoreography>"),
        source
            .replace("<choreographyTask id=", "<callChoreography id=")
            .replace("</choreographyTask>", "</callChoreography>"),
    ] {
        let parsed = prepare_import(input.as_bytes(), "unimplemented choreography scope").unwrap();
        assert!(!parsed.report.editable);
        assert_eq!(export(&parsed).bytes, input.as_bytes());
    }
}

#[test]
fn choreography_external_qname_does_not_bind_to_same_local_id() {
    use processcraft_core::bpmn_activity::ModelReference;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn20-schema/choreography.bpmn"
    ))
    .unwrap();
    let source = source
        .replace(
            "xmlns:tns=\"urn:choreo\"",
            "xmlns:tns=\"urn:choreo\" xmlns:external=\"urn:external\"",
        )
        .replace(
            "<messageFlowRef>tns:order</messageFlowRef>",
            "<messageFlowRef>external:order</messageFlowRef>",
        );
    let parsed = prepare_import(source.as_bytes(), "external choreography reference").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    assert!(
        matches!(&parsed.document.choreographies[0].activities[0].message_flow_refs[0], ModelReference::Qualified(q) if q.namespace == "urn:external" && q.local.to_string() == "order")
    );
    let again = prepare_import(&export(&parsed).bytes, "external round trip").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    assert!(
        matches!(&again.document.choreographies[0].activities[0].message_flow_refs[0], ModelReference::Qualified(q) if q.namespace == "urn:external" && q.local.to_string() == "order")
    );
}

#[test]
fn nested_choreography_graph_round_trips() {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, ChoreographyRootKind,
    };
    let source = include_bytes!("../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn");
    let parsed = prepare_import(source, "nested choreography").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let main = &parsed.document.choreographies[0];
    let graph = main.graph.as_ref().unwrap();
    assert_eq!(graph.flow_elements.len(), 6);
    let ChoreographyFlowElement::Activity(sub) = &graph.flow_elements[1] else {
        panic!("sub choreography expected")
    };
    let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) = &sub.details else {
        panic!("sub content expected")
    };
    assert_eq!(graph.flow_elements.len(), 3);
    let ChoreographyFlowElement::Event(event) = &graph.flow_elements[0] else {
        panic!("nested event expected")
    };
    assert_eq!(event.flow_metadata.as_ref().unwrap().outgoing.len(), 1);
    assert!(
        matches!(&event.details, Some(processcraft_core::bpmn_model::NodeDetails::Event { definitions, .. }) if definitions.len() == 1)
    );
    let ChoreographyFlowElement::Gateway(gateway) = &graph.flow_elements[1] else {
        panic!("nested gateway expected")
    };
    assert_eq!(gateway.flow_metadata.as_ref().unwrap().incoming.len(), 1);
    assert_eq!(artifacts.len(), 1);
    assert!(matches!(
        &parsed.document.choreographies[1].kind,
        Some(ChoreographyRootKind::Global { .. })
    ));
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "nested round trip").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    assert_eq!(
        again.document.choreographies[0]
            .graph
            .as_ref()
            .unwrap()
            .flow_elements
            .len(),
        6
    );
}

#[test]
fn empty_choreography_graph_exports_as_empty_scope() {
    use processcraft_core::bpmn_collaboration::ChoreographyGraph;
    let source = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" id="defs" targetNamespace="urn:empty"><choreography id="choreo"/></definitions>"#;
    let mut parsed = prepare_import(source, "empty graph").unwrap();
    assert!(parsed.report.editable);
    parsed.document.choreographies[0].graph = Some(ChoreographyGraph::default());
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "empty round trip").unwrap();
    assert!(again.report.editable);
    assert!(again.document.choreographies[0].activities.is_empty());
    assert!(again.document.choreographies[0].sequence_flows.is_empty());
}

#[test]
fn empty_sub_choreography_scope_round_trips() {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement,
    };
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn"
    ))
    .unwrap();
    let from = source.find("      <startEvent id=\"subStart\"").unwrap();
    let to = source.find("    </subChoreography>").unwrap();
    let input = format!("{}{}", &source[..from], &source[to..]);
    let parsed = prepare_import(input.as_bytes(), "empty sub choreography").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "empty sub round trip").unwrap();
    let graph = again.document.choreographies[0].graph.as_ref().unwrap();
    let ChoreographyFlowElement::Activity(sub) = &graph.flow_elements[1] else {
        panic!("sub choreography expected")
    };
    assert!(
        matches!(&sub.details, Some(ChoreographyActivityDetails::Sub { graph, artifacts }) if graph.flow_elements.is_empty() && artifacts.is_empty())
    );
}

#[test]
fn global_choreography_initiator_presence_round_trips() {
    use processcraft_core::bpmn_collaboration::ChoreographyRootKind;
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn"
    ))
    .unwrap();
    for present in [false, true] {
        let input = if present {
            source.to_owned()
        } else {
            source.replace(
                "id=\"global\" initiatingParticipantRef=\"t:globalBuyer\"",
                "id=\"global\"",
            )
        };
        let parsed = prepare_import(input.as_bytes(), "global initiator").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let again = prepare_import(&export(&parsed).bytes, "global round trip").unwrap();
        assert!(
            matches!(&again.document.choreographies[1].kind, Some(ChoreographyRootKind::Global { initiating_participant_ref }) if initiating_participant_ref.is_some() == present)
        );
    }
}

#[test]
fn call_choreography_external_target_is_not_rebound() {
    use processcraft_core::{
        bpmn_activity::ModelReference,
        bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyFlowElement},
    };
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn"
    ))
    .unwrap();
    let from = source
        .find("      <participantAssociation id=\"mapping\"")
        .unwrap();
    let to = source[from..].find("</participantAssociation>").unwrap()
        + from
        + "</participantAssociation>".len();
    let input = format!("{}{}", &source[..from], &source[to..]).replace(
        "calledChoreographyRef=\"t:global\"",
        "calledChoreographyRef=\"ext:global\"",
    );
    let parsed = prepare_import(input.as_bytes(), "external called choreography").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "external call round trip").unwrap();
    let graph = again.document.choreographies[0].graph.as_ref().unwrap();
    let ChoreographyFlowElement::Activity(call) = &graph.flow_elements[2] else {
        panic!("call expected")
    };
    assert!(
        matches!(&call.details, Some(ChoreographyActivityDetails::Call { called_choreography_ref: Some(ModelReference::Qualified(q)), .. }) if q.namespace == "urn:external" && q.local.to_string() == "global")
    );
}

#[test]
fn choreography_boundary_event_io_round_trips_and_owner_order_is_stable() {
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_collaboration::ChoreographyFlowElement;
    let source =
        include_bytes!("../../../tests/fixtures/bpmn/choreography-graph/boundary-event-io.bpmn");
    let mut parsed = prepare_import(source, "boundary event io").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let graph = parsed.document.choreographies[0].graph.as_mut().unwrap();
    assert_eq!(graph.data_associations.len(), 2);
    let ChoreographyFlowElement::Event(event) = &graph.flow_elements[1] else {
        panic!("boundary event expected")
    };
    assert!(matches!(
        &event.details,
        Some(processcraft_core::bpmn_model::NodeDetails::Event {
            attached_to: Some(ModelReference::Local(_)),
            ..
        })
    ));
    graph.data_associations.reverse();
    let again = prepare_import(&export(&parsed).bytes, "boundary event io round trip").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    assert_eq!(
        again.document.choreographies[0]
            .graph
            .as_ref()
            .unwrap()
            .data_associations
            .len(),
        2
    );
}

#[test]
fn choreography_graph_node_native_fields_require_omission_acknowledgement() {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, CollaborationArtifact,
    };
    let source = include_bytes!("../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn");
    for annotation in [false, true] {
        let mut parsed = prepare_import(source, "native fields").unwrap();
        let graph = parsed.document.choreographies[0].graph.as_mut().unwrap();
        if annotation {
            let ChoreographyFlowElement::Activity(sub) = &mut graph.flow_elements[1] else {
                panic!("sub expected")
            };
            let Some(ChoreographyActivityDetails::Sub { artifacts, .. }) = &mut sub.details else {
                panic!("sub content expected")
            };
            let CollaborationArtifact::TextAnnotation(node) = &mut artifacts[0] else {
                panic!("annotation expected")
            };
            node.contact = "analyst".into();
        } else {
            let ChoreographyFlowElement::Event(node) = &mut graph.flow_elements[0] else {
                panic!("event expected")
            };
            node.owner = "analyst".into();
        }
        let mut project = ProjectData::new("p".into());
        project.documents.push(Arc::new(parsed.document.clone()));
        project.resources = parsed.resources.clone();
        assert!(export_bpmn(&project, parsed.document.id, false).is_err());
        assert!(export_bpmn(&project, parsed.document.id, true).is_ok());
    }
}

#[test]
fn choreography_boundary_external_attachment_keeps_namespace() {
    use processcraft_core::{
        bpmn_activity::ModelReference, bpmn_collaboration::ChoreographyFlowElement,
        bpmn_model::NodeDetails,
    };
    let source = std::str::from_utf8(include_bytes!(
        "../../../tests/fixtures/bpmn/choreography-graph/boundary-event-io.bpmn"
    ))
    .unwrap();
    let source = source
        .replace(
            "xmlns:t=\"urn:boundary-graph\"",
            "xmlns:t=\"urn:boundary-graph\" xmlns:ext=\"urn:external\"",
        )
        .replacen("attachedToRef=\"t:sub\"", "attachedToRef=\"ext:sub\"", 1);
    let parsed = prepare_import(source.as_bytes(), "external attachment").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "external attachment round trip").unwrap();
    let graph = again.document.choreographies[0].graph.as_ref().unwrap();
    let ChoreographyFlowElement::Event(event) = &graph.flow_elements[1] else {
        panic!("boundary expected")
    };
    assert!(
        matches!(&event.details, Some(NodeDetails::Event { attached_to: Some(ModelReference::Qualified(q)), .. }) if q.namespace == "urn:external" && q.local.to_string() == "sub")
    );
}

#[test]
fn di_extension_wrapper_whitespace_is_preserved_and_significant() {
    use processcraft_core::bpmn_xml::XmlNode;
    let source = include_str!("../../../tests/fixtures/bpmn20-schema/di-choreography-bands.bpmn");
    let parsed = prepare_import(source.as_bytes(), "DI extension whitespace").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    for diagram in &parsed.document.diagrams {
        let content = diagram
            .plane_metadata
            .as_ref()
            .unwrap()
            .extension
            .as_ref()
            .unwrap();
        assert!(matches!(&content.nodes[0], XmlNode::Text { text } if text == "\n        "));
        assert!(matches!(&content.nodes[2], XmlNode::Text { text } if text == "\n      "));
    }
    let output = export(&parsed);
    let again = prepare_import(&output.bytes, "DI extension round trip").unwrap();
    for (before, after) in parsed
        .document
        .diagrams
        .iter()
        .zip(&again.document.diagrams)
    {
        assert_eq!(
            before.plane_metadata.as_ref().unwrap().extension,
            after.plane_metadata.as_ref().unwrap().extension
        );
    }
    let changed = source.replacen(
        "<di:extension>\n        <bpmn:documentation>",
        "<di:extension>\n         <bpmn:documentation>",
        1,
    );
    let changed = prepare_import(changed.as_bytes(), "changed whitespace").unwrap();
    assert_ne!(
        parsed.document.diagrams[0]
            .plane_metadata
            .as_ref()
            .unwrap()
            .extension,
        changed.document.diagrams[0]
            .plane_metadata
            .as_ref()
            .unwrap()
            .extension
    );
}

#[test]
fn sub_choreography_plane_round_trips_and_excludes_sibling_scope() {
    use processcraft_io::native::{load_bytes, write_bytes};
    let source = include_str!("../../../tests/fixtures/bpmn/choreography-graph/subplane.bpmn");
    let parsed = prepare_import(source.as_bytes(), "subplane").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let mut project = ProjectData::new("subplane".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources.clone();
    let loaded = load_bytes(&write_bytes(&project).unwrap()).unwrap().project;
    assert_eq!(
        project.content_identity().unwrap(),
        loaded.content_identity().unwrap()
    );
    let output = export_bpmn(&loaded, parsed.document.id, false).unwrap();
    let again = prepare_import(&output.bytes, "subplane round trip").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    assert_eq!(again.document.diagrams[0].shapes.len(), 6);
    assert_eq!(again.document.diagrams[0].edges.len(), 2);
    let invalid = source.replace("bpmnElement=\"t:innerStart\"", "bpmnElement=\"t:sibling\"");
    let preserved = prepare_import(invalid.as_bytes(), "sibling outside subplane").unwrap();
    assert!(!preserved.report.editable);
    assert_eq!(export(&preserved).bytes, invalid.as_bytes());
    let invalid_band = source.replacen(
        "choreographyActivityShape=\"t:innerShape\"",
        "choreographyActivityShape=\"t:startShape\"",
        1,
    );
    let preserved_band =
        prepare_import(invalid_band.as_bytes(), "band outside activity owner").unwrap();
    assert!(!preserved_band.report.editable);
    assert_eq!(export(&preserved_band).bytes, invalid_band.as_bytes());
}

#[test]
fn sub_choreography_plane_admits_only_referenced_message_flows() {
    let source = include_str!("../../../tests/fixtures/bpmn/choreography-graph/subplane.bpmn")
        .replace("<participant id=\"buyer\"/><participant id=\"seller\"/>", "<participant id=\"buyer\"/><participant id=\"seller\"/><messageFlow id=\"message\" sourceRef=\"t:buyer\" targetRef=\"t:seller\"/>")
        .replace("<startEvent id=\"innerStart\"/>", "<choreographyTask id=\"task\" initiatingParticipantRef=\"t:buyer\"><participantRef>t:buyer</participantRef><participantRef>t:seller</participantRef><messageFlowRef>t:message</messageFlowRef></choreographyTask><startEvent id=\"innerStart\"/>")
        .replace("</bpmndi:BPMNPlane>", "<bpmndi:BPMNEdge id=\"messageEdge\" bpmnElement=\"t:message\"><di:waypoint x=\"120\" y=\"20\"/><di:waypoint x=\"400\" y=\"20\"/></bpmndi:BPMNEdge></bpmndi:BPMNPlane>");
    let parsed = prepare_import(source.as_bytes(), "subplane message flow").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let again = prepare_import(&export(&parsed).bytes, "subplane message flow round trip").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    let invalid = source.replace(
        "<messageFlowRef>t:message</messageFlowRef>",
        "<messageFlowRef xmlns:ext=\"urn:external\">ext:message</messageFlowRef>",
    );
    let parsed = prepare_import(invalid.as_bytes(), "unreferenced local message flow").unwrap();
    assert!(!parsed.report.editable);
    assert_eq!(export(&parsed).bytes, invalid.as_bytes());
}
