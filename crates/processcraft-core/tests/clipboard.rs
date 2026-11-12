use processcraft_core::{
    arrange::{Alignment, Distribution, prepare_align, prepare_distribute},
    bpmn_model::EventReference,
    clipboard::{copy, paste},
    commands::{Envelope, Request, Session},
    ids::ElementSelector,
    model::ProjectData,
};

fn session() -> Session {
    let mut session = Session::new(ProjectData::new("project".into())).unwrap();
    let fixture: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(&mut session, fixture);
    session
}

fn apply(session: &mut Session, request: Request) {
    let envelope = Envelope {
        request_id: "test".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session.apply(envelope, request).unwrap();
}

fn sel(id: &str) -> ElementSelector {
    id.parse().unwrap()
}

#[test]
fn paste_remaps_ids_keeps_internal_references_and_undoes_in_one_step() {
    let mut s = session();
    let before = s.snapshot();
    let document = &before.project.documents[0];
    let diagram = sel("procurement_diagram");
    // The lane is not copied without its participant; the flow between both copies is.
    let clip = copy(
        document,
        &diagram,
        &[sel("review"), sel("decision"), sel("lane_request")],
    )
    .unwrap();
    assert_eq!(clip.nodes.len(), 2);
    assert_eq!(clip.flows.len(), 1);
    assert_eq!((clip.shapes.len(), clip.edges.len()), (2, 1));
    let pasted = paste(document, &diagram, &sel("procurement"), &clip, 20.0, 20.0).unwrap();
    apply(
        &mut s,
        Request::Batch {
            mutations: pasted.mutations,
        },
    );
    let after = s.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    let document = &after.project.documents[0];
    let process = &document.processes[0];
    assert_eq!(process.nodes.len(), 7);
    assert_eq!(pasted.selection.len(), 3);
    let new_nodes: Vec<_> = process.nodes[5..].iter().collect();
    for node in &new_nodes {
        assert!(pasted.selection.contains(&node.id.selector()));
        let id = node.id.to_string();
        assert!(id != "review" && id != "decision");
        assert!(id.starts_with("review_") || id.starts_with("decision_"));
    }
    let flow = process.flows.last().unwrap();
    assert_eq!(
        flow.source,
        Some(EventReference::Local(new_nodes[0].id.selector()))
    );
    assert_eq!(
        flow.target,
        Some(EventReference::Local(new_nodes[1].id.selector()))
    );
    let shapes = &document.diagrams[0].shapes;
    let review = shapes
        .iter()
        .find(|s| s.element == new_nodes[0].id)
        .unwrap();
    assert_eq!((review.bounds.x, review.bounds.y), (210.0, 108.0));
    let decision = shapes
        .iter()
        .find(|s| s.element == new_nodes[1].id)
        .unwrap();
    assert_eq!(
        decision.label.as_ref().unwrap().bounds.as_ref().unwrap().x,
        380.0
    );
    let edge = document.diagrams[0].edges.last().unwrap();
    assert_eq!(edge.element, flow.id);
    assert_eq!((edge.waypoints[0].x, edge.waypoints[0].y), (350.0, 148.0));
    // Pasting the same clipboard twice yields distinct IDs again.
    let again = paste(
        document,
        &sel("procurement_diagram"),
        &sel("procurement"),
        &clip,
        40.0,
        40.0,
    )
    .unwrap();
    apply(
        &mut s,
        Request::Batch {
            mutations: again.mutations,
        },
    );
    apply(&mut s, Request::Undo);
    apply(&mut s, Request::Undo);
    assert_eq!(s.snapshot().content_id, before.content_id);
}

#[test]
fn nothing_copyable_is_refused() {
    let s = session();
    let snap = s.snapshot();
    let document = &snap.project.documents[0];
    assert!(
        copy(
            document,
            &sel("procurement_diagram"),
            &[sel("lane_request")]
        )
        .is_err()
    );
}

#[test]
fn align_and_distribute_apply_as_one_undo_step() {
    let mut s = session();
    let before = s.snapshot();
    let document = &before.project.documents[0];
    let diagram = sel("procurement_diagram");
    let shapes = [
        sel("pc_shape_start"),
        sel("pc_shape_review"),
        sel("pc_shape_decision"),
        sel("pc_shape_lane_request"),
    ];
    let prepared = prepare_align(document, &diagram, &shapes, Alignment::Top).unwrap();
    apply(&mut s, prepared.request);
    let after = s.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    let diagram_after = &after.project.documents[0].diagrams[0];
    let y = |id: &str| {
        diagram_after
            .shapes
            .iter()
            .find(|s| s.id.to_string() == id)
            .unwrap()
            .bounds
            .y
    };
    // Lanes are filtered out; the rest align to the topmost shape.
    assert_eq!(y("pc_shape_lane_request"), 40.0);
    assert_eq!(
        (
            y("pc_shape_start"),
            y("pc_shape_review"),
            y("pc_shape_decision")
        ),
        (88.0, 88.0, 88.0)
    );
    let document = &after.project.documents[0];
    let prepared =
        prepare_distribute(document, &diagram, &shapes[..3], Distribution::Horizontal).unwrap();
    apply(&mut s, prepared.request);
    assert!(prepare_align(document, &diagram, &shapes[..1], Alignment::Left).is_err());
}
