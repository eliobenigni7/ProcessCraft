//! M3 process/collaboration family: command edits with undo, model values and
//! dangling-reference diagnostics for process, lane, collaboration, participant,
//! message flow, root message and sequence flow fields.
use processcraft_core::{
    bpmn_activity::{ModelReference, ProcessActivityMetadata, ProcessType},
    bpmn_collaboration::CollaborationExtras,
    bpmn_model::{EventResource, QNameRef},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
    validation::activity_family_reference_issues,
};

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn ident(s: &str) -> ElementIdentity {
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(sel(s))
}
fn ext(s: &str) -> ModelReference {
    ModelReference::Qualified(QNameRef {
        namespace: "urn:external".into(),
        local: s.parse().unwrap(),
    })
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn send(s: &mut Session, request: Request) -> Result<(), CommandError> {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, request).map(|_| ())
}
fn run(s: &mut Session, mutations: Vec<Mutation>) {
    send(s, Request::Batch { mutations }).unwrap();
}
/// A rejected batch must name the unresolved reference and leave state untouched.
fn rejected(s: &mut Session, mutations: Vec<Mutation>) {
    let (content, generation) = (s.snapshot().content_id, s.snapshot().generation);
    let error = send(s, Request::Batch { mutations }).unwrap_err();
    assert!(
        matches!(&error, CommandError::InvalidParams(m) if m.contains("unresolved local activity reference")),
        "{error:?}"
    );
    assert_eq!(s.snapshot().content_id, content);
    assert_eq!(s.snapshot().generation, generation);
}
fn undo(s: &mut Session) {
    send(s, Request::Undo).unwrap();
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
fn collaboration(id: &str) -> Collaboration {
    Collaboration {
        id: ident(id),
        name: None,
        base: None,
        kind: None,
        extras: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    }
}
fn participant(id: &str, name: Option<&str>) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: ident(id),
        name: name.map(Into::into),
        process: None,
    }
}
fn flow(id: &str, kind: FlowKind, name: Option<&str>, source: &str, target: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: ident(id),
        kind,
        source: Some(local(source)),
        target: Some(local(target)),
        name: name.map(Into::into),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    }
}
fn set_root(c: Collaboration) -> Mutation {
    Mutation::SetCollaborationRoot {
        document_id: docid(),
        collaboration: c,
    }
}
fn collab_of(s: &Session) -> Collaboration {
    document(s).collaboration.unwrap()
}

