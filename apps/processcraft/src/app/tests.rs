//! Unit tests for the app shell: drafts, transitions, selection and panel behavior.
use super::*;

#[test]
fn untouched_projects_are_pristine_and_file_names_are_safe() {
    let mut project = controller::blank_project();
    assert!(is_pristine_project(&project));
    project.name = "Renamed".into();
    assert!(!is_pristine_project(&project));
    assert_eq!(file_stem("Order: Q1/Q2?"), "Order_ Q1_Q2_");
    assert_eq!(file_stem("  ..."), "project");
    let app = ProcessCraft::default();
    assert!(!app.ctl.session.snapshot().dirty);
    assert!(!app.has_drafts());
}
#[test]
fn pool_creation_uses_the_selected_collaboration_plane() {
    let mut app = ProcessCraft::default();
    let document_id = app.ctl.session.snapshot().project.documents[0].id;
    let root = |id: &str| Collaboration {
        id: id.parse().unwrap(),
        name: None,
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let diagram = Diagram {
        id: "SecondDiagram".parse().unwrap(),
        plane_id: "SecondPlane".parse().unwrap(),
        plane: "SecondRoot".parse().unwrap(),
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_metadata: None,
        plane_order: None,
        shapes: vec![],
        edges: vec![],
    };
    assert!(app.ctl.mutate(vec![
        Mutation::SetCollaborationRoot {
            document_id,
            collaboration: root("FirstRoot")
        },
        Mutation::SetCollaborationRoot {
            document_id,
            collaboration: root("SecondRoot")
        },
        Mutation::CreateDiagram {
            document_id,
            diagram
        }
    ]));
    app.diagram_id = Some("SecondDiagram".parse().unwrap());
    app.add_pool();
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    assert!(
        document
            .collaboration
            .as_ref()
            .unwrap()
            .participants
            .is_empty()
    );
    assert_eq!(document.additional_collaborations[0].participants.len(), 1);
    assert_eq!(
        document
            .diagrams
            .iter()
            .find(|diagram| diagram.id == "SecondDiagram".parse::<ElementSelector>().unwrap())
            .unwrap()
            .plane,
        "SecondRoot".parse::<ElementSelector>().unwrap()
    );
}

#[test]
fn graph_gateway_properties_keep_metadata_and_representation_through_history() {
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyFlowElement, ChoreographyGraph,
    };
    use processcraft_core::bpmn_model::{GatewayDirection, NodeDetails};
    let mut app = ProcessCraft::default();
    let document_id = app.ctl.session.snapshot().project.documents[0].id;
    let node = Node {
        id: "GraphGateway".parse().unwrap(),
        name: None,
        documentation: "Original".to_owned().into(),
        base_extras: None,
        kind: NodeKind::ExclusiveGateway,
        details: Some(NodeDetails::Gateway {
            default_flow: None,
            direction: None,
            instantiate: None,
            event_gateway_type: None,
            activation_condition: None,
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        flow_metadata: None,
        item_aware: None,
        owner: String::new(),
        contact: String::new(),
    };
    let root = Choreography {
        id: "GraphRoot".parse().unwrap(),
        kind: None,
        graph: Some(ChoreographyGraph {
            flow_elements: vec![ChoreographyFlowElement::Gateway(Box::new(node))],
            ..Default::default()
        }),
        name: None,
        extras: None,
        base: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    };
    let shape = |id: &str| Shape {
        id: id.parse().unwrap(),
        element: "GraphGateway".parse().unwrap(),
        bounds: Bounds {
            x: 10.0,
            y: 10.0,
            width: 60.0,
            height: 60.0,
        },
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        label: None,
        label_visibility: None,
        style: None,
    };
    let diagram = Diagram {
        id: "GraphDiagram".parse().unwrap(),
        plane_id: "GraphPlane".parse().unwrap(),
        plane: "GraphRoot".parse().unwrap(),
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_metadata: None,
        plane_order: None,
        shapes: vec![shape("FirstView"), shape("SecondView")],
        edges: vec![],
    };
    assert!(app.ctl.mutate(vec![
        Mutation::CreateChoreography {
            document_id,
            choreography: root
        },
        Mutation::CreateDiagram {
            document_id,
            diagram
        }
    ]));
    app.diagram_id = Some("GraphDiagram".parse().unwrap());
    app.select(Some("GraphGateway".parse().unwrap()));
    app.representation_diagram = app.diagram_id.clone();
    app.representation_selection = vec!["SecondView".parse().unwrap()];
    app.draft_name_present = true;
    app.draft = "Decision".into();
    if let Some(NodeDetails::Gateway { direction, .. }) = &mut app.bpmn_details {
        *direction = Some(GatewayDirection::Diverging);
    } else {
        panic!("gateway details");
    }
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let scope = processcraft_core::queries::choreography_scopes(&snapshot.project.documents[0])
        .into_iter()
        .find(|scope| scope.owner == "GraphRoot".parse::<ElementSelector>().unwrap())
        .unwrap();
    assert_eq!(scope.gateways[0].name.as_deref(), Some("Decision"));
    assert_eq!(
        scope.gateways[0].documentation.plain_text(),
        Some("Original")
    );
    assert!(matches!(
        &scope.gateways[0].details,
        Some(NodeDetails::Gateway {
            direction: Some(GatewayDirection::Diverging),
            ..
        })
    ));
    app.history(Request::Undo);
    assert_eq!(
        app.representation_selection,
        vec!["SecondView".parse::<ElementSelector>().unwrap()]
    );
    assert_eq!(app.draft, "");
    app.history(Request::Redo);
    assert_eq!(app.draft, "Decision");
    assert_eq!(
        app.representation_selection,
        vec!["SecondView".parse::<ElementSelector>().unwrap()]
    );
    app.draft = "Stale edit".into();
    app.draft_dirty = true;
    let project = app.ctl.session.snapshot().project.clone();
    assert!(app.ctl.mutate(vec![Mutation::ProjectProperties {
        name: "Changed externally".into(),
        description: project.description.clone()
    }]));
    assert!(!app.commit_draft());
    let snapshot = app.ctl.session.snapshot();
    let scope = processcraft_core::queries::choreography_scopes(&snapshot.project.documents[0])
        .into_iter()
        .find(|scope| scope.owner == "GraphRoot".parse::<ElementSelector>().unwrap())
        .unwrap();
    assert_eq!(scope.gateways[0].name.as_deref(), Some("Decision"));
}

#[test]
fn collaboration_property_selection_preserves_optional_names_and_metadata() {
    use processcraft_core::bpmn_collaboration::{
        CollaborationArtifact, CollaborationExtras, EndPoint, PartnerEntity, PartnerRole,
    };
    use processcraft_core::bpmn_metadata::{BaseMetadata, Documentation};
    let mut app = ProcessCraft::default();
    let document_id = app.ctl.session.snapshot().project.documents[0].id;
    let metadata = Some(BaseMetadata {
        documentation: vec![Documentation {
            id: None,
            text_format: Some("text/plain".into()),
            content: "Keep this".into(),
        }],
        ..Default::default()
    });
    let entity_id = processcraft_core::ids::ElementIdentity::anonymous();
    let role_id = processcraft_core::ids::ElementIdentity::anonymous();
    let endpoint_id = processcraft_core::ids::ElementIdentity::anonymous();
    let group_id = processcraft_core::ids::ElementIdentity::anonymous();
    let root = Collaboration {
        id: "Root".parse().unwrap(),
        name: Some("Collaboration name".into()),
        kind: None,
        extras: Some(CollaborationExtras {
            artifacts: vec![CollaborationArtifact::Group(
                processcraft_core::bpmn_model::Group {
                    id: group_id.clone(),
                    base: None,
                    category_value_ref: None,
                },
            )],
            ..Default::default()
        }),
        base: metadata.clone(),
        correlation_keys: vec![],
        participants: vec![],
        flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    let choreography = processcraft_core::bpmn_collaboration::Choreography {
        kind: None,
        graph: None,
        id: "Choreography".parse().unwrap(),
        name: Some(String::new()),
        base: metadata.clone(),
        extras: None,
        correlation_keys: vec![],
        participants: vec![],
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
        conversation_nodes: vec![],
        conversation_links: vec![],
    };
    assert!(app.ctl.mutate(vec![
        Mutation::SetCollaborationRoot {
            document_id,
            collaboration: root
        },
        Mutation::CreateChoreography {
            document_id,
            choreography
        },
        Mutation::SetPartnerCatalog {
            document_id,
            partner_entities: vec![PartnerEntity {
                id: Some(entity_id.clone()),
                base: metadata.clone(),
                name: Some("Customers".into()),
                participant_refs: vec![]
            }],
            partner_roles: vec![PartnerRole {
                id: Some(role_id.clone()),
                base: metadata.clone(),
                name: None,
                participant_refs: vec![]
            }],
            endpoints: vec![EndPoint {
                id: Some(endpoint_id.clone()),
                base: metadata.clone()
            }]
        }
    ]));
    let context = egui::Context::default();
    for (selector, expected, present) in [
        (entity_id.selector(), "Customers", true),
        (role_id.selector(), "", false),
        ("Root".parse().unwrap(), "Collaboration name", true),
        ("Choreography".parse().unwrap(), "", true),
    ] {
        app.select(Some(selector));
        assert_eq!(app.draft, expected);
        assert_eq!(app.draft_name_present, present);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| app.inspector_ui(ui, true));
        output.textures_delta.clear();
        assert_eq!(app.draft, expected);
        assert_eq!(app.draft_name_present, present);
    }
    app.select(Some(entity_id.selector()));
    app.draft = "Clients".into();
    app.draft_dirty = true;
    assert!(app.commit_draft());
    let snapshot = app.ctl.session.snapshot();
    assert_eq!(
        snapshot.project.documents[0].partner_entities[0]
            .name
            .as_deref(),
        Some("Clients")
    );
    assert_eq!(
        snapshot.project.documents[0].partner_entities[0].base,
        metadata
    );
    app.history(Request::Undo);
    assert_eq!(
        app.ctl.session.snapshot().project.documents[0].partner_entities[0]
            .name
            .as_deref(),
        Some("Customers")
    );
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    assert_eq!(
        inspector::selection_kind(document, &entity_id.selector()),
        Some("Partner entity")
    );
    assert_eq!(
        inspector::selection_kind(document, &role_id.selector()),
        Some("Partner role")
    );
    assert_eq!(
        inspector::selection_kind(document, &endpoint_id.selector()),
        Some("Endpoint")
    );
    assert_eq!(
        inspector::selection_kind(document, &group_id.selector()),
        Some("Group")
    );
    assert_eq!(
        inspector::selection_kind(document, &"Root".parse().unwrap()),
        Some("Collaboration")
    );
    assert_eq!(
        inspector::selection_kind(document, &"Choreography".parse().unwrap()),
        Some("Choreography")
    );
    assert!(!inspector::selection_has_name(
        document,
        &endpoint_id.selector()
    ));
    assert!(!inspector::selection_has_name(
        document,
        &group_id.selector()
    ));
    let mut with_association = document.as_ref().clone();
    with_association
        .collaboration
        .as_mut()
        .unwrap()
        .extras
        .as_mut()
        .unwrap()
        .artifacts
        .push(CollaborationArtifact::Association(Box::new(Flow {
            id: "Association".parse().unwrap(),
            kind: FlowKind::Association,
            source: None,
            target: None,
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
            association_direction: None,
            flow_metadata: None,
            message_ref: None,
        })));
    let association: ElementSelector = "Association".parse().unwrap();
    assert!(!inspector::selection_has_name(
        &with_association,
        &association
    ));
    assert_eq!(
        inspector::selection_kind(&with_association, &association),
        Some("Association")
    );
    app.select(Some(endpoint_id.selector()));
    let mut output = context.run_ui(egui::RawInput::default(), |ui| app.inspector_ui(ui, true));
    output.textures_delta.clear();
    assert!(!app.draft_name_present);
}

#[test]
fn clicking_choreography_band_preserves_participant_name_through_di_history() {
    use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
    let mut app = ProcessCraft::default();
    let document_id = app.ctl.session.snapshot().project.documents[0].id;
    let participant = |id: &str, name: Option<&str>| Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        id: id.parse().unwrap(),
        name: name.map(str::to_owned),
        base: None,
        process: None,
        multiplicity: None,
    };
    let choreography = Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        id: "Choreography".parse().unwrap(),
        name: None,
        base: None,
        participants: vec![
            participant("Buyer", Some("Buyer")),
            participant("Seller", None),
        ],
        message_flows: vec![Flow {
            id: "Message".parse().unwrap(),
            kind: FlowKind::Message,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Buyer".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Seller".parse().unwrap(),
            )),
            name: Some(String::new()),
            documentation: "Original documentation".into(),
            base_extras: None,
            flow_metadata: None,
            condition: None,
            association_direction: None,
            message_ref: None,
        }],
        activities: vec![ChoreographyActivity {
            loop_type: None,
            details: None,
            id: "Task".parse().unwrap(),
            name: Some("Order".into()),
            base: None,
            flow_metadata: None,
            correlation_keys: vec![],
            participant_refs: vec![
                processcraft_core::bpmn_activity::ModelReference::Local("Buyer".parse().unwrap()),
                processcraft_core::bpmn_activity::ModelReference::Local("Seller".parse().unwrap()),
            ],
            initiating_participant: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Buyer".parse().unwrap(),
            )),
            message_flow_refs: vec![processcraft_core::bpmn_activity::ModelReference::Local(
                "Message".parse().unwrap(),
            )],
        }],
        sequence_flows: vec![],
    };
    let shape = |id: &str,
                 element: &str,
                 bounds: Bounds,
                 band: Option<ParticipantBandKind>,
                 owner: Option<DiReference>| Shape {
        id: id.parse().unwrap(),
        element: element.parse().unwrap(),
        bounds,
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: band,
        choreography_activity_shape: owner,
        is_expanded: None,
        is_horizontal: None,
        is_marker_visible: None,
        label: Some(Label::default()),
        label_visibility: Some(DiLabelVisibility::Explicit),
        style: None,
    };
    let diagram = Diagram {
        id: "ChoreographyDiagram".parse().unwrap(),
        plane_id: "ChoreographyPlane".parse().unwrap(),
        plane: "Choreography".parse().unwrap(),
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        plane_metadata: None,
        plane_order: Some(vec![
            "ActivityShape".parse().unwrap(),
            "BuyerBand".parse().unwrap(),
            "OtherShape".parse().unwrap(),
            "OtherBand".parse().unwrap(),
        ]),
        shapes: vec![
            shape(
                "ActivityShape",
                "Task",
                Bounds {
                    x: 50.,
                    y: 50.,
                    width: 160.,
                    height: 100.,
                },
                None,
                None,
            ),
            shape(
                "BuyerBand",
                "Buyer",
                Bounds {
                    x: 50.,
                    y: 50.,
                    width: 160.,
                    height: 30.,
                },
                Some(ParticipantBandKind::TopInitiating),
                Some(DiReference::Local("ActivityShape".parse().unwrap())),
            ),
            shape(
                "OtherShape",
                "Task",
                Bounds {
                    x: 350.,
                    y: 50.,
                    width: 160.,
                    height: 100.,
                },
                None,
                None,
            ),
            shape(
                "OtherBand",
                "Seller",
                Bounds {
                    x: 350.,
                    y: 50.,
                    width: 160.,
                    height: 30.,
                },
                Some(ParticipantBandKind::TopNonInitiating),
                Some(DiReference::Local("OtherShape".parse().unwrap())),
            ),
        ],
        edges: vec![],
    };
    assert!(app.ctl.mutate(vec![
        Mutation::CreateChoreography {
            document_id,
            choreography
        },
        Mutation::CreateDiagram {
            document_id,
            diagram
        }
    ]));
    app.diagram_id = Some("ChoreographyDiagram".parse().unwrap());
    let context = egui::Context::default();
    let frame = |app: &mut ProcessCraft, events| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| app.canvas_ui(ui),
        );
        output.textures_delta.clear();
    };
    for _ in 0..300 {
        frame(&mut app, vec![]);
        if app
            .hits
            .iter()
            .any(|h| h.diagram_element_id == "BuyerBand".parse::<ElementSelector>().unwrap())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let hit = app
        .hits
        .iter()
        .rev()
        .find(|h| h.contains(130., 65., 0.))
        .expect("band hit");
    assert_eq!(
        hit.diagram_element_id,
        "BuyerBand".parse::<ElementSelector>().unwrap()
    );
    let canvas = context
        .data(|data| data.get_temp::<Rect>(egui::Id::unique("test_canvas_rect")))
        .unwrap();
    let point = Pos2::new(
        canvas.left() + 12. + ((130. - app.camera.0) as f32 * app.zoom),
        canvas.top() + 12. + ((65. - app.camera.1) as f32 * app.zoom),
    );
    let button = |pressed| egui::Event::PointerButton {
        pos: point,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut app,
        vec![egui::Event::PointerMoved(point), button(true)],
    );
    frame(&mut app, vec![button(false)]);
    assert_eq!(app.selected, Some("Buyer".parse().unwrap()));
    assert!(app.draft_name_present);
    assert_eq!(app.draft, "Buyer");
    // Dragging one of two representations of the same task must not move the other.
    let owner_point = Pos2::new(
        canvas.left() + 12. + ((130. - app.camera.0) as f32 * app.zoom),
        canvas.top() + 12. + ((115. - app.camera.1) as f32 * app.zoom),
    );
    let moved = owner_point + Vec2::new(30., 0.);
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut app,
        vec![
            egui::Event::PointerMoved(owner_point),
            pointer(owner_point, true),
        ],
    );
    frame(&mut app, vec![egui::Event::PointerMoved(moved)]);
    frame(&mut app, vec![pointer(moved, false)]);
    let bounds = |app: &ProcessCraft, id: &str| {
        app.ctl.session.snapshot().project.documents[0].diagrams[1]
            .shapes
            .iter()
            .find(|shape| shape.id == id.parse::<ElementSelector>().unwrap())
            .unwrap()
            .bounds
            .clone()
    };
    assert!(bounds(&app, "ActivityShape").x > 50.);
    assert_eq!(bounds(&app, "OtherShape").x, 350.);
    assert_eq!(bounds(&app, "OtherBand").x, 350.);
    app.history(Request::Undo);
    assert_eq!(bounds(&app, "ActivityShape").x, 50.);
    app.history(Request::Redo);
    assert!(bounds(&app, "ActivityShape").x > 50.);
    app.history(Request::Undo);
    // A concurrent model edit cancels a gesture instead of applying stale bounds.
    for _ in 0..300 {
        frame(&mut app, vec![]);
        if app
            .hits
            .iter()
            .any(|h| h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    frame(
        &mut app,
        vec![
            egui::Event::PointerMoved(owner_point),
            pointer(owner_point, true),
        ],
    );
    frame(&mut app, vec![egui::Event::PointerMoved(moved)]);
    assert!(app.drag.is_some());
    assert!(app.ctl.mutate(vec![Mutation::Rename {
        document_id,
        element_id: "Task".parse().unwrap(),
        name: Some("Updated order".into())
    }]));
    frame(&mut app, vec![pointer(moved, false)]);
    assert_eq!(bounds(&app, "ActivityShape").x, 50.);
    assert!(app.drag.is_none());
    app.history(Request::Undo);
    assert!(app.ctl.mutate(vec![Mutation::SetLabelBounds {
        document_id,
        diagram_id: "ChoreographyDiagram".parse().unwrap(),
        representation_id: "ActivityShape".parse().unwrap(),
        bounds: Bounds {
            x: 100.,
            y: 100.,
            width: 60.,
            height: 30.
        },
    }]));
    for _ in 0..300 {
        frame(&mut app, vec![]);
        if app.hits.iter().any(|h| {
            h.is_label
                && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
        }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let alt = egui::Modifiers {
        alt: true,
        ..egui::Modifiers::NONE
    };
    let frame_alt = |app: &mut ProcessCraft, events| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events: std::iter::once(egui::Event::ModifiersChanged(alt))
                    .chain(events)
                    .collect(),
                ..Default::default()
            },
            |ui| app.canvas_ui(ui),
        );
        output.textures_delta.clear();
    };
    let alt_pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: alt,
    };
    frame_alt(
        &mut app,
        vec![
            egui::Event::PointerMoved(owner_point),
            alt_pointer(owner_point, true),
        ],
    );
    frame_alt(&mut app, vec![egui::Event::PointerMoved(moved)]);
    frame_alt(&mut app, vec![alt_pointer(moved, false)]);
    assert_eq!(bounds(&app, "ActivityShape").x, 50.);
    let label_x = |app: &ProcessCraft| {
        app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .x
    };
    assert!(label_x(&app) > 100.);
    app.history(Request::Undo);
    assert_eq!(label_x(&app), 100.);
    app.history(Request::Redo);
    assert!(label_x(&app) > 100.);
    app.history(Request::Undo);
    app.history(Request::Undo);
    for _ in 0..300 {
        frame(
            &mut app,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
        if let Some(hit) = app.hits.iter().find(|h| {
            h.is_label
                && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
        }) && matches!(&hit.geometry, processcraft_render::HitGeometry::Shape(b) if b.width != 60.)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let fallback = match &app
        .hits
        .iter()
        .find(|h| {
            h.is_label
                && h.diagram_element_id == "ActivityShape".parse::<ElementSelector>().unwrap()
        })
        .unwrap()
        .geometry
    {
        processcraft_render::HitGeometry::Shape(bounds) => bounds.clone(),
        _ => panic!("expected label bounds"),
    };
    frame_alt(
        &mut app,
        vec![
            egui::Event::PointerMoved(owner_point),
            alt_pointer(owner_point, true),
        ],
    );
    frame_alt(&mut app, vec![egui::Event::PointerMoved(moved)]);
    frame_alt(&mut app, vec![alt_pointer(moved, false)]);
    let actual = app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
        .label
        .as_ref()
        .unwrap()
        .bounds
        .clone()
        .unwrap();
    assert!((actual.x - fallback.x - 30. / app.zoom as f64).abs() < 0.001);
    assert_eq!(actual.width, fallback.width);
    app.history(Request::Undo);
    assert!(
        app.ctl.session.snapshot().project.documents[0].diagrams[1].shapes[0]
            .label
            .as_ref()
            .unwrap()
            .bounds
            .is_none()
    );
    app.select(Some("Buyer".parse().unwrap()));
    assert!(
        app.ctl
            .mutate(vec![Mutation::SetShapeRepresentationMetadata {
                document_id,
                diagram_id: "ChoreographyDiagram".parse().unwrap(),
                shape_id: "BuyerBand".parse().unwrap(),
                metadata: Some(DiElementMetadata::default()),
                is_message_visible: None,
                participant_band_kind: Some(ParticipantBandKind::TopInitiating),
                choreography_activity_shape: Some(DiReference::Local(
                    "ActivityShape".parse().unwrap()
                ))
            }])
    );
    let buyer_name = |app: &ProcessCraft| {
        app.ctl.session.snapshot().project.documents[0].choreographies[0].participants[0]
            .name
            .clone()
    };
    assert_eq!(buyer_name(&app), Some("Buyer".into()));
    app.history(Request::Undo);
    assert_eq!(buyer_name(&app), Some("Buyer".into()));
    let snapshot = app.ctl.session.snapshot();
    let buyer = &snapshot.project.documents[0].choreographies[0].participants[0];
    assert_eq!(buyer.name.as_deref(), Some("Buyer"));
    assert!(app.remove_shape_representation(
        document_id,
        "ChoreographyDiagram".parse().unwrap(),
        "BuyerBand".parse().unwrap()
    ));
    assert_eq!(app.selected, Some("Buyer".parse().unwrap()));
    assert_eq!(buyer_name(&app), Some("Buyer".into()));
    let snapshot = app.ctl.session.snapshot();
    let diagram = &snapshot.project.documents[0].diagrams[1];
    assert!(
        !diagram
            .shapes
            .iter()
            .any(|shape| shape.id == "BuyerBand".parse::<ElementSelector>().unwrap())
    );
    assert!(
        diagram
            .shapes
            .iter()
            .any(|shape| shape.id == "OtherBand".parse::<ElementSelector>().unwrap())
    );
    app.history(Request::Undo);
    let snapshot = app.ctl.session.snapshot();
    let diagram = &snapshot.project.documents[0].diagrams[1];
    assert!(
        diagram
            .shapes
            .iter()
            .any(|shape| shape.id == "BuyerBand".parse::<ElementSelector>().unwrap())
    );
    assert!(
        diagram
            .shapes
            .iter()
            .any(|shape| shape.id == "OtherBand".parse::<ElementSelector>().unwrap())
    );
    assert_eq!(buyer_name(&app), Some("Buyer".into()));
    app.select(Some("Task".parse().unwrap()));
    assert!(app.draft_name_present);
    assert_eq!(app.draft, "Order");
    for name in [Some(String::new()), None] {
        assert!(app.ctl.mutate(vec![Mutation::Rename {
            document_id,
            element_id: "Buyer".parse().unwrap(),
            name: name.clone()
        }]));
        app.select(Some("Buyer".parse().unwrap()));
        assert_eq!(app.draft_name_present, name.is_some());
        assert_eq!(app.draft, name.clone().unwrap_or_default());
        assert_eq!(buyer_name(&app), name);
    }
    app.select(Some("Message".parse().unwrap()));
    assert!(app.is_flow);
    assert!(app.draft_name_present);
    assert!(app.draft.is_empty());
    assert_eq!(app.docs, "Original documentation");
    app.docs = "Updated documentation".into();
    app.docs_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let flow = &snapshot.project.documents[0].choreographies[0].message_flows[0];
    assert_eq!(flow.name.as_deref(), Some(""));
    assert_eq!(
        flow.documentation.plain_text(),
        Some("Updated documentation")
    );
}
#[test]
fn new_task_has_explicit_label_and_exports_in_memory() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    let task = app.selected.as_ref().expect("new task selected");
    let shape = document.diagrams[0]
        .shapes
        .iter()
        .find(|shape| &shape.element == task)
        .expect("task shape");
    assert!(matches!(
        shape.label_visibility,
        Some(DiLabelVisibility::Explicit)
    ));
    assert!(shape.label.is_some());
    let exported = processcraft_io::interchange::export_bpmn(&snapshot.project, document.id, true)
        .expect("new diagram exports");
    let xml = String::from_utf8(exported.bytes).unwrap();
    assert!(xml.contains("BPMNLabel"));
}

