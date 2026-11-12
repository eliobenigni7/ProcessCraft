use processcraft_core::{
    bpmn_activity::ActivityProperties,
    bpmn_model::{NodeDetails, QNameRef},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{ElementKey, XmlId},
    model::*,
    queries::find_node,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn run(s: &mut Session, request: Request) -> Result<(), CommandError> {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        request,
    )
    .map(|_| ())
}

fn apply(s: &mut Session, m: Vec<Mutation>) -> Result<(), CommandError> {
    run(s, Request::Batch { mutations: m })
}

fn node(kind: NodeKind, name: &str) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id(name),
        name: Some(name.into()),
        documentation: String::new().into(),
        kind,
        details: match kind {
            NodeKind::CallActivity => Some(NodeDetails::CallActivity {
                called_element: None,
            }),
            NodeKind::SubProcess => Some(NodeDetails::SubProcess {
                content: Box::default(),
            }),
            _ => None,
        },
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    }
}

fn session() -> (Session, ElementKey) {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![node(NodeKind::Task, "task")],
        lanes: vec![],
        flows: vec![],
    });
    let key = ElementKey {
        document_id: d.id,
        xml_id: id("task"),
    };
    p.documents.push(Arc::new(d));
    (Session::new(p).unwrap(), key)
}

fn current(s: &Session, name: &str) -> Node {
    find_node(
        &s.snapshot().project.documents[0],
        &id::<XmlId>(name).into(),
    )
    .unwrap()
    .clone()
}

fn set(key: &ElementKey, name: &str, activity: Option<ActivityProperties>) -> Mutation {
    Mutation::SetActivityProperties {
        document_id: key.document_id,
        element_id: id::<XmlId>(name).into(),
        activity,
    }
}

fn create(key: &ElementKey, kind: NodeKind, name: &str) -> Mutation {
    Mutation::CreateElement {
        document_id: key.document_id,
        process_id: id("process"),
        element: node(kind, name),
    }
}

fn undo(s: &mut Session) {
    run(s, Request::Undo).unwrap();
}

fn err_text(r: Result<(), CommandError>) -> String {
    match r.unwrap_err() {
        CommandError::InvalidParams(m) => m,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn completion_quantity_is_set_validated_and_undone() {
    let (mut s, key) = session();
    let before = s.snapshot().content_id;
    let props = ActivityProperties {
        completion_quantity: Some("4".into()),
        ..Default::default()
    };
    apply(&mut s, vec![set(&key, "task", Some(props))]).unwrap();
    assert_eq!(
        current(&s, "task")
            .activity
            .unwrap()
            .completion_quantity
            .as_deref(),
        Some("4")
    );
    let changed = s.snapshot().content_id;
    let bad = ActivityProperties {
        completion_quantity: Some("four".into()),
        ..Default::default()
    };
    let m = err_text(apply(&mut s, vec![set(&key, "task", Some(bad))]));
    assert!(m.contains("invalid activity quantity"), "{m}");
    assert_eq!(s.snapshot().content_id, changed);
    let bad_start = ActivityProperties {
        start_quantity: Some("1.5".into()),
        ..Default::default()
    };
    let m = err_text(apply(&mut s, vec![set(&key, "task", Some(bad_start))]));
    assert!(m.contains("invalid activity quantity"), "{m}");
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
    assert!(current(&s, "task").activity.is_none());
}

#[test]
fn call_activity_called_element_is_set_and_undone() {
    let (mut s, key) = session();
    apply(&mut s, vec![create(&key, NodeKind::CallActivity, "call")]).unwrap();
    let before = s.snapshot().content_id;
    let details = NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: "urn:other".into(),
            local: id("callee"),
        }),
    };
    apply(
        &mut s,
        vec![Mutation::SetNodeDetails {
            document_id: key.document_id,
            element_id: id::<XmlId>("call").into(),
            details: Some(details.clone()),
        }],
    )
    .unwrap();
    assert_eq!(current(&s, "call").details, Some(details.clone()));
    undo(&mut s);
    assert_eq!(s.snapshot().content_id, before);
    assert_ne!(current(&s, "call").details, Some(details));
}

#[test]
fn task_bindings_edit_and_kind_rules_validate() {
    let (mut s, key) = session();
    let before = s.snapshot().content_id;
    for (kind, name) in [
        (NodeKind::ReceiveTask, "receive"),
        (NodeKind::SendTask, "send"),
        (NodeKind::ServiceTask, "service"),
        (NodeKind::UserTask, "user"),
        (NodeKind::SubProcess, "sub"),
    ] {
        apply(&mut s, vec![create(&key, kind, name)]).unwrap();
    }
    let created = s.snapshot().content_id;
    let q = |local: &str| QNameRef {
        namespace: "urn:m".into(),
        local: id(local),
    };
    let receive = ActivityProperties {
        instantiate: Some(true),
        implementation: Some("##WebService".into()),
        message_ref: Some(q("request")),
        operation_ref: Some(q("lookup")),
        ..Default::default()
    };
    apply(&mut s, vec![set(&key, "receive", Some(receive.clone()))]).unwrap();
    assert_eq!(current(&s, "receive").activity, Some(receive));
    for name in ["send", "service", "user"] {
        let props = ActivityProperties {
            implementation: Some("##unspecified".into()),
            ..Default::default()
        };
        apply(&mut s, vec![set(&key, name, Some(props.clone()))]).unwrap();
        assert_eq!(current(&s, name).activity, Some(props));
    }
    let sub = ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    };
    apply(&mut s, vec![set(&key, "sub", Some(sub.clone()))]).unwrap();
    assert_eq!(current(&s, "sub").activity, Some(sub));
    let applied = s.snapshot().content_id;
    let cases: Vec<(&str, ActivityProperties, &str)> = vec![
        (
            "task",
            ActivityProperties {
                implementation: Some("x".into()),
                ..Default::default()
            },
            "implementation on unsupported activity",
        ),
        (
            "service",
            ActivityProperties {
                message_ref: Some(q("request")),
                ..Default::default()
            },
            "message reference requires send or receive task",
        ),
        (
            "user",
            ActivityProperties {
                operation_ref: Some(q("lookup")),
                ..Default::default()
            },
            "operation reference requires service, send or receive task",
        ),
        (
            "send",
            ActivityProperties {
                instantiate: Some(true),
                ..Default::default()
            },
            "instantiate requires receive task",
        ),
        (
            "task",
            ActivityProperties {
                triggered_by_event: Some(true),
                ..Default::default()
            },
            "event trigger requires subprocess",
        ),
    ];
    for (name, props, expected) in cases {
        let m = err_text(apply(&mut s, vec![set(&key, name, Some(props))]));
        assert!(m.contains(expected), "{name}: {m}");
        assert_eq!(s.snapshot().content_id, applied);
    }
    // Undo the five property edits: back to the just-created content, then creations.
    for _ in 0..5 {
        undo(&mut s);
    }
    assert_eq!(s.snapshot().content_id, created);
    assert!(current(&s, "receive").activity.is_none());
    for _ in 0..5 {
        undo(&mut s);
    }
    assert_eq!(s.snapshot().content_id, before);
}
