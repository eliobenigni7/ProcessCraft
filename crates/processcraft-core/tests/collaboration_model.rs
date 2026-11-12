use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{CollaborationRootKind, PartnerEntity},
    ids::{ElementIdentity, ElementSelector},
    model::*,
};
use std::sync::Arc;
fn collaboration(id: &str) -> Collaboration {
    Collaboration {
        id: id.parse().unwrap(),
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
#[test]
fn legacy_local_endpoint_and_process_wire_is_unchanged() {
    let selector: ElementSelector = "item".parse().unwrap();
    let old = serde_json::to_value(&selector).unwrap();
    let wrapped = serde_json::to_value(Some(ModelReference::Local(selector))).unwrap();
    assert_eq!(old, wrapped);
    let doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    assert!(!doc.has_expanded_collaboration());
    assert!(
        serde_json::to_value(&doc)
            .unwrap()
            .get("root_order")
            .is_none()
    );
}
#[test]
fn root_order_requires_explicit_anonymous_promotion_and_exact_membership() {
    let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    let mut first = collaboration("first");
    first.kind = Some(CollaborationRootKind::GlobalConversation);
    doc.collaboration = Some(first);
    doc.additional_collaborations.push(collaboration("second"));
    doc.partner_entities.push(PartnerEntity {
        id: None,
        base: None,
        name: None,
        participant_refs: vec![],
    });
    assert!(doc.has_expanded_collaboration());
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.root_order = Some(vec!["first".parse().unwrap(), "second".parse().unwrap()]);
    assert!(project.validate().unwrap_err().contains("promotion"));
    let doc = Arc::make_mut(&mut project.documents[0]);
    let mut paths = vec![];
    doc.visit_optional_root_identities_with_path_mut(|path, id| {
        if id.is_none() {
            paths.push(path.to_owned());
            *id = Some(ElementIdentity::anonymous())
        }
    });
    assert_eq!(paths, ["/partner_entities/0/id"]);
    doc.root_order = Some(
        doc.root_element_identities()
            .into_iter()
            .map(|id| id.unwrap().selector())
            .collect(),
    );
    project.validate().unwrap();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.root_order
        .as_mut()
        .unwrap()
        .push("first".parse().unwrap());
    assert!(
        project
            .validate()
            .unwrap_err()
            .contains("each modeled root exactly once")
    );
}

#[test]
fn group_category_reference_preserves_legacy_wire_and_external_namespace() {
    use processcraft_core::bpmn_model::{Group, QNameRef};
    let selector: ElementSelector = "category".parse().unwrap();
    let local = Group {
        id: "group".parse().unwrap(),
        base: None,
        category_value_ref: Some(ModelReference::Local(selector.clone())),
    };
    assert_eq!(
        serde_json::to_value(&local).unwrap()["category_value_ref"],
        serde_json::to_value(&selector).unwrap()
    );
    let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    doc.processes.push(Process {
        id: "process".parse().unwrap(),
        name: None,
        documentation: "".into(),
        base_extras: None,
        is_executable: Some(false),
        activity_metadata: None,
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![local],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    assert!(!doc.has_expanded_collaboration());
    doc.processes[0].groups[0].category_value_ref = Some(ModelReference::Qualified(QNameRef {
        namespace: "urn:external".into(),
        local: "category".parse().unwrap(),
    }));
    assert!(doc.has_expanded_collaboration());
    assert!(doc.has_collaboration_qname_refs());
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
}

#[test]
fn additional_collaboration_retains_external_and_missing_qname_endpoints() {
    use processcraft_core::bpmn_model::QNameRef;
    let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    doc.collaboration = Some(collaboration("first"));
    let mut second = collaboration("second");
    second.participants.push(Participant {
        id: "participant".parse().unwrap(),
        name: None,
        base: None,
        multiplicity: None,
        process: Some(ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "process".parse().unwrap(),
        })),
        interface_refs: vec![],
        endpoint_refs: vec![],
    });
    second.flows.push(Flow {
        id: "message_flow".parse().unwrap(),
        kind: FlowKind::Message,
        source: Some(ModelReference::Local("participant".parse().unwrap())),
        target: Some(ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "participant".parse().unwrap(),
        })),
        name: None,
        documentation: "".into(),
        base_extras: None,
        flow_metadata: None,
        message_ref: None,
        condition: None,
        association_direction: None,
    });
    second
        .conversation_links
        .push(processcraft_core::bpmn_collaboration::ConversationLink {
            id: "link".parse().unwrap(),
            name: None,
            base: None,
            source: None,
            target: None,
        });
    doc.additional_collaborations.push(second);
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
    let doc = Arc::make_mut(&mut project.documents[0]);
    doc.additional_collaborations[0].flows[0].target = None;
    assert!(doc.has_expanded_collaboration());
    project.validate().unwrap();
}

