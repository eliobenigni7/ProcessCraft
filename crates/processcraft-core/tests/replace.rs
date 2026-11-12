//! Replace (morph) through the shared command protocol: option tables, in-place replacement,
//! flow cleanup, refusals, and undo.
use processcraft_core::{
    bpmn_model::{EventDefinition, NodeDetails},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementSelector},
    model::*,
    replace::{ReplaceTarget, Trigger, replace_options},
};
use serde_json::json;

fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn send(s: &mut Session, request: Request) -> Result<(), CommandError> {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request).map(|_| ())
}
fn batch(s: &mut Session, mutations: Vec<Mutation>) -> Result<(), CommandError> {
    send(s, Request::Batch { mutations })
}
fn session() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let setup: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    send(&mut s, setup).unwrap();
    s
}
fn document(s: &Session) -> BpmnDocument {
    (*s.snapshot().project.documents[0]).clone()
}
fn node(s: &Session, id: &str) -> Node {
    processcraft_core::queries::find_node(&document(s), &sel(id))
        .unwrap()
        .clone()
}
fn shape(s: &Session, id: &str) -> Shape {
    document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(id))
        .unwrap()
        .clone()
}
fn flow_ids(s: &Session) -> Vec<String> {
    document(s).processes[0]
        .flows
        .iter()
        .map(|f| f.id.to_string())
        .collect()
}
fn options(s: &Session, id: &str) -> Vec<ReplaceTarget> {
    replace_options(&document(s), &sel(id))
}
fn kind(kind: NodeKind) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: None,
        non_interrupting: false,
        event_sub_process: false,
        collapsed: false,
    }
}
fn typed(kind: NodeKind, trigger: Trigger, non_interrupting: bool) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: Some(trigger),
        non_interrupting,
        event_sub_process: false,
        collapsed: false,
    }
}
fn replace(id: &str, target: ReplaceTarget) -> Mutation {
    Mutation::ReplaceElement {
        document_id: docid(),
        element_id: sel(id),
        target,
    }
}
fn create(element: serde_json::Value, bounds: [f64; 4]) -> Vec<Mutation> {
    let id = element["id"].as_str().unwrap().to_owned();
    serde_json::from_value(json!([
        {"type": "createElement", "document_id": docid(), "process_id": "procurement", "element": element},
        {"type": "createShape", "document_id": docid(), "diagram_id": "procurement_diagram",
         "shape": {"id": format!("{id}_di"), "element": id, "bounds": {"x": bounds[0], "y": bounds[1], "width": bounds[2], "height": bounds[3]}, "label": null, "style": null}}
    ]))
    .unwrap()
}
fn element(id: &str, kind: &str) -> serde_json::Value {
    json!({"id": id, "name": null, "documentation": "", "kind": kind, "owner": "", "contact": ""})
}

