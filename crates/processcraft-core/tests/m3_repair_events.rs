//! Repair evidence for the events family: value read-backs, undo, and specific diagnostics.
use processcraft_core::{
    bpmn_data::{DataPort, IoSet},
    bpmn_model::*,
    commands::{Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
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
fn node(s: &Session, name: &str) -> Node {
    let snap = s.snapshot();
    processcraft_core::queries::find_node(&snap.project.documents[0], &id::<ElementSelector>(name))
        .unwrap()
        .clone()
}
fn defs(s: &Session, name: &str) -> Vec<EventDefinition> {
    let Some(NodeDetails::Event { definitions, .. }) = node(s, name).details else {
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
fn set(name: &str, d: NodeDetails) -> Mutation {
    Mutation::SetNodeDetails {
        document_id: docid(),
        element_id: id(name),
        details: Some(d),
    }
}
fn create_event(name: &str, kind: NodeKind) -> Mutation {
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
            kind,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        },
    }
}
fn local(s: &str) -> EventReference {
    EventReference::Local(id(s))
}
fn signal_resource(rid: &str) -> EventResource {
    EventResource::Signal {
        id: id(rid),
        base: None,
        name: Some("S".into()),
        structure_ref: None,
    }
}
fn message_resource(rid: &str) -> EventResource {
    EventResource::Message {
        id: id(rid),
        base: None,
        name: Some(rid.into()),
        item_ref: None,
    }
}
fn message(def: &str, m: Option<&str>) -> EventDefinition {
    EventDefinition::Message {
        id: Some(id(def)),
        base: None,
        message_ref: m.map(local),
        operation_ref: None,
    }
}
fn link(idn: &str, name: &str, sources: Vec<&str>, target: Option<&str>) -> EventDefinition {
    EventDefinition::Link {
        id: Some(id(idn)),
        base: None,
        name: name.into(),
        sources: sources.into_iter().map(local).collect(),
        target: target.map(local),
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
/// Content built outside the command pipeline, as a legacy file would be.
fn project_with(node_id: &str, kind: NodeKind, d: NodeDetails) -> ProjectData {
    let mut p = (*base().snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
    let n = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<ElementIdentity>(node_id))
        .unwrap();
    n.kind = kind;
    n.details = Some(d);
    p
}

#[test]
fn start_event_is_interrupting_set_clear_and_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let start = |v: Option<bool>| {
        let mut d = ev(vec![], vec![]);
        if let NodeDetails::Event {
            is_interrupting, ..
        } = &mut d
        {
            *is_interrupting = v;
        }
        d
    };
    let read = |s: &Session| {
        let Some(NodeDetails::Event {
            is_interrupting, ..
        }) = node(s, "start").details
        else {
            panic!()
        };
        is_interrupting
    };
    for v in [Some(false), Some(true), None] {
        apply(&mut s, vec![set("start", start(v))]).unwrap();
        assert_eq!(read(&s), v);
    }
    undo(&mut s);
    assert_eq!(read(&s), Some(true));
    undo(&mut s);
    assert_eq!(read(&s), Some(false));
    // outside an event subprocess the flag is ignored: specific advisory code
    assert!(codes(&s).contains(&"startInterruptingIgnored"));
    undo(&mut s);
    assert!(!codes(&s).contains(&"startInterruptingIgnored"));
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn message_ref_set_change_clear_and_undo() {
    let mut s = base();
    apply(
        &mut s,
        vec![Mutation::SetEventResources {
            document_id: docid(),
            resources: vec![message_resource("m1"), message_resource("m2")],
        }],
    )
    .unwrap();
    let before = s.snapshot().content_id;
    for m in [Some("m1"), Some("m2"), None] {
        apply(
            &mut s,
            vec![set("start", ev(vec![message("md", m)], vec![]))],
        )
        .unwrap();
        assert_eq!(defs(&s, "start"), vec![message("md", m)]);
    }
    undo(&mut s);
    assert_eq!(defs(&s, "start"), vec![message("md", Some("m2"))]);
    undo(&mut s);
    assert_eq!(defs(&s, "start"), vec![message("md", Some("m1"))]);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn link_name_edit_and_throw_without_target_is_diagnosed() {
    let mut s = base();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        vec![
            create_event("lt", NodeKind::IntermediateThrowEvent),
            create_event("lc", NodeKind::IntermediateCatchEvent),
            set(
                "lt",
                ev(
                    vec![link("l_throw", "jump", vec![], Some("l_catch"))],
                    vec![],
                ),
            ),
            set(
                "lc",
                ev(vec![link("l_catch", "jump", vec!["l_throw"], None)], vec![]),
            ),
        ],
    )
    .unwrap();
    let after_create = s.snapshot().content_id;
    assert!(!codes(&s).contains(&"incompleteLinkPairing"));
    apply(
        &mut s,
        vec![
            set(
                "lt",
                ev(
                    vec![link("l_throw", "leap", vec![], Some("l_catch"))],
                    vec![],
                ),
            ),
            set(
                "lc",
                ev(vec![link("l_catch", "leap", vec!["l_throw"], None)], vec![]),
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        defs(&s, "lt"),
        vec![link("l_throw", "leap", vec![], Some("l_catch"))]
    );
    assert_eq!(
        defs(&s, "lc"),
        vec![link("l_catch", "leap", vec!["l_throw"], None)]
    );
    undo(&mut s);
    assert_eq!(
        defs(&s, "lt"),
        vec![link("l_throw", "jump", vec![], Some("l_catch"))]
    );
    assert_eq!(s.snapshot().content_id, after_create);
    // a throw link with a name but no target is incomplete; a target clears it
    apply(
        &mut s,
        vec![set(
            "lt",
            ev(vec![link("l_throw", "jump", vec![], None)], vec![]),
        )],
    )
    .unwrap();
    assert!(codes(&s).contains(&"incompleteLinkPairing"));
    undo(&mut s);
    assert!(!codes(&s).contains(&"incompleteLinkPairing"));
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn signal_definition_reads_back_and_dangling_signal_ref_reports_event_signal_ref() {
    let mut s = base();
    let sig = |r: &str| EventDefinition::Signal {
        id: Some(id("sd")),
        base: None,
        signal_ref: Some(local(r)),
    };
    apply(
        &mut s,
        vec![
            Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![signal_resource("sig"), message_resource("msg")],
            },
            set("end", ev(vec![sig("sig")], vec![])),
        ],
    )
    .unwrap();
    assert_eq!(defs(&s, "end"), vec![sig("sig")]);
    s.snapshot().project.validate().unwrap();
    // missing resource, and a resource of the wrong kind (a message used as a signal)
    for bad in ["nowhere", "msg"] {
        let p = project_with("end", NodeKind::EndEvent, ev(vec![sig(bad)], vec![]));
        assert_eq!(
            p.validate().unwrap_err(),
            "Missing or wrong-kind local event resource",
            "{bad}"
        );
    }
}

#[test]
fn catch_and_throw_event_definition_ref_diagnostics_are_specific() {
    let root = |i: &str| EventDefinition::Signal {
        id: Some(id(i)),
        base: None,
        signal_ref: None,
    };
    let mut s = base();
    apply(
        &mut s,
        vec![
            Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![root("shared")],
            },
            create_event("catch", NodeKind::IntermediateCatchEvent),
            create_event("throw", NodeKind::IntermediateThrowEvent),
            set("catch", ev(vec![], vec![local("shared")])),
            set("throw", ev(vec![], vec![local("shared")])),
        ],
    )
    .unwrap();
    s.snapshot().project.validate().unwrap();
    for (owner, kind) in [
        ("catch", NodeKind::IntermediateCatchEvent),
        ("throw", NodeKind::IntermediateThrowEvent),
        ("end", NodeKind::EndEvent),
    ] {
        // (a) missing root, (b) a local id that is not an event definition
        for bad in ["nowhere", "review"] {
            let mut p = (*s.snapshot().project).clone();
            let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
            let n = doc.processes[0]
                .nodes
                .iter_mut()
                .find(|n| n.id == id::<ElementIdentity>(owner))
                .unwrap();
            n.kind = kind;
            n.details = Some(ev(vec![], vec![local(bad)]));
            assert_eq!(
                p.validate().unwrap_err(),
                "Missing or wrong-kind root event definition",
                "{owner}/{bad}"
            );
        }
    }
    // (c) repeated use of one root definition on the same event is refused atomically
    let before = s.snapshot().content_id;
    let err = apply(
        &mut s,
        vec![set(
            "throw",
            ev(vec![], vec![local("shared"), local("shared")]),
        )],
    )
    .unwrap_err();
    assert_eq!(s.snapshot().content_id, before);
    assert!(
        format!("{err:?}").contains("Duplicate event definition reference"),
        "{err:?}"
    );
}

#[test]
fn throw_and_end_events_read_back_refs_and_io_with_undo() {
    let mut s = base();
    let before = s.snapshot().content_id;
    let mk_io = |tag: &str| EventIo::Throw {
        inputs: vec![DataPort {
            id: Some(id(&format!("tp_{tag}"))),
            base: None,
            name: Some("payload".into()),
            is_collection: Some(false),
            item_aware: None,
        }],
        input_set: Some(IoSet {
            id: Some(id(&format!("tset_{tag}"))),
            base: None,
            name: None,
            data_refs: vec![id(&format!("tp_{tag}"))],
            optional_refs: vec![],
            while_executing_refs: vec![],
            counterpart_refs: vec![],
        }),
    };
    let details = |refs: Vec<EventReference>, tag: &str| {
        let mut d = ev(vec![], refs);
        if let NodeDetails::Event {
            parallel_multiple,
            properties,
            io: slot,
            ..
        } = &mut d
        {
            *parallel_multiple = None;
            *properties = vec![EventProperty {
                base: None,
                id: Some(id(&format!("prop_{tag}"))),
                name: Some("p".into()),
                item_aware: None,
            }];
            *slot = Some(mk_io(tag));
        }
        d
    };
    apply(
        &mut s,
        vec![
            Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![EventDefinition::Signal {
                    id: Some(id("shared")),
                    base: None,
                    signal_ref: None,
                }],
            },
            create_event("throw", NodeKind::IntermediateThrowEvent),
            set("throw", details(vec![local("shared")], "throw")),
            set("end", details(vec![local("shared")], "end")),
        ],
    )
    .unwrap();
    for name in ["throw", "end"] {
        let n = node(&s, name);
        let Some(NodeDetails::Event {
            definition_refs,
            parallel_multiple,
            properties,
            io: got_io,
            ..
        }) = n.details
        else {
            panic!()
        };
        assert_eq!(definition_refs, vec![local("shared")], "{name}");
        assert_eq!(parallel_multiple, None);
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].id, Some(id(&format!("prop_{name}"))));
        assert_eq!(got_io, Some(mk_io(name)), "{name}");
    }
    assert_eq!(node(&s, "throw").kind, NodeKind::IntermediateThrowEvent);
    assert_eq!(node(&s, "end").kind, NodeKind::EndEvent);
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn root_event_definitions_read_back_and_invalid_event_messages_are_specific() {
    let mut s = base();
    let timer = EventDefinition::Timer {
        id: Some(id("timer")),
        base: None,
        kind: Some(TimerKind::Duration),
        expression: Some(Expression::Legacy("PT1H".into())),
    };
    apply(
        &mut s,
        vec![Mutation::SetRootEventDefinitions {
            document_id: docid(),
            definitions: vec![timer.clone()],
        }],
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.documents[0].root_event_definitions,
        vec![timer.clone()]
    );
    let mut dup = (*s.snapshot().project).clone();
    std::sync::Arc::make_mut(&mut dup.documents[0])
        .root_event_definitions
        .push(timer);
    assert_eq!(dup.validate().unwrap_err(), "duplicate XML ID");
    let mut d = ev(vec![], vec![]);
    if let NodeDetails::Event { io, .. } = &mut d {
        *io = Some(EventIo::Catch {
            outputs: vec![],
            output_set: None,
        });
    }
    assert_eq!(
        project_with("end", NodeKind::EndEvent, d)
            .validate()
            .unwrap_err(),
        "Event I/O direction does not match event kind"
    );
    let cancel = EventDefinition::Cancel {
        id: None,
        base: None,
    };
    assert_eq!(
        project_with("end", NodeKind::EndEvent, ev(vec![cancel], vec![]))
            .validate()
            .unwrap_err(),
        "Cancel definition requires an interrupting transaction boundary or transaction end event"
    );
    // parallelMultiple is catch-only
    let mut d = ev(vec![], vec![]);
    if let NodeDetails::Event {
        parallel_multiple, ..
    } = &mut d
    {
        *parallel_multiple = Some(true);
    }
    let r = apply(&mut s, vec![set("end", d)]).unwrap_err();
    assert!(
        format!("{r:?}").contains("parallelMultiple requires a catch event"),
        "{r:?}"
    );
}
