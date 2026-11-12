use processcraft_core::{
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementKey},
    model::*,
};
use std::sync::Arc;
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn env(s: &Session) -> Envelope {
    Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: None,
    }
}
fn apply(
    s: &mut Session,
    r: Request,
) -> Result<processcraft_core::commands::ChangeSet, CommandError> {
    s.apply(env(s), r)
}
fn batch(m: Vec<Mutation>) -> Request {
    Request::Batch { mutations: m }
}
fn project() -> Session {
    Session::new(ProjectData::new("Project".into())).unwrap()
}
#[test]
fn additional_collaboration_crud_does_not_retarget_legacy_first_root() {
    use processcraft_core::bpmn_collaboration::CollaborationRootKind;
    let mut session = project();
    fixture(&mut session);
    let root = |name: &str, kind| Collaboration {
        id: id(name),
        kind,
        extras: None,
        base: None,
        name: Some(name.into()),
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: root("first", None),
            },
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: root("second", Some(CollaborationRootKind::GlobalConversation)),
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::DeleteCollaborationRoot {
            document_id: docid(),
            collaboration_id: id("first"),
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.project.documents[0].collaboration.is_none());
    assert_eq!(
        snapshot.project.documents[0]
            .additional_collaborations
            .len(),
        1
    );
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetCollaborationName {
                document_id: docid(),
                name: Some("wrong root".into())
            }])
        )
        .is_err()
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn additional_collaboration_annotation_edits_and_deletes_atomically() {
    use processcraft_core::{
        bpmn_collaboration::{CollaborationArtifact, CollaborationExtras},
        bpmn_model::NodeDetails,
        bpmn_xml::MixedXml,
    };
    let mut session = project();
    fixture(&mut session);
    let snapshot = session.snapshot();
    let mut annotation = snapshot.project.documents[0].processes[0].nodes[0].clone();
    annotation.id = id("extra_annotation");
    annotation.kind = NodeKind::TextAnnotation;
    annotation.name = None;
    annotation.details = Some(NodeDetails::TextAnnotation {
        text_format: None,
        text: None,
    });
    annotation.activity = None;
    annotation.loop_characteristics = None;
    annotation.io_specification = None;
    let root = Collaboration {
        id: id("artifact_root"),
        kind: None,
        extras: Some(CollaborationExtras {
            artifacts: vec![CollaborationArtifact::TextAnnotation(Box::new(annotation))],
            ..Default::default()
        }),
        base: None,
        name: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let first = Collaboration {
        id: id("first_root"),
        extras: None,
        ..root.clone()
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: first,
            },
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: root,
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![
            Mutation::SetAnnotationBody {
                document_id: docid(),
                element_id: id("extra_annotation"),
                body: Some(MixedXml::Text("body".into())),
            },
            Mutation::Documentation {
                document_id: docid(),
                element_id: id("extra_annotation"),
                documentation: "documentation".into(),
            },
        ]),
    )
    .unwrap();
    let after = session.snapshot().content_id;
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::Rename {
                document_id: docid(),
                element_id: id("extra_annotation"),
                name: Some("not a name".into())
            }])
        )
        .is_err()
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
    apply(
        &mut session,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("extra_annotation"),
        }]),
    )
    .unwrap();
    assert!(
        session.snapshot().project.documents[0].additional_collaborations[0]
            .extras
            .as_ref()
            .unwrap()
            .artifacts
            .is_empty()
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
}
#[test]
fn additional_collaboration_local_qname_tracks_process_identity() {
    use processcraft_core::bpmn_model::{EventReference, QNameRef};
    let mut session = project();
    fixture(&mut session);
    let participant = |name: &str, namespace: &str| Participant {
        id: id(name),
        base: None,
        name: Some(name.into()),
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        process: Some(EventReference::Qualified(QNameRef {
            namespace: namespace.into(),
            local: id("procurement"),
        })),
    };
    let first = Collaboration {
        id: id("first_root"),
        kind: None,
        extras: None,
        base: None,
        name: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let second = Collaboration {
        id: id("second_root"),
        participants: vec![
            participant("local_participant", GENERATED_TARGET_NAMESPACE),
            participant("external_participant", "urn:external"),
        ],
        ..first.clone()
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: first,
            },
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: second,
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("procurement"),
            xml_id: Some(id("renamed_process")),
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    let root = &snapshot.project.documents[0].additional_collaborations[0];
    assert!(
        matches!(&root.participants[0].process, Some(EventReference::Qualified(q)) if q.local == id::<processcraft_core::ids::XmlId>("renamed_process"))
    );
    assert!(
        matches!(&root.participants[1].process, Some(EventReference::Qualified(q)) if q.local == id::<processcraft_core::ids::XmlId>("procurement"))
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn promote_nested_call_association_returns_native_identity() {
    use processcraft_core::bpmn_collaboration::{
        CallConversationData, ConversationKind, ConversationNode, ParticipantAssociation,
    };
    let mut session = project();
    fixture(&mut session);
    let root = Collaboration {
        id: id("first"),
        kind: None,
        extras: None,
        base: None,
        name: None,
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let additional = Collaboration {
        id: id("additional"),
        conversation_nodes: vec![ConversationNode {
            id: id("call"),
            name: None,
            kind: ConversationKind::CallConversation,
            base: None,
            participant_refs: vec![],
            message_flow_refs: vec![],
            correlation_keys: vec![],
            parent: None,
            call: Some(CallConversationData {
                called_collaboration_ref: None,
                participant_associations: vec![ParticipantAssociation {
                    id: None,
                    base: None,
                    inner: None,
                    outer: None,
                }],
            }),
        }],
        ..root.clone()
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: root,
            },
            Mutation::SetCollaborationRoot {
                document_id: docid(),
                collaboration: additional,
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    let path =
        "/additional_collaborations/0/conversation_nodes/0/call/participant_associations/0/id";
    let mut envelope = env(&session);
    envelope.expected_generation = Some(session.snapshot().generation);
    let changes = session
        .apply(
            envelope,
            batch(vec![Mutation::PromoteElementIdentity {
                document_id: docid(),
                model_path: path.into(),
                xml_id: None,
            }]),
        )
        .unwrap();
    assert_eq!(changes.promoted_identities[0].model_path, path);
    let snapshot = session.snapshot();
    let association = &snapshot.project.documents[0].additional_collaborations[0]
        .conversation_nodes[0]
        .call
        .as_ref()
        .unwrap()
        .participant_associations[0];
    assert_eq!(
        association.id.as_ref().unwrap().selector(),
        changes.promoted_identities[0].selector
    );
    assert!(association.id.as_ref().unwrap().xml_id().is_none());
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn group_category_qnames_rewrite_only_local_target() {
    use processcraft_core::bpmn_model::{Category, CategoryValue, EventReference, Group, QNameRef};
    let mut session = project();
    fixture(&mut session);
    let group = |id_value: &str, namespace: &str| Group {
        id: id(id_value),
        base: None,
        category_value_ref: Some(EventReference::Qualified(QNameRef {
            namespace: namespace.into(),
            local: id("category_value"),
        })),
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::CreateCategory {
                document_id: docid(),
                category: Category {
                    id: id("category"),
                    base: None,
                    name: Some("Category".into()),
                    values: vec![CategoryValue {
                        id: id("category_value"),
                        base: None,
                        value: Some("Value".into()),
                    }],
                },
            },
            Mutation::CreateGroup {
                document_id: docid(),
                scope_id: id("procurement"),
                group: group("local_group", GENERATED_TARGET_NAMESPACE),
            },
            Mutation::CreateGroup {
                document_id: docid(),
                scope_id: id("procurement"),
                group: group("external_group", "urn:external"),
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("category_value"),
            xml_id: Some(id("new_value")),
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    let groups = &snapshot.project.documents[0].processes[0].groups;
    assert!(
        matches!(&groups[0].category_value_ref, Some(EventReference::Qualified(q)) if q.local == id::<processcraft_core::ids::XmlId>("new_value"))
    );
    assert!(
        matches!(&groups[1].category_value_ref, Some(EventReference::Qualified(q)) if q.local == id::<processcraft_core::ids::XmlId>("category_value"))
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn initialize_root_order_promotes_anonymous_roots_with_generation_guard() {
    use processcraft_core::bpmn_collaboration::{PartnerEntity, PartnerRole};
    let mut initial = project();
    fixture(&mut initial);
    let mut data = (*initial.snapshot().project).clone();
    Arc::make_mut(&mut data.documents[0])
        .partner_entities
        .push(PartnerEntity {
            id: None,
            base: None,
            name: Some("Partner".into()),
            participant_refs: vec![],
        });
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::InitializeDocumentRootOrder {
                document_id: docid()
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
    let mut envelope = env(&session);
    envelope.expected_generation = Some(session.snapshot().generation);
    let changes = session
        .apply(
            envelope,
            batch(vec![Mutation::InitializeDocumentRootOrder {
                document_id: docid(),
            }]),
        )
        .unwrap();
    assert_eq!(changes.promoted_identities.len(), 1);
    assert_eq!(
        changes.promoted_identities[0].model_path,
        "/partner_entities/0/id"
    );
    let selector = changes.promoted_identities[0].selector.clone();
    let snapshot = session.snapshot();
    let document = &snapshot.project.documents[0];
    assert!(
        document.partner_entities[0]
            .id
            .as_ref()
            .unwrap()
            .xml_id()
            .is_none()
    );
    assert!(document.root_order.as_ref().unwrap().contains(&selector));
    let initialized = snapshot.content_id;
    let mut invalid_order = document.root_order.clone().unwrap();
    invalid_order.push(selector.clone());
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetDocumentRootOrder {
                document_id: docid(),
                order: Some(invalid_order)
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, initialized);
    let partners = document.partner_entities.clone();
    apply(
        &mut session,
        batch(vec![Mutation::SetPartnerCatalog {
            document_id: docid(),
            partner_entities: partners,
            partner_roles: vec![PartnerRole {
                id: Some(id("role")),
                base: None,
                name: Some("Role".into()),
                participant_refs: vec![],
            }],
            endpoints: vec![],
        }]),
    )
    .unwrap();
    assert_eq!(
        session.snapshot().project.documents[0]
            .root_order
            .as_ref()
            .unwrap()
            .last(),
        Some(&id("role"))
    );
    let added = session.snapshot().content_id;
    let renamed = apply(
        &mut session,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("role"),
            xml_id: Some(id("renamed_role")),
        }]),
    )
    .unwrap();
    assert!(
        session.snapshot().project.documents[0]
            .root_order
            .as_ref()
            .unwrap()
            .contains(&renamed.selector_mappings[0].to)
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, added);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, initialized);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, initialized);
}
#[test]
fn explicit_order_does_not_block_other_diagram_cleanup_after_rename() {
    let mut session = project();
    fixture(&mut session);
    let snapshot = session.snapshot();
    let original = &snapshot.project.documents[0].diagrams[0];
    let diagram_a = original.id.selector();
    let order_a: Vec<_> = original
        .shapes
        .iter()
        .map(|shape| shape.id.selector())
        .chain(original.edges.iter().map(|edge| edge.id.selector()))
        .collect();
    let mut shape_one = original.shapes[0].clone();
    shape_one.id = id("other_shape_one");
    let mut shape_two = original.shapes[1].clone();
    shape_two.id = id("other_shape_two");
    let diagram_b = Diagram {
        id: id("other_diagram"),
        plane_id: id("other_plane"),
        plane: original.plane.clone(),
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_metadata: None,
        plane_order: Some(vec![id("other_shape_one"), id("other_shape_two")]),
        shapes: vec![shape_one, shape_two],
        edges: vec![],
    };
    apply(
        &mut session,
        batch(vec![
            Mutation::CreateDiagram {
                document_id: docid(),
                diagram: diagram_b,
            },
            Mutation::SetDiagramPlaneOrder {
                document_id: docid(),
                diagram_id: diagram_a.clone(),
                order: Some(order_a.clone()),
            },
        ]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    let mut reversed = order_a.clone();
    reversed.reverse();
    let result = apply(
        &mut session,
        batch(vec![
            Mutation::SetDiagramPlaneOrder {
                document_id: docid(),
                diagram_id: diagram_a.clone(),
                order: Some(reversed.clone()),
            },
            Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: diagram_a,
                xml_id: Some(id("renamed_diagram")),
            },
            Mutation::DeleteShape {
                document_id: docid(),
                diagram_id: id("other_diagram"),
                shape_id: id("other_shape_one"),
            },
        ]),
    )
    .unwrap();
    let after = session.snapshot().content_id;
    let snapshot = session.snapshot();
    let document = &snapshot.project.documents[0];
    let renamed = &result.selector_mappings[0].to;
    assert_eq!(
        document
            .diagrams
            .iter()
            .find(|diagram| diagram.id == *renamed)
            .unwrap()
            .plane_order,
        Some(reversed.clone())
    );
    assert_eq!(
        document
            .diagrams
            .iter()
            .find(|diagram| diagram.id
                == id::<processcraft_core::ids::ElementSelector>("other_diagram"))
            .unwrap()
            .plane_order,
        Some(vec![id("other_shape_two")])
    );
    let mut malformed = reversed;
    malformed.push(malformed[0].clone());
    assert!(
        apply(
            &mut session,
            batch(vec![
                Mutation::SetDiagramPlaneOrder {
                    document_id: docid(),
                    diagram_id: renamed.clone(),
                    order: Some(malformed)
                },
                Mutation::DeleteShape {
                    document_id: docid(),
                    diagram_id: id("other_diagram"),
                    shape_id: id("other_shape_two")
                },
            ])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, after);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
}
#[test]
fn clearing_multiplicity_removes_documentation_navigation_and_keeps_comment() {
    use processcraft_core::{
        bpmn_metadata::{BaseMetadata, Documentation},
        bpmn_xml::MixedXml,
        ids::CommentId,
    };
    let mut initial = project();
    fixture(&mut initial);
    let mut data = (*initial.snapshot().project).clone();
    Arc::make_mut(&mut data.documents[0]).collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        id: id("collaboration"),
        base: None,
        name: None,
        correlation_keys: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: id("participant"),
            base: None,
            name: Some("Participant".into()),
            process: None,
            multiplicity: Some(ParticipantMultiplicity {
                id: Some(id("multiplicity")),
                minimum: Some(0),
                maximum: Some(2),
                base: Some(BaseMetadata {
                    documentation: vec![Documentation {
                        id: Some(id("multiplicity_doc")),
                        text_format: None,
                        content: MixedXml::Text("note".into()),
                    }],
                    ..Default::default()
                }),
            }),
        }],
    });
    let target = ElementKey {
        document_id: docid(),
        xml_id: id("multiplicity_doc"),
    };
    data.links.push(NavigationLink {
        source: target.clone(),
        target: docid(),
        label: "note".into(),
    });
    data.comments.push(Comment {
        id: CommentId::new(),
        target,
        author: "Author".into(),
        text: "Keep".into(),
    });
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::SetParticipantMultiplicity {
            document_id: docid(),
            participant_id: id("participant"),
            multiplicity: None,
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.project.links.is_empty());
    assert_eq!(snapshot.project.comments.len(), 1);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn explicit_plane_order_preserves_creation_order_and_rejects_invalid_order() {
    let mut session = project();
    fixture(&mut session);
    let snapshot = session.snapshot();
    let diagram = &snapshot.project.documents[0].diagrams[0];
    let diagram_id = diagram.id.selector();
    let mut order: Vec<_> = diagram
        .shapes
        .iter()
        .map(|shape| shape.id.selector())
        .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
        .collect();
    order.reverse();
    let mut shape = diagram.shapes[0].clone();
    shape.id = id("new_representation_shape");
    let mut edge = diagram.edges[0].clone();
    edge.id = id("new_representation_edge");
    apply(
        &mut session,
        batch(vec![Mutation::SetDiagramPlaneOrder {
            document_id: docid(),
            diagram_id: diagram_id.clone(),
            order: Some(order.clone()),
        }]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![
            Mutation::CreateEdge {
                document_id: docid(),
                diagram_id: diagram_id.clone(),
                edge,
            },
            Mutation::CreateShape {
                document_id: docid(),
                diagram_id: diagram_id.clone(),
                shape,
            },
        ]),
    )
    .unwrap();
    order.push(id("new_representation_edge"));
    order.push(id("new_representation_shape"));
    assert_eq!(
        session.snapshot().project.documents[0].diagrams[0]
            .plane_order
            .as_ref(),
        Some(&order)
    );
    let after = session.snapshot().content_id;
    order.push(id("new_representation_shape"));
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetDiagramPlaneOrder {
                document_id: docid(),
                diagram_id,
                order: Some(order)
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, after);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
    let changed = apply(
        &mut session,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("new_representation_shape"),
            xml_id: Some(id("renamed_representation_shape")),
        }]),
    )
    .unwrap();
    assert!(
        session.snapshot().project.documents[0].diagrams[0]
            .plane_order
            .as_ref()
            .unwrap()
            .contains(&changed.selector_mappings[0].to)
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
}
#[test]
fn activity_metadata_rejects_new_dangling_reference_and_undoes() {
    use processcraft_core::bpmn_activity::{ModelReference, ProcessActivityMetadata};
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    let mut metadata = ProcessActivityMetadata {
        is_closed: Some(false),
        ..Default::default()
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetProcessActivityMetadata {
            document_id: docid(),
            process_id: id("procurement"),
            metadata: Some(metadata.clone()),
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    assert_ne!(before, after);
    metadata
        .supports
        .push(ModelReference::Local(id("missing_process")));
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetProcessActivityMetadata {
                document_id: docid(),
                process_id: id("procurement"),
                metadata: Some(metadata),
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}

#[test]
fn existing_unresolved_activity_reference_survives_unrelated_edit() {
    use processcraft_core::bpmn_activity::{ModelReference, ProcessActivityMetadata};
    let mut initial = project();
    fixture(&mut initial);
    let mut data = (*initial.snapshot().project).clone();
    Arc::make_mut(&mut data.documents[0]).processes[0].activity_metadata =
        Some(ProcessActivityMetadata {
            supports: vec![ModelReference::Local(id("missing_process"))],
            ..Default::default()
        });
    let mut session = Session::new(data).unwrap();
    apply(&mut session, batch(vec![rename("Changed")])).unwrap();
    let before = session.snapshot().content_id;
    let mut metadata = session.snapshot().project.documents[0].processes[0]
        .activity_metadata
        .clone()
        .unwrap();
    metadata
        .supports
        .push(ModelReference::Local(id("another_missing")));
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetProcessActivityMetadata {
                document_id: docid(),
                process_id: id("procurement"),
                metadata: Some(metadata),
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn deleting_activity_cleans_navigation_from_nested_resource_role() {
    use processcraft_core::bpmn_activity::{
        ActivityProperties, ResourceRole, ResourceRoleAssignment, ResourceRoleKind,
    };
    let mut original = project();
    fixture(&mut original);
    let mut data = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut data.documents[0]);
    let node = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("review"))
        .unwrap();
    node.activity = Some(ActivityProperties {
        resource_roles: vec![ResourceRole {
            id: Some(id("role")),
            base: None,
            name: None,
            kind: ResourceRoleKind::ResourceRole,
            assignment: ResourceRoleAssignment::None,
        }],
        ..Default::default()
    });
    doc.processes[0].flows.retain(|f| {
        f.source
            != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "review",
            )))
            && f.target
                != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "review",
                )))
    });
    for diagram in &mut doc.diagrams {
        diagram
            .shapes
            .retain(|s| s.element != id::<processcraft_core::ids::ElementSelector>("review"));
        diagram.edges.clear();
    }
    data.links.push(NavigationLink {
        source: ElementKey {
            document_id: docid(),
            xml_id: id("role"),
        },
        target: docid(),
        label: "role".into(),
    });
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("review"),
        }]),
    )
    .unwrap();
    assert!(session.snapshot().project.links.is_empty());
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn removing_global_task_cleans_navigation_and_undo_restores_it() {
    use processcraft_core::bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind};
    let mut initial = project();
    fixture(&mut initial);
    let mut data = (*initial.snapshot().project).clone();
    Arc::make_mut(&mut data.documents[0])
        .global_tasks
        .push(GlobalTask {
            id: Some(id("global")),
            base: None,
            name: Some("Global".into()),
            callable: CallableMetadata::default(),
            io_specification: None,
            resource_roles: vec![],
            kind: GlobalTaskKind::Manual,
        });
    data.links.push(NavigationLink {
        source: ElementKey {
            document_id: docid(),
            xml_id: id("global"),
        },
        target: docid(),
        label: "global".into(),
    });
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::SetGlobalTasks {
            document_id: docid(),
            global_tasks: vec![],
        }]),
    )
    .unwrap();
    assert!(session.snapshot().project.links.is_empty());
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn global_task_replacement_cannot_retarget_local_call_by_reusing_xml_id() {
    use processcraft_core::{
        bpmn_activity::{CallableMetadata, GlobalTask, GlobalTaskKind},
        bpmn_model::{NodeDetails, QNameRef},
        ids::{ElementIdentity, NativeElementId},
    };
    let mut original = project();
    fixture(&mut original);
    let mut data = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut data.documents[0]);
    let node = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<ElementIdentity>("review"))
        .unwrap();
    node.kind = NodeKind::CallActivity;
    node.details = Some(NodeDetails::CallActivity {
        called_element: Some(QNameRef {
            namespace: GENERATED_TARGET_NAMESPACE.into(),
            local: id("global"),
        }),
    });
    let task = GlobalTask {
        id: Some(id("global")),
        base: None,
        name: Some("Global".into()),
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::Manual,
    };
    doc.global_tasks.push(task.clone());
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    let mut replacement = task;
    replacement.id = Some(ElementIdentity::Native {
        native_id: NativeElementId::new(),
        xml_id: Some(id("global")),
    });
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetGlobalTasks {
                document_id: docid(),
                global_tasks: vec![replacement]
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, before);
}
#[test]
fn activity_interface_references_follow_xml_id_edits() {
    use processcraft_core::{
        bpmn_activity::{ModelReference, ProcessActivityMetadata},
        bpmn_catalog::{Interface, Operation, SupportingCatalog},
        bpmn_model::{EventResource, QNameRef},
    };
    let mut s = project();
    fixture(&mut s);
    let metadata = ProcessActivityMetadata {
        callable: processcraft_core::bpmn_activity::CallableMetadata {
            supported_interface_refs: vec![
                ModelReference::Qualified(QNameRef {
                    namespace: GENERATED_TARGET_NAMESPACE.into(),
                    local: id("api"),
                }),
                ModelReference::Qualified(QNameRef {
                    namespace: "urn:external".into(),
                    local: id("api"),
                }),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
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
            Mutation::SetProcessActivityMetadata {
                document_id: docid(),
                process_id: id("procurement"),
                metadata: Some(metadata),
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("api"),
            xml_id: Some(id("api2")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let refs = &snap.project.documents[0].processes[0]
        .activity_metadata
        .as_ref()
        .unwrap()
        .callable
        .supported_interface_refs;
    assert!(
        matches!(&refs[0], ModelReference::Qualified(q) if q.local == id::<processcraft_core::ids::XmlId>("api2"))
    );
    assert!(
        matches!(&refs[1], ModelReference::Qualified(q) if q.local == id::<processcraft_core::ids::XmlId>("api"))
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn flow_node_incoming_reference_follows_sequence_flow_rename() {
    use processcraft_core::{
        bpmn_activity::{FlowElementMetadata, ModelReference},
        bpmn_model::QNameRef,
    };
    let mut session = project();
    fixture(&mut session);
    apply(
        &mut session,
        batch(vec![Mutation::SetNodeFlowMetadata {
            document_id: docid(),
            element_id: id("review"),
            metadata: Some(FlowElementMetadata {
                incoming: vec![ModelReference::Qualified(QNameRef {
                    namespace: GENERATED_TARGET_NAMESPACE.into(),
                    local: id("flow_start_review"),
                })],
                ..Default::default()
            }),
        }]),
    )
    .unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("flow_start_review"),
            xml_id: Some(id("renamed_flow")),
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    let node = processcraft_core::queries::find_node(&snapshot.project.documents[0], &id("review"))
        .unwrap();
    assert!(
        matches!(&node.flow_metadata.as_ref().unwrap().incoming[0], ModelReference::Qualified(q) if q.local == id::<processcraft_core::ids::XmlId>("renamed_flow"))
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn choreography_sequence_flow_metadata_is_undoable_and_message_flow_rejected() {
    use processcraft_core::{
        bpmn_activity::FlowElementMetadata,
        bpmn_collaboration::{Choreography, ChoreographyActivity},
    };
    let mut initial = project();
    fixture(&mut initial);
    let mut data = (*initial.snapshot().project).clone();
    let participants = ["buyer", "seller"]
        .into_iter()
        .map(|name| Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            multiplicity: None,
            id: id(name),
            base: None,
            name: Some(name.into()),
            process: None,
        })
        .collect();
    let activity = |name| ChoreographyActivity {
        loop_type: None,
        details: None,
        id: id(name),
        base: None,
        name: Some(name.into()),
        flow_metadata: None,
        correlation_keys: vec![],
        participant_refs: vec![
            processcraft_core::bpmn_activity::ModelReference::Local(id("buyer")),
            processcraft_core::bpmn_activity::ModelReference::Local(id("seller")),
        ],
        initiating_participant: Some(processcraft_core::bpmn_activity::ModelReference::Local(id(
            "buyer",
        ))),
        message_flow_refs: vec![],
    };
    let flow = |name, kind, source, target| Flow {
        message_ref: None,
        id: id(name),
        kind,
        source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            source,
        ))),
        target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
            target,
        ))),
        name: Some(name.into()),
        documentation: String::new().into(),
        base_extras: None,
        association_direction: None,
        flow_metadata: None,
        condition: None,
    };
    Arc::make_mut(&mut data.documents[0])
        .choreographies
        .push(Choreography {
            graph: None,
            kind: None,
            extras: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            id: id("choreo"),
            base: None,
            name: None,
            participants,
            activities: vec![activity("first"), activity("second")],
            sequence_flows: vec![flow("sequence", FlowKind::Sequence, "first", "second")],
            message_flows: vec![flow("message", FlowKind::Message, "buyer", "seller")],
        });
    let mut session = Session::new(data).unwrap();
    let before = session.snapshot().content_id;
    let metadata = Some(FlowElementMetadata {
        is_immediate: Some(false),
        ..Default::default()
    });
    apply(
        &mut session,
        batch(vec![Mutation::SetFlowMetadata {
            document_id: docid(),
            element_id: id("sequence"),
            metadata: metadata.clone(),
        }]),
    )
    .unwrap();
    let after = session.snapshot().content_id;
    assert_ne!(before, after);
    assert_eq!(
        session.snapshot().project.documents[0].choreographies[0].sequence_flows[0].flow_metadata,
        metadata
    );
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::SetFlowMetadata {
                document_id: docid(),
                element_id: id("message"),
                metadata
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, after);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
    apply(&mut session, Request::Redo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
    use processcraft_core::bpmn_model::{EventReference, QNameRef};
    let reference = EventReference::Qualified(QNameRef {
        namespace: "urn:external".into(),
        local: id("Message"),
    });
    apply(
        &mut session,
        batch(vec![
            Mutation::SetParticipantMultiplicity {
                document_id: docid(),
                participant_id: id("buyer"),
                multiplicity: Some(ParticipantMultiplicity {
                    id: Some(id("multiplicity")),
                    base: None,
                    minimum: Some(0),
                    maximum: Some(2),
                }),
            },
            Mutation::SetMessageFlowReference {
                document_id: docid(),
                flow_id: id("message"),
                message_ref: Some(reference.clone()),
            },
        ]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    assert_eq!(
        snapshot.project.documents[0].choreographies[0].message_flows[0].message_ref,
        Some(reference)
    );
    assert_eq!(
        snapshot.project.documents[0].choreographies[0].participants[0]
            .multiplicity
            .as_ref()
            .unwrap()
            .maximum,
        Some(2)
    );
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, after);
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::MixedXml,
    };
    let rich = DocumentationSet::Structured(vec![Documentation {
        id: Some(id("choreo_doc")),
        text_format: None,
        content: MixedXml::Text("retained".into()),
    }]);
    apply(
        &mut session,
        batch(vec![Mutation::ReplaceDocumentation {
            document_id: docid(),
            element_id: id("sequence"),
            documentation: rich.clone(),
        }]),
    )
    .unwrap();
    let before_names = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![
            Mutation::Rename {
                document_id: docid(),
                element_id: id("choreo"),
                name: Some("Choreography".into()),
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: id("buyer"),
                name: None,
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: id("first"),
                name: Some("Task".into()),
            },
            Mutation::Rename {
                document_id: docid(),
                element_id: id("message"),
                name: Some("Message".into()),
            },
            Mutation::Documentation {
                document_id: docid(),
                element_id: id("message"),
                documentation: "Flow documentation".into(),
            },
        ]),
    )
    .unwrap();
    let changed = session.snapshot().content_id;
    let snapshot = session.snapshot();
    let choreography = &snapshot.project.documents[0].choreographies[0];
    assert_eq!(choreography.participants[0].name, None);
    assert_eq!(choreography.activities[0].name.as_deref(), Some("Task"));
    assert_eq!(
        choreography.message_flows[0].name.as_deref(),
        Some("Message")
    );
    assert_eq!(
        choreography.message_flows[0].documentation,
        DocumentationSet::Legacy("Flow documentation".into())
    );
    assert_eq!(choreography.sequence_flows[0].documentation, rich);
    assert!(
        apply(
            &mut session,
            batch(vec![Mutation::Documentation {
                document_id: docid(),
                element_id: id("sequence"),
                documentation: "overwrite".into()
            }])
        )
        .is_err()
    );
    assert_eq!(session.snapshot().content_id, changed);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before_names);
}
fn fixture(s: &mut Session) {
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(s, r).unwrap();
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn rename(name: &str) -> Mutation {
    Mutation::RenameProject { name: name.into() }
}
#[test]
fn definitions_metadata_is_undoable_and_validated_atomically() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    let metadata = DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        target_namespace: GENERATED_TARGET_NAMESPACE.into(),
        name: Some("Definitions".into()),
        expression_language: None,
        type_language: Some("http://www.w3.org/2001/XMLSchema".into()),
        exporter: Some(String::new()),
        exporter_version: None,
        imports: vec![DefinitionsImport {
            namespace: String::new(),
            location: "local.xsd".into(),
            import_type: "urn:type".into(),
        }],
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsMetadata {
            document_id: docid(),
            metadata: metadata.clone(),
        }]),
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.documents[0].definitions,
        Some(metadata.clone())
    );
    let after = s.snapshot().content_id;
    let generation = s.snapshot().generation;
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsMetadata {
            document_id: docid(),
            metadata: metadata.clone(),
        }]),
    )
    .unwrap();
    assert_eq!(s.snapshot().generation, generation);
    let mut invalid = metadata;
    invalid.target_namespace = "x".repeat(16_385);
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata: invalid,
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    {
        let mut invalid = s.snapshot().project.documents[0]
            .definitions
            .clone()
            .unwrap();
        invalid.target_namespace = "urn:bad\u{1}".into();
        assert!(
            apply(
                &mut s,
                batch(vec![Mutation::SetDefinitionsMetadata {
                    document_id: docid(),
                    metadata: invalid,
                }])
            )
            .is_err()
        );
        assert_eq!(s.snapshot().content_id, after);
    }
    let mut invalid = s.snapshot().project.documents[0]
        .definitions
        .clone()
        .unwrap();
    invalid.exporter = Some("bad\u{0}".into());
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata: invalid,
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    let mut invalid = s.snapshot().project.documents[0]
        .definitions
        .clone()
        .unwrap();
    invalid.imports = vec![
        DefinitionsImport {
            namespace: String::new(),
            location: String::new(),
            import_type: String::new(),
        };
        257
    ];
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata: invalid,
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    let mut invalid = s.snapshot().project.documents[0]
        .definitions
        .clone()
        .unwrap();
    invalid.imports[0].location = "bad\u{1}".into();
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata: invalid,
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}
#[test]
fn definitions_import_changes_are_undoable() {
    let mut s = project();
    fixture(&mut s);
    let mut metadata = DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        target_namespace: "urn:example".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsMetadata {
            document_id: docid(),
            metadata: metadata.clone(),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    metadata.imports.push(DefinitionsImport {
        namespace: String::new(),
        location: String::new(),
        import_type: String::new(),
    });
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsMetadata {
            document_id: docid(),
            metadata,
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    assert_ne!(before, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}
#[test]
fn supporting_catalog_validates_references_and_undoes_atomically() {
    use processcraft_core::{
        bpmn_catalog::*,
        bpmn_model::{EventResource, QNameRef},
    };
    let mut s = project();
    fixture(&mut s);
    let mut project = (*s.snapshot().project).clone();
    Arc::make_mut(&mut project.documents[0])
        .event_resources
        .push(EventResource::Message {
            base: None,
            id: id("message1"),
            name: None,
            item_ref: None,
        });
    let mut s = Session::new(project).unwrap();
    let q = |local| QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    };
    let catalog = SupportingCatalog {
        item_definitions: vec![ItemDefinition {
            base: None,
            id: Some(id("item1")),
            structure_ref: None,
            is_collection: None,
            item_kind: None,
        }],
        interfaces: vec![Interface {
            base: None,
            id: Some(id("interface1")),
            name: String::new(),
            implementation_ref: None,
            operations: vec![Operation {
                base: None,
                id: Some(id("operation1")),
                name: "op".into(),
                implementation_ref: None,
                in_message_ref: q("message1"),
                out_message_ref: None,
                error_refs: vec![],
            }],
        }],
        resources: vec![BpmnResource {
            base: None,
            id: Some(id("resource1")),
            name: "r".into(),
            parameters: vec![ResourceParameter {
                base: None,
                id: Some(id("parameter1")),
                name: None,
                type_ref: Some(q("item1")),
                is_required: None,
            }],
        }],
    };
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetSupportingCatalog {
            document_id: docid(),
            catalog: catalog.clone(),
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    let mut invalid = catalog.clone();
    invalid.item_definitions.clear();
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: invalid
            }])
        )
        .is_err()
    );
    let mut invalid = catalog.clone();
    invalid.interfaces[0].operations[0].in_message_ref = q("missing");
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: invalid
            }])
        )
        .is_err()
    );
    let mut invalid = catalog.clone();
    invalid.interfaces[0].id = Some(id("review"));
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: invalid
            }])
        )
        .is_err()
    );
    let mut invalid = catalog;
    invalid.interfaces[0].operations.clear();
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: invalid
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}
#[test]
fn expanded_data_preserves_omissions_and_validates_transformation() {
    use processcraft_core::{
        bpmn_data::*,
        bpmn_model::{Expression, ExpressionData, ExpressionKind},
    };
    let mut s = project();
    fixture(&mut s);
    let object = DataObject {
        flow_metadata: None,
        base: None,
        id: None,
        name: None,
        is_collection: None,
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: None,
            data_state: Some(DataState {
                base: None,
                id: Some(id("state1")),
                name: None,
            }),
        }),
    };
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetScopeDataObjects {
            document_id: docid(),
            scope_id: id("procurement"),
            data_objects: vec![object.clone()],
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    assert!(s.snapshot().project.documents[0].has_expanded_data());
    let mut duplicate = object;
    duplicate
        .item_aware
        .as_mut()
        .unwrap()
        .data_state
        .as_mut()
        .unwrap()
        .id = Some(id("review"));
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetScopeDataObjects {
                document_id: docid(),
                scope_id: id("procurement"),
                data_objects: vec![duplicate]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
    let formal = Expression::Structured(ExpressionData {
        foreign_attributes: vec![],
        id: None,
        kind: ExpressionKind::Formal {
            explicit_type: false,
            language: None,
            evaluates_to_type_ref: None,
        },
        text: "x".into(),
        namespaces: vec![(
            "bpmn".into(),
            "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
        )],
    });
    let io = IoSpecification {
        base: None,
        id: None,
        inputs: vec![DataPort {
            base: None,
            id: Some(id("port1")),
            name: None,
            item_aware: None,
            is_collection: None,
        }],
        outputs: vec![],
        input_sets: vec![],
        output_sets: vec![],
    };
    let association = DataAssociation {
        base: None,
        id: None,
        owner: id("review"),
        kind: DataAssociationKind::Input,
        sources: vec![],
        target: id("port1"),
        transformation: Some(formal),
        assignments: vec![],
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetNodeIoSpecification {
                document_id: docid(),
                element_id: id("review"),
                io_specification: Some(io),
            },
            Mutation::SetScopeDataAssociations {
                document_id: docid(),
                scope_id: id("procurement"),
                data_associations: vec![association],
            },
        ]),
    )
    .unwrap();
    assert!(s.snapshot().project.documents[0].has_structured_expressions());
}
#[test]
fn activity_bindings_validate_new_local_refs_and_preserve_legacy() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_catalog::*,
        bpmn_model::{EventResource, QNameRef},
    };
    let mut s = project();
    fixture(&mut s);
    let mut p = (*s.snapshot().project).clone();
    let d = Arc::make_mut(&mut p.documents[0]);
    d.event_resources.push(EventResource::Message {
        base: None,
        id: id("message1"),
        name: None,
        item_ref: None,
    });
    let q = |local| QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    };
    d.catalog.interfaces.push(Interface {
        base: None,
        id: Some(id("interface1")),
        name: "i".into(),
        implementation_ref: None,
        operations: vec![Operation {
            base: None,
            id: Some(id("operation1")),
            name: "o".into(),
            implementation_ref: None,
            in_message_ref: q("message1"),
            out_message_ref: None,
            error_refs: vec![],
        }],
    });
    let node = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("send"),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind: NodeKind::SendTask,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    d.processes[0].nodes.push(node);
    let mut s = Session::new(p).unwrap();
    let valid = ActivityProperties {
        message_ref: Some(q("message1")),
        operation_ref: Some(q("operation1")),
        ..Default::default()
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetActivityProperties {
            document_id: docid(),
            element_id: id("send"),
            activity: Some(valid.clone()),
        }]),
    )
    .unwrap();
    let bound = s.snapshot().content_id;
    let mut invalid = valid.clone();
    invalid.operation_ref = Some(q("message1"));
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetActivityProperties {
                document_id: docid(),
                element_id: id("send"),
                activity: Some(invalid)
            }])
        )
        .is_err()
    );
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetSupportingCatalog {
                document_id: docid(),
                catalog: SupportingCatalog::default()
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, bound);
    apply(&mut s, Request::Undo).unwrap();
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, bound);
    let mut external = valid;
    external.operation_ref = Some(QNameRef {
        namespace: String::new(),
        local: id("external"),
    });
    apply(
        &mut s,
        batch(vec![Mutation::SetActivityProperties {
            document_id: docid(),
            element_id: id("send"),
            activity: Some(external),
        }]),
    )
    .unwrap();
    let mut legacy = (*s.snapshot().project).clone();
    Arc::make_mut(&mut legacy.documents[0]).processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("send"))
        .unwrap()
        .activity
        .as_mut()
        .unwrap()
        .operation_ref = Some(q("legacy_missing"));
    let mut legacy = Session::new(legacy).unwrap();
    assert!(
        legacy
            .snapshot()
            .project
            .diagnostics()
            .iter()
            .any(|d| d.code == "missingLocalActivityOperation")
    );
    let mut props = legacy.snapshot().project.documents[0].processes[0]
        .nodes
        .iter()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("send"))
        .unwrap()
        .activity
        .clone()
        .unwrap();
    props.implementation = Some("##WebService".into());
    apply(
        &mut legacy,
        batch(vec![Mutation::SetActivityProperties {
            document_id: docid(),
            element_id: id("send"),
            activity: Some(props),
        }]),
    )
    .unwrap();
    let mut stored = (*legacy.snapshot().project).clone();
    let node = Arc::make_mut(&mut stored.documents[0]).processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("send"))
        .unwrap();
    node.activity.as_mut().unwrap().operation_ref = Some(QNameRef {
        namespace: "urn:old namespace".into(),
        local: id("op"),
    });
    let mut old_namespace = Session::new(stored).unwrap();
    assert!(
        old_namespace
            .snapshot()
            .project
            .diagnostics()
            .iter()
            .any(|d| d.code == "invalidActivityOperationNamespace")
    );
    let mut props = old_namespace.snapshot().project.documents[0].processes[0]
        .nodes
        .iter()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("send"))
        .unwrap()
        .activity
        .clone()
        .unwrap();
    props.implementation = Some("custom".into());
    apply(
        &mut old_namespace,
        batch(vec![Mutation::SetActivityProperties {
            document_id: docid(),
            element_id: id("send"),
            activity: Some(props.clone()),
        }]),
    )
    .unwrap();
    props.operation_ref.as_mut().unwrap().namespace = "urn:new bad".into();
    assert!(
        apply(
            &mut old_namespace,
            batch(vec![Mutation::SetActivityProperties {
                document_id: docid(),
                element_id: id("send"),
                activity: Some(props)
            }])
        )
        .is_err()
    );
}
#[test]
fn structured_expression_preserves_identity_and_rejects_duplicate_ids() {
    use processcraft_core::bpmn_model::{Expression, ExpressionData, ExpressionKind};
    let mut s = project();
    fixture(&mut s);
    let legacy: Expression = serde_json::from_str("\"amount > 10\"").unwrap();
    assert_eq!(serde_json::to_string(&legacy).unwrap(), "\"amount > 10\"");
    let structured = Expression::Structured(ExpressionData {
        foreign_attributes: vec![],
        id: Some(id("expr1")),
        kind: ExpressionKind::Formal {
            explicit_type: true,
            language: Some("urn:language".into()),
            evaluates_to_type_ref: None,
        },
        text: "amount > 10".into(),
        namespaces: vec![
            (
                "bpmn".into(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            ),
            (
                "xsi".into(),
                "http://www.w3.org/2001/XMLSchema-instance".into(),
            ),
        ],
    });
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetFlowCondition {
            document_id: docid(),
            flow_id: id("flow_approved"),
            condition: Some(structured.clone()),
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    let mut duplicate = structured;
    if let Expression::Structured(data) = &mut duplicate {
        data.id = Some(id("review"));
    }
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: id("flow_approved"),
                condition: Some(duplicate),
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}
#[test]
fn expression_profile_preserves_empty_base_and_rejects_unserializable_context() {
    use processcraft_core::bpmn_model::{Expression, ExpressionData, ExpressionKind};
    let mut s = project();
    fixture(&mut s);
    let base = Expression::Structured(ExpressionData {
        foreign_attributes: vec![],
        id: None,
        kind: ExpressionKind::Base {
            explicit_type: false,
        },
        text: String::new().into(),
        namespaces: vec![(
            "bpmn".into(),
            "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
        )],
    });
    apply(
        &mut s,
        batch(vec![Mutation::SetFlowCondition {
            document_id: docid(),
            flow_id: id("flow_approved"),
            condition: Some(base.clone()),
        }]),
    )
    .unwrap();
    assert!(
        s.snapshot()
            .project
            .diagnostics()
            .iter()
            .any(|d| d.code == "emptyExpression")
    );
    let before = s.snapshot().content_id;
    let mut invalid = base;
    if let Expression::Structured(data) = &mut invalid {
        data.kind = ExpressionKind::Formal {
            explicit_type: false,
            language: None,
            evaluates_to_type_ref: None,
        };
    }
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: id("flow_approved"),
                condition: Some(invalid)
            }])
        )
        .is_err()
    );
    let bad = Expression::Structured(ExpressionData {
        foreign_attributes: vec![],
        id: None,
        kind: ExpressionKind::Base {
            explicit_type: false,
        },
        text: "x".into(),
        namespaces: vec![("p".into(), "urn:bad space".into())],
    });
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: id("flow_approved"),
                condition: Some(bad)
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
}
#[test]
fn link_and_compensation_events_validate_atomically() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut s = project();
    fixture(&mut s);
    let node = |name: &str, kind, definition| Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id(name),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind,
        details: Some(NodeDetails::Event {
            definitions: vec![definition],
            attached_to: None,
            cancel_activity: None,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    let throw = node(
        "link_throw",
        NodeKind::IntermediateThrowEvent,
        EventDefinition::Link {
            base: None,
            id: Some(id("throw_def")),
            name: "jump".into(),
            sources: vec![],
            target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "catch_def",
            ))),
        },
    );
    let catch = node(
        "link_catch",
        NodeKind::IntermediateCatchEvent,
        EventDefinition::Link {
            base: None,
            id: Some(id("catch_def")),
            name: "jump".into(),
            sources: vec![processcraft_core::bpmn_model::EventReference::Local(id(
                "throw_def",
            ))],
            target: None,
        },
    );
    let compensation = node(
        "compensate",
        NodeKind::IntermediateThrowEvent,
        EventDefinition::Compensate {
            base: None,
            id: None,
            wait_for_completion: None,
            activity_ref: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "review",
            ))),
        },
    );
    let create = |element| Mutation::CreateElement {
        document_id: docid(),
        process_id: id("procurement"),
        element,
    };
    apply(
        &mut s,
        batch(vec![create(throw), create(catch), create(compensation)]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let bad = node(
        "bad_cancel",
        NodeKind::EndEvent,
        EventDefinition::Cancel {
            id: None,
            base: None,
        },
    );
    assert!(apply(&mut s, batch(vec![create(bad)])).is_err());
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Undo).unwrap();
    assert_ne!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}
