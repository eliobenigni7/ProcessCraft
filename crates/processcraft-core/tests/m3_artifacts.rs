use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_model::{AssociationDirection, Category, CategoryValue},
    commands::{CommandError, Envelope, Mutation, Request, Session},
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

fn apply(s: &mut Session, m: Vec<Mutation>) -> Result<(), CommandError> {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Batch { mutations: m },
    )
    .map(|_| ())
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

fn flow(name: &str, kind: FlowKind, source: &str, target: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        id: id(name),
        kind,
        source: Some(ModelReference::Local(id(source))),
        target: Some(ModelReference::Local(id(target))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
        association_direction: None,
    }
}

fn document(flows: Vec<Flow>) -> BpmnDocument {
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
        nodes: vec![
            node("task", NodeKind::Task),
            node("task2", NodeKind::Task),
            node("note", NodeKind::TextAnnotation),
        ],
        lanes: vec![],
        flows,
    });
    d
}

fn category(name: &str, value: &str) -> Category {
    Category {
        base: None,
        id: id("cat"),
        name: Some(name.into()),
        values: vec![CategoryValue {
            base: None,
            id: id("val"),
            value: Some(value.into()),
        }],
    }
}

fn session(d: BpmnDocument) -> Session {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    Session::new(p).unwrap()
}

#[test]
fn category_name_and_value_edit_are_stored_and_undo_restores() {
    let d = document(vec![]);
    let doc_id = d.id;
    let mut s = session(d);
    apply(
        &mut s,
        vec![Mutation::CreateCategory {
            document_id: doc_id,
            category: category("Risk", "Low"),
        }],
    )
    .unwrap();
    let created = s.snapshot().content_id;
    apply(
        &mut s,
        vec![Mutation::UpdateCategory {
            document_id: doc_id,
            category: category("Stage", "Review"),
        }],
    )
    .unwrap();
    let c = &s.snapshot().project.documents[0].categories[0];
    assert_eq!(c.name.as_deref(), Some("Stage"));
    assert_eq!(c.values[0].value.as_deref(), Some("Review"));
    s.apply(
        Envelope {
            request_id: "u".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Undo,
    )
    .unwrap();
    assert_eq!(s.snapshot().content_id, created);
    let c = &s.snapshot().project.documents[0].categories[0];
    assert_eq!(c.name.as_deref(), Some("Risk"));
    assert_eq!(c.values[0].value.as_deref(), Some("Low"));
}

#[test]
fn association_dangling_endpoints_report_source_and_target_diagnostics() {
    let ok = document(vec![flow("a", FlowKind::Association, "task", "note")]);
    assert!(activity_family_reference_issues(&ok).is_empty());
    let bad_source = document(vec![flow("a", FlowKind::Association, "ghost", "note")]);
    let issues = activity_family_reference_issues(&bad_source);
    assert!(issues.iter().any(|i| i.field == "association.sourceRef"));
    assert!(!issues.iter().any(|i| i.field == "association.targetRef"));
    let bad_target = document(vec![flow("a", FlowKind::Association, "task", "ghost")]);
    let issues = activity_family_reference_issues(&bad_target);
    assert!(issues.iter().any(|i| i.field == "association.targetRef"));
    assert!(!issues.iter().any(|i| i.field == "association.sourceRef"));
}

#[test]
fn association_direction_is_rejected_on_non_association_flow() {
    let d = document(vec![
        flow("assoc", FlowKind::Association, "task", "note"),
        flow("seq", FlowKind::Sequence, "task", "task2"),
    ]);
    let doc_id = d.id;
    let mut s = session(d);
    let before = s.snapshot().content_id;
    let set = |flow: &str| Mutation::SetAssociationDirection {
        document_id: doc_id,
        flow_id: id(flow),
        direction: Some(AssociationDirection::One),
    };
    assert!(apply(&mut s, vec![set("seq")]).is_err());
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, vec![set("assoc")]).unwrap();
    assert_eq!(
        find_flow(&s.snapshot().project.documents[0], &id("assoc"))
            .unwrap()
            .association_direction,
        Some(AssociationDirection::One)
    );
}
