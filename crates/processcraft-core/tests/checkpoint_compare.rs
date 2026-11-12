use processcraft_core::{ids::CheckpointId, model::*, queries};
use std::sync::Arc;

#[test]
fn diagram_details_report_order_and_metadata_without_fabricating_semantic_changes() {
    use processcraft_core::{
        bpmn_xml::{NamespaceBinding, XmlAttribute},
        ids::ElementIdentity,
    };
    use serde_json::json;
    let mut doc = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    doc.processes.push(serde_json::from_value(json!({
        "id":"Process", "name":null, "documentation":"", "lanes":[], "flows":[],
        "nodes":[{"id":"Task","kind":"Task","name":"Review","documentation":"","owner":"","contact":""}]
    })).unwrap());
    let first = ElementIdentity::anonymous();
    let second = ElementIdentity::anonymous();
    doc.diagrams.push(serde_json::from_value(json!({
        "id":"Diagram", "plane_id":"Plane", "plane":"Process", "edges":[],
        "plane_order":[first.selector(),second.selector()],
        "shapes":[
            {"id":first,"element":"Task","bounds":{"x":0.0,"y":0.0,"width":100.0,"height":70.0},"label":null,"style":null},
            {"id":second,"element":"Task","bounds":{"x":200.0,"y":0.0,"width":100.0,"height":70.0},"label":null,"style":null}
        ]
    })).unwrap());
    doc.diagrams[0].plane_metadata = Some(DiElementMetadata {
        namespaces: vec![NamespaceBinding {
            prefix: "v".into(),
            uri: "urn:vendor".into(),
        }],
        foreign_attributes: vec![XmlAttribute {
            namespace: "urn:vendor".into(),
            local: "layer".parse().unwrap(),
            prefix: Some("v".parse().unwrap()),
            value: "front".into(),
        }],
        extension: None,
    });
    let mut project = ProjectData::new("Diagram comparison".into());
    project.documents.push(Arc::new(doc));
    let checkpoint = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id: checkpoint,
        name: "Before".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    project.validate().unwrap();
    let diagram = &mut Arc::make_mut(&mut project.documents[0]).diagrams[0];
    diagram.plane_order.as_mut().unwrap().swap(0, 1);
    diagram.plane_metadata.as_mut().unwrap().foreign_attributes[0].value = "rear".into();
    diagram.shapes[0].bounds.x = 20.0;
    let before_query = project.content_identity().unwrap();
    let compared = queries::compare_checkpoint(&project, checkpoint).unwrap();
    assert!(compared.added_elements.is_empty());
    assert!(compared.removed_elements.is_empty());
    assert!(compared.changed_properties.is_empty());
    assert_eq!(compared.changed_diagrams.len(), 1);
    assert_eq!(compared.moved_shapes.len(), 1);
    let fields = &compared.changed_diagrams[0].properties;
    assert!(fields.iter().any(|field| field.property
        == "/plane_metadata/foreign_attributes/0/value"
        && field.before == "front"
        && field.after == "rear"));
    assert!(
        fields
            .iter()
            .any(|field| field.property == "/shapes/0/bounds/x"
                && field.before == 0.0
                && field.after == 20.0)
    );
    assert!(
        fields
            .iter()
            .any(|field| field.property == "/plane_order/0/native_id")
    );
    let mut offset = 0;
    let mut seen = false;
    loop {
        let page = queries::checkpoint_fields_page(&project, checkpoint, offset, 5)
            .unwrap()
            .unwrap();
        assert!(page.items.len() <= 5);
        seen |= page
            .items
            .iter()
            .any(|field| field.value == "/plane_metadata/foreign_attributes/0/value");
        if let Some(next) = page.next_offset {
            assert!(next > offset);
            offset = next;
        } else {
            break;
        }
    }
    assert!(seen);
    assert_eq!(project.content_identity().unwrap(), before_query);
    assert!(
        project.documents[0].diagrams[0]
            .shapes
            .iter()
            .all(|shape| shape.id.xml_id().is_none())
    );
}

#[test]
fn comparison_keeps_native_element_identity_while_reporting_xml_id_changes() {
    use processcraft_core::ids::{ElementIdentity, ElementSelector, NativeElementId};
    let native_id = NativeElementId::new();
    let mut project = ProjectData::new("Identity comparison".into());
    let mut document = BpmnDocument::new(
        "Anonymous definitions".into(),
        ElementIdentity::Native {
            native_id,
            xml_id: None,
        },
    );
    document.definitions = Some(DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        target_namespace: String::new(),
        imports: vec![],
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
    });
    let document_id = document.id;
    project.documents.push(Arc::new(document));
    let checkpoint_id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id: checkpoint_id,
        name: "Before assignment".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let before = project.content_identity().unwrap();
    Arc::make_mut(&mut project.documents[0]).definitions_id = ElementIdentity::Native {
        native_id,
        xml_id: Some("AssignedDefinitions".parse().unwrap()),
    };
    assert_ne!(project.content_identity().unwrap(), before);
    let comparison = queries::compare_checkpoint(&project, checkpoint_id).unwrap();
    assert!(comparison.added_elements.is_empty());
    assert!(comparison.removed_elements.is_empty());
    assert_eq!(comparison.changed_document_properties, vec![document_id]);
    assert_eq!(comparison.changed_properties.len(), 1);
    assert_eq!(
        comparison.changed_properties[0].key.xml_id,
        ElementSelector::Native { native_id }
    );
    assert!(
        comparison.changed_properties[0]
            .properties
            .iter()
            .any(|difference| {
                difference.property == "id.xml_id"
                    && difference.before.is_null()
                    && difference.after == "AssignedDefinitions"
            })
    );
    let before_query = project.content_identity().unwrap();
    let expected = serde_json::to_value(&comparison).unwrap();
    let fields = queries::checkpoint_fields(&project, checkpoint_id).unwrap();
    assert!(fields.len() > 10);
    for field in fields {
        assert_eq!(expected.pointer(&field.path), Some(&field.value));
    }
    assert_eq!(project.content_identity().unwrap(), before_query);
    assert!(
        project.checkpoints[0].content.documents[0]
            .definitions_id
            .xml_id()
            .is_none()
    );
}

