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
        flow_metadata: None,
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
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            source,
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            target,
        ))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
    };
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
            lane_set_name: None,
            lane_set_id: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![gateway, node("nested_task", NodeKind::Task, "Task")],
            lanes: vec![],
            flows: vec![Flow {
                message_ref: None,
                flow_metadata: None,
                base_extras: None,
                association_direction: None,
                id: id("default_flow"),
                kind: FlowKind::Sequence,
                source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "nested_gateway",
                ))),
                target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "nested_task",
                ))),
                name: Some(String::new()),
                documentation: String::new().into(),
                condition: Some("true".into()),
            }],
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            source,
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            target,
        ))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: condition.map(Into::into),
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
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

#[test]
fn incomplete_activity_and_correlation_metadata_remain_advisory() {
    use processcraft_core::bpmn_activity::{
        CallableMetadata, CorrelationProperty, CorrelationPropertyBinding, CorrelationRetrieval,
        CorrelationSubscription, IoBinding, ProcessActivityMetadata,
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.correlation_properties.push(CorrelationProperty {
        id: Some(id("property")),
        base: None,
        name: None,
        item_type: None,
        retrieval_expressions: vec![CorrelationRetrieval {
            id: None,
            base: None,
            message_ref: None,
            message_path: None,
        }],
    });
    doc.processes.push(Process {
        activity_metadata: Some(ProcessActivityMetadata {
            callable: CallableMetadata {
                supported_interface_refs: vec![],
                io_bindings: vec![IoBinding {
                    id: None,
                    base: None,
                    operation_ref: None,
                    input_data_ref: None,
                    output_data_ref: None,
                }],
            },
            correlation_subscriptions: vec![CorrelationSubscription {
                id: None,
                base: None,
                key_ref: None,
                bindings: vec![CorrelationPropertyBinding {
                    id: None,
                    base: None,
                    property_ref: None,
                    data_path: None,
                }],
            }],
            ..Default::default()
        }),
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostics = project.diagnostics();
    for code in [
        "incompleteCorrelationRetrieval",
        "incompleteIoBinding",
        "incompleteCorrelationSubscription",
        "incompleteCorrelationBinding",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == code && d.severity == DiagnosticSeverity::Warning),
            "{code}"
        );
    }
}

#[test]
fn activity_reference_issue_keys_keep_owner_field_and_target_without_resolving_external() {
    use processcraft_core::bpmn_activity::{
        CallableMetadata, IoBinding, ModelReference, ProcessActivityMetadata,
    };
    use processcraft_core::bpmn_model::QNameRef;
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: Some(ProcessActivityMetadata {
            callable: CallableMetadata {
                supported_interface_refs: vec![
                    ModelReference::Local(id("missing_interface")),
                    ModelReference::Qualified(QNameRef {
                        namespace: "urn:external".into(),
                        local: id("remote"),
                    }),
                ],
                io_bindings: vec![IoBinding {
                    id: Some(id("binding")),
                    base: None,
                    operation_ref: Some(ModelReference::Local(id("missing_operation"))),
                    input_data_ref: None,
                    output_data_ref: None,
                }],
            },
            ..Default::default()
        }),
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert_eq!(issues.len(), 2);
    assert!(issues.iter().any(|issue| issue.owner
        == id::<processcraft_core::ids::ElementSelector>("process")
        && issue.field == "callable.supportedInterfaceRef"
        && issue.target == ModelReference::Local(id("missing_interface"))));
    assert!(issues.iter().any(|issue| issue.owner
        == id::<processcraft_core::ids::ElementSelector>("binding")
        && issue.field == "ioBinding.operationRef"
        && issue.target == ModelReference::Local(id("missing_operation"))));
}

#[test]
fn activity_reference_resolution_detects_reused_xml_id_with_new_identity() {
    use processcraft_core::bpmn_activity::{
        CallableMetadata, ModelReference, ProcessActivityMetadata,
    };
    use processcraft_core::bpmn_catalog::{Interface, Operation};
    use processcraft_core::bpmn_model::QNameRef;
    use processcraft_core::ids::{ElementIdentity, NativeElementId};
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    let target = |native_id| ElementIdentity::Native {
        native_id,
        xml_id: Some(id("interface")),
    };
    let first_id = NativeElementId::new();
    doc.catalog.interfaces.push(Interface {
        id: Some(target(first_id)),
        base: None,
        name: "Service".into(),
        implementation_ref: None,
        operations: vec![Operation {
            id: None,
            base: None,
            name: "Run".into(),
            implementation_ref: None,
            in_message_ref: QNameRef {
                namespace: "urn:external".into(),
                local: id("Message"),
            },
            out_message_ref: None,
            error_refs: vec![],
        }],
    });
    doc.processes.push(Process {
        activity_metadata: Some(ProcessActivityMetadata {
            callable: CallableMetadata {
                supported_interface_refs: vec![ModelReference::Qualified(QNameRef {
                    namespace: processcraft_core::model::GENERATED_TARGET_NAMESPACE.into(),
                    local: id("interface"),
                })],
                io_bindings: vec![],
            },
            ..Default::default()
        }),
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let before = processcraft_core::validation::activity_family_reference_resolutions(&doc);
    doc.catalog.interfaces[0].id = Some(target(NativeElementId::new()));
    let after = processcraft_core::validation::activity_family_reference_resolutions(&doc);
    assert_eq!(before[0].reference, after[0].reference);
    assert_ne!(before[0].resolved_target, after[0].resolved_target);
}

#[test]
fn subprocess_loop_data_references_respect_ancestor_and_sibling_scope() {
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_data::DataObject;
    use processcraft_core::bpmn_model::{LoopCharacteristics, NodeDetails};
    let object = |name: &str| DataObject {
        id: Some(id(name)),
        name: None,
        base: None,
        flow_metadata: None,
        is_collection: Some(true),
        item_aware: None,
    };
    let mut left = node("left", NodeKind::SubProcess, "Left");
    left.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            data_objects: vec![object("left_object")],
            ..Default::default()
        }),
    });
    let mut right = node("right", NodeKind::SubProcess, "Right");
    right.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer::default()),
    });
    right.loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        id: None,
        base: None,
        is_sequential: None,
        behavior: None,
        loop_data_input_ref: Some(ModelReference::Local(id("left_object"))),
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        cardinality: None,
        completion_condition: None,
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![object("root_object")],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![left, right],
        lanes: vec![],
        flows: vec![],
    });
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|issue| issue.field == "loop.loopDataInputRef"
                && issue.target == ModelReference::Local(id("left_object")))
    );
    if let Some(LoopCharacteristics::MultiInstance {
        loop_data_input_ref,
        ..
    }) = &mut doc.processes[0].nodes[1].loop_characteristics
    {
        *loop_data_input_ref = Some(ModelReference::Local(id("root_object")));
    }
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "loop.loopDataInputRef")
    );
}

