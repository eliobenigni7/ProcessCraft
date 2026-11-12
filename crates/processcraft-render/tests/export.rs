use processcraft_core::model::{
    Bounds, BpmnDocument, Capability, Diagram, Node, NodeKind, Process, Shape,
};
use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine, export_diagram};
fn document() -> BpmnDocument {
    let mut d = BpmnDocument::new("test".into(), "defs".parse().unwrap());
    d.processes.push(Process {
        activity_metadata: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: "p".parse().unwrap(),
        name: Some(String::new()),
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![Node {
            flow_metadata: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: "n".parse().unwrap(),
            name: Some("A & B".into()),
            documentation: String::new().into(),
            base_extras: None,
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: "d".parse().unwrap(),
        plane_id: "plane".parse().unwrap(),
        plane: "p".parse().unwrap(),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
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
fn svg_export_carries_searchable_text_and_reports_it() {
    let d = document();
    let opts = ExportOptions {
        format: ExportFormat::Svg,
        appearance: Appearance::Light,
        scale: 1.,
        acknowledge_diagnostics: false,
    };
    // Informational: no acknowledgement is needed, unlike the outlined PDF.
    let svg = export_diagram(
        &d,
        &"d".parse().unwrap(),
        opts,
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert!(
        svg.diagnostics
            .iter()
            .any(|m| m.contains("searchable and selectable"))
    );
    let text = String::from_utf8(svg.bytes).unwrap();
    assert!(text.contains("lengthAdjust=\"spacingAndGlyphs\">A &amp; B</tspan>"));
    assert!(text.contains("<g fill-opacity=\"0\""));
    assert!(text.contains("<path "));
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
    d.processes[0].nodes[0].name =
        Some("A very long label that cannot fit within these bounds".into());
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
    d.processes[0].nodes[0].name =
        Some("A very long label that cannot fit within these bounds".into());
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

#[test]
fn styled_label_uses_same_scene_in_svg_png_and_pdf() {
    use processcraft_core::model::{DiFont, DiLabelVisibility, DiStyleRef, Label, LabelStyle};
    let mut d = document();
    d.diagrams[0].label_styles.push(LabelStyle {
        id: Some("style".parse().unwrap()),
        font: Some(DiFont {
            name: Some("Noto Sans".into()),
            size: Some(16.0),
            is_bold: Some(true),
            is_italic: Some(true),
            is_underline: Some(true),
            is_strike_through: Some(false),
        }),
    });
    d.diagrams[0].shapes[0].label_visibility = Some(DiLabelVisibility::Explicit);
    d.diagrams[0].shapes[0].label = Some(Label {
        di_metadata: None,
        id: Some("label".parse().unwrap()),
        bounds: None,
        style_ref: Some(DiStyleRef::Local("style".parse().unwrap())),
        style: None,
    });
    let diagram = "d".parse().unwrap();
    let export = |format| {
        export_diagram(
            &d,
            &diagram,
            ExportOptions {
                format,
                appearance: Appearance::Light,
                scale: 1.0,
                acknowledge_diagnostics: true,
            },
            &mut TextEngine::new(),
            "Noto Sans",
        )
        .unwrap()
    };
    let svg = export(ExportFormat::Svg);
    let png = export(ExportFormat::Png);
    let pdf = export(ExportFormat::Pdf);
    assert!(String::from_utf8(svg.bytes).unwrap().contains("A &amp; B"));
    assert!(png.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(pdf.bytes.starts_with(b"%PDF"));
    assert!(
        !png.diagnostics
            .iter()
            .any(|message| message.contains("unavailable") || message.contains("unresolved"))
    );
}

#[test]
fn opaque_di_visual_effects_require_export_acknowledgement() {
    use processcraft_core::{
        bpmn_xml::{NamespaceBinding, XmlContent, XmlElement, XmlNode},
        model::DiElementMetadata,
    };
    let mut d = document();
    d.diagrams[0].plane_metadata = Some(DiElementMetadata {
        namespaces: vec![NamespaceBinding {
            prefix: "vendor".into(),
            uri: "urn:vendor".into(),
        }],
        foreign_attributes: vec![],
        extension: Some(XmlContent {
            namespaces: vec![],
            nodes: vec![XmlNode::Comment {
                text: "inert".into(),
            }],
        }),
    });
    let id = "d".parse().unwrap();
    let options = ExportOptions {
        format: ExportFormat::Png,
        appearance: Appearance::Light,
        scale: 1.0,
        acknowledge_diagnostics: false,
    };
    let baseline = export_diagram(&d, &id, options, &mut TextEngine::new(), "Noto Sans").unwrap();
    d.diagrams[0]
        .plane_metadata
        .as_mut()
        .unwrap()
        .extension
        .as_mut()
        .unwrap()
        .nodes
        .push(XmlNode::ProcessingInstruction {
            target: "vendor".into(),
            data: Some("opaque".into()),
        });
    assert!(export_diagram(&d, &id, options, &mut TextEngine::new(), "Noto Sans").is_err());
    let output = export_diagram(
        &d,
        &id,
        ExportOptions {
            acknowledge_diagnostics: true,
            ..options
        },
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert!(
        output
            .diagnostics
            .iter()
            .any(|v| v.contains("visual effects are not interpreted"))
    );
    assert_eq!(&output.bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(output.bytes, baseline.bytes);
    d.diagrams[0].plane_metadata = None;
    d.diagrams[0].shapes[0].di_metadata = Some(DiElementMetadata {
        namespaces: vec![],
        foreign_attributes: vec![],
        extension: Some(XmlContent {
            namespaces: vec![],
            nodes: vec![XmlNode::Element {
                element: XmlElement {
                    namespace: "http://www.omg.org/spec/DD/20100524/DC".into(),
                    local: "Bounds".parse().unwrap(),
                    prefix: Some("dc".parse().unwrap()),
                    namespaces: vec![],
                    attributes: vec![],
                    children: vec![],
                },
            }],
        }),
    });
    assert!(export_diagram(&d, &id, options, &mut TextEngine::new(), "Noto Sans").is_err());
    let inert = export_diagram(
        &d,
        &id,
        ExportOptions {
            acknowledge_diagnostics: true,
            ..options
        },
        &mut TextEngine::new(),
        "Noto Sans",
    )
    .unwrap();
    assert_eq!(inert.bytes, baseline.bytes);
}