#[test]
fn discard_keeps_confirmation_and_draft_while_export_is_pending() {
    let mut app = ProcessCraft::default();
    let session = app.ctl.session.id();
    app.confirm = Some(PendingAction::New);
    app.draft_dirty = true;
    app.export_ready = Some((vec![1], vec![], "bpmn"));
    assert_eq!(app.discard_transition(), None);
    assert_eq!(app.confirm, Some(PendingAction::New));
    assert!(app.draft_dirty);
    app.execute(PendingAction::New);
    assert_eq!(app.ctl.session.id(), session);
    assert_eq!(app.confirm, Some(PendingAction::New));
    app.export_ready = None;
    assert_eq!(app.discard_transition(), Some(PendingAction::New));
}
#[test]
fn canceled_or_failed_open_preserves_uncommitted_drafts_and_selection() {
    let mut app = ProcessCraft {
        draft: "Uncommitted".into(),
        draft_dirty: true,
        dictionary_name: "Draft entry".into(),
        process_id: Some(controller::id("Process").into()),
        ..Default::default()
    };
    let process = app.process_id.clone();
    let session = app.ctl.session.id();
    app.confirm = Some(PendingAction::Open);
    assert_eq!(app.discard_transition(), Some(PendingAction::Open));
    app.open_selected_path(None);
    assert!(app.draft_dirty);
    assert_eq!(app.draft, "Uncommitted");
    assert_eq!(app.dictionary_name, "Draft entry");
    assert_eq!(app.process_id, process);
    assert_eq!(app.ctl.session.id(), session);
    let missing =
        std::env::temp_dir().join(format!("missing-{}.processcraft", uuid::Uuid::new_v4()));
    app.open_selected_path(Some(missing));
    assert!(app.draft_dirty);
    assert_eq!(app.draft, "Uncommitted");
    assert_eq!(app.dictionary_name, "Draft entry");
    assert_eq!(app.process_id, process);
    assert_eq!(app.ctl.session.id(), session);
}
#[test]
fn dictionary_history_and_external_update_do_not_create_phantom_drafts() {
    let mut app = ProcessCraft::default();
    let id = processcraft_core::ids::DictionaryEntryId::new();
    let mut entry = DictionaryEntry {
        id,
        kind: DictionaryKind::System,
        name: "ERP".into(),
        description: String::new(),
    };
    assert!(app.ctl.mutate(vec![Mutation::CreateDictionaryEntry {
        entry: entry.clone()
    }]));
    app.dictionary_selected = Some(id);
    app.refresh_dictionary_editor();
    app.dictionary_name = "ERP Cloud".into();
    assert!(app.commit_dictionary_draft());
    app.history(Request::Undo);
    assert_eq!(app.dictionary_name, "ERP");
    assert!(!app.has_drafts());
    let generation = app.ctl.session.snapshot().generation;
    assert!(app.commit_dictionary_draft());
    assert_eq!(app.ctl.session.snapshot().generation, generation);
    app.history(Request::Redo);
    assert_eq!(app.dictionary_name, "ERP Cloud");
    assert!(!app.has_drafts());
    entry.name = "Remote ERP".into();
    assert!(
        app.ctl
            .mutate(vec![Mutation::UpdateDictionaryEntry { entry }])
    );
    app.refresh_dictionary_editor();
    assert_eq!(app.dictionary_name, "Remote ERP");
    assert!(!app.has_drafts());
}
#[test]
fn recovery_respects_pending_export_and_dictionary_draft() {
    let mut app = ProcessCraft::default();
    assert!(app.can_restore_recovery());
    app.export_ready = Some((vec![1], vec![], "bpmn"));
    assert!(!app.can_restore_recovery());
    app.export_ready = None;
    app.dictionary_name = "Uncommitted entry".into();
    assert!(!app.can_restore_recovery());
    app.dictionary_name.clear();
    assert!(app.can_restore_recovery());
}
#[test]
fn saved_transition_waits_for_export_and_rechecks_drafts() {
    let mut app = ProcessCraft::default();
    let content = app.ctl.session.snapshot().content_id;
    app.ctl.session.mark_saved(content);
    app.after_save = Some(PendingAction::Close);
    app.after_save_ready = Some(true);
    app.export_ready = Some((vec![1], vec![], "bpmn"));
    assert!(app.take_ready_transition().is_none());
    assert_eq!(app.after_save, Some(PendingAction::Close));
    app.export_ready = None;
    app.dictionary_name = "New draft".into();
    assert_eq!(
        app.take_ready_transition(),
        Some((PendingAction::Close, false))
    );
    app.dictionary_name.clear();
    app.after_save = Some(PendingAction::New);
    app.after_save_ready = Some(true);
    assert!(app.ctl.mutate(vec![Mutation::RenameProject {
        name: "Edited after save".into()
    }]));
    assert_eq!(
        app.take_ready_transition(),
        Some((PendingAction::New, false))
    );
}
#[test]
fn widget_drag_commits_once_and_escape_commits_nothing() {
    let mut project = controller::blank_project();
    let doc = std::sync::Arc::make_mut(&mut project.documents[0]);
    let node: XmlId = "Node".parse().unwrap();
    let shape: XmlId = "Shape".parse().unwrap();
    doc.processes[0].nodes.push(Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        activity: None,
        io_specification: None,
        loop_characteristics: None,
        details: None,
        id: node.clone().into(),
        name: Some("Task".into()),
        documentation: String::new().into(),
        kind: NodeKind::Task,
        owner: String::new(),
        contact: String::new(),
    });
    doc.diagrams[0].shapes.push(Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: shape.clone().into(),
        element: node.into(),
        bounds: Bounds {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 70.0,
        },
        label: None,
        style: None,
    });
    let mut session = processcraft_core::commands::Session::new(project).unwrap();
    let ctx = egui::Context::default();
    let mut drag = None;
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut pass = |events: Vec<egui::Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                events,
                ..Default::default()
            },
            |ui| {
                let (_, response) =
                    ui.allocate_exact_size(Vec2::splat(300.0), Sense::click_and_drag());
                let snap = session.snapshot();
                let d = &snap.project.documents[0];
                let diagram = &d.diagrams[0];
                if response.drag_started() {
                    let items = diagram
                        .shapes
                        .iter()
                        .map(|s| (s.id.selector(), s.bounds.clone()))
                        .collect();
                    drag = begin_drag(&response, ui.ctx(), items);
                }
                advance_drag(&mut drag, ui.ctx());
                if let Some(done) = finish_drag(&response, &mut drag)
                    && done.offset.length() > 1.0
                {
                    let bounds = done
                        .items
                        .into_iter()
                        .map(|(shape_id, mut bounds)| {
                            bounds.x += done.offset.x as f64;
                            bounds.y += done.offset.y as f64;
                            processcraft_core::commands::ShapeBounds {
                                diagram_id: diagram.id.selector(),
                                shape_id,
                                bounds,
                            }
                        })
                        .collect();
                    let prepared = processcraft_core::geometry::prepare_move(
                        d,
                        &diagram.id.selector(),
                        bounds,
                    )
                    .unwrap();
                    session
                        .apply(
                            processcraft_core::commands::Envelope {
                                request_id: "drag".into(),
                                session_id: session.id(),
                                expected_generation: Some(snap.generation),
                            },
                            prepared.request,
                        )
                        .unwrap();
                }
            },
        );
        output.textures_delta.clear();
    };
    let start = Pos2::new(20.0, 20.0);
    let end = Pos2::new(70.0, 35.0);
    pass(vec![]);
    pass(vec![egui::Event::PointerMoved(start), button(start, true)]);
    pass(vec![egui::Event::PointerMoved(Pos2::new(40.0, 25.0))]);
    pass(vec![egui::Event::PointerMoved(end)]);
    pass(vec![button(end, false)]);

    pass(vec![egui::Event::PointerMoved(start), button(start, true)]);
    pass(vec![egui::Event::PointerMoved(end)]);
    pass(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    pass(vec![button(end, false)]);
    let snap = session.snapshot();
    assert_eq!(snap.undo_count, 1);
    assert_eq!(
        snap.project.documents[0].diagrams[0].shapes[0].bounds.x,
        60.0
    );
}
#[test]
fn fit_extents_include_external_label_and_edge() {
    let diagram = Diagram {
        plane_metadata: None,
        plane_order: None,
        documentation: None,
        label_styles: Vec::new(),
        resolution: None,
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
            is_expanded: None,
            id: "Shape".parse().unwrap(),
            element: "Node".parse().unwrap(),
            bounds: Bounds {
                x: 50.0,
                y: 30.0,
                width: 40.0,
                height: 40.0,
            },
            label: Some(Label {
                di_metadata: None,
                id: None,
                style_ref: None,
                bounds: Some(Bounds {
                    x: 0.0,
                    y: 74.0,
                    width: 140.0,
                    height: 30.0,
                }),
                style: None,
            }),
            style: None,
        }],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: None,
            label_visibility: None,
            source_element: None,
            target_element: None,
            id: "Edge".parse().unwrap(),
            element: "Flow".parse().unwrap(),
            waypoints: vec![Point { x: 90.0, y: 50.0 }, Point { x: 300.0, y: 50.0 }],
            label: None,
            style: None,
        }],
    };
    let (minx, _, maxx, maxy) = diagram_extents(&diagram).unwrap();
    assert_eq!(minx, 0.0);
    assert!(maxx >= 300.0);
    assert!(maxy >= 104.0);
}
#[test]
fn definitions_draft_is_guarded_and_uses_history() {
    let mut app = ProcessCraft::default();
    app.select(None);
    app.definitions_draft.name = Some(String::new());
    app.definitions_draft.target_namespace.clear();
    app.definitions_dirty = true;
    app.draft_dirty = true;
    assert!(!app.commit_draft());
    assert!(app.draft_dirty);
    assert!(
        app.ctl.session.snapshot().project.documents[0]
            .definitions
            .is_none()
    );
    app.definitions_draft.target_namespace = "urn:example".into();
    assert!(app.commit_draft(), "{}", app.ctl.error);
    assert_eq!(
        app.ctl.session.snapshot().project.documents[0]
            .definitions
            .as_ref()
            .unwrap()
            .name,
        Some(String::new())
    );
    app.history(Request::Undo);
    assert!(
        app.ctl.session.snapshot().project.documents[0]
            .definitions
            .is_none()
    );
    app.history(Request::Redo);
    assert_eq!(app.definitions_draft.target_namespace, "urn:example");
}
#[test]
fn definitions_import_drafts_add_edit_remove_and_undo() {
    use processcraft_core::model::DefinitionsImport;
    let mut app = ProcessCraft::default();
    app.select(None);
    app.definitions_draft.target_namespace = "urn:example".into();
    app.definitions_draft.imports.push(DefinitionsImport {
        namespace: "urn:other".into(),
        location: "other.bpmn".into(),
        import_type: "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
    });
    app.definitions_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.definitions_draft.imports[0].location = "renamed.bpmn".into();
    app.definitions_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.definitions_draft.imports.clear();
    app.definitions_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    assert!(app.definitions_draft.imports.is_empty());
    app.history(Request::Undo);
    assert_eq!(app.definitions_draft.imports[0].location, "renamed.bpmn");
    app.history(Request::Undo);
    assert_eq!(app.definitions_draft.imports[0].location, "other.bpmn");
    app.history(Request::Redo);
    assert_eq!(app.definitions_draft.imports[0].location, "renamed.bpmn");
    app.history(Request::Redo);
    assert!(app.definitions_draft.imports.is_empty());
}
#[test]
fn link_and_compensation_drafts_use_shared_history() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut app = ProcessCraft::default();
    app.add(NodeKind::IntermediateCatchEvent);
    let link = EventDefinition::Link {
        base: None,
        id: Some("LinkDefinition".parse().unwrap()),
        name: "handoff".into(),
        sources: Vec::new(),
        target: None,
    };
    app.bpmn_details = Some(NodeDetails::Event {
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
        definitions: vec![link.clone()],
        attached_to: None,
        cancel_activity: None,
    });
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.history(Request::Undo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { .. }))
    );
    app.history(Request::Redo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if definitions[0] == link)
    );
    app.add(NodeKind::IntermediateThrowEvent);
    let compensate = EventDefinition::Compensate {
        base: None,
        id: None,
        wait_for_completion: Some(false),
        activity_ref: None,
    };
    app.bpmn_details = Some(NodeDetails::Event {
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
        definitions: vec![compensate.clone()],
        attached_to: None,
        cancel_activity: None,
    });
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.history(Request::Undo);
    app.history(Request::Redo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if definitions[0] == compensate)
    );
}