#[test]
fn implicit_event_data_sources_respect_nested_scope() {
    use processcraft_core::bpmn_activity::{
        ComplexBehaviorDefinition, ImplicitInputAssociation, ImplicitThrowEvent, ModelReference,
    };
    use processcraft_core::bpmn_data::{DataObject, DataPort};
    use processcraft_core::bpmn_model::{LoopCharacteristics, NodeDetails};
    let object = |name: &str| DataObject {
        id: Some(id(name)),
        name: None,
        base: None,
        flow_metadata: None,
        is_collection: None,
        item_aware: None,
    };
    let mut left = node("left", NodeKind::SubProcess, "Left");
    left.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            data_objects: vec![object("left_object")],
            ..Default::default()
        }),
    });
    let mut task = node("task", NodeKind::Task, "Task");
    task.loop_characteristics = Some(LoopCharacteristics::MultiInstance {
        id: None,
        base: None,
        is_sequential: None,
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![ComplexBehaviorDefinition {
            id: None,
            base: None,
            condition: None,
            event: Some(ImplicitThrowEvent {
                id: None,
                name: None,
                base: None,
                flow: None,
                properties: vec![],
                incoming: vec![],
                outgoing: vec![],
                inputs: vec![DataPort {
                    id: Some(id("event_input")),
                    base: None,
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                input_associations: vec![ImplicitInputAssociation {
                    id: None,
                    base: None,
                    sources: vec![id("left_object")],
                    target: Some(id("event_input")),
                    transformation: None,
                    assignments: vec![],
                }],
                input_set: None,
                definitions: vec![],
                definition_refs: vec![],
            }),
        }],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        cardinality: None,
        completion_condition: None,
    });
    let mut right = node("right", NodeKind::SubProcess, "Right");
    right.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            nodes: vec![task],
            ..Default::default()
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![object("root_object")],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![left, right],
        lanes: vec![],
        flows: vec![],
    });
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "implicitEvent.associationSource"
                && issue.target == ModelReference::Local(id("left_object")))
    );
    if let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0].nodes[1].details
        && let Some(LoopCharacteristics::MultiInstance {
            complex_behavior_definitions,
            ..
        }) = &mut content.nodes[0].loop_characteristics
    {
        complex_behavior_definitions[0]
            .event
            .as_mut()
            .unwrap()
            .input_associations[0]
            .sources = vec![id("root_object")];
    }
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "implicitEvent.associationSource")
    );
    if let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0].nodes[1].details
        && let Some(LoopCharacteristics::MultiInstance {
            complex_behavior_definitions,
            ..
        }) = &mut content.nodes[0].loop_characteristics
    {
        complex_behavior_definitions[0]
            .event
            .as_mut()
            .unwrap()
            .definitions = vec![processcraft_core::bpmn_model::EventDefinition::Timer {
            id: None,
            base: None,
            kind: None,
            expression: None,
        }];
    }
    if let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0].nodes[1].details
        && let Some(LoopCharacteristics::MultiInstance {
            input_data_item,
            complex_behavior_definitions,
            ..
        }) = &mut content.nodes[0].loop_characteristics
    {
        *input_data_item = Some(DataPort {
            id: Some(id("loop_item")),
            base: None,
            name: None,
            item_aware: None,
            is_collection: None,
        });
        complex_behavior_definitions[0]
            .event
            .as_mut()
            .unwrap()
            .input_associations[0]
            .sources = vec![id("loop_item")];
    }
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "implicitEvent.associationSource")
    );
    if let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0].nodes[1].details
        && let Some(LoopCharacteristics::MultiInstance {
            complex_behavior_definitions,
            ..
        }) = &mut content.nodes[0].loop_characteristics
    {
        complex_behavior_definitions[0]
            .event
            .as_mut()
            .unwrap()
            .input_set = Some(processcraft_core::bpmn_data::IoSet {
            id: Some(id("input_set")),
            counterpart_refs: vec![id("output_set")],
            ..Default::default()
        });
    }
    doc.global_tasks
        .push(processcraft_core::bpmn_activity::GlobalTask {
            id: Some(id("global")),
            base: None,
            name: None,
            callable: Default::default(),
            resource_roles: vec![],
            io_specification: Some(processcraft_core::bpmn_data::IoSpecification {
                base: None,
                id: None,
                inputs: vec![],
                outputs: vec![],
                input_sets: vec![],
                output_sets: vec![processcraft_core::bpmn_data::IoSet {
                    id: Some(id("output_set")),
                    ..Default::default()
                }],
            }),
            kind: processcraft_core::bpmn_activity::GlobalTaskKind::Task,
        });
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "implicitEvent.inputSetCounterpart")
    );
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    assert!(
        project
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "implicitIoSetPairIncomplete")
    );
    assert!(
        project
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "implicitEventTriggerContextUnverified")
    );
}

