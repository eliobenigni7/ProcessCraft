//! M3 round 2 (edit-model-io): lane-set `name`, lane `partitionElement` and
//! `partitionElementRef` model values and commands (BPMN 2.0.2 sec 9.3 lane
//! metamodel / Semantic.xsd tLaneSet, tLane), plus element-level create / update /
//! delete with undo for process, collaboration, participant, lane, message,
//! message flow and sequence flow.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::CollaborationExtras,
    bpmn_model::{EventResource, QNameRef},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
};

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn ident(s: &str) -> ElementIdentity {
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn send(s: &mut Session, request: Request) -> Result<(), CommandError> {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, request).map(|_| ())
}
fn run(s: &mut Session, mutations: Vec<Mutation>) {
    send(s, Request::Batch { mutations }).unwrap();
}
fn undo(s: &mut Session) {
    send(s, Request::Undo).unwrap();
}
fn session() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let setup: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    send(&mut s, setup).unwrap();
    s
}
fn document(s: &Session) -> BpmnDocument {
    (*s.snapshot().project.documents[0]).clone()
}
fn lane(s: &Session, id: &str) -> Lane {
    document(s).processes[0]
        .lanes
        .iter()
        .find(|l| l.id == ident(id))
        .cloned()
        .unwrap()
}
fn external(local: &str) -> ModelReference {
    ModelReference::Qualified(QNameRef {
        namespace: "urn:external".into(),
        local: local.parse().unwrap(),
    })
}

#[test]
fn lane_set_name_edits_with_undo_and_rejects_invalid_text() {
    let mut s = session();
    let base = s.snapshot().content_id;
    run(
        &mut s,
        vec![Mutation::SetLaneSetName {
            document_id: docid(),
            scope_id: sel("procurement"),
            lane_set_name: Some("By department".into()),
        }],
    );
    assert_eq!(
        document(&s).processes[0].lane_set_name.as_deref(),
        Some("By department")
    );
    // Explicit empty string is a value distinct from absence.
    run(
        &mut s,
        vec![Mutation::SetLaneSetName {
            document_id: docid(),
            scope_id: sel("procurement"),
            lane_set_name: Some(String::new()),
        }],
    );
    assert_eq!(document(&s).processes[0].lane_set_name.as_deref(), Some(""));
    let before = (s.snapshot().content_id, s.snapshot().generation);
    let error = send(
        &mut s,
        Request::Batch {
            mutations: vec![Mutation::SetLaneSetName {
                document_id: docid(),
                scope_id: sel("procurement"),
                lane_set_name: Some("bad\u{1}".into()),
            }],
        },
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::InvalidParams(_)), "{error:?}");
    assert_eq!((s.snapshot().content_id, s.snapshot().generation), before);
    run(
        &mut s,
        vec![Mutation::SetLaneSetName {
            document_id: docid(),
            scope_id: sel("procurement"),
            lane_set_name: None,
        }],
    );
    assert_eq!(document(&s).processes[0].lane_set_name, None);
    undo(&mut s);
    undo(&mut s);
    assert_eq!(
        document(&s).processes[0].lane_set_name.as_deref(),
        Some("By department")
    );
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, base);
}

