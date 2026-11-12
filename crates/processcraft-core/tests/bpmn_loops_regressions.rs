use processcraft_core::{
    bpmn_model::LoopCharacteristics,
    commands::{Envelope, Mutation, Request, Session},
    ids::ElementKey,
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
        lane_set_name: None,
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
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
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

fn std_loop(
    test_before: Option<bool>,
    cond: Option<&str>,
    max: Option<&str>,
) -> LoopCharacteristics {
    LoopCharacteristics::Standard {
        id: None,
        base: None,
        test_before,
        condition: cond.map(Into::into),
        maximum: max.map(Into::into),
    }
}

fn run(session: &mut Session, name: &str, req: Request) {
    let env = Envelope {
        request_id: name.into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session.apply(env, req).unwrap();
}

fn set(session: &mut Session, key: &ElementKey, v: Option<LoopCharacteristics>) {
    run(
        session,
        "set",
        Request::Batch {
            mutations: vec![Mutation::SetNodeLoop {
                document_id: key.document_id,
                element_id: key.xml_id.clone(),
                loop_characteristics: v,
            }],
        },
    );
}

fn stored(session: &Session, key: &ElementKey) -> Option<LoopCharacteristics> {
    find_node(&session.snapshot().project.documents[0], &key.xml_id)
        .unwrap()
        .loop_characteristics
        .clone()
}

/// standardLoopCharacteristics: set, edit, clear, then undo each step back to the prior value
/// and content identity (BPMN 2.0.2 Table 10.28).
#[test]
fn standard_loop_set_edit_clear_each_undo_to_previous_value_and_identity() {
    let (mut s, key) = fixture();
    let c0 = s.snapshot().content_id;
    let a = std_loop(Some(true), Some("again"), Some("5"));
    let b = std_loop(Some(false), Some("retry"), None);

    set(&mut s, &key, Some(a.clone()));
    let c1 = s.snapshot().content_id;
    assert_eq!(stored(&s, &key), Some(a.clone()));
    set(&mut s, &key, Some(b.clone()));
    let c2 = s.snapshot().content_id;
    let Some(LoopCharacteristics::Standard {
        test_before,
        condition,
        maximum,
        ..
    }) = stored(&s, &key)
    else {
        panic!("standard loop expected");
    };
    assert_eq!(test_before, Some(false));
    assert_eq!(condition.unwrap().plain_text(), Some("retry"));
    assert_eq!(maximum, None);
    assert!(c2 != c1 && c1 != c0);

    set(&mut s, &key, None);
    assert_eq!(stored(&s, &key), None);
    assert_eq!(s.snapshot().content_id, c0);

    run(&mut s, "u1", Request::Undo);
    assert_eq!(stored(&s, &key), Some(b));
    assert_eq!(s.snapshot().content_id, c2);
    run(&mut s, "u2", Request::Undo);
    assert_eq!(stored(&s, &key), Some(a));
    assert_eq!(s.snapshot().content_id, c1);
    run(&mut s, "u3", Request::Undo);
    assert_eq!(stored(&s, &key), None);
    assert_eq!(s.snapshot().content_id, c0);
}
