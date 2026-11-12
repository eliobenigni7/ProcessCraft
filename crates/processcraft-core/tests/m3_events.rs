use processcraft_core::{
    bpmn_catalog::{Interface, Operation},
    bpmn_model::*,
    commands::{Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
    validation::activity_family_reference_issues,
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    id("11111111-1111-4111-8111-111111111111")
}
fn run(s: &mut Session, r: Request) -> Result<(), processcraft_core::commands::CommandError> {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, r).map(|_| ())
}
fn apply(
    s: &mut Session,
    m: Vec<Mutation>,
) -> Result<(), processcraft_core::commands::CommandError> {
    run(s, Request::Batch { mutations: m })
}
fn undo(s: &mut Session) {
    run(s, Request::Undo).unwrap();
}
fn base() -> Session {
    let mut s = Session::new(ProjectData::new("P".into())).unwrap();
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    run(&mut s, r).unwrap();
    s
}
/// Session whose document already holds message "msg" and interface "svc" with operation "op".
fn with_operation() -> Session {
    let mut p = (*base().snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
    doc.catalog.interfaces.push(Interface {
        base: None,
        id: Some(id("svc")),
        name: "Svc".into(),
        implementation_ref: None,
        operations: vec![Operation {
            base: None,
            id: Some(id("op")),
            name: "Op".into(),
            implementation_ref: None,
            in_message_ref: QNameRef {
                namespace: GENERATED_TARGET_NAMESPACE.into(),
                local: id("msg"),
            },
            out_message_ref: None,
            error_refs: vec![],
        }],
    });
    doc.event_resources.push(EventResource::Message {
        id: id("msg"),
        base: None,
        name: Some("Msg".into()),
        item_ref: None,
    });
    Session::new(p).unwrap()
}
fn node(s: &Session, name: &str) -> Node {
    let snap = s.snapshot();
    processcraft_core::queries::find_node(&snap.project.documents[0], &id::<ElementSelector>(name))
        .unwrap()
        .clone()
}
fn details(s: &Session, name: &str) -> NodeDetails {
    node(s, name).details.unwrap()
}
fn defs(s: &Session, name: &str) -> Vec<EventDefinition> {
    let NodeDetails::Event { definitions, .. } = details(s, name) else {
        panic!("event details expected")
    };
    definitions
}
fn ev(defs: Vec<EventDefinition>, refs: Vec<EventReference>) -> NodeDetails {
    NodeDetails::Event {
        definitions: defs,
        definition_refs: refs,
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    }
}
fn boundary(attached: &str, cancel: Option<bool>, defs: Vec<EventDefinition>) -> NodeDetails {
    NodeDetails::Event {
        definitions: defs,
        definition_refs: vec![],
        attached_to: Some(EventReference::Local(id(attached))),
        cancel_activity: cancel,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    }
}
fn set(name: &str, d: NodeDetails) -> Mutation {
    Mutation::SetNodeDetails {
        document_id: docid(),
        element_id: id(name),
        details: Some(d),
    }
}
fn create_boundary(name: &str) -> Mutation {
    Mutation::CreateElement {
        document_id: docid(),
        process_id: id("procurement"),
        element: Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            id: id::<ElementIdentity>(name),
            name: None,
            documentation: String::new().into(),
            kind: NodeKind::BoundaryEvent,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        },
    }
}
fn timer(kind: TimerKind, text: &str) -> EventDefinition {
    EventDefinition::Timer {
        id: None,
        base: None,
        kind: Some(kind),
        expression: Some(Expression::Legacy(text.into())),
    }
}
fn has_issue(s: &Session, field: &str) -> bool {
    let snap = s.snapshot();
    activity_family_reference_issues(&snap.project.documents[0])
        .iter()
        .any(|i| i.field == field)
}
/// A dangling local reference is refused atomically; `field` names the clause the issue scan reports
/// for the same content built outside the command pipeline (see `dangling_event_references_report_their_clause`).
fn bad_edit_is_refused(s: &mut Session, m: Mutation, field: &str) {
    let before = s.snapshot().content_id;
    let err = apply(s, vec![m]).unwrap_err();
    assert!(
        format!("{err:?}").contains("unresolved") || format!("{err:?}").contains("nvalid"),
        "{field}: {err:?}"
    );
    assert_eq!(s.snapshot().content_id, before, "{field}");
}

