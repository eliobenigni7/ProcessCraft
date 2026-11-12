//! Repair tests for the foundation family (base types, documentation, flow element/node, callable).
use processcraft_core::{
    bpmn_activity::{
        CallableMetadata, FlowElementMetadata, ModelReference, ProcessActivityMetadata,
    },
    bpmn_catalog::{Interface, Operation, SupportingCatalog},
    bpmn_metadata::{Documentation, DocumentationSet},
    bpmn_model::{EventResource, QNameRef},
    bpmn_xml::MixedXml,
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, XmlId},
    model::*,
    queries::find_node,
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn apply(
    s: &mut Session,
    r: Request,
) -> Result<processcraft_core::commands::ChangeSet, CommandError> {
    let env = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, r)
}
fn batch(m: Vec<Mutation>) -> Request {
    Request::Batch { mutations: m }
}
fn fixture() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(&mut s, r).unwrap();
    s
}
fn err_text(r: Result<processcraft_core::commands::ChangeSet, CommandError>) -> String {
    format!("{:?}", r.unwrap_err())
}
fn xml(s: &str) -> Option<XmlId> {
    Some(id(s))
}

#[test]
fn base_element_id_is_read_back_renamed_and_collision_is_diagnosed() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    let old = find_node(&s.snapshot().project.documents[0], &id("review"))
        .unwrap()
        .id
        .selector();
    let node_id = |s: &Session, sel: &processcraft_core::ids::ElementSelector| {
        find_node(&s.snapshot().project.documents[0], sel)
            .unwrap()
            .id
            .xml_id()
            .cloned()
    };
    assert_eq!(node_id(&s, &old), xml("review"));
    let change = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("review"),
            xml_id: xml("review2"),
        }]),
    )
    .unwrap();
    assert_eq!(node_id(&s, &change.selector_mappings[0].to), xml("review2"));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(node_id(&s, &old), xml("review"));
    assert_eq!(s.snapshot().content_id, before);
    // collision with another element id is refused with a duplicate-id diagnostic, state unchanged
    let e = err_text(apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("review"),
            xml_id: xml("start"),
        }]),
    ));
    assert_eq!(e, "InvalidParams(\"XML ID already exists\")");
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn documentation_id_collision_and_format_diagnostics_are_specific() {
    let mut s = fixture();
    let doc = |idv: &str| {
        DocumentationSet::Structured(vec![Documentation {
            id: xml_ident(idv),
            text_format: None,
            content: MixedXml::Text("t".into()),
        }])
    };
    apply(
        &mut s,
        batch(vec![Mutation::ReplaceDocumentation {
            document_id: docid(),
            element_id: id("review"),
            documentation: doc("doc_a"),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let e = err_text(apply(
        &mut s,
        batch(vec![Mutation::ReplaceDocumentation {
            document_id: docid(),
            element_id: id("start"),
            documentation: doc("doc_a"),
        }]),
    ));
    assert_eq!(e, "InvalidParams(\"duplicate XML ID\")");
    assert_eq!(s.snapshot().content_id, before);
    let bad = Documentation {
        id: None,
        text_format: Some("x".repeat(16_385)),
        content: MixedXml::Text("t".into()),
    };
    assert_eq!(
        processcraft_core::bpmn_metadata::validate_documentation(&bad),
        Err("Invalid documentation text format".to_string())
    );
}
fn xml_ident(s: &str) -> Option<processcraft_core::ids::ElementIdentity> {
    Some(id(s))
}

#[test]
fn flow_element_name_edit_model_and_undo_for_node_and_flow() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    let names = |s: &Session| {
        let snap = s.snapshot();
        let p = &snap.project.documents[0].processes[0];
        let flow = p
            .flows
            .iter()
            .find(|f| f.id.xml_id() == Some(&id("flow_start_review")))
            .unwrap();
        (
            find_node(&snap.project.documents[0], &id("review"))
                .unwrap()
                .name
                .clone(),
            flow.name.clone(),
        )
    };
    let (n0, f0) = names(&s);
    apply(
        &mut s,
        batch(vec![
            Mutation::Rename {
                document_id: docid(),
                element_id: id("review"),
                name: Some("Renamed node".into()),
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: id("flow_start_review"),
                name: Some("Renamed flow".into()),
            },
        ]),
    )
    .unwrap();
    assert_eq!(
        names(&s),
        (
            Some("Renamed node".to_string()),
            Some("Renamed flow".to_string())
        )
    );
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("flow_start_review"),
            name: None,
        }]),
    )
    .unwrap();
    assert_eq!(names(&s).1, None);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(names(&s).1.as_deref(), Some("Renamed flow"));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!((names(&s).0, names(&s).1), (n0, f0));
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn flow_node_incoming_is_set_cleared_and_undone() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    let incoming = |s: &Session| {
        find_node(&s.snapshot().project.documents[0], &id("review"))
            .unwrap()
            .flow_metadata
            .clone()
    };
    let md = FlowElementMetadata {
        incoming: vec![ModelReference::Local(id("flow_start_review"))],
        ..Default::default()
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetNodeFlowMetadata {
            document_id: docid(),
            element_id: id("review"),
            metadata: Some(md.clone()),
        }]),
    )
    .unwrap();
    assert_eq!(incoming(&s), Some(md));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(incoming(&s), None);
    assert_eq!(s.snapshot().content_id, before);
    // clearing: set, then clear with None, then undo restores the set value
    let md = FlowElementMetadata {
        incoming: vec![ModelReference::Local(id("flow_start_review"))],
        ..Default::default()
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetNodeFlowMetadata {
            document_id: docid(),
            element_id: id("review"),
            metadata: Some(md.clone()),
        }]),
    )
    .unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::SetNodeFlowMetadata {
            document_id: docid(),
            element_id: id("review"),
            metadata: None,
        }]),
    )
    .unwrap();
    assert_eq!(incoming(&s), None);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(incoming(&s), Some(md));
}

