use processcraft_render::{DiagramScene, TextEngine, TextRequest};

#[test]
fn same_layout_drives_all_outputs() {
    let mut engine = TextEngine::new();
    let layout = engine
        .layout(TextRequest::new("Café e\u{301} مرحبا 世界", 24.0, 300.0))
        .unwrap();
    assert!(layout.glyphs().iter().all(|g| g.glyph_id != 0));
    let mut scene = DiagramScene::new(400.0, 200.0).unwrap();
    scene.add_label(layout.clone(), 20.0, 20.0).unwrap();
    let paths: Vec<_> = scene.paths().collect();
    assert_eq!(
        paths.len(),
        layout
            .glyphs()
            .iter()
            .filter(|g| g.outline.is_some())
            .count()
    );
    let svg = scene.to_svg();
    assert_eq!(svg.matches("<path ").count(), paths.len());
    // Outlines stay the only visible ink; one transparent, searchable text line
    // carries the same string with the pinned-metric advance.
    assert_eq!(svg.matches("<text ").count(), 1);
    assert!(svg.contains("<g fill-opacity=\"0\" font-family=\"'Noto Sans', 'Noto Sans Arabic', 'Noto Sans CJK SC', sans-serif\""));
    let line = &layout.visible_lines()[0];
    assert_eq!(line.text, "Café e\u{301} مرحبا 世界");
    assert!(svg.contains(&format!(
        "textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\">Café e\u{301} مرحبا 世界</tspan>",
        line.width
    )));
    let before = format!("{:?}", scene.labels());
    assert_eq!(scene.rasterize(1.0).unwrap().width(), 400);
    assert_eq!(scene.rasterize(2.0).unwrap().width(), 800);
    assert_eq!(before, format!("{:?}", scene.labels()));
    assert_eq!(scene.labels()[0].layout.glyphs(), layout.glyphs());
}

#[test]
fn missing_glyph_reports_fallback() {
    let mut engine = TextEngine::new();
    let layout = engine
        .layout(TextRequest::new(
            "Latin العربية 世界 \u{10FFFF}",
            24.0,
            500.0,
        ))
        .unwrap();
    assert!(
        layout
            .diagnostics()
            .iter()
            .any(|d| d.contains("Fallback font"))
    );
    assert!(
        layout
            .diagnostics()
            .iter()
            .any(|d| d.contains("Missing glyph") && d.contains("10FFFF"))
    );
    assert!(
        layout
            .glyphs()
            .iter()
            .any(|g| g.glyph_id == 0 && g.outline.is_some())
    );
}

#[test]
fn advanced_layout_combines_marks_wraps_and_preserves_rtl_clusters() {
    let mut engine = TextEngine::new();
    let decomposed = engine
        .layout(TextRequest::new("e\u{301}", 24.0, 300.0))
        .unwrap();
    let composed = engine.layout(TextRequest::new("é", 24.0, 300.0)).unwrap();
    assert_eq!(decomposed.glyphs().len(), 1);
    assert_eq!(
        decomposed.glyphs()[0].glyph_id,
        composed.glyphs()[0].glyph_id
    );
    assert_eq!(decomposed.glyphs()[0].cluster, 0..3);
    let rtl = engine
        .layout(TextRequest::new("مرحبا 123", 24.0, 300.0))
        .unwrap();
    assert!(rtl.glyphs().iter().any(|g| g.rtl));
    assert!(rtl.glyphs().iter().any(|g| !g.rtl));
    assert!(rtl.glyphs().iter().all(|g| g.glyph_id != 0));
    let wrapped = engine
        .layout(TextRequest::new(
            "Approve request after reviewing all purchase details",
            24.0,
            150.0,
        ))
        .unwrap();
    assert!(wrapped.line_count() >= 3);
    assert!(wrapped.height() > 72.0);
}

#[test]
fn missing_font_is_explicit_and_reproducible() {
    let mut engine = TextEngine::new();
    let mut request = TextRequest::new("Café", 24.0, 300.0);
    let normal = engine.layout(request).unwrap();
    request.family = "Deliberately unavailable ProcessCraft font";
    let fallback = engine.layout(request).unwrap();
    assert!(
        fallback
            .diagnostics()
            .iter()
            .any(|d| d.contains("Missing font"))
    );
    assert_eq!(normal.glyphs(), fallback.glyphs());
}