#[test]
fn child_lane_set_name_and_lane_partition_edit_with_undo() {
    let mut s = session();
    let base = s.snapshot().content_id;
    run(
        &mut s,
        vec![
            Mutation::SetChildLaneSetName {
                document_id: docid(),
                scope_id: sel("procurement"),
                lane_id: sel("lane_procurement"),
                child_lane_set_name: Some("By skill".into()),
            },
            Mutation::SetLanePartition {
                document_id: docid(),
                scope_id: sel("procurement"),
                lane_id: sel("lane_procurement"),
                partition_element: Some(LanePartitionElement {
                    id: ident("part"),
                    value: Some("Office".into()),
                }),
                partition_element_ref: Some(external("Dept")),
            },
        ],
    );
    let l = lane(&s, "lane_procurement");
    assert_eq!(l.child_lane_set_name.as_deref(), Some("By skill"));
    let part = l.partition_element.unwrap();
    assert_eq!(
        (part.id, part.value.as_deref()),
        (ident("part"), Some("Office"))
    );
    assert_eq!(l.partition_element_ref, Some(external("Dept")));
    // The partition element id is a document identity: a duplicate is rejected atomically.
    let before = (s.snapshot().content_id, s.snapshot().generation);
    let error = send(
        &mut s,
        Request::Batch {
            mutations: vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: sel("review"),
                xml_id: Some("part".parse().unwrap()),
            }],
        },
    );
    assert!(error.is_err());
    assert_eq!((s.snapshot().content_id, s.snapshot().generation), before);
    // Native JSON round trip keeps the typed fields.
    let json = serde_json::to_value(document(&s)).unwrap();
    let back: BpmnDocument = serde_json::from_value(json).unwrap();
    let lanes = &back.processes[0].lanes;
    let restored = lanes.iter().find(|l| l.id == ident("lane_procurement"));
    assert_eq!(
        restored.unwrap().partition_element_ref,
        Some(external("Dept")),
        "{lanes:?}"
    );
    run(
        &mut s,
        vec![Mutation::SetLanePartition {
            document_id: docid(),
            scope_id: sel("procurement"),
            lane_id: sel("lane_procurement"),
            partition_element: None,
            partition_element_ref: None,
        }],
    );
    let l = lane(&s, "lane_procurement");
    assert!(l.partition_element.is_none() && l.partition_element_ref.is_none());
    undo(&mut s);
    assert!(lane(&s, "lane_procurement").partition_element.is_some());
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, base);
}

#[test]
fn element_level_process_lane_and_sequence_flow_create_delete_undo() {
    let mut s = session();
    let base = s.snapshot().content_id;
    let process = Process {
        activity_metadata: None,
        is_executable: Some(true),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: ident("second"),
        name: Some("Second".into()),
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    };
    run(
        &mut s,
        vec![
            Mutation::CreateProcess {
                document_id: docid(),
                process,
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: sel("second"),
                name: Some("Renamed".into()),
            },
            Mutation::CreateLane {
                document_id: docid(),
                process_id: sel("second"),
                lane: Lane {
                    base: None,
                    child_lane_set_id: None,
                    child_lane_set_base: None,
                    child_lane_set_name: None,
                    partition_element: None,
                    partition_element_ref: None,
                    id: ident("lane_second"),
                    name: Some("Only".into()),
                    parent: None,
                    members: vec![],
                },
            },
        ],
    );
    let d = document(&s);
    assert_eq!(d.processes.len(), 2);
    assert_eq!(d.processes[1].name.as_deref(), Some("Renamed"));
    assert_eq!(d.processes[1].lanes[0].id, ident("lane_second"));
    run(
        &mut s,
        vec![Mutation::Delete {
            document_id: docid(),
            element_id: sel("lane_second"),
        }],
    );
    assert!(document(&s).processes[1].lanes.is_empty());
    undo(&mut s);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, base);
    // Sequence flow: delete and restore.
    let flows = document(&s).processes[0].flows.len();
    assert!(flows > 0);
    let flow_id = document(&s).processes[0].flows[0].id.selector();
    run(
        &mut s,
        vec![Mutation::Delete {
            document_id: docid(),
            element_id: flow_id,
        }],
    );
    assert_eq!(document(&s).processes[0].flows.len(), flows - 1);
    undo(&mut s);
    assert_eq!(document(&s).processes[0].flows.len(), flows);
}