#[test]
fn boundary_cancel_activity_and_attached_to_edit_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let t = || vec![timer(TimerKind::Duration, "PT5M")];
    apply(
        &mut s,
        vec![
            create_boundary("bound"),
            set("bound", boundary("review", Some(false), t())),
        ],
    )
    .unwrap();
    assert_eq!(node(&s, "bound").kind, NodeKind::BoundaryEvent);
    let NodeDetails::Event {
        attached_to,
        cancel_activity,
        ..
    } = details(&s, "bound")
    else {
        panic!()
    };
    assert_eq!(attached_to, Some(EventReference::Local(id("review"))));
    assert_eq!(cancel_activity, Some(false));
    apply(
        &mut s,
        vec![set("bound", boundary("order", Some(true), t()))],
    )
    .unwrap();
    let NodeDetails::Event {
        attached_to,
        cancel_activity,
        ..
    } = details(&s, "bound")
    else {
        panic!()
    };
    assert_eq!(attached_to, Some(EventReference::Local(id("order"))));
    assert_eq!(cancel_activity, Some(true));
    undo(&mut s);
    let NodeDetails::Event {
        attached_to,
        cancel_activity,
        ..
    } = details(&s, "bound")
    else {
        panic!()
    };
    assert_eq!(attached_to, Some(EventReference::Local(id("review"))));
    assert_eq!(cancel_activity, Some(false));
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn boundary_attached_to_unknown_or_non_activity_is_refused() {
    let mut s = base();
    apply(
        &mut s,
        vec![
            create_boundary("bound"),
            set(
                "bound",
                boundary("review", None, vec![timer(TimerKind::Duration, "PT1M")]),
            ),
        ],
    )
    .unwrap();
    assert!(!has_issue(&s, "boundaryEvent.attachedToRef"));
    for target in ["nowhere", "decision"] {
        bad_edit_is_refused(
            &mut s,
            set(
                "bound",
                boundary(target, None, vec![timer(TimerKind::Duration, "PT1M")]),
            ),
            "boundaryEvent.attachedToRef",
        );
    }
}

#[test]
fn timer_date_duration_cycle_edit_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    for (kind, text) in [
        (TimerKind::Date, "2026-11-12T10:00:00Z"),
        (TimerKind::Duration, "PT5M"),
        (TimerKind::Cycle, "R3/PT5M"),
    ] {
        apply(
            &mut s,
            vec![set("start", ev(vec![timer(kind, text)], vec![]))],
        )
        .unwrap();
        assert_eq!(defs(&s, "start"), vec![timer(kind, text)]);
    }
    undo(&mut s);
    assert_eq!(defs(&s, "start"), vec![timer(TimerKind::Duration, "PT5M")]);
    undo(&mut s);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn terminate_definition_edit_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let t = EventDefinition::Terminate {
        id: Some(id("term")),
        base: None,
    };
    apply(&mut s, vec![set("end", ev(vec![t.clone()], vec![]))]).unwrap();
    assert_eq!(defs(&s, "end"), vec![t]);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn throw_event_definition_ref_edit_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let sig = EventDefinition::Signal {
        id: Some(id("shared")),
        base: None,
        signal_ref: None,
    };
    apply(
        &mut s,
        vec![
            Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![sig],
            },
            set("end", ev(vec![], vec![EventReference::Local(id("shared"))])),
        ],
    )
    .unwrap();
    assert_eq!(node(&s, "end").kind, NodeKind::EndEvent);
    let NodeDetails::Event {
        definition_refs, ..
    } = details(&s, "end")
    else {
        panic!()
    };
    assert_eq!(definition_refs, vec![EventReference::Local(id("shared"))]);
    bad_edit_is_refused(
        &mut s,
        set("end", ev(vec![], vec![EventReference::Local(id("gone"))])),
        "event.eventDefinitionRef",
    );
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