#[test]
fn callable_supported_interface_refs_are_set_cleared_and_undone() {
    let mut s = fixture();
    apply(
        &mut s,
        batch(vec![
            Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![EventResource::Message {
                    id: id("message"),
                    base: None,
                    name: Some("Message".into()),
                    item_ref: None,
                }],
            },
            Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: SupportingCatalog {
                    interfaces: vec![Interface {
                        id: Some(id("api")),
                        base: None,
                        name: "API".into(),
                        implementation_ref: None,
                        operations: vec![Operation {
                            id: Some(id("operation")),
                            base: None,
                            name: "Operation".into(),
                            implementation_ref: None,
                            in_message_ref: QNameRef {
                                namespace: GENERATED_TARGET_NAMESPACE.into(),
                                local: id("message"),
                            },
                            out_message_ref: None,
                            error_refs: vec![],
                        }],
                    }],
                    ..Default::default()
                },
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let refs = |s: &Session| {
        s.snapshot().project.documents[0].processes[0]
            .activity_metadata
            .as_ref()
            .map(|m| m.callable.supported_interface_refs.clone())
            .unwrap_or_default()
    };
    let want = vec![ModelReference::Qualified(QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("api"),
    })];
    let set = |refs: Vec<ModelReference>| {
        batch(vec![Mutation::SetProcessActivityMetadata {
            document_id: docid(),
            process_id: id("procurement"),
            metadata: Some(ProcessActivityMetadata {
                callable: CallableMetadata {
                    supported_interface_refs: refs,
                    ..Default::default()
                },
                ..Default::default()
            }),
        }])
    };
    apply(&mut s, set(want.clone())).unwrap();
    assert_eq!(refs(&s), want);
    apply(&mut s, set(vec![])).unwrap();
    assert!(refs(&s).is_empty());
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(refs(&s), want);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    // dangling interface reference is refused, state unchanged
    let dangling = vec![ModelReference::Qualified(QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("missing_api"),
    })];
    let e = err_text(apply(&mut s, set(dangling)));
    assert_eq!(
        e,
        "InvalidParams(\"change introduces an unresolved local activity reference\")"
    );
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn native_duplicate_xml_id_validate_names_the_rule() {
    let s = fixture();
    let mut p = (*s.snapshot().project).clone();
    let doc = std::sync::Arc::make_mut(&mut p.documents[0]);
    let dup = doc.processes[0].nodes[0].id.xml_id().cloned();
    doc.processes[0].nodes[1].id = processcraft_core::ids::ElementIdentity::Native {
        native_id: processcraft_core::ids::NativeElementId::new(),
        xml_id: dup,
    };
    assert_eq!(p.validate(), Err("duplicate XML ID".to_string()));
}
