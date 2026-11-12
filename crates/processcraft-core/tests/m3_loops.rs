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

/// Apply one SetNodeLoop, assert the stored value equals `value`, then undo and assert the
/// content identity returns to the original.
fn set_read_undo(value: LoopCharacteristics) {
    let (mut session, key) = fixture();
    let original = session.snapshot().content_id;
    let e = |n: &str, s: &Session| Envelope {
        request_id: n.into(),
        session_id: s.id(),
        expected_generation: None,
    };
    session
        .apply(
            e("set", &session),
            Request::Batch {
                mutations: vec![Mutation::SetNodeLoop {
                    document_id: key.document_id,
                    element_id: key.xml_id.clone(),
                    loop_characteristics: Some(value.clone()),
                }],
            },
        )
        .unwrap();
    let stored = find_node(&session.snapshot().project.documents[0], &key.xml_id)
        .unwrap()
        .loop_characteristics
        .clone();
    assert_eq!(stored, Some(value));
    assert_ne!(session.snapshot().content_id, original);
    session.apply(e("undo", &session), Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, original);
    assert_eq!(
        find_node(&session.snapshot().project.documents[0], &key.xml_id)
            .unwrap()
            .loop_characteristics,
        None
    );
}

fn loop_json(v: serde_json::Value) -> LoopCharacteristics {
    serde_json::from_value(v).unwrap()
}

#[test]
fn multi_instance_data_items_completion_and_behavior_events_set_read_and_undo() {
    let value = loop_json(serde_json::json!({
        "type": "multiInstance",
        "is_sequential": true,
        "behavior": "One",
        "loop_data_input_ref": {"namespace": "urn:ext", "local": "TaskIn"},
        "loop_data_output_ref": {"namespace": "urn:ext", "local": "TaskOut"},
        "input_data_item": {"id": "IterIn", "name": "item", "is_collection": false},
        "output_data_item": {"id": "IterOut", "name": "result"},
        "one_behavior_event_ref": {"namespace": "urn:ext", "local": "OneEvt"},
        "none_behavior_event_ref": {"namespace": "urn:ext", "local": "NoneEvt"},
        "cardinality": "5",
        "completion_condition": "done"
    }));
    let LoopCharacteristics::MultiInstance {
        behavior,
        loop_data_input_ref,
        loop_data_output_ref,
        input_data_item,
        output_data_item,
        one_behavior_event_ref,
        none_behavior_event_ref,
        completion_condition,
        ..
    } = &value
    else {
        panic!("expected multi-instance");
    };
    assert_eq!(
        *behavior,
        Some(processcraft_core::bpmn_activity::MultiInstanceFlowCondition::One)
    );
    assert!(loop_data_input_ref.is_some() && loop_data_output_ref.is_some());
    assert_eq!(
        input_data_item.as_ref().unwrap().name.as_deref(),
        Some("item")
    );
    assert_eq!(
        output_data_item.as_ref().unwrap().name.as_deref(),
        Some("result")
    );
    assert!(one_behavior_event_ref.is_some());
    assert!(matches!(
        none_behavior_event_ref,
        Some(processcraft_core::bpmn_model::EventReference::Qualified(_))
    ));
    assert_eq!(
        completion_condition.as_ref().and_then(|c| c.plain_text()),
        Some("done")
    );
    set_read_undo(value);
}

#[test]
fn standard_loop_condition_test_before_and_maximum_set_read_and_undo() {
    let value = LoopCharacteristics::Standard {
        id: None,
        base: None,
        test_before: Some(true),
        condition: Some("retry".into()),
        maximum: Some("12".into()),
    };
    set_read_undo(value.clone());
    let LoopCharacteristics::Standard {
        test_before,
        condition,
        maximum,
        ..
    } = value
    else {
        unreachable!()
    };
    assert_eq!(test_before, Some(true));
    assert_eq!(condition.unwrap().plain_text(), Some("retry"));
    assert_eq!(maximum.as_deref(), Some("12"));
}
