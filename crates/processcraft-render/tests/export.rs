use processcraft_core::model::{
    Bounds, BpmnDocument, Capability, Diagram, Node, NodeKind, Process, Shape,
};
use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine, export_diagram};
fn document() -> BpmnDocument {
    let mut d = BpmnDocument::new("test".into(), "defs".parse().unwrap());
    d.processes.push(Process {
        io_specification: None,
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: "p".parse().unwrap(),
        name: String::new(),
        documentation: String::new(),
        nodes: vec![Node {
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: "n".parse().unwrap(),
            name: "A & B".into(),
            documentation: String::new(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        id: "d".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "p".parse().unwrap(),
        shapes: vec![Shape {
            is_expanded: None,
            id: "s".parse().unwrap(),
            element: "n".parse().unwrap(),
            bounds: Bounds {
                x: -50.,
                y: -20.,
                width: 120.,
                height: 80.,
            },
            label: None,
            style: None,
        }],
        edges: vec![],
    });
    d
}
#[test]
fn exports_full_scene_and_exact_dimensions() {
    let d = document();
    let mut fonts = TextEngine::new();
    let id = "d".parse().unwrap();
    let opts = ExportOptions {
        format: ExportFormat::Svg,
        appearance: Appearance::Light,
        scale: 2.,
        acknowledge_diagnostics: false,
    };
    let svg = export_diagram(&d, &id, opts, &mut fonts, "Noto Sans").unwrap();
    assert_eq!(svg.origin, (-66., -36.));
    assert_eq!(svg.pixel_size, (304, 224));
    assert!(String::from_utf8(svg.bytes).unwrap().contains("A &amp; B"));
    let png = export_diagram(
        &d,
        &id,
        ExportOptions {
            format: ExportFormat::Png,
            ..opts
        },
        &mut fonts,
        "Noto Sans",
    )
    .unwrap();
    assert_eq!(png.pixel_size, svg.pixel_size);
    assert_eq!(&png.bytes[..8], b"\x89PNG\r\n\x1a\n");
}
#[test]
fn rejects_preserved_placeholder_and_oversized() {
    let mut d = document();
    d.capability = Capability::PreservedSource {
        source_resource_id: "r".parse().unwrap(),
    };
    let opts = ExportOptions {
        format: ExportFormat::Png,
        appearance: Appearance::Light,
        scale: 1.,
        acknowledge_diagnostics: false,
    };
    assert!(
        export_diagram(
            &d,
            &"d".parse().unwrap(),
            opts,
            &mut TextEngine::new(),
            "Noto Sans"
        )
        .is_err()
    );
    d.capability = Capability::Editable;
    d.diagrams[0].shapes[0].bounds.width = 20000.;
    assert!(
        export_diagram(
            &d,
            &"d".parse().unwrap(),
            opts,
            &mut TextEngine::new(),
            "Noto Sans"
        )
        .is_err()
    );
}
#[test]
fn diagnostic_requires_acknowledgement() {
    let mut d = document();
    d.processes[0].nodes[0].name = "A very long label that cannot fit within these bounds".into();
    d.diagrams[0].shapes[0].bounds.width = 35.;
    d.diagrams[0].shapes[0].bounds.height = 20.;
    let options = ExportOptions {
        format: ExportFormat::Svg,
        appearance: Appearance::Light,
        scale: 1.,
        acknowledge_diagnostics: false,
    };
    let mut fonts = TextEngine::new();
    let id = "d".parse().unwrap();
    assert!(export_diagram(&d, &id, options, &mut fonts, "Noto Sans").is_err());
    let output = export_diagram(
        &d,
        &id,
        ExportOptions {
            acknowledge_diagnostics: true,
            ..options
        },
        &mut fonts,
        "Noto Sans",
    )
    .unwrap();
    assert!(!output.diagnostics.is_empty());
}
#[test]
fn fractional_scale_uses_identical_pixel_canvas_and_transform() {
    let d = document();
    let id = "d".parse().unwrap();
    let opts = ExportOptions {
        format: ExportFormat::Svg,
        appearance: Appearance::Light,
        scale: 1.3,
        acknowledge_diagnostics: false,
    };
    let svg = export_diagram(&d, &id, opts, &mut TextEngine::new(), "Noto Sans").unwrap();
    let png = export_diagram(
        &d,
        &id,
        ExportOptions {
            format: ExportFormat::Png,
            ..opts
        },
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert_eq!(svg.pixel_size, png.pixel_size);
    let text = String::from_utf8(svg.bytes).unwrap();
    assert!(text.contains(&format!(
        "viewBox=\"0 0 {} {}\"",
        svg.pixel_size.0, svg.pixel_size.1
    )));
    assert!(text.contains("<g transform=\"scale(1.3)\">"));
    assert_eq!(
        u32::from_be_bytes(png.bytes[16..20].try_into().unwrap()),
        svg.pixel_size.0
    );
    assert_eq!(
        u32::from_be_bytes(png.bytes[20..24].try_into().unwrap()),
        svg.pixel_size.1
    );
}
#[test]
fn preflight_returns_acknowledgeable_diagnostics() {
    let mut d = document();
    d.processes[0].nodes[0].name = "A very long label that cannot fit within these bounds".into();
    d.diagrams[0].shapes[0].bounds.width = 35.;
    d.diagrams[0].shapes[0].bounds.height = 20.;
    let diagnostics = processcraft_render::preflight_diagram(
        &d,
        &"d".parse().unwrap(),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert!(diagnostics.iter().any(|d| d.contains("clipped")));
}