#[test]
fn option_tables_follow_bpmn_js_per_family() {
    let mut s = session();
    let labels = |s: &Session, id: &str| -> Vec<String> {
        options(s, id).iter().map(ReplaceTarget::label).collect()
    };
    // Top-level start: blank start/throw/end and four typed starts, minus itself.
    assert_eq!(
        labels(&s, "start"),
        [
            "Intermediate event",
            "End event",
            "Message start event",
            "Timer start event",
            "Conditional start event",
            "Signal start event"
        ]
    );
    // Gateways: the other four kinds.
    assert_eq!(
        options(&s, "decision"),
        [
            NodeKind::ParallelGateway,
            NodeKind::InclusiveGateway,
            NodeKind::ComplexGateway,
            NodeKind::EventBasedGateway
        ]
        .map(kind)
    );
    // Tasks: the other task types, call activity, collapsed/expanded (ad-hoc) subprocesses.
    let task = options(&s, "order");
    assert_eq!(task.len(), 12);
    assert!(!task.contains(&kind(NodeKind::Task)) && task.contains(&kind(NodeKind::UserTask)));
    assert!(labels(&s, "order").contains(&"Collapsed ad-hoc subprocess".to_owned()));
    // End events outside a transaction offer no cancel end.
    let end = labels(&s, "end");
    assert!(
        end.contains(&"Terminate end event".into()) && !end.contains(&"Cancel end event".into())
    );
    // Boundary events: interrupting and non-interrupting, no cancel on a plain task.
    let mut boundary = element("timeout", "BoundaryEvent");
    boundary["details"] = json!({"type": "event", "definitions": [], "attached_to": "order", "cancel_activity": true});
    batch(&mut s, create(boundary, [560.0, 332.0, 36.0, 36.0])).unwrap();
    let boundary = options(&s, "timeout");
    assert_eq!(boundary.len(), 12);
    assert!(boundary.contains(&typed(NodeKind::BoundaryEvent, Trigger::Timer, true)));
    assert!(!boundary.contains(&typed(NodeKind::BoundaryEvent, Trigger::Cancel, false)));
    // Data references swap; sequence flows: plain/default/conditional by source and state.
    let mut store = element("store", "DataStoreReference");
    store["details"] = json!({"type": "dataStoreReference", "data_store_ref": null});
    batch(&mut s, create(store, [700.0, 420.0, 50.0, 50.0])).unwrap();
    assert_eq!(options(&s, "store"), [kind(NodeKind::DataObjectReference)]);
    assert_eq!(
        options(&s, "flow_review_decision"),
        [ReplaceTarget::DefaultFlow, ReplaceTarget::ConditionalFlow]
    );
    assert_eq!(options(&s, "flow_approved"), [ReplaceTarget::DefaultFlow]);
    assert!(options(&s, "flow_start_review").is_empty());
}

