use crate::{RenderError, TextLayout};
use std::fmt::Write;
use tiny_skia::{Path, PathBuilder, PathSegment, Rect};

#[derive(Clone, Debug)]
pub struct Label {
    pub layout: TextLayout,
    pub x: f32,
    pub y: f32,
    pub color: [u8; 3],
}

/// Read-only positioned geometry consumed unchanged by raster/SVG/PDF adapters.
#[derive(Clone, Copy)]
pub struct ScenePath<'a> {
    pub path: &'a Path,
    pub x: f32,
    pub y: f32,
    pub color: [u8; 3],
}

/// M0 artboard: solid background rectangles, then labels. No BPMN domain model.
#[derive(Clone, Debug)]
pub struct DiagramScene {
    width: f32,
    height: f32,
    backgrounds: Vec<(Path, [u8; 3])>,
    labels: Vec<Label>,
    glyph_count: usize,
    segment_count: usize,
    ordering: bool,
    active_rank: usize,
    sequence: usize,
    background_order: Vec<(usize, usize)>,
    label_order: Vec<(usize, usize)>,
}
impl DiagramScene {
    pub fn new(width: f32, height: f32) -> Result<Self, RenderError> {
        Self::with_limit(width, height, 16384.0)
    }
    pub(crate) fn for_diagram(width: f32, height: f32) -> Result<Self, RenderError> {
        Self::with_limit(width, height, 2_000_256.0)
    }
    fn with_limit(width: f32, height: f32, limit: f32) -> Result<Self, RenderError> {
        if !width.is_finite()
            || !height.is_finite()
            || !(1.0..=limit).contains(&width)
            || !(1.0..=limit).contains(&height)
        {
            return Err(RenderError(
                "Scene dimensions must be finite and within 1..16384",
            ));
        }
        Ok(Self {
            width,
            height,
            backgrounds: Vec::new(),
            labels: Vec::new(),
            glyph_count: 0,
            segment_count: 0,
            ordering: false,
            active_rank: 0,
            sequence: 0,
            background_order: Vec::new(),
            label_order: Vec::new(),
        })
    }
    pub fn width(&self) -> f32 {
        self.width
    }
    pub fn height(&self) -> f32 {
        self.height
    }
    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// Finite placement within +/-1,000,000 logical pixels. Artboard clips output.
    pub fn add_label(&mut self, layout: TextLayout, x: f32, y: f32) -> Result<(), RenderError> {
        coordinates(&[x, y])?;
        let segments: usize = layout
            .glyphs()
            .iter()
            .filter_map(|g| g.outline.as_ref())
            .map(|p| p.segments().count())
            .sum();
        if self.labels.len() >= 4096
            || self.glyph_count + layout.glyphs().len() > 32768
            || self.segment_count + segments > 1_000_000
        {
            return Err(RenderError(
                "Scene exceeds 4096 labels, 32768 glyphs or one million path segments",
            ));
        }
        self.glyph_count += layout.glyphs().len();
        self.segment_count += segments;
        self.labels.push(Label {
            layout,
            x,
            y,
            color: [26, 37, 52],
        });
        self.label_order.push((self.active_rank, self.sequence));
        self.sequence += 1;
        Ok(())
    }
    pub fn add_background_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 3],
    ) -> Result<(), RenderError> {
        coordinates(&[x, y, width, height])?;
        if width <= 0.0 || height <= 0.0 || self.backgrounds.len() >= 16384 {
            return Err(RenderError(
                "Use positive rectangles; at most 16384 backgrounds",
            ));
        }
        let rect = Rect::from_xywh(x, y, width, height).ok_or(RenderError("Invalid rectangle"))?;
        self.backgrounds.push((PathBuilder::from_rect(rect), color));
        self.background_order
            .push((self.active_rank, self.sequence));
        self.sequence += 1;
        Ok(())
    }
    pub(crate) fn prepend_background_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 3],
    ) -> Result<(), RenderError> {
        let rect = Rect::from_xywh(x, y, width, height)
            .ok_or(RenderError("Invalid viewport background"))?;
        self.backgrounds
            .insert(0, (PathBuilder::from_rect(rect), color));
        self.background_order.insert(0, (0, 0));
        Ok(())
    }
    /// Under everything already in the scene, like [`Self::prepend_background_rect`].
    pub(crate) fn prepend_background_path(&mut self, path: tiny_skia::Path, color: [u8; 3]) {
        self.backgrounds.insert(0, (path, color));
        self.background_order.insert(0, (0, 0));
    }
    pub fn add_colored_label(
        &mut self,
        layout: TextLayout,
        x: f32,
        y: f32,
        color: [u8; 3],
    ) -> Result<(), RenderError> {
        self.add_label(layout, x, y)?;
        self.labels.last_mut().expect("label just added").color = color;
        Ok(())
    }
    pub(crate) fn rotate_last_label(&mut self, x: f32, bottom: f32) {
        if let Some(label) = self.labels.last_mut() {
            label.layout.rotate_counterclockwise();
            let (old_x, old_y) = (label.x, label.y);
            label.x = x + old_y;
            label.y = bottom - old_x;
        }
    }
    pub fn add_path(&mut self, path: Path, color: [u8; 3]) -> Result<(), RenderError> {
        let segments = path.segments().count();
        if self.backgrounds.len() >= 16384 || self.segment_count + segments > 1_000_000 {
            return Err(RenderError("Scene exceeds path complexity limit"));
        }
        self.segment_count += segments;
        self.backgrounds.push((path, color));
        self.background_order
            .push((self.active_rank, self.sequence));
        self.sequence += 1;
        Ok(())
    }
    pub(crate) fn set_paint_rank(&mut self, rank: usize) {
        self.ordering = true;
        self.active_rank = rank;
    }
    pub fn paths(&self) -> impl Iterator<Item = ScenePath<'_>> {
        let legacy = self
            .backgrounds
            .iter()
            .map(|(path, color)| ScenePath {
                path,
                x: 0.0,
                y: 0.0,
                color: *color,
            })
            .chain(self.labels.iter().flat_map(|label| {
                label.layout.glyphs().iter().filter_map(move |g| {
                    g.outline.as_ref().map(|path| ScenePath {
                        path,
                        x: label.x,
                        y: label.y,
                        color: label.color,
                    })
                })
            }));
        if !self.ordering {
            return legacy.collect::<Vec<_>>().into_iter();
        }
        let mut ordered = Vec::new();
        for ((path, color), order) in self.backgrounds.iter().zip(&self.background_order) {
            ordered.push((
                *order,
                ScenePath {
                    path,
                    x: 0.0,
                    y: 0.0,
                    color: *color,
                },
            ));
        }
        for (label, order) in self.labels.iter().zip(&self.label_order) {
            for glyph in label.layout.glyphs() {
                if let Some(path) = &glyph.outline {
                    ordered.push((
                        *order,
                        ScenePath {
                            path,
                            x: label.x,
                            y: label.y,
                            color: label.color,
                        },
                    ));
                }
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        ordered
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>()
            .into_iter()
    }
    /// Outlined, font-independent SVG drawing plus a transparent text layer.
    pub fn to_svg(&self) -> String {
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><title>ProcessCraft M0 shared outlines</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
            self.width, self.height, self.width, self.height
        );
        for item in self.paths() {
            let _ = write!(
                svg,
                "<path fill=\"#{:02x}{:02x}{:02x}\" transform=\"translate({} {})\" d=\"",
                item.color[0], item.color[1], item.color[2], item.x, item.y
            );
            for segment in item.path.segments() {
                match segment {
                    PathSegment::MoveTo(p) => {
                        let _ = write!(svg, "M{} {}", p.x, p.y);
                    }
                    PathSegment::LineTo(p) => {
                        let _ = write!(svg, "L{} {}", p.x, p.y);
                    }
                    PathSegment::QuadTo(a, p) => {
                        let _ = write!(svg, "Q{} {} {} {}", a.x, a.y, p.x, p.y);
                    }
                    PathSegment::CubicTo(a, b, p) => {
                        let _ = write!(svg, "C{} {} {} {} {} {}", a.x, a.y, b.x, b.y, p.x, p.y);
                    }
                    PathSegment::Close => svg.push('Z'),
                }
            }
            svg.push_str("\"/>");
        }
        // The outlines above are the only visible rendering. This transparent layer
        // makes labels selectable and searchable; textLength pins each line to the
        // bundled-font advance so selection boxes match the drawn glyphs.
        let _ = write!(
            svg,
            "<g fill-opacity=\"0\" font-family=\"{SVG_FONT_STACK}\" xml:space=\"preserve\">"
        );
        for label in &self.labels {
            let layout = &label.layout;
            let lines = layout.visible_lines();
            if lines.is_empty() && !layout.partial() {
                continue;
            }
            let _ = write!(
                svg,
                "<text transform=\"translate({} {}){}\" font-size=\"{}\"{}{}>",
                label.x,
                label.y,
                if layout.rotated() { " rotate(-90)" } else { "" },
                layout.font_size(),
                if layout.bold() {
                    " font-weight=\"bold\""
                } else {
                    ""
                },
                if layout.italic() {
                    " font-style=\"italic\""
                } else {
                    ""
                }
            );
            if layout.partial() {
                let _ = write!(svg, "<desc>{}</desc>", xml_escape(layout.text()));
            }
            for line in lines {
                let _ = write!(
                    svg,
                    "<tspan x=\"{}\" y=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\">{}</tspan>",
                    line.x,
                    line.baseline,
                    line.width,
                    xml_escape(&line.text)
                );
            }
            svg.push_str("</text>");
        }
        svg.push_str("</g></svg>");
        svg
    }
}

/// The pinned catalog, in fallback order, then a generic family.
const SVG_FONT_STACK: &str = "'Noto Sans', 'Noto Sans Arabic', 'Noto Sans CJK SC', sans-serif";
pub(crate) const SVG_TEXT_NOTICE: &str = "SVG text is drawn as outlines from the bundled fonts, so appearance is preserved; a transparent text layer (Noto Sans font stack, line lengths pinned to the bundled metrics) makes labels searchable and selectable. That layer is not tagged for accessibility, and selection boxes follow the viewer's font fitted to those lengths.";
fn coordinates(values: &[f32]) -> Result<(), RenderError> {
    if values
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 2_000_256.0)
    {
        Err(RenderError("Coordinates must be finite within +/-1000000"))
    } else {
        Ok(())
    }
}
fn xml_escape(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            // XML 1.0 cannot represent these controls; metadata only, layout is unchanged.
            c if !matches!(c, '\u{9}'|'\u{A}'|'\u{D}'|'\u{20}'..='\u{D7FF}'|'\u{E000}'..='\u{FFFD}'|'\u{10000}'..='\u{10FFFF}') => {
                result.push('\u{FFFD}')
            }
            c => result.push(c),
        }
    }
    result
}
