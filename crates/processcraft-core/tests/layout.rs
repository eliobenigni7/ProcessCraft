use processcraft_core::{
    ids::{ElementIdentity, ElementSelector},
    layout::{propose_layout, validate_proposal},
    model::{
        Bounds, BpmnDocument, Diagram, Edge, Flow, FlowKind, Node, NodeKind, Point, Process, Shape,
    },
};
use std::sync::atomic::AtomicBool;
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn fixture() -> BpmnDocument {
    let mut d = BpmnDocument::new("Example".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("p"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: ["a", "b", "c", "d"]
            .iter()
            .map(|s| Node {
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                activity: None,
                id: id(s),
                name: Some(s.to_string()),
                documentation: String::new().into(),
                kind: NodeKind::Task,
                details: None,
                loop_characteristics: None,
                io_specification: None,
                owner: String::new(),
                contact: String::new(),
            })
            .collect(),
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("p"),
        shapes: ["a", "b", "c", "d"]
            .iter()
            .enumerate()
            .map(|(i, s)| Shape {
                di_metadata: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                label_visibility: None,
                is_horizontal: None,
                is_marker_visible: None,
                is_expanded: None,
                id: id(&format!("shape_{s}")),
                element: id(s),
                bounds: Bounds {
                    x: i as f64 * 200.0,
                    y: 20.0,
                    width: 100.0,
                    height: 60.0,
                },
                label: None,
                style: None,
            })
            .collect(),
        edges: vec![],
    });
    d
}
fn flow(d: &mut BpmnDocument, name: &str, a: &str, b: &str) {
    d.processes[0].flows.push(Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(a))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(b))),
        name: Some(String::new()),
        documentation: String::new().into(),
        condition: None,
    });
    d.diagrams[0].edges.push(Edge {
        di_metadata: None,
        message_visible_kind: None,
        label_visibility: None,
        source_element: None,
        target_element: None,
        id: id(&format!("di_{name}")),
        element: id(name),
        waypoints: vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }],
        label: None,
        style: None,
    });
}
fn layout(d: &BpmnDocument) -> processcraft_core::layout::LayoutProposal {
    propose_layout(d, &id("diagram"), "session", 1, &AtomicBool::new(false)).unwrap()
}
#[test]
fn branches_are_deterministic_and_routes_orthogonal() {
    let mut d = fixture();
    for (name, a, b) in [
        ("ab", "a", "b"),
        ("ac", "a", "c"),
        ("bd", "b", "d"),
        ("cd", "c", "d"),
    ] {
        flow(&mut d, name, a, b);
    }
    let a = layout(&d);
    let b = layout(&d);
    assert_eq!(
        serde_json::to_value(&a.proposed).unwrap(),
        serde_json::to_value(&b.proposed).unwrap()
    );
    assert_eq!(a.proposed.edges.len(), 4);
    for edge in &a.proposed.edges {
        assert!(
            edge.waypoints
                .windows(2)
                .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
        );
    }
    validate_proposal(&d, &a).unwrap();
}
#[test]
fn forged_or_stale_proposals_fail() {
    let d = fixture();
    let mut p = layout(&d);
    p.proposed.shapes[0].bounds.x += 1.0;
    assert!(validate_proposal(&d, &p).is_err());
    let mut p = layout(&d);
    p.original.shapes[0].bounds.x += 1.0;
    assert!(validate_proposal(&d, &p).is_err());
}
#[test]
fn cancellation_and_unsupported_leave_document_unchanged() {
    let mut d = fixture();
    let before = serde_json::to_vec(&d).unwrap();
    assert_eq!(
        propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(true))
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(before, serde_json::to_vec(&d).unwrap());
    d.processes[0].nodes[0].kind = NodeKind::TextAnnotation;
    assert!(propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
}
#[test]
fn layout_refuses_missing_or_external_sequence_endpoints_without_mutation() {
    let mut doc = fixture();
    flow(&mut doc, "ab", "a", "b");
    doc.processes[0].flows[0].target = None;
    let before = serde_json::to_vec(&doc).unwrap();
    assert!(propose_layout(&doc, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
    assert_eq!(before, serde_json::to_vec(&doc).unwrap());
    doc.processes[0].flows[0].target =
        Some(processcraft_core::bpmn_model::EventReference::Qualified(
            processcraft_core::bpmn_model::QNameRef {
                namespace: "urn:foreign".into(),
                local: id("b"),
            },
        ));
    let before = serde_json::to_vec(&doc).unwrap();
    assert!(propose_layout(&doc, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
    assert_eq!(before, serde_json::to_vec(&doc).unwrap());
}
#[test]
fn cycles_and_self_loops_are_bounded() {
    let mut d = fixture();
    flow(&mut d, "ab", "a", "b");
    flow(&mut d, "ba", "b", "a");
    flow(&mut d, "cc", "c", "c");
    let p = layout(&d);
    assert_eq!(p.proposed.edges.len(), 3);
}
#[test]
fn repeated_preview_after_geometry_application_is_idempotent() {
    let mut d = fixture();
    flow(&mut d, "ab", "a", "b");
    let first = layout(&d);
    for shape in &mut d.diagrams[0].shapes {
        let update = first
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == shape.id)
            .unwrap();
        shape.bounds = update.bounds.clone();
    }
    for edge in &mut d.diagrams[0].edges {
        let update = first
            .proposed
            .edges
            .iter()
            .find(|e| e.id == edge.id)
            .unwrap();
        edge.waypoints = update.waypoints.clone();
    }
    let second = layout(&d);
    assert_eq!(
        serde_json::to_value(first.proposed).unwrap(),
        serde_json::to_value(second.proposed).unwrap()
    );
}
#[test]
fn session_apply_undo_redo_and_noop_preserve_history() {
    use processcraft_core::{
        commands::{Envelope, Mutation, Request, Session},
        model::ProjectData,
    };
    use std::sync::Arc;
    let mut doc = fixture();
    flow(&mut doc, "ab", "a", "b");
    let mut project = ProjectData::new("Project".into());
    project.documents.push(Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let before = session.snapshot();
    let original = serde_json::to_vec(&before.project).unwrap();
    let proposal = propose_layout(
        &before.project.documents[0],
        &id("diagram"),
        &session.id(),
        before.generation,
        &AtomicBool::new(false),
    )
    .unwrap();
    let apply = |session: &mut Session, request| {
        let envelope = Envelope {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: session.id(),
            expected_generation: None,
        };
        session.apply(envelope, request).unwrap();
    };
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::ApplyLayout { proposal }],
        },
    );
    let after = session.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    let repeated = propose_layout(
        &after.project.documents[0],
        &id("diagram"),
        &session.id(),
        after.generation,
        &AtomicBool::new(false),
    )
    .unwrap();
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::ApplyLayout { proposal: repeated }],
        },
    );
    assert_eq!(session.snapshot().generation, after.generation);
    apply(&mut session, Request::Undo);
    assert_eq!(
        serde_json::to_vec(&session.snapshot().project).unwrap(),
        original
    );
    apply(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after.content_id);
}
#[test]
fn lane_membership_and_unlaned_nodes_use_separate_bands() {
    use processcraft_core::model::Lane;
    let mut d = fixture();
    d.processes[0].lanes.push(Lane {
        base: None,
        child_lane_set_base: None,
        child_lane_set_name: None,
        partition_element: None,
        partition_element_ref: None,
        child_lane_set_id: None,
        id: id("lane"),
        name: Some("Lane".into()),
        parent: None,
        members: vec![id("a"), id("b")],
    });
    d.diagrams[0].shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id("lane_di"),
        element: id("lane"),
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 200.0,
        },
        label: None,
        style: None,
    });
    let p = layout(&d);
    let lane = &p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("lane_di"))
        .unwrap()
        .bounds;
    for name in ["shape_a", "shape_b"] {
        let b = &p
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == id::<ElementSelector>(name))
            .unwrap()
            .bounds;
        assert!(b.y >= lane.y && b.y + b.height <= lane.y + lane.height);
    }
    let unlaned = &p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("shape_c"))
        .unwrap()
        .bounds;
    assert!(unlaned.y >= lane.y + lane.height);
}
#[test]
fn missing_duplicate_or_invalid_di_is_refused() {
    let mut d = fixture();
    d.diagrams[0].shapes.pop();
    assert!(propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
    let mut d = fixture();
    let duplicate = d.diagrams[0].shapes[0].clone();
    d.diagrams[0].shapes.push(duplicate);
    assert!(propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
    let mut d = fixture();
    d.diagrams[0].shapes[0].bounds.x = f64::NAN;
    assert!(propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false)).is_err());
}
#[test]
fn external_labels_keep_sizes_and_styles() {
    use processcraft_core::model::{Label, Style};
    let mut d = fixture();
    flow(&mut d, "ab", "a", "b");
    let style = Some(Style {
        fill: Some("#123456".into()),
        stroke: None,
        font: None,
    });
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 0.0,
            y: -30.0,
            width: 60.0,
            height: 20.0,
        }),
        style: style.clone(),
    });
    d.diagrams[0].edges[0].label = Some(Label {
        di_metadata: None,
        id: None,
        style_ref: None,
        bounds: Some(Bounds {
            x: 150.0,
            y: 0.0,
            width: 60.0,
            height: 20.0,
        }),
        style,
    });
    let p = layout(&d);
    let shape = p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("shape_a"))
        .unwrap();
    assert_eq!(shape.label_bounds.as_ref().unwrap().width, 60.0);
    let edge = &p.proposed.edges[0];
    assert_eq!(edge.label_bounds.as_ref().unwrap().height, 20.0);
    assert!(
        d.diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .style
            .is_some()
    );
    assert!(
        d.diagrams[0].edges[0]
            .label
            .as_ref()
            .unwrap()
            .style
            .is_some()
    );
}
#[test]
fn maximum_shape_count_is_bounded_and_above_limits_refused() {
    let mut d = fixture();
    for i in 4..400 {
        let node_id: ElementIdentity = id(&format!("node_{i}"));
        d.processes[0].nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            id: node_id.clone(),
            name: Some(String::new()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            owner: String::new(),
            contact: String::new(),
        });
        d.diagrams[0].shapes.push(Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id(&format!("shape_{i}")),
            element: node_id.selector(),
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 60.0,
            },
            label: None,
            style: None,
        });
    }
    let p = layout(&d);
    assert_eq!(p.proposed.shapes.len(), 400);
    let extra = d.diagrams[0].shapes[0].clone();
    d.diagrams[0].shapes.push(extra);
    assert_eq!(
        propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false))
            .unwrap_err()
            .code,
        "limit"
    );
}
#[test]
fn edge_ceiling_fails_safely_without_partial_changes() {
    let mut d = fixture();
    for i in 0..500 {
        flow(&mut d, &format!("edge_{i}"), "a", "b");
    }
    let before = serde_json::to_vec(&d).unwrap();
    let result = propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false));
    if let Ok(proposal) = result {
        assert_eq!(proposal.proposed.edges.len(), 500);
    }
    assert_eq!(before, serde_json::to_vec(&d).unwrap());
    flow(&mut d, "edge_over", "a", "b");
    assert_eq!(
        propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false))
            .unwrap_err()
            .code,
        "limit"
    );
}
#[test]
fn one_pool_contains_lanes_and_unlaned_band() {
    use processcraft_core::model::{Collaboration, Lane, Participant};
    let mut d = fixture();
    d.processes[0].lanes.push(Lane {
        base: None,
        child_lane_set_base: None,
        child_lane_set_name: None,
        partition_element: None,
        partition_element_ref: None,
        child_lane_set_id: None,
        id: id("lane"),
        name: Some(String::new()),
        parent: None,
        members: vec![id("a")],
    });
    d.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collab"),
        conversation_nodes: vec![],
        conversation_links: vec![],
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            base: None,
            id: id("pool"),
            name: Some(String::new()),
            process: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "p",
            ))),
        }],
        flows: vec![],
    });
    for (shape, element) in [("lane_di", "lane"), ("pool_di", "pool")] {
        d.diagrams[0].shapes.push(Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id(shape),
            element: id(element),
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            label: None,
            style: None,
        });
    }
    let p = layout(&d);
    let pool = &p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("pool_di"))
        .unwrap()
        .bounds;
    for name in ["lane_di", "shape_a", "shape_b"] {
        let b = &p
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == id::<ElementSelector>(name))
            .unwrap()
            .bounds;
        assert!(
            b.x >= pool.x
                && b.y >= pool.y
                && b.x + b.width <= pool.x + pool.width
                && b.y + b.height <= pool.y + pool.height
        );
    }
}
#[test]
fn impossible_long_labels_are_refused_without_changes() {
    use processcraft_core::model::Label;
    let mut d = fixture();
    for shape in &mut d.diagrams[0].shapes {
        shape.label = Some(Label {
            di_metadata: None,
            id: None,
            style_ref: None,
            bounds: Some(Bounds {
                x: 0.0,
                y: 0.0,
                width: 10000.0,
                height: 10000.0,
            }),
            style: None,
        });
    }
    let before = serde_json::to_vec(&d).unwrap();
    let result = propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false));
    assert_eq!(result.unwrap_err().code, "collision");
    assert_eq!(before, serde_json::to_vec(&d).unwrap());
}
#[test]
fn event_and_gateway_routes_terminate_on_shape_border() {
    let mut d = fixture();
    d.processes[0].nodes[0].kind = NodeKind::StartEvent;
    d.processes[0].nodes[1].kind = NodeKind::ExclusiveGateway;
    flow(&mut d, "ab", "a", "b");
    let p = layout(&d);
    let a = &p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("shape_a"))
        .unwrap()
        .bounds;
    let b = &p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("shape_b"))
        .unwrap()
        .bounds;
    let edge = &p.proposed.edges[0];
    let start = &edge.waypoints[0];
    let end = edge.waypoints.last().unwrap();
    assert_eq!((start.x, start.y), (a.x + a.width, a.y + a.height / 2.0));
    assert_eq!((end.x, end.y), (b.x, b.y + b.height / 2.0));
}
#[test]
fn four_hundred_connected_nodes_fit_bounded_straight_route_profile() {
    let mut d = fixture();
    let template = d.processes[0].nodes[0].clone();
    let shape_template = d.diagrams[0].shapes[0].clone();
    d.processes[0].nodes.clear();
    d.diagrams[0].shapes.clear();
    for i in 0..400 {
        let mut node = template.clone();
        node.id = id(&format!("n{i:03}"));
        node.name = Some(String::new());
        let mut shape = shape_template.clone();
        shape.id = id(&format!("s{i:03}"));
        shape.element = node.id.selector();
        shape.label = None;
        d.processes[0].nodes.push(node);
        d.diagrams[0].shapes.push(shape);
    }
    for i in 0..399 {
        flow(
            &mut d,
            &format!("f{i:03}"),
            &format!("n{i:03}"),
            &format!("n{:03}", i + 1),
        );
    }
    let p = layout(&d);
    assert_eq!(
        (p.proposed.shapes.len(), p.proposed.edges.len()),
        (400, 399)
    );
    assert!(p.proposed.edges.iter().all(|e| e.waypoints.len() == 2));
    for i in 0..101 {
        flow(
            &mut d,
            &format!("extra{i:03}"),
            &format!("n{i:03}"),
            &format!("n{:03}", i + 2),
        );
    }
    let before = serde_json::to_vec(&d).unwrap();
    let result = propose_layout(&d, &id("diagram"), "s", 0, &AtomicBool::new(false));
    println!(
        "500-edge connected fixture: {}",
        match &result {
            Ok(_) => "accepted".to_owned(),
            Err(error) => format!("refused: {}", error.code),
        }
    );
    assert!(
        result.is_ok()
            || matches!(
                result.unwrap_err().code.as_str(),
                "budget" | "routing" | "collision"
            )
    );
    assert_eq!(before, serde_json::to_vec(&d).unwrap());
}
#[test]
fn labelled_two_lane_branch_has_collision_free_layout() {
    let d: BpmnDocument =
        serde_json::from_str(include_str!("fixtures/layout-operations.json")).unwrap();
    let diagram = &d.diagrams[0];
    let p = propose_layout(
        &d,
        &diagram.id.selector(),
        "session",
        0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(p.proposed.edges.len(), 5);
    validate_proposal(&d, &p).unwrap();
}

#[test]
fn layout_uses_native_selectors_without_inventing_xml_ids() {
    use processcraft_core::ids::NativeElementId;
    let mut doc = fixture();
    flow(&mut doc, "ab", "a", "b");
    doc.diagrams[0].id = ElementIdentity::Native {
        native_id: NativeElementId::new(),
        xml_id: None,
    };
    doc.diagrams[0].shapes[0].id = ElementIdentity::Native {
        native_id: NativeElementId::new(),
        xml_id: None,
    };
    let diagram = doc.diagrams[0].id.selector();
    let shape = doc.diagrams[0].shapes[0].id.selector();
    let proposal = propose_layout(&doc, &diagram, "session", 0, &AtomicBool::new(false)).unwrap();
    assert!(
        proposal
            .original
            .shapes
            .iter()
            .any(|entry| entry.id == shape)
    );
    assert!(doc.diagrams[0].id.xml_id().is_none());
    assert!(doc.diagrams[0].shapes[0].id.xml_id().is_none());
    validate_proposal(&doc, &proposal).unwrap();
}

#[test]
fn layout_keeps_explicit_label_without_inventing_bounds() {
    use processcraft_core::model::Label;
    let mut doc = fixture();
    flow(&mut doc, "ab", "a", "b");
    doc.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: None,
        bounds: None,
        style: None,
        style_ref: None,
    });
    let proposal = layout(&doc);
    let shape_id = doc.diagrams[0].shapes[0].id.selector();
    assert!(
        proposal
            .original
            .shapes
            .iter()
            .find(|s| s.id == shape_id)
            .unwrap()
            .label_bounds
            .is_none()
    );
    assert!(
        proposal
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == shape_id)
            .unwrap()
            .label_bounds
            .is_none()
    );
    validate_proposal(&doc, &proposal).unwrap();
    assert!(doc.diagrams[0].shapes[0].label.is_some());
    assert!(
        doc.diagrams[0].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .is_none()
    );
    use processcraft_core::commands::{Envelope, Mutation, Request, Session};
    let mut project = processcraft_core::model::ProjectData::new("Project".into());
    project.documents.push(std::sync::Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let snapshot = session.snapshot();
    let proposal = propose_layout(
        &snapshot.project.documents[0],
        &id("diagram"),
        &session.id(),
        snapshot.generation,
        &AtomicBool::new(false),
    )
    .unwrap();
    session
        .apply(
            Envelope {
                request_id: uuid::Uuid::new_v4().to_string(),
                session_id: session.id(),
                expected_generation: Some(snapshot.generation),
            },
            Request::Batch {
                mutations: vec![Mutation::ApplyLayout { proposal }],
            },
        )
        .unwrap();
    let after = session.snapshot();
    let label = after.project.documents[0].diagrams[0].shapes[0]
        .label
        .as_ref()
        .unwrap();
    assert!(label.bounds.is_none());
}