#[test]
fn event_subprocess_starts_offer_non_interrupting_and_typed_variants() {
    let mut s = session();
    let expanded = ReplaceTarget::Node {
        kind: NodeKind::SubProcess,
        trigger: None,
        non_interrupting: false,
        event_sub_process: false,
        collapsed: false,
    };
    batch(&mut s, vec![replace("order", expanded)]).unwrap();
    let mut start = element("inner_start", "StartEvent");
    start["details"] = json!({"type": "event", "definitions": [{"type": "message", "id": null, "message_ref": null, "operation_ref": null}], "attached_to": null, "cancel_activity": null});
    let mut mutations = create(start, [470.0, 230.0, 36.0, 36.0]);
    if let Mutation::CreateElement { process_id, .. } = &mut mutations[0] {
        *process_id = sel("order");
    }
    batch(&mut s, mutations).unwrap();
    // Plain subprocess: typed starts are not offered for its start events.
    assert!(!options(&s, "inner_start").iter().any(|t| matches!(
        t,
        ReplaceTarget::Node {
            kind: NodeKind::StartEvent,
            trigger: Some(_),
            ..
        }
    )));
    let event_sub = ReplaceTarget::Node {
        kind: NodeKind::SubProcess,
        trigger: None,
        non_interrupting: false,
        event_sub_process: true,
        collapsed: false,
    };
    batch(&mut s, vec![replace("order", event_sub)]).unwrap();
    // An event subprocess loses its sequence flows.
    assert!(!flow_ids(&s).contains(&"flow_approved".into()));
    assert!(!flow_ids(&s).contains(&"flow_order_end".into()));
    let inner = options(&s, "inner_start");
    assert!(inner.contains(&typed(NodeKind::StartEvent, Trigger::Message, true)));
    assert!(inner.contains(&typed(NodeKind::StartEvent, Trigger::Error, false)));
    assert!(!inner.contains(&typed(NodeKind::StartEvent, Trigger::Message, false)));
    // Toggle to non-interrupting keeps the definition.
    batch(
        &mut s,
        vec![replace(
            "inner_start",
            typed(NodeKind::StartEvent, Trigger::Message, true),
        )],
    )
    .unwrap();
    match node(&s, "inner_start").details {
        Some(NodeDetails::Event {
            definitions,
            is_interrupting,
            ..
        }) => {
            assert_eq!(is_interrupting, Some(false));
            assert!(matches!(definitions[..], [EventDefinition::Message { .. }]));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn replacing_a_task_keeps_identity_flows_lane_and_di_and_undoes_in_one_step() {
    let mut s = session();
    let before = s.snapshot();
    let (old, old_shape) = (node(&s, "order"), shape(&s, "order"));
    batch(&mut s, vec![replace("order", kind(NodeKind::UserTask))]).unwrap();
    let new = node(&s, "order");
    assert_eq!(new.kind, NodeKind::UserTask);
    assert_eq!((&new.id, &new.name), (&old.id, &old.name));
    assert_eq!(flow_ids(&s), flow_ids_of(&before));
    assert_eq!(shape(&s, "order").bounds, old_shape.bounds);
    assert!(
        document(&s).processes[0].lanes[1]
            .members
            .contains(&sel("order"))
    );
    assert_eq!(s.snapshot().undo_count, before.undo_count + 1);
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
}
fn flow_ids_of(snapshot: &processcraft_core::commands::Snapshot) -> Vec<String> {
    snapshot.project.documents[0].processes[0]
        .flows
        .iter()
        .map(|f| f.id.to_string())
        .collect()
}

#[test]
fn growing_into_an_expanded_subprocess_resizes_around_the_center_and_redocks_edges() {
    let mut s = session();
    let expanded = ReplaceTarget::Node {
        kind: NodeKind::AdHocSubProcess,
        trigger: None,
        non_interrupting: false,
        event_sub_process: false,
        collapsed: false,
    };
    batch(&mut s, vec![replace("order", expanded)]).unwrap();
    let shape = shape(&s, "order");
    assert_eq!(
        shape.bounds,
        Bounds {
            x: 410.0,
            y: 210.0,
            width: 350.0,
            height: 200.0
        }
    );
    assert_eq!(shape.is_expanded, Some(true));
    let edges = document(&s).diagrams[0].edges.clone();
    let points = |id: &str| {
        edges
            .iter()
            .find(|e| e.element == sel(id))
            .unwrap()
            .waypoints
            .iter()
            .map(|p| (p.x, p.y))
            .collect::<Vec<_>>()
    };
    // The left-middle entry and right-middle exit move with the outline.
    assert_eq!(points("flow_approved").last(), Some(&(410.0, 310.0)));
    assert_eq!(points("flow_order_end").first(), Some(&(760.0, 310.0)));
    assert!(matches!(
        node(&s, "order").details,
        Some(NodeDetails::SubProcess { .. })
    ));
}

#[test]
fn flows_the_rules_no_longer_allow_are_removed_with_their_edges() {
    let mut s = session();
    let label = shape(&s, "end").label.unwrap().bounds;
    batch(&mut s, vec![replace("end", kind(NodeKind::StartEvent))]).unwrap();
    assert_eq!(node(&s, "end").kind, NodeKind::StartEvent);
    let flows = flow_ids(&s);
    assert!(!flows.contains(&"flow_rejected".into()) && !flows.contains(&"flow_order_end".into()));
    assert!(
        document(&s).diagrams[0]
            .edges
            .iter()
            .all(|e| e.element != sel("flow_rejected") && e.element != sel("flow_order_end"))
    );
    // Same size class: shape and external label stay.
    assert_eq!(shape(&s, "end").label.unwrap().bounds, label);
    // A gateway's default flow goes when the new kind cannot hold one; an event-based gateway
    // keeps only flows to receive tasks or message/timer/conditional/signal catch events.
    batch(
        &mut s,
        vec![Mutation::ReplaceElement {
            document_id: docid(),
            element_id: sel("flow_approved"),
            target: ReplaceTarget::DefaultFlow,
        }],
    )
    .unwrap();
    assert!(
        document(&s).processes[0]
            .flows
            .iter()
            .any(|f| f.id == sel("flow_approved") && f.condition.is_none())
    );
    batch(
        &mut s,
        vec![replace("decision", kind(NodeKind::ParallelGateway))],
    )
    .unwrap();
    assert_eq!(node(&s, "decision").details, None);
    batch(
        &mut s,
        vec![replace("decision", kind(NodeKind::EventBasedGateway))],
    )
    .unwrap();
    assert!(!flow_ids(&s).contains(&"flow_approved".into()));
}

#[test]
fn sequence_flow_variants_set_condition_and_default() {
    let mut s = session();
    let flow = |s: &Session| {
        document(s).processes[0]
            .flows
            .iter()
            .find(|f| f.id == sel("flow_review_decision"))
            .unwrap()
            .clone()
    };
    let default = |s: &Session| node(s, "review").activity.and_then(|a| a.default_flow);
    let apply = |s: &mut Session, target| {
        batch(
            s,
            vec![Mutation::ReplaceElement {
                document_id: docid(),
                element_id: sel("flow_review_decision"),
                target,
            }],
        )
    };
    apply(&mut s, ReplaceTarget::ConditionalFlow).unwrap();
    assert!(flow(&s).condition.is_some());
    assert_eq!(
        options(&s, "flow_review_decision"),
        [ReplaceTarget::SequenceFlow, ReplaceTarget::DefaultFlow]
    );
    apply(&mut s, ReplaceTarget::DefaultFlow).unwrap();
    assert!(flow(&s).condition.is_none());
    assert_eq!(default(&s), Some(sel("flow_review_decision")));
    apply(&mut s, ReplaceTarget::SequenceFlow).unwrap();
    assert_eq!((flow(&s).condition, default(&s)), (None, None));
}

#[test]
fn kind_specific_data_is_refused_and_state_is_unchanged() {
    let mut s = session();
    batch(&mut s, vec![replace("order", kind(NodeKind::ScriptTask))]).unwrap();
    batch(
        &mut s,
        vec![Mutation::SetScriptBody {
            document_id: docid(),
            element_id: sel("order"),
            body: Some("run()".to_owned().into()),
        }],
    )
    .unwrap();
    let before = s.snapshot();
    let error = batch(&mut s, vec![replace("order", kind(NodeKind::Task))]).unwrap_err();
    assert!(
        matches!(&error, CommandError::InvalidParams(m) if m.contains("script")),
        "{error:?}"
    );
    let after = s.snapshot();
    assert_eq!(
        (after.content_id, after.generation),
        (before.content_id, before.generation)
    );
    // Not an option for this element.
    assert!(
        batch(
            &mut s,
            vec![replace("order", kind(NodeKind::ExclusiveGateway))]
        )
        .is_err()
    );
    // An authored conditional expression is refused; a palette-default timer is not.
    batch(
        &mut s,
        vec![replace(
            "start",
            typed(NodeKind::StartEvent, Trigger::Timer, false),
        )],
    )
    .unwrap();
    batch(
        &mut s,
        vec![replace(
            "start",
            typed(NodeKind::StartEvent, Trigger::Conditional, false),
        )],
    )
    .unwrap();
    batch(
        &mut s,
        vec![Mutation::SetNodeDetails {
            document_id: docid(),
            element_id: sel("start"),
            details: Some(NodeDetails::Event {
                definitions: vec![EventDefinition::Conditional {
                    id: None,
                    base: None,
                    expression: "amount > 100".into(),
                }],
                attached_to: None,
                cancel_activity: None,
                definition_refs: Vec::new(),
                parallel_multiple: None,
                is_interrupting: None,
                properties: Vec::new(),
                io: None,
            }),
        }],
    )
    .unwrap();
    assert!(
        batch(
            &mut s,
            vec![replace(
                "start",
                typed(NodeKind::StartEvent, Trigger::Signal, false)
            )]
        )
        .is_err()
    );
    // Same definition type in another position keeps it.
    batch(
        &mut s,
        vec![replace(
            "start",
            typed(
                NodeKind::IntermediateCatchEvent,
                Trigger::Conditional,
                false,
            ),
        )],
    )
    .unwrap();
    assert!(matches!(
        node(&s, "start").details,
        Some(NodeDetails::Event { definitions, .. })
            if matches!(&definitions[..], [EventDefinition::Conditional { expression, .. }] if expression.plain_text() == Some("amount > 100"))
    ));
}
