use processcraft_core::{
    bpmn_collaboration::{Choreography, ChoreographyActivity, ChoreographyFlowElement},
    bpmn_model::EventReference,
    commands::{Envelope, Mutation, Request, Session},
    model::{BpmnDocument, Flow, FlowKind, Participant, ProjectData},
};
use std::sync::Arc;

#[test]
fn scoped_event_data_association_commands_are_atomic_and_undoable() {
    use processcraft_core::{
        bpmn_collaboration::ChoreographyGraph,
        bpmn_data::{DataAssociation, DataAssociationKind, DataPort, IoSet},
        bpmn_model::{EventDefinition, EventIo, EventProperty, NodeDetails},
        model::{Node, NodeKind},
    };
    let event = Node {
        id: "event".parse().unwrap(),
        name: None,
        documentation: "".into(),
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
                id: None,
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
                item_aware: None,
            }],
            io: Some(EventIo::Catch {
                outputs: vec![DataPort {
                    id: Some("port".parse().unwrap()),
                    base: None,
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                output_set: Some(IoSet {
                    id: None,
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
    let mut doc = BpmnDocument::new("Document".into(), "defs".parse().unwrap());
    let document_id = doc.id;
    doc.choreographies.push(Choreography {
        id: "choreo".parse().unwrap(),
        name: None,
        kind: None,
        graph: Some(ChoreographyGraph {
            flow_elements: vec![ChoreographyFlowElement::Event(Box::new(event))],
            data_associations: vec![],
        }),
        extras: None,
        base: None,
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
    });
    let mut project = ProjectData::new("Project".into());
    project.documents.push(Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let envelope = |s: &Session| Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: Some(s.snapshot().generation),
    };
    let before = session.snapshot().content_id;
    let association = DataAssociation {
        id: Some("association".parse().unwrap()),
        base: None,
        owner: "event".parse().unwrap(),
        kind: DataAssociationKind::Output,
        sources: vec!["port".parse().unwrap()],
        target: "property".parse().unwrap(),
        transformation: None,
        assignments: vec![],
    };
    session
        .apply(
            envelope(&session),
            Request::Batch {
                mutations: vec![Mutation::CreateChoreographyDataAssociation {
                    document_id,
                    choreography_id: "choreo".parse().unwrap(),
                    scope_id: "choreo".parse().unwrap(),
                    index: 0,
                    association: association.clone(),
                }],
            },
        )
        .unwrap();
    let created = session.snapshot().content_id;
    let mut anonymous_project = (*session.snapshot().project).clone();
    Arc::make_mut(&mut anonymous_project.documents[0]).choreographies[0]
        .graph
        .as_mut()
        .unwrap()
        .data_associations[0]
        .id = None;
    let mut anonymous = Session::new(anonymous_project).unwrap();
    let anonymous_before = anonymous.snapshot().content_id;
    let change = anonymous
        .apply(
            envelope(&anonymous),
            Request::Batch {
                mutations: vec![Mutation::PromoteElementIdentity {
                    document_id,
                    model_path: "/choreographies/0/graph/data_associations/0/id".into(),
                    xml_id: None,
                }],
            },
        )
        .unwrap();
    assert_eq!(change.promoted_identities.len(), 1);
    anonymous
        .apply(envelope(&anonymous), Request::Undo)
        .unwrap();
    assert_eq!(anonymous.snapshot().content_id, anonymous_before);
    let mut invalid = association;
    invalid.target = "missing".parse().unwrap();
    assert!(
        session
            .apply(
                envelope(&session),
                Request::Batch {
                    mutations: vec![Mutation::UpdateChoreographyDataAssociation {
                        document_id,
                        choreography_id: "choreo".parse().unwrap(),
                        scope_id: "choreo".parse().unwrap(),
                        association: invalid
                    }]
                }
            )
            .is_err()
    );
    assert_eq!(session.snapshot().content_id, created);
    session
        .apply(
            envelope(&session),
            Request::Batch {
                mutations: vec![Mutation::DeleteChoreographyDataAssociation {
                    document_id,
                    choreography_id: "choreo".parse().unwrap(),
                    scope_id: "choreo".parse().unwrap(),
                    association_id: "association".parse().unwrap(),
                }],
            },
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
    session.apply(envelope(&session), Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, created);
}

#[test]
fn artifact_crud_and_reparent_preserve_unambiguous_di() {
    use processcraft_core::{
        bpmn_collaboration::{
            ChoreographyActivityDetails, ChoreographyGraph, CollaborationArtifact,
        },
        bpmn_model::Group,
        model::{Bounds, Diagram, Shape},
    };
    let activity = |name: &str, details| ChoreographyActivity {
        id: name.parse().unwrap(),
        name: None,
        base: None,
        flow_metadata: None,
        correlation_keys: vec![],
        loop_type: None,
        participant_refs: vec![],
        initiating_participant: None,
        message_flow_refs: vec![],
        details,
    };
    let sub = activity(
        "sub",
        Some(ChoreographyActivityDetails::Sub {
            graph: Box::new(ChoreographyGraph::default()),
            artifacts: vec![],
        }),
    );
    let task = activity("task", None);
    let mut document = BpmnDocument::new("Document".into(), "defs".parse().unwrap());
    let document_id = document.id;
    document.choreographies.push(Choreography {
        id: "choreo".parse().unwrap(),
        name: None,
        kind: None,
        graph: Some(ChoreographyGraph {
            flow_elements: vec![
                ChoreographyFlowElement::Activity(Box::new(sub)),
                ChoreographyFlowElement::Activity(Box::new(task)),
            ],
            data_associations: vec![],
        }),
        extras: None,
        base: None,
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
    });
    let shape = |id: &str, element: &str, bounds, is_expanded| Shape {
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        bounds,
        is_expanded,
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        is_horizontal: None,
        is_marker_visible: None,
        label: None,
        label_visibility: None,
        style: None,
    };
    document.diagrams.push(Diagram {
        id: "diagram".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "choreo".parse().unwrap(),
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        edges: vec![],
        shapes: vec![
            shape(
                "sub_shape",
                "sub",
                Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 300.0,
                    height: 200.0,
                },
                Some(true),
            ),
            shape(
                "task_shape",
                "task",
                Bounds {
                    x: 30.0,
                    y: 40.0,
                    width: 100.0,
                    height: 60.0,
                },
                None,
            ),
        ],
    });
    let mut project = ProjectData::new("Project".into());
    project.documents.push(Arc::new(document));
    let mut ambiguous = project.clone();
    let diagram = &mut Arc::make_mut(&mut ambiguous.documents[0]).diagrams[0];
    let mut duplicate = diagram.shapes[0].clone();
    duplicate.id = "duplicate_sub_shape".parse().unwrap();
    diagram.shapes.push(duplicate);
    let mut session = Session::new(project).unwrap();
    let apply = |session: &mut Session, mutation| {
        let snapshot = session.snapshot();
        session.apply(
            Envelope {
                request_id: "test".into(),
                session_id: session.id(),
                expected_generation: Some(snapshot.generation),
            },
            Request::Batch {
                mutations: vec![mutation],
            },
        )
    };
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        Mutation::ReparentChoreographyFlowElement {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            source_scope_id: "choreo".parse().unwrap(),
            target_scope_id: "sub".parse().unwrap(),
            element_id: "task".parse().unwrap(),
            index: 0,
        },
    )
    .unwrap();
    let moved = session.snapshot();
    assert_eq!(
        moved.project.documents[0].diagrams[0].shapes[1].bounds.x,
        30.0
    );
    apply(
        &mut session,
        Mutation::CreateChoreographyArtifact {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            scope_id: "sub".parse().unwrap(),
            index: 0,
            artifact: CollaborationArtifact::Group(Group {
                id: "group".parse().unwrap(),
                base: None,
                category_value_ref: None,
            }),
        },
    )
    .unwrap();
    let with_artifact = session.snapshot().content_id;
    apply(
        &mut session,
        Mutation::ReparentChoreographyArtifact {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            source_scope_id: "sub".parse().unwrap(),
            target_scope_id: "choreo".parse().unwrap(),
            artifact_id: "group".parse().unwrap(),
            index: 0,
        },
    )
    .unwrap();
    session
        .apply(
            Envelope {
                request_id: "undo".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, with_artifact);
    apply(
        &mut session,
        Mutation::UpdateChoreographyArtifact {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            scope_id: "sub".parse().unwrap(),
            artifact: CollaborationArtifact::Group(Group {
                id: "group".parse().unwrap(),
                base: Some(Default::default()),
                category_value_ref: None,
            }),
        },
    )
    .unwrap();
    session
        .apply(
            Envelope {
                request_id: "undo".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, with_artifact);
    apply(
        &mut session,
        Mutation::DeleteChoreographyArtifact {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            scope_id: "sub".parse().unwrap(),
            artifact_id: "group".parse().unwrap(),
        },
    )
    .unwrap();
    assert_eq!(session.snapshot().content_id, moved.content_id);
    session
        .apply(
            Envelope {
                request_id: "undo".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, with_artifact);
    session
        .apply(
            Envelope {
                request_id: "undo".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    session
        .apply(
            Envelope {
                request_id: "undo".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
    let mut ambiguous = Session::new(ambiguous).unwrap();
    let before_ambiguous = ambiguous.snapshot().content_id;
    assert!(
        apply(
            &mut ambiguous,
            Mutation::ReparentChoreographyFlowElement {
                document_id,
                choreography_id: "choreo".parse().unwrap(),
                source_scope_id: "choreo".parse().unwrap(),
                target_scope_id: "sub".parse().unwrap(),
                element_id: "task".parse().unwrap(),
                index: 0
            }
        )
        .is_err()
    );
    assert_eq!(ambiguous.snapshot().content_id, before_ambiguous);
}

#[test]
fn promotion_requires_generation_and_undo_restores_legacy_identity() {
    let mut document = BpmnDocument::new("Document".into(), "defs".parse().unwrap());
    let document_id = document.id;
    document.choreographies.push(Choreography {
        id: "choreo".parse().unwrap(),
        name: None,
        kind: None,
        graph: None,
        extras: None,
        base: None,
        participants: ["participant", "other"]
            .into_iter()
            .map(|name| Participant {
                id: name.parse().unwrap(),
                name: None,
                base: None,
                process: None,
                interface_refs: vec![],
                endpoint_refs: vec![],
                multiplicity: None,
            })
            .collect(),
        message_flows: vec![],
        sequence_flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        activities: vec![ChoreographyActivity {
            id: "activity".parse().unwrap(),
            name: None,
            base: None,
            flow_metadata: None,
            correlation_keys: vec![],
            loop_type: None,
            details: None,
            participant_refs: vec![
                EventReference::Local("participant".parse().unwrap()),
                EventReference::Local("other".parse().unwrap()),
            ],
            initiating_participant: Some(EventReference::Local("participant".parse().unwrap())),
            message_flow_refs: vec![],
        }],
    });
    let mut project = ProjectData::new("Project".into());
    project.documents.push(Arc::new(document));
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot().content_id;
    let request = || Request::Batch {
        mutations: vec![Mutation::PromoteChoreographyGraph {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
        }],
    };
    let envelope = |session: &Session, generation| Envelope {
        request_id: "test".into(),
        session_id: session.id(),
        expected_generation: generation,
    };
    assert!(session.apply(envelope(&session, None), request()).is_err());
    assert_eq!(session.snapshot().content_id, before);
    session
        .apply(envelope(&session, Some(0)), request())
        .unwrap();
    let snapshot = session.snapshot();
    let c = &snapshot.project.documents[0].choreographies[0];
    assert!(c.activities.is_empty());
    assert!(matches!(
        &c.graph.as_ref().unwrap().flow_elements[..],
        [ChoreographyFlowElement::Activity(_)]
    ));
    let promoted = snapshot.content_id;
    session
        .apply(envelope(&session, None), Request::Undo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
    session
        .apply(envelope(&session, None), Request::Redo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, promoted);
    let snapshot = session.snapshot();
    let mut element = snapshot.project.documents[0].choreographies[0]
        .graph
        .as_ref()
        .unwrap()
        .flow_elements[0]
        .clone();
    if let ChoreographyFlowElement::Activity(activity) = &mut element {
        activity.id = "second".parse().unwrap();
    }
    let create = Request::Batch {
        mutations: vec![Mutation::CreateChoreographyFlowElement {
            document_id,
            choreography_id: "choreo".parse().unwrap(),
            scope_id: "choreo".parse().unwrap(),
            index: 0,
            element,
        }],
    };
    assert!(
        session
            .apply(envelope(&session, None), create.clone())
            .is_err()
    );
    session
        .apply(envelope(&session, Some(snapshot.generation)), create)
        .unwrap();
    let created = session.snapshot();
    assert_eq!(
        created.project.documents[0].choreographies[0]
            .graph
            .as_ref()
            .unwrap()
            .flow_elements
            .len(),
        2
    );
    session
        .apply(envelope(&session, None), Request::Undo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, promoted);
    session
        .apply(envelope(&session, None), Request::Redo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, created.content_id);
    let generation = session.snapshot().generation;
    session
        .apply(
            envelope(&session, Some(generation)),
            Request::Batch {
                mutations: vec![Mutation::DeleteChoreographyFlowElement {
                    document_id,
                    choreography_id: "choreo".parse().unwrap(),
                    scope_id: "choreo".parse().unwrap(),
                    element_id: "second".parse().unwrap(),
                }],
            },
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, promoted);
    session
        .apply(envelope(&session, None), Request::Undo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, created.content_id);
    let flow = Flow {
        id: "flow".parse().unwrap(),
        kind: FlowKind::Sequence,
        source: Some(EventReference::Local("activity".parse().unwrap())),
        target: Some(EventReference::Local("second".parse().unwrap())),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        association_direction: None,
        flow_metadata: None,
        condition: None,
        message_ref: None,
    };
    let generation = session.snapshot().generation;
    session
        .apply(
            envelope(&session, Some(generation)),
            Request::Batch {
                mutations: vec![Mutation::CreateChoreographyFlowElement {
                    document_id,
                    choreography_id: "choreo".parse().unwrap(),
                    scope_id: "choreo".parse().unwrap(),
                    index: 2,
                    element: ChoreographyFlowElement::SequenceFlow(Box::new(flow)),
                }],
            },
        )
        .unwrap();
    let connected = session.snapshot();
    // Deleting a connected element cascades to its sequence flows.
    session
        .apply(
            envelope(&session, Some(connected.generation)),
            Request::Batch {
                mutations: vec![Mutation::DeleteChoreographyFlowElement {
                    document_id,
                    choreography_id: "choreo".parse().unwrap(),
                    scope_id: "choreo".parse().unwrap(),
                    element_id: "second".parse().unwrap(),
                }],
            },
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, promoted);
    session
        .apply(envelope(&session, None), Request::Undo)
        .unwrap();
    assert_eq!(session.snapshot().content_id, connected.content_id);
}

mod generic_graph_commands {
    use super::*;
    use processcraft_core::{
        bpmn_collaboration::{ChoreographyActivityDetails, ChoreographyGraph},
        bpmn_data::{DataAssociation, DataAssociationKind, DataPort},
        bpmn_model::{EventDefinition, EventIo, EventProperty, NodeDetails},
        ids::ElementSelector,
        model::{Node, NodeKind},
    };

    fn sel(id: &str) -> ElementSelector {
        id.parse().unwrap()
    }
    fn node(id: &str, kind: NodeKind, details: Option<NodeDetails>) -> Node {
        Node {
            id: id.parse().unwrap(),
            name: None,
            documentation: "".into(),
            base_extras: None,
            kind,
            flow_metadata: None,
            item_aware: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
            details,
        }
    }
    fn data_event(id: &str) -> Node {
        node(
            id,
            NodeKind::StartEvent,
            Some(NodeDetails::Event {
                definitions: vec![EventDefinition::Signal {
                    id: None,
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
                    item_aware: None,
                }],
                io: Some(EventIo::Catch {
                    outputs: vec![DataPort {
                        id: Some("port".parse().unwrap()),
                        base: None,
                        name: None,
                        item_aware: None,
                        is_collection: None,
                    }],
                    output_set: None,
                }),
            }),
        )
    }
    fn activity(id: &str, details: Option<ChoreographyActivityDetails>) -> ChoreographyFlowElement {
        ChoreographyFlowElement::Activity(Box::new(ChoreographyActivity {
            id: id.parse().unwrap(),
            name: None,
            base: None,
            flow_metadata: None,
            correlation_keys: vec![],
            loop_type: None,
            participant_refs: vec![],
            initiating_participant: None,
            message_flow_refs: vec![],
            details,
        }))
    }
    fn flow(id: &str, source: &str, target: &str) -> ChoreographyFlowElement {
        ChoreographyFlowElement::SequenceFlow(Box::new(Flow {
            id: id.parse().unwrap(),
            kind: FlowKind::Sequence,
            source: Some(EventReference::Local(sel(source))),
            target: Some(EventReference::Local(sel(target))),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            association_direction: None,
            flow_metadata: None,
            condition: None,
            message_ref: None,
        }))
    }
    fn association(owner: &str) -> DataAssociation {
        DataAssociation {
            id: Some("assoc".parse().unwrap()),
            base: None,
            owner: sel(owner),
            kind: DataAssociationKind::Output,
            sources: vec![sel("port")],
            target: sel("property"),
            transformation: None,
            assignments: vec![],
        }
    }
    /// Root graph: a, e (with data association), g, b (boundary on a), f (a -> g), sub{ inner }.
    fn fixture() -> (Session, processcraft_core::ids::DocumentId) {
        let sub = activity(
            "sub",
            Some(ChoreographyActivityDetails::Sub {
                graph: Box::new(ChoreographyGraph {
                    flow_elements: vec![activity("inner", None)],
                    data_associations: vec![],
                }),
                artifacts: vec![],
            }),
        );
        let mut doc = BpmnDocument::new("Document".into(), "defs".parse().unwrap());
        let document_id = doc.id;
        doc.choreographies.push(Choreography {
            id: "choreo".parse().unwrap(),
            name: None,
            kind: None,
            graph: Some(ChoreographyGraph {
                flow_elements: vec![
                    activity("a", None),
                    ChoreographyFlowElement::Event(Box::new(data_event("e"))),
                    ChoreographyFlowElement::Gateway(Box::new(node(
                        "g",
                        NodeKind::ExclusiveGateway,
                        None,
                    ))),
                    ChoreographyFlowElement::Event(Box::new(node(
                        "b",
                        NodeKind::BoundaryEvent,
                        Some(NodeDetails::BoundaryEvent {
                            attached_to: EventReference::Local(sel("a")),
                            cancel_activity: true,
                        }),
                    ))),
                    flow("f", "a", "g"),
                    sub,
                ],
                data_associations: vec![association("e")],
            }),
            extras: None,
            base: None,
            participants: vec![],
            message_flows: vec![],
            activities: vec![],
            sequence_flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
        });
        let mut project = ProjectData::new("Project".into());
        project.documents.push(Arc::new(doc));
        (Session::new(project).unwrap(), document_id)
    }
    fn run(s: &mut Session, mutations: Vec<Mutation>) -> Result<(), ()> {
        let expected_generation = Some(s.snapshot().generation);
        s.apply(
            Envelope {
                request_id: "t".into(),
                session_id: s.id(),
                expected_generation,
            },
            Request::Batch { mutations },
        )
        .map(|_| ())
        .map_err(|_| ())
    }
    fn history(s: &mut Session, request: Request) {
        s.apply(
            Envelope {
                request_id: "h".into(),
                session_id: s.id(),
                expected_generation: None,
            },
            request,
        )
        .unwrap();
    }
    fn walk(graph: &ChoreographyGraph, out: &mut Vec<(String, Option<String>)>) {
        for e in &graph.flow_elements {
            match e {
                ChoreographyFlowElement::Activity(a) => {
                    out.push((a.id.selector().to_string(), a.name.clone()));
                    if let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &a.details {
                        walk(graph, out);
                    }
                }
                ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                    out.push((n.id.selector().to_string(), n.name.clone()))
                }
                ChoreographyFlowElement::SequenceFlow(f) => {
                    out.push((f.id.selector().to_string(), f.name.clone()))
                }
            }
        }
    }
    fn elements(s: &Session) -> Vec<(String, Option<String>)> {
        let snapshot = s.snapshot();
        let mut out = vec![];
        walk(
            snapshot.project.documents[0].choreographies[0]
                .graph
                .as_ref()
                .unwrap(),
            &mut out,
        );
        out
    }
    fn root_associations(s: &Session) -> usize {
        s.snapshot().project.documents[0].choreographies[0]
            .graph
            .as_ref()
            .unwrap()
            .data_associations
            .len()
    }

    #[test]
    fn generic_rename_reaches_graph_elements_with_undo_redo_and_atomicity() {
        for id in ["a", "e", "g", "f", "inner", "sub"] {
            let (mut s, document_id) = fixture();
            let before = s.snapshot().content_id;
            run(
                &mut s,
                vec![Mutation::Rename {
                    document_id,
                    element_id: sel(id),
                    name: Some("X".into()),
                }],
            )
            .unwrap();
            let renamed = s.snapshot().content_id;
            assert_ne!(renamed, before);
            let names = elements(&s);
            assert_eq!(names.iter().filter(|(_, n)| n.is_some()).count(), 1, "{id}");
            assert!(
                names
                    .iter()
                    .any(|(i, n)| i == id && n.as_deref() == Some("X"))
            );
            history(&mut s, Request::Undo);
            assert_eq!(s.snapshot().content_id, before);
            history(&mut s, Request::Redo);
            assert_eq!(s.snapshot().content_id, renamed);
        }
        let (mut s, document_id) = fixture();
        let (before, generation) = (s.snapshot().content_id, s.snapshot().generation);
        assert!(
            run(
                &mut s,
                vec![
                    Mutation::Rename {
                        document_id,
                        element_id: sel("a"),
                        name: Some("X".into()),
                    },
                    Mutation::Rename {
                        document_id,
                        element_id: sel("missing"),
                        name: None,
                    },
                ],
            )
            .is_err()
        );
        assert_eq!(s.snapshot().content_id, before);
        assert_eq!(s.snapshot().generation, generation);
    }

    #[test]
    fn generic_delete_cascades_in_graph_with_undo_redo_and_atomicity() {
        // (deleted id, ids that must disappear with it)
        let cases: [(&str, &[&str]); 7] = [
            ("f", &["f"]),
            ("g", &["g", "f"]),
            ("e", &["e"]),
            ("a", &["a", "b", "f"]),
            ("b", &["b"]),
            ("inner", &["inner"]),
            ("sub", &["sub", "inner"]),
        ];
        for (id, gone) in cases {
            let (mut s, document_id) = fixture();
            let before = s.snapshot().content_id;
            let all = elements(&s);
            run(
                &mut s,
                vec![Mutation::Delete {
                    document_id,
                    element_id: sel(id),
                }],
            )
            .unwrap();
            let after = s.snapshot().content_id;
            let left = elements(&s);
            assert_eq!(left.len(), all.len() - gone.len(), "{id}");
            assert!(
                left.iter().all(|(i, _)| !gone.contains(&i.as_str())),
                "{id}"
            );
            assert_eq!(root_associations(&s), usize::from(id != "e"), "{id}");
            history(&mut s, Request::Undo);
            assert_eq!(s.snapshot().content_id, before, "{id}");
            history(&mut s, Request::Redo);
            assert_eq!(s.snapshot().content_id, after, "{id}");
        }
        let (mut s, document_id) = fixture();
        let (before, generation) = (s.snapshot().content_id, s.snapshot().generation);
        assert!(
            run(
                &mut s,
                vec![
                    Mutation::Delete {
                        document_id,
                        element_id: sel("a"),
                    },
                    Mutation::Delete {
                        document_id,
                        element_id: sel("missing"),
                    },
                ],
            )
            .is_err()
        );
        assert_eq!(s.snapshot().content_id, before);
        assert_eq!(s.snapshot().generation, generation);
    }

    #[test]
    fn scoped_delete_of_event_with_associations_cascades() {
        let (mut s, document_id) = fixture();
        let before = s.snapshot().content_id;
        run(
            &mut s,
            vec![Mutation::DeleteChoreographyFlowElement {
                document_id,
                choreography_id: sel("choreo"),
                scope_id: sel("choreo"),
                element_id: sel("e"),
            }],
        )
        .unwrap();
        assert_eq!(root_associations(&s), 0);
        history(&mut s, Request::Undo);
        assert_eq!(s.snapshot().content_id, before);
    }

    #[test]
    fn reparent_moves_event_data_associations_with_the_event() {
        let (mut s, document_id) = fixture();
        let before = s.snapshot().content_id;
        let reparent = |index| Mutation::ReparentChoreographyFlowElement {
            document_id,
            choreography_id: sel("choreo"),
            source_scope_id: sel("choreo"),
            target_scope_id: sel("sub"),
            element_id: sel("e"),
            index,
        };
        // Atomic: an out-of-range index leaves the project untouched.
        assert!(run(&mut s, vec![reparent(99)]).is_err());
        assert_eq!(s.snapshot().content_id, before);
        run(&mut s, vec![reparent(0)]).unwrap();
        let moved = s.snapshot().content_id;
        assert_eq!(root_associations(&s), 0);
        {
            let snapshot = s.snapshot();
            let graph = snapshot.project.documents[0].choreographies[0]
                .graph
                .as_ref()
                .unwrap();
            let sub = graph
                .flow_elements
                .iter()
                .find_map(|e| match e {
                    ChoreographyFlowElement::Activity(a) => match &a.details {
                        Some(ChoreographyActivityDetails::Sub { graph, .. }) => Some(graph),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            assert_eq!(sub.data_associations.len(), 1);
            assert!(matches!(
                &sub.flow_elements[0],
                ChoreographyFlowElement::Event(_)
            ));
        }
        history(&mut s, Request::Undo);
        assert_eq!(s.snapshot().content_id, before);
        history(&mut s, Request::Redo);
        assert_eq!(s.snapshot().content_id, moved);
    }
}