#[test]
fn comparison_reports_definitions_values_without_inventing_semantic_elements() {
    let mut project = ProjectData::new("Metadata".into());
    let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    document.definitions = Some(DefinitionsMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        imports: vec![],
        target_namespace: "urn:original".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: Some("First tool".into()),
        exporter_version: None,
    });
    let document_id = document.id;
    project.documents.push(Arc::new(document));
    let id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id,
        name: "Before".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let metadata = Arc::make_mut(&mut project.documents[0])
        .definitions
        .as_mut()
        .unwrap();
    metadata.exporter = Some("Second tool".into());
    metadata.expression_language = Some("http://www.w3.org/1999/XPath".into());
    let diff = queries::compare_checkpoint(&project, id).unwrap();
    assert_eq!(diff.changed_document_properties, vec![document_id]);
    assert!(diff.added_elements.is_empty());
    assert!(diff.removed_elements.is_empty());
    assert!(diff.renamed_elements.is_empty());
    assert_eq!(diff.changed_properties.len(), 1);
    let change = &diff.changed_properties[0];
    assert_eq!(change.key.xml_id.to_string(), "Definitions");
    assert!(
        change
            .properties
            .iter()
            .any(|p| p.property == "definitions.exporter"
                && p.before == "First tool"
                && p.after == "Second tool")
    );
    assert!(
        change
            .properties
            .iter()
            .any(|p| p.property == "definitions.expression_language"
                && p.before.is_null()
                && p.after == "http://www.w3.org/1999/XPath")
    );
    assert_eq!(
        project.checkpoints[0].content.documents[0]
            .definitions
            .as_ref()
            .unwrap()
            .exporter
            .as_deref(),
        Some("First tool")
    );
}

#[test]
fn comparison_separates_rename_properties_geometry_and_frozen_context() {
    let mut project = ProjectData::new("Analysis".into());
    let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    document.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        id: "Process".parse().unwrap(),
        name: Some("Process".into()),
        documentation: String::new().into(),
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            id: "Task".parse().unwrap(),
            name: Some("Before".into()),
            documentation: String::new().into(),
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
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: "Diagram".parse().unwrap(),
        plane_id: "Plane".parse().unwrap(),
        plane: "Process".parse().unwrap(),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
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
    document.processes[0].nodes[0].name = Some("After".into());
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
        project.checkpoints[0].content.documents[0].processes[0].nodes[0]
            .name
            .as_deref(),
        Some("Before")
    );
}

#[test]
fn comparison_reports_unidentified_root_metadata_without_fabricating_elements() {
    let mut project = ProjectData::new("Anonymous root metadata".into());
    let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    document.extensions.push(DefinitionsExtension {
        documentation: vec![],
        definition: None,
        must_understand: None,
    });
    document.relationships.push(DefinitionsRelationship {
        id: None,
        base: None,
        relationship_type: "trace".into(),
        direction: None,
        sources: vec![],
        targets: vec![],
    });
    let document_id = document.id;
    project.documents.push(Arc::new(document));
    let checkpoint_id = CheckpointId::new();
    project.checkpoints.push(Checkpoint {
        id: checkpoint_id,
        name: "Before".into(),
        created_unix_ms: 1,
        content: FrozenProject::capture(&project),
    });
    let document = Arc::make_mut(&mut project.documents[0]);
    document.extensions[0].must_understand = Some(false);
    document.relationships[0].relationship_type = "explain".into();
    let before_query = project.content_identity().unwrap();
    let comparison = queries::compare_checkpoint(&project, checkpoint_id).unwrap();
    assert!(comparison.added_elements.is_empty());
    assert!(comparison.removed_elements.is_empty());
    assert_eq!(comparison.changed_document_properties, vec![document_id]);
    let root = comparison
        .changed_properties
        .iter()
        .find(|change| {
            change.key.xml_id
                == "Definitions"
                    .parse::<processcraft_core::ids::ElementSelector>()
                    .unwrap()
        })
        .unwrap();
    assert!(
        root.properties
            .iter()
            .any(|property| property.property == "content.extensions")
    );
    assert!(
        root.properties
            .iter()
            .any(|property| property.property == "content.relationships")
    );
    assert_eq!(project.content_identity().unwrap(), before_query);
    assert!(project.documents[0].relationships[0].id.is_none());
}
