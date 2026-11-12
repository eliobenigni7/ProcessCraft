use processcraft_core::{DiagnosticSeverity, model::*};
use std::sync::Arc;
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn node(name: &str, kind: NodeKind, label: &str) -> Node {
    Node {
        base_extras: None,
        item_aware: None,
        activity: None,
        io_specification: None,
        loop_characteristics: None,
        details: None,
        id: id(name),
        name: Some(label.into()),
        documentation: String::new().into(),
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
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("p".into()),
        documentation: String::new().into(),
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
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: id(source),
        target: id(target),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
    };
    d.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("p".into()),
        documentation: String::new().into(),
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
            lane_set_base: None,
            lane_set_id: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![gateway, node("nested_task", NodeKind::Task, "Task")],
            lanes: vec![],
            flows: vec![Flow {
                base_extras: None,
                association_direction: None,
                id: id("default_flow"),
                kind: FlowKind::Sequence,
                source: id("nested_gateway"),
                target: id("nested_task"),
                name: Some(String::new()),
                documentation: String::new().into(),
                condition: Some("true".into()),
            }],
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("p".into()),
        documentation: String::new().into(),
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

#[test]
fn activity_conditions_are_allowed_but_event_gateway_conditions_are_diagnosed() {
    // BPMN 2.0.2 §8.4.13 permits conditions on Activity flows (with another
    // outgoing flow); §10.6.6 prohibits them on Event-Based Gateway flows.
    let flow = |name: &str, source: &str, target: &str, condition: Option<&str>| Flow {
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: id(source),
        target: id(target),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: condition.map(Into::into),
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("p".into()),
        documentation: String::new().into(),
        nodes: vec![
            node("task", NodeKind::Task, "Task"),
            node("gateway", NodeKind::EventBasedGateway, "Events"),
            node("receive1", NodeKind::ReceiveTask, "Receive 1"),
            node("receive2", NodeKind::ReceiveTask, "Receive 2"),
            node("end", NodeKind::EndEvent, "End"),
        ],
        lanes: vec![],
        flows: vec![
            flow("task_condition", "task", "gateway", Some("true")),
            flow("task_other", "task", "end", None),
            flow("gateway_condition", "gateway", "receive1", Some("true")),
            flow("gateway_other", "gateway", "receive2", None),
        ],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    assert!(project.validate().is_ok());
    let diagnostics = project.diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.element_id == Some(id("task_condition")))
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "eventGatewayConditionalFlow"
                && d.element_id == Some(id("gateway_condition")))
    );
}

#[test]
fn diagnostics_anchor_anonymous_native_elements_without_xml_fallback() {
    use processcraft_core::ids::{ElementIdentity, NativeElementId};
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    let mut anonymous = node("temporary", NodeKind::Task, "");
    anonymous.id = ElementIdentity::Native {
        native_id: NativeElementId::new(),
        xml_id: None,
    };
    let selector = anonymous.id.selector();
    doc.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        nodes: vec![anonymous],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostic = project
        .diagnostics()
        .into_iter()
        .find(|d| d.code == "unconnectedNode")
        .unwrap();
    assert_eq!(diagnostic.element_id, Some(selector));
}