#[test]
fn call_activity_local_target_is_checked_without_resolving_external() {
    use processcraft_core::bpmn_model::{NodeDetails, QNameRef};
    let mut call = node("call", NodeKind::CallActivity, "Call");
    call.details = Some(NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: processcraft_core::model::GENERATED_TARGET_NAMESPACE.into(),
            local: id("missing"),
        }),
    });
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![call],
        lanes: vec![],
        flows: vec![],
    });
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "callActivity.calledElement")
    );
    if let Some(NodeDetails::CallActivity {
        called_element: Some(reference),
    }) = &mut doc.processes[0].nodes[0].details
    {
        reference.namespace = "urn:external".into();
    }
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "callActivity.calledElement")
    );
}

#[test]
fn explicit_flow_node_refs_check_kind_scope_and_direction() {
    use processcraft_core::bpmn_activity::{FlowElementMetadata, ModelReference};
    use processcraft_core::bpmn_model::QNameRef;
    let mut source = node("source", NodeKind::Task, "Source");
    source.flow_metadata = Some(FlowElementMetadata {
        incoming: vec![ModelReference::Local(id("sequence"))],
        outgoing: vec![
            ModelReference::Local(id("sequence")),
            ModelReference::Qualified(QNameRef {
                namespace: "urn:external".into(),
                local: id("remote"),
            }),
        ],
        ..Default::default()
    });
    let flow = Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("sequence"),
        kind: FlowKind::Sequence,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "source",
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "target",
        ))),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![source, node("target", NodeKind::Task, "Target")],
        lanes: vec![],
        flows: vec![flow],
    });
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert_eq!(
        issues
            .iter()
            .filter(|issue| issue.field == "flow.incoming")
            .count(),
        1
    );
    assert!(!issues.iter().any(|issue| issue.field == "flow.outgoing"));
}

