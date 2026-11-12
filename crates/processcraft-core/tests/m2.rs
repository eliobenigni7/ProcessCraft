use processcraft_core::{
    commands::{DictionaryDeleteResolution, Envelope, Mutation, Request, Session},
    ids::{DictionaryEntryId, ElementKey},
    model::*,
    queries::*,
};
use std::sync::Arc;
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn apply(
    s: &mut Session,
    m: Vec<Mutation>,
) -> Result<(), processcraft_core::commands::CommandError> {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Batch { mutations: m },
    )
    .map(|_| ())
}
fn fixture() -> (Session, ElementKey) {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            id: id("activity"),
            name: Some("A".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![Lane {
            base: None,
            child_lane_set_id: None,
            child_lane_set_base: None,
            id: id("lane"),
            name: Some("L".into()),
            parent: None,
            members: vec![id("activity")],
        }],
        flows: vec![],
    });
    let key = ElementKey {
        document_id: d.id,
        xml_id: id("activity"),
    };
    p.documents.push(Arc::new(d));
    (Session::new(p).unwrap(), key)
}
#[test]
fn folder_cycle_rolls_back_and_search_returns_stable_ids() {
    let (mut s, key) = fixture();
    apply(
        &mut s,
        vec![
            Mutation::CreateFolder {
                folder: Folder {
                    id: id("a"),
                    name: "A".into(),
                    parent: None,
                },
            },
            Mutation::CreateFolder {
                folder: Folder {
                    id: id("b"),
                    name: "B".into(),
                    parent: Some(id("a")),
                },
            },
        ],
    )
    .unwrap();
    let before = s.snapshot();
    assert!(
        apply(
            &mut s,
            vec![Mutation::MoveFolder {
                folder_id: id("a"),
                parent: Some(id("b"))
            }]
        )
        .is_err()
    );
    assert_eq!(before.content_id, s.snapshot().content_id);
    assert_eq!(
        library_search(&s.snapshot().project, "DOC"),
        vec![key.document_id]
    );
    assert!(apply(&mut s, vec![Mutation::DeleteFolder { folder_id: id("a") }]).is_err());
}
#[test]
fn dictionary_names_are_live_delete_unlinks_and_undo_restores() {
    let (mut s, key) = fixture();
    let eid = DictionaryEntryId::new();
    let entry = DictionaryEntry {
        id: eid,
        kind: DictionaryKind::System,
        name: "old".into(),
        description: String::new(),
    };
    apply(
        &mut s,
        vec![
            Mutation::CreateDictionaryEntry {
                entry: entry.clone(),
            },
            Mutation::SetDictionaryLinks {
                source: key.clone(),
                kind: DictionaryKind::System,
                targets: vec![DictionaryTarget::Entry(eid)],
            },
        ],
    )
    .unwrap();
    let mut renamed = entry;
    renamed.name = "new".into();
    apply(
        &mut s,
        vec![Mutation::UpdateDictionaryEntry { entry: renamed }],
    )
    .unwrap();
    assert_eq!(
        activity_table(&s.snapshot().project, key.document_id, &id("process")).unwrap()[0].systems,
        vec!["new"]
    );
    assert_eq!(
        dictionary_usages(&s.snapshot().project, eid),
        vec![key.clone()]
    );
    assert!(
        apply(
            &mut s,
            vec![Mutation::DeleteDictionaryEntry {
                entry_id: eid,
                resolution: DictionaryDeleteResolution::Reject
            }]
        )
        .is_err()
    );
    apply(
        &mut s,
        vec![Mutation::DeleteDictionaryEntry {
            entry_id: eid,
            resolution: DictionaryDeleteResolution::Unlink,
        }],
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.dictionary_links[0].target,
        DictionaryTarget::Literal("new".into())
    );
    let env = Envelope {
        request_id: "undo".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, Request::Undo).unwrap();
    assert_eq!(
        s.snapshot().project.dictionary_links[0].target,
        DictionaryTarget::Entry(eid)
    );
}

#[test]
fn role_choices_require_explicit_selection_when_ambiguous() {
    let (mut s, key) = fixture();
    let rid = DictionaryEntryId::new();
    let entry = DictionaryEntry {
        id: rid,
        kind: DictionaryKind::Role,
        name: "Role".into(),
        description: String::new(),
    };
    let lane = ElementKey {
        document_id: key.document_id,
        xml_id: id("lane"),
    };
    apply(
        &mut s,
        vec![
            Mutation::CreateDictionaryEntry { entry },
            Mutation::SetDictionaryLinks {
                source: lane,
                kind: DictionaryKind::Role,
                targets: vec![DictionaryTarget::Entry(rid)],
            },
            Mutation::CreateLane {
                document_id: key.document_id,
                process_id: id("process"),
                lane: Lane {
                    base: None,
                    child_lane_set_id: None,
                    child_lane_set_base: None,
                    id: id("lane2"),
                    name: Some("L2".into()),
                    parent: None,
                    members: vec![],
                },
            },
            Mutation::SetDictionaryLinks {
                source: ElementKey {
                    document_id: key.document_id,
                    xml_id: id("lane2"),
                },
                kind: DictionaryKind::Role,
                targets: vec![DictionaryTarget::Entry(rid)],
            },
        ],
    )
    .unwrap();
    assert_eq!(
        role_lane_choices(&s.snapshot().project, key.document_id, &id("process"), rid),
        Some(RoleLaneChoices::Choose(vec![id("lane"), id("lane2")]))
    );
}
#[test]
fn request_schema_covers_all_discovered_tags() {
    let schema = processcraft_core::commands::request_schema();
    let mut tags = Vec::new();
    fn visit(v: &serde_json::Value, tags: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(t) = o
                    .get("type")
                    .and_then(|v| v.get("const"))
                    .and_then(|v| v.as_str())
                {
                    tags.push(t.to_owned());
                }
                for x in o.values() {
                    visit(x, tags)
                }
            }
            serde_json::Value::Array(a) => {
                for x in a {
                    visit(x, tags)
                }
            }
            _ => {}
        }
    }
    visit(&schema, &mut tags);
    for d in processcraft_core::commands::command_descriptors() {
        assert!(
            tags.iter().any(|s| s == d.serde_tag),
            "missing {}",
            d.serde_tag
        );
    }
}