#[test]
fn incomplete_event_metadata_is_advisory_and_anchored() {
    use processcraft_core::bpmn_model::{
        EventDefinition, EventIo, EventReference, NodeDetails, QNameRef,
    };
    let mut start = node("start", NodeKind::StartEvent, "Start");
    start.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Timer {
            id: None,
            base: None,
            kind: None,
            expression: None,
        }],
        attached_to: None,
        cancel_activity: None,
        definition_refs: vec![],
        parallel_multiple: Some(true),
        is_interrupting: Some(false),
        properties: vec![],
        io: Some(EventIo::Catch {
            outputs: vec![processcraft_core::bpmn_data::DataPort {
                base: None,
                id: Some(id("port")),
                name: None,
                item_aware: None,
                is_collection: None,
            }],
            output_set: None,
        }),
    });
    let mut external = node("external", NodeKind::IntermediateCatchEvent, "External");
    external.details = Some(NodeDetails::Event {
        definitions: vec![],
        attached_to: None,
        cancel_activity: None,
        definition_refs: vec![EventReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: id("event"),
        })],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        nodes: vec![start, external],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostics = project.diagnostics();
    for (code, anchor) in [
        ("incompleteTimerDefinition", "start"),
        ("parallelMultipleNotApplicable", "start"),
        ("startInterruptingIgnored", "start"),
        ("incompleteEventIo", "start"),
        ("externalEventDefinitionUnverified", "external"),
    ] {
        let diagnostic = diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(diagnostic.element_id, Some(id(anchor)));
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    }
}

#[test]
fn referenced_event_subprocess_error_interrupting_advisory_uses_root_definition() {
    use processcraft_core::bpmn_model::{EventDefinition, EventReference, NodeDetails};
    let mut start = node("start", NodeKind::StartEvent, "Start");
    start.details = Some(NodeDetails::Event {
        definitions: vec![],
        definition_refs: vec![EventReference::Local(id("error_definition"))],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: Some(false),
        properties: vec![],
        io: None,
    });
    let mut subprocess = node("sub", NodeKind::SubProcess, "Subprocess");
    subprocess.activity = Some(processcraft_core::bpmn_activity::ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    });
    subprocess.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            nodes: vec![start],
            ..Default::default()
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.root_event_definitions.push(EventDefinition::Error {
        base: None,
        id: Some(id("error_definition")),
        error_ref: None,
    });
    doc.processes.push(Process {
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        nodes: vec![subprocess],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostics = project.diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "startInterruptingTriggerMismatch"
                && d.element_id == Some(id("start")))
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == "startInterruptingIgnored" && d.element_id == Some(id("start")))
    );
}

#[test]
fn mixed_expression_is_not_misclassified_as_empty_plain_text() {
    use processcraft_core::bpmn_model::{
        EventDefinition, Expression, ExpressionBody, ExpressionData, ExpressionKind,
        ExpressionPart, NodeDetails, TimerKind,
    };
    let mut project = ProjectData::new("p".into());
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    let mut timer = node("timer", NodeKind::StartEvent, "Timer");
    timer.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Timer {
            id: None,
            base: None,
            kind: Some(TimerKind::Duration),
            expression: Some(Expression::Structured(ExpressionData {
                id: None,
                kind: ExpressionKind::Base {
                    explicit_type: false,
                },
                text: ExpressionBody::Mixed {
                    parts: vec![ExpressionPart::Comment {
                        text: "metadata".into(),
                    }],
                },
                namespaces: vec![],
                foreign_attributes: vec![],
            })),
        }],
        definition_refs: vec![],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    });
    doc.processes.push(Process {
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![timer],
        lanes: vec![],
        flows: vec![],
    });
    project.documents.push(Arc::new(doc));
    assert!(
        !project
            .diagnostics()
            .iter()
            .any(|d| d.code == "emptyExpression")
    );
}

#[test]
fn required_extensions_and_incomplete_relationships_are_advisory() {
    use processcraft_core::model::{DefinitionsExtension, DefinitionsRelationship};
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.extensions.push(DefinitionsExtension {
        documentation: vec![],
        definition: None,
        must_understand: Some(true),
    });
    doc.relationships.push(DefinitionsRelationship {
        id: Some(id("relation")),
        base: None,
        relationship_type: "trace".into(),
        direction: None,
        sources: vec![],
        targets: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostics = project.diagnostics();
    for (code, anchor) in [
        ("requiredExtensionUnverified", "defs"),
        ("incompleteRelationship", "relation"),
    ] {
        let d = diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(d.element_id, Some(id(anchor)));
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
    }
}