#[test]
fn cancel_boundary_draft_uses_shared_history() {
    use processcraft_core::bpmn_model::{EventDefinition, NodeDetails};
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Transaction);
    let transaction = app.selected.clone().unwrap();
    app.add(NodeKind::BoundaryEvent);
    app.bpmn_details = Some(NodeDetails::Event {
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
        definitions: vec![EventDefinition::Cancel {
            base: None,
            id: None,
        }],
        attached_to: Some(processcraft_core::bpmn_activity::ModelReference::Local(
            transaction,
        )),
        cancel_activity: Some(true),
    });
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.history(Request::Undo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { .. }))
    );
    app.history(Request::Redo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Cancel { .. }))
    );
}

#[test]
fn activity_references_use_shared_history_with_local_catalog() {
    use processcraft_core::bpmn_catalog::{Interface, Operation};
    use processcraft_core::bpmn_model::{EventResource, QNameRef};
    let namespace = "urn:processcraft:generated";
    let message = QNameRef {
        namespace: namespace.into(),
        local: "Message".parse().unwrap(),
    };
    let operation = QNameRef {
        namespace: namespace.into(),
        local: "Operation".parse().unwrap(),
    };
    let mut project = crate::controller::blank_project();
    let document = std::sync::Arc::make_mut(&mut project.documents[0]);
    document.event_resources.push(EventResource::Message {
        base: None,
        item_ref: None,
        id: message.local.clone().into(),
        name: None,
    });
    document.catalog.interfaces.push(Interface {
        base: None,
        id: None,
        name: "Service".into(),
        implementation_ref: None,
        operations: vec![Operation {
            base: None,
            id: Some(operation.local.clone().into()),
            name: "Send".into(),
            implementation_ref: None,
            in_message_ref: message.clone(),
            out_message_ref: None,
            error_refs: Vec::new(),
        }],
    });
    let mut app = ProcessCraft::default();
    app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
    app.add(NodeKind::SendTask);
    let mut properties = app.bpmn_activity.clone().unwrap_or_default();
    properties.message_ref = Some(message);
    properties.operation_ref = Some(operation);
    app.bpmn_activity = Some(properties.clone());
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    app.history(Request::Undo);
    assert!(
        app.bpmn_activity
            .as_ref()
            .is_none_or(|value| value.message_ref.is_none())
    );
    app.history(Request::Redo);
    assert_eq!(app.bpmn_activity, Some(properties));
}

