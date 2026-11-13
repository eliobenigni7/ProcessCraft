//! M5.4 attribute overlay in the shared scene and the export formats.
use super::document;
use processcraft_core::model::{BpmnDocument, Node, NodeKind, Shape, Style};
use processcraft_core::overlay::{LegendItem, Overlay, OverlayEntry, OverlayScale};
use processcraft_render::{
    Appearance, BpmnScene, BpmnViewport, ExportFormat, ExportOptions, TextEngine, Viewport,
    contrast_ratio, export_diagram, export_diagram_with_overlay, overlay_fill, overlay_ink,
};

const ALL: [Appearance; 4] = [
    Appearance::Light,
    Appearance::Dark,
    Appearance::CanvasLight,
    Appearance::CanvasDark,
];

/// Task `n` (the shared fixture) plus start event `e` to its right.
fn two_nodes() -> BpmnDocument {
    let mut d = document();
    let mut node: Node = d.processes[0].nodes[0].clone();
    node.id = "e".parse().unwrap();
    node.name = Some("Start".into());
    node.kind = NodeKind::StartEvent;
    d.processes[0].nodes.push(node);
    let mut shape: Shape = d.diagrams[0].shapes[0].clone();
    shape.id = "s2".parse().unwrap();
    shape.element = "e".parse().unwrap();
    shape.bounds.x = 150.;
    shape.bounds.width = 36.;
    shape.bounds.height = 36.;
    d.diagrams[0].shapes.push(shape);
    d
}
fn entry(element: &str, bucket: Option<u8>, badge: Option<&str>) -> OverlayEntry {
    OverlayEntry {
        element: element.parse().unwrap(),
        bucket,
        badge: badge.map(Into::into),
    }
}
fn heatmap() -> Overlay {
    Overlay {
        entries: vec![entry("n", Some(3), None)],
        legend: (0..5)
            .map(|bucket| LegendItem {
                bucket,
                label: format!("range {bucket}"),
            })
            .collect(),
        title: "Cost (EUR)".into(),
        scale: OverlayScale::Sequential,
    }
}
fn options(format: ExportFormat, appearance: Appearance) -> ExportOptions {
    ExportOptions {
        format,
        appearance,
        scale: 1.,
        acknowledge_diagnostics: true,
    }
}
fn build(d: &BpmnDocument, appearance: Appearance, overlay: Option<&Overlay>) -> BpmnScene {
    BpmnScene::build_with_overlay(
        d,
        &"d".parse().unwrap(),
        appearance,
        &mut TextEngine::new(),
        "Noto Sans",
        overlay,
    )
    .unwrap()
}
fn colours(scene: &BpmnScene) -> Vec<[u8; 3]> {
    scene.scene.paths().map(|p| p.color).collect()
}

#[test]
fn overlay_off_is_byte_identical_for_every_format_and_appearance() {
    let d = two_nodes();
    let id = "d".parse().unwrap();
    let empty = Overlay::empty();
    for appearance in ALL {
        for format in [ExportFormat::Svg, ExportFormat::Png, ExportFormat::Pdf] {
            let o = options(format, appearance);
            let plain = export_diagram(&d, &id, o, &mut TextEngine::new(), "Noto Sans").unwrap();
            let none =
                export_diagram_with_overlay(&d, &id, o, &mut TextEngine::new(), "Noto Sans", None)
                    .unwrap();
            assert_eq!(plain.bytes, none.bytes, "{format:?} {appearance:?}");
            assert_eq!(plain.logical_size, none.logical_size);
            let with_empty = export_diagram_with_overlay(
                &d,
                &id,
                o,
                &mut TextEngine::new(),
                "Noto Sans",
                Some(&empty),
            )
            .unwrap();
            assert_eq!(plain.bytes, with_empty.bytes, "empty overlay {format:?}");
        }
    }
}

