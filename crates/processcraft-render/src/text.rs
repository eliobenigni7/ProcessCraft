use crate::RenderError;
use cosmic_text::{Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap, fontdb};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};
use tiny_skia::{Path, PathBuilder};

const FAMILIES: [&str; 3] = ["Noto Sans", "Noto Sans Arabic", "Noto Sans CJK SC"];
const FONTS: [&[u8]; 6] = [
    include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-Italic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf"),
];

// A fixed catalog and fallback list: never discover machine fonts or locale.
struct BundledFallback;
impl cosmic_text::Fallback for BundledFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &FAMILIES
    }
    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }
    fn script_fallback(&self, _: unicode_script::Script, _: &str) -> &[&'static str] {
        &[]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextRequest<'a> {
    pub text: &'a str,
    pub font_size: f32,
    pub width: f32,
    pub family: &'a str,
    pub center: bool,
    pub bold: bool,
    pub italic: bool,
}
impl<'a> TextRequest<'a> {
    pub fn new(text: &'a str, font_size: f32, width: f32) -> Self {
        Self {
            text,
            font_size,
            width,
            family: FAMILIES[0],
            center: false,
            bold: false,
            italic: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub font: String,
    pub glyph_id: u16,
    pub source_line: usize,
    pub visual_line: usize,
    /// UTF-8 byte offsets within the original `source_line`, not the full layout text.
    pub cluster: std::ops::Range<usize>,
    pub x: f32,
    pub baseline: f32,
    pub advance: f32,
    pub rtl: bool,
    /// Unhinted outline in layout coordinates, Y down, already positioned.
    pub outline: Option<Path>,
}

#[derive(Clone, Debug)]
pub struct TextLayout {
    text: String,
    glyphs: Vec<Glyph>,
    diagnostics: Vec<String>,
    line_count: usize,
    height: f32,
}
impl TextLayout {
    pub(crate) fn rotate_counterclockwise(&mut self) {
        let transform = tiny_skia::Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, 0.0);
        for glyph in &mut self.glyphs {
            glyph.outline = glyph
                .outline
                .take()
                .and_then(|path| path.transform(transform));
        }
    }
    fn glyph_fits(glyph: &Glyph, width: f32, height: f32) -> bool {
        glyph.outline.as_ref().is_none_or(|p| {
            let b = p.bounds();
            b.left() >= 0.0 && b.top() >= 0.0 && b.right() <= width && b.bottom() <= height
        })
    }
    pub(crate) fn overflows(&self, width: f32, height: f32) -> bool {
        self.height > height
            || self
                .glyphs
                .iter()
                .any(|glyph| !Self::glyph_fits(glyph, width, height))
    }
    pub(crate) fn clip_to_bounds(&mut self, width: f32, height: f32) -> bool {
        let before = self.glyphs.len();
        self.glyphs
            .retain(|glyph| Self::glyph_fits(glyph, width, height));
        before != self.glyphs.len() || self.height > height
    }
    /// Keep the full source text (SVG description) for an ellipsized layout.
    pub(crate) fn set_source_text(&mut self, text: &str) {
        self.text = text.to_owned();
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
    pub fn line_count(&self) -> usize {
        self.line_count
    }
    pub fn height(&self) -> f32 {
        self.height
    }
}

pub struct TextEngine {
    fonts: FontSystem,
}
impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}
impl TextEngine {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        for bytes in FONTS {
            db.load_font_data(bytes.to_vec());
        }
        db.set_sans_serif_family(FAMILIES[0]);
        Self {
            fonts: FontSystem::new_with_locale_and_db_and_fallback(
                "en-US".into(),
                db,
                BundledFallback,
            ),
        }
    }