#[test]
fn invalid_or_excessive_inputs_are_rejected_without_scene_mutation() {
    let mut engine = TextEngine::new();
    for (size, width) in [
        (f32::NAN, 200.0),
        (f32::INFINITY, 200.0),
        (0.0, 200.0),
        (257.0, 200.0),
        (20.0, f32::NAN),
        (20.0, 0.0),
        (20.0, 16385.0),
    ] {
        assert!(
            engine
                .layout(TextRequest::new("label", size, width))
                .is_err()
        );
    }
    assert!(
        engine
            .layout(TextRequest::new(&"a".repeat(16385), 24.0, 300.0))
            .is_err()
    );
    assert!(DiagramScene::new(f32::NAN, 200.0).is_err());
    let mut scene = DiagramScene::new(400.0, 200.0).unwrap();
    let layout = engine
        .layout(TextRequest::new("label", 24.0, 300.0))
        .unwrap();
    assert!(scene.add_label(layout.clone(), f32::INFINITY, 0.0).is_err());
    assert!(scene.labels().is_empty());
    for _ in 0..4096 {
        scene.add_label(layout.clone(), 0.0, 0.0).unwrap();
    }
    assert!(scene.add_label(layout, 0.0, 0.0).is_err());
    assert_eq!(scene.labels().len(), 4096);
    assert!(
        scene
            .add_background_rect(0.0, 0.0, -1.0, 20.0, [0, 0, 0])
            .is_err()
    );
    for scale in [0.0, f32::NAN, f32::INFINITY, 9.0] {
        assert!(scene.rasterize(scale).is_err());
    }
    assert!(
        DiagramScene::new(16384.0, 16384.0)
            .unwrap()
            .rasterize(1.0)
            .is_err()
    );
}

#[test]
fn bundled_font_and_corpus_hashes_match_the_manifest() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("tests/fixtures/manifest.json")).unwrap())
            .unwrap();
    for asset in manifest["fonts"].as_array().unwrap() {
        let bytes = std::fs::read(root.join(asset["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            asset["sha256"].as_str().unwrap()
        );
        assert!(asset["license"].as_str().is_some());
    }
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("assets/fonts/provenance.json")).unwrap())
            .unwrap();
    for asset in provenance.as_array().unwrap() {
        let bytes = std::fs::read(root.join(asset["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            asset["sha256"].as_str().unwrap()
        );
        assert_eq!(asset["license"].as_str(), Some("OFL-1.1"));
    }
}

#[test]
fn svg_metadata_cannot_inject_elements() {
    let mut scene = DiagramScene::new(300.0, 100.0).unwrap();
    let layout = TextEngine::new()
        .layout(TextRequest::new("<script>&\u{1}", 20.0, 280.0))
        .unwrap();
    scene.add_label(layout, 0.0, 0.0).unwrap();
    let svg = scene.to_svg();
    assert!(!svg.contains("<script>"));
    assert!(svg.contains("&lt;script&gt;&amp;"));
    assert!(!svg.contains('\u{1}'));
}

#[test]
fn bundled_styled_faces_and_unavailable_script_style_are_reported() {
    let mut engine = TextEngine::new();
    let mut regular = TextRequest::new("Analyst", 20.0, 200.0);
    let normal = engine.layout(regular).unwrap();
    regular.bold = true;
    regular.italic = true;
    let styled = engine.layout(regular).unwrap();
    assert!(
        !styled
            .diagnostics()
            .iter()
            .any(|s| s.contains("style is unavailable"))
    );
    let first_path = |layout: &processcraft_render::TextLayout| {
        layout
            .glyphs()
            .iter()
            .find_map(|g| g.outline.as_ref())
            .unwrap()
            .bounds()
    };
    assert_ne!(first_path(&normal), first_path(&styled));
    let mut cjk = TextRequest::new("中文", 20.0, 200.0);
    cjk.family = "Noto Sans CJK SC";
    cjk.bold = true;
    cjk.italic = true;
    let fallback = engine.layout(cjk).unwrap();
    assert!(
        fallback
            .diagnostics()
            .iter()
            .any(|s| s.contains("style is unavailable"))
    );
}

#[test]
fn centered_wrapped_lines_keep_balanced_advances_for_ltr_and_rtl() {
    for text in [
        "Approve a large purchase request",
        "مراجعة طلب الشراء الكبير",
    ] {
        let mut request = TextRequest::new(text, 16.0, 110.0);
        request.center = true;
        let layout = TextEngine::new().layout(request).unwrap();
        assert!(layout.line_count() > 1);
        for line in 0..layout.line_count() {
            let glyphs: Vec<_> = layout
                .glyphs()
                .iter()
                .filter(|glyph| glyph.visual_line == line)
                .collect();
            if glyphs.is_empty() {
                continue;
            }
            let left = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|g| g.x + g.advance)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                ((left + right) / 2.0 - 55.0).abs() < 3.0,
                "{text}, line {line}: {left}..{right}"
            );
        }
    }
}