#[test]
fn definitions_namespace_change_rejects_existing_qualified_references() {
    use processcraft_core::bpmn_model::{NodeDetails, QNameRef};
    let mut s = project();
    fixture(&mut s);
    let metadata = DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        target_namespace: GENERATED_TARGET_NAMESPACE.into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    };
    let node = Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id("call"),
        name: Some(String::new()),
        documentation: String::new().into(),
        kind: NodeKind::CallActivity,
        details: Some(NodeDetails::CallActivity {
            called_element: Some(QNameRef {
                namespace: GENERATED_TARGET_NAMESPACE.into(),
                local: id("procurement"),
            }),
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    };
    apply(
        &mut s,
        batch(vec![Mutation::CreateElement {
            document_id: docid(),
            process_id: id("procurement"),
            element: node,
        }]),
    )
    .unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsMetadata {
            document_id: docid(),
            metadata: metadata.clone(),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let mut changed = metadata;
    changed.target_namespace = "urn:other".into();
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata: changed,
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
}
#[test]
fn definitions_metadata_refuses_protected_and_unresolved_legacy_documents() {
    let mut s = project();
    fixture(&mut s);
    let mut p = (*s.snapshot().project).clone();
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    let metadata = DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        target_namespace: "urn:example".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    };
    for capability in [
        Capability::RetainedExtensions {
            source_resource_id: id("source"),
            extensions: vec![],
        },
        Capability::PreservedSource {
            source_resource_id: id("source"),
        },
    ] {
        let mut protected_project = p.clone();
        Arc::make_mut(&mut protected_project.documents[0]).capability = capability;
        let mut session = Session::new(protected_project).unwrap();
        let before = session.snapshot().content_id;
        assert_eq!(
            apply(
                &mut session,
                batch(vec![Mutation::SetDefinitionsMetadata {
                    document_id: docid(),
                    metadata: metadata.clone(),
                }])
            )
            .unwrap_err(),
            CommandError::CapabilityDenied
        );
        assert_eq!(session.snapshot().content_id, before);
    }
    Arc::make_mut(&mut p.documents[0]).provenance = Some(Provenance {
        source_resource_id: id("source"),
        source_name: "original.bpmn".into(),
    });
    let mut legacy = Session::new(p).unwrap();
    let before = legacy.snapshot().content_id;
    assert!(
        apply(
            &mut legacy,
            batch(vec![Mutation::SetDefinitionsMetadata {
                document_id: docid(),
                metadata,
            }])
        )
        .is_err()
    );
    assert_eq!(legacy.snapshot().content_id, before);
}
#[test]
fn failed_batch_preserves_state_and_history() {
    let mut s = project();
    fixture(&mut s);
    apply(&mut s, Request::Undo).unwrap();
    apply(&mut s, Request::Redo).unwrap();
    s.set_selection(vec![ElementKey {
        document_id: docid(),
        xml_id: id("review"),
    }]);
    let before = s.snapshot();
    let bytes = serde_json::to_vec(&before.project).unwrap();
    let invalid = Mutation::CreateFlow {
        document_id: docid(),
        process_id: Some(id("procurement")),
        flow: Flow {
            message_ref: None,
            flow_metadata: None,
            base_extras: None,
            association_direction: None,
            id: id("bad"),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "review",
            ))),
            target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "missing",
            ))),
            name: Some(String::new()),
            documentation: String::new().into(),
            condition: None,
        },
    };
    assert!(apply(&mut s, batch(vec![rename("changed"), invalid])).is_err());
    let after = s.snapshot();
    assert_eq!(bytes, serde_json::to_vec(&after.project).unwrap());
    assert_eq!(before.content_id, after.content_id);
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.undo_count, after.undo_count);
    assert_eq!(before.redo_count, after.redo_count);
    assert_eq!(before.selection, after.selection);
}
#[test]
fn generation_conflict_changes_nothing() {
    let mut s = project();
    let before = s.snapshot();
    let mut e = env(&s);
    e.expected_generation = Some(9);
    assert_eq!(
        s.apply(e, batch(vec![rename("changed")])).unwrap_err(),
        CommandError::GenerationConflict
    );
    assert_eq!(s.snapshot().content_id, before.content_id);
    assert_eq!(s.snapshot().generation, 0);
}
#[test]
fn one_batch_is_one_undo() {
    let mut s = project();
    let initial = s.snapshot().content_id;
    fixture(&mut s);
    assert_eq!(s.snapshot().undo_count, 1);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, initial);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().project.documents.len(), 1);
}
#[test]
fn undo_to_saved_content_is_clean() {
    let mut s = project();
    fixture(&mut s);
    let saved = s.snapshot().content_id;
    s.mark_saved(saved);
    apply(&mut s, batch(vec![rename("changed")])).unwrap();
    assert!(s.snapshot().dirty);
    apply(&mut s, Request::Undo).unwrap();
    assert!(!s.snapshot().dirty);
}
#[test]
fn history_budget_drops_only_undo() {
    let mut s = project();
    for i in 0..205 {
        apply(&mut s, batch(vec![rename(&format!("p{i}"))])).unwrap();
    }
    assert_eq!(s.snapshot().undo_count, 200);
    assert_eq!(s.snapshot().project.name, "p204");
    for _ in 0..200 {
        apply(&mut s, Request::Undo).unwrap();
    }
    assert_eq!(s.snapshot().project.name, "p4");
    assert_eq!(s.snapshot().redo_count, 200);
}
#[test]
fn noop_and_stale_save_ack() {
    let mut s = project();
    let saved = s.snapshot().content_id;
    assert!(
        !apply(&mut s, batch(vec![rename("Project")]))
            .unwrap()
            .changed
    );
    assert_eq!(s.snapshot().generation, 0);
    apply(&mut s, batch(vec![rename("later")])).unwrap();
    s.mark_saved(saved);
    assert!(s.snapshot().dirty);
    assert_eq!(s.snapshot().undo_count, 1);
}
#[test]
fn copy_on_write_shares_unchanged_document() {
    let mut s = project();
    fixture(&mut s);
    let d = BpmnDocument::new("second".into(), id("defs2"));
    apply(
        &mut s,
        batch(vec![Mutation::CreateDocument { document: d }]),
    )
    .unwrap();
    let before = s.snapshot();
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("review"),
            name: Some("New".into()),
        }]),
    )
    .unwrap();
    let after = s.snapshot();
    assert!(!Arc::ptr_eq(
        &before.project.documents[0],
        &after.project.documents[0]
    ));
    assert!(Arc::ptr_eq(
        &before.project.documents[1],
        &after.project.documents[1]
    ));
}
#[test]
fn capability_restrictions_and_unknown_fields() {
    let mut p = ProjectData::new("p".into());
    let r = Resource::new(id("source"), "text/xml".into(), Arc::from(&b"xml"[..]));
    p.resources.push(r);
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    let did = d.id;
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::DocumentProperties {
                document_id: did,
                name: "x".into(),
                folder: None
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert!(
        serde_json::from_str::<Request>(r#"{"type":"batch","mutations":[],"extra":1}"#).is_err()
    );
}
#[test]
fn retained_extensions_allow_labels_but_reject_structural_changes() {
    let mut p = ProjectData::new("p".into());
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
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
        id: id("p"),
        name: Some("before".into()),
        documentation: String::new().into(),
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let did = d.id;
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: did,
            element_id: id("p"),
            name: Some("after".into()),
        }]),
    )
    .unwrap();
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::Delete {
                document_id: did,
                element_id: id("p")
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
}
#[test]
fn deletion_cascades_connected_flows_and_defaults_in_one_undo_step() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut s = project();
    fixture(&mut s);
    apply(
        &mut s,
        batch(vec![Mutation::SetNodeDetails {
            document_id: docid(),
            element_id: id("decision"),
            details: Some(NodeDetails::Gateway {
                default_flow: Some(id("flow_approved")),
                direction: None,
                instantiate: None,
                event_gateway_type: None,
                activation_condition: None,
            }),
        }]),
    )
    .unwrap();
    let before = s.snapshot();
    let flow_ids = |s: &Session| -> Vec<String> {
        let snapshot = s.snapshot();
        let document = &snapshot.project.documents[0];
        assert_eq!(
            document.processes[0].flows.len(),
            document.diagrams[0].edges.len()
        );
        document.processes[0]
            .flows
            .iter()
            .map(|f| f.id.to_string())
            .collect()
    };
    // A connected node takes its incident flows (and their DI edges) with it.
    apply(
        &mut s,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("review"),
        }]),
    )
    .unwrap();
    let remaining = flow_ids(&s);
    assert!(!remaining.contains(&"flow_start_review".to_owned()));
    assert!(!remaining.contains(&"flow_review_decision".to_owned()));
    assert!(remaining.contains(&"flow_approved".to_owned()));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
    assert_eq!(s.snapshot().undo_count, before.undo_count);
    // Deleting a gateway's default flow clears the default.
    apply(
        &mut s,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("flow_approved"),
        }]),
    )
    .unwrap();
    let snapshot = s.snapshot();
    let decision = snapshot.project.documents[0].processes[0]
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "decision")
        .unwrap();
    assert!(matches!(
        &decision.details,
        Some(NodeDetails::Gateway {
            default_flow: None,
            ..
        })
    ));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before.content_id);
}
#[test]
fn batch_cap_and_unknown_mutation_fields_are_enforced() {
    let mut s = project();
    assert!(apply(&mut s, batch((0..1001).map(|_| rename("x")).collect())).is_err());
    assert!(
        serde_json::from_str::<Request>(
            r#"{"type":"batch","mutations":[{"type":"renameProject","name":"x","extra":1}]}"#
        )
        .is_err()
    );
}
#[test]
fn protected_documents_cannot_be_deleted_or_moved_between_folders() {
    let mut p = ProjectData::new("p".into());
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    p.folders.push(Folder {
        id: id("folder"),
        name: "folder".into(),
        parent: None,
    });
    let mut retained = BpmnDocument::new("retained".into(), id("defs"));
    retained.capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
    let retained_id = retained.id;
    p.documents.push(Arc::new(retained));
    let mut preserved = BpmnDocument::new("preserved".into(), id("defs2"));
    preserved.capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    let preserved_id = preserved.id;
    p.documents.push(Arc::new(preserved));
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    for document_id in [retained_id, preserved_id] {
        assert_eq!(
            apply(
                &mut s,
                batch(vec![Mutation::DeleteDocument { document_id }])
            )
            .unwrap_err(),
            CommandError::CapabilityDenied
        );
    }
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::DocumentProperties {
                document_id: retained_id,
                name: "renamed".into(),
                folder: Some(id("folder"))
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::DocumentProperties {
            document_id: retained_id,
            name: "renamed".into(),
            folder: None,
        }]),
    )
    .unwrap();
}
#[test]
fn unsaved_state_remains_dirty_until_save_acknowledged() {
    let mut s = project();
    s.mark_unsaved();
    assert!(s.snapshot().dirty);
    apply(&mut s, batch(vec![rename("change")])).unwrap();
    apply(&mut s, Request::Undo).unwrap();
    assert!(s.snapshot().dirty);
    s.mark_saved(s.snapshot().content_id);
    assert!(!s.snapshot().dirty);
}
#[test]
fn undo_redo_restore_selection_context() {
    let mut s = project();
    fixture(&mut s);
    let key = ElementKey {
        document_id: docid(),
        xml_id: id("review"),
    };
    s.set_selection(vec![key.clone()]);
    apply(
        &mut s,
        batch(vec![Mutation::DeleteDocument {
            document_id: docid(),
        }]),
    )
    .unwrap();
    assert!(s.snapshot().selection.is_empty());
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().selection, vec![key]);
    apply(&mut s, Request::Redo).unwrap();
    assert!(s.snapshot().selection.is_empty());
}
#[test]
fn flow_condition_changes_identity_and_protected_document_rejects_it() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetFlowCondition {
            document_id: docid(),
            flow_id: id("flow_approved"),
            condition: Some("amount > 10".into()),
        }]),
    )
    .unwrap();
    assert_ne!(s.snapshot().content_id, before);
    let mut p = (*s.snapshot().project).clone();
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    Arc::make_mut(&mut p.documents[0]).capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
    let mut protected = Session::new(p).unwrap();
    let old = protected.snapshot().content_id;
    assert_eq!(
        apply(
            &mut protected,
            batch(vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: id("flow_approved"),
                condition: None
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert_eq!(protected.snapshot().content_id, old);
}
#[test]
fn descriptors_match_protocol_tags() {
    let descriptors = processcraft_core::commands::command_descriptors();
    let expected = [
        "setCollaborationRoot",
        "deleteCollaborationRoot",
        "setPartnerCatalog",
        "setDocumentRootOrder",
        "initializeDocumentRootOrder",
        "setDiagramPlaneMetadata",
        "setDiagramPlaneOrder",
        "setShapeRepresentationMetadata",
        "setEdgeRepresentationMetadata",
        "setParticipantMultiplicity",
        "setMessageFlowReference",
        "setGlobalTasks",
        "setCorrelationProperties",
        "setExpression",
        "setDefinitionsExtensions",
        "setDefinitionsRelationships",
        "setDiagramMetadata",
        "setDiagramLabelStyles",
        "setShapeLabel",
        "setEdgeLabel",
        "setDocumentationEntry",
        "setBaseMetadata",
        "setBaseExtras",
        "replaceFlowCondition",
        "setScriptBody",
        "setAnnotationBody",
        "replaceDocumentation",
        "setEventResources",
        "setRootEventDefinitions",
        "setShapeDiAttributes",
        "setEdgeDiEndpoints",
        "setCollaborationName",
        "setDiagramName",
        "setProcessExecutable",
        "setCollaborationCorrelationKeys",
        "setProcessActivityMetadata",
        "setNodeFlowMetadata",
        "setFlowMetadata",
        "setElementXmlId",
        "promoteElementIdentity",
        "setTextAnnotation",
        "setNodeItemAware",
        "setProcessIoSpecification",
        "setDataStores",
        "setScopeDataObjects",
        "setScopeDataAssociations",
        "setSupportingCatalog",
        "setDefinitionsMetadata",
        "setActivityProperties",
        "createCategory",
        "updateCategory",
        "deleteCategory",
        "createGroup",
        "updateGroup",
        "deleteGroup",
        "setAssociationDirection",
        "setLaneSetId",
        "setChildLaneSetId",
        "setLaneSetName",
        "setChildLaneSetName",
        "setLanePartition",
        "setLabelBounds",
        "setNodeIoSpecification",
        "createDataStore",
        "updateDataStore",
        "deleteDataStore",
        "createDataObject",
        "updateDataObject",
        "deleteDataObject",
        "createDataAssociation",
        "updateDataAssociation",
        "deleteDataAssociation",
        "setDiagramPlane",
        "createChoreography",
        "updateChoreography",
        "deleteChoreography",
        "promoteChoreographyGraph",
        "createChoreographyFlowElement",
        "updateChoreographyFlowElement",
        "deleteChoreographyFlowElement",
        "reorderChoreographyFlowElement",
        "reparentChoreographyFlowElement",
        "createChoreographyArtifact",
        "updateChoreographyArtifact",
        "deleteChoreographyArtifact",
        "reorderChoreographyArtifact",
        "reparentChoreographyArtifact",
        "createChoreographyDataAssociation",
        "updateChoreographyDataAssociation",
        "deleteChoreographyDataAssociation",
        "reorderChoreographyDataAssociation",
        "createChoreographyActivity",
        "updateChoreographyActivity",
        "deleteChoreographyActivity",
        "createConversationNode",
        "updateConversationNode",
        "deleteConversationNode",
        "createConversationLink",
        "updateConversationLink",
        "deleteConversationLink",
        "applyLayout",
        "createComment",
        "updateComment",
        "deleteComment",
        "createCheckpoint",
        "renameCheckpoint",
        "deleteCheckpoint",
        "setReferenceCheckpoint",
        "restoreCheckpoint",
        "setNodeLoop",
        "setNodeDetails",
        "addResource",
        "spliceActivity",
        "createFolder",
        "renameFolder",
        "moveFolder",
        "deleteFolder",
        "createDictionaryEntry",
        "updateDictionaryEntry",
        "deleteDictionaryEntry",
        "setDictionaryLinks",
        "batch",
        "undo",
        "redo",
        "createDocument",
        "createProcess",
        "createCollaboration",
        "createParticipant",
        "createLane",
        "createElement",
        "createFlow",
        "createDiagram",
        "createShape",
        "deleteShape",
        "createEdge",
        "deleteDocument",
        "projectProperties",
        "renameProject",
        "documentProperties",
        "rename",
        "documentation",
        "delete",
        "moveShape",
        "reroute",
        "assignLane",
        "setFlowCondition",
        "setNodeOwnerContact",
        "connect",
        "append",
        "replaceElement",
        "moveElements",
        "resizeShape",
        "reconnect",
        "createAt",
    ];
    assert_eq!(
        descriptors.iter().map(|d| d.serde_tag).collect::<Vec<_>>(),
        expected
    );
    for d in descriptors {
        assert_eq!(d.id, d.serde_tag);
        assert!(!d.label.is_empty());
    }
    let request: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    let serialized = serde_json::to_value(request).unwrap();
    for m in serialized["mutations"].as_array().unwrap() {
        assert!(
            descriptors
                .iter()
                .any(|d| Some(d.serde_tag) == m["type"].as_str())
        );
    }
    let mutation = Mutation::SetFlowCondition {
        document_id: docid(),
        flow_id: id("flow_approved"),
        condition: None,
    };
    assert_eq!(
        serde_json::to_value(mutation).unwrap()["type"],
        "setFlowCondition"
    );
}

#[test]
fn clearing_connected_id_promotes_selection_and_undo_restores_it() {
    use processcraft_core::ids::ElementSelector;
    let mut s = project();
    fixture(&mut s);
    let key = ElementKey {
        document_id: docid(),
        xml_id: id("review"),
    };
    s.set_selection(vec![key.clone()]);
    let before = s.snapshot().content_id;
    let change = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: key.xml_id.clone(),
            xml_id: None,
        }]),
    )
    .unwrap();
    assert_eq!(change.selector_mappings.len(), 1);
    let promoted = &change.selector_mappings[0].to;
    assert!(matches!(promoted, ElementSelector::Native { .. }));
    assert_eq!(&s.snapshot().selection[0].xml_id, promoted);
    let snap = s.snapshot();
    let process = &snap.project.documents[0].processes[0];
    assert!(process.flows.iter().any(|flow| flow.target
        == Some(processcraft_core::bpmn_model::EventReference::Local(
            promoted.clone()
        ))));
    assert!(
        snap.project.documents[0]
            .diagrams
            .iter()
            .flat_map(|d| &d.shapes)
            .any(|shape| &shape.element == promoted)
    );
    let undo = apply(&mut s, Request::Undo).unwrap();
    assert_eq!(&undo.selector_mappings[0].from, promoted);
    assert_eq!(undo.selector_mappings[0].to, key.xml_id);
    assert_eq!(s.snapshot().content_id, before);
    assert_eq!(s.snapshot().selection, vec![key.clone()]);
    let redo = apply(&mut s, Request::Redo).unwrap();
    assert_eq!(redo.selector_mappings[0].from, key.xml_id);
    assert_eq!(&redo.selector_mappings[0].to, promoted);
    assert_eq!(&s.snapshot().selection[0].xml_id, promoted);
}

