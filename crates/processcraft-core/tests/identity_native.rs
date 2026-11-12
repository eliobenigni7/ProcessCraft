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
            message_ref: None,
            flow_metadata: None,
            base_extras: None,
            id: flow_id,
            source: Some(a.selector().into()),
            target: Some(b.selector().into()),
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
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        label_styles: vec![],
        resolution: None,
        name: None,
        id: native(),
        plane_id: native(),
        plane: process_id.selector(),
        shapes: vec![Shape {
            choreography_activity_shape: None,
            participant_band_kind: None,
            di_metadata: None,
            is_message_visible: None,
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
    let mut ordered = project.clone();
    let diagram = &mut Arc::make_mut(&mut ordered.documents[0]).diagrams[0];
    let shape = diagram.shapes[0].id.selector();
    diagram.plane_order = Some(vec![shape.clone()]);
    assert!(ordered.documents[0].has_di_choreography_features());
    ordered.validate().unwrap();
    Arc::make_mut(&mut ordered.documents[0]).diagrams[0].plane_order =
        Some(vec![shape.clone(), shape]);
    assert!(
        ordered
            .validate()
            .unwrap_err()
            .contains("Plane order must contain each representation exactly once")
    );
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
        kind: None,
        extras: None,
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
        plane_metadata: None,
        plane_order: None,
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

#[test]
fn each_di_representation_field_sets_capability() {
    let mut document = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    let diagram = || Diagram {
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_metadata: None,
        plane_order: None,
        id: "diagram".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "process".parse().unwrap(),
        shapes: vec![Shape {
            id: "shape".parse().unwrap(),
            element: "node".parse().unwrap(),
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            di_metadata: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            label: Some(Label::default()),
            label_visibility: None,
            style: None,
        }],
        edges: vec![Edge {
            id: "edge".parse().unwrap(),
            element: "flow".parse().unwrap(),
            source_element: None,
            target_element: None,
            waypoints: vec![],
            label: Some(Label::default()),
            label_visibility: None,
            style: None,
            di_metadata: None,
            message_visible_kind: None,
        }],
    };
    document.diagrams.push(diagram());
    assert!(!document.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].plane_order = Some(vec![]);
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].plane_metadata = Some(DiElementMetadata::default());
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].shapes[0].is_message_visible = Some(false);
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].shapes[0].choreography_activity_shape =
        Some(DiReference::Local("shape".parse().unwrap()));
    assert!(changed.has_di_choreography_features());
    assert_eq!(
        serde_json::to_value(ParticipantBandKind::TopNonInitiating).unwrap(),
        "top_non_initiating"
    );
    assert_eq!(
        serde_json::to_value(MessageVisibleKind::NonInitiating).unwrap(),
        "non_initiating"
    );
    for band in [
        ParticipantBandKind::TopInitiating,
        ParticipantBandKind::MiddleInitiating,
        ParticipantBandKind::BottomInitiating,
        ParticipantBandKind::TopNonInitiating,
        ParticipantBandKind::MiddleNonInitiating,
        ParticipantBandKind::BottomNonInitiating,
    ] {
        let mut changed = document.clone();
        changed.diagrams[0].shapes[0].participant_band_kind = Some(band);
        assert!(changed.has_di_choreography_features());
    }
    for kind in [
        MessageVisibleKind::Initiating,
        MessageVisibleKind::NonInitiating,
    ] {
        let mut changed = document.clone();
        changed.diagrams[0].edges[0].message_visible_kind = Some(kind);
        assert!(changed.has_di_choreography_features());
    }
    let mut changed = document.clone();
    changed.diagrams[0].shapes[0].di_metadata = Some(DiElementMetadata::default());
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].edges[0].di_metadata = Some(DiElementMetadata::default());
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].shapes[0]
        .label
        .as_mut()
        .unwrap()
        .di_metadata = Some(DiElementMetadata::default());
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.diagrams[0].edges[0]
        .label
        .as_mut()
        .unwrap()
        .di_metadata = Some(DiElementMetadata::default());
    assert!(changed.has_di_choreography_features());
    document.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        id: "collab".parse().unwrap(),
        name: None,
        base: None,
        correlation_keys: vec![],
        participants: vec![Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: "participant".parse().unwrap(),
            name: None,
            process: None,
            base: None,
            multiplicity: None,
        }],
        flows: vec![Flow {
            id: "flow".parse().unwrap(),
            kind: FlowKind::Message,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "a".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "b".parse().unwrap(),
            )),
            name: None,
            documentation: "".into(),
            base_extras: None,
            flow_metadata: None,
            message_ref: None,
            condition: None,
            association_direction: None,
        }],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    let mut changed = document.clone();
    changed.collaboration.as_mut().unwrap().participants[0].multiplicity =
        Some(ParticipantMultiplicity::default());
    assert!(changed.has_di_choreography_features());
    let mut changed = document.clone();
    changed.collaboration.as_mut().unwrap().flows[0].message_ref = Some(
        processcraft_core::bpmn_activity::ModelReference::Local("message".parse().unwrap()),
    );
    assert!(changed.has_di_choreography_features());
}

#[test]
fn di_foreign_payload_is_opaque_but_empty_extension_is_not() {
    use processcraft_core::bpmn_xml::{NamespaceBinding, XmlContent, XmlElement, XmlNode};
    let mut document = BpmnDocument::new("d".into(), "defs".parse().unwrap());
    document.diagrams.push(Diagram {
        id: "diagram".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "process".parse().unwrap(),
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_order: None,
        shapes: vec![],
        edges: vec![],
        plane_metadata: Some(DiElementMetadata {
            namespaces: vec![],
            foreign_attributes: vec![],
            extension: Some(XmlContent {
                namespaces: vec![],
                nodes: vec![XmlNode::Comment {
                    text: "safe".into(),
                }],
            }),
        }),
    });
    assert!(!document.has_opaque_xml());
    document.diagrams[0]
        .plane_metadata
        .as_mut()
        .unwrap()
        .extension = Some(XmlContent {
        namespaces: vec![NamespaceBinding {
            prefix: "v".into(),
            uri: "urn:vendor".into(),
        }],
        nodes: vec![XmlNode::Element {
            element: XmlElement {
                namespace: "urn:vendor".into(),
                local: "child".parse().unwrap(),
                prefix: Some("v".parse().unwrap()),
                namespaces: vec![],
                attributes: vec![],
                children: vec![],
            },
        }],
    });
    assert!(document.has_opaque_xml());
}