#[test]
fn overlay_changes_only_the_activity_fill_and_appends_the_legend() {
    let d = two_nodes();
    let overlay = heatmap();
    for appearance in ALL {
        let base = build(&d, appearance, None);
        let with = build(&d, appearance, Some(&overlay));
        let (base_colours, with_colours) = (colours(&base), colours(&with));
        let fill = overlay_fill(appearance, OverlayScale::Sequential, 3);
        // Nothing was removed except the task's own fill; the ramp fill is present.
        let mut remaining = with_colours.clone();
        let mut removed = vec![];
        for colour in &base_colours {
            match remaining.iter().position(|c| c == colour) {
                Some(i) => {
                    remaining.swap_remove(i);
                }
                None => removed.push(*colour),
            }
        }
        assert_eq!(removed.len(), 1, "{appearance:?}: {removed:?}");
        assert!(with_colours.contains(&fill));
        assert!(with_colours.len() > base_colours.len());
        assert_eq!(base.hits.len(), with.hits.len());
        assert_eq!(base.diagnostics, with.diagnostics);
        assert_eq!(base.origin, with.origin);
        // The legend grows the scene downwards (and sideways only if needed).
        assert!(with.scene.height() > base.scene.height());
        assert!(with.scene.width() >= base.scene.width());
        let texts: Vec<String> = with
            .scene
            .labels()
            .iter()
            .map(|l| l.layout.text().to_owned())
            .collect();
        assert!(texts.contains(&"Cost (EUR)".to_owned()));
        assert!(texts.contains(&"range 4".to_owned()));
        // Label ink on the filled task meets 4.5:1 against its fill.
        let task_label = with
            .scene
            .labels()
            .iter()
            .find(|l| l.layout.text() == "A & B")
            .unwrap();
        assert_eq!(task_label.color, overlay_ink(appearance));
        assert!(contrast_ratio(task_label.color, fill) >= 4.5);
    }
}

#[test]
fn badges_are_pills_at_the_top_right_corner_with_readable_text() {
    let d = two_nodes();
    let overlay = Overlay {
        entries: vec![
            entry("n", None, Some("12 EUR")),
            entry("e", None, Some("skipped")),
        ],
        legend: vec![],
        title: "Cost".into(),
        scale: OverlayScale::Sequential,
    };
    for appearance in ALL {
        let base = build(&d, appearance, None);
        let with = build(&d, appearance, Some(&overlay));
        assert_eq!(base.diagnostics, with.diagnostics);
        // No legend, so the artboard is unchanged.
        assert_eq!(base.scene.height(), with.scene.height());
        assert_eq!(base.scene.width(), with.scene.width());
        let badge = with
            .scene
            .labels()
            .iter()
            .find(|l| l.layout.text() == "12 EUR")
            .unwrap();
        let pill = colours(&with)
            .into_iter()
            .rev()
            .find(|c| *c == overlay_ink(appearance))
            .unwrap();
        assert!(contrast_ratio(badge.color, pill) >= 4.5, "{appearance:?}");
        // Task right edge is at x=70 (origin -66, so 136 in the scene): the pill ends 6 px
        // beyond it and starts to its left; its text sits inside that span.
        assert!(badge.x > 80. && badge.x < 142., "{}", badge.x);
        assert!(
            with.scene
                .labels()
                .iter()
                .any(|l| l.layout.text() == "skipped")
        );
    }
}

#[test]
fn overlay_replaces_an_explicit_imported_fill_only_while_drawn() {
    let mut d = two_nodes();
    d.diagrams[0].shapes[0].style = Some(Style {
        fill: Some("#ff0000".into()),
        stroke: None,
        font: Some("#ffff00".into()),
    });
    let overlay = heatmap();
    let plain = build(&d, Appearance::Light, None);
    assert!(colours(&plain).contains(&[255, 0, 0]));
    let with = build(&d, Appearance::Light, Some(&overlay));
    assert!(!colours(&with).contains(&[255, 0, 0]));
    let label = with
        .scene
        .labels()
        .iter()
        .find(|l| l.layout.text() == "A & B")
        .unwrap();
    assert_eq!(label.color, overlay_ink(Appearance::Light));
    // The document is never touched: the same document draws red again without the overlay.
    assert_eq!(
        d.diagrams[0].shapes[0]
            .style
            .as_ref()
            .unwrap()
            .fill
            .as_deref(),
        Some("#ff0000")
    );
    assert!(colours(&build(&d, Appearance::Light, None)).contains(&[255, 0, 0]));
}