#[test]
fn message_task_cardinality_is_conditional_and_receive_instantiate_checks_incoming() {
    use processcraft_core::bpmn_activity::ActivityProperties;
    use processcraft_core::bpmn_data::{DataPort, IoSet, IoSpecification};
    use processcraft_core::bpmn_model::QNameRef;
    let port = |name: &str| DataPort {
        id: Some(id(name)),
        base: None,
        name: None,
        item_aware: None,
        is_collection: None,
    };
    let mut send = node("send", NodeKind::SendTask, "Send");
    send.io_specification = Some(IoSpecification {
        id: None,
        base: None,
        inputs: vec![port("a"), port("b")],
        outputs: vec![],
        input_sets: vec![IoSet::default(), IoSet::default()],
        output_sets: vec![IoSet::default()],
    });
    let mut receive = node("receive", NodeKind::ReceiveTask, "Receive");
    receive.activity = Some(ActivityProperties {
        instantiate: Some(true),
        ..Default::default()
    });
    let flow = Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id("sequence"),
        kind: FlowKind::Sequence,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "send",
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "receive",
        ))),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![send, receive],
        lanes: vec![],
        flows: vec![flow],
    });
    let diagnostics = |document: BpmnDocument| {
        let mut project = ProjectData::new("p".into());
        project.documents.push(Arc::new(document));
        project.diagnostics()
    };
    let before = diagnostics(doc.clone());
    assert!(!before.iter().any(|d| d.code == "messageTaskIoCardinality"));
    assert!(
        before
            .iter()
            .any(|d| d.code == "instantiatingReceiveIncoming")
    );
    doc.processes[0].nodes[0].activity = Some(ActivityProperties {
        message_ref: Some(QNameRef {
            namespace: "urn:external".into(),
            local: id("message"),
        }),
        ..Default::default()
    });
    let after = diagnostics(doc);
    assert!(after.iter().any(|d| d.code == "messageTaskIoCardinality"));
    assert!(
        !after
            .iter()
            .any(|d| d.code == "messageTaskItemEquivalenceUnverified")
    );
}

#[test]
fn participant_band_and_multiplicity_advisories_are_anchored() {
    use processcraft_core::model::{
        Bounds, Collaboration, Diagram, Participant, ParticipantBandKind, ParticipantMultiplicity,
        Shape,
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: Some(ParticipantMultiplicity {
                id: None,
                base: None,
                minimum: Some(2),
                maximum: Some(1),
            }),
            base: None,
            id: id("participant"),
            name: None,
            process: None,
        }],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    doc.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("collab"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: Some(true),
            participant_band_kind: Some(ParticipantBandKind::TopInitiating),
            choreography_activity_shape: None,
            id: id("shape"),
            element: id("participant"),
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 20.0,
            },
            label: None,
            label_visibility: None,
            style: None,
        }],
        edges: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let diagnostics = project.diagnostics();
    for (code, anchor) in [
        ("participantMultiplicityRange", "participant"),
        ("participantBandMissingOwner", "shape"),
    ] {
        assert!(
            diagnostics.iter().any(|d| d.code == code
                && d.element_id == Some(id(anchor))
                && d.severity == DiagnosticSeverity::Warning),
            "{code}"
        );
    }
}