#[test]
fn process_and_diagram_name_drafts_preserve_absent_and_empty() {
    let mut app = ProcessCraft::default();
    app.select(None);
    app.process_name = None;
    app.diagram_name = Some(String::new());
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    assert!(document.processes[0].name.is_none());
    assert_eq!(document.diagrams[0].name, Some(String::new()));
    app.history(Request::Undo);
    let snapshot = app.ctl.session.snapshot();
    assert_eq!(
        snapshot.project.documents[0].processes[0].name.as_deref(),
        Some("Process")
    );
    assert!(snapshot.project.documents[0].diagrams[0].name.is_none());
    app.history(Request::Redo);
    let snapshot = app.ctl.session.snapshot();
    assert!(snapshot.project.documents[0].processes[0].name.is_none());
    assert_eq!(
        snapshot.project.documents[0].diagrams[0].name,
        Some(String::new())
    );
}

#[test]
fn process_and_diagram_context_follow_id_changes_and_history() {
    let mut app = ProcessCraft::default();
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    let document_id = document.id;
    let process = document.processes[0].id.selector();
    let diagram = document.diagrams[0].id.selector();
    app.process_id = Some(process.clone());
    app.diagram_id = Some(diagram.clone());
    let changes = app
        .ctl
        .mutate_changes(vec![Mutation::SetElementXmlId {
            document_id,
            element_id: process.clone(),
            xml_id: Some("RenamedProcess".parse().unwrap()),
        }])
        .expect("process ID change");
    app.remap_context(&changes.selector_mappings);
    let renamed = app.process_id.clone().unwrap();
    assert!(matches!(renamed, ElementSelector::Native { .. }));
    assert_ne!(renamed, process);
    assert_eq!(app.diagram_id, Some(diagram));
    app.history(Request::Undo);
    assert_eq!(app.process_id, Some(process));
    app.history(Request::Redo);
    assert_eq!(app.process_id, Some(renamed));
}