#[test]
fn identity_edit_rewrites_only_typed_local_references() {
    use processcraft_core::{
        bpmn_catalog::ItemDefinition,
        bpmn_data::{DataStore, ItemAwareMetadata},
        bpmn_model::{EventResource, QNameRef},
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut p.documents[0]);
    let reference = QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("item"),
    };
    doc.catalog.item_definitions.push(ItemDefinition {
        base: None,
        id: Some(id("item")),
        structure_ref: Some(reference.clone()),
        is_collection: None,
        item_kind: None,
    });
    doc.data_stores.push(DataStore {
        base: None,
        id: Some(id("store")),
        name: Some("Store".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: Some(reference.clone()),
            data_state: None,
        }),
    });
    doc.event_resources.push(EventResource::Signal {
        base: None,
        id: id("signal"),
        name: None,
        structure_ref: Some(reference.clone()),
    });
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: id("item"),
                xml_id: None
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("item"),
            xml_id: Some(id("item_new")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let doc = &snap.project.documents[0];
    assert!(
        matches!(&doc.event_resources[0], EventResource::Signal { structure_ref: Some(q), .. } if q.local == id::<processcraft_core::ids::XmlId>("item_new"))
    );
    assert_eq!(
        doc.catalog.item_definitions[0].structure_ref,
        Some(reference)
    );
    assert_eq!(
        doc.data_stores[0]
            .item_aware
            .as_ref()
            .unwrap()
            .item_subject_ref
            .as_ref()
            .unwrap()
            .local,
        id::<processcraft_core::ids::XmlId>("item_new")
    );
}