#[test]
fn participant_band_owner_resolves_to_same_diagram_activity_shape() {
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    use processcraft_core::model::{
        Bounds, DiReference, Diagram, Participant, ParticipantBandKind, Shape,
    };
    let local = |name: &str| processcraft_core::bpmn_model::EventReference::Local(id(name));
    let shape = |name: &str, element: &str| Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        id: id(name),
        element: id(element),
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        },
        label: None,
        label_visibility: None,
        style: None,
    };
    let participant = |name: &str| Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(name),
        name: None,
        process: None,
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.choreographies.push(Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: vec![participant("p1"), participant("p2")],
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            loop_type: None,
            details: None,
            flow_metadata: None,
            correlation_keys: vec![],
            base: None,
            id: id("task"),
            name: None,
            participant_refs: vec![local("p1"), local("p2")],
            initiating_participant: Some(local("p1")),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![],
    });
    let mut band = shape("band", "p1");
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(id("owner_other_diagram")));
    let diagram = |name: &str, plane: &str, shapes| Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: id(name),
        plane_id: id(plane),
        plane: id("choreo"),
        shapes,
        edges: vec![],
    };
    doc.diagrams.push(diagram(
        "first",
        "plane1",
        vec![shape("owner_here", "task"), band],
    ));
    doc.diagrams.push(diagram(
        "second",
        "plane2",
        vec![shape("owner_other_diagram", "task")],
    ));
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "di.choreographyActivityShape")
    );
    doc.diagrams[0].shapes[1].choreography_activity_shape =
        Some(DiReference::Local(id("owner_here")));
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "di.choreographyActivityShape")
    );
    doc.diagrams[0].shapes[1].is_message_visible = Some(true);
    doc.choreographies[0].message_flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id("request"),
        kind: FlowKind::Message,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "p2",
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            "p1",
        ))),
        name: None,
        documentation: String::new().into(),
        condition: None,
    });
    doc.choreographies[0].activities[0].message_flow_refs = vec![local("request")];
    let has_warning = |doc: &BpmnDocument, code: &str| {
        let mut project = ProjectData::new("p".into());
        project.documents.push(Arc::new(doc.clone()));
        project
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == code)
    };
    assert!(has_warning(&doc, "participantBandMessageMissing"));
    doc.choreographies[0].activities[0]
        .message_flow_refs
        .push(local("unresolved"));
    assert!(!has_warning(&doc, "participantBandMessageMissing"));
    doc.choreographies[0].activities[0].message_flow_refs.pop();
    doc.choreographies[0].message_flows[0].source = Some(
        processcraft_core::bpmn_model::EventReference::Local(id("p1")),
    );
    assert!(!has_warning(&doc, "participantBandMessageMissing"));
    doc.diagrams[0].shapes[1].participant_band_kind = Some(ParticipantBandKind::MiddleInitiating);
    assert!(has_warning(&doc, "participantBandMiddleMessage"));
    doc.choreographies[0].participants[0].process = Some(
        processcraft_core::bpmn_model::EventReference::Local(id("absent_process")),
    );
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|issue| issue.field == "participant.processRef")
    );
    doc.choreographies[0].participants[0].process =
        Some(processcraft_core::bpmn_model::EventReference::Qualified(
            processcraft_core::bpmn_model::QNameRef {
                namespace: "urn:foreign".into(),
                local: id("absent_process"),
            },
        ));
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "participant.processRef")
    );
    doc.choreographies[0].participants[0].process = None;
    doc.choreographies[0].message_flows[0].target = Some(
        processcraft_core::bpmn_model::EventReference::Local(id("absent_participant")),
    );
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|issue| issue.field == "messageFlow.targetRef")
    );
    doc.choreographies[0].message_flows[0].target = Some(
        processcraft_core::bpmn_model::EventReference::Local(id("p2")),
    );
    doc.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("other_root"),
        participants: vec![participant("other_participant")],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    let local = |name: &str| {
        Some(processcraft_core::bpmn_model::EventReference::Local(id(
            name,
        )))
    };
    let association = processcraft_core::bpmn_collaboration::ParticipantAssociation {
        id: None,
        base: None,
        inner: local("other_participant"),
        outer: local("other_participant"),
    };
    doc.choreographies[0].extras =
        Some(processcraft_core::bpmn_collaboration::CollaborationExtras {
            participant_associations: vec![association],
            ..Default::default()
        });
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|issue| issue.field == "participantAssociation.outerRef")
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.field == "participantAssociation.innerRef")
    );
    doc.choreographies[0]
        .extras
        .as_mut()
        .unwrap()
        .participant_associations[0]
        .outer = local("p1");
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        !issues
            .iter()
            .any(|issue| issue.field == "participantAssociation.outerRef")
    );
    doc.choreographies[0].conversation_nodes.push(
        processcraft_core::bpmn_collaboration::ConversationNode {
            message_flow_refs: vec![],
            call: None,
            correlation_keys: vec![],
            base: None,
            id: id("conversation"),
            name: None,
            kind: processcraft_core::bpmn_collaboration::ConversationKind::Conversation,
            participant_refs: vec![],
            parent: None,
        },
    );
    doc.choreographies[0].conversation_links.push(
        processcraft_core::bpmn_collaboration::ConversationLink {
            id: id("link"),
            name: None,
            base: None,
            source: local("conversation"),
            target: local("p1"),
        },
    );
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field.starts_with("conversationLink."))
    );
    doc.choreographies[0].conversation_links[0].source = local("p2");
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field.starts_with("conversationLink."))
    );
    let mut second_choreography = doc.choreographies[0].clone();
    second_choreography.id = id("second_choreography");
    second_choreography.participants = vec![participant("inner_second")];
    second_choreography.activities.clear();
    second_choreography.message_flows.clear();
    second_choreography.conversation_nodes.clear();
    second_choreography.conversation_links.clear();
    second_choreography.extras = None;
    doc.choreographies.push(second_choreography);
    let extras = doc.choreographies[0].extras.as_mut().unwrap();
    extras.choreography_refs = vec![
        processcraft_core::bpmn_model::EventReference::Local(id("choreo")),
        processcraft_core::bpmn_model::EventReference::Local(id("second_choreography")),
    ];
    extras.participant_associations[0].inner = local("p1");
    assert!(processcraft_core::validation::association_inner_scope_unqualified(&doc).is_empty());
    doc.choreographies[0]
        .extras
        .as_mut()
        .unwrap()
        .participant_associations[0]
        .inner = local("inner_second");
    assert!(processcraft_core::validation::association_inner_scope_unqualified(&doc).is_empty());
    doc.choreographies[0]
        .extras
        .as_mut()
        .unwrap()
        .participant_associations[0]
        .inner = local("other_participant");
    assert_eq!(
        processcraft_core::validation::association_inner_scope_unqualified(&doc).len(),
        1
    );
    let process =
        |name: &str,
         metadata: Option<processcraft_core::bpmn_activity::ProcessActivityMetadata>| {
            Process {
                activity_metadata: metadata,
                base_extras: None,
                is_executable: None,
                io_specification: None,
                lane_set_base: None,
                lane_set_name: None,
                lane_set_id: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                id: id(name),
                name: None,
                documentation: String::new().into(),
                nodes: vec![],
                lanes: vec![],
                flows: vec![],
            }
        };
    let metadata = processcraft_core::bpmn_activity::ProcessActivityMetadata {
        definitional_collaboration_ref: local("other_root"),
        ..Default::default()
    };
    doc.processes
        .push(process("caller_process", Some(metadata.clone())));
    doc.choreographies[0].participants[0].process = local("caller_process");
    assert!(processcraft_core::validation::association_inner_scope_unqualified(&doc).is_empty());
    doc.processes[0].activity_metadata = None;
    doc.processes
        .push(process("called_process", Some(metadata)));
    let mut call_node = node("call", NodeKind::CallActivity, "call");
    call_node.details = Some(processcraft_core::bpmn_model::NodeDetails::CallActivity {
        called_element: Some(processcraft_core::bpmn_model::QNameRef {
            namespace: processcraft_core::model::GENERATED_TARGET_NAMESPACE.into(),
            local: id("called_process"),
        }),
    });
    doc.processes[0].nodes.push(call_node);
    doc.processes[1]
        .nodes
        .push(node("unrelated_task", NodeKind::Task, "other"));
    doc.categories
        .push(processcraft_core::bpmn_model::Category {
            id: id("category"),
            name: None,
            base: None,
            values: vec![processcraft_core::bpmn_model::CategoryValue {
                id: id("category_value"),
                value: None,
                base: None,
            }],
        });
    doc.processes[0]
        .groups
        .push(processcraft_core::bpmn_model::Group {
            id: id("group"),
            base: None,
            category_value_ref: local("category_value"),
        });
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "group.categoryValueRef")
    );
    doc.processes[0].groups[0].category_value_ref = local("category");
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "group.categoryValueRef")
    );
    doc.processes[0].groups[0].category_value_ref =
        Some(processcraft_core::bpmn_model::EventReference::Qualified(
            processcraft_core::bpmn_model::QNameRef {
                namespace: "urn:foreign".into(),
                local: id("category"),
            },
        ));
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "group.categoryValueRef")
    );

    doc.choreographies[0].message_flows[0].source = local("call");
    assert!(
        processcraft_core::validation::message_flow_endpoint_scope_unqualified(&doc).is_empty()
    );
    doc.choreographies[0].message_flows[0].source = local("unrelated_task");
    assert_eq!(
        processcraft_core::validation::message_flow_endpoint_scope_unqualified(&doc).len(),
        1
    );
    doc.choreographies[0].message_flows[0].source = local("p1");
    doc.choreographies[0].conversation_links[0].source = local("conversation");
    doc.choreographies[0].conversation_links[0].target = local("call");
    assert!(
        processcraft_core::validation::conversation_link_endpoint_scope_unqualified(&doc)
            .is_empty()
    );
    doc.choreographies[0].conversation_links[0].target = local("unrelated_task");
    assert_eq!(
        processcraft_core::validation::conversation_link_endpoint_scope_unqualified(&doc).len(),
        1
    );
    assert!(processcraft_core::validation::association_inner_scope_unqualified(&doc).is_empty());
    doc.collaboration.as_mut().unwrap().kind =
        Some(processcraft_core::bpmn_collaboration::CollaborationRootKind::GlobalConversation);
    doc.collaboration.as_mut().unwrap().extras =
        Some(processcraft_core::bpmn_collaboration::CollaborationExtras {
            choreography_refs: vec![processcraft_core::bpmn_model::EventReference::Local(id(
                "choreo",
            ))],
            ..Default::default()
        });
    assert!(has_warning(&doc, "globalConversationChoreographyRef"));
    doc.choreographies[0].activities[0].participant_refs =
        vec![processcraft_core::bpmn_model::EventReference::Local(id(
            "p2",
        ))];
    assert!(
        processcraft_core::validation::activity_family_reference_issues(&doc)
            .iter()
            .any(|issue| issue.field == "di.choreographyActivityShape")
    );
}