#[test]
fn anonymous_node_selection_name_and_xml_id_survive_history() {
    let mut app = ProcessCraft {
        create_anonymous: true,
        ..Default::default()
    };
    app.add(NodeKind::Task);
    let selected = app.selected.clone().expect("anonymous node selected");
    assert!(matches!(selected, ElementSelector::Native { .. }));
    let snapshot = app.ctl.session.snapshot();
    let document = &snapshot.project.documents[0];
    assert!(
        processcraft_core::queries::element_identity(document, &selected)
            .unwrap()
            .xml_id()
            .is_none()
    );
    app.draft_name_present = false;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    assert!(
        processcraft_core::queries::find_node(
            &app.ctl.session.snapshot().project.documents[0],
            &selected
        )
        .unwrap()
        .name
        .is_none()
    );
    let document_id = app.ctl.session.snapshot().project.documents[0].id;
    assert!(
        app.ctl.mutate(vec![Mutation::SetElementXmlId {
            document_id,
            element_id: selected.clone(),
            xml_id: Some("TaskId".parse().unwrap())
        }]),
        "{}",
        app.ctl.error
    );
    app.restore_selection();
    assert_eq!(app.selected, Some(selected.clone()));
    app.history(Request::Undo);
    assert_eq!(app.selected, Some(selected.clone()));
    assert!(
        processcraft_core::queries::element_identity(
            &app.ctl.session.snapshot().project.documents[0],
            &selected
        )
        .unwrap()
        .xml_id()
        .is_none()
    );
    app.history(Request::Redo);
    assert_eq!(
        processcraft_core::queries::element_identity(
            &app.ctl.session.snapshot().project.documents[0],
            &selected
        )
        .unwrap()
        .xml_id()
        .unwrap()
        .to_string(),
        "TaskId"
    );
}