#[test]
fn promotion_requires_generation_and_targets_only_optional_identity() {
    use processcraft_core::bpmn_catalog::ItemDefinition;
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    Arc::make_mut(&mut p.documents[0])
        .catalog
        .item_definitions
        .push(ItemDefinition {
            base: None,
            id: None,
            structure_ref: None,
            is_collection: None,
            item_kind: None,
        });
    let mut s = Session::new(p).unwrap();
    let path = "/catalog/item_definitions/0/id".to_string();
    let request = || {
        batch(vec![Mutation::PromoteElementIdentity {
            document_id: docid(),
            model_path: path.clone(),
            xml_id: None,
        }])
    };
    assert!(apply(&mut s, request()).is_err());
    let mut guarded = env(&s);
    guarded.expected_generation = Some(s.snapshot().generation);
    let change = s.apply(guarded, request()).unwrap();
    assert_eq!(change.promoted_identities.len(), 1);
    assert_eq!(
        s.snapshot().project.documents[0].catalog.item_definitions[0]
            .id
            .as_ref()
            .unwrap()
            .selector(),
        change.promoted_identities[0].selector
    );
    apply(&mut s, Request::Undo).unwrap();
    assert!(
        s.snapshot().project.documents[0].catalog.item_definitions[0]
            .id
            .is_none()
    );
}

