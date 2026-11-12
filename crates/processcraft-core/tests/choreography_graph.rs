use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyFlowElement,
        ChoreographyGraph, ChoreographyLoopType, ChoreographyRootKind,
    },
    ids::ElementSelector,
    model::BpmnDocument,
    queries::choreography_scopes,
};

fn activity(id: &str) -> ChoreographyActivity {
    ChoreographyActivity {
        id: id.parse().unwrap(),
        name: None,
        base: None,
        flow_metadata: None,
        correlation_keys: vec![],
        loop_type: None,
        details: None,
        participant_refs: vec![ModelReference::Local("buyer".parse().unwrap())],
        initiating_participant: Some(ModelReference::Local("buyer".parse().unwrap())),
        message_flow_refs: vec![],
    }
}
fn root() -> Choreography {
    Choreography {
        id: "root".parse().unwrap(),
        name: None,
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        participants: vec![],
        message_flows: vec![],
        activities: vec![activity("child")],
        sequence_flows: vec![],
    }
}
#[test]
fn legacy_wire_and_explicit_defaults_are_distinct() {
    let mut a = activity("child");
    let value = serde_json::to_value(&a).unwrap();
    let selector: ElementSelector = "buyer".parse().unwrap();
    assert_eq!(
        value["initiating_participant"],
        serde_json::to_value(selector).unwrap()
    );
    assert!(value.get("loop_type").is_none());
    assert!(value.get("details").is_none());
    a.loop_type = Some(ChoreographyLoopType::None);
    assert_ne!(serde_json::to_value(a).unwrap(), value);
    let mut d = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    d.choreographies.push(root());
    assert!(!d.has_expanded_collaboration());
    d.choreographies[0].kind = Some(ChoreographyRootKind::Global {
        initiating_participant_ref: None,
    });
    assert!(d.has_expanded_collaboration());
}
#[test]
fn scope_adapter_keeps_legacy_and_nested_owners() {
    let mut d = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    d.choreographies.push(root());
    let scopes = choreography_scopes(&d);
    assert_eq!(scopes.len(), 1);
    assert_eq!(
        scopes[0].activities[0].id.selector(),
        "child".parse::<ElementSelector>().unwrap()
    );
    let mut child = activity("child");
    child.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(activity(
                "nested",
            )))],
        }),
        artifacts: vec![],
    });
    let root = &mut d.choreographies[0];
    root.activities.clear();
    root.graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(child))],
    });
    let scopes = choreography_scopes(&d);
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes[1].parent, Some(scopes[0].owner.clone()));
    assert_eq!(
        scopes[1].activities[0].id.selector(),
        "nested".parse::<ElementSelector>().unwrap()
    );
}

