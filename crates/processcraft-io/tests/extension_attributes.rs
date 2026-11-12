//! Foreign (vendor) attributes, as Camunda Modeler writes them, are kept verbatim and no longer
//! block structural edits; an element such an attribute names by ID cannot be removed.
use processcraft_core::{
    commands::{CommandError, Envelope, Mutation, Request, Session},
    model::ProjectData,
};
use processcraft_io::interchange::{export_bpmn, prepare_import};
use serde_json::json;
use std::sync::Arc;

const CAMUNDA: &str = r#"<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" xmlns:camunda="http://camunda.org/schema/1.0/bpmn" xmlns:modeler="http://camunda.org/schema/modeler/1.0" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn" modeler:executionPlatform="Camunda Platform" modeler:executionPlatformVersion="7.20.0"><bpmn:process id="Process_1" isExecutable="true"><bpmn:startEvent id="Start_1"/><bpmn:userTask id="Task_a" name="Register" camunda:assignee="clerk" camunda:followUpTask="Task_b"/><bpmn:task id="Task_b" name="Assess"/><bpmn:endEvent id="End_1"/><bpmn:sequenceFlow id="F1" sourceRef="Start_1" targetRef="Task_a"/><bpmn:sequenceFlow id="F2" sourceRef="Task_a" targetRef="Task_b"/><bpmn:sequenceFlow id="F3" sourceRef="Task_b" targetRef="End_1"/></bpmn:process><bpmndi:BPMNDiagram id="D1"><bpmndi:BPMNPlane id="P1" bpmnElement="Process_1"><bpmndi:BPMNShape id="Start_1_di" bpmnElement="Start_1"><dc:Bounds x="100" y="100" width="36" height="36"/></bpmndi:BPMNShape><bpmndi:BPMNShape id="Task_a_di" bpmnElement="Task_a"><dc:Bounds x="190" y="78" width="100" height="80"/></bpmndi:BPMNShape><bpmndi:BPMNShape id="Task_b_di" bpmnElement="Task_b"><dc:Bounds x="350" y="78" width="100" height="80"/></bpmndi:BPMNShape><bpmndi:BPMNShape id="End_1_di" bpmnElement="End_1"><dc:Bounds x="510" y="100" width="36" height="36"/></bpmndi:BPMNShape><bpmndi:BPMNEdge id="F1_di" bpmnElement="F1"><di:waypoint x="136" y="118"/><di:waypoint x="190" y="118"/></bpmndi:BPMNEdge><bpmndi:BPMNEdge id="F2_di" bpmnElement="F2"><di:waypoint x="290" y="118"/><di:waypoint x="350" y="118"/></bpmndi:BPMNEdge><bpmndi:BPMNEdge id="F3_di" bpmnElement="F3"><di:waypoint x="450" y="118"/><di:waypoint x="510" y="118"/></bpmndi:BPMNEdge></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></bpmn:definitions>"#;

fn batch(s: &mut Session, mutations: serde_json::Value) -> Result<(), CommandError> {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    let mutations: Vec<Mutation> = serde_json::from_value(mutations).unwrap();
    s.apply(envelope, Request::Batch { mutations }).map(|_| ())
}

#[test]
fn camunda_attributes_allow_structural_edits_but_protect_named_elements() {
    let prepared = prepare_import(CAMUNDA.as_bytes(), "camunda").unwrap();
    assert!(
        prepared.report.editable,
        "{:?}",
        prepared.report.limitations
    );
    let doc = prepared.document.id;
    assert!(prepared.document.has_opaque_xml());
    assert!(!prepared.document.blocks_structural_edits());
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(prepared.document.clone()));
    project.resources = prepared.resources.clone();
    let mut s = Session::new(project).unwrap();

    // Structural edits: append after a task and connect back to the end event.
    batch(
        &mut s,
        json!([
            {"type": "append", "document_id": doc, "diagram_id": "D1", "source_id": "Task_a",
             "kind": "Task", "name": "Check", "node_id": "Task_c", "shape_id": "Task_c_di",
             "flow_id": "F4", "edge_id": "F4_di"},
            {"type": "connect", "document_id": doc, "diagram_id": "D1", "source_id": "Task_c",
             "target_id": "End_1", "flow_id": "F5", "edge_id": "F5_di"}
        ]),
    )
    .unwrap();

    // Task_b is named by camunda:followUpTask: removing it would leave that value dangling.
    let before = s.snapshot();
    let error = batch(
        &mut s,
        json!([{"type": "delete", "document_id": doc, "element_id": "Task_b"}]),
    )
    .unwrap_err();
    assert!(
        matches!(&error, CommandError::InvalidParams(m) if m.contains("Task_b is referenced by an extension attribute")),
        "{error:?}"
    );
    let after = s.snapshot();
    assert_eq!(
        (after.content_id, after.generation),
        (before.content_id, before.generation)
    );
    // An element no attribute names can be deleted.
    batch(
        &mut s,
        json!([{"type": "delete", "document_id": doc, "element_id": "Task_c"}]),
    )
    .unwrap();

    // The vendor attributes are exported verbatim.
    let snapshot = s.snapshot();
    let out = String::from_utf8(
        export_bpmn(&snapshot.project, doc, false)
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    for kept in [
        r#"camunda:assignee="clerk""#,
        r#"camunda:followUpTask="Task_b""#,
        r#"modeler:executionPlatform="Camunda Platform""#,
    ] {
        assert!(out.contains(kept), "missing {kept}");
    }
}