#[test]
fn choreography_graph_reports_bounded_semantics_without_matching_external_names() {
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyFlowElement, ChoreographyGraph,
        ChoreographyRootKind,
    };
    use processcraft_core::bpmn_model::QNameRef;
    let local = |name: &str| ModelReference::Local(id(name));
    let participant = |name: &str| Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(name),
        name: None,
        process: None,
    };
    let activity = ChoreographyActivity {
        loop_type: None,
        details: None,
        flow_metadata: None,
        correlation_keys: vec![],
        base: None,
        id: id("task"),
        name: None,
        participant_refs: vec![local("p1"), local("p2"), local("p3")],
        initiating_participant: Some(local("p1")),
        message_flow_refs: vec![ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: id("same"),
        })],
    };
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.choreographies.push(Choreography {
        kind: Some(ChoreographyRootKind::Global {
            initiating_participant_ref: None,
        }),
        graph: Some(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(activity))],
        }),
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: vec![participant("p1"), participant("p2"), participant("p3")],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    });
    let codes = |doc: &BpmnDocument| {
        let mut project = ProjectData::new("p".into());
        project.documents.push(Arc::new(doc.clone()));
        project
            .diagnostics()
            .into_iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    };
    let found = codes(&doc);
    assert!(found.contains(&"globalChoreographyFlowElements"));
    assert!(found.contains(&"choreographyTaskParticipantCount"));
    assert!(!found.contains(&"choreographyMessageFlowMissing"));
    assert!(!found.contains(&"choreographyTaskMessageFlowSchemaCardinality"));
    assert!(processcraft_core::validation::choreography_reference_issues(&doc).is_empty());
    let ChoreographyFlowElement::Activity(a) =
        &mut doc.choreographies[0].graph.as_mut().unwrap().flow_elements[0]
    else {
        unreachable!()
    };
    a.message_flow_refs.push(local("missing"));
    let issues = processcraft_core::validation::choreography_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|i| i.field == "choreography.messageFlowRef" && i.target == local("missing"))
    );
    let ChoreographyFlowElement::Activity(a) =
        &mut doc.choreographies[0].graph.as_mut().unwrap().flow_elements[0]
    else {
        unreachable!()
    };
    a.message_flow_refs.pop();
    doc.choreographies[0].kind = None;
    let ChoreographyFlowElement::Activity(a) =
        &mut doc.choreographies[0].graph.as_mut().unwrap().flow_elements[0]
    else {
        unreachable!()
    };
    a.participant_refs.pop();
    a.message_flow_refs.clear();
    let found = codes(&doc);
    assert!(!found.contains(&"globalChoreographyFlowElements"));
    assert!(!found.contains(&"choreographyTaskParticipantCount"));
    assert!(found.contains(&"choreographyTaskMessageFlowSchemaCardinality"));
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut event = node("event", NodeKind::StartEvent, "start");
    event.details = Some(NodeDetails::Event {
        definitions: vec![EventDefinition::Message {
            id: None,
            base: None,
            message_ref: Some(local("missing_message")),
            operation_ref: None,
        }],
        attached_to: None,
        cancel_activity: None,
        definition_refs: vec![local("missing_definition")],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    });
    doc.choreographies[0]
        .graph
        .as_mut()
        .unwrap()
        .flow_elements
        .push(ChoreographyFlowElement::Event(Box::new(event)));
    let issues = processcraft_core::validation::activity_family_reference_issues(&doc);
    assert!(
        issues
            .iter()
            .any(|i| i.field == "event.messageRef" && i.target == local("missing_message"))
    );
    assert!(
        issues
            .iter()
            .any(|i| i.field == "event.eventDefinitionRef"
                && i.target == local("missing_definition"))
    );
    assert!(codes(&doc).contains(&"choreographyEventDefinitionWrongContext"));
    doc.data_stores
        .push(processcraft_core::bpmn_data::DataStore {
            base: None,
            id: Some(id("store")),
            name: None,
            capacity: None,
            is_unlimited: None,
            item_aware: None,
        });
    doc.choreographies[0]
        .graph
        .as_mut()
        .unwrap()
        .data_associations
        .push(processcraft_core::bpmn_data::DataAssociation {
            base: None,
            id: Some(id("data_assoc")),
            owner: id("event"),
            kind: processcraft_core::bpmn_data::DataAssociationKind::Output,
            sources: vec![],
            target: id("store"),
            transformation: None,
            assignments: vec![],
        });
    assert!(codes(&doc).contains(&"choreographyDataRepositoryUse"));
    doc.choreographies[0]
        .graph
        .as_mut()
        .unwrap()
        .data_associations
        .clear();
    assert!(!codes(&doc).contains(&"choreographyDataRepositoryUse"));
    let ChoreographyFlowElement::Activity(template) =
        &doc.choreographies[0].graph.as_ref().unwrap().flow_elements[0]
    else {
        unreachable!()
    };
    let mut sub = (**template).clone();
    sub.id = id("sub");
    sub.details = Some(
        processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Sub {
            graph: Box::new(ChoreographyGraph {
                data_associations: vec![],
                flow_elements: vec![],
            }),
            artifacts: vec![
                processcraft_core::bpmn_collaboration::CollaborationArtifact::Group(
                    processcraft_core::bpmn_model::Group {
                        base: None,
                        id: id("group"),
                        category_value_ref: Some(local("missing_category")),
                    },
                ),
            ],
        },
    );
    doc.choreographies[0]
        .graph
        .as_mut()
        .unwrap()
        .flow_elements
        .push(ChoreographyFlowElement::Activity(Box::new(sub)));
    assert!(
        processcraft_core::validation::choreography_reference_issues(&doc)
            .iter()
            .any(
                |i| i.owner == id::<processcraft_core::ids::ElementSelector>("group")
                    && i.field == "group.categoryValueRef"
                    && i.target == local("missing_category")
            )
    );
    let ChoreographyFlowElement::Activity(template) =
        &doc.choreographies[0].graph.as_ref().unwrap().flow_elements[0]
    else {
        unreachable!()
    };
    let mut before = (**template).clone();
    before.id = id("before");
    before.participant_refs = vec![local("p1"), local("p2")];
    let mut target = (**template).clone();
    target.id = id("after");
    target.participant_refs = vec![local("p2"), local("p3")];
    target.initiating_participant = Some(local("p3"));
    let gateway = node("gateway", NodeKind::ExclusiveGateway, "decision");
    let flow = |name: &str, source: &str, target: &str| Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(local(source)),
        target: Some(local(target)),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    };
    doc.choreographies[0].graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![
            ChoreographyFlowElement::Activity(Box::new(before)),
            ChoreographyFlowElement::Gateway(Box::new(gateway)),
            ChoreographyFlowElement::Activity(Box::new(target)),
            ChoreographyFlowElement::SequenceFlow(Box::new(flow("f1", "before", "gateway"))),
            ChoreographyFlowElement::SequenceFlow(Box::new(flow("f2", "gateway", "after"))),
        ],
    });
    assert!(codes(&doc).contains(&"choreographyInitiatorNotPreviouslyInvolved"));
    let ChoreographyFlowElement::SequenceFlow(edge) =
        &mut doc.choreographies[0].graph.as_mut().unwrap().flow_elements[3]
    else {
        unreachable!()
    };
    edge.source = Some(local("missing"));
    assert!(!codes(&doc).contains(&"choreographyInitiatorNotPreviouslyInvolved"));
    assert!(codes(&doc).contains(&"choreographyInitiatorSequenceUnqualified"));
}

