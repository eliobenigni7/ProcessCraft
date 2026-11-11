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
            id: id("message1"),
            name: None,
        });
    let mut s = Session::new(project).unwrap();
    let q = |local| QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    };
    let catalog = SupportingCatalog {
        item_definitions: vec![ItemDefinition {
            id: Some(id("item1")),
            structure_ref: None,
            is_collection: None,
            item_kind: None,
        }],
        interfaces: vec![Interface {
            id: Some(id("interface1")),
            name: String::new(),
            implementation_ref: None,
            operations: vec![Operation {
                id: Some(id("operation1")),
                name: "op".into(),
                implementation_ref: None,
                in_message_ref: q("message1"),
                out_message_ref: None,
                error_refs: vec![],
            }],
        }],
        resources: vec![BpmnResource {
            id: Some(id("resource1")),
            name: "r".into(),
            parameters: vec![ResourceParameter {
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
        id: None,
        name: None,
        is_collection: None,
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: None,
            data_state: Some(DataState {
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
        id: None,
        inputs: vec![DataPort {
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
        id: id("message1"),
        name: None,
    });
    let q = |local| QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    };
    d.catalog.interfaces.push(Interface {
        id: Some(id("interface1")),
        name: "i".into(),
        implementation_ref: None,
        operations: vec![Operation {
            id: Some(id("operation1")),
            name: "o".into(),
            implementation_ref: None,
            in_message_ref: q("message1"),
            out_message_ref: None,
            error_refs: vec![],
        }],
    });
    let node = Node {
        item_aware: None,
        id: id("send"),
        name: Some(String::new()),
        documentation: String::new(),
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
        id: None,
        kind: ExpressionKind::Base {
            explicit_type: false,
        },
        text: String::new(),
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
        item_aware: None,
        id: id(name),
        name: Some(String::new()),
        documentation: String::new(),
        kind,
        details: Some(NodeDetails::Event {
            definitions: vec![definition],
            attached_to: None,
            cancel_activity: None,
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
            id: Some(id("throw_def")),
            name: "jump".into(),
            sources: vec![],
            target: Some(id("catch_def")),
        },
    );
    let catch = node(
        "link_catch",
        NodeKind::IntermediateCatchEvent,
        EventDefinition::Link {
            id: Some(id("catch_def")),
            name: "jump".into(),
            sources: vec![id("throw_def")],
            target: None,
        },
    );
    let compensation = node(
        "compensate",
        NodeKind::IntermediateThrowEvent,
        EventDefinition::Compensate {
            id: None,
            wait_for_completion: None,
            activity_ref: Some(id("review")),
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
        EventDefinition::Cancel { id: None },
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
        target_namespace: GENERATED_TARGET_NAMESPACE.into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    };
    let node = Node {
        item_aware: None,
        id: id("call"),
        name: Some(String::new()),
        documentation: String::new(),
        kind: NodeKind::CallActivity,
        details: Some(NodeDetails::CallActivity {
            called_element: QNameRef {
                namespace: GENERATED_TARGET_NAMESPACE.into(),
                local: id("procurement"),
            },
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
            association_direction: None,
            id: id("bad"),
            kind: FlowKind::Sequence,
            source: id("review"),
            target: id("missing"),
            name: Some(String::new()),
            documentation: String::new(),
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
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("p"),
        name: Some("before".into()),
        documentation: String::new(),
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
fn deletion_rejects_dangling_connections_atomically() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot();
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
    assert_eq!(s.snapshot().content_id, before.content_id);
    assert_eq!(s.snapshot().undo_count, before.undo_count);
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
        "setCollaborationName",
        "setDiagramName",
        "setProcessExecutable",
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
    assert!(process.flows.iter().any(|flow| &flow.target == promoted));
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
        bpmn_model::QNameRef,
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
        id: Some(id("item")),
        structure_ref: Some(reference.clone()),
        is_collection: None,
        item_kind: None,
    });
    doc.data_stores.push(DataStore {
        id: Some(id("store")),
        name: Some("Store".into()),
        capacity: None,
        is_unlimited: None,
        item_aware: Some(ItemAwareMetadata {
            item_subject_ref: Some(reference.clone()),
            data_state: None,
        }),
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
            id: id("annotation"),
            name: Some("old body".into()),
            documentation: String::new(),
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
        matches!(&node.details, Some(NodeDetails::TextAnnotation { text_format: Some(format), text: Some(text) }) if format == "text/plain" && text == "new body")
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
