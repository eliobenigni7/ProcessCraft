use processcraft_core::{
    ids::{ElementIdentity, ElementSelector, NativeElementId, XmlId},
    model::*,
};
use std::sync::Arc;
fn xid(s: &str) -> XmlId {
    s.parse().unwrap()
}
fn native() -> ElementIdentity {
    ElementIdentity::Native {
        native_id: NativeElementId::new(),
        xml_id: None,
    }
}
#[test]
fn native_graph_identity_is_stable_and_duplicate_xml_ids_are_rejected() {
    let process_id = native();
    let a = native();
    let b = native();
    let flow_id = native();
    let node = |id| Node {
        flow_metadata: None,
        base_extras: None,
        id,
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
    };
    let mut document = BpmnDocument::new("doc".into(), native());
    document.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        lane_set_base: None,
        is_executable: Some(false),
        id: process_id.clone(),
        name: None,
        documentation: String::new().into(),
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        nodes: vec![node(a.clone()), node(b.clone())],
        lanes: vec![],
        flows: vec![Flow {
            flow_metadata: None,
            base_extras: None,
            id: flow_id,
            source: a.selector(),
            target: b.selector(),
            name: None,
            kind: FlowKind::Sequence,
            documentation: String::new().into(),
            condition: None,
            association_direction: None,
        }],
    });
    let mut executable = document.processes[0].clone();
    let legacy = serde_json::to_value(&executable).unwrap();
    assert!(legacy.get("is_executable").is_none());
    executable.is_executable = None;
    assert!(serde_json::to_value(&executable).unwrap()["is_executable"].is_null());
    document.diagrams.push(Diagram {
        documentation: None,
        label_styles: vec![],
        resolution: None,
        name: None,
        id: native(),
        plane_id: native(),
        plane: process_id.selector(),
        shapes: vec![Shape {
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            id: native(),
            element: a.selector(),
            is_expanded: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 50.0,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(document));
    project.validate().unwrap();
    let before = project.content_identity().unwrap();
    let mut duplicate = project.clone();
    let doc = Arc::make_mut(&mut duplicate.documents[0]);
    doc.processes[0].nodes[0].id = ElementIdentity::Native {
        native_id: match a.selector() {
            ElementSelector::Native { native_id } => native_id,
            _ => unreachable!(),
        },
        xml_id: Some(xid("duplicate")),
    };
    doc.processes[0].nodes[1].id = ElementIdentity::Native {
        native_id: match b.selector() {
            ElementSelector::Native { native_id } => native_id,
            _ => unreachable!(),
        },
        xml_id: Some(xid("duplicate")),
    };
    assert!(duplicate.validate().is_err());
    assert_eq!(project.content_identity().unwrap(), before);
}

#[test]
fn empty_definitions_namespace_is_valid_model_data() {
    let mut doc = BpmnDocument::new("doc".into(), xid("defs").into());
    doc.definitions = Some(DefinitionsMetadata {
        foreign_attributes: vec![],
        namespaces: vec![],
        target_namespace: String::new(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
    });
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(doc));
    project.validate().unwrap();
}

#[test]
fn new_diagram_and_collaboration_names_require_native_capability() {
    let mut doc = BpmnDocument::new("doc".into(), xid("defs").into());
    doc.collaboration = Some(Collaboration {
        correlation_keys: vec![],
        base: None,
        id: xid("collab").into(),
        name: None,
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    doc.diagrams.push(Diagram {
        documentation: None,
        label_styles: vec![],
        resolution: None,
        id: xid("diagram").into(),
        name: None,
        plane_id: xid("plane").into(),
        plane: xid("collab").into(),
        shapes: vec![],
        edges: vec![],
    });
    assert!(!doc.has_native_identity_features());
    doc.diagrams[0].name = Some(String::new());
    assert!(doc.has_native_identity_features());
    doc.diagrams[0].name = None;
    doc.collaboration.as_mut().unwrap().name = Some("Conversation".into());
    assert!(doc.has_native_identity_features());
}