type Make = Box<dyn Fn(Option<EventReference>) -> EventDefinition>;

#[test]
fn signal_error_escalation_refs_edit_with_undo_and_dangling_is_refused() {
    let cases: Vec<(&str, EventResource, Make, &str)> = vec![
        (
            "sig",
            EventResource::Signal {
                id: id("sig"),
                base: None,
                name: Some("S".into()),
                structure_ref: None,
            },
            Box::new(|r| EventDefinition::Signal {
                id: Some(id("def")),
                base: None,
                signal_ref: r,
            }),
            "event.signalRef",
        ),
        (
            "err",
            EventResource::Error {
                id: id("err"),
                base: None,
                name: Some("E".into()),
                code: Some("E1".into()),
                structure_ref: None,
            },
            Box::new(|r| EventDefinition::Error {
                id: Some(id("def")),
                base: None,
                error_ref: r,
            }),
            "event.errorRef",
        ),
        (
            "esc",
            EventResource::Escalation {
                id: id("esc"),
                base: None,
                name: Some("X".into()),
                code: Some("S1".into()),
                structure_ref: None,
            },
            Box::new(|r| EventDefinition::Escalation {
                id: Some(id("def")),
                base: None,
                escalation_ref: r,
            }),
            "event.escalationRef",
        ),
    ];
    for (rid, resource, make, field) in cases {
        let mut s = base();
        let before = s.snapshot().content_id;
        let good = make(Some(EventReference::Local(id(rid))));
        apply(
            &mut s,
            vec![
                Mutation::SetEventResources {
                    document_id: docid(),
                    resources: vec![resource],
                },
                set("end", ev(vec![good.clone()], vec![])),
            ],
        )
        .unwrap();
        assert_eq!(defs(&s, "end"), vec![good], "{field}");
        assert!(!has_issue(&s, field), "{field}");
        let bad = make(Some(EventReference::Local(id("missing"))));
        bad_edit_is_refused(&mut s, set("end", ev(vec![bad], vec![])), field);
        undo(&mut s);
        assert_eq!(s.snapshot().content_id, before, "{field}");
    }
}