#[test]
fn conversation_correlation_refs_are_scanned_for_namespace_and_native_identity() {
    use processcraft_core::bpmn_activity::CorrelationKey;
    use processcraft_core::bpmn_collaboration::{ConversationKind, ConversationNode};
    use processcraft_core::bpmn_model::QNameRef;
    let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    let mut root = collaboration("root");
    root.conversation_nodes.push(ConversationNode {
        id: "node".parse().unwrap(),
        base: None,
        name: None,
        kind: ConversationKind::Conversation,
        parent: None,
        participant_refs: vec![],
        message_flow_refs: vec![],
        call: None,
        correlation_keys: vec![CorrelationKey {
            id: None,
            base: None,
            name: None,
            property_refs: vec![ModelReference::Qualified(QNameRef {
                namespace: "urn:outside".into(),
                local: "property".parse().unwrap(),
            })],
        }],
    });
    doc.additional_collaborations.push(root);
    assert!(doc.has_collaboration_qname_refs());
    doc.additional_collaborations[0].conversation_nodes[0].correlation_keys[0].property_refs =
        vec![ModelReference::Local(
            ElementIdentity::anonymous().selector(),
        )];
    assert!(doc.has_collaboration_native_refs());
}

#[test]
fn initialize_root_order_matches_legacy_writer_family_order() {
    use processcraft_core::{
        bpmn_activity::{CallableMetadata, CorrelationProperty, GlobalTask, GlobalTaskKind},
        bpmn_catalog::ItemDefinition,
        bpmn_collaboration::{Choreography, EndPoint, PartnerRole},
        bpmn_data::DataStore,
        bpmn_model::{Category, EventDefinition, EventResource},
        commands::{Envelope, Mutation, Request, Session},
    };
    let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    doc.catalog.item_definitions.push(ItemDefinition {
        id: Some("item".parse().unwrap()),
        base: None,
        structure_ref: None,
        is_collection: None,
        item_kind: None,
    });
    doc.global_tasks.push(GlobalTask {
        id: Some("global".parse().unwrap()),
        base: None,
        name: None,
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::Task,
    });
    doc.correlation_properties.push(CorrelationProperty {
        id: Some("correlation".parse().unwrap()),
        base: None,
        name: None,
        item_type: None,
        retrieval_expressions: vec![],
    });
    doc.categories.push(Category {
        id: "category".parse().unwrap(),
        base: None,
        name: None,
        values: vec![],
    });
    doc.data_stores.push(DataStore {
        id: Some("store".parse().unwrap()),
        base: None,
        name: None,
        capacity: None,
        is_unlimited: None,
        item_aware: None,
    });
    doc.event_resources.push(EventResource::Signal {
        id: "signal".parse().unwrap(),
        base: None,
        name: None,
        structure_ref: None,
    });
    doc.root_event_definitions.push(EventDefinition::Cancel {
        id: Some("definition".parse().unwrap()),
        base: None,
    });
    doc.partner_entities.push(PartnerEntity {
        id: Some("entity".parse().unwrap()),
        base: None,
        name: None,
        participant_refs: vec![],
    });
    doc.partner_roles.push(PartnerRole {
        id: Some("role".parse().unwrap()),
        base: None,
        name: None,
        participant_refs: vec![],
    });
    doc.endpoints.push(EndPoint {
        id: Some("endpoint".parse().unwrap()),
        base: None,
    });
    doc.choreographies.push(Choreography {
        kind: None,
        graph: None,
        id: "choreography".parse().unwrap(),
        base: None,
        extras: None,
        name: None,
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
    });
    doc.collaboration = Some(collaboration("collaboration"));
    doc.additional_collaborations
        .push(collaboration("additional"));
    doc.processes.push(Process {
        id: "process".parse().unwrap(),
        name: None,
        documentation: "".into(),
        base_extras: None,
        is_executable: Some(false),
        activity_metadata: None,
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let document_id = doc.id;
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    let mut session = Session::new(project).unwrap();
    let envelope = Envelope {
        request_id: "order".into(),
        session_id: session.id(),
        expected_generation: Some(session.snapshot().generation),
    };
    session
        .apply(
            envelope,
            Request::Batch {
                mutations: vec![Mutation::InitializeDocumentRootOrder { document_id }],
            },
        )
        .unwrap();
    let snapshot = session.snapshot();
    let ids: Vec<_> = snapshot.project.documents[0]
        .root_order
        .as_ref()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        ids,
        [
            "item",
            "global",
            "correlation",
            "category",
            "store",
            "signal",
            "definition",
            "entity",
            "role",
            "endpoint",
            "choreography",
            "collaboration",
            "additional",
            "process"
        ]
    );
}
