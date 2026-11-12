//! Round-3 repair evidence for the activities family (BPMN 2.0.2 section 10.3, Tables 10.8-10.12).
use processcraft_core::{
    bpmn_activity::{ActivityProperties, ModelReference},
    bpmn_model::{NodeDetails, QNameRef},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementSelector, XmlId},
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
fn node(kind: NodeKind, name: &str) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id(name),
        name: Some(name.into()),
        documentation: String::new().into(),
        kind,
        details: matches!(kind, NodeKind::SubProcess).then(|| NodeDetails::SubProcess {
            content: Box::default(),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    }
}
fn flow(name: &str, source: &str, target: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(ModelReference::Local(id(source))),
        target: Some(ModelReference::Local(id(target))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    }
}
fn process(nodes: Vec<Node>, flows: Vec<Flow>) -> Process {
    Process {
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
        nodes,
        lanes: vec![],
        flows,
    }
}
fn project(p: Process) -> ProjectData {
    let mut project = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.processes.push(p);
    project.documents.push(Arc::new(d));
    project
}
fn session(p: Process) -> (Session, DocumentId) {
    let project = project(p);
    let doc = project.documents[0].id;
    (Session::new(project).unwrap(), doc)
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
fn set(doc: DocumentId, name: &str, activity: Option<ActivityProperties>) -> Request {
    Request::Batch {
        mutations: vec![Mutation::SetActivityProperties {
            document_id: doc,
            element_id: id::<XmlId>(name).into(),
            activity,
        }],
    }
}
fn err_text(r: Result<(), CommandError>) -> String {
    match r.unwrap_err() {
        CommandError::InvalidParams(m) => m,
        other => panic!("unexpected {other:?}"),
    }
}
fn current(s: &Session, name: &str) -> Node {
    find_node(
        &s.snapshot().project.documents[0],
        &id::<XmlId>(name).into(),
    )
    .unwrap()
    .clone()
}
fn q(local: &str) -> QNameRef {
    QNameRef {
        namespace: "urn:m".into(),
        local: id(local),
    }
}

#[test]
fn default_flow_must_be_outgoing_sequence_flow_of_the_activity() {
    let (mut s, doc) = session(process(
        vec![node(NodeKind::Task, "task"), node(NodeKind::Task, "other")],
        vec![
            flow("out", "task", "other"),
            flow("foreign", "other", "task"),
        ],
    ));
    let base = s.snapshot().content_id;
    let want = "activity default must be outgoing flow in same scope";
    for bad in ["missing", "foreign"] {
        let props = ActivityProperties {
            default_flow: Some(id::<ElementSelector>(bad)),
            ..Default::default()
        };
        assert_eq!(
            err_text(run(&mut s, set(doc, "task", Some(props)))),
            want,
            "{bad}"
        );
        assert_eq!(s.snapshot().content_id, base);
    }
    let ok = ActivityProperties {
        default_flow: Some(id("out")),
        ..Default::default()
    };
    run(&mut s, set(doc, "task", Some(ok.clone()))).unwrap();
    assert_eq!(current(&s, "task").activity, Some(ok));
    run(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, base);
}

#[test]
fn trigger_by_event_is_stored_on_subprocess_and_refused_elsewhere() {
    let (mut s, doc) = session(process(
        vec![
            node(NodeKind::Task, "task"),
            node(NodeKind::SubProcess, "sub"),
        ],
        vec![],
    ));
    let base = s.snapshot().content_id;
    let props = ActivityProperties {
        triggered_by_event: Some(true),
        ..Default::default()
    };
    assert_eq!(
        err_text(run(&mut s, set(doc, "task", Some(props.clone())))),
        "event trigger requires subprocess"
    );
    assert_eq!(s.snapshot().content_id, base);
    run(&mut s, set(doc, "sub", Some(props))).unwrap();
    assert_eq!(
        current(&s, "sub").activity.unwrap().triggered_by_event,
        Some(true)
    );
    run(&mut s, Request::Undo).unwrap();
    assert!(current(&s, "sub").activity.is_none());
    assert_eq!(s.snapshot().content_id, base);
}

#[test]
fn send_task_message_ref_is_stored_cleared_and_undone() {
    let (mut s, doc) = session(process(vec![node(NodeKind::SendTask, "send")], vec![]));
    let base = s.snapshot().content_id;
    let props = ActivityProperties {
        message_ref: Some(q("request")),
        ..Default::default()
    };
    run(&mut s, set(doc, "send", Some(props))).unwrap();
    assert_eq!(
        current(&s, "send").activity.unwrap().message_ref,
        Some(q("request"))
    );
    let bound = s.snapshot().content_id;
    run(
        &mut s,
        set(doc, "send", Some(ActivityProperties::default())),
    )
    .unwrap();
    assert_eq!(current(&s, "send").activity.unwrap().message_ref, None);
    run(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, bound);
    run(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, base);
    assert!(current(&s, "send").activity.is_none());
}

#[test]
fn implementation_value_diagnostic_on_send_user_service_and_refusal_on_plain_task() {
    for kind in [
        NodeKind::SendTask,
        NodeKind::UserTask,
        NodeKind::ServiceTask,
    ] {
        let with = |implementation: &str| {
            let mut n = node(kind, "t");
            n.activity = Some(ActivityProperties {
                implementation: Some(implementation.into()),
                ..Default::default()
            });
            project(process(vec![n], vec![])).diagnostics()
        };
        for ok in ["##unspecified", "##WebService", "http://example.com/rpc"] {
            assert!(
                with(ok)
                    .iter()
                    .all(|d| d.code != "activityImplementationInvalid"),
                "{kind:?} {ok}"
            );
        }
        for bad in ["##Carrier", "not a uri"] {
            let found: Vec<_> = with(bad)
                .into_iter()
                .filter(|d| d.code == "activityImplementationInvalid")
                .collect();
            assert_eq!(found.len(), 1, "{kind:?} {bad}");
            assert_eq!(found[0].element_id, Some(id::<ElementSelector>("t")));
        }
    }
    let (mut s, doc) = session(process(vec![node(NodeKind::Task, "task")], vec![]));
    let props = ActivityProperties {
        implementation: Some("##WebService".into()),
        ..Default::default()
    };
    assert_eq!(
        err_text(run(&mut s, set(doc, "task", Some(props)))),
        "implementation on unsupported activity"
    );
}

#[test]
fn subprocess_scope_rejects_duplicate_ids_and_cross_scope_flows() {
    let nested = |inner: Node, flows: Vec<Flow>| {
        let mut sub = node(NodeKind::SubProcess, "sub");
        let Some(NodeDetails::SubProcess { content }) = &mut sub.details else {
            unreachable!()
        };
        content.nodes.push(inner);
        content.flows = flows;
        project(process(vec![node(NodeKind::Task, "outer"), sub], vec![]))
    };
    assert!(
        nested(node(NodeKind::Task, "inner"), vec![])
            .validate()
            .is_ok()
    );
    assert_eq!(
        nested(node(NodeKind::Task, "outer"), vec![])
            .validate()
            .unwrap_err(),
        "duplicate XML ID"
    );
    let cross = nested(
        node(NodeKind::Task, "inner"),
        vec![flow("f", "inner", "outer")],
    )
    .validate()
    .unwrap_err();
    assert_eq!(cross, "invalid process flow endpoints");
}