#[test]
fn message_ref_and_operation_ref_edit_with_undo_and_dangling_is_refused() {
    let mut s = with_operation();
    let before = s.snapshot().content_id;
    let def = |m: &str, o: &str| EventDefinition::Message {
        id: Some(id("mdef")),
        base: None,
        message_ref: Some(EventReference::Local(id(m))),
        operation_ref: Some(EventReference::Local(id(o))),
    };
    apply(
        &mut s,
        vec![set("start", ev(vec![def("msg", "op")], vec![]))],
    )
    .unwrap();
    assert_eq!(defs(&s, "start"), vec![def("msg", "op")]);
    assert!(!has_issue(&s, "event.messageRef") && !has_issue(&s, "event.operationRef"));
    for (m, o, field) in [
        ("nomsg", "op", "event.messageRef"),
        ("msg", "noop", "event.operationRef"),
    ] {
        bad_edit_is_refused(&mut s, set("start", ev(vec![def(m, o)], vec![])), field);
    }
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

fn create_event(name: &str, kind: NodeKind) -> Mutation {
    let Mutation::CreateElement {
        document_id,
        process_id,
        mut element,
    } = create_boundary(name)
    else {
        unreachable!()
    };
    element.kind = kind;
    Mutation::CreateElement {
        document_id,
        process_id,
        element,
    }
}
fn link(idn: &str, sources: Vec<&str>, target: Option<&str>) -> EventDefinition {
    EventDefinition::Link {
        id: Some(id(idn)),
        base: None,
        name: "jump".into(),
        sources: sources
            .into_iter()
            .map(|s| EventReference::Local(id(s)))
            .collect(),
        target: target.map(|t| EventReference::Local(id(t))),
    }
}
fn codes(s: &Session) -> Vec<&'static str> {
    s.snapshot()
        .project
        .diagnostics()
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn link_pair_edit_with_undo_and_incomplete_pairing_is_diagnosed() {
    let mut s = base();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        vec![
            create_event("lt", NodeKind::IntermediateThrowEvent),
            create_event("lc", NodeKind::IntermediateCatchEvent),
            set(
                "lt",
                ev(vec![link("l_throw", vec![], Some("l_catch"))], vec![]),
            ),
            set(
                "lc",
                ev(vec![link("l_catch", vec!["l_throw"], None)], vec![]),
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        defs(&s, "lt"),
        vec![link("l_throw", vec![], Some("l_catch"))]
    );
    assert_eq!(defs(&s, "lc"), vec![link("l_catch", vec!["l_throw"], None)]);
    assert!(!codes(&s).contains(&"incompleteLinkPairing"));
    // a catch link without sources and an unnamed link are incomplete
    apply(
        &mut s,
        vec![set("lc", ev(vec![link("l_catch", vec![], None)], vec![]))],
    )
    .unwrap();
    assert!(codes(&s).contains(&"incompleteLinkPairing"));
    undo(&mut s);
    assert!(!codes(&s).contains(&"incompleteLinkPairing"));
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn link_dangling_reference_is_refused() {
    let mut s = base();
    apply(
        &mut s,
        vec![
            create_event("lt", NodeKind::IntermediateThrowEvent),
            set("lt", ev(vec![link("l_throw", vec![], None)], vec![])),
        ],
    )
    .unwrap();
    bad_edit_is_refused(
        &mut s,
        set(
            "lt",
            ev(vec![link("l_throw", vec![], Some("nowhere"))], vec![]),
        ),
        "event.linkRef",
    );
}

#[test]
fn conditional_definition_edit_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let cond = |t: &str| EventDefinition::Conditional {
        id: Some(id("c1")),
        base: None,
        expression: Expression::Legacy(t.into()),
    };
    apply(
        &mut s,
        vec![set("start", ev(vec![cond("amount > 10")], vec![]))],
    )
    .unwrap();
    assert_eq!(defs(&s, "start"), vec![cond("amount > 10")]);
    apply(
        &mut s,
        vec![set("start", ev(vec![cond("amount > 99")], vec![]))],
    )
    .unwrap();
    assert_eq!(defs(&s, "start"), vec![cond("amount > 99")]);
    undo(&mut s);
    assert_eq!(defs(&s, "start"), vec![cond("amount > 10")]);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

/// Content built outside the command pipeline (e.g. a legacy file) is reported with the clause.
#[test]
fn dangling_event_references_report_their_clause() {
    let missing = || Some(EventReference::Local(id("nowhere")));
    let cases: Vec<(&str, NodeDetails)> = vec![
        (
            "event.messageRef",
            ev(
                vec![EventDefinition::Message {
                    id: Some(id("d")),
                    base: None,
                    message_ref: missing(),
                    operation_ref: None,
                }],
                vec![],
            ),
        ),
        (
            "event.operationRef",
            ev(
                vec![EventDefinition::Message {
                    id: Some(id("d")),
                    base: None,
                    message_ref: None,
                    operation_ref: missing(),
                }],
                vec![],
            ),
        ),
        (
            "event.signalRef",
            ev(
                vec![EventDefinition::Signal {
                    id: Some(id("d")),
                    base: None,
                    signal_ref: missing(),
                }],
                vec![],
            ),
        ),
        (
            "event.errorRef",
            ev(
                vec![EventDefinition::Error {
                    id: Some(id("d")),
                    base: None,
                    error_ref: missing(),
                }],
                vec![],
            ),
        ),
        (
            "event.escalationRef",
            ev(
                vec![EventDefinition::Escalation {
                    id: Some(id("d")),
                    base: None,
                    escalation_ref: missing(),
                }],
                vec![],
            ),
        ),
        (
            "event.linkRef",
            ev(vec![link("d", vec![], Some("nowhere"))], vec![]),
        ),
        (
            "event.eventDefinitionRef",
            ev(vec![], vec![EventReference::Local(id("nowhere"))]),
        ),
        (
            "boundaryEvent.attachedToRef",
            boundary("nowhere", None, vec![]),
        ),
        (
            "boundaryEvent.attachedToRef",
            boundary("decision", None, vec![]),
        ),
    ];
    let msgs = [
        "Missing or wrong-kind local event resource",
        "Missing or wrong-kind local event operation",
        "Missing or wrong-kind local event resource",
        "Missing or wrong-kind local event resource",
        "Missing or wrong-kind local event resource",
        "Invalid or cross-scope link reference",
        "Missing or wrong-kind root event definition",
        "invalid boundary attachment",
        "invalid boundary attachment",
    ];
    for ((field, details), msg) in cases.into_iter().zip(msgs) {
        let mut p = (*base().snapshot().project).clone();
        let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
        let n = doc.processes[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == id::<ElementIdentity>("end"))
            .unwrap();
        n.kind = match field {
            "event.linkRef" => NodeKind::IntermediateThrowEvent,
            "boundaryEvent.attachedToRef" => NodeKind::BoundaryEvent,
            _ => NodeKind::EndEvent,
        };
        n.details = Some(details);
        let err = p.validate().unwrap_err();
        assert_eq!(err, msg, "{field}");
    }
}

fn validate_end_as(kind: NodeKind, d: EventDefinition) -> Result<(), String> {
    let mut p = (*base().snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
    let n = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<ElementIdentity>("end"))
        .unwrap();
    n.kind = kind;
    n.details = Some(ev(vec![d], vec![]));
    p.validate()
}
fn timer_of(kind: Option<TimerKind>, text: Option<&str>) -> EventDefinition {
    EventDefinition::Timer {
        id: None,
        base: None,
        kind,
        expression: text.map(|t| Expression::Legacy(t.into())),
    }
}

#[test]
fn timer_choice_must_be_complete_and_catch_only() {
    for kind in [TimerKind::Date, TimerKind::Duration, TimerKind::Cycle] {
        let catch = NodeKind::IntermediateCatchEvent;
        validate_end_as(catch, timer_of(Some(kind), Some("x"))).unwrap();
        assert_eq!(
            validate_end_as(catch, timer_of(Some(kind), None)).unwrap_err(),
            "Incomplete timer choice",
            "{kind:?}"
        );
        assert_eq!(
            validate_end_as(NodeKind::EndEvent, timer_of(Some(kind), Some("x"))).unwrap_err(),
            "timer definition requires a catch event",
            "{kind:?}"
        );
    }
    assert_eq!(
        validate_end_as(
            NodeKind::IntermediateCatchEvent,
            timer_of(None, Some("PT1M"))
        )
        .unwrap_err(),
        "Incomplete timer choice"
    );
}

#[test]
fn terminate_is_valid_only_on_end_events() {
    let t = || EventDefinition::Terminate {
        id: None,
        base: None,
    };
    validate_end_as(NodeKind::EndEvent, t()).unwrap();
    for kind in [NodeKind::IntermediateCatchEvent, NodeKind::StartEvent] {
        assert_eq!(
            validate_end_as(kind, t()).unwrap_err(),
            "unsupported event definition",
            "{kind:?}"
        );
    }
}

#[test]
fn link_without_name_is_diagnosed() {
    let codes_for = |name: &str| {
        let mut p = (*base().snapshot().project).clone();
        let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
        let n = &mut doc.processes[0].nodes[0];
        n.kind = NodeKind::IntermediateThrowEvent;
        n.details = Some(ev(
            vec![EventDefinition::Link {
                id: Some(id("l")),
                base: None,
                name: name.into(),
                sources: vec![],
                target: Some(EventReference::Local(id("l"))),
            }],
            vec![],
        ));
        p.diagnostics().iter().map(|d| d.code).collect::<Vec<_>>()
    };
    assert!(!codes_for("Jump").contains(&"incompleteLinkPairing"));
    assert!(codes_for("  ").contains(&"incompleteLinkPairing"));
}
