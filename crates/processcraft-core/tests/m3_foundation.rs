//! M3 foundation family: command/undo, model and validation assertions.
use processcraft_core::{
    bpmn_activity::{FlowElementMetadata, ModelReference, MultiInstanceFlowCondition},
    bpmn_metadata::{Documentation, validate_documentation},
    bpmn_model::{
        Category, CategoryValue, EventGatewayType, LoopCharacteristics, NodeDetails, QNameRef,
    },
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, XmlId},
    model::*,
    queries::find_node,
    validation::activity_family_reference_issues,
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
fn local(name: &str) -> ModelReference {
    ModelReference::Local(id(name))
}

#[test]
fn process_name_rename_is_undoable() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("procurement"),
            name: Some("Purchasing".into()),
        }]),
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.documents[0].processes[0]
            .name
            .as_deref(),
        Some("Purchasing")
    );
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert_eq!(
        s.snapshot().project.documents[0].processes[0]
            .name
            .as_deref(),
        Some("Procurement")
    );
}

#[test]
fn category_value_ref_and_outgoing_are_set_undone_and_dangling_refs_diagnosed() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    let category = Category {
        base: None,
        id: id("cat"),
        name: Some("Class".into()),
        values: vec![CategoryValue {
            base: None,
            id: id("catvalue"),
            value: Some("A".into()),
        }],
    };
    let metadata = FlowElementMetadata {
        outgoing: vec![local("flow_review_decision")],
        category_value_refs: vec![ModelReference::Qualified(QNameRef {
            namespace: GENERATED_TARGET_NAMESPACE.into(),
            local: id::<XmlId>("catvalue"),
        })],
        ..Default::default()
    };
    apply(
        &mut s,
        batch(vec![
            Mutation::CreateCategory {
                document_id: docid(),
                category,
            },
            Mutation::SetNodeFlowMetadata {
                document_id: docid(),
                element_id: id("review"),
                metadata: Some(metadata.clone()),
            },
        ]),
    )
    .unwrap();
    let snap = s.snapshot();
    let doc = &snap.project.documents[0];
    assert_eq!(
        find_node(doc, &id("review")).unwrap().flow_metadata,
        Some(metadata.clone())
    );
    assert!(activity_family_reference_issues(doc).is_empty());
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert!(
        find_node(&s.snapshot().project.documents[0], &id("review"))
            .unwrap()
            .flow_metadata
            .is_none()
    );

    // Negative: dangling category value, and outgoing refs to a missing flow / a flow not leaving the node.
    let mut doc = (*snap.project.documents[0]).clone();
    let node = doc.processes[0]
        .nodes
        .iter_mut()
        .find(|n| n.id.to_string() == "review")
        .unwrap();
    node.flow_metadata = Some(FlowElementMetadata {
        outgoing: vec![local("flow_start_review"), local("no_such_flow")],
        category_value_refs: vec![local("no_such_value")],
        ..Default::default()
    });
    let issues = activity_family_reference_issues(&doc);
    let has = |field: &str, target: &str| {
        issues
            .iter()
            .any(|i| i.field == field && i.target == local(target))
    };
    assert!(has("flow.categoryValueRef", "no_such_value"), "{issues:?}");
    assert!(has("flow.outgoing", "no_such_flow"), "{issues:?}");
    assert!(has("flow.outgoing", "flow_start_review"), "{issues:?}");
}

#[test]
fn multi_instance_behavior_values_round_trip_through_set_node_loop_with_undo() {
    let mut s = fixture();
    let before = s.snapshot().content_id;
    for behavior in [
        MultiInstanceFlowCondition::All,
        MultiInstanceFlowCondition::Complex,
    ] {
        let value = LoopCharacteristics::MultiInstance {
            behavior: Some(behavior),
            loop_data_input_ref: None,
            loop_data_output_ref: None,
            input_data_item: None,
            output_data_item: None,
            complex_behavior_definitions: vec![],
            one_behavior_event_ref: None,
            none_behavior_event_ref: None,
            id: None,
            base: None,
            is_sequential: Some(false),
            cardinality: None,
            completion_condition: None,
        };
        apply(
            &mut s,
            batch(vec![Mutation::SetNodeLoop {
                document_id: docid(),
                element_id: id("order"),
                loop_characteristics: Some(value.clone()),
            }]),
        )
        .unwrap();
        assert_eq!(
            find_node(&s.snapshot().project.documents[0], &id("order"))
                .unwrap()
                .loop_characteristics,
            Some(value)
        );
    }
    apply(&mut s, Request::Undo).unwrap();
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert_ne!(
        MultiInstanceFlowCondition::One,
        MultiInstanceFlowCondition::None
    );
}

fn documentation(format: Option<String>) -> Documentation {
    Documentation {
        id: None,
        text_format: format,
        content: "text".into(),
    }
}

#[test]
fn documentation_text_format_is_bounded_and_character_checked() {
    assert!(validate_documentation(&documentation(Some("text/plain".into()))).is_ok());
    assert!(validate_documentation(&documentation(Some("x".repeat(16_384)))).is_ok());
    assert!(validate_documentation(&documentation(Some("x".repeat(16_385)))).is_err());
    assert!(validate_documentation(&documentation(Some("text/\u{1}plain".into()))).is_err());
}

#[test]
fn parallel_event_gateway_must_instantiate() {
    let s = fixture();
    let gateway = |instantiate| NodeDetails::Gateway {
        default_flow: None,
        direction: None,
        instantiate,
        event_gateway_type: Some(EventGatewayType::Parallel),
        activation_condition: None,
    };
    let mut project = (*s.snapshot().project).clone();
    let set = |project: &mut ProjectData, instantiate| {
        let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
        let node = doc.processes[0]
            .nodes
            .iter_mut()
            .find(|n| n.id.to_string() == "decision")
            .unwrap();
        node.kind = NodeKind::EventBasedGateway;
        node.details = Some(gateway(instantiate));
    };
    set(&mut project, Some(false));
    let diagnostics = project.diagnostics();
    let hit = diagnostics
        .iter()
        .find(|d| d.code == "parallelEventGatewayNotInstantiating")
        .expect("parallel event gateway without instantiate is diagnosed");
    assert_eq!(hit.element_id, Some(id("decision")));
    set(&mut project, Some(true));
    assert!(
        !project
            .diagnostics()
            .iter()
            .any(|d| d.code == "parallelEventGatewayNotInstantiating")
    );
}

#[test]
fn process_type_is_set_read_back_and_undone() {
    use processcraft_core::bpmn_activity::{ProcessActivityMetadata, ProcessType};
    let mut s = fixture();
    let before = s.snapshot().content_id;
    for value in [ProcessType::Public, ProcessType::Private, ProcessType::None] {
        apply(
            &mut s,
            batch(vec![Mutation::SetProcessActivityMetadata {
                document_id: docid(),
                process_id: id("procurement"),
                metadata: Some(ProcessActivityMetadata {
                    process_type: Some(value),
                    ..Default::default()
                }),
            }]),
        )
        .unwrap();
        assert_eq!(
            s.snapshot().project.documents[0].processes[0]
                .activity_metadata
                .as_ref()
                .unwrap()
                .process_type,
            Some(value)
        );
    }
    for _ in 0..3 {
        apply(&mut s, Request::Undo).unwrap();
    }
    assert_eq!(s.snapshot().content_id, before);
}
