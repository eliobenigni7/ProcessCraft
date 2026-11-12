use processcraft_core::{
    ids::{DocumentId, ElementKey, XmlId},
    model::*,
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn sample() -> ProjectData {
    let mut p = ProjectData::new("Project".into());
    let mut d = BpmnDocument::new("Document".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_base: None,
        lane_set_name: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: id("task"),
            name: Some("Task".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    p.documents.push(d.into());
    p
}
#[test]
fn ids_are_scoped_to_document() {
    let a = DocumentId::new();
    let b = DocumentId::new();
    assert_ne!(
        ElementKey {
            document_id: a,
            xml_id: id("task")
        },
        ElementKey {
            document_id: b,
            xml_id: id("task")
        }
    );
    let mut p = sample();
    let mut d = (*p.documents[0]).clone();
    d.id = b;
    p.documents.push(d.into());
    assert!(p.validate().is_ok());
}
#[test]
fn lane_membership_is_not_a_duplicate_node() {
    let mut p = sample();
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0]
        .lanes
        .push(Lane {
            base: None,
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            child_lane_set_id: None,
            id: id("lane"),
            name: Some(String::new()),
            parent: None,
            members: vec![id("task")],
        });
    assert!(p.validate().is_ok());
}
#[test]
fn multiple_diagrams_share_semantics() {
    let mut p = sample();
    let shape = Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id("shape"),
        element: id("task"),
        bounds: Bounds {
            x: 0.,
            y: 0.,
            width: 20.,
            height: 10.,
        },
        label: None,
        style: None,
    };
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram"),
            plane_id: id("diagram_plane"),
            plane: id("process"),
            shapes: vec![shape.clone()],
            edges: vec![],
        });
    let mut shape2 = shape;
    shape2.id = id("shape2");
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram2"),
            plane_id: id("diagram2_plane"),
            plane: id("process"),
            shapes: vec![shape2],
            edges: vec![],
        });
    assert!(p.validate().is_ok());
}
#[test]
fn invalid_reference_is_rejected() {
    let mut p = sample();
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0]
        .lanes
        .push(Lane {
            base: None,
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            child_lane_set_id: None,
            id: id("lane"),
            name: Some(String::new()),
            parent: None,
            members: vec![id("missing")],
        });
    assert!(p.validate().is_err());
}
#[test]
fn persistent_identity_covers_every_field() {
    let p = sample();
    let mut q = p.clone();
    std::sync::Arc::make_mut(&mut q.documents[0]).processes[0].nodes[0].documentation =
        "changed".into();
    assert_ne!(p.content_identity().unwrap(), q.content_identity().unwrap());
}
#[test]
fn invalid_ids_cannot_be_deserialized() {
    assert!(serde_json::from_str::<XmlId>("\"bad:id\"").is_err());
    assert!(
        serde_json::from_str::<DocumentId>("\"00000000-0000-0000-0000-000000000000\"").is_err()
    );
}
#[test]
fn geometry_must_be_finite_and_negative_zero_is_canonical() {
    let mut p = sample();
    let shape = Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: id("s"),
        element: id("task"),
        bounds: Bounds {
            x: -0.,
            y: 0.,
            width: 1.,
            height: 1.,
        },
        label: None,
        style: None,
    };
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("d"),
            plane_id: id("d_plane"),
            plane: id("process"),
            shapes: vec![shape],
            edges: vec![],
        });
    let mut q = p.clone();
    std::sync::Arc::make_mut(&mut q.documents[0]).diagrams[0].shapes[0]
        .bounds
        .x = 0.;
    assert_eq!(p.content_identity().unwrap(), q.content_identity().unwrap());
    std::sync::Arc::make_mut(&mut q.documents[0]).diagrams[0].shapes[0]
        .bounds
        .x = f64::NAN;
    assert!(q.validate().is_err());
}
#[test]
fn incomplete_process_and_cycles_in_sequence_are_valid() {
    let mut p = sample();
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0]
        .flows
        .push(Flow {
            message_ref: None,
            flow_metadata: None,
            base_extras: None,
            association_direction: None,
            id: id("loop"),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "task",
            ))),
            target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "task",
            ))),
            name: Some(String::new()),
            documentation: String::new().into(),
            condition: None,
        });
    assert!(p.validate().is_ok());
}
#[test]
fn preserved_source_requires_resource_and_disables_structural_edits() {
    let mut p = sample();
    std::sync::Arc::make_mut(&mut p.documents[0]).capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    assert!(
        !std::sync::Arc::make_mut(&mut p.documents[0])
            .capability
            .allows_structural_editing()
    );
    assert!(p.validate().is_err());
    p.resources.push(Resource::new(
        id("source"),
        "application/xml".into(),
        vec![1].into(),
    ));
    assert!(p.validate().is_ok());
}
#[test]
fn endpoint_plane_and_resource_boundaries() {
    let mut p = sample();
    let n = &mut std::sync::Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0];
    n.kind = NodeKind::EndEvent;
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0]
        .flows
        .push(Flow {
            message_ref: None,
            flow_metadata: None,
            base_extras: None,
            association_direction: None,
            id: id("flow"),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "task",
            ))),
            target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "task",
            ))),
            name: Some(String::new()),
            documentation: String::new().into(),
            condition: None,
        });
    assert!(p.validate().is_err());
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0].kind = NodeKind::Task;
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0].flows[0].kind =
        FlowKind::Association;
    assert!(p.validate().is_err());
    std::sync::Arc::make_mut(&mut p.documents[0]).processes[0]
        .flows
        .clear();
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram"),
            plane_id: id("diagram_plane"),
            plane: id("missing"),
            shapes: vec![],
            edges: vec![],
        });
    assert!(p.validate().is_err());
    std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].plane = id("process");
    p.resources.push(Resource::new(
        id("resource"),
        "application/xml".into(),
        vec![1].into(),
    ));
    p.resources[0].bytes = vec![2].into();
    assert!(p.validate().is_err());
}
#[test]
fn identity_changes_for_persistent_categories() {
    let mut p = sample();
    p.folders.push(Folder {
        id: id("folder"),
        name: "Folder".into(),
        parent: None,
    });
    std::sync::Arc::make_mut(&mut p.documents[0]).folder = Some(id("folder"));
    p.resources.push(Resource::new(
        id("source"),
        "application/xml".into(),
        vec![1].into(),
    ));
    std::sync::Arc::make_mut(&mut p.documents[0]).provenance = Some(Provenance {
        source_resource_id: id("source"),
        source_name: "original".into(),
    });
    std::sync::Arc::make_mut(&mut p.documents[0]).capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![RetainedExtension {
            owner: id("task"),
            position: 0,
            namespaces: vec![("p".into(), "urn:x".into())],
            content: "extension".into(),
        }],
    };
    p.links.push(NavigationLink {
        source: ElementKey {
            document_id: std::sync::Arc::make_mut(&mut p.documents[0]).id,
            xml_id: id("task"),
        },
        target: std::sync::Arc::make_mut(&mut p.documents[0]).id,
        label: "link".into(),
    });
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram"),
            plane_id: id("diagram_plane"),
            plane: id("process"),
            shapes: vec![Shape {
                di_metadata: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                label_visibility: None,
                is_horizontal: None,
                is_marker_visible: None,
                is_expanded: None,
                id: id("shape"),
                element: id("task"),
                bounds: Bounds {
                    x: 1.,
                    y: 2.,
                    width: 3.,
                    height: 4.,
                },
                label: Some(Label {
                    di_metadata: None,
                    id: None,
                    style_ref: None,
                    bounds: Some(Bounds {
                        x: 1.,
                        y: 2.,
                        width: 3.,
                        height: 4.,
                    }),
                    style: None,
                }),
                style: Some(Style {
                    fill: Some("red".into()),
                    stroke: None,
                    font: None,
                }),
            }],
            edges: vec![],
        });
    let baseline = p.content_identity().unwrap();
    let mutations: &[fn(&mut ProjectData)] = &[
        |p| p.name.push('x'),
        |p| p.description.push('x'),
        |p| p.folders[0].name.push('x'),
        |p| std::sync::Arc::make_mut(&mut p.documents[0]).name.push('x'),
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0]
                .owner
                .push('x')
        },
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0]).processes[0].nodes[0]
                .contact
                .push('x')
        },
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0])
                .provenance
                .as_mut()
                .unwrap()
                .source_name
                .push('x')
        },
        |p| {
            if let Capability::RetainedExtensions { extensions, .. } =
                &mut std::sync::Arc::make_mut(&mut p.documents[0]).capability
            {
                extensions[0].content.push('x');
            }
        },
        |p| p.resources[0] = Resource::new(id("source"), "application/xml".into(), vec![2].into()),
        |p| p.resources[0].media_type.push('x'),
        |p| p.links[0].label.push('x'),
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].shapes[0]
                .bounds
                .x += 1.
        },
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].shapes[0]
                .label
                .as_mut()
                .unwrap()
                .bounds
                .as_mut()
                .unwrap()
                .x += 1.
        },
        |p| {
            std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].shapes[0]
                .style
                .as_mut()
                .unwrap()
                .fill = Some("blue".into())
        },
    ];
    for (i, mutate) in mutations.iter().enumerate() {
        let mut q = p.clone();
        mutate(&mut q);
        assert_ne!(baseline, q.content_identity().unwrap(), "mutation {i}");
    }
}
#[test]
fn nested_unknown_fields_are_rejected() {
    let mut v = serde_json::to_value(sample()).unwrap();
    v["documents"][0]["processes"][0]["nodes"][0]["future_field"] = serde_json::json!("future");
    assert!(serde_json::from_value::<ProjectData>(v).is_err());
}

