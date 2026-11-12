use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyLoopType,
        ChoreographyRootKind,
    },
    commands::{Envelope, Mutation, Request, Session},
    ids::DocumentId,
    model::*,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(id(s))
}
fn request(s: &mut Session, request: Request) {
    try_request(s, request).unwrap();
}
fn try_request(
    s: &mut Session,
    request: Request,
) -> Result<processcraft_core::commands::ChangeSet, processcraft_core::commands::CommandError> {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        request,
    )
}
fn apply(s: &mut Session, mutations: Vec<Mutation>) {
    request(s, Request::Batch { mutations });
}
fn try_apply(s: &mut Session, mutations: Vec<Mutation>) -> bool {
    try_request(s, Request::Batch { mutations }).is_ok()
}
fn undo(s: &mut Session) {
    request(s, Request::Undo);
}
fn participant(name: &str) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(name),
        name: Some(name.into()),
        process: None,
    }
}
fn activity(name: &str) -> ChoreographyActivity {
    ChoreographyActivity {
        loop_type: None,
        details: None,
        flow_metadata: None,
        correlation_keys: vec![],
        base: None,
        id: id(name),
        name: Some(name.into()),
        participant_refs: vec![local("buyer"), local("seller")],
        initiating_participant: Some(local("buyer")),
        message_flow_refs: vec![],
    }
}
fn root(name: &str, participants: &[&str], activities: Vec<ChoreographyActivity>) -> Choreography {
    Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id(name),
        name: Some(name.into()),
        participants: participants.iter().map(|p| participant(p)).collect(),
        message_flows: vec![],
        activities,
        sequence_flows: vec![],
    }
}
fn message_flow(name: &str, source: &str, target: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Message,
        source: Some(local(source)),
        target: Some(local(target)),
        name: None,
        documentation: String::new().into(),
        condition: None,
    }
}
fn session_with(roots: Vec<Choreography>) -> (Session, DocumentId) {
    let mut p = ProjectData::new("p".into());
    let d = BpmnDocument::new("doc".into(), id("defs"));
    let document_id = d.id;
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    apply(
        &mut s,
        roots
            .into_iter()
            .map(|choreography| Mutation::CreateChoreography {
                document_id,
                choreography,
            })
            .collect(),
    );
    (s, document_id)
}
fn codes(s: &Session) -> Vec<String> {
    s.snapshot()
        .project
        .diagnostics()
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}
fn doc_of(s: &Session) -> Arc<BpmnDocument> {
    s.snapshot().project.documents[0].clone()
}