#[test]
fn choreography_layout_moves_real_band_with_its_owner_and_rejects_stale_geometry() {
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    use processcraft_core::model::{DiReference, Participant, ParticipantBandKind};
    let mut doc = fixture();
    doc.processes.clear();
    doc.diagrams[0].plane = id("choreo");
    doc.diagrams[0].shapes.truncate(1);
    let mut band = doc.diagrams[0].shapes[0].clone();
    band.id = id("band");
    band.element = id("participant");
    band.participant_band_kind = Some(ParticipantBandKind::TopInitiating);
    band.choreography_activity_shape = Some(DiReference::Local(id("shape_a")));
    band.bounds.height = 15.0;
    doc.diagrams[0].shapes.push(band);
    doc.choreographies.push(Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            base: None,
            id: id("participant"),
            name: None,
            process: None,
        }],
        message_flows: vec![],
        activities: vec![ChoreographyActivity {
            loop_type: None,
            details: None,
            flow_metadata: None,
            correlation_keys: vec![],
            base: None,
            id: id("a"),
            name: None,
            participant_refs: vec![ModelReference::Local(id("participant"))],
            initiating_participant: Some(ModelReference::Local(id("participant"))),
            message_flow_refs: vec![],
        }],
        sequence_flows: vec![],
    });
    let proposal = layout(&doc);
    validate_proposal(&doc, &proposal).unwrap();
    let find = |shapes: &[processcraft_core::layout::ShapeGeometry], name: &str| {
        shapes
            .iter()
            .find(|s| s.id == id::<ElementSelector>(name))
            .unwrap()
            .bounds
            .clone()
    };
    let owner = find(&proposal.proposed.shapes, "shape_a");
    let band = find(&proposal.proposed.shapes, "band");
    assert_eq!((owner.x, owner.y), (band.x, band.y));
    doc.diagrams[0].shapes[0].bounds.x += 1.0;
    assert_eq!(
        validate_proposal(&doc, &proposal).unwrap_err().code,
        "stale"
    );
}