#[test]
fn plane_identity_is_unique_and_persistent() {
    let mut p = sample();
    std::sync::Arc::make_mut(&mut p.documents[0])
        .diagrams
        .push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram"),
            plane_id: id("plane"),
            plane: id("process"),
            shapes: vec![],
            edges: vec![],
        });
    let original = p.content_identity().unwrap();
    std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].plane_id = id("other_plane");
    assert_ne!(original, p.content_identity().unwrap());
    std::sync::Arc::make_mut(&mut p.documents[0]).diagrams[0].plane_id = id("task");
    assert!(p.validate().is_err());
}
#[test]
fn deep_folder_chain_and_cycle_are_checked_iteratively() {
    let mut p = ProjectData::new("deep".into());
    for i in 0..10_000 {
        p.folders.push(Folder {
            id: id(&format!("f{i}")),
            name: String::new(),
            parent: if i == 0 {
                None
            } else {
                Some(id(&format!("f{}", i - 1)))
            },
        });
    }
    assert!(p.validate().is_ok());
    p.folders[0].parent = Some(id("f9999"));
    assert!(p.validate().is_err());
}

#[test]
fn collaboration_and_diagram_names_preserve_omission_and_empty_values() {
    let mut project = sample();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    doc.collaboration = Some(Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: id("collaboration"),
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    });
    doc.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![],
        edges: vec![],
    });
    let omitted = serde_json::to_value(&project).unwrap();
    assert!(
        omitted["documents"][0]["collaboration"]
            .get("name")
            .is_none()
    );
    assert!(omitted["documents"][0]["diagrams"][0].get("name").is_none());
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    doc.collaboration.as_mut().unwrap().name = Some(String::new());
    doc.diagrams[0].name = Some(String::new());
    let explicit = serde_json::to_value(&project).unwrap();
    assert_eq!(explicit["documents"][0]["collaboration"]["name"], "");
    assert_eq!(explicit["documents"][0]["diagrams"][0]["name"], "");
    let restored: ProjectData = serde_json::from_value(explicit).unwrap();
    assert_eq!(
        restored.documents[0]
            .collaboration
            .as_ref()
            .unwrap()
            .name
            .as_deref(),
        Some("")
    );
    assert_eq!(restored.documents[0].diagrams[0].name.as_deref(), Some(""));
}