#[test]
fn collaboration_is_closed_and_choreography_refs_edit_with_undo() {
    let mut s = session();
    let before = s.snapshot().content_id;
    let mut c = collaboration("collab");
    c.extras = Some(CollaborationExtras {
        is_closed: Some(true),
        choreography_refs: vec![local("no_such_choreography")],
        ..Default::default()
    });
    rejected(&mut s, vec![set_root(c.clone())]);
    c.extras.as_mut().unwrap().choreography_refs = vec![ext("chor")];
    run(&mut s, vec![set_root(c)]);
    let extras = collab_of(&s).extras.unwrap();
    assert_eq!(extras.is_closed, Some(true));
    assert_eq!(extras.choreography_refs, vec![ext("chor")]);
    let mut c = collab_of(&s);
    c.extras.as_mut().unwrap().is_closed = Some(false);
    c.extras.as_mut().unwrap().choreography_refs.clear();
    run(&mut s, vec![set_root(c)]);
    let extras = collab_of(&s).extras.unwrap();
    assert_eq!(extras.is_closed, Some(false));
    assert!(extras.choreography_refs.is_empty());
    undo(&mut s);
    let extras = collab_of(&s).extras.unwrap();
    assert_eq!(extras.is_closed, Some(true));
    assert_eq!(extras.choreography_refs, vec![ext("chor")]);
    undo(&mut s);
    assert!(document(&s).collaboration.is_none());
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn collaboration_choreography_ref_dangling_is_reported() {
    let mut doc = BpmnDocument::new("d".into(), ident("defs"));
    let mut c = collaboration("collab");
    c.extras = Some(CollaborationExtras {
        choreography_refs: vec![
            local("missing_chor"),
            ModelReference::Qualified(QNameRef {
                namespace: "urn:external".into(),
                local: "remote".parse().unwrap(),
            }),
        ],
        ..Default::default()
    });
    doc.collaboration = Some(c);
    let issues = activity_family_reference_issues(&doc);
    assert_eq!(issues.len(), 1, "external stays opaque: {issues:?}");
    assert_eq!(issues[0].field, "collaboration.choreographyRef");
    assert_eq!(issues[0].owner, sel("collab"));
    assert_eq!(issues[0].target, local("missing_chor"));
}

#[test]
fn participant_name_and_catalog_refs_edit_with_undo() {
    let mut s = session();
    let mut c = collaboration("collab");
    c.participants.push(participant("buyer", Some("Buyer")));
    run(&mut s, vec![set_root(c)]);
    let mut extra = participant("seller", None);
    extra.interface_refs = vec![local("no_interface")];
    extra.endpoint_refs = vec![local("no_endpoint")];
    rejected(
        &mut s,
        vec![Mutation::CreateParticipant {
            document_id: docid(),
            participant: extra.clone(),
        }],
    );
    extra.interface_refs = vec![ext("iface")];
    extra.endpoint_refs = vec![ext("endpoint")];
    run(
        &mut s,
        vec![
            Mutation::CreateParticipant {
                document_id: docid(),
                participant: extra,
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: sel("buyer"),
                name: Some("Customer".into()),
            },
        ],
    );
    let c = collab_of(&s);
    assert_eq!(c.participants[0].name.as_deref(), Some("Customer"));
    assert_eq!(c.participants[1].id, ident("seller"));
    assert_eq!(c.participants[1].interface_refs, vec![ext("iface")]);
    assert_eq!(c.participants[1].endpoint_refs, vec![ext("endpoint")]);
    undo(&mut s);
    let c = collab_of(&s);
    assert_eq!(c.participants.len(), 1);
    assert_eq!(c.participants[0].name.as_deref(), Some("Buyer"));
}

#[test]
fn participant_interface_and_endpoint_refs_dangling_are_reported() {
    let mut doc = BpmnDocument::new("d".into(), ident("defs"));
    let mut c = collaboration("collab");
    let mut p = participant("buyer", Some("Buyer"));
    p.interface_refs = vec![local("no_interface")];
    p.endpoint_refs = vec![local("no_endpoint")];
    c.participants.push(p);
    doc.collaboration = Some(c);
    let issues = activity_family_reference_issues(&doc);
    let find = |field: &str| issues.iter().find(|i| i.field == field).unwrap();
    assert_eq!(
        find("participant.interfaceRef").target,
        local("no_interface")
    );
    assert_eq!(find("participant.interfaceRef").owner, sel("buyer"));
    assert_eq!(find("participant.endPointRef").target, local("no_endpoint"));
    assert_eq!(find("participant.endPointRef").owner, sel("buyer"));
}

#[test]
fn message_flow_name_ends_and_message_ref_edit_with_undo() {
    let mut s = session();
    let mut c = collaboration("collab");
    c.participants = vec![participant("buyer", None), participant("seller", None)];
    c.flows = vec![flow(
        "order_msg",
        FlowKind::Message,
        Some("Order"),
        "buyer",
        "seller",
    )];
    run(&mut s, vec![set_root(c)]);
    rejected(
        &mut s,
        vec![Mutation::SetMessageFlowReference {
            document_id: docid(),
            flow_id: sel("order_msg"),
            message_ref: Some(local("msg")),
        }],
    );
    run(
        &mut s,
        vec![Mutation::SetEventResources {
            document_id: docid(),
            resources: vec![EventResource::Message {
                id: ident("msg"),
                base: None,
                name: Some("Order".into()),
                item_ref: None,
            }],
        }],
    );
    run(
        &mut s,
        vec![
            Mutation::Rename {
                document_id: docid(),
                element_id: sel("order_msg"),
                name: Some("Purchase order".into()),
            },
            Mutation::SetMessageFlowReference {
                document_id: docid(),
                flow_id: sel("order_msg"),
                message_ref: Some(local("msg")),
            },
            Mutation::CreateFlow {
                document_id: docid(),
                process_id: None,
                flow: flow("reply_msg", FlowKind::Message, None, "seller", "buyer"),
            },
        ],
    );
    let c = collab_of(&s);
    assert_eq!(c.flows[0].name.as_deref(), Some("Purchase order"));
    assert_eq!(c.flows[0].message_ref, Some(local("msg")));
    assert_eq!(c.flows[1].source, Some(local("seller")));
    assert_eq!(c.flows[1].target, Some(local("buyer")));
    undo(&mut s);
    let c = collab_of(&s);
    assert_eq!(c.flows.len(), 1);
    assert_eq!(c.flows[0].name.as_deref(), Some("Order"));
    assert_eq!(c.flows[0].message_ref, None);
}

#[test]
fn message_flow_dangling_source_and_message_ref_are_reported() {
    let mut doc = BpmnDocument::new("d".into(), ident("defs"));
    let mut c = collaboration("collab");
    c.participants = vec![participant("seller", None)];
    let mut f = flow("order", FlowKind::Message, None, "ghost_source", "seller");
    f.message_ref = Some(local("ghost_message"));
    c.flows = vec![f];
    doc.collaboration = Some(c);
    let issues = activity_family_reference_issues(&doc);
    let source = issues.iter().find(|i| i.field == "messageFlow.sourceRef");
    assert_eq!(source.unwrap().target, local("ghost_source"));
    assert_eq!(source.unwrap().owner, sel("order"));
    let message = issues.iter().find(|i| i.field == "messageFlow.messageRef");
    assert_eq!(message.unwrap().target, local("ghost_message"));
    assert!(!issues.iter().any(|i| i.field == "messageFlow.targetRef"));
}

#[test]
fn root_message_name_and_item_ref_edit_with_undo() {
    let mut s = session();
    let item = QNameRef {
        namespace: "urn:items".into(),
        local: "Item".parse().unwrap(),
    };
    let message = |name: &str, item_ref| EventResource::Message {
        id: ident("msg"),
        base: None,
        name: Some(name.into()),
        item_ref,
    };
    for resource in [
        message("Order", Some(item.clone())),
        message("Order v2", None),
    ] {
        run(
            &mut s,
            vec![Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![resource],
            }],
        );
    }
    match &document(&s).event_resources[..] {
        [EventResource::Message { name, item_ref, .. }] => {
            assert_eq!(name.as_deref(), Some("Order v2"));
            assert_eq!(*item_ref, None);
        }
        other => panic!("{other:?}"),
    }
    undo(&mut s);
    match &document(&s).event_resources[..] {
        [EventResource::Message { name, item_ref, .. }] => {
            assert_eq!(name.as_deref(), Some("Order"));
            assert_eq!(item_ref.as_ref(), Some(&item));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn sequence_flow_source_edit_with_undo_and_dangling_source_is_reported() {
    let mut s = session();
    run(
        &mut s,
        vec![Mutation::CreateFlow {
            document_id: docid(),
            process_id: Some(sel("procurement")),
            flow: flow("extra_flow", FlowKind::Sequence, None, "review", "end"),
        }],
    );
    let doc = document(&s);
    let created = doc.processes[0]
        .flows
        .iter()
        .find(|f| f.id == ident("extra_flow"))
        .unwrap();
    assert_eq!(created.source, Some(local("review")));
    assert!(activity_family_reference_issues(&doc).is_empty());
    undo(&mut s);
    assert!(
        !document(&s).processes[0]
            .flows
            .iter()
            .any(|f| f.id == ident("extra_flow"))
    );
    let mut broken = doc;
    broken.processes[0]
        .flows
        .iter_mut()
        .find(|f| f.id == ident("extra_flow"))
        .unwrap()
        .source = Some(local("no_such_node"));
    let issues = activity_family_reference_issues(&broken);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].field, "sequenceFlow.sourceRef");
    assert_eq!(issues[0].owner, sel("extra_flow"));
    assert_eq!(issues[0].target, local("no_such_node"));
}

#[test]
fn process_executable_edit_with_undo() {
    let mut s = session();
    let get = |s: &Session| document(s).processes[0].is_executable;
    let original = get(&s);
    let target = Some(original != Some(true));
    for value in [target, None] {
        run(
            &mut s,
            vec![Mutation::SetProcessExecutable {
                document_id: docid(),
                process_id: sel("procurement"),
                is_executable: value,
            }],
        );
        assert_eq!(get(&s), value);
    }
    undo(&mut s);
    assert_eq!(get(&s), target);
    undo(&mut s);
    assert_eq!(get(&s), original);
}

#[test]
fn process_type_closed_supports_and_collaboration_ref_edit_with_undo() {
    let mut s = session();
    run(
        &mut s,
        vec![
            set_root(collaboration("collab")),
            Mutation::CreateProcess {
                document_id: docid(),
                process: serde_json::from_value(serde_json::json!({
                    "id": "other_process", "name": "Other", "documentation": "",
                    "nodes": [], "lanes": [], "flows": []
                }))
                .unwrap(),
            },
        ],
    );
    let set = |metadata| Mutation::SetProcessActivityMetadata {
        document_id: docid(),
        process_id: sel("procurement"),
        metadata,
    };
    let metadata = ProcessActivityMetadata {
        process_type: Some(ProcessType::Private),
        is_closed: Some(true),
        definitional_collaboration_ref: Some(local("collab")),
        supports: vec![local("other_process")],
        ..Default::default()
    };
    run(&mut s, vec![set(Some(metadata.clone()))]);
    assert_eq!(
        document(&s).processes[0].activity_metadata,
        Some(metadata.clone())
    );
    let mut changed = metadata;
    changed.process_type = Some(ProcessType::Public);
    changed.supports.clear();
    changed.definitional_collaboration_ref = None;
    run(&mut s, vec![set(Some(changed))]);
    let m = document(&s).processes[0].activity_metadata.clone().unwrap();
    assert_eq!(m.process_type, Some(ProcessType::Public));
    assert!(m.supports.is_empty() && m.definitional_collaboration_ref.is_none());
    undo(&mut s);
    let m = document(&s).processes[0].activity_metadata.clone().unwrap();
    assert_eq!(m.process_type, Some(ProcessType::Private));
    assert_eq!(m.is_closed, Some(true));
    assert_eq!(m.supports, vec![local("other_process")]);
    assert_eq!(m.definitional_collaboration_ref, Some(local("collab")));
    undo(&mut s);
    assert_eq!(document(&s).processes[0].activity_metadata, None);
}

#[test]
fn process_supports_and_collaboration_ref_dangling_are_reported() {
    let mut s = session();
    let metadata = Some(ProcessActivityMetadata {
        definitional_collaboration_ref: Some(local("no_collab")),
        supports: vec![local("no_process")],
        ..Default::default()
    });
    rejected(
        &mut s,
        vec![Mutation::SetProcessActivityMetadata {
            document_id: docid(),
            process_id: sel("procurement"),
            metadata: metadata.clone(),
        }],
    );
    let mut doc = document(&s);
    doc.processes[0].activity_metadata = metadata;
    let issues = activity_family_reference_issues(&doc);
    let supports = issues.iter().find(|i| i.field == "process.supports");
    assert_eq!(supports.unwrap().target, local("no_process"));
    assert_eq!(supports.unwrap().owner, sel("procurement"));
    let collab = issues
        .iter()
        .find(|i| i.field == "process.definitionalCollaborationRef");
    assert_eq!(collab.unwrap().target, local("no_collab"));
}

#[test]
fn lane_name_members_and_child_lane_set_edit_with_undo() {
    let mut s = session();
    let bounds: Vec<_> = document(&s)
        .diagrams
        .iter()
        .flat_map(|d| {
            d.shapes
                .iter()
                .filter(|sh| sh.element == sel("order"))
                .map(|sh| processcraft_core::commands::ShapeBounds {
                    diagram_id: d.id.selector(),
                    shape_id: sh.id.selector(),
                    bounds: sh.bounds.clone(),
                })
        })
        .collect();
    run(
        &mut s,
        vec![
            Mutation::CreateLane {
                document_id: docid(),
                process_id: sel("procurement"),
                lane: Lane {
                    base: None,
                    child_lane_set_id: None,
                    child_lane_set_base: None,
                    child_lane_set_name: None,
                    partition_element: None,
                    partition_element_ref: None,
                    id: ident("lane_new"),
                    name: Some("Audit".into()),
                    parent: None,
                    members: vec![],
                },
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: sel("lane_new"),
                name: Some("Compliance".into()),
            },
            Mutation::AssignLane {
                document_id: docid(),
                process_id: sel("procurement"),
                element_id: sel("order"),
                lane_id: Some(sel("lane_new")),
                bounds,
            },
            Mutation::SetChildLaneSetId {
                document_id: docid(),
                scope_id: sel("procurement"),
                lane_id: sel("lane_new"),
                child_lane_set_id: Some(ident("child_set")),
            },
        ],
    );
    let lane = |s: &Session, id: &str| {
        document(s).processes[0]
            .lanes
            .iter()
            .find(|l| l.id == ident(id))
            .cloned()
    };
    let new = lane(&s, "lane_new").unwrap();
    assert_eq!(new.name.as_deref(), Some("Compliance"));
    assert_eq!(new.members, vec![sel("order")]);
    assert_eq!(new.child_lane_set_id, Some(ident("child_set")));
    let old = lane(&s, "lane_procurement").unwrap();
    assert!(!old.members.contains(&sel("order")));
    undo(&mut s);
    assert!(lane(&s, "lane_new").is_none());
    let old = lane(&s, "lane_procurement").unwrap();
    assert!(old.members.contains(&sel("order")));
}

#[test]
fn process_typed_fields_survive_native_json_round_trip() {
    let mut s = session();
    run(
        &mut s,
        vec![Mutation::SetProcessActivityMetadata {
            document_id: docid(),
            process_id: sel("procurement"),
            metadata: Some(ProcessActivityMetadata {
                process_type: Some(ProcessType::Public),
                is_closed: Some(false),
                ..Default::default()
            }),
        }],
    );
    let json = serde_json::to_value(&*s.snapshot().project.documents[0]).unwrap();
    let meta = &json["processes"][0]["activity_metadata"];
    assert_eq!(meta["process_type"], "Public");
    assert_eq!(meta["is_closed"], false);
    let back: BpmnDocument = serde_json::from_value(json).unwrap();
    let m = back.processes[0].activity_metadata.as_ref().unwrap();
    assert_eq!(
        (m.process_type, m.is_closed),
        (Some(ProcessType::Public), Some(false))
    );
}