#[test]
fn graph_event_child_identities_are_indexed_and_unique() {
    use processcraft_core::{
        bpmn_data::{
            DataAssociation, DataAssociationKind, DataPort, DataState, IoSet, ItemAwareMetadata,
        },
        bpmn_model::{EventDefinition, EventIo, EventProperty, NodeDetails},
        model::{Node, NodeKind, ProjectData},
        reference_resolution::{ReferenceIndex, ReferenceResolution},
    };
    use std::sync::Arc;
    let metadata = |id: &str| {
        Some(ItemAwareMetadata {
            item_subject_ref: None,
            data_state: Some(DataState {
                base: None,
                id: Some(id.parse().unwrap()),
                name: None,
            }),
        })
    };
    let event = Node {
        id: "event".parse().unwrap(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::StartEvent,
        flow_metadata: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
        details: Some(NodeDetails::Event {
            definitions: vec![EventDefinition::Signal {
                id: Some("definition".parse().unwrap()),
                base: None,
                signal_ref: None,
            }],
            attached_to: None,
            cancel_activity: None,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![EventProperty {
                id: Some("property".parse().unwrap()),
                base: None,
                name: None,
                item_aware: metadata("property_state"),
            }],
            io: Some(EventIo::Catch {
                outputs: vec![DataPort {
                    id: Some("port".parse().unwrap()),
                    base: None,
                    name: None,
                    item_aware: metadata("port_state"),
                    is_collection: None,
                }],
                output_set: Some(IoSet {
                    id: Some("set".parse().unwrap()),
                    base: None,
                    name: None,
                    data_refs: vec!["port".parse().unwrap()],
                    optional_refs: vec![],
                    while_executing_refs: vec![],
                    counterpart_refs: vec![],
                }),
            }),
        }),
    };
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    let mut choreography = root();
    choreography.activities.clear();
    choreography.graph = Some(ChoreographyGraph {
        data_associations: vec![DataAssociation {
            id: Some("association".parse().unwrap()),
            base: None,
            owner: "event".parse().unwrap(),
            kind: DataAssociationKind::Output,
            sources: vec!["port".parse().unwrap()],
            target: "property".parse().unwrap(),
            transformation: None,
            assignments: vec![],
        }],
        flow_elements: vec![ChoreographyFlowElement::Event(Box::new(event))],
    });
    doc.choreographies.push(choreography);
    let index = ReferenceIndex::new(&doc);
    for name in [
        "event",
        "association",
        "definition",
        "property",
        "property_state",
        "port",
        "port_state",
        "set",
    ] {
        let selector: ElementSelector = name.parse().unwrap();
        assert_eq!(
            index.resolve(&ModelReference::Local(selector.clone())),
            ReferenceResolution::Resolved(selector)
        );
    }
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
    let document = Arc::make_mut(&mut project.documents[0]);
    let Some(ChoreographyGraph { flow_elements, .. }) = &mut document.choreographies[0].graph
    else {
        unreachable!()
    };
    let ChoreographyFlowElement::Event(node) = &mut flow_elements[0] else {
        unreachable!()
    };
    let Some(NodeDetails::Event { properties, .. }) = &mut node.details else {
        unreachable!()
    };
    properties[0].id = Some("event".parse().unwrap());
    assert!(project.validate().is_err());
}

#[test]
fn nested_annotation_markup_is_opaque() {
    use processcraft_core::{
        bpmn_collaboration::CollaborationArtifact,
        bpmn_model::NodeDetails,
        bpmn_xml::{MixedXml, XmlContent, XmlNode},
        model::{Node, NodeKind},
    };
    let annotation = Node {
        id: "annotation".parse().unwrap(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::TextAnnotation,
        flow_metadata: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
        details: Some(NodeDetails::TextAnnotation {
            text_format: None,
            text: Some(MixedXml::Structured(XmlContent {
                namespaces: vec![],
                nodes: vec![XmlNode::ProcessingInstruction {
                    target: "marker".into(),
                    data: None,
                }],
            })),
        }),
    };
    let mut child = activity("child");
    child.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph::default()),
        artifacts: vec![CollaborationArtifact::TextAnnotation(Box::new(annotation))],
    });
    let mut choreography = root();
    choreography.activities.clear();
    choreography.graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(child))],
    });
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.choreographies.push(choreography);
    assert!(doc.has_opaque_xml());
}

#[test]
fn boundary_attachment_local_wire_and_external_namespace_are_preserved() {
    use processcraft_core::bpmn_model::{NodeDetails, QNameRef};
    let selector: ElementSelector = "child".parse().unwrap();
    let local = NodeDetails::BoundaryEvent {
        attached_to: ModelReference::Local(selector.clone()),
        cancel_activity: true,
    };
    let json = serde_json::to_value(&local).unwrap();
    assert_eq!(json["attached_to"], serde_json::to_value(selector).unwrap());
    let external = NodeDetails::BoundaryEvent {
        attached_to: ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "child".parse().unwrap(),
        }),
        cancel_activity: true,
    };
    let roundtrip: NodeDetails =
        serde_json::from_value(serde_json::to_value(&external).unwrap()).unwrap();
    assert_eq!(roundtrip, external);
}