#[test]
fn call_choreography_ref_is_edited_validated_and_undone() {
    let mut call = activity("call");
    call.details = Some(ChoreographyActivityDetails::Call {
        participant_associations: vec![],
        called_choreography_ref: Some(local("global")),
    });
    let mut global = root("global", &["gb", "gs"], vec![]);
    global.kind = Some(ChoreographyRootKind::Global {
        initiating_participant_ref: None,
    });
    let (mut s, document_id) =
        session_with(vec![global, root("main", &["buyer", "seller"], vec![call])]);
    let called = |s: &Session| match &doc_of(s).choreographies[1].activities[0].details {
        Some(ChoreographyActivityDetails::Call {
            called_choreography_ref,
            ..
        }) => called_choreography_ref.clone(),
        _ => panic!("call expected"),
    };
    assert_eq!(called(&s), Some(local("global")));
    let c = codes(&s);
    assert!(
        !c.iter().any(|c| c.starts_with("calledChoreography")),
        "{c:?}"
    );
    let before = s.snapshot().content_id;
    // validate: unresolved and wrong-type called targets are refused atomically.
    for target in ["absent", "buyer"] {
        let mut edited = doc_of(&s).choreographies[1].activities[0].clone();
        edited.details = Some(ChoreographyActivityDetails::Call {
            participant_associations: vec![],
            called_choreography_ref: Some(local(target)),
        });
        assert!(!try_apply(
            &mut s,
            vec![Mutation::UpdateChoreographyActivity {
                document_id,
                choreography_id: id("main"),
                activity: edited,
            }],
        ));
        assert_eq!(called(&s), Some(local("global")));
        assert_eq!(s.snapshot().content_id, before);
    }
    // edit: retarget to the second global choreography, then undo.
    let mut edited = doc_of(&s).choreographies[1].activities[0].clone();
    edited.details = Some(ChoreographyActivityDetails::Call {
        participant_associations: vec![],
        called_choreography_ref: Some(local("main")),
    });
    apply(
        &mut s,
        vec![Mutation::UpdateChoreographyActivity {
            document_id,
            choreography_id: id("main"),
            activity: edited,
        }],
    );
    assert_eq!(called(&s), Some(local("main")));
    undo(&mut s);
    assert_eq!(called(&s), Some(local("global")));
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn global_choreography_task_kind_and_initiator_edit_validate_and_undo() {
    let (mut s, document_id) = session_with(vec![
        root("global", &["gb", "gs"], vec![]),
        root("other", &["buyer"], vec![]),
    ]);
    let before = s.snapshot().content_id;
    let kind_of = |s: &Session| doc_of(s).choreographies[0].kind.clone();
    assert!(kind_of(&s).is_none());
    let update = |s: &mut Session, initiator: Option<ModelReference>| {
        let mut edited = doc_of(s).choreographies[0].clone();
        edited.kind = Some(ChoreographyRootKind::Global {
            initiating_participant_ref: initiator,
        });
        apply(
            s,
            vec![Mutation::UpdateChoreography {
                document_id,
                choreography: edited,
            }],
        );
    };
    let initiator = |s: &Session| match kind_of(s) {
        Some(ChoreographyRootKind::Global {
            initiating_participant_ref,
        }) => initiating_participant_ref,
        None => panic!("global expected"),
    };
    // edit: a legacy choreography becomes a globalChoreographyTask; undo restores.
    update(&mut s, Some(local("gb")));
    assert_eq!(initiator(&s), Some(local("gb")));
    assert!(
        !codes(&s)
            .iter()
            .any(|c| c.starts_with("globalChoreographyInitiator"))
    );
    update(&mut s, Some(local("gs")));
    assert_eq!(initiator(&s), Some(local("gs")));
    undo(&mut s);
    assert_eq!(initiator(&s), Some(local("gb")));
    // validate: unresolved and foreign-owner initiators are refused atomically.
    let accepted = s.snapshot().content_id;
    for bad in ["absent", "buyer"] {
        let mut edited = doc_of(&s).choreographies[0].clone();
        edited.kind = Some(ChoreographyRootKind::Global {
            initiating_participant_ref: Some(local(bad)),
        });
        assert!(!try_apply(
            &mut s,
            vec![Mutation::UpdateChoreography {
                document_id,
                choreography: edited,
            }],
        ));
        assert_eq!(initiator(&s), Some(local("gb")));
        assert_eq!(s.snapshot().content_id, accepted);
    }
    for _ in 0..1 {
        undo(&mut s);
    }
    assert!(kind_of(&s).is_none());
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn choreography_activity_fields_edit_with_undo() {
    let mut c = root(
        "main",
        &["buyer", "seller", "carrier"],
        vec![activity("task")],
    );
    c.message_flows = vec![
        message_flow("order", "buyer", "seller"),
        message_flow("reply", "seller", "buyer"),
    ];
    c.activities[0].message_flow_refs = vec![local("order")];
    let (mut s, document_id) = session_with(vec![c]);
    let before = s.snapshot().content_id;
    let task = |s: &Session| doc_of(s).choreographies[0].activities[0].clone();
    let mut edited = task(&s);
    edited.initiating_participant = Some(local("seller"));
    edited.participant_refs = vec![local("seller"), local("carrier")];
    edited.message_flow_refs = vec![local("order"), local("reply")];
    edited.loop_type = Some(ChoreographyLoopType::MultiInstanceParallel);
    apply(
        &mut s,
        vec![Mutation::UpdateChoreographyActivity {
            document_id,
            choreography_id: id("main"),
            activity: edited,
        }],
    );
    let t = task(&s);
    assert_eq!(t.initiating_participant, Some(local("seller")));
    assert_eq!(t.participant_refs, vec![local("seller"), local("carrier")]);
    assert_eq!(t.message_flow_refs, vec![local("order"), local("reply")]);
    assert_eq!(
        t.loop_type,
        Some(ChoreographyLoopType::MultiInstanceParallel)
    );
    undo(&mut s);
    let t = task(&s);
    assert_eq!(t.initiating_participant, Some(local("buyer")));
    assert_eq!(t.participant_refs, vec![local("buyer"), local("seller")]);
    assert_eq!(t.message_flow_refs, vec![local("order")]);
    assert_eq!(t.loop_type, None);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn participant_multiplicity_minimum_and_maximum_edit_with_undo() {
    let (mut s, document_id) = session_with(vec![root("main", &["buyer", "seller"], vec![])]);
    let before = s.snapshot().content_id;
    let multiplicity = |s: &Session| {
        doc_of(s).choreographies[0].participants[0]
            .multiplicity
            .clone()
    };
    for (min, max) in [(0, 2), (3, 7)] {
        apply(
            &mut s,
            vec![Mutation::SetParticipantMultiplicity {
                document_id,
                participant_id: id("buyer"),
                multiplicity: Some(ParticipantMultiplicity {
                    id: None,
                    base: None,
                    minimum: Some(min),
                    maximum: Some(max),
                }),
            }],
        );
        let m = multiplicity(&s).unwrap();
        assert_eq!((m.minimum, m.maximum), (Some(min), Some(max)));
    }
    undo(&mut s);
    let m = multiplicity(&s).unwrap();
    assert_eq!((m.minimum, m.maximum), (Some(0), Some(2)));
    undo(&mut s);
    assert!(multiplicity(&s).is_none());
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn choreography_reference_diagnostics_name_the_violated_clause() {
    let mut calls = vec![];
    for (name, target) in [("missing_call", "absent"), ("wrong_call", "buyer")] {
        let mut call = activity(name);
        call.details = Some(ChoreographyActivityDetails::Call {
            participant_associations: vec![],
            called_choreography_ref: Some(local(target)),
        });
        calls.push(call);
    }
    let mut global = root("global", &["gb"], vec![]);
    global.kind = Some(ChoreographyRootKind::Global {
        initiating_participant_ref: Some(local("buyer")),
    });
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.choreographies = vec![root("main", &["buyer", "seller"], calls), global];
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    let found = p.diagnostics();
    for code in [
        "calledChoreographyMissing",
        "calledChoreographyWrongType",
        "globalChoreographyInitiatorWrongOwner",
    ] {
        let diagnostic = found.iter().find(|d| d.code == code).expect(code);
        assert!(diagnostic.message.contains("BPMN 2.0.2 §11.5"), "{code}");
    }
    p.documents[0] = Arc::new({
        let mut d = (*p.documents[0]).clone();
        d.choreographies[1].kind = Some(ChoreographyRootKind::Global {
            initiating_participant_ref: Some(local("absent")),
        });
        d
    });
    assert!(
        p.diagnostics()
            .iter()
            .any(|d| d.code == "globalChoreographyInitiatorMissing")
    );
}