#[test]
fn legacy_annotation_fallback_is_preserved_until_body_edit() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut project = controller::blank_project();
    let document = std::sync::Arc::make_mut(&mut project.documents[0]);
    document.processes[0].nodes.push(Node {
        flow_metadata: None,
        base_extras: None,
        id: "Note".parse().unwrap(),
        name: Some("Old body".into()),
        documentation: String::new().into(),
        kind: NodeKind::TextAnnotation,
        item_aware: None,
        details: Some(NodeDetails::TextAnnotation {
            text_format: None,
            text: None,
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    });
    let mut app = ProcessCraft::default();
    app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
    let selected: ElementSelector = "Note".parse().unwrap();
    app.select(Some(selected.clone()));
    assert_eq!(app.draft, "Old body");
    app.docs = "Description".into();
    app.docs_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let node =
        processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap();
    assert_eq!(node.name.as_deref(), Some("Old body"));
    assert!(matches!(
        &node.details,
        Some(NodeDetails::TextAnnotation { text: None, .. })
    ));
    app.draft = "New body".into();
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let node =
        processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap();
    assert!(node.name.is_none());
    assert!(
        matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("New body"))
    );
    app.history(Request::Undo);
    assert_eq!(app.draft, "Old body");
}

#[test]
fn annotation_body_draft_does_not_create_a_name() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut app = ProcessCraft::default();
    app.add(NodeKind::TextAnnotation);
    let selected = app.selected.clone().unwrap();
    app.draft = "A note".into();
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let node =
        processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap();
    assert!(node.name.is_none());
    assert!(
        matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
    );
    app.docs = "Description".into();
    app.docs_dirty = true;
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    assert!(
        matches!(&processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap().details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
    );
    app.history(Request::Undo);
    app.history(Request::Redo);
    let snapshot = app.ctl.session.snapshot();
    let node =
        processcraft_core::queries::find_node(&snapshot.project.documents[0], &selected).unwrap();
    assert!(node.name.is_none());
    assert!(
        matches!(&node.details, Some(NodeDetails::TextAnnotation { text: Some(text), .. }) if text.plain_text() == Some("A note"))
    );
}