#[test]
fn annotation_text_edit_migrates_legacy_body_and_undo_restores_it() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut s = project();
    fixture(&mut s);
    let mut p = (*s.snapshot().project).clone();
    Arc::make_mut(&mut p.documents[0]).processes[0]
        .nodes
        .push(Node {
            flow_metadata: None,
            base_extras: None,
            id: id("annotation"),
            name: Some("old body".into()),
            documentation: String::new().into(),
            kind: NodeKind::TextAnnotation,
            details: Some(NodeDetails::TextAnnotation {
                text_format: Some("text/plain".into()),
                text: None,
            }),
            item_aware: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
        });
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetTextAnnotation {
            document_id: docid(),
            element_id: id("annotation"),
            text: Some("new body".into()),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let node = processcraft_core::queries::find_node(&snap.project.documents[0], &id("annotation"))
        .unwrap();
    assert_eq!(node.name, None);
    assert!(
        matches!(&node.details, Some(NodeDetails::TextAnnotation { text_format: Some(format), text: Some(text) }) if format == "text/plain" && text.plain_text() == Some("new body"))
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    let snap = s.snapshot();
    assert_eq!(
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("annotation"))
            .unwrap()
            .name
            .as_deref(),
        Some("old body")
    );
}

#[test]
fn reusable_event_catalog_edits_are_atomic_and_undoable() {
    use processcraft_core::bpmn_model::{EventDefinition, EventReference, NodeDetails};
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    let root = EventDefinition::Signal {
        base: None,
        id: Some(id("root_signal")),
        signal_ref: None,
    };
    let details = NodeDetails::Event {
        definitions: vec![],
        definition_refs: vec![EventReference::Local(id("root_signal"))],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: Some(false),
        is_interrupting: None,
        properties: vec![],
        io: None,
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![root],
            },
            Mutation::SetNodeDetails {
                document_id: docid(),
                element_id: id("start"),
                details: Some(details),
            },
        ]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    assert_ne!(before, after);
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}

