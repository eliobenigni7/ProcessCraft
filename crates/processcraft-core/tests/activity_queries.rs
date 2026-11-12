use processcraft_core::{
    bpmn_activity::{
        CallableMetadata, CorrelationProperty, CorrelationRetrieval, GlobalTask, GlobalTaskKind,
    },
    ids::{CheckpointId, ElementIdentity},
    model::{BpmnDocument, Checkpoint, FrozenProject, ProjectData},
    queries,
};
use std::{collections::BTreeMap, sync::Arc};

#[test]
fn activity_pages_and_checkpoint_preserve_anonymous_owner_and_omission() {
    let mut project = ProjectData::new("Activity queries".into());
    let mut doc = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    let task_id = ElementIdentity::anonymous();
    doc.global_tasks.push(GlobalTask {
        id: Some(task_id.clone()),
        base: None,
        name: None,
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::Manual,
    });
    doc.correlation_properties.push(CorrelationProperty {
        id: None,
        base: None,
        name: Some(String::new()),
        item_type: None,
        retrieval_expressions: vec![CorrelationRetrieval {
            id: None,
            base: None,
            message_ref: None,
            message_path: Some("before".into()),
        }],
    });
    let document_id = doc.id;
    project.documents.push(Arc::new(doc));
    let checkpoint_id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id: checkpoint_id,
        name: "Before".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let original = serde_json::to_value(&project).unwrap();
    let rows = queries::identified_elements(&project, document_id).unwrap();
    assert!(
        rows.iter()
            .any(|row| row.selector == task_id.selector() && row.xml_id.is_none())
    );
    let mut fields = BTreeMap::new();
    let mut offset = 0;
    loop {
        let page = queries::document_fields_page(&project, document_id, offset, 3)
            .unwrap()
            .unwrap();
        assert!(page.items.len() <= 3);
        for field in page.items {
            assert!(fields.insert(field.path, field.value).is_none());
        }
        if let Some(next) = page.next_offset {
            assert!(next > offset);
            offset = next;
        } else {
            break;
        }
    }
    assert_eq!(fields["/global_tasks/0/name"], serde_json::Value::Null);
    assert_eq!(fields["/correlation_properties/0/name"], "");
    assert_eq!(
        fields["/correlation_properties/0/id"],
        serde_json::Value::Null
    );
    assert_eq!(
        fields["/correlation_properties/0/retrieval_expressions/0/message_path"],
        "before"
    );
    assert_eq!(serde_json::to_value(&project).unwrap(), original);

    Arc::make_mut(&mut project.documents[0]).correlation_properties[0].retrieval_expressions[0]
        .message_path = Some("after".into());
    let changed = serde_json::to_value(&project).unwrap();
    let comparison = queries::compare_checkpoint(&project, checkpoint_id).unwrap();
    assert!(comparison.added_elements.is_empty());
    assert!(comparison.removed_elements.is_empty());
    let change = comparison
        .changed_properties
        .iter()
        .find(|row| row.key.xml_id == project.documents[0].definitions_id.selector())
        .unwrap();
    assert!(change.properties.iter().any(|field| {
        field.property == "content.correlation_properties"
            && field
                .before
                .pointer("/0/retrieval_expressions/0/message_path")
                == Some(&serde_json::json!("before"))
            && field
                .after
                .pointer("/0/retrieval_expressions/0/message_path")
                == Some(&serde_json::json!("after"))
    }));
    assert!(project.documents[0].correlation_properties[0].id.is_none());
    assert_eq!(
        project.documents[0].global_tasks[0].id.as_ref(),
        Some(&task_id)
    );
    assert_eq!(serde_json::to_value(&project).unwrap(), changed);
}

#[test]
fn activity_root_names_resolve_by_stable_selector_for_navigation() {
    let mut doc = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    let task = ElementIdentity::anonymous();
    let property = ElementIdentity::anonymous();
    doc.global_tasks.push(GlobalTask {
        id: Some(task.clone()),
        base: None,
        name: Some("Reusable review".into()),
        callable: CallableMetadata::default(),
        io_specification: None,
        resource_roles: vec![],
        kind: GlobalTaskKind::User {
            implementation: None,
            renderings: vec![],
        },
    });
    doc.correlation_properties.push(CorrelationProperty {
        id: Some(property.clone()),
        base: None,
        name: Some("Order number".into()),
        item_type: None,
        retrieval_expressions: vec![],
    });
    let before = serde_json::to_value(&doc).unwrap();
    assert_eq!(
        queries::element_name(&doc, &task.selector()),
        Some("Reusable review")
    );
    assert_eq!(
        queries::element_name(&doc, &property.selector()),
        Some("Order number")
    );
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}
