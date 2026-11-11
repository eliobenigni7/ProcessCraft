//! Evidence only: paths-only PDF, no searchable text or production export API.
use pdf_writer::{Content, Pdf, Rect, Ref};
use processcraft_render::{DiagramScene, TextEngine, TextRequest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, path::PathBuf};
use tiny_skia::PathSegment;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--out" {
        return Err("Usage: text_parity --out DIRECTORY".into());
    }
    let out = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out)?;
    let samples: Vec<Value> =
        serde_json::from_str(include_str!("../../../tests/fixtures/fonts/samples.json"))?;
    let mut engine = TextEngine::new();
    let mut scene = DiagramScene::new(960.0, 860.0)?;
    scene.add_label(
        engine.layout(TextRequest::new(
            "ProcessCraft / shared text proof",
            30.0,
            880.0,
        ))?,
        32.0,
        18.0,
    )?;
    let mut reports = Vec::new();
    for sample in samples {
        let y = sample["y"].as_f64().ok_or("fixture y")? as f32;
        let text = sample["text"].as_str().ok_or("fixture text")?;
        let title = sample["title"].as_str().ok_or("fixture title")?;
        let size = sample["size"].as_f64().ok_or("fixture size")? as f32;
        let width = sample["width"].as_f64().ok_or("fixture width")? as f32;
        scene.add_background_rect(
            24.0,
            y - 28.0,
            912.0,
            if sample["id"] == "wrapping" {
                182.0
            } else {
                98.0
            },
            [242, 245, 247],
        )?;
        scene.add_label(
            engine.layout(TextRequest::new(title, 15.0, 880.0))?,
            36.0,
            y - 24.0,
        )?;
        let layout = engine.layout(TextRequest::new(text, size, width))?;
        println!(
            "{}: {} glyphs, {} lines",
            sample["id"],
            layout.glyphs().len(),
            layout.line_count()
        );
        for message in layout.diagnostics() {
            println!("  {message}");
        }
        reports.push(json!({"sample": sample["id"], "text": text, "lines":layout.line_count(), "diagnostics":layout.diagnostics(), "glyphs":layout.glyphs().iter().map(|g| json!({"font":g.font,"glyph_id":g.glyph_id,"source_line":g.source_line,"cluster":[g.cluster.start,g.cluster.end],"x":g.x,"baseline":g.baseline,"advance":g.advance,"rtl":g.rtl,"outline_segments":g.outline.as_ref().map_or(0,|p|p.segments().count())})).collect::<Vec<_>>()}));
        scene.add_label(layout, 36.0, y)?;
    }
    // One scene and one positioned outline stream for every output.
    scene
        .rasterize(1.0)?
        .save_png(out.join("text-parity-100.png"))?;
    scene
        .rasterize(2.0)?
        .save_png(out.join("text-parity-200.png"))?;
    std::fs::write(out.join("text-parity.svg"), scene.to_svg())?;
    std::fs::write(out.join("text-parity.pdf"), candidate_pdf(&scene))?;
    let evidence = json!({"scope":"M0 shared outline evidence only", "canvas":[scene.width(),scene.height()],"raster_scales":[1,2],"logical_path_sha256":path_digest(&scene),"shared_path_count":scene.paths().count(),"pdf_policy":"Outlined paths; no embedded fonts, searchable text, Unicode extraction or PDF accessibility qualification. Candidate harness only.","samples":reports});
    std::fs::write(
        out.join("layout-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!(
        "Wrote PNG at 100%/200%, outlined SVG, candidate outlined PDF and glyph evidence to {}",
        out.display()
    );
    Ok(())
}

fn path_digest(scene: &DiagramScene) -> String {
    let mut digest = Sha256::new();
    for item in scene.paths() {
        digest.update(item.x.to_le_bytes());
        digest.update(item.y.to_le_bytes());
        digest.update(item.color);
        for segment in item.path.segments() {
            digest.update(format!("{segment:?}"));
        }
    }
    format!("{:x}", digest.finalize())
}

fn candidate_pdf(scene: &DiagramScene) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (catalog, pages, page, stream) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4));
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids([page]).count(1);
    pdf.page(page)
        .parent(pages)
        .media_box(Rect::new(0.0, 0.0, scene.width(), scene.height()))
        .contents(stream);
    let mut content = Content::new();
    content.transform([1.0, 0.0, 0.0, -1.0, 0.0, scene.height()]);
    content
        .set_fill_rgb(1.0, 1.0, 1.0)
        .rect(0.0, 0.0, scene.width(), scene.height())
        .fill_nonzero();
    for item in scene.paths() {
        content
            .save_state()
            .transform([1.0, 0.0, 0.0, 1.0, item.x, item.y]);
        content.set_fill_rgb(
            f32::from(item.color[0]) / 255.0,
            f32::from(item.color[1]) / 255.0,
            f32::from(item.color[2]) / 255.0,
        );
        let mut current = (0.0, 0.0);
        for segment in item.path.segments() {
            match segment {
                PathSegment::MoveTo(p) => {
                    content.move_to(p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::LineTo(p) => {
                    content.line_to(p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::QuadTo(a, p) => {
                    content.cubic_to(
                        current.0 + (a.x - current.0) * 2.0 / 3.0,
                        current.1 + (a.y - current.1) * 2.0 / 3.0,
                        p.x + (a.x - p.x) * 2.0 / 3.0,
                        p.y + (a.y - p.y) * 2.0 / 3.0,
                        p.x,
                        p.y,
                    );
                    current = (p.x, p.y);
                }
                PathSegment::CubicTo(a, b, p) => {
                    content.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::Close => {
                    content.close_path();
                }
            }
        }
        content.fill_nonzero().restore_state();
    }
    pdf.stream(stream, &content.finish());
    pdf.finish()
}

#[test]
fn candidate_pdf_consumes_shared_scene_paths() {
    let mut scene = DiagramScene::new(200.0, 100.0).unwrap();
    scene
        .add_label(
            TextEngine::new()
                .layout(TextRequest::new("é مرحبا 世界", 20.0, 180.0))
                .unwrap(),
            5.0,
            5.0,
        )
        .unwrap();
    let pdf = candidate_pdf(&scene);
    let source = String::from_utf8_lossy(&pdf);
    assert_eq!(source.matches("\nf\n").count(), scene.paths().count() + 1);
    assert!(!source.contains("/Subtype /Image"));
    assert!(!source.contains("/Font"));
    assert_eq!(
        scene.to_svg().matches("<path ").count(),
        scene.paths().count()
    );
    assert_eq!(path_digest(&scene), path_digest(&scene.clone()));
}