#[test]
fn element_level_collaboration_participant_message_and_message_flow_with_undo() {
    let mut s = session();
    let base = s.snapshot().content_id;
    let participant = |id: &str, name: &str| Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: ident(id),
        name: Some(name.into()),
        process: None,
    };
    let collaboration = Collaboration {
        id: ident("collab"),
        name: Some("Order handling".into()),
        base: None,
        kind: None,
        extras: Some(CollaborationExtras {
            is_closed: Some(true),
            ..Default::default()
        }),
        correlation_keys: vec![],
        participants: vec![
            participant("buyer", "Buyer"),
            participant("seller", "Seller"),
        ],
        flows: vec![Flow {
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            id: ident("order_msg"),
            kind: FlowKind::Message,
            source: Some(ModelReference::Local(sel("buyer"))),
            target: Some(ModelReference::Local(sel("seller"))),
            name: Some("Order".into()),
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        }],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    run(
        &mut s,
        vec![
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration,
            },
            Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![EventResource::Message {
                    id: ident("msg"),
                    base: None,
                    name: Some("Order message".into()),
                    item_ref: None,
                }],
            },
            Mutation::SetMessageFlowReference {
                document_id: docid(),
                flow_id: sel("order_msg"),
                message_ref: Some(ModelReference::Local(sel("msg"))),
            },
        ],
    );
    let d = document(&s);
    let c = d.collaboration.as_ref().unwrap();
    assert_eq!(c.participants.len(), 2);
    assert_eq!(
        c.flows[0].message_ref,
        Some(ModelReference::Local(sel("msg")))
    );
    assert!(
        matches!(&d.event_resources[..], [EventResource::Message { id, .. }] if *id == ident("msg"))
    );
    run(
        &mut s,
        vec![Mutation::DeleteCollaborationRoot {
            document_id: docid(),
            collaboration_id: sel("collab"),
        }],
    );
    assert!(document(&s).collaboration.is_none());
    undo(&mut s);
    assert!(document(&s).collaboration.is_some());
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, base);
}

#[test]
fn text_annotation_text_format_edits_with_undo() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut s = session();
    let node = Node {
        flow_metadata: None,
        base_extras: None,
        id: ident("note"),
        name: None,
        documentation: String::new().into(),
        kind: NodeKind::TextAnnotation,
        details: Some(NodeDetails::TextAnnotation {
            text_format: Some("text/plain".into()),
            text: None,
        }),
        item_aware: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    run(
        &mut s,
        vec![Mutation::CreateElement {
            document_id: docid(),
            process_id: sel("procurement"),
            element: node,
        }],
    );
    let format =
        |s: &Session| match processcraft_core::queries::find_node(&document(s), &sel("note"))
            .unwrap()
            .details
            .clone()
        {
            Some(NodeDetails::TextAnnotation { text_format, .. }) => text_format,
            other => panic!("{other:?}"),
        };
    assert_eq!(format(&s).as_deref(), Some("text/plain"));
    run(
        &mut s,
        vec![Mutation::SetNodeDetails {
            document_id: docid(),
            element_id: sel("note"),
            details: Some(NodeDetails::TextAnnotation {
                text_format: Some("text/markdown".into()),
                text: None,
            }),
        }],
    );
    assert_eq!(format(&s).as_deref(), Some("text/markdown"));
    undo(&mut s);
    assert_eq!(format(&s).as_deref(), Some("text/plain"));
}

/// BPMN 2.0.2 Semantic.xsd tServiceTask `operationRef`: typed value on the
/// activity, set/cleared by `SetActivityProperties`, undoable.
#[test]
fn service_task_operation_ref_sets_clears_and_undoes() {
    let mut s = session();
    let svc = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: ident("m3r2_svc"),
        name: Some("Lookup".into()),
        documentation: String::new().into(),
        kind: NodeKind::ServiceTask,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    run(
        &mut s,
        vec![Mutation::CreateElement {
            document_id: docid(),
            process_id: sel("procurement"),
            element: svc,
        }],
    );
    let created = s.snapshot().content_id;
    let op = |s: &Session| {
        processcraft_core::queries::find_node(&document(s), &sel("m3r2_svc"))
            .unwrap()
            .activity
            .as_ref()
            .and_then(|a| a.operation_ref.clone())
    };
    assert_eq!(op(&s), None);
    let q = QNameRef {
        namespace: "urn:external".into(),
        local: "lookupOp".parse().unwrap(),
    };
    let set = |activity| Mutation::SetActivityProperties {
        document_id: docid(),
        element_id: sel("m3r2_svc"),
        activity,
    };
    run(
        &mut s,
        vec![set(Some(
            processcraft_core::bpmn_activity::ActivityProperties {
                implementation: Some("##WebService".into()),
                operation_ref: Some(q.clone()),
                ..Default::default()
            },
        ))],
    );
    assert_eq!(op(&s), Some(q));
    run(&mut s, vec![set(None)]);
    assert_eq!(op(&s), None);
    undo(&mut s);
    assert!(op(&s).is_some());
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, created);
}