#[test]
fn expanded_choreography_scope_layout_targets_one_representation() {
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyFlowElement,
        ChoreographyGraph,
    };
    use processcraft_core::layout::propose_choreography_scope_layout;
    let mut doc = fixture();
    doc.processes.clear();
    doc.diagrams[0].plane = id("choreo");
    let mut scope_a = doc.diagrams[0].shapes[0].clone();
    scope_a.id = id("scope_a");
    scope_a.element = id("sub");
    scope_a.is_expanded = Some(true);
    scope_a.bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 600.0,
        height: 400.0,
    };
    let mut scope_b = scope_a.clone();
    scope_b.id = id("scope_b");
    scope_b.bounds.x = 1000.0;
    let mut child_a = doc.diagrams[0].shapes[0].clone();
    child_a.id = id("child_a");
    child_a.bounds.x = 10.0;
    child_a.bounds.y = 10.0;
    child_a.bounds.width = 300.0;
    child_a.bounds.height = 200.0;
    child_a.is_expanded = Some(true);
    let mut grand_a = doc.diagrams[0].shapes[0].clone();
    grand_a.id = id("grand_a");
    grand_a.element = id("grand");
    grand_a.bounds.x = 30.0;
    grand_a.bounds.y = 40.0;
    let mut grand_b = grand_a.clone();
    grand_b.id = id("grand_b");
    grand_b.bounds.x = 1030.0;
    let mut grand2_a = grand_a.clone();
    grand2_a.id = id("grand2_a");
    grand2_a.element = id("grand2");
    grand2_a.bounds.x = 180.0;
    let mut grand2_b = grand2_a.clone();
    grand2_b.id = id("grand2_b");
    grand2_b.bounds.x = 1180.0;
    let mut child_b = child_a.clone();
    child_b.id = id("child_b");
    child_b.bounds.x = 1010.0;
    let mut band_a = child_a.clone();
    band_a.id = id("band_a");
    band_a.element = id("p");
    band_a.bounds.height = 15.0;
    band_a.participant_band_kind =
        Some(processcraft_core::model::ParticipantBandKind::TopInitiating);
    band_a.choreography_activity_shape =
        Some(processcraft_core::model::DiReference::Local(id("child_a")));
    let mut band_b = band_a.clone();
    band_b.id = id("band_b");
    band_b.bounds.x = 1010.0;
    band_b.choreography_activity_shape =
        Some(processcraft_core::model::DiReference::Local(id("child_b")));
    doc.diagrams[0].shapes = vec![
        scope_a, scope_b, child_a, child_b, band_a, band_b, grand_a, grand_b, grand2_a, grand2_b,
    ];
    let edge = |name: &str, source: &str, target: &str, offset: f64| Edge {
        di_metadata: None,
        message_visible_kind: None,
        id: id(name),
        element: id("inner_flow"),
        source_element: Some(id(source)),
        target_element: Some(id(target)),
        waypoints: vec![
            Point {
                x: 130.0 + offset,
                y: 70.0,
            },
            Point {
                x: 180.0 + offset,
                y: 70.0,
            },
        ],
        label: Some(processcraft_core::model::Label {
            di_metadata: None,
            id: None,
            bounds: Some(Bounds {
                x: 140.0 + offset,
                y: 55.0,
                width: 30.0,
                height: 10.0,
            }),
            style: None,
            style_ref: None,
        }),
        label_visibility: None,
        style: None,
    };
    doc.diagrams[0].edges = vec![
        edge("edge_a", "grand_a", "grand2_a", 0.0),
        edge("edge_b", "grand_b", "grand2_b", 1000.0),
    ];
    let activity = |name: &str| ChoreographyActivity {
        loop_type: None,
        details: None,
        flow_metadata: None,
        correlation_keys: vec![],
        base: None,
        id: id(name),
        name: None,
        participant_refs: vec![
            ModelReference::Local(id("p")),
            ModelReference::Local(id("q")),
        ],
        initiating_participant: Some(ModelReference::Local(id("p"))),
        message_flow_refs: vec![],
    };
    let mut nested = activity("a");
    nested.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![
                ChoreographyFlowElement::Activity(Box::new(activity("grand"))),
                ChoreographyFlowElement::Activity(Box::new(activity("grand2"))),
                ChoreographyFlowElement::SequenceFlow(Box::new(Flow {
                    message_ref: None,
                    flow_metadata: None,
                    association_direction: None,
                    id: id("inner_flow"),
                    kind: FlowKind::Sequence,
                    source: Some(ModelReference::Local(id("grand"))),
                    target: Some(ModelReference::Local(id("grand2"))),
                    name: None,
                    documentation: String::new().into(),
                    base_extras: None,
                    condition: None,
                })),
            ],
        }),
        artifacts: vec![],
    });
    let mut sub = activity("sub");
    sub.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(nested))],
        }),
        artifacts: vec![],
    });
    doc.choreographies.push(Choreography {
        kind: None,
        graph: Some(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(sub))],
        }),
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: ["p", "q"]
            .iter()
            .map(|name| processcraft_core::model::Participant {
                interface_refs: vec![],
                endpoint_refs: vec![],
                multiplicity: None,
                base: None,
                id: id(name),
                name: None,
                process: None,
            })
            .collect(),
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    });
    let proposal = propose_choreography_scope_layout(
        &doc,
        &id("diagram"),
        &id("scope_a"),
        "session",
        1,
        &AtomicBool::new(false),
    )
    .unwrap();
    validate_proposal(&doc, &proposal).unwrap();
    let bounds = |name: &str| {
        proposal
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == id::<ElementSelector>(name))
            .unwrap()
            .bounds
            .clone()
    };
    assert_ne!(bounds("child_a").x, 10.0);
    assert_eq!(bounds("child_a").x, bounds("band_a").x);
    assert_eq!(bounds("child_b").x, 1010.0);
    assert_eq!(bounds("band_b").x, 1010.0);
    assert_eq!(bounds("grand_a").x - 30.0, bounds("child_a").x - 10.0);
    assert_eq!(bounds("grand_b").x, 1030.0);
    let inner_a = proposal
        .proposed
        .edges
        .iter()
        .find(|e| e.id == id::<ElementSelector>("edge_a"))
        .unwrap();
    let inner_b = proposal
        .proposed
        .edges
        .iter()
        .find(|e| e.id == id::<ElementSelector>("edge_b"))
        .unwrap();
    assert_eq!(inner_a.waypoints[0].x - 130.0, bounds("child_a").x - 10.0);
    assert_eq!(
        inner_a.label_bounds.as_ref().unwrap().x - 140.0,
        bounds("child_a").x - 10.0
    );
    assert_eq!(inner_b.waypoints[0].x, 1130.0);
    assert_eq!(inner_b.label_bounds.as_ref().unwrap().x, 1140.0);
    let mut single_view = doc.clone();
    single_view.diagrams[0].shapes.retain(|s| {
        !["scope_b", "child_b", "band_b", "grand_b", "grand2_b"]
            .iter()
            .any(|name| s.id == id::<ElementSelector>(name))
    });
    single_view.diagrams[0]
        .edges
        .retain(|e| e.id != id::<ElementSelector>("edge_b"));
    let root_proposal = propose_layout(
        &single_view,
        &id("diagram"),
        "root",
        1,
        &AtomicBool::new(false),
    )
    .unwrap();
    validate_proposal(&single_view, &root_proposal).unwrap();
    let root_shape = |name: &str| {
        root_proposal
            .proposed
            .shapes
            .iter()
            .find(|s| s.id == id::<ElementSelector>(name))
            .unwrap()
            .bounds
            .clone()
    };
    assert_eq!(root_shape("grand_a").x - 30.0, root_shape("scope_a").x);
    let root_edge = root_proposal
        .proposed
        .edges
        .iter()
        .find(|e| e.id == id::<ElementSelector>("edge_a"))
        .unwrap();
    assert_eq!(root_edge.waypoints[0].x - 130.0, root_shape("scope_a").x);
    let mut changed = doc.clone();
    changed.diagrams[0].shapes[2].bounds.x += 1.0;
    assert_eq!(
        validate_proposal(&changed, &proposal).unwrap_err().code,
        "stale"
    );
    use processcraft_core::commands::{Envelope, Mutation, Request, Session};
    use processcraft_core::model::ProjectData;
    use std::sync::Arc;
    let mut project = ProjectData::new("project".into());
    project.documents.push(Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let before = serde_json::to_vec(&session.snapshot().project).unwrap();
    let proposal = propose_choreography_scope_layout(
        &session.snapshot().project.documents[0],
        &id("diagram"),
        &id("scope_a"),
        &session.id(),
        session.snapshot().generation,
        &AtomicBool::new(false),
    )
    .unwrap();
    let apply = |session: &mut Session, request| {
        session
            .apply(
                Envelope {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    session_id: session.id(),
                    expected_generation: None,
                },
                request,
            )
            .unwrap();
    };
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::ApplyLayout { proposal }],
        },
    );
    let after = session.snapshot().content_id;
    apply(&mut session, Request::Undo);
    assert_eq!(
        serde_json::to_vec(&session.snapshot().project).unwrap(),
        before
    );
    apply(&mut session, Request::Redo);
    assert_eq!(session.snapshot().content_id, after);
}

