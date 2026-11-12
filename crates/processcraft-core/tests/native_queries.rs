use processcraft_core::{
    ids::{CheckpointId, CommentId, ElementIdentity, ElementKey},
    model::*,
    queries,
};
use std::sync::Arc;

#[test]
fn anonymous_native_elements_remain_queryable_without_promoting_or_naming_them() {
    let mut document = BpmnDocument::new("Native model".into(), ElementIdentity::anonymous());
    let process = ElementIdentity::anonymous();
    let task = ElementIdentity::anonymous();
    let lane = ElementIdentity::anonymous();
    let task_selector = task.selector();
    let process_selector = process.selector();
    let lane_selector = lane.selector();
    document.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        id: process,
        name: None,
        documentation: String::new().into(),
        nodes: vec![Node {
            flow_metadata: None,
            base_extras: None,
            id: task,
            name: None,
            documentation: String::new().into(),
            kind: NodeKind::Task,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            item_aware: None,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![Lane {
            base: None,
            id: lane,
            name: Some(String::new()),
            parent: None,
            members: vec![task_selector.clone()],
            child_lane_set_id: None,
            child_lane_set_base: None,
        }],
        flows: vec![],
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
    });
    let document_id = document.id;
    let mut project = ProjectData::new("Queries".into());
    project.documents.push(Arc::new(document));
    let comment = Comment {
        id: CommentId::new(),
        target: ElementKey {
            document_id,
            xml_id: task_selector.clone(),
        },
        author: "Analyst".into(),
        text: "Review anonymous task".into(),
    };
    project.comments.push(comment.clone());
    project.validate().unwrap();
    let before = project.content_identity().unwrap();
    let identities = queries::identified_elements(&project, document_id).unwrap();
    assert!(
        identities
            .iter()
            .any(|row| row.selector == task_selector && row.xml_id.is_none())
    );
    let document = &project.documents[0];
    assert!(
        queries::find_node(document, &task_selector)
            .unwrap()
            .name
            .is_none()
    );
    assert!(
        queries::element_identity(document, &task_selector)
            .unwrap()
            .xml_id()
            .is_none()
    );
    assert!(!queries::comment_is_orphan(&project, &comment));
    let rows = queries::activity_table(&project, document_id, &process_selector).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.xml_id, task_selector);
    assert!(rows[0].name.is_none());
    assert_eq!(rows[0].lane.as_ref(), Some(&lane_selector));
    assert_eq!(document.processes[0].lanes[0].name.as_deref(), Some(""));
    assert_eq!(project.content_identity().unwrap(), before);

    let checkpoint_id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id: checkpoint_id,
        name: "Anonymous task".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let node = &mut Arc::make_mut(&mut project.documents[0]).processes[0].nodes[0];
    let ElementIdentity::Native { xml_id, .. } = &mut node.id else {
        unreachable!()
    };
    *xml_id = Some("NamedTask".parse().unwrap());
    let comparison = queries::compare_checkpoint(&project, checkpoint_id).unwrap();
    assert!(comparison.added_elements.is_empty());
    assert!(comparison.removed_elements.is_empty());
    assert_eq!(comparison.changed_properties.len(), 1);
    assert_eq!(comparison.changed_properties[0].key.xml_id, task_selector);
    assert!(
        comparison.changed_properties[0]
            .properties
            .iter()
            .any(|difference| {
                difference.property == "id.xml_id" && difference.after == "NamedTask"
            })
    );
    assert!(!queries::comment_is_orphan(&project, &comment));

    let identities = queries::identified_elements(&project, document_id).unwrap();
    assert!(identities.iter().any(|row| {
        row.selector == task_selector
            && row
                .xml_id
                .as_ref()
                .is_some_and(|id| id.to_string() == "NamedTask")
    }));
}