#[test]
fn expression_text_draft_preserves_metadata_and_history() {
    use processcraft_core::bpmn_model::{
        EventDefinition, Expression, ExpressionData, ExpressionKind, NodeDetails, TimerKind,
    };
    let mut app = ProcessCraft::default();
    app.add(NodeKind::IntermediateCatchEvent);
    let expression = Expression::Structured(ExpressionData {
        id: Some("TimerExpression".parse().unwrap()),
        kind: ExpressionKind::Formal {
            explicit_type: true,
            language: Some("urn:language".into()),
            evaluates_to_type_ref: None,
        },
        text: "PT1M".into(),
        namespaces: vec![
            (
                String::new(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            ),
            (
                "xsi".into(),
                "http://www.w3.org/2001/XMLSchema-instance".into(),
            ),
            ("ex".into(), "urn:example".into()),
        ],
        foreign_attributes: Vec::new(),
    });
    app.bpmn_details = Some(NodeDetails::Event {
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
        definitions: vec![EventDefinition::Timer {
            base: None,
            id: None,
            kind: Some(TimerKind::Duration),
            expression: Some(expression.clone()),
        }],
        attached_to: None,
        cancel_activity: None,
    });
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    if let Some(NodeDetails::Event { definitions, .. }) = &mut app.bpmn_details
        && let EventDefinition::Timer { expression, .. } = &mut definitions[0]
    {
        expression
            .as_mut()
            .unwrap()
            .set_text("PT5M".into())
            .unwrap();
    }
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
        panic!("event missing")
    };
    let EventDefinition::Timer {
        expression: updated,
        ..
    } = &definitions[0]
    else {
        panic!("timer missing")
    };
    let mut expected = expression.clone();
    expected.set_text("PT5M".into()).unwrap();
    assert_eq!(updated.as_ref(), Some(&expected));
    app.history(Request::Undo);
    let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
        panic!("event missing")
    };
    assert!(
        matches!(&definitions[0], EventDefinition::Timer { expression: actual, .. } if actual.as_ref() == Some(&expression))
    );
    app.history(Request::Redo);
    let Some(NodeDetails::Event { definitions, .. }) = &app.bpmn_details else {
        panic!("event missing")
    };
    assert!(
        matches!(&definitions[0], EventDefinition::Timer { expression: actual, .. } if actual.as_ref() == Some(&expected))
    );
}
#[test]
fn nested_scope_and_event_properties_use_shared_history() {
    use processcraft_core::{
        bpmn_model::{EventDefinition, NodeDetails, TimerKind},
        queries,
    };
    let mut app = ProcessCraft::default();
    app.add(NodeKind::SubProcess);
    let parent = app.selected.clone().expect("subprocess created");
    app.process_id = Some(parent.clone());
    app.select(None);
    app.add(NodeKind::IntermediateCatchEvent);
    let child = app.selected.clone().expect("child created");
    assert_eq!(app.process_id.as_ref(), Some(&parent));
    app.bpmn_details = Some(NodeDetails::Event {
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
        definitions: vec![EventDefinition::Timer {
            base: None,
            id: None,
            kind: Some(TimerKind::Duration),
            expression: Some("PT5M".into()),
        }],
        attached_to: None,
        cancel_activity: None,
    });
    app.draft_dirty = true;
    assert!(app.commit_draft(), "{}", app.ctl.error);
    let snapshot = app.ctl.session.snapshot();
    let node = queries::find_node(&snapshot.project.documents[0], &child).unwrap();
    assert_eq!(node.details, app.bpmn_details);
    app.history(Request::Undo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression.as_ref().is_some_and(|value| value.plain_text() == Some("PT1M"))))
    );
    app.history(Request::Redo);
    assert!(
        matches!(&app.bpmn_details, Some(NodeDetails::Event { definitions, .. }) if matches!(&definitions[0], EventDefinition::Timer { expression, .. } if expression.as_ref().is_some_and(|value| value.plain_text() == Some("PT5M"))))
    );
}