#[test]
fn graph_boundary_external_qname_does_not_alias_local_activity() {
    use processcraft_core::{
        bpmn_model::{NodeDetails, QNameRef},
        model::{Node, NodeKind, ProjectData},
    };
    use std::sync::Arc;
    let boundary = Node {
        id: "boundary".parse().unwrap(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::BoundaryEvent,
        flow_metadata: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
        details: Some(NodeDetails::Event {
            definitions: vec![],
            attached_to: Some(ModelReference::Qualified(QNameRef {
                namespace: "urn:external".into(),
                local: "child".parse().unwrap(),
            })),
            cancel_activity: None,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        }),
    };
    let mut choreography = root();
    choreography.activities.clear();
    choreography.graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![
            ChoreographyFlowElement::Activity(Box::new(activity("child"))),
            ChoreographyFlowElement::Event(Box::new(boundary)),
        ],
    });
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.choreographies.push(choreography);
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
    let scope = choreography_scopes(&project.documents[0]);
    let Some(NodeDetails::Event {
        attached_to: Some(reference),
        ..
    }) = &scope[0].events[0].details
    else {
        unreachable!()
    };
    assert!(reference.is_external(Some(processcraft_core::model::GENERATED_TARGET_NAMESPACE)));
    assert!(!reference.matches_identity(
        &scope[0].activities[0].id,
        Some(processcraft_core::model::GENERATED_TARGET_NAMESPACE)
    ));
}

#[test]
fn combined_graph_and_mixed_xml_obey_reader_depth_budget() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_model::NodeDetails,
        bpmn_xml::{MixedXml, XmlContent, XmlElement, XmlNode},
        model::{Node, NodeKind, ProjectData},
    };
    use std::sync::Arc;
    let mut body = XmlNode::Text {
        text: "leaf".into(),
    };
    for _ in 0..24 {
        body = XmlNode::Element {
            element: XmlElement {
                namespace: String::new(),
                local: "part".parse().unwrap(),
                prefix: None,
                namespaces: vec![],
                attributes: vec![],
                children: vec![body],
            },
        };
    }
    let event = Node {
        id: "deep_event".parse().unwrap(),
        name: None,
        documentation: DocumentationSet::Structured(vec![Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Structured(XmlContent {
                namespaces: vec![],
                nodes: vec![body],
            }),
        }]),
        base_extras: None,
        kind: NodeKind::StartEvent,
        details: Some(NodeDetails::Event {
            definitions: vec![],
            attached_to: None,
            cancel_activity: None,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        }),
        flow_metadata: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let mut graph = ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![ChoreographyFlowElement::Event(Box::new(event))],
    };
    for i in 0..16 {
        let mut a = activity(&format!("scope_{i}"));
        a.details = Some(ChoreographyActivityDetails::Sub {
            graph: Box::new(graph),
            artifacts: vec![],
        });
        graph = ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(a))],
        };
    }
    let mut choreography = root();
    choreography.activities.clear();
    choreography.graph = Some(graph);
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    doc.choreographies.push(choreography);
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(doc));
    let error = project.validate().unwrap_err();
    assert!(error.contains("project JSON is not readable"), "{error}");
}