#[test]
fn splice_is_one_undo_and_rejects_branches() {
    let (s, key) = fixture();
    let mut p = (*s.snapshot().project).clone();
    let d = Arc::make_mut(&mut p.documents[0]);
    let pr = &mut d.processes[0];
    let node = |name: &str| Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        id: id(name),
        name: Some(name.into()),
        documentation: String::new().into(),
        kind: NodeKind::Task,
        owner: String::new(),
        contact: String::new(),
    };
    pr.nodes.push(node("target"));
    let flow = |name: &str, source: &str, target: &str| Flow {
        message_ref: None,
        flow_metadata: None,
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
    pr.flows.push(flow("old", "activity", "target"));
    let pt = |x: f64| Point { x, y: 0. };
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: None,
            label_visibility: None,
            source_element: None,
            target_element: None,
            id: id("oldedge"),
            element: id("old"),
            waypoints: vec![pt(0.), pt(10.)],
            label: None,
            style: None,
        }],
    });
    let splice = || Mutation::SpliceActivity {
        document_id: key.document_id,
        process_id: id("process"),
        flow_id: id("old"),
        node: node("inserted"),
        new_flow: flow("newflow", "inserted", "target"),
        diagram_id: id("diagram"),
        shape: Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id("shape"),
            element: id("inserted"),
            bounds: Bounds {
                x: 2.,
                y: 0.,
                width: 2.,
                height: 2.,
            },
            label: None,
            style: None,
        },
        old_waypoints: vec![pt(0.), pt(2.)],
        new_edge: Edge {
            di_metadata: None,
            message_visible_kind: None,
            label_visibility: None,
            source_element: None,
            target_element: None,
            id: id("newedge"),
            element: id("newflow"),
            waypoints: vec![pt(4.), pt(10.)],
            label: None,
            style: None,
        },
    };
    let mut branch = p.clone();
    let pr = &mut Arc::make_mut(&mut branch.documents[0]).processes[0];
    pr.nodes.push(node("other"));
    pr.flows.push(flow("branch", "activity", "other"));
    let mut branch = Session::new(branch).unwrap();
    let before = branch.snapshot().content_id;
    assert!(apply(&mut branch, vec![splice()]).is_err());
    assert_eq!(before, branch.snapshot().content_id);
    let mut session = Session::new(p).unwrap();
    let before = session.snapshot().content_id;
    apply(&mut session, vec![splice()]).unwrap();
    assert_ne!(before, session.snapshot().content_id);
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
    assert_eq!(before, session.snapshot().content_id);
}

