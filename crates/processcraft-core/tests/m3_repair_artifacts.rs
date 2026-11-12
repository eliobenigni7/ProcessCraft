//! Repair tests for the artifacts family (association create/undo, group category-value diagnostics).
use processcraft_core::{
    bpmn_activity::ModelReference,
    commands::{Envelope, Mutation, Request, Session},
    model::*,
    queries::find_flow,
    validation::activity_family_reference_issues,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn node(name: &str, kind: NodeKind) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        id: id(name),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}

fn assoc(name: &str, source: &str, target: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        id: id(name),
        kind: FlowKind::Association,
        source: Some(ModelReference::Local(id(source))),
        target: Some(ModelReference::Local(id(target))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    }
}

fn document() -> BpmnDocument {
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
        nodes: vec![
            node("task", NodeKind::Task),
            node("note", NodeKind::TextAnnotation),
        ],
        lanes: vec![],
        flows: vec![],
    });
    d
}

fn env(s: &Session) -> Envelope {
    Envelope {
        request_id: "x".into(),
        session_id: s.id(),
        expected_generation: None,
    }
}

#[test]
fn association_endpoints_are_created_read_back_and_undone() {
    let d = document();
    let doc_id = d.id;
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    let e = env(&s);
    s.apply(
        e,
        Request::Batch {
            mutations: vec![Mutation::CreateFlow {
                document_id: doc_id,
                process_id: Some(id("process")),
                flow: assoc("a", "task", "note"),
            }],
        },
    )
    .unwrap();
    let snap = s.snapshot();
    let f = find_flow(&snap.project.documents[0], &id("a")).unwrap();
    assert_eq!(f.source, Some(ModelReference::Local(id("task"))));
    assert_eq!(f.target, Some(ModelReference::Local(id("note"))));
    let e = env(&s);
    s.apply(e, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert!(find_flow(&s.snapshot().project.documents[0], &id("a")).is_none());
}

#[test]
fn group_category_value_ref_resolves_to_category_value_only() {
    use processcraft_core::bpmn_model::{Category, CategoryValue, Group};
    let mut d = document();
    d.categories.push(Category {
        base: None,
        id: id("cat"),
        name: Some("Stage".into()),
        values: vec![CategoryValue {
            base: None,
            id: id("val"),
            value: Some("Review".into()),
        }],
    });
    let group = |r: &str| Group {
        base: None,
        id: id("grp"),
        category_value_ref: Some(ModelReference::Local(id(r))),
    };
    d.processes[0].groups.push(group("val"));
    assert!(activity_family_reference_issues(&d).is_empty());
    for bad in ["ghost", "cat"] {
        d.processes[0].groups[0] = group(bad);
        let issues = activity_family_reference_issues(&d);
        assert_eq!(issues.len(), 1, "{bad}");
        assert_eq!(issues[0].field, "group.categoryValueRef");
        assert_eq!(
            issues[0].owner,
            id::<processcraft_core::ids::ElementSelector>("grp")
        );
        assert_eq!(issues[0].target, ModelReference::Local(id(bad)));
    }
}
