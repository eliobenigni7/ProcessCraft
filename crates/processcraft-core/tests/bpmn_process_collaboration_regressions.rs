//! Repair tests for the process/collaboration family: typed reads, edits with
//! undo of pre-existing elements, and exact diagnostics (not bare `is_err()`).
use processcraft_core::{
    bpmn_activity::{FlowElementMetadata, ModelReference},
    bpmn_model::Expression,
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
    validation::activity_family_reference_issues,
};

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn ident(s: &str) -> ElementIdentity {
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(sel(s))
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
fn content(s: &Session) -> String {
    format!("{:?}", s.snapshot().content_id)
}
fn lane(s: &Session, id: &str) -> Lane {
    document(s).processes[0]
        .lanes
        .iter()
        .find(|l| l.id == ident(id))
        .cloned()
        .unwrap()
}
fn seq_flow(s: &Session, id: &str) -> Flow {
    document(s).processes[0]
        .flows
        .iter()
        .find(|f| f.id == ident(id))
        .cloned()
        .unwrap()
}
fn validate_err(doc: BpmnDocument) -> String {
    let mut p = ProjectData::new("p".into());
    p.documents.push(std::sync::Arc::new(doc));
    p.validate().unwrap_err()
}
fn collaboration() -> Collaboration {
    Collaboration {
        id: ident("collab"),
        name: None,
        base: None,
        kind: None,
        extras: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    }
}
fn participant(id: &str, process: Option<ModelReference>) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: ident(id),
        name: None,
        process,
    }
}
fn set_root(c: Collaboration) -> Mutation {
    Mutation::SetCollaborationRoot {
        document_id: docid(),
        collaboration: c,
    }
}
fn participants(s: &Session) -> Vec<Participant> {
    document(s).collaboration.unwrap().participants
}

#[test]
fn existing_lane_rename_and_child_lane_set_edit_each_undo_to_prior_value() {
    let mut s = session();
    assert_eq!(
        lane(&s, "lane_procurement").name.as_deref(),
        Some("Procurement")
    );
    let start = content(&s);
    run(
        &mut s,
        vec![Mutation::Rename {
            document_id: docid(),
            element_id: sel("lane_procurement"),
            name: Some("Buying".into()),
        }],
    );
    assert_eq!(lane(&s, "lane_procurement").name.as_deref(), Some("Buying"));
    undo(&mut s);
    assert_eq!(
        lane(&s, "lane_procurement").name.as_deref(),
        Some("Procurement")
    );
    assert_eq!(content(&s), start);

    let set = |s: &mut Session, value: Option<ElementIdentity>| {
        run(
            s,
            vec![Mutation::SetChildLaneSetId {
                document_id: docid(),
                scope_id: sel("procurement"),
                lane_id: sel("lane_procurement"),
                child_lane_set_id: value,
            }],
        )
    };
    let child = |s: &Session| lane(s, "lane_procurement").child_lane_set_id;
    assert_eq!(child(&s), None);
    set(&mut s, Some(ident("child_set")));
    assert_eq!(child(&s), Some(ident("child_set")));
    undo(&mut s);
    assert_eq!(child(&s), None);
    set(&mut s, Some(ident("child_set")));
    let with_id = content(&s);
    set(&mut s, None);
    assert_eq!(child(&s), None);
    undo(&mut s);
    assert_eq!(child(&s), Some(ident("child_set")));
    assert_eq!(content(&s), with_id);
}

#[test]
fn lane_membership_and_nesting_violations_name_the_rule() {
    let s = session();
    let base = document(&s);
    let mut p = ProjectData::new("p".into());
    p.documents.push(std::sync::Arc::new(base.clone()));
    p.validate().unwrap();

    let mut dangling = base.clone();
    dangling.processes[0].lanes[0]
        .members
        .push(sel("no_such_node"));
    assert_eq!(
        validate_err(dangling),
        "invalid or duplicate lane membership"
    );

    let mut duplicate = base.clone();
    duplicate.processes[0].lanes[1].members.push(sel("start"));
    assert_eq!(
        validate_err(duplicate),
        "invalid or duplicate lane membership"
    );

    let mut cycle = base.clone();
    cycle.processes[0].lanes[0].parent = Some(sel("lane_procurement"));
    cycle.processes[0].lanes[1].parent = Some(sel("lane_request"));
    assert_eq!(validate_err(cycle), "lane cycle");

    let mut orphan = base;
    orphan.processes[0].lanes[0].parent = Some(sel("no_such_lane"));
    assert_eq!(validate_err(orphan), "missing parent lane");
}

#[test]
fn participant_process_ref_is_created_read_back_cleared_and_undone() {
    let mut s = session();
    run(&mut s, vec![set_root(collaboration())]);
    let empty = content(&s);
    run(
        &mut s,
        vec![Mutation::CreateParticipant {
            document_id: docid(),
            participant: participant("buyer", Some(local("procurement"))),
        }],
    );
    let ps = participants(&s);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].id, ident("buyer"));
    assert_eq!(ps[0].process, Some(local("procurement")));
    undo(&mut s);
    assert!(participants(&s).is_empty());
    assert_eq!(content(&s), empty);

    let mut c = collaboration();
    c.participants = vec![participant("buyer", Some(local("procurement")))];
    run(&mut s, vec![set_root(c)]);
    let with_ref = content(&s);
    let mut c = document(&s).collaboration.unwrap();
    c.participants[0].process = None;
    run(&mut s, vec![set_root(c)]);
    assert_eq!(participants(&s)[0].process, None);
    undo(&mut s);
    assert_eq!(participants(&s)[0].process, Some(local("procurement")));
    assert_eq!(content(&s), with_ref);
}

