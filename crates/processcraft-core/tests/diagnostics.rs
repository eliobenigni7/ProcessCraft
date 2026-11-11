use processcraft_core::{DiagnosticSeverity, ids::XmlId, model::*};
use std::sync::Arc;
fn id(s: &str) -> XmlId {
    s.parse().unwrap()
}
fn node(name: &str, kind: NodeKind, label: &str) -> Node {
    Node {
        activity: None,
        io_specification: None,
        loop_characteristics: None,
        details: None,
        id: id(name),
        name: label.into(),
        documentation: String::new(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
#[test]
fn incomplete_process_is_valid_but_diagnostics_are_anchored() {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: "p".into(),
        documentation: String::new(),
        nodes: vec![node("gateway", NodeKind::ExclusiveGateway, "")],
        lanes: vec![],
        flows: vec![],
    });
    let did = d.id;
    p.documents.push(Arc::new(d));
    assert!(p.validate().is_ok());
    let ds = p.diagnostics();
    assert_eq!(ds.len(), 4);
    for (code, anchor) in [
        ("missingStart", "process"),
        ("missingEnd", "process"),
        ("unconnectedNode", "gateway"),
        ("unnamedExclusiveGateway", "gateway"),
    ] {
        let d = ds.iter().find(|d| d.code == code).unwrap();
        assert_eq!(d.document_id, did);
        assert_eq!(d.element_id, Some(id(anchor)));
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
    }
}
#[test]
fn complete_connected_process_has_no_incomplete_warnings() {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    let f = |name: &str, source: &str, target: &str| Flow {
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: id(source),
        target: id(target),
        name: String::new(),
        documentation: String::new(),
        condition: None,
    };
    d.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: "p".into(),
        documentation: String::new(),
        nodes: vec![
            node("start", NodeKind::StartEvent, "Start"),
            node("gateway", NodeKind::Task, "Review"),
            node("end", NodeKind::EndEvent, "End"),
        ],
        lanes: vec![],
        flows: vec![f("one", "start", "gateway"), f("two", "gateway", "end")],
    });
    p.documents.push(Arc::new(d));
    assert!(p.validate().is_ok());
    assert!(p.diagnostics().is_empty());
}

#[test]
fn nested_gateway_diagnostics_are_advisory_and_anchored() {
    use processcraft_core::bpmn_model::{GatewayDirection, NodeDetails};
    let mut gateway = node("nested_gateway", NodeKind::ExclusiveGateway, "Decision");
    gateway.details = Some(NodeDetails::Gateway {
        default_flow: Some(id("default_flow")),
        direction: Some(GatewayDirection::Converging),
        instantiate: None,
        event_gateway_type: None,
        activation_condition: None,
    });
    let mut subprocess = node("sub", NodeKind::SubProcess, "Subprocess");
    subprocess.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            lane_set_id: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![gateway, node("nested_task", NodeKind::Task, "Task")],
            lanes: vec![],
            flows: vec![Flow {
                association_direction: None,
                id: id("default_flow"),
                kind: FlowKind::Sequence,
                source: id("nested_gateway"),
                target: id("nested_task"),
                name: String::new(),
                documentation: String::new(),
                condition: Some("true".into()),
            }],
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: "p".into(),
        documentation: String::new(),
        nodes: vec![subprocess],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    assert!(project.validate().is_ok());
    let diagnostics = project.diagnostics();
    for (code, anchor) in [
        ("gatewayDirectionMismatch", "nested_gateway"),
        ("defaultFlowCondition", "default_flow"),
    ] {
        let diagnostic = diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(diagnostic.element_id, Some(id(anchor)));
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    }
}