#[test]
fn choreography_layout_routes_intermediate_events_and_gateway_kinds() {
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyFlowElement, ChoreographyGraph,
    };
    let mut doc = fixture();
    let nodes = std::mem::take(&mut doc.processes[0].nodes);
    doc.processes.clear();
    doc.diagrams[0].plane = id("choreo");
    let mut kinds = [
        NodeKind::IntermediateCatchEvent,
        NodeKind::IntermediateThrowEvent,
        NodeKind::InclusiveGateway,
        NodeKind::EventBasedGateway,
    ]
    .into_iter();
    let elements = nodes
        .into_iter()
        .map(|mut n| {
            n.kind = kinds.next().unwrap();
            if matches!(
                n.kind,
                NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
            ) {
                ChoreographyFlowElement::Event(Box::new(n))
            } else {
                ChoreographyFlowElement::Gateway(Box::new(n))
            }
        })
        .collect();
    doc.choreographies.push(Choreography {
        kind: None,
        graph: Some(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: elements,
        }),
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id("choreo"),
        name: None,
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    });
    let proposal = layout(&doc);
    validate_proposal(&doc, &proposal).unwrap();
    assert_eq!(proposal.proposed.shapes.len(), 4);
}
/// Ordinary process shapes: gateway/event nodes need ports facing their targets.
fn ordinary_process(
    nodes: &[(&str, NodeKind)],
    flows: &[(&str, &str)],
    lane: bool,
) -> BpmnDocument {
    let mut d = fixture();
    let template = d.processes[0].nodes[0].clone();
    let shape = d.diagrams[0].shapes[0].clone();
    d.processes[0].nodes = nodes
        .iter()
        .map(|(name, kind)| Node {
            id: id(name),
            kind: *kind,
            ..template.clone()
        })
        .collect();
    d.diagrams[0].shapes = nodes
        .iter()
        .map(|(name, kind)| {
            let size = match kind {
                NodeKind::StartEvent | NodeKind::EndEvent => 36.0,
                NodeKind::ExclusiveGateway | NodeKind::ParallelGateway => 50.0,
                _ => 0.0,
            };
            let mut s = shape.clone();
            s.id = id(&format!("shape_{name}"));
            s.element = id(name);
            if size > 0.0 {
                s.bounds.width = size;
                s.bounds.height = size;
            }
            s
        })
        .collect();
    for (a, b) in flows {
        flow(&mut d, &format!("{a}_{b}"), a, b);
    }
    if lane {
        d.processes[0].lanes.push(processcraft_core::model::Lane {
            base: None,
            child_lane_set_base: None,
            child_lane_set_id: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            id: id("lane"),
            name: Some("Lane".into()),
            parent: None,
            members: nodes.iter().map(|(name, _)| id(name)).collect(),
        });
        let mut s = shape;
        s.id = id("lane_di");
        s.element = id("lane");
        d.diagrams[0].shapes.push(s);
    }
    d
}
fn assert_layout_succeeds(d: &BpmnDocument) {
    let p = propose_layout(d, &id("diagram"), "s", 0, &AtomicBool::new(false))
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    validate_proposal(d, &p).unwrap();
    let container = p
        .proposed
        .shapes
        .iter()
        .find(|s| s.id == id::<ElementSelector>("lane_di"));
    for edge in &p.proposed.edges {
        for point in &edge.waypoints {
            assert!(point.y >= 0.0, "route leaves the diagram: {point:?}");
            if let Some(lane) = container {
                assert!(point.y <= lane.bounds.y + lane.bounds.height);
            }
        }
    }
}
#[test]
fn parallel_split_and_join_layout_succeeds() {
    use NodeKind::*;
    let d = ordinary_process(
        &[
            ("start", StartEvent),
            ("split", ParallelGateway),
            ("t1", Task),
            ("t2", Task),
            ("join", ParallelGateway),
            ("end", EndEvent),
        ],
        &[
            ("start", "split"),
            ("split", "t1"),
            ("split", "t2"),
            ("t1", "join"),
            ("t2", "join"),
            ("join", "end"),
        ],
        false,
    );
    assert_layout_succeeds(&d);
}
#[test]
fn three_way_exclusive_split_layout_succeeds() {
    use NodeKind::*;
    let d = ordinary_process(
        &[
            ("start", StartEvent),
            ("xor", ExclusiveGateway),
            ("t1", Task),
            ("t2", Task),
            ("t3", Task),
            ("end", EndEvent),
        ],
        &[
            ("start", "xor"),
            ("xor", "t1"),
            ("xor", "t2"),
            ("xor", "t3"),
            ("t1", "end"),
            ("t2", "end"),
            ("t3", "end"),
        ],
        false,
    );
    assert_layout_succeeds(&d);
}
#[test]
fn task_gateway_loop_back_layout_succeeds_with_and_without_lane() {
    use NodeKind::*;
    for lane in [false, true] {
        let d = ordinary_process(
            &[
                ("start", StartEvent),
                ("work", Task),
                ("check", ExclusiveGateway),
                ("end", EndEvent),
            ],
            &[
                ("start", "work"),
                ("work", "check"),
                ("check", "work"),
                ("check", "end"),
            ],
            lane,
        );
        assert_layout_succeeds(&d);
    }
}