#[test]
fn dangling_participant_process_ref_is_diagnosed_with_field_owner_and_target() {
    let mut doc = BpmnDocument::new("d".into(), ident("defs"));
    let mut c = collaboration();
    c.participants = vec![participant("buyer", Some(local("no_such_process")))];
    doc.collaboration = Some(c);
    let issues = activity_family_reference_issues(&doc);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].field, "participant.processRef");
    assert_eq!(issues[0].owner, sel("buyer"));
    assert_eq!(issues[0].target, local("no_such_process"));
}

#[test]
fn sequence_flow_ends_and_condition_are_read_back_edited_cleared_and_undone() {
    let mut s = session();
    let f = seq_flow(&s, "flow_approved");
    assert_eq!(f.kind, FlowKind::Sequence);
    assert_eq!(f.source, Some(local("decision")));
    assert_eq!(f.target, Some(local("order")));
    assert_eq!(f.condition, Some(Expression::Legacy("approved".into())));
    let start = content(&s);
    for value in [Some(Expression::Legacy("amount > 5".into())), None] {
        run(
            &mut s,
            vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: sel("flow_approved"),
                condition: value.clone(),
            }],
        );
        assert_eq!(seq_flow(&s, "flow_approved").condition, value);
        undo(&mut s);
        assert_eq!(
            seq_flow(&s, "flow_approved").condition,
            Some(Expression::Legacy("approved".into()))
        );
        assert_eq!(content(&s), start);
    }
}

#[test]
fn sequence_and_message_flow_dangling_targets_are_diagnosed() {
    let s = session();
    let mut doc = document(&s);
    doc.processes[0]
        .flows
        .iter_mut()
        .find(|f| f.id == ident("flow_approved"))
        .unwrap()
        .target = Some(local("no_such_node"));
    let issues = activity_family_reference_issues(&doc);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].field, "sequenceFlow.targetRef");
    assert_eq!(issues[0].owner, sel("flow_approved"));
    assert_eq!(issues[0].target, local("no_such_node"));

    let mut doc = BpmnDocument::new("d".into(), ident("defs"));
    let mut c = collaboration();
    c.participants = vec![participant("seller", None)];
    c.flows = vec![Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: ident("order"),
        kind: FlowKind::Message,
        source: Some(local("seller")),
        target: Some(local("ghost_target")),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    }];
    doc.collaboration = Some(c);
    let issues = activity_family_reference_issues(&doc);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].field, "messageFlow.targetRef");
    assert_eq!(issues[0].owner, sel("order"));
    assert_eq!(issues[0].target, local("ghost_target"));
}

#[test]
fn sequence_flow_declaration_violations_name_the_rule() {
    let s = session();
    let base = document(&s);
    let set = |target: Option<&str>, source: Option<&str>| {
        let mut d = base.clone();
        let f = d.processes[0]
            .flows
            .iter_mut()
            .find(|f| f.id == ident("flow_approved"))
            .unwrap();
        if let Some(t) = target {
            f.target = Some(local(t));
        }
        if let Some(sr) = source {
            f.source = Some(local(sr));
        }
        d
    };
    // an existing id that is not a node of this process (cross-scope / non-node target)
    assert_eq!(
        validate_err(set(Some("lane_request"), None)),
        "invalid process flow endpoints"
    );
    assert_eq!(
        validate_err(set(Some("start"), None)),
        "invalid sequence endpoint"
    );
    assert_eq!(
        validate_err(set(None, Some("end"))),
        "invalid sequence endpoint"
    );
    // duplicate xml id inside the document
    let mut dup = base.clone();
    dup.processes[0].nodes[1].id = dup.processes[0].nodes[0].id.clone();
    assert_eq!(validate_err(dup), "duplicate XML ID");
}

#[test]
fn is_immediate_is_refused_on_a_message_flow_with_the_sequence_flow_rule() {
    let mut s = session();
    let mut c = collaboration();
    c.participants = vec![participant("a", None), participant("b", None)];
    c.flows = vec![Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: ident("m"),
        kind: FlowKind::Message,
        source: Some(local("a")),
        target: Some(local("b")),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    }];
    run(&mut s, vec![set_root(c)]);
    let start = content(&s);
    let metadata = FlowElementMetadata {
        is_immediate: Some(true),
        ..Default::default()
    };
    let error = send(
        &mut s,
        Request::Batch {
            mutations: vec![Mutation::SetFlowMetadata {
                document_id: docid(),
                element_id: sel("m"),
                metadata: Some(metadata.clone()),
            }],
        },
    )
    .unwrap_err();
    let text = format!("{error:?}");
    assert!(
        text.contains("Flow-element metadata requires a sequence flow"),
        "{text}"
    );
    assert_eq!(content(&s), start);
    // the same value on a sequence flow is accepted
    run(
        &mut s,
        vec![Mutation::SetFlowMetadata {
            document_id: docid(),
            element_id: sel("flow_approved"),
            metadata: Some(metadata),
        }],
    );
}