#[test]
fn root_event_rename_remaps_local_refs_without_touching_external_qnames() {
    use processcraft_core::bpmn_model::{EventDefinition, EventReference, NodeDetails, QNameRef};
    let mut s = project();
    fixture(&mut s);
    let local = QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("root_signal"),
    };
    let external = QNameRef {
        namespace: "urn:external".into(),
        local: id("root_signal"),
    };
    let details = NodeDetails::Event {
        definitions: vec![],
        definition_refs: vec![
            EventReference::Qualified(local),
            EventReference::Qualified(external.clone()),
        ],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: Some(false),
        is_interrupting: None,
        properties: vec![],
        io: None,
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![EventDefinition::Signal {
                    base: None,
                    id: Some(id("root_signal")),
                    signal_ref: None,
                }],
            },
            Mutation::SetNodeDetails {
                document_id: docid(),
                element_id: id("start"),
                details: Some(details),
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: id("root_signal"),
                xml_id: None
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("root_signal"),
            xml_id: Some(id("signal_new")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("start")).unwrap();
    let Some(NodeDetails::Event {
        definition_refs, ..
    }) = &node.details
    else {
        panic!("event details missing")
    };
    assert!(
        matches!(&definition_refs[0], EventReference::Qualified(q) if q.local == id::<processcraft_core::ids::XmlId>("signal_new"))
    );
    assert_eq!(definition_refs[1], EventReference::Qualified(external));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn diagram_attributes_and_endpoint_changes_are_atomic_and_undoable() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    let changes = vec![
        Mutation::SetShapeDiAttributes {
            document_id: docid(),
            diagram_id: id("procurement_diagram"),
            shape_id: id("pc_shape_start"),
            is_horizontal: Some(false),
            is_marker_visible: Some(true),
        },
        Mutation::SetEdgeDiEndpoints {
            document_id: docid(),
            diagram_id: id("procurement_diagram"),
            edge_id: id("pc_edge_flow_start_review"),
            source_element: Some(id("pc_shape_start")),
            target_element: Some(id("pc_shape_review")),
        },
    ];
    apply(&mut s, batch(changes)).unwrap();
    let after = s.snapshot().content_id;
    assert_ne!(before, after);
    let diagram = &s.snapshot().project.documents[0].diagrams[0];
    assert_eq!(
        diagram
            .shapes
            .iter()
            .find(
                |shape| shape.id == id::<processcraft_core::ids::ElementIdentity>("pc_shape_start")
            )
            .unwrap()
            .is_horizontal,
        Some(false)
    );
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetEdgeDiEndpoints {
                document_id: docid(),
                diagram_id: id("procurement_diagram"),
                edge_id: id("pc_edge_flow_start_review"),
                source_element: Some(id("missing")),
                target_element: None
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, after);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}

#[test]
fn event_resource_catalog_rejects_dangling_local_reference() {
    use processcraft_core::bpmn_model::{
        EventDefinition, EventReference, EventResource, NodeDetails,
    };
    let mut s = project();
    fixture(&mut s);
    let details = NodeDetails::Event {
        definitions: vec![EventDefinition::Signal {
            base: None,
            id: None,
            signal_ref: Some(EventReference::Local(id("signal"))),
        }],
        definition_refs: vec![],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: None,
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![EventResource::Signal {
                    base: None,
                    id: id("signal"),
                    name: None,
                    structure_ref: None,
                }],
            },
            Mutation::SetNodeDetails {
                document_id: docid(),
                element_id: id("start"),
                details: Some(details),
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn diagram_representation_id_promotion_remaps_edge_endpoint_and_undo() {
    let mut s = project();
    fixture(&mut s);
    apply(
        &mut s,
        batch(vec![Mutation::SetEdgeDiEndpoints {
            document_id: docid(),
            diagram_id: id("procurement_diagram"),
            edge_id: id("pc_edge_flow_start_review"),
            source_element: Some(id("pc_shape_start")),
            target_element: None,
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let changed = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("pc_shape_start"),
            xml_id: Some(id("renamed_shape")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let diagram = &snap.project.documents[0].diagrams[0];
    let edge = diagram
        .edges
        .iter()
        .find(|e| {
            e.id == id::<processcraft_core::ids::ElementIdentity>("pc_edge_flow_start_review")
        })
        .unwrap();
    assert_eq!(
        edge.source_element.as_ref(),
        Some(&changed.selector_mappings[0].to)
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn deleting_event_cleans_navigation_from_owned_property() {
    use processcraft_core::bpmn_model::{EventProperty, NodeDetails};
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut p.documents[0]);
    let event = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|node| node.id == id::<processcraft_core::ids::ElementIdentity>("start"))
        .unwrap();
    event.details = Some(NodeDetails::Event {
        definitions: vec![],
        definition_refs: vec![],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![EventProperty {
            base: None,
            id: Some(id("property")),
            name: Some("property".into()),
            item_aware: None,
        }],
        io: None,
    });
    p.links.push(NavigationLink {
        source: ElementKey {
            document_id: docid(),
            xml_id: id("property"),
        },
        target: docid(),
        label: "property".into(),
    });
    // Remove connected flows and diagram representations so deleting this event is otherwise valid.
    let doc = Arc::make_mut(&mut p.documents[0]);
    doc.processes[0].flows.retain(|flow| {
        flow.source
            != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "start",
            )))
            && flow.target
                != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "start",
                )))
    });
    for diagram in &mut doc.diagrams {
        diagram.shapes.retain(|shape| {
            shape.element != id::<processcraft_core::ids::ElementSelector>("start")
        });
        diagram.edges.retain(|edge| {
            edge.element != id::<processcraft_core::ids::ElementSelector>("flow_start_review")
        });
    }
    let mut session = Session::new(p).unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("start"),
        }]),
    )
    .unwrap();
    assert!(session.snapshot().project.links.is_empty());
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn deleting_event_cascades_its_data_association_and_undo_restores_it() {
    use processcraft_core::{
        bpmn_data::{DataAssociation, DataAssociationKind, DataObject, DataPort},
        bpmn_model::{EventIo, NodeDetails},
        ids::CommentId,
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut p.documents[0]);
    let process = &mut doc.processes[0];
    process
        .nodes
        .iter_mut()
        .find(|n| n.id == id::<processcraft_core::ids::ElementIdentity>("start"))
        .unwrap()
        .details = Some(NodeDetails::Event {
        definitions: vec![],
        definition_refs: vec![],
        attached_to: None,
        cancel_activity: None,
        parallel_multiple: None,
        is_interrupting: None,
        properties: vec![],
        io: Some(EventIo::Catch {
            outputs: vec![DataPort {
                base: None,
                id: Some(id("event_output")),
                name: Some("output".into()),
                item_aware: None,
                is_collection: None,
            }],
            output_set: None,
        }),
    });
    process.data_objects.push(DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("object")),
        name: Some("object".into()),
        is_collection: None,
        item_aware: None,
    });
    process.data_associations.push(DataAssociation {
        base: None,
        id: Some(id("association")),
        owner: id("start"),
        kind: DataAssociationKind::Output,
        sources: vec![id("event_output")],
        target: id("object"),
        transformation: None,
        assignments: vec![],
    });
    process.flows.retain(|flow| {
        flow.source
            != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "start",
            )))
            && flow.target
                != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "start",
                )))
    });
    for diagram in &mut doc.diagrams {
        diagram.shapes.retain(|shape| {
            shape.element != id::<processcraft_core::ids::ElementSelector>("start")
        });
        diagram.edges.retain(|edge| {
            edge.element != id::<processcraft_core::ids::ElementSelector>("flow_start_review")
        });
    }
    p.links.push(NavigationLink {
        source: ElementKey {
            document_id: docid(),
            xml_id: id("association"),
        },
        target: docid(),
        label: "association".into(),
    });
    p.comments.push(Comment {
        id: CommentId::new(),
        target: ElementKey {
            document_id: docid(),
            xml_id: id("event_output"),
        },
        author: "test".into(),
        text: "context".into(),
    });
    let mut session = Session::new(p).unwrap();
    let before = session.snapshot().content_id;
    apply(
        &mut session,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("start"),
        }]),
    )
    .unwrap();
    let snapshot = session.snapshot();
    assert!(
        snapshot.project.documents[0].processes[0]
            .data_associations
            .is_empty()
    );
    assert!(snapshot.project.links.is_empty());
    assert_eq!(snapshot.project.comments.len(), 1);
    apply(&mut session, Request::Undo).unwrap();
    assert_eq!(session.snapshot().content_id, before);
}