#[test]
fn svg_png_and_pdf_carry_the_same_legend() {
    let d = two_nodes();
    let overlay = heatmap();
    let id = "d".parse().unwrap();
    let mut sizes = vec![];
    for format in [ExportFormat::Svg, ExportFormat::Png, ExportFormat::Pdf] {
        let o = options(format, Appearance::Light);
        let plain = export_diagram(&d, &id, o, &mut TextEngine::new(), "Noto Sans").unwrap();
        let with = export_diagram_with_overlay(
            &d,
            &id,
            o,
            &mut TextEngine::new(),
            "Noto Sans",
            Some(&overlay),
        )
        .unwrap();
        assert_ne!(plain.bytes, with.bytes, "{format:?}");
        assert!(with.logical_size.1 > plain.logical_size.1, "{format:?}");
        if format == ExportFormat::Svg {
            let svg = String::from_utf8(with.bytes.clone()).unwrap();
            assert!(svg.contains("Cost (EUR)"));
            assert!(svg.contains("range 0") && svg.contains("range 4"));
        }
        sizes.push(with.logical_size);
    }
    assert!(sizes.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn raster_pixels_show_the_ramp_colour_and_a_legend_swatch() {
    let d = two_nodes();
    let overlay = heatmap();
    let scene = build(&d, Appearance::Light, Some(&overlay));
    let pixels = scene.scene.rasterize(1.).unwrap();
    let fill = overlay_fill(Appearance::Light, OverlayScale::Sequential, 3);
    let at = |x: u32, y: u32| {
        let p = pixels.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue()]
    };
    // Task: x -50..70, y -20..60, origin (-66,-36); (30,30) is plain fill.
    assert_eq!(at(30, 30), fill);
    // First legend swatch: x 16..28, first row below the diagram.
    let diagram_height = build(&d, Appearance::Light, None).scene.height();
    let y = (diagram_height + 8. + 18. + 2. + 6.) as u32;
    assert_eq!(
        at(22, y),
        overlay_fill(Appearance::Light, OverlayScale::Sequential, 0)
    );
}

#[test]
fn viewport_overlay_fills_activities_and_leaves_the_legend_to_the_desktop() {
    let d = two_nodes();
    let overlay = heatmap();
    let view = Viewport {
        x: -66.,
        y: -36.,
        width: 260.,
        height: 140.,
        zoom: 1.,
    };
    let build_view = |overlay: Option<&Overlay>| {
        BpmnViewport::build_with_overlay(
            &d,
            &"d".parse().unwrap(),
            Appearance::CanvasLight,
            &mut TextEngine::new(),
            "Noto Sans",
            view,
            overlay,
        )
        .unwrap()
    };
    let (plain, with) = (build_view(None), build_view(Some(&overlay)));
    assert_eq!(plain.pixels.width(), with.pixels.width());
    assert_eq!(plain.pixels.height(), with.pixels.height());
    let px = |v: &BpmnViewport| {
        let p = v.pixels.pixel(50, 50).unwrap();
        [p.red(), p.green(), p.blue()]
    };
    assert_ne!(px(&plain), px(&with));
    assert_eq!(
        px(&with),
        overlay_fill(Appearance::CanvasLight, OverlayScale::Sequential, 3)
    );
}

#[test]
fn viewport_badges_are_culled_with_their_shape_at_the_view_edge() {
    let d = two_nodes();
    let overlay = Overlay {
        entries: vec![entry("n", None, Some("12 EUR"))],
        legend: vec![],
        title: "Cost".into(),
        scale: OverlayScale::Sequential,
    };
    let build_view = |x: f64, overlay: Option<&Overlay>| {
        BpmnViewport::build_with_overlay(
            &d,
            &"d".parse().unwrap(),
            Appearance::CanvasLight,
            &mut TextEngine::new(),
            "Noto Sans",
            Viewport {
                x,
                y: -36.,
                width: 60.,
                height: 100.,
                zoom: 1.,
            },
            overlay,
        )
        .unwrap()
    };
    // The task spans x -50..70. A view starting at 100 does not intersect it (nor its
    // badge, which hangs only 6 px past the right edge): identical pixels, no error.
    assert_eq!(
        build_view(100., None).pixels.data(),
        build_view(100., Some(&overlay)).pixels.data()
    );
    // A view whose left edge cuts the task keeps the shape and draws its badge pill.
    assert_ne!(
        build_view(60., None).pixels.data(),
        build_view(60., Some(&overlay)).pixels.data()
    );
}