#[test]
fn deletion_coalesces_existing_replacement_or_literal_and_undo_restores_identity() {
    for unlink in [false, true] {
        let (mut s, key) = fixture();
        let a = DictionaryEntryId::new();
        let b = DictionaryEntryId::new();
        let entry = |id, name: &str| DictionaryEntry {
            id,
            kind: DictionaryKind::System,
            name: name.into(),
            description: String::new(),
        };
        let existing = if unlink {
            DictionaryTarget::Literal("A".into())
        } else {
            DictionaryTarget::Entry(b)
        };
        apply(
            &mut s,
            vec![
                Mutation::CreateDictionaryEntry {
                    entry: entry(a, "A"),
                },
                Mutation::CreateDictionaryEntry {
                    entry: entry(b, "B"),
                },
                Mutation::SetDictionaryLinks {
                    source: key,
                    kind: DictionaryKind::System,
                    targets: vec![DictionaryTarget::Entry(a), existing.clone()],
                },
            ],
        )
        .unwrap();
        let before = s.snapshot().content_id;
        let resolution = if unlink {
            DictionaryDeleteResolution::Unlink
        } else {
            DictionaryDeleteResolution::Replace(b)
        };
        apply(
            &mut s,
            vec![Mutation::DeleteDictionaryEntry {
                entry_id: a,
                resolution,
            }],
        )
        .unwrap();
        let snapshot = s.snapshot();
        assert_eq!(snapshot.project.dictionary_links.len(), 1);
        assert_eq!(snapshot.project.dictionary_links[0].target, existing);
        s.apply(
            Envelope {
                request_id: "undo".into(),
                session_id: s.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
        assert_eq!(s.snapshot().content_id, before);
        assert_eq!(s.snapshot().project.dictionary_links.len(), 2);
    }
}

#[test]
fn dictionary_source_kind_validation_is_scoped_to_document_and_element() {
    let (mut session, activity) = fixture();
    let role = DictionaryEntryId::new();
    let system = DictionaryEntryId::new();
    apply(
        &mut session,
        vec![
            Mutation::CreateDictionaryEntry {
                entry: DictionaryEntry {
                    id: role,
                    kind: DictionaryKind::Role,
                    name: "Role".into(),
                    description: String::new(),
                },
            },
            Mutation::CreateDictionaryEntry {
                entry: DictionaryEntry {
                    id: system,
                    kind: DictionaryKind::System,
                    name: "System".into(),
                    description: String::new(),
                },
            },
        ],
    )
    .unwrap();
    let before = session.snapshot().content_id;
    assert!(
        apply(
            &mut session,
            vec![Mutation::SetDictionaryLinks {
                source: activity.clone(),
                kind: DictionaryKind::Role,
                targets: vec![DictionaryTarget::Entry(role)]
            }]
        )
        .is_err()
    );
    let lane = ElementKey {
        document_id: activity.document_id,
        xml_id: id("lane"),
    };
    assert!(
        apply(
            &mut session,
            vec![Mutation::SetDictionaryLinks {
                source: lane,
                kind: DictionaryKind::System,
                targets: vec![DictionaryTarget::Entry(system)]
            }]
        )
        .is_err()
    );
    let missing = ElementKey {
        document_id: processcraft_core::DocumentId::new(),
        xml_id: activity.xml_id,
    };
    assert!(
        apply(
            &mut session,
            vec![Mutation::SetDictionaryLinks {
                source: missing,
                kind: DictionaryKind::System,
                targets: vec![DictionaryTarget::Entry(system)]
            }]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn comments_survive_element_deletion_as_explicit_orphans_and_undo() {
    use processcraft_core::ids::CommentId;
    let (mut session, key) = fixture();
    let id = CommentId::new();
    apply(
        &mut session,
        vec![Mutation::CreateComment {
            comment: Comment {
                id,
                target: key.clone(),
                author: "Local analyst".into(),
                text: "Check this".into(),
            },
        }],
    )
    .unwrap();
    assert!(!comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
    apply(
        &mut session,
        vec![Mutation::DeleteDocument {
            document_id: key.document_id,
        }],
    )
    .unwrap();
    assert!(comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
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
    assert!(!comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
}
#[test]
fn checkpoint_restore_is_atomic_undoable_and_reference_is_guarded() {
    use processcraft_core::ids::CheckpointId;
    let (mut session, key) = fixture();
    let id = CheckpointId::new();
    apply(
        &mut session,
        vec![
            Mutation::CreateCheckpoint {
                id,
                name: "Baseline".into(),
                created_unix_ms: 42,
            },
            Mutation::SetReferenceCheckpoint { id: Some(id) },
        ],
    )
    .unwrap();
    assert!(apply(&mut session, vec![Mutation::DeleteCheckpoint { id }]).is_err());
    apply(
        &mut session,
        vec![Mutation::Rename {
            document_id: key.document_id,
            element_id: key.xml_id,
            name: Some("Changed".into()),
        }],
    )
    .unwrap();
    let diff = compare_checkpoint(&session.snapshot().project, id).unwrap();
    assert_eq!(diff.changed_documents, vec![key.document_id]);
    apply(&mut session, vec![Mutation::RestoreCheckpoint { id }]).unwrap();
    assert!(
        compare_checkpoint(&session.snapshot().project, id)
            .unwrap()
            .changed_documents
            .is_empty()
    );
    assert_eq!(session.snapshot().project.reference_checkpoint, Some(id));
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
    assert_eq!(
        compare_checkpoint(&session.snapshot().project, id)
            .unwrap()
            .changed_documents,
        vec![key.document_id]
    );
}

#[test]
fn details_mutation_is_validated_atomically_and_undoable() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails, TimerKind};
    let (mut session, key) = fixture();
    let before = session.snapshot();
    let details = Some(NodeDetails::Event {
        definition_refs: vec![],
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
        definitions: vec![EventDefinition::Timer {
            base: None,
            id: None,
            kind: Some(TimerKind::Duration),
            expression: Some("PT5M".into()),
        }],
        attached_to: None,
        cancel_activity: None,
    });
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Must roll back".into()
                },
                Mutation::SetNodeDetails {
                    document_id: key.document_id,
                    element_id: key.xml_id.clone(),
                    details
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before.content_id);
    assert_eq!(session.snapshot().project.name, before.project.name);
}

#[test]
fn nested_scope_edits_and_cross_scope_flows_are_atomic() {
    use processcraft_core::bpmn_model::NodeDetails;
    let (session, key) = fixture();
    let mut project = (*session.snapshot().project).clone();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.processes[0].nodes.push(Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("sub"),
        name: Some("Sub".into()),
        documentation: String::new().into(),
        kind: NodeKind::SubProcess,
        details: Some(NodeDetails::SubProcess {
            content: Box::new(FlowContainer {
                lane_set_id: None,
                lane_set_base: None,
                groups: vec![],
                data_objects: vec![],
                data_associations: vec![],
                nodes: vec![Node {
                    flow_metadata: None,
                    base_extras: None,
                    item_aware: None,
                    id: id("inner"),
                    name: Some("Inside".into()),
                    documentation: String::new().into(),
                    kind: NodeKind::Task,
                    details: None,
                    loop_characteristics: None,
                    io_specification: None,
                    activity: None,
                    owner: String::new(),
                    contact: String::new(),
                }],
                lanes: vec![],
                flows: vec![],
            }),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    });
    let mut session = Session::new(project).unwrap();
    assert_eq!(scopes(&session.snapshot().project.documents[0]).len(), 2);
    apply(
        &mut session,
        vec![Mutation::Rename {
            document_id: key.document_id,
            element_id: id("inner"),
            name: Some("Renamed".into()),
        }],
    )
    .unwrap();
    assert_eq!(
        find_node(&session.snapshot().project.documents[0], &id("inner"))
            .unwrap()
            .name
            .as_deref(),
        Some("Renamed")
    );
    assert_eq!(
        activity_table(&session.snapshot().project, key.document_id, &id("sub")).unwrap()[0]
            .name
            .as_deref(),
        Some("Renamed")
    );
    let before = session.snapshot().content_id;
    let cross = Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id("cross"),
        kind: FlowKind::Sequence,
        source: id("inner"),
        target: id("activity"),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
    };
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::CreateFlow {
                    document_id: key.document_id,
                    process_id: Some(id("sub")),
                    flow: cross
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
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
    assert_eq!(
        find_node(&session.snapshot().project.documents[0], &id("inner"))
            .unwrap()
            .name
            .as_deref(),
        Some("Inside")
    );
    let comment_id = processcraft_core::ids::CommentId::new();
    apply(
        &mut session,
        vec![Mutation::CreateComment {
            comment: Comment {
                id: comment_id,
                target: ElementKey {
                    document_id: key.document_id,
                    xml_id: id("inner"),
                },
                author: "Analyst".into(),
                text: "Context".into(),
            },
        }],
    )
    .unwrap();
    assert!(!comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
    apply(
        &mut session,
        vec![Mutation::Delete {
            document_id: key.document_id,
            element_id: id("sub"),
        }],
    )
    .unwrap();
    assert!(comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
    session
        .apply(
            Envelope {
                request_id: "undo-delete".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert!(!comment_is_orphan(
        &session.snapshot().project,
        &session.snapshot().project.comments[0]
    ));
}

#[test]
fn loop_metadata_is_atomic_and_undoable() {
    use processcraft_core::bpmn_model::LoopCharacteristics;
    let (mut session, key) = fixture();
    let original = session.snapshot().content_id;
    let metadata = LoopCharacteristics::MultiInstance {
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        id: None,
        base: None,
        is_sequential: Some(false),
        cardinality: Some("3".into()),
        completion_condition: None,
    };
    apply(
        &mut session,
        vec![Mutation::SetNodeLoop {
            document_id: key.document_id,
            element_id: key.xml_id.clone(),
            loop_characteristics: Some(metadata.clone()),
        }],
    )
    .unwrap();
    assert_eq!(
        find_node(&session.snapshot().project.documents[0], &key.xml_id)
            .unwrap()
            .loop_characteristics,
        Some(metadata)
    );
    assert_ne!(session.snapshot().content_id, original);
    session
        .apply(
            Envelope {
                request_id: "undo-loop".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, original);
    let mut event_project = (*session.snapshot().project).clone();
    Arc::make_mut(&mut event_project.documents[0]).processes[0].nodes[0].kind =
        NodeKind::StartEvent;
    let mut event_session = Session::new(event_project).unwrap();
    let event_before = event_session.snapshot().content_id;
    assert!(
        apply(
            &mut event_session,
            vec![Mutation::SetNodeLoop {
                document_id: key.document_id,
                element_id: key.xml_id,
                loop_characteristics: Some(LoopCharacteristics::Standard {
                    id: None,
                    base: None,
                    test_before: Some(false),
                    condition: None,
                    maximum: None
                })
            }]
        )
        .is_err()
    );
    assert_eq!(event_session.snapshot().content_id, event_before);
}

#[test]
fn conversation_crud_checks_references_and_undoes_atomically() {
    use processcraft_core::bpmn_collaboration::{
        ConversationKind, ConversationLink, ConversationNode,
    };
    let (initial, key) = fixture();
    let mut project = (*initial.snapshot().project).clone();
    Arc::make_mut(&mut project.documents[0]).collaboration = Some(Collaboration {
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collaboration"),
        participants: vec![Participant {
            multiplicity: None,
            base: None,
            id: id("participant"),
            name: Some("Buyer".into()),
            process: None,
        }],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    let mut session = Session::new(project).unwrap();
    let node = ConversationNode {
        correlation_keys: vec![],
        base: None,
        id: id("conversation"),
        name: Some("Order".into()),
        kind: ConversationKind::Conversation,
        participant_refs: vec![id("participant")],
        parent: None,
    };
    let link = ConversationLink {
        base: None,
        id: id("conversation_link"),
        name: Some(String::new()),
        source: id("participant"),
        target: id("conversation"),
    };
    apply(
        &mut session,
        vec![
            Mutation::CreateConversationNode {
                document_id: key.document_id,
                node: node.clone(),
            },
            Mutation::CreateConversationLink {
                document_id: key.document_id,
                link,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        find_conversation_node(
            &session.snapshot().project.documents[0],
            &id("conversation")
        )
        .unwrap()
        .name
        .as_deref(),
        Some("Order")
    );
    let before = session.snapshot().content_id;
    let bad = ConversationLink {
        base: None,
        id: id("bad_link"),
        name: Some(String::new()),
        source: id("missing"),
        target: id("conversation"),
    };
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::CreateConversationLink {
                    document_id: key.document_id,
                    link: bad
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
    assert!(
        apply(
            &mut session,
            vec![Mutation::DeleteConversationNode {
                document_id: key.document_id,
                node_id: id("conversation")
            }]
        )
        .is_err()
    );
    session
        .apply(
            Envelope {
                request_id: "undo-conversation".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert!(
        find_conversation_node(
            &session.snapshot().project.documents[0],
            &id("conversation")
        )
        .is_none()
    );
}

#[test]
fn choreography_crud_rejects_cross_scope_references_and_undoes() {
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    let (mut session, key) = fixture();
    let choreography = Choreography {
        base: None,
        id: id("choreography"),
        name: Some("Exchange".into()),
        participants: vec![
            Participant {
                multiplicity: None,
                base: None,
                id: id("buyer"),
                name: Some("Buyer".into()),
                process: None,
            },
            Participant {
                multiplicity: None,
                base: None,
                id: id("seller"),
                name: Some("Seller".into()),
                process: None,
            },
        ],
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            correlation_keys: vec![],
            flow_metadata: None,
            base: None,
            id: id("task_exchange"),
            name: Some("Exchange".into()),
            participant_refs: vec![id("buyer"), id("seller")],
            initiating_participant: id("buyer"),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![],
    };
    apply(
        &mut session,
        vec![Mutation::CreateChoreography {
            document_id: key.document_id,
            choreography,
        }],
    )
    .unwrap();
    let before = session.snapshot().content_id;
    let invalid = ChoreographyActivity {
        correlation_keys: vec![],
        flow_metadata: None,
        base: None,
        id: id("outside"),
        name: Some("Invalid".into()),
        participant_refs: vec![id("buyer"), id("missing")],
        initiating_participant: id("buyer"),
        message_flow_refs: vec![],
    };
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::CreateChoreographyActivity {
                    document_id: key.document_id,
                    choreography_id: id("choreography"),
                    activity: invalid
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
    session
        .apply(
            Envelope {
                request_id: "undo-choreography".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert!(
        session.snapshot().project.documents[0]
            .choreographies
            .is_empty()
    );
}

#[test]
fn typed_data_association_is_scoped_to_owner_ports_and_undoable() {
    use processcraft_core::bpmn_data::{
        DataAssociation, DataAssociationKind, DataObject, DataPort, IoSpecification,
    };
    use processcraft_core::bpmn_model::NodeDetails;
    let (mut session, key) = fixture();
    let reference = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("data_ref"),
        name: Some("Order".into()),
        documentation: String::new().into(),
        kind: NodeKind::DataObjectReference,
        details: Some(NodeDetails::DataObjectReference {
            data_object_ref: Some(id("data_object")),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let association = DataAssociation {
        base: None,
        transformation: None,
        assignments: vec![],
        id: Some(id("data_assoc")),
        owner: key.xml_id.clone(),
        kind: DataAssociationKind::Input,
        sources: vec![id("data_ref")],
        target: id("input_port"),
    };
    apply(
        &mut session,
        vec![
            Mutation::CreateDataObject {
                document_id: key.document_id,
                scope_id: id("process"),
                data_object: DataObject {
                    flow_metadata: None,
                    base: None,
                    item_aware: None,
                    id: Some(id("data_object")),
                    name: Some("Order".into()),
                    is_collection: None,
                },
            },
            Mutation::CreateElement {
                document_id: key.document_id,
                process_id: id("process"),
                element: reference,
            },
            Mutation::SetNodeIoSpecification {
                document_id: key.document_id,
                element_id: key.xml_id.clone(),
                io_specification: Some(IoSpecification {
                    base: None,
                    id: None,
                    input_sets: vec![processcraft_core::bpmn_data::IoSet {
                        data_refs: vec![id("input_port")],
                        ..Default::default()
                    }],
                    output_sets: vec![processcraft_core::bpmn_data::IoSet::default()],
                    inputs: vec![DataPort {
                        base: None,
                        item_aware: None,
                        is_collection: None,
                        id: Some(id("input_port")),
                        name: Some("Order".into()),
                    }],
                    outputs: vec![],
                }),
            },
            Mutation::CreateDataAssociation {
                document_id: key.document_id,
                scope_id: id("process"),
                association: association.clone(),
            },
        ],
    )
    .unwrap();
    let before = session.snapshot().content_id;
    let mut invalid = association;
    invalid.id = Some(id("bad_data_assoc"));
    invalid.target = id("activity");
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::CreateDataAssociation {
                    document_id: key.document_id,
                    scope_id: id("process"),
                    association: invalid
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
    session
        .apply(
            Envelope {
                request_id: "undo-data".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert!(
        session.snapshot().project.documents[0].processes[0]
            .data_associations
            .is_empty()
    );
}

#[test]
fn nested_data_associations_resolve_ancestor_and_global_endpoints() {
    use processcraft_core::{bpmn_data::*, bpmn_model::NodeDetails};
    let (session, _) = fixture();
    let mut project = (*session.snapshot().project).clone();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.data_stores.push(DataStore {
        base: None,
        id: Some(id("store")),
        name: None,
        capacity: None,
        is_unlimited: None,
        item_aware: None,
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
        plane: id("process"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            id: id("store_shape"),
            element: id("store"),
            is_expanded: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 60.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    let port = |name: &str| DataPort {
        base: None,
        id: Some(id(name)),
        name: None,
        is_collection: None,
        item_aware: None,
    };
    let process = &mut doc.processes[0];
    process.io_specification = Some(IoSpecification {
        base: None,
        id: None,
        inputs: vec![port("process_in")],
        outputs: vec![port("process_out")],
        input_sets: vec![],
        output_sets: vec![],
    });
    process.data_objects.push(DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("outer_object")),
        name: None,
        is_collection: None,
        item_aware: None,
    });
    let reference = Node {
        flow_metadata: None,
        base_extras: None,
        id: id("outer_reference"),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind: NodeKind::DataObjectReference,
        details: Some(NodeDetails::DataObjectReference {
            data_object_ref: Some(id("outer_object")),
        }),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    process.nodes.push(reference);
    let task = Node {
        flow_metadata: None,
        base_extras: None,
        id: id("nested_task"),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind: NodeKind::Task,
        details: None,
        item_aware: None,
        loop_characteristics: None,
        io_specification: Some(IoSpecification {
            base: None,
            id: None,
            inputs: vec![port("task_in")],
            outputs: vec![port("task_out")],
            input_sets: vec![],
            output_sets: vec![],
        }),
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let assoc = |name: &str, kind, sources: Vec<&str>, target: &str| DataAssociation {
        base: None,
        id: Some(id(name)),
        owner: id("nested_task"),
        kind,
        sources: sources.into_iter().map(id).collect(),
        target: id(target),
        transformation: None,
        assignments: vec![],
    };
    let sub = |name: &str, content| Node {
        flow_metadata: None,
        base_extras: None,
        id: id(name),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind: NodeKind::SubProcess,
        details: Some(NodeDetails::SubProcess {
            content: Box::new(content),
        }),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    process.nodes.push(sub(
        "sub_a",
        FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            nodes: vec![task],
            lanes: vec![],
            flows: vec![],
            data_associations: vec![
                assoc(
                    "a1",
                    DataAssociationKind::Input,
                    vec!["outer_object"],
                    "task_in",
                ),
                assoc(
                    "a2",
                    DataAssociationKind::Input,
                    vec!["outer_reference"],
                    "task_in",
                ),
                assoc(
                    "a3",
                    DataAssociationKind::Input,
                    vec!["process_in"],
                    "task_in",
                ),
                assoc("a4", DataAssociationKind::Input, vec!["store"], "task_in"),
                assoc(
                    "a5",
                    DataAssociationKind::Output,
                    vec!["task_out"],
                    "process_out",
                ),
                assoc("a6", DataAssociationKind::Output, vec!["task_out"], "store"),
            ],
        },
    ));
    process.nodes.push(sub(
        "sub_b",
        FlowContainer {
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![DataObject {
                flow_metadata: None,
                base: None,
                id: Some(id("sibling_object")),
                name: None,
                is_collection: None,
                item_aware: None,
            }],
            data_associations: vec![],
            nodes: vec![],
            lanes: vec![],
            flows: vec![],
        },
    ));
    project.validate().unwrap();
    let mut invalid = project.clone();
    let doc = Arc::make_mut(&mut invalid.documents[0]);
    let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id.selector() == id::<processcraft_core::ids::ElementSelector>("sub_a"))
        .unwrap()
        .details
    else {
        unreachable!()
    };
    content.data_associations[0].sources = vec![id("sibling_object")];
    assert!(invalid.validate().is_err());
    let mut invalid = project.clone();
    let doc = Arc::make_mut(&mut invalid.documents[0]);
    let Some(NodeDetails::SubProcess { content }) = &mut doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id.selector() == id::<processcraft_core::ids::ElementSelector>("sub_a"))
        .unwrap()
        .details
    else {
        unreachable!()
    };
    content.data_associations[2].sources = vec![id("process_out")];
    assert!(invalid.validate().is_err());
}

#[test]
fn checkpoint_ceiling_refuses_twenty_first_without_changing_session() {
    use processcraft_core::ids::CheckpointId;
    let mut session = Session::new(ProjectData::new("Limit".into())).unwrap();
    for index in 0..20 {
        apply(
            &mut session,
            vec![Mutation::CreateCheckpoint {
                id: CheckpointId::new(),
                name: format!("Checkpoint {index}"),
                created_unix_ms: index,
            }],
        )
        .unwrap();
    }
    let before = session.snapshot();
    assert!(
        apply(
            &mut session,
            vec![Mutation::CreateCheckpoint {
                id: CheckpointId::new(),
                name: "Too many".into(),
                created_unix_ms: 20
            }]
        )
        .is_err()
    );
    let after = session.snapshot();
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.content_id, before.content_id);
    assert_eq!(after.project.checkpoints.len(), 20);
}

#[test]
fn conversation_delete_removes_di_and_undo_restores_it() {
    use processcraft_core::bpmn_collaboration::{
        ConversationKind, ConversationLink, ConversationNode,
    };
    let (initial, key) = fixture();
    let mut project = (*initial.snapshot().project).clone();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.collaboration = Some(Collaboration {
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collaboration"),
        participants: vec![Participant {
            multiplicity: None,
            base: None,
            id: id("buyer"),
            name: Some("Buyer".into()),
            process: None,
        }],
        flows: vec![],
        conversation_nodes: vec![ConversationNode {
            correlation_keys: vec![],
            base: None,
            id: id("conversation"),
            name: Some("Order".into()),
            kind: ConversationKind::Conversation,
            participant_refs: vec![id("buyer")],
            parent: None,
        }],
        conversation_links: vec![ConversationLink {
            base: None,
            id: id("link"),
            name: Some(String::new()),
            source: id("buyer"),
            target: id("conversation"),
        }],
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
        plane: id("collaboration"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            id: id("shape"),
            element: id("conversation"),
            is_expanded: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 70.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: None,
            label_visibility: None,
            source_element: None,
            target_element: None,
            id: id("edge"),
            element: id("link"),
            waypoints: vec![Point { x: 0.0, y: 0.0 }, Point { x: 50.0, y: 50.0 }],
            label: None,
            style: None,
        }],
    });
    let mut session = Session::new(project).unwrap();
    assert!(
        apply(
            &mut session,
            vec![Mutation::DeleteConversationNode {
                document_id: key.document_id,
                node_id: id("conversation")
            }]
        )
        .is_err()
    );
    apply(
        &mut session,
        vec![
            Mutation::DeleteConversationLink {
                document_id: key.document_id,
                link_id: id("link"),
            },
            Mutation::DeleteConversationNode {
                document_id: key.document_id,
                node_id: id("conversation"),
            },
        ],
    )
    .unwrap();
    assert!(
        session.snapshot().project.documents[0].diagrams[0]
            .shapes
            .is_empty()
    );
    assert!(
        session.snapshot().project.documents[0].diagrams[0]
            .edges
            .is_empty()
    );
    session
        .apply(
            Envelope {
                request_id: "undo-di".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(
        session.snapshot().project.documents[0].diagrams[0]
            .shapes
            .len(),
        1
    );
    assert_eq!(
        session.snapshot().project.documents[0].diagrams[0]
            .edges
            .len(),
        1
    );
}

#[test]
fn label_bounds_update_preserves_style_and_undo() {
    let (initial, key) = fixture();
    let mut project = (*initial.snapshot().project).clone();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram_label"),
        plane_id: id("plane_label"),
        plane: id("process"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            id: id("shape_label"),
            element: key.xml_id.clone(),
            is_expanded: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 60.0,
            },
            label: Some(Label {
                di_metadata: None,
                id: None,
                style_ref: None,
                bounds: Some(Bounds {
                    x: 1.0,
                    y: 2.0,
                    width: 30.0,
                    height: 10.0,
                }),
                style: None,
            }),
            style: None,
        }],
        edges: vec![],
    });
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot().content_id;
    let bounds = Bounds {
        x: 12.0,
        y: 22.0,
        width: 30.0,
        height: 10.0,
    };
    apply(
        &mut session,
        vec![Mutation::SetLabelBounds {
            document_id: key.document_id,
            diagram_id: id("diagram_label"),
            representation_id: id("shape_label"),
            bounds,
        }],
    )
    .unwrap();
    assert_eq!(
        session.snapshot().project.documents[0].diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .x,
        12.0
    );
    assert!(
        session.snapshot().project.documents[0].diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .style
            .is_none()
    );
    session
        .apply(
            Envelope {
                request_id: "undo-label".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn descriptive_commands_validate_references_and_undo_atomically() {
    use processcraft_core::bpmn_model::{
        AssociationDirection, Category, CategoryValue, Group, NodeDetails,
    };
    let (mut session, key) = fixture();
    let before = session.snapshot().content_id;
    let annotation = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("note"),
        name: Some("Review note".into()),
        documentation: String::new().into(),
        kind: NodeKind::TextAnnotation,
        details: Some(NodeDetails::TextAnnotation {
            text: None,
            text_format: Some("text/plain".into()),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let association = Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        id: id("association"),
        kind: FlowKind::Association,
        source: key.xml_id.clone(),
        target: id("note"),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    };
    apply(
        &mut session,
        vec![
            Mutation::CreateCategory {
                document_id: key.document_id,
                category: Category {
                    base: None,
                    id: id("category"),
                    name: Some("Risk".into()),
                    values: vec![CategoryValue {
                        base: None,
                        id: id("category_value"),
                        value: Some("Review".into()),
                    }],
                },
            },
            Mutation::CreateGroup {
                document_id: key.document_id,
                scope_id: id("process"),
                group: Group {
                    base: None,
                    id: id("group"),
                    category_value_ref: Some(id("category_value")),
                },
            },
            Mutation::SetLaneSetId {
                document_id: key.document_id,
                scope_id: id("process"),
                lane_set_id: Some(id("laneset")),
            },
            Mutation::SetChildLaneSetId {
                document_id: key.document_id,
                scope_id: id("process"),
                lane_id: id("lane"),
                child_lane_set_id: Some(id("child_laneset")),
            },
            Mutation::CreateElement {
                document_id: key.document_id,
                process_id: id("process"),
                element: annotation,
            },
            Mutation::CreateFlow {
                document_id: key.document_id,
                process_id: Some(id("process")),
                flow: association,
            },
            Mutation::SetAssociationDirection {
                document_id: key.document_id,
                flow_id: id("association"),
                direction: Some(AssociationDirection::Both),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        find_flow(&session.snapshot().project.documents[0], &id("association"))
            .unwrap()
            .association_direction,
        Some(AssociationDirection::Both)
    );
    let changed = session.snapshot().content_id;
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::DeleteCategory {
                    document_id: key.document_id,
                    category_id: id("category")
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, changed);
    session
        .apply(
            Envelope {
                request_id: "undo-descriptive".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn deleting_group_removes_di_and_undo_restores_both() {
    use processcraft_core::bpmn_model::Group;
    let (mut session, key) = fixture();
    apply(
        &mut session,
        vec![
            Mutation::CreateGroup {
                document_id: key.document_id,
                scope_id: id("process"),
                group: Group {
                    base: None,
                    id: id("group_di"),
                    category_value_ref: None,
                },
            },
            Mutation::CreateDiagram {
                document_id: key.document_id,
                diagram: Diagram {
                    plane_metadata: None,
                    plane_order: None,
                    documentation: None,
                    resolution: None,
                    label_styles: vec![],
                    name: None,
                    id: id("group_diagram"),
                    plane_id: id("group_plane"),
                    plane: id("process"),
                    shapes: vec![Shape {
                        di_metadata: None,
                        is_message_visible: None,
                        participant_band_kind: None,
                        choreography_activity_shape: None,
                        label_visibility: None,
                        is_horizontal: None,
                        is_marker_visible: None,
                        id: id("group_shape"),
                        element: id("group_di"),
                        is_expanded: None,
                        bounds: Bounds {
                            x: 0.0,
                            y: 0.0,
                            width: 200.0,
                            height: 100.0,
                        },
                        label: None,
                        style: None,
                    }],
                    edges: vec![],
                },
            },
        ],
    )
    .unwrap();
    apply(
        &mut session,
        vec![Mutation::DeleteGroup {
            document_id: key.document_id,
            scope_id: id("process"),
            group_id: id("group_di"),
        }],
    )
    .unwrap();
    let changed = session.snapshot();
    assert!(changed.project.documents[0].processes[0].groups.is_empty());
    assert!(changed.project.documents[0].diagrams[0].shapes.is_empty());
    session
        .apply(
            Envelope {
                request_id: "undo-group".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    let restored = session.snapshot();
    assert_eq!(restored.project.documents[0].processes[0].groups.len(), 1);
    assert_eq!(restored.project.documents[0].diagrams[0].shapes.len(), 1);
}

#[test]
fn activity_properties_validate_scope_and_kind_and_undo() {
    use processcraft_core::bpmn_activity::ActivityProperties;
    let (mut session, key) = fixture();
    let target = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("activity_target"),
        name: Some("Target".into()),
        documentation: String::new().into(),
        kind: NodeKind::Task,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let flow = Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        id: id("activity_default"),
        kind: FlowKind::Sequence,
        source: key.xml_id.clone(),
        target: id("activity_target"),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    };
    apply(
        &mut session,
        vec![
            Mutation::CreateElement {
                document_id: key.document_id,
                process_id: id("process"),
                element: target,
            },
            Mutation::CreateFlow {
                document_id: key.document_id,
                process_id: Some(id("process")),
                flow,
            },
        ],
    )
    .unwrap();
    let before = session.snapshot().content_id;
    let properties = ActivityProperties {
        is_for_compensation: Some(true),
        start_quantity: Some("123456789012345678901234567890".into()),
        default_flow: Some(id("activity_default")),
        ..Default::default()
    };
    apply(
        &mut session,
        vec![Mutation::SetActivityProperties {
            document_id: key.document_id,
            element_id: key.xml_id.clone(),
            activity: Some(properties.clone()),
        }],
    )
    .unwrap();
    assert_eq!(
        find_node(&session.snapshot().project.documents[0], &key.xml_id)
            .unwrap()
            .activity,
        Some(properties)
    );
    let changed = session.snapshot().content_id;
    let invalid = ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    };
    assert!(
        apply(
            &mut session,
            vec![
                Mutation::RenameProject {
                    name: "Rollback".into()
                },
                Mutation::SetActivityProperties {
                    document_id: key.document_id,
                    element_id: key.xml_id.clone(),
                    activity: Some(invalid)
                }
            ]
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, changed);
    let invalid_default = ActivityProperties {
        default_flow: Some(id("missing")),
        ..Default::default()
    };
    assert!(
        apply(
            &mut session,
            vec![Mutation::SetActivityProperties {
                document_id: key.document_id,
                element_id: key.xml_id.clone(),
                activity: Some(invalid_default)
            }]
        )
        .is_err()
    );
    session
        .apply(
            Envelope {
                request_id: "undo-activity".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