#[test]
fn structured_documentation_requires_explicit_edits_and_preserves_history() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::MixedXml,
    };
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    let first = Documentation {
        id: Some(id("doc_a")),
        text_format: None,
        content: MixedXml::Text("first".into()),
    };
    let second = Documentation {
        id: Some(id("doc_b")),
        text_format: Some("text/plain".into()),
        content: MixedXml::Text("second".into()),
    };
    apply(
        &mut s,
        batch(vec![Mutation::ReplaceDocumentation {
            document_id: docid(),
            element_id: id("review"),
            documentation: DocumentationSet::Structured(vec![first.clone(), second.clone()]),
        }]),
    )
    .unwrap();
    let structured = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::Documentation {
                document_id: docid(),
                element_id: id("review"),
                documentation: "flatten".into()
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, structured);
    let mut changed = first;
    changed.content = MixedXml::Text("updated".into());
    apply(
        &mut s,
        batch(vec![Mutation::SetDocumentationEntry {
            document_id: docid(),
            documentation_id: id("doc_a"),
            documentation: changed.clone(),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("review")).unwrap();
    assert_eq!(
        node.documentation,
        DocumentationSet::Structured(vec![changed, second])
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, structured);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn opaque_metadata_blocks_identity_and_structural_edits_without_partial_changes() {
    use processcraft_core::{
        bpmn_metadata::BaseExtras,
        bpmn_xml::{NamespaceBinding, XmlAttribute},
    };
    let mut s = project();
    fixture(&mut s);
    let extras = BaseExtras {
        namespaces: vec![NamespaceBinding {
            prefix: "v".into(),
            uri: "urn:vendor".into(),
        }],
        foreign_attributes: vec![XmlAttribute {
            namespace: "urn:vendor".into(),
            local: "reference".parse().unwrap(),
            prefix: Some("v".parse().unwrap()),
            value: "review".into(),
        }],
        extension_elements: None,
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetBaseExtras {
            document_id: docid(),
            element_id: id("review"),
            extras: Some(extras.clone()),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: id("review"),
                xml_id: Some(id("renamed"))
            }])
        )
        .is_err()
    );
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::Delete {
                document_id: docid(),
                element_id: id("review")
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("review"),
            name: Some("Label only".into()),
        }]),
    )
    .unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::SetShapeDiAttributes {
            document_id: docid(),
            diagram_id: id("procurement_diagram"),
            shape_id: id("pc_shape_review"),
            is_horizontal: Some(false),
            is_marker_visible: None,
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("review")).unwrap();
    assert_eq!(node.base_extras, Some(extras));
}

