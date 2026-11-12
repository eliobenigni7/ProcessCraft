//! Round-3 repair tests: isImmediate validation, associationDirection edit/undo,
//! and typed StartEvent read-back.
use processcraft_core::{
    bpmn_activity::{FlowElementMetadata, ModelReference},
    bpmn_model::*,
    commands::{Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
    queries::{find_flow, find_node},
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    id("11111111-1111-4111-8111-111111111111")
}
fn send(s: &mut Session, request: Request) {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, request).unwrap();
}
fn apply(s: &mut Session, mutations: Vec<Mutation>) {
    send(s, Request::Batch { mutations });
}
fn undo(s: &mut Session) {
    send(s, Request::Undo);
}
fn base() -> Session {
    let mut s = Session::new(ProjectData::new("P".into())).unwrap();
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    send(&mut s, r);
    s
}
fn document(s: &Session) -> BpmnDocument {
    (*s.snapshot().project.documents[0]).clone()
}
fn validate(doc: BpmnDocument) -> Result<(), String> {
    let mut p = ProjectData::new("p".into());
    p.documents.push(std::sync::Arc::new(doc));
    p.validate()
}
fn immediate() -> Option<FlowElementMetadata> {
    Some(FlowElementMetadata {
        is_immediate: Some(true),
        ..Default::default()
    })
}
fn first_sequence(doc: &mut BpmnDocument) -> &mut Flow {
    doc.processes[0]
        .flows
        .iter_mut()
        .find(|f| f.kind == FlowKind::Sequence)
        .unwrap()
}

#[test]
fn is_immediate_validation_accepts_sequence_flow_and_names_the_rule_elsewhere() {
    let s = base();
    let mut doc = document(&s);
    validate(doc.clone()).unwrap();
    // sequence flow: accepted
    first_sequence(&mut doc).flow_metadata = immediate();
    validate(doc.clone()).unwrap();
    // the same metadata on an association (non-sequence flow) is rejected by name
    let mut assoc = doc.clone();
    let f = first_sequence(&mut assoc);
    f.kind = FlowKind::Association;
    f.condition = None;
    assert_eq!(
        validate(assoc).unwrap_err(),
        "Flow-element metadata requires a sequence flow"
    );
    // on a flow node (task) the isImmediate-specific rule fires
    let mut node = document(&s);
    node.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.kind == NodeKind::Task)
        .unwrap()
        .flow_metadata = immediate();
    assert_eq!(
        validate(node).unwrap_err(),
        "isImmediate requires a sequence flow"
    );
}

#[test]
fn is_immediate_on_a_message_flow_fails_document_validation() {
    let s = base();
    let mut doc = document(&s);
    doc.collaboration = Some(Collaboration {
        id: id("collab"),
        name: None,
        base: None,
        kind: None,
        extras: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![Flow {
            message_ref: None,
            flow_metadata: immediate(),
            association_direction: None,
            id: id("m"),
            kind: FlowKind::Message,
            source: Some(ModelReference::Local(id("a"))),
            target: Some(ModelReference::Local(id("b"))),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        }],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    assert_eq!(
        validate(doc.clone()).unwrap_err(),
        "Flow-element metadata requires a sequence flow"
    );
    doc.collaboration.as_mut().unwrap().flows[0].flow_metadata = None;
    // without the metadata that error is gone (any remaining error is a different one)
    assert_ne!(
        validate(doc).err().as_deref(),
        Some("Flow-element metadata requires a sequence flow")
    );
}

#[test]
fn association_direction_edit_clear_and_undo_on_an_existing_association() {
    let mut s = base();
    let mut doc = document(&s);
    let src = first_sequence(&mut doc).source.clone();
    apply(
        &mut s,
        vec![Mutation::CreateElement {
            document_id: docid(),
            process_id: id("procurement"),
            element: Node {
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                id: id::<ElementIdentity>("note"),
                name: None,
                documentation: String::new().into(),
                kind: NodeKind::TextAnnotation,
                details: None,
                loop_characteristics: None,
                io_specification: None,
                activity: None,
                owner: String::new(),
                contact: String::new(),
            },
        }],
    );
    apply(
        &mut s,
        vec![Mutation::CreateFlow {
            document_id: docid(),
            process_id: Some(id("procurement")),
            flow: Flow {
                message_ref: None,
                flow_metadata: None,
                base_extras: None,
                id: id("assoc"),
                kind: FlowKind::Association,
                source: src,
                target: Some(ModelReference::Local(id("note"))),
                name: None,
                documentation: String::new().into(),
                condition: None,
                association_direction: None,
            },
        }],
    );
    let read = |s: &Session| {
        find_flow(&s.snapshot().project.documents[0], &id("assoc"))
            .unwrap()
            .association_direction
    };
    let set = |s: &mut Session, d| {
        apply(
            s,
            vec![Mutation::SetAssociationDirection {
                document_id: docid(),
                flow_id: id("assoc"),
                direction: d,
            }],
        )
    };
    assert_eq!(read(&s), None);
    let id0 = s.snapshot().content_id;
    set(&mut s, Some(AssociationDirection::Both));
    assert_eq!(read(&s), Some(AssociationDirection::Both));
    set(&mut s, Some(AssociationDirection::One));
    assert_eq!(read(&s), Some(AssociationDirection::One));
    set(&mut s, None);
    assert_eq!(read(&s), None);
    undo(&mut s);
    assert_eq!(read(&s), Some(AssociationDirection::One));
    undo(&mut s);
    assert_eq!(read(&s), Some(AssociationDirection::Both));
    undo(&mut s);
    assert_eq!(read(&s), None);
    assert_eq!(s.snapshot().content_id, id0);
}

#[test]
fn start_event_node_reads_back_interrupting_parallel_multiple_and_definitions() {
    let mut s = base();
    let def = |i: &str| EventDefinition::Message {
        id: Some(id(i)),
        base: None,
        message_ref: None,
        operation_ref: None,
    };
    let details = NodeDetails::Event {
        definitions: vec![def("d1"), def("d2")],
        definition_refs: vec![],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: Some(true),
        is_interrupting: Some(false),
        properties: vec![],
        io: None,
    };
    apply(
        &mut s,
        vec![
            Mutation::CreateElement {
                document_id: docid(),
                process_id: id("procurement"),
                element: Node {
                    flow_metadata: None,
                    base_extras: None,
                    item_aware: None,
                    id: id::<ElementIdentity>("s2"),
                    name: None,
                    documentation: String::new().into(),
                    kind: NodeKind::StartEvent,
                    details: None,
                    loop_characteristics: None,
                    io_specification: None,
                    activity: None,
                    owner: String::new(),
                    contact: String::new(),
                },
            },
            Mutation::SetNodeDetails {
                document_id: docid(),
                element_id: id("s2"),
                details: Some(details),
            },
        ],
    );
    let snap = s.snapshot();
    let n = find_node(&snap.project.documents[0], &id::<ElementSelector>("s2")).unwrap();
    assert_eq!(n.kind, NodeKind::StartEvent);
    let Some(NodeDetails::Event {
        definitions,
        parallel_multiple,
        is_interrupting,
        ..
    }) = &n.details
    else {
        panic!("event details")
    };
    assert_eq!(*parallel_multiple, Some(true));
    assert_eq!(*is_interrupting, Some(false));
    assert_eq!(definitions, &vec![def("d1"), def("d2")]);
    undo(&mut s);
    assert!(
        find_node(
            &s.snapshot().project.documents[0],
            &id::<ElementSelector>("s2")
        )
        .is_none()
    );
}
