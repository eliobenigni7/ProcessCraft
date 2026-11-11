use processcraft_core::{
    bpmn_data::*,
    bpmn_model::*,
    ids::{ElementIdentity, ElementSelector, XmlId},
    model::*,
};
use std::sync::Arc;
fn xml(s: &str) -> XmlId {
    s.parse().unwrap()
}
fn ident(s: &str) -> ElementIdentity {
    xml(s).into()
}
fn selector(s: &str) -> ElementSelector {
    xml(s).into()
}
fn event(id: &str, kind: NodeKind, details: NodeDetails) -> Node {
    Node {
        id: ident(id),
        name: None,
        documentation: String::new(),
        kind,
        details: Some(details),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    }
}
fn fixture() -> ProjectData {
    let mut d = BpmnDocument::new("events".into(), ident("defs"));
    d.root_event_definitions.push(EventDefinition::Timer {
        id: Some(ident("timer")),
        kind: None,
        expression: None,
    });
    let start = event(
        "start",
        NodeKind::StartEvent,
        NodeDetails::Event {
            definitions: vec![EventDefinition::Signal {
                id: None,
                signal_ref: None,
            }],
            definition_refs: vec![EventReference::Local(selector("timer"))],
            parallel_multiple: Some(true),
            is_interrupting: Some(false),
            attached_to: None,
            cancel_activity: None,
            properties: vec![EventProperty {
                id: Some(ident("property")),
                name: None,
                item_aware: None,
            }],
            io: Some(EventIo::Catch {
                outputs: vec![DataPort {
                    id: Some(ident("event_out")),
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                output_set: Some(IoSet {
                    id: None,
                    name: None,
                    data_refs: vec![selector("event_out")],
                    optional_refs: vec![],
                    while_executing_refs: vec![],
                    counterpart_refs: vec![],
                }),
            }),
        },
    );
    let throw = event(
        "throw",
        NodeKind::IntermediateThrowEvent,
        NodeDetails::Event {
            definitions: vec![],
            definition_refs: vec![EventReference::Qualified(QNameRef {
                namespace: "urn:external".into(),
                local: xml("external"),
            })],
            parallel_multiple: None,
            is_interrupting: None,
            attached_to: None,
            cancel_activity: None,
            properties: vec![],
            io: None,
        },
    );
    d.processes.push(Process {
        id: ident("process"),
        name: None,
        documentation: String::new(),
        is_executable: None,
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![DataAssociation {
            id: Some(ident("data_assoc")),
            owner: selector("start"),
            kind: DataAssociationKind::Output,
            sources: vec![selector("event_out")],
            target: selector("property"),
            transformation: None,
            assignments: vec![],
        }],
        nodes: vec![start, throw],
        lanes: vec![],
        flows: vec![],
    });
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(d));
    project
}
#[test]
fn reused_definitions_multiple_and_event_data_validate_without_synthetic_ids() {
    let project = fixture();
    project.validate().unwrap();
    let d = &project.documents[0];
    assert!(d.has_expanded_events_and_di());
    assert!(
        d.element_identities()
            .iter()
            .any(|id| id.xml_id() == Some(&xml("event_out")))
    );
    let serialized = serde_json::to_vec(&project).unwrap();
    let restored: ProjectData = serde_json::from_slice(&serialized).unwrap();
    restored.validate().unwrap();
    assert_eq!(
        restored.content_identity().unwrap(),
        project.content_identity().unwrap()
    );
}
#[test]
fn missing_local_root_wrong_event_direction_and_duplicate_ids_are_rejected() {
    let p = fixture();
    let mut missing = p.clone();
    Arc::make_mut(&mut missing.documents[0])
        .root_event_definitions
        .clear();
    assert!(missing.validate().is_err());
    let mut wrong = p.clone();
    let d = Arc::make_mut(&mut wrong.documents[0]);
    let Some(NodeDetails::Event { io, .. }) = &mut d.processes[0].nodes[0].details else {
        unreachable!()
    };
    *io = Some(EventIo::Throw {
        inputs: vec![],
        input_set: None,
    });
    assert!(wrong.validate().is_err());
    let mut duplicate = p;
    let d = Arc::make_mut(&mut duplicate.documents[0]);
    d.root_event_definitions.push(EventDefinition::Timer {
        id: Some(ident("timer")),
        kind: None,
        expression: None,
    });
    assert!(duplicate.validate().is_err());
}
#[test]
fn qualified_root_reference_keeps_namespace_and_requires_local_kind() {
    let mut project = fixture();
    let d = Arc::make_mut(&mut project.documents[0]);
    d.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:example".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    });
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[0].details
    else {
        unreachable!()
    };
    definition_refs[0] = EventReference::Qualified(QNameRef {
        namespace: "urn:example".into(),
        local: xml("timer"),
    });
    project.validate().unwrap();
    let mut invalid = project.clone();
    let d = Arc::make_mut(&mut invalid.documents[0]);
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[0].details
    else {
        unreachable!()
    };
    definition_refs[0] = EventReference::Qualified(QNameRef {
        namespace: "urn:example".into(),
        local: xml("start"),
    });
    assert!(invalid.validate().is_err());
    definition_refs_external(&mut project);
    project.validate().unwrap();
}
#[test]
fn repeated_root_uses_are_rejected_without_namespace_aliasing() {
    let mut p = fixture();
    let d = Arc::make_mut(&mut p.documents[0]);
    d.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:example".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    });
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[0].details
    else {
        unreachable!()
    };
    definition_refs.push(EventReference::Qualified(QNameRef {
        namespace: "urn:example".into(),
        local: xml("timer"),
    }));
    assert!(p.validate().is_err());
    let mut external = fixture();
    let d = Arc::make_mut(&mut external.documents[0]);
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[1].details
    else {
        unreachable!()
    };
    definition_refs.push(definition_refs[0].clone());
    assert!(external.validate().is_err());
    let mut distinct = fixture();
    let d = Arc::make_mut(&mut distinct.documents[0]);
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[1].details
    else {
        unreachable!()
    };
    definition_refs.push(EventReference::Qualified(QNameRef {
        namespace: "urn:another".into(),
        local: xml("external"),
    }));
    distinct.validate().unwrap();
}
#[test]
fn error_start_is_valid_only_in_event_subprocess_context() {
    let mut project = fixture();
    let doc = Arc::make_mut(&mut project.documents[0]);
    let start = event(
        "error_start",
        NodeKind::StartEvent,
        NodeDetails::Event {
            definitions: vec![EventDefinition::Error {
                id: None,
                error_ref: None,
            }],
            definition_refs: vec![],
            attached_to: None,
            cancel_activity: None,
            parallel_multiple: None,
            is_interrupting: Some(true),
            properties: vec![],
            io: None,
        },
    );
    doc.processes[0].nodes.push(Node {
        id: ident("event_sub"),
        name: None,
        documentation: String::new(),
        kind: NodeKind::SubProcess,
        details: Some(NodeDetails::SubProcess {
            content: Box::new(FlowContainer {
                lane_set_id: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                nodes: vec![start],
                lanes: vec![],
                flows: vec![],
            }),
        }),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: Some(processcraft_core::bpmn_activity::ActivityProperties {
            triggered_by_event: Some(true),
            ..Default::default()
        }),
        owner: String::new(),
        contact: String::new(),
    });
    project.validate().unwrap();
    let d = Arc::make_mut(&mut project.documents[0]);
    d.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == selector("event_sub"))
        .unwrap()
        .activity
        .as_mut()
        .unwrap()
        .triggered_by_event = Some(false);
    assert!(project.validate().is_err());
}
fn definition_refs_external(project: &mut ProjectData) {
    let d = Arc::make_mut(&mut project.documents[0]);
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &mut d.processes[0].nodes[0].details
    else {
        unreachable!()
    };
    definition_refs[0] = EventReference::Qualified(QNameRef {
        namespace: "urn:other".into(),
        local: xml("timer"),
    });
}
#[test]
fn di_endpoints_resolve_representation_ids_within_the_same_diagram() {
    let mut p = fixture();
    let d = Arc::make_mut(&mut p.documents[0]);
    d.processes[0].flows.push(Flow {
        id: ident("flow"),
        kind: FlowKind::Sequence,
        source: selector("start"),
        target: selector("throw"),
        name: None,
        documentation: String::new(),
        condition: None,
        association_direction: None,
    });
    let shape = |id: &str, element: &str| Shape {
        id: ident(id),
        element: selector(element),
        is_expanded: None,
        is_horizontal: Some(false),
        is_marker_visible: Some(true),
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
        label: None,
        style: None,
    };
    d.diagrams.push(Diagram {
        id: ident("diagram"),
        name: None,
        plane_id: ident("plane"),
        plane: selector("process"),
        shapes: vec![shape("shape_start", "start"), shape("shape_throw", "throw")],
        edges: vec![Edge {
            id: ident("edge"),
            element: selector("flow"),
            source_element: Some(selector("shape_start")),
            target_element: Some(selector("shape_throw")),
            waypoints: vec![Point { x: 0.0, y: 0.0 }, Point { x: 30.0, y: 30.0 }],
            label: None,
            style: None,
        }],
    });
    p.validate().unwrap();
    let mut invalid = p.clone();
    Arc::make_mut(&mut invalid.documents[0]).diagrams[0].edges[0].source_element =
        Some(selector("start"));
    assert!(invalid.validate().is_err());
    let mut cross = p;
    let d = Arc::make_mut(&mut cross.documents[0]);
    d.diagrams.push(Diagram {
        id: ident("other_diagram"),
        name: None,
        plane_id: ident("other_plane"),
        plane: selector("process"),
        shapes: vec![shape("other_shape", "start")],
        edges: vec![],
    });
    d.diagrams[0].edges[0].source_element = Some(selector("other_shape"));
    assert!(cross.validate().is_err());
}
#[test]
fn annotation_association_may_target_sequence_flow_and_its_di_edge() {
    let mut p = fixture();
    let d = Arc::make_mut(&mut p.documents[0]);
    d.processes[0].nodes.push(Node {
        id: ident("note"),
        name: None,
        documentation: String::new(),
        kind: NodeKind::TextAnnotation,
        details: Some(NodeDetails::TextAnnotation {
            text_format: None,
            text: Some("note".into()),
        }),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    });
    d.processes[0].flows.push(Flow {
        id: ident("sequence"),
        kind: FlowKind::Sequence,
        source: selector("start"),
        target: selector("throw"),
        name: None,
        documentation: String::new(),
        condition: None,
        association_direction: None,
    });
    d.processes[0].flows.push(Flow {
        id: ident("association"),
        kind: FlowKind::Association,
        source: selector("note"),
        target: selector("sequence"),
        name: None,
        documentation: String::new(),
        condition: None,
        association_direction: None,
    });
    let shape = |id: &str, element: &str| Shape {
        id: ident(id),
        element: selector(element),
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
        label: None,
        style: None,
    };
    let edge = |id: &str, element: &str, source: &str, target: &str| Edge {
        id: ident(id),
        element: selector(element),
        source_element: Some(selector(source)),
        target_element: Some(selector(target)),
        waypoints: vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }],
        label: None,
        style: None,
    };
    d.diagrams.push(Diagram {
        id: ident("diagram"),
        name: None,
        plane_id: ident("plane"),
        plane: selector("process"),
        shapes: vec![
            shape("start_shape", "start"),
            shape("throw_shape", "throw"),
            shape("note_shape", "note"),
        ],
        edges: vec![
            edge("sequence_edge", "sequence", "start_shape", "throw_shape"),
            edge(
                "association_edge",
                "association",
                "note_shape",
                "sequence_edge",
            ),
        ],
    });
    p.validate().unwrap();
    let mut invalid = p.clone();
    let d = Arc::make_mut(&mut invalid.documents[0]);
    d.processes[0]
        .flows
        .iter_mut()
        .find(|f| f.id == selector("sequence"))
        .unwrap()
        .kind = FlowKind::Association;
    assert!(invalid.validate().is_err());
    let mut cross_scope = p;
    let d = Arc::make_mut(&mut cross_scope.documents[0]);
    let task = |name: &str| Node {
        id: ident(name),
        name: None,
        documentation: String::new(),
        kind: NodeKind::Task,
        details: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    d.processes.push(Process {
        id: ident("other_process"),
        name: None,
        documentation: String::new(),
        is_executable: None,
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![task("other_a"), task("other_b")],
        lanes: vec![],
        flows: vec![Flow {
            id: ident("other_sequence"),
            kind: FlowKind::Sequence,
            source: selector("other_a"),
            target: selector("other_b"),
            name: None,
            documentation: String::new(),
            condition: None,
            association_direction: None,
        }],
    });
    d.processes[0]
        .flows
        .iter_mut()
        .find(|f| f.id == selector("association"))
        .unwrap()
        .target = selector("other_sequence");
    assert!(cross_scope.validate().is_err());
}
