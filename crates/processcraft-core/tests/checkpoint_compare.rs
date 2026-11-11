use processcraft_core::{ids::CheckpointId, model::*, queries};
use std::sync::Arc;

#[test]
fn comparison_separates_rename_properties_geometry_and_frozen_context() {
    let mut project = ProjectData::new("Analysis".into());
    let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    document.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        id: "Process".parse().unwrap(),
        name: "Process".into(),
        documentation: String::new(),
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            activity: None,
            id: "Task".parse().unwrap(),
            name: "Before".into(),
            documentation: String::new(),
            kind: NodeKind::Task,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    document.diagrams.push(Diagram {
        id: "Diagram".parse().unwrap(),
        plane_id: "Plane".parse().unwrap(),
        plane: "Process".parse().unwrap(),
        shapes: vec![Shape {
            id: "Shape".parse().unwrap(),
            element: "Task".parse().unwrap(),
            is_expanded: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 70.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    project.documents.push(Arc::new(document));
    let id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id,
        name: "Baseline".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let initial = queries::compare_checkpoint(&project, id).unwrap();
    assert!(initial.changed_documents.is_empty());
    let document = Arc::make_mut(&mut project.documents[0]);
    document.processes[0].nodes[0].name = "After".into();
    let renamed = queries::compare_checkpoint(&project, id).unwrap();
    assert_eq!(renamed.renamed_elements.len(), 1);
    assert_eq!(renamed.renamed_elements[0].key.xml_id.to_string(), "Task");
    assert!(renamed.changed_properties.is_empty());
    assert!(renamed.changed_diagrams.is_empty());
    let document = Arc::make_mut(&mut project.documents[0]);
    document.processes[0].nodes[0].documentation = "Changed".into();
    document.diagrams[0].shapes[0].bounds.x = 20.0;
    project.name = "Renamed project".into();
    let diff = queries::compare_checkpoint(&project, id).unwrap();
    assert_eq!(diff.changed_properties.len(), 1);
    assert!(
        diff.changed_properties[0]
            .properties
            .iter()
            .any(|p| p.property == "documentation" && p.before == "" && p.after == "Changed")
    );
    assert_eq!(diff.changed_diagrams.len(), 1);
    assert_eq!(diff.moved_shapes.len(), 1);
    assert!(diff.rerouted_edges.is_empty());
    assert!(diff.changed_document_properties.is_empty());
    assert!(diff.project_properties_changed);
    assert!(
        !diff.dictionary_changed
            && !diff.resources_changed
            && !diff.comments_changed
            && !diff.links_changed
    );
    project.links.push(NavigationLink {
        source: processcraft_core::ids::ElementKey {
            document_id: project.documents[0].id,
            xml_id: "Task".parse().unwrap(),
        },
        target: project.documents[0].id,
        label: "Open context".into(),
    });
    project.dictionary_links.push(DictionaryLink {
        source: processcraft_core::ids::ElementKey {
            document_id: project.documents[0].id,
            xml_id: "Task".parse().unwrap(),
        },
        kind: DictionaryKind::System,
        target: DictionaryTarget::Literal("ERP".into()),
    });
    let linked = queries::compare_checkpoint(&project, id).unwrap();
    assert_eq!(linked.added_links.len(), 1);
    assert!(linked.removed_links.is_empty());
    assert_eq!(linked.added_dictionary_links.len(), 1);
    assert!(linked.removed_dictionary_links.is_empty());
    assert_eq!(
        project.checkpoints[0].content.documents[0].processes[0].nodes[0].name,
        "Before"
    );
}
