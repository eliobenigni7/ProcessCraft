use processcraft_core::model::*;

fn sel(s: &str) -> processcraft_core::ids::ElementSelector {
    s.parse().unwrap()
}
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn diagram() -> Diagram {
    Diagram {
        plane_metadata: None,
        plane_order: Some(vec![id("shape"), id("edge")]),
        name: Some("Main".into()),
        documentation: Some("Docs".into()),
        resolution: Some(96.0),
        label_styles: vec![LabelStyle {
            id: Some(id("style")),
            font: Some(DiFont {
                name: Some("Arial".into()),
                size: Some(10.0),
                is_bold: Some(true),
                ..Default::default()
            }),
        }],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: Some(true),
            participant_band_kind: Some(ParticipantBandKind::BottomInitiating),
            choreography_activity_shape: Some(DiReference::Local(id("owner"))),
            id: id("shape"),
            element: id("task"),
            is_expanded: Some(true),
            is_horizontal: Some(false),
            is_marker_visible: Some(true),
            bounds: Bounds {
                x: 1.5,
                y: -2.0,
                width: 30.0,
                height: 40.25,
            },
            label: Some(Label {
                bounds: Some(Bounds {
                    x: 0.0,
                    y: 1.0,
                    width: 2.0,
                    height: 3.0,
                }),
                style_ref: Some(DiStyleRef::Local(id("style"))),
                ..Default::default()
            }),
            label_visibility: None,
            style: None,
        }],
        edges: vec![Edge {
            di_metadata: None,
            message_visible_kind: Some(MessageVisibleKind::NonInitiating),
            id: id("edge"),
            element: id("flow"),
            source_element: Some(id("shape")),
            target_element: Some(id("shape")),
            waypoints: vec![Point { x: 1.0, y: 2.0 }, Point { x: 3.5, y: 4.5 }],
            label: None,
            label_visibility: None,
            style: None,
        }],
    }
}

#[test]
fn di_fields_are_typed_and_survive_serde() {
    let d = diagram();
    let back: Diagram = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&d).unwrap()
    );
    assert_eq!(
        (
            back.name.as_deref(),
            back.documentation.as_deref(),
            back.resolution
        ),
        (Some("Main"), Some("Docs"), Some(96.0))
    );
    assert_eq!(back.plane, sel("process"));
    assert_eq!(back.plane_order.as_ref().unwrap().len(), 2);
    let s = &back.shapes[0];
    assert_eq!(s.element, sel("task"));
    assert_eq!(
        (
            s.is_expanded,
            s.is_horizontal,
            s.is_marker_visible,
            s.is_message_visible
        ),
        (Some(true), Some(false), Some(true), Some(true))
    );
    assert_eq!(
        s.participant_band_kind,
        Some(ParticipantBandKind::BottomInitiating)
    );
    assert_eq!(
        (s.bounds.x, s.bounds.y, s.bounds.width, s.bounds.height),
        (1.5, -2.0, 30.0, 40.25)
    );
    assert_eq!(
        s.label.as_ref().unwrap().style_ref,
        Some(DiStyleRef::Local(id("style")))
    );
    let e = &back.edges[0];
    assert_eq!(e.element, sel("flow"));
    assert_eq!(
        e.message_visible_kind,
        Some(MessageVisibleKind::NonInitiating)
    );
    assert_eq!((e.waypoints[1].x, e.waypoints[1].y), (3.5, 4.5));
}

#[test]
fn unknown_di_field_is_rejected_by_the_model() {
    let mut v = serde_json::to_value(diagram()).unwrap();
    v["shapes"][0]["bogus"] = serde_json::json!(1);
    assert!(serde_json::from_value::<Diagram>(v).is_err());
}
