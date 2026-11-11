//! Bounded vector PDF using the shared scene, including its positioned glyph outlines.
use crate::{DiagramScene, RenderError};
use pdf_writer::{Content, Name, Pdf, Rect, Ref};
use tiny_skia::PathSegment;

pub(crate) const OUTLINE_NOTICE: &str = "PDF text is outlined using the bundled fonts. Appearance is preserved; text is not searchable, selectable or tagged for accessibility.";

pub(crate) fn export(scene: &DiagramScene, scale: f32) -> Result<(Vec<u8>, usize), RenderError> {
    let (width, height, margin) = (841.89_f32, 595.28_f32, 24.0_f32);
    // Scene units are CSS pixels; PDF page coordinates are points.
    let scale = scale * 0.75;
    let tile_width = (width - 2.0 * margin) / scale;
    let tile_height = (height - 2.0 * margin) / scale;
    let columns = (scene.width() / tile_width).ceil().max(1.0) as usize;
    let rows = (scene.height() / tile_height).ceil().max(1.0) as usize;
    let pages_count = columns
        .checked_mul(rows)
        .ok_or(RenderError("PDF page count overflow"))?;
    if pages_count > 100 {
        return Err(RenderError(
            "PDF exceeds the 100-page limit; reduce the export scale",
        ));
    }
    let mut drawing = Content::new();
    for item in scene.paths() {
        drawing
            .save_state()
            .transform([1.0, 0.0, 0.0, 1.0, item.x, item.y]);
        drawing.set_fill_rgb(
            item.color[0] as f32 / 255.0,
            item.color[1] as f32 / 255.0,
            item.color[2] as f32 / 255.0,
        );
        let mut current = (0.0, 0.0);
        for segment in item.path.segments() {
            match segment {
                PathSegment::MoveTo(p) => {
                    drawing.move_to(p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::LineTo(p) => {
                    drawing.line_to(p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::QuadTo(a, p) => {
                    drawing.cubic_to(
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
                    drawing.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y);
                    current = (p.x, p.y);
                }
                PathSegment::Close => {
                    drawing.close_path();
                }
            }
        }
        drawing.fill_nonzero().restore_state();
    }
    let drawing = drawing.finish();
    if drawing.len() > 64 * 1024 * 1024 {
        return Err(RenderError("PDF vector stream exceeds 64 MiB"));
    }
    let mut pdf = Pdf::new();
    let (catalog, pages, form) = (Ref::new(1), Ref::new(2), Ref::new(3));
    pdf.catalog(catalog).pages(pages);
    let page_ids: Vec<_> = (0..pages_count)
        .map(|n| Ref::new(4 + n as i32 * 2))
        .collect();
    pdf.pages(pages)
        .kids(page_ids.iter().copied())
        .count(pages_count as i32);
    pdf.form_xobject(form, &drawing)
        .bbox(Rect::new(0.0, 0.0, scene.width(), scene.height()));
    for (index, page) in page_ids.iter().enumerate() {
        let stream = Ref::new(5 + index as i32 * 2);
        {
            let mut page = pdf.page(*page);
            page.parent(pages)
                .media_box(Rect::new(0.0, 0.0, width, height))
                .contents(stream);
            page.resources().x_objects().pair(Name(b"Diagram"), form);
        }
        let mut content = Content::new();
        content
            .set_fill_rgb(1.0, 1.0, 1.0)
            .rect(0.0, 0.0, width, height)
            .fill_nonzero();
        content
            .save_state()
            .rect(margin, margin, width - 2.0 * margin, height - 2.0 * margin)
            .clip_nonzero()
            .end_path();
        let x = (index % columns) as f32 * tile_width;
        let y = (index / columns) as f32 * tile_height;
        content.transform([
            scale,
            0.0,
            0.0,
            -scale,
            margin - x * scale,
            height - margin + y * scale,
        ]);
        content.x_object(Name(b"Diagram")).restore_state();
        pdf.stream(stream, &content.finish());
    }
    let bytes = pdf.finish();
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(RenderError("PDF exceeds 128 MiB"));
    }
    Ok((bytes, pages_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paginates_shared_vectors_once_and_bounds_work() {
        let mut scene = DiagramScene::new(2200.0, 1000.0).unwrap();
        scene
            .add_background_rect(10.0, 10.0, 2000.0, 900.0, [12, 34, 56])
            .unwrap();
        let (bytes, pages) = export(&scene, 1.0).unwrap();
        assert_eq!(pages, 6);
        let source = String::from_utf8_lossy(&bytes);
        assert_eq!(source.matches("/Subtype /Form").count(), 1);
        assert_eq!(source.matches("/Diagram Do").count(), 6);
        assert!(!source.contains("/Subtype /Image"));
        assert!(!source.contains("/JavaScript"));
        assert!(export(&DiagramScene::new(16384.0, 16384.0).unwrap(), 8.0).is_err());
    }
}