#[test]
fn sub_scope_diagram_accepts_descendants_and_real_participant_bands() {
    use processcraft_core::{
        bpmn_collaboration::ChoreographyActivityDetails,
        bpmn_model::{NodeDetails, QNameRef},
        commands::{Envelope, Mutation, Request, Session},
        model::{
            Bounds, DiReference, Diagram, Edge, Flow, FlowKind, Node, NodeKind, Participant, Point,
            ProjectData, Shape,
        },
    };
    use std::sync::Arc;
    let sub = |id: &str, elements| {
        let mut a = activity(id);
        a.details = Some(ChoreographyActivityDetails::Sub {
            graph: Box::new(ChoreographyGraph {
                flow_elements: elements,
                data_associations: vec![],
            }),
            artifacts: vec![],
        });
        a
    };
    let mut task = activity("nested_task");
    task.participant_refs = vec![ModelReference::Local("buyer".parse().unwrap())];
    task.message_flow_refs = vec![
        ModelReference::Local("message".parse().unwrap()),
        ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "other_message".parse().unwrap(),
        }),
    ];
    let inner = sub(
        "inner",
        vec![ChoreographyFlowElement::Activity(Box::new(task))],
    );
    let outer = sub(
        "outer",
        vec![ChoreographyFlowElement::Activity(Box::new(inner))],
    );
    let sibling = sub(
        "sibling",
        vec![ChoreographyFlowElement::Activity(Box::new(activity(
            "sibling_task",
        )))],
    );
    let mut call = activity("call");
    call.details = Some(ChoreographyActivityDetails::Call {
        participant_associations: vec![],
        called_choreography_ref: None,
    });
    let mut choreography = root();
    choreography.activities.clear();
    choreography.participants = ["buyer", "seller"]
        .iter()
        .map(|id| Participant {
            id: id.parse().unwrap(),
            name: None,
            process: None,
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            base: None,
        })
        .collect();
    choreography.message_flows = ["message", "other_message"]
        .iter()
        .map(|id| Flow {
            id: id.parse().unwrap(),
            kind: FlowKind::Message,
            source: Some(ModelReference::Local("buyer".parse().unwrap())),
            target: Some(ModelReference::Local("seller".parse().unwrap())),
            name: None,
            documentation: "".into(),
            base_extras: None,
            association_direction: None,
            flow_metadata: None,
            condition: None,
            message_ref: None,
        })
        .collect();
    choreography.graph = Some(ChoreographyGraph {
        data_associations: vec![],
        flow_elements: vec![
            ChoreographyFlowElement::Activity(Box::new(outer)),
            ChoreographyFlowElement::Activity(Box::new(sibling)),
            ChoreographyFlowElement::Activity(Box::new(activity("plain_task"))),
            ChoreographyFlowElement::Activity(Box::new(call)),
        ],
    });
    let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
    let document_id = doc.id;
    doc.choreographies.push(choreography);
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let apply = |session: &mut Session, request| {
        session.apply(
            Envelope {
                request_id: "scope-diagram".into(),
                session_id: session.id(),
                expected_generation: Some(session.snapshot().generation),
            },
            request,
        )
    };
    let diagram = |id: &str, plane: &str| Diagram {
        id: id.parse().unwrap(),
        plane_id: format!("{id}_plane").parse().unwrap(),
        plane: plane.parse().unwrap(),
        shapes: vec![],
        edges: vec![],
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
    };
    let shape = |id: &str, element: &str| Shape {
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        },
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        label: None,
        label_visibility: None,
        style: None,
    };
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::CreateDiagram {
                document_id,
                diagram: diagram("sub_diagram", "outer"),
            }],
        },
    )
    .unwrap();
    let diagram_only = session.snapshot().content_id;
    let event = Node {
        id: "event".parse().unwrap(),
        name: None,
        documentation: "".into(),
        base_extras: None,
        kind: NodeKind::StartEvent,
        details: Some(NodeDetails::Event {
            definitions: vec![],
            attached_to: None,
            cancel_activity: None,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        }),
        flow_metadata: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![
                Mutation::CreateChoreographyFlowElement {
                    document_id,
                    choreography_id: "root".parse().unwrap(),
                    scope_id: "outer".parse().unwrap(),
                    index: 1,
                    element: ChoreographyFlowElement::Event(Box::new(event)),
                },
                Mutation::CreateShape {
                    document_id,
                    diagram_id: "sub_diagram".parse().unwrap(),
                    shape: shape("event_shape", "event"),
                },
                Mutation::CreateShape {
                    document_id,
                    diagram_id: "sub_diagram".parse().unwrap(),
                    shape: shape("task_shape", "nested_task"),
                },
            ],
        },
    )
    .unwrap();
    let mut band = shape("buyer_band", "buyer");
    band.choreography_activity_shape = Some(DiReference::Local("task_shape".parse().unwrap()));
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::CreateShape {
                document_id,
                diagram_id: "sub_diagram".parse().unwrap(),
                shape: band.clone(),
            }],
        },
    )
    .unwrap();
    let edge = |id: &str, element: &str| Edge {
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        waypoints: vec![Point { x: 0.0, y: 0.0 }, Point { x: 50.0, y: 50.0 }],
        di_metadata: None,
        message_visible_kind: None,
        source_element: None,
        target_element: None,
        label: None,
        label_visibility: None,
        style: None,
    };
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::CreateEdge {
                document_id,
                diagram_id: "sub_diagram".parse().unwrap(),
                edge: edge("message_edge", "message"),
            }],
        },
    )
    .unwrap();
    let after = session.snapshot().content_id;
    assert!(
        apply(
            &mut session,
            Request::Batch {
                mutations: vec![Mutation::CreateEdge {
                    document_id,
                    diagram_id: "sub_diagram".parse().unwrap(),
                    edge: edge("foreign_collision", "other_message")
                }]
            }
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, after);
    let mut wrong_band = band;
    wrong_band.id = "seller_band".parse().unwrap();
    wrong_band.element = "seller".parse().unwrap();
    for candidate in [
        shape("sibling_shape", "sibling_task"),
        shape("participant_shape", "buyer"),
        wrong_band,
    ] {
        assert!(
            apply(
                &mut session,
                Request::Batch {
                    mutations: vec![Mutation::CreateShape {
                        document_id,
                        diagram_id: "sub_diagram".parse().unwrap(),
                        shape: candidate
                    }]
                }
            )
            .is_err()
        );
        assert_eq!(session.snapshot().content_id, after);
    }
    for plane in ["plain_task", "nested_task", "call"] {
        assert!(
            apply(
                &mut session,
                Request::Batch {
                    mutations: vec![Mutation::CreateDiagram {
                        document_id,
                        diagram: diagram("bad_diagram", plane)
                    }]
                }
            )
            .is_err()
        );
    }
    apply(&mut session, Request::Undo).unwrap();
    apply(&mut session, Request::Undo).unwrap();
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, diagram_only);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    apply(&mut session, Request::Redo).unwrap();
    apply(&mut session, Request::Redo).unwrap();
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
}