#[test]
fn compensation_event_subprocess_and_ad_hoc_children_are_not_unconnected() {
    use processcraft_core::{bpmn_activity::ActivityProperties, bpmn_model::NodeDetails};
    let container = |nodes: Vec<Node>| {
        Some(NodeDetails::SubProcess {
            content: Box::new(FlowContainer {
                lane_set_base: None,
                lane_set_name: None,
                lane_set_id: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                nodes,
                lanes: vec![],
                flows: vec![],
            }),
        })
    };
    let mut compensation = node("comp", NodeKind::Task, "Undo");
    compensation.activity = Some(ActivityProperties {
        is_for_compensation: Some(true),
        ..Default::default()
    });
    let mut event_sub = node("event_sub", NodeKind::SubProcess, "On error");
    event_sub.activity = Some(ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    });
    event_sub.details = container(vec![]);
    let mut ad_hoc = node("adhoc", NodeKind::AdHocSubProcess, "Ad hoc");
    ad_hoc.details = container(vec![node("adhoc_child", NodeKind::Task, "Any order")]);
    let mut doc = BpmnDocument::new("d".into(), id("defs"));
    doc.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("p".into()),
        documentation: String::new().into(),
        nodes: vec![
            compensation,
            event_sub,
            ad_hoc,
            node("plain", NodeKind::Task, "Plain"),
        ],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    assert!(project.validate().is_ok());
    let unconnected: Vec<_> = project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "unconnectedNode")
        .filter_map(|d| d.element_id.map(|id| id.to_string()))
        .collect();
    assert!(unconnected.contains(&"plain".to_owned()));
    for exempt in ["comp", "event_sub", "adhoc_child"] {
        assert!(!unconnected.contains(&exempt.to_owned()), "{exempt}");
    }
}