#[test]
fn explicit_anonymous_lane_set_can_be_promoted_with_generation_guard() {
    use processcraft_core::bpmn_metadata::BaseMetadata;
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let process = &mut Arc::make_mut(&mut p.documents[0]).processes[0];
    process.lane_set_id = None;
    process.lane_set_base = Some(BaseMetadata::default());
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    let mutation = || Mutation::PromoteElementIdentity {
        document_id: docid(),
        model_path: "/processes/0/lane_set_id".into(),
        xml_id: None,
    };
    assert!(apply(&mut s, batch(vec![mutation()])).is_err());
    let mut envelope = env(&s);
    envelope.expected_generation = Some(s.snapshot().generation);
    let result = s.apply(envelope, batch(vec![mutation()])).unwrap();
    assert_eq!(
        s.snapshot().project.documents[0].processes[0]
            .lane_set_id
            .as_ref()
            .unwrap()
            .selector(),
        result.promoted_identities[0].selector
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn explicit_anonymous_child_lane_set_can_be_promoted() {
    use processcraft_core::bpmn_metadata::BaseMetadata;
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let process = &mut Arc::make_mut(&mut p.documents[0]).processes[0];
    process.lanes[0].child_lane_set_id = None;
    process.lanes[0].child_lane_set_base = Some(BaseMetadata::default());
    let mut s = Session::new(p).unwrap();
    let mut envelope = env(&s);
    envelope.expected_generation = Some(s.snapshot().generation);
    let result = s
        .apply(
            envelope,
            batch(vec![Mutation::PromoteElementIdentity {
                document_id: docid(),
                model_path: "/processes/0/lanes/0/child_lane_set_id".into(),
                xml_id: None,
            }]),
        )
        .unwrap();
    assert_eq!(
        s.snapshot().project.documents[0].processes[0].lanes[0]
            .child_lane_set_id
            .as_ref()
            .unwrap()
            .selector(),
        result.promoted_identities[0].selector
    );
}

#[test]
fn mixed_script_survives_unrelated_edits_and_rejects_plain_replacement() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_xml::{MixedXml, NamespaceBinding, XmlContent, XmlNode},
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let node = Arc::make_mut(&mut p.documents[0]).processes[0]
        .nodes
        .iter_mut()
        .find(|node| node.id == id::<processcraft_core::ids::ElementIdentity>("review"))
        .unwrap();
    node.kind = NodeKind::ScriptTask;
    let body = MixedXml::Structured(XmlContent {
        namespaces: vec![NamespaceBinding {
            prefix: String::new(),
            uri: String::new(),
        }],
        nodes: vec![
            XmlNode::Text {
                text: "before".into(),
            },
            XmlNode::ProcessingInstruction {
                target: "vendor".into(),
                data: Some("inert".into()),
            },
            XmlNode::Text {
                text: "after".into(),
            },
        ],
    });
    node.activity = Some(ActivityProperties {
        script: Some(body.clone()),
        ..Default::default()
    });
    let mut s = Session::new(p).unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("review"),
            name: Some("New label".into()),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetScriptBody {
                document_id: docid(),
                element_id: id("review"),
                body: Some(MixedXml::Text("flattened".into()))
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    let snap = s.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("review")).unwrap();
    assert_eq!(node.activity.as_ref().unwrap().script, Some(body));
}

#[test]
fn base_metadata_edit_preserves_other_fields_and_document_id() {
    use processcraft_core::{
        bpmn_metadata::{BaseMetadata, Documentation},
        bpmn_xml::MixedXml,
    };
    let mut s = project();
    fixture(&mut s);
    let docs = Documentation {
        id: Some(id("lane_doc")),
        text_format: None,
        content: MixedXml::Text("original".into()),
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetBaseMetadata {
            document_id: docid(),
            element_id: id("lane_request"),
            metadata: Some(BaseMetadata {
                documentation: vec![docs.clone()],
                ..Default::default()
            }),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let mut changed = docs.clone();
    changed.content = MixedXml::Text("updated".into());
    apply(
        &mut s,
        batch(vec![Mutation::SetDocumentationEntry {
            document_id: docid(),
            documentation_id: id("lane_doc"),
            documentation: changed.clone(),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let lane = snap.project.documents[0].processes[0]
        .lanes
        .iter()
        .find(|lane| lane.id == id::<processcraft_core::ids::ElementIdentity>("lane_request"))
        .unwrap();
    assert_eq!(lane.base.as_ref().unwrap().documentation, vec![changed]);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn anonymous_mixed_documentation_can_gain_only_internal_identity() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::{MixedXml, NamespaceBinding, XmlContent, XmlNode},
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    Arc::make_mut(&mut p.documents[0]).processes[0].documentation =
        DocumentationSet::Structured(vec![Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Structured(XmlContent {
                namespaces: vec![NamespaceBinding {
                    prefix: String::new(),
                    uri: String::new(),
                }],
                nodes: vec![
                    XmlNode::Text {
                        text: "before".into(),
                    },
                    XmlNode::ProcessingInstruction {
                        target: "vendor".into(),
                        data: Some("opaque".into()),
                    },
                    XmlNode::Text {
                        text: "after".into(),
                    },
                ],
            }),
        }]);
    let mut s = Session::new(p).unwrap();
    let mut envelope = env(&s);
    envelope.expected_generation = Some(0);
    let result = s
        .apply(
            envelope,
            batch(vec![Mutation::PromoteElementIdentity {
                document_id: docid(),
                model_path: "/processes/0/documentation/0/id".into(),
                xml_id: None,
            }]),
        )
        .unwrap();
    assert_eq!(result.promoted_identities.len(), 1);
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: result.promoted_identities[0].selector.clone(),
                xml_id: Some(id("new_id"))
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    let snap = s.snapshot();
    let DocumentationSet::Structured(docs) = &snap.project.documents[0].processes[0].documentation
    else {
        panic!("missing structured documentation")
    };
    let mut edited = docs[0].clone();
    edited.text_format = Some("text/plain".into());
    apply(
        &mut s,
        batch(vec![Mutation::SetDocumentationEntry {
            document_id: docid(),
            documentation_id: result.promoted_identities[0].selector.clone(),
            documentation: edited.clone(),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    assert_eq!(
        snap.project.documents[0].processes[0].documentation,
        DocumentationSet::Structured(vec![edited])
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn definitions_relationship_id_rewrite_preserves_extension_definition_qname() {
    use processcraft_core::{
        bpmn_metadata::Documentation, bpmn_model::QNameRef, bpmn_xml::MixedXml,
    };
    let mut s = project();
    fixture(&mut s);
    let reference = QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("procurement"),
    };
    let extension = DefinitionsExtension {
        documentation: vec![Documentation {
            id: Some(id("extension_doc")),
            text_format: None,
            content: MixedXml::Text("help".into()),
        }],
        definition: Some(reference.clone()),
        must_understand: Some(false),
    };
    let relationship = DefinitionsRelationship {
        id: Some(id("relationship")),
        base: None,
        relationship_type: "trace".into(),
        direction: None,
        sources: vec![reference.clone()],
        targets: vec![QNameRef {
            namespace: "urn:external".into(),
            local: id("procurement"),
        }],
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetDefinitionsExtensions {
                document_id: docid(),
                extensions: vec![extension],
            },
            Mutation::SetDefinitionsRelationships {
                document_id: docid(),
                relationships: vec![relationship],
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("procurement"),
            xml_id: Some(id("renamed_process")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let doc = &snap.project.documents[0];
    assert_eq!(doc.extensions[0].definition.as_ref(), Some(&reference));
    assert_eq!(
        doc.relationships[0].sources[0].local,
        id::<processcraft_core::ids::XmlId>("renamed_process")
    );
    assert_eq!(
        doc.relationships[0].targets[0].local,
        id::<processcraft_core::ids::XmlId>("procurement")
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn documentation_id_rename_and_collision_are_atomic() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::MixedXml,
    };
    let mut s = project();
    fixture(&mut s);
    apply(
        &mut s,
        batch(vec![Mutation::ReplaceDocumentation {
            document_id: docid(),
            element_id: id("review"),
            documentation: DocumentationSet::Structured(vec![Documentation {
                id: Some(id("doc_id")),
                text_format: None,
                content: MixedXml::Text("safe".into()),
            }]),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetElementXmlId {
                document_id: docid(),
                element_id: id("doc_id"),
                xml_id: Some(id("review"))
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    let change = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("doc_id"),
            xml_id: Some(id("doc_new")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &id("review")).unwrap();
    let DocumentationSet::Structured(docs) = &node.documentation else {
        panic!("missing docs")
    };
    assert_eq!(
        docs[0].id.as_ref().unwrap().selector(),
        change.selector_mappings[0].to
    );
    assert_eq!(docs[0].id.as_ref().unwrap().xml_id(), Some(&id("doc_new")));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn deleting_node_removes_navigation_from_its_documentation() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::MixedXml,
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut p.documents[0]);
    doc.processes[0]
        .nodes
        .iter_mut()
        .find(|node| node.id == id::<processcraft_core::ids::ElementIdentity>("start"))
        .unwrap()
        .documentation = DocumentationSet::Structured(vec![Documentation {
        id: Some(id("owned_doc")),
        text_format: None,
        content: MixedXml::Text("context".into()),
    }]);
    doc.processes[0].flows.retain(|flow| {
        flow.source
            != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "start",
            )))
            && flow.target
                != Some(processcraft_core::bpmn_model::EventReference::Local(id(
                    "start",
                )))
    });
    for diagram in &mut doc.diagrams {
        diagram.shapes.retain(|shape| {
            shape.element != id::<processcraft_core::ids::ElementSelector>("start")
        });
        diagram.edges.retain(|edge| {
            edge.element != id::<processcraft_core::ids::ElementSelector>("flow_start_review")
        });
    }
    p.links.push(NavigationLink {
        source: ElementKey {
            document_id: docid(),
            xml_id: id("owned_doc"),
        },
        target: docid(),
        label: "doc".into(),
    });
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::Delete {
            document_id: docid(),
            element_id: id("start"),
        }]),
    )
    .unwrap();
    assert!(s.snapshot().project.links.is_empty());
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn removing_documentation_referenced_by_local_relationship_is_atomic() {
    use processcraft_core::{
        bpmn_metadata::Documentation, bpmn_model::QNameRef, bpmn_xml::MixedXml,
    };
    let mut s = project();
    fixture(&mut s);
    let q = |local: &str| QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::SetDefinitionsExtensions {
                document_id: docid(),
                extensions: vec![DefinitionsExtension {
                    documentation: vec![Documentation {
                        id: Some(id("extension_doc")),
                        text_format: None,
                        content: MixedXml::Text("help".into()),
                    }],
                    definition: None,
                    must_understand: None,
                }],
            },
            Mutation::SetDefinitionsRelationships {
                document_id: docid(),
                relationships: vec![DefinitionsRelationship {
                    id: Some(id("relation")),
                    base: None,
                    relationship_type: "trace".into(),
                    direction: None,
                    sources: vec![q("extension_doc")],
                    targets: vec![q("review")],
                }],
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsExtensions {
                document_id: docid(),
                extensions: vec![]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    let replacement = Documentation {
        id: Some(processcraft_core::ids::ElementIdentity::Native {
            native_id: processcraft_core::ids::NativeElementId::new(),
            xml_id: Some(id("extension_doc")),
        }),
        text_format: None,
        content: MixedXml::Text("different element".into()),
    };
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsExtensions {
                document_id: docid(),
                extensions: vec![DefinitionsExtension {
                    documentation: vec![replacement],
                    definition: None,
                    must_understand: None
                }]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn new_local_relationship_reference_must_resolve_but_legacy_and_external_survive() {
    use processcraft_core::bpmn_model::QNameRef;
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let local = QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("missing"),
    };
    let external = QNameRef {
        namespace: "urn:external".into(),
        local: id("external"),
    };
    let relationship = DefinitionsRelationship {
        id: Some(id("legacy_relation")),
        base: None,
        relationship_type: "trace".into(),
        direction: None,
        sources: vec![local.clone()],
        targets: vec![external.clone()],
    };
    Arc::make_mut(&mut p.documents[0])
        .relationships
        .push(relationship.clone());
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    let mut new = relationship.clone();
    new.sources.push(QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id("another_missing"),
    });
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetDefinitionsRelationships {
                document_id: docid(),
                relationships: vec![new]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    let mut preserved = relationship;
    preserved.relationship_type = "updated".into();
    apply(
        &mut s,
        batch(vec![Mutation::SetDefinitionsRelationships {
            document_id: docid(),
            relationships: vec![preserved],
        }]),
    )
    .unwrap();
}

#[test]
fn text_and_comment_only_documentation_allows_safe_identity_edit() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::{MixedXml, NamespaceBinding, XmlContent, XmlNode},
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    Arc::make_mut(&mut p.documents[0]).processes[0].documentation =
        DocumentationSet::Structured(vec![Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Structured(XmlContent {
                namespaces: vec![NamespaceBinding {
                    prefix: String::new(),
                    uri: String::new(),
                }],
                nodes: vec![
                    XmlNode::Text {
                        text: "before".into(),
                    },
                    XmlNode::Comment {
                        text: "comment".into(),
                    },
                    XmlNode::Text {
                        text: "after".into(),
                    },
                ],
            }),
        }]);
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("review"),
            xml_id: Some(id("renamed")),
        }]),
    )
    .unwrap();
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn explicit_expression_edit_preserves_id_and_undo() {
    use processcraft_core::bpmn_model::{
        Expression, ExpressionBody, ExpressionData, ExpressionKind,
    };
    let mut s = project();
    fixture(&mut s);
    let value = Expression::Structured(ExpressionData {
        id: Some(id("expression")),
        kind: ExpressionKind::Base {
            explicit_type: false,
        },
        text: ExpressionBody::Text("first".into()),
        namespaces: vec![(
            "bpmn".into(),
            "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
        )],
        foreign_attributes: vec![],
    });
    apply(
        &mut s,
        batch(vec![Mutation::SetFlowCondition {
            document_id: docid(),
            flow_id: id("flow_approved"),
            condition: Some(value.clone()),
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let mut edited = value;
    if let Expression::Structured(data) = &mut edited {
        data.text = ExpressionBody::Text("second".into());
    }
    apply(
        &mut s,
        batch(vec![Mutation::SetExpression {
            document_id: docid(),
            expression_id: id("expression"),
            expression: edited,
        }]),
    )
    .unwrap();
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn whole_value_mutations_cannot_change_existing_native_xml_id() {
    use processcraft_core::{
        bpmn_model::EventDefinition,
        ids::{ElementIdentity, NativeElementId},
    };
    let mut s = project();
    fixture(&mut s);
    let native_id = NativeElementId::new();
    let root = |xml: &str| EventDefinition::Signal {
        id: Some(ElementIdentity::Native {
            native_id,
            xml_id: Some(id(xml)),
        }),
        base: None,
        signal_ref: None,
    };
    apply(
        &mut s,
        batch(vec![Mutation::SetRootEventDefinitions {
            document_id: docid(),
            definitions: vec![root("original")],
        }]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetRootEventDefinitions {
                document_id: docid(),
                definitions: vec![root("changed")]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: processcraft_core::ids::ElementSelector::Native { native_id },
            xml_id: Some(id("changed")),
        }]),
    )
    .unwrap();
}

#[test]
fn event_resource_replacement_preserves_previously_resolved_activity_message() {
    use processcraft_core::{
        bpmn_activity::ActivityProperties,
        bpmn_model::{EventResource, QNameRef},
    };
    let mut original = project();
    fixture(&mut original);
    let mut p = (*original.snapshot().project).clone();
    let doc = Arc::make_mut(&mut p.documents[0]);
    doc.event_resources.push(EventResource::Message {
        id: id("message"),
        base: None,
        name: None,
        item_ref: None,
    });
    let node = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|node| node.id == id::<processcraft_core::ids::ElementIdentity>("review"))
        .unwrap();
    node.kind = NodeKind::SendTask;
    node.activity = Some(ActivityProperties {
        message_ref: Some(QNameRef {
            namespace: GENERATED_TARGET_NAMESPACE.into(),
            local: id("message"),
        }),
        ..Default::default()
    });
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::SetEventResources {
                document_id: docid(),
                resources: vec![]
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn diagram_label_style_identity_rename_updates_local_reference_and_undo() {
    let mut s = project();
    fixture(&mut s);
    apply(
        &mut s,
        batch(vec![
            Mutation::SetDiagramLabelStyles {
                document_id: docid(),
                diagram_id: id("procurement_diagram"),
                label_styles: vec![LabelStyle {
                    id: Some(id("label_style")),
                    font: Some(DiFont {
                        name: Some("Sans".into()),
                        ..Default::default()
                    }),
                }],
            },
            Mutation::SetShapeLabel {
                document_id: docid(),
                diagram_id: id("procurement_diagram"),
                shape_id: id("pc_shape_start"),
                label: Some(Label {
                    di_metadata: None,
                    id: Some(id("label")),
                    bounds: None,
                    style: None,
                    style_ref: Some(DiStyleRef::Local(id("label_style"))),
                }),
                label_visibility: Some(DiLabelVisibility::Explicit),
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let changed = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("label_style"),
            xml_id: Some(id("new_style")),
        }]),
    )
    .unwrap();
    let snap = s.snapshot();
    let diagram = &snap.project.documents[0].diagrams[0];
    let shape = diagram
        .shapes
        .iter()
        .find(|shape| shape.id == id::<processcraft_core::ids::ElementIdentity>("pc_shape_start"))
        .unwrap();
    assert!(
        matches!(&shape.label.as_ref().unwrap().style_ref, Some(DiStyleRef::Local(selector)) if selector == &changed.selector_mappings[0].to)
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn loop_identity_rename_preserves_checkpoint_and_undo_redo() {
    use processcraft_core::{
        bpmn_model::{LoopCharacteristics, QNameRef},
        ids::CheckpointId,
    };
    let mut s = project();
    fixture(&mut s);
    let loop_value = LoopCharacteristics::Standard {
        id: Some(id("loop_id")),
        base: None,
        test_before: Some(false),
        condition: None,
        maximum: None,
    };
    let relation = DefinitionsRelationship {
        id: Some(id("relation_loop")),
        base: None,
        relationship_type: "trace".into(),
        direction: None,
        sources: vec![QNameRef {
            namespace: GENERATED_TARGET_NAMESPACE.into(),
            local: id("loop_id"),
        }],
        targets: vec![QNameRef {
            namespace: "urn:external".into(),
            local: id("loop_id"),
        }],
    };
    let checkpoint = CheckpointId::new();
    apply(
        &mut s,
        batch(vec![
            Mutation::SetNodeLoop {
                document_id: docid(),
                element_id: id("review"),
                loop_characteristics: Some(loop_value),
            },
            Mutation::SetDefinitionsRelationships {
                document_id: docid(),
                relationships: vec![relation],
            },
            Mutation::CreateCheckpoint {
                id: checkpoint,
                name: "Before loop rename".into(),
                created_unix_ms: 42,
            },
        ]),
    )
    .unwrap();
    let before = s.snapshot().content_id;
    let change = apply(
        &mut s,
        batch(vec![Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("loop_id"),
            xml_id: Some(id("loop_new")),
        }]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    assert_ne!(before, after);
    let snapshot = s.snapshot();
    let doc = &snapshot.project.documents[0];
    let node = processcraft_core::queries::find_node(doc, &id("review")).unwrap();
    let loop_id = node.loop_characteristics.as_ref().unwrap().id().unwrap();
    assert_eq!(loop_id.selector(), change.selector_mappings[0].to);
    assert_eq!(loop_id.xml_id(), Some(&id("loop_new")));
    assert_eq!(
        doc.relationships[0].sources[0].local,
        id::<processcraft_core::ids::XmlId>("loop_new")
    );
    assert_eq!(
        doc.relationships[0].targets[0].local,
        id::<processcraft_core::ids::XmlId>("loop_id")
    );
    let frozen = &snapshot.project.checkpoints[0].content.documents[0];
    assert_eq!(
        processcraft_core::queries::find_node(frozen, &id("review"))
            .unwrap()
            .loop_characteristics
            .as_ref()
            .unwrap()
            .id()
            .unwrap()
            .xml_id(),
        Some(&id("loop_id"))
    );
    let undo = apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert_eq!(
        undo.selector_mappings[0].to,
        id::<processcraft_core::ids::ElementSelector>("loop_id")
    );
    let redo = apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
    assert_eq!(redo.selector_mappings[0].to, change.selector_mappings[0].to);
}