    /// At most 16 KiB UTF-8; 4..=256 px fonts; 4..=16384 logical-pixel width.
    /// Word-or-glyph wrapping and advanced shaping are identical for every sink.
    pub fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, RenderError> {
        if request.text.len() > 16 * 1024 || request.family.len() > 128 {
            return Err(RenderError(
                "Text exceeds the 16 KiB label or 128-byte family limit",
            ));
        }
        if !request.font_size.is_finite()
            || !(4.0..=256.0).contains(&request.font_size)
            || !request.width.is_finite()
            || !(4.0..=16384.0).contains(&request.width)
        {
            return Err(RenderError(
                "Use finite font size 4..256 and label width 4..16384",
            ));
        }
        let mut diagnostics = Vec::new();
        let family = if FAMILIES.contains(&request.family) {
            request.family
        } else {
            diagnostics.push(format!(
                "Missing font '{}'; deterministic fallback to Noto Sans",
                request.family
            ));
            FAMILIES[0]
        };
        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(request.font_size, request.font_size * 1.6),
        );
        buffer.set_hinting(cosmic_text::Hinting::Disabled);
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_size(Some(request.width), None);
        buffer.set_text(
            request.text,
            &Attrs::new()
                .family(Family::Name(family))
                .weight(if request.bold {
                    cosmic_text::Weight::BOLD
                } else {
                    cosmic_text::Weight::NORMAL
                })
                .style(if request.italic {
                    cosmic_text::Style::Italic
                } else {
                    cosmic_text::Style::Normal
                }),
            Shaping::Advanced,
            None,
        );
        if request.center {
            for line in &mut buffer.lines {
                line.set_align(Some(Align::Center));
            }
        }
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut glyphs = Vec::new();
        let mut line_count = 0;
        let mut height: f32 = 0.0;
        let mut segments = 0;
        for run in buffer.layout_runs() {
            line_count += 1;
            height = height.max(run.line_top + run.line_height);
            if height > 1_000_000.0 {
                return Err(RenderError("Laid-out label exceeds height limit"));
            }
            for g in run.glyphs {
                if glyphs.len() >= 32768 {
                    return Err(RenderError("Label exceeds glyph limit"));
                }
                let face = self
                    .fonts
                    .db()
                    .face(g.font_id)
                    .ok_or(RenderError("Bundled face missing"))?;
                let name = face.families[0].0.clone();
                if (request.bold && face.weight.0 < 600)
                    || (request.italic && face.style == fontdb::Style::Normal)
                {
                    let message = format!(
                        "Requested font style is unavailable for {name}; using closest bundled face"
                    );
                    if !diagnostics.contains(&message) {
                        diagnostics.push(message);
                    }
                }
                let index = face.index;
                if name != family {
                    let message = format!("Fallback font: {family} -> {name}");
                    if !diagnostics.contains(&message) {
                        diagnostics.push(message);
                    }
                }
                if g.glyph_id == 0 {
                    let codes = run
                        .text
                        .get(g.start..g.end)
                        .unwrap_or("")
                        .chars()
                        .map(|c| format!("U+{:04X}", c as u32))
                        .collect::<Vec<_>>()
                        .join(" ");
                    diagnostics.push(format!("Missing glyph: {codes}; visible .notdef outline from {name}; no catalog font covers this cluster"));
                }
                let x = g.x + g.font_size * g.x_offset;
                let baseline = run.line_y + g.y - g.font_size * g.y_offset;
                let font = self
                    .fonts
                    .get_font(g.font_id, g.font_weight)
                    .ok_or(RenderError("Bundled font unavailable"))?;
                let font_ref = FontRef::from_index(font.data(), index)
                    .map_err(|_| RenderError("Invalid bundled font"))?;
                let mut pen = Pen {
                    path: PathBuilder::new(),
                    x,
                    y: baseline,
                };
                if let Some(outline) = font_ref
                    .outline_glyphs()
                    .get(GlyphId::new(g.glyph_id as u32))
                {
                    outline
                        .draw(
                            DrawSettings::unhinted(Size::new(g.font_size), LocationRef::default()),
                            &mut pen,
                        )
                        .map_err(|_| RenderError("Bundled glyph outline failed"))?;
                }
                let outline = pen.path.finish();
                segments += outline.as_ref().map_or(0, |p| p.segments().count());
                if segments > 1_000_000 {
                    return Err(RenderError("Label exceeds outline complexity limit"));
                }
                glyphs.push(Glyph {
                    font: name,
                    glyph_id: g.glyph_id,
                    source_line: run.line_i,
                    visual_line: line_count - 1,
                    cluster: g.start..g.end,
                    x,
                    baseline,
                    advance: g.w,
                    rtl: g.level.is_rtl(),
                    outline,
                });
            }
        }
        Ok(TextLayout {
            text: request.text.to_owned(),
            glyphs,
            diagnostics,
            line_count,
            height,
        })
    }
}

struct Pen {
    path: PathBuilder,
    x: f32,
    y: f32,
}
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.x + x, self.y - y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.x + x, self.y - y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path
            .quad_to(self.x + x1, self.y - y1, self.x + x, self.y - y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.cubic_to(
            self.x + x1,
            self.y - y1,
            self.x + x2,
            self.y - y2,
            self.x + x,
            self.y - y,
        );
    }
    fn close(&mut self) {
        self.path.close();
    }
}
