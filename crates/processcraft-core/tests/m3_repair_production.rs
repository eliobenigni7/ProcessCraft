//! Repair tests: typed edge endpoints and the BPMNPlane.bpmnElement diagnostic.
use processcraft_core::{
    commands::{Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity},
    model::ProjectData,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    id("11111111-1111-4111-8111-111111111111")
}
fn apply(s: &mut Session, r: Request) {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, r).unwrap();
}
fn fixture() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(&mut s, r);
    s
}
fn edge_ends(s: &Session) -> (Option<String>, Option<String>) {
    let snap = s.snapshot();
    let edge = snap.project.documents[0].diagrams[0]
        .edges
        .iter()
        .find(|e| e.id == id::<ElementIdentity>("pc_edge_flow_start_review"))
        .unwrap();
    (
        edge.source_element.as_ref().map(ToString::to_string),
        edge.target_element.as_ref().map(ToString::to_string),
    )
}

#[test]
fn edge_source_and_target_elements_are_set_read_back_and_undone() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    let ends = |source: Option<&str>, target: Option<&str>| Request::Batch {
        mutations: vec![Mutation::SetEdgeDiEndpoints {
            document_id: docid(),
            diagram_id: id("procurement_diagram"),
            edge_id: id("pc_edge_flow_start_review"),
            source_element: source.map(id),
            target_element: target.map(id),
        }],
    };
    // Different shapes for source and target make the assertion discriminating.
    apply(
        &mut s,
        ends(Some("pc_shape_start"), Some("pc_shape_review")),
    );
    assert_eq!(
        edge_ends(&s),
        (
            Some("pc_shape_start".into()),
            Some("pc_shape_review".into())
        )
    );
    apply(&mut s, ends(Some("pc_shape_start"), None));
    assert_eq!(edge_ends(&s), (Some("pc_shape_start".into()), None));
    apply(&mut s, Request::Undo);
    assert_eq!(
        edge_ends(&s).1.as_deref(),
        Some("pc_shape_review"),
        "undo restores the target link"
    );
    apply(&mut s, Request::Undo);
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn dangling_plane_element_is_diagnosed_and_valid_planes_are_not() {
    let s = fixture();
    let code = "danglingPlaneElement";
    let project = (*s.snapshot().project).clone();
    assert!(
        !project.diagnostics().iter().any(|d| d.code == code),
        "a plane naming its process is valid"
    );
    let mut bad = project;
    let plane_id = {
        let doc = Arc::make_mut(&mut bad.documents[0]);
        doc.diagrams[0].plane = id("no_such_process");
        doc.diagrams[0].plane_id.selector()
    };
    let hits: Vec<_> = bad
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == code)
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].element_id, Some(plane_id));
    assert!(hits[0].message.contains("di.bpmnElement"));
}