#[test]
fn normal_and_global_choreography_roots_are_valid_diagram_planes() {
    use processcraft_core::{
        commands::{Envelope, Mutation, Request, Session},
        model::{Diagram, ProjectData},
    };
    use std::sync::Arc;
    for kind in [
        None,
        Some(ChoreographyRootKind::Global {
            initiating_participant_ref: None,
        }),
    ] {
        let mut choreography = root();
        choreography.kind = kind;
        choreography.activities.clear();
        choreography.graph = Some(ChoreographyGraph::default());
        let mut doc = BpmnDocument::new("doc".into(), "defs".parse().unwrap());
        let document_id = doc.id;
        doc.choreographies.push(choreography);
        let mut project = ProjectData::new("project".into());
        project.documents.push(Arc::new(doc));
        let mut session = Session::new(project).unwrap();
        let before = session.snapshot().content_id;
        let apply = |session: &mut Session, request| {
            session.apply(
                Envelope {
                    request_id: "root-plane".into(),
                    session_id: session.id(),
                    expected_generation: Some(session.snapshot().generation),
                },
                request,
            )
        };
        apply(
            &mut session,
            Request::Batch {
                mutations: vec![Mutation::CreateDiagram {
                    document_id,
                    diagram: Diagram {
                        id: "diagram".parse().unwrap(),
                        plane_id: "plane".parse().unwrap(),
                        plane: "root".parse().unwrap(),
                        shapes: vec![],
                        edges: vec![],
                        plane_metadata: None,
                        plane_order: None,
                        name: None,
                        documentation: None,
                        resolution: None,
                        label_styles: vec![],
                    },
                }],
            },
        )
        .unwrap();
        let after = session.snapshot().content_id;
        apply(&mut session, Request::Undo).unwrap();
        assert_eq!(session.snapshot().content_id, before);
        apply(&mut session, Request::Redo).unwrap();
        assert_eq!(session.snapshot().content_id, after);
    }
}
