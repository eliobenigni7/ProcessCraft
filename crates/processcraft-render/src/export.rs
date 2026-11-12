//! In-memory full-diagram export. File capabilities remain the caller's responsibility.
use crate::{Appearance, BpmnScene, RenderError, TextEngine};
use processcraft_core::{
    ids::ElementSelector,
    model::{BpmnDocument, Capability},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Png,
    Pdf,
}
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub appearance: Appearance,
    pub scale: f32,
    /// Explicitly accept diagnostics such as omitted geometry or clipped labels.
    pub acknowledge_diagnostics: bool,
}
#[derive(Debug)]
pub struct ExportedDiagram {
    pub bytes: Vec<u8>,
    pub origin: (f64, f64),
    pub logical_size: (f32, f32),
    pub pixel_size: (u32, u32),
    pub diagnostics: Vec<String>,
}
/// Collect the exact diagnostics that export will require the user to acknowledge.
pub fn preflight_diagram(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    appearance: Appearance,
    fonts: &mut TextEngine,
    family: &str,
) -> Result<Vec<String>, RenderError> {
    if !matches!(document.capability, Capability::Editable) {
        return Err(RenderError(
            "Document does not have a qualified editable scene",
        ));
    }
    Ok(BpmnScene::build(document, diagram_id, appearance, fonts, family)?.diagnostics)
}
/// Build the same complete scene for SVG and PNG, without editor overlays.
pub fn export_diagram(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    options: ExportOptions,
    fonts: &mut TextEngine,
    family: &str,
) -> Result<ExportedDiagram, RenderError> {
    if !matches!(document.capability, Capability::Editable) {
        return Err(RenderError(
            "Document does not have a qualified editable scene",
        ));
    }
    if !options.scale.is_finite() || !(0.125..=8.0).contains(&options.scale) {
        return Err(RenderError("Export scale must be finite within 0.125..8"));
    }
    let mut built = BpmnScene::build(document, diagram_id, options.appearance, fonts, family)?;
    if options.format == ExportFormat::Pdf {
        built.diagnostics.push(crate::pdf::OUTLINE_NOTICE.into());
    }
    if !options.acknowledge_diagnostics && !built.diagnostics.is_empty() {
        return Err(RenderError("Export diagnostics require acknowledgement"));
    }
    let width = (built.scene.width() * options.scale).ceil() as u32;
    let height = (built.scene.height() * options.scale).ceil() as u32;
    if options.format != ExportFormat::Pdf
        && (width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024)
    {
        return Err(RenderError(
            "Export exceeds 16384-pixel axis or 64 MiB allocation limit",
        ));
    }
    let bytes = match options.format {
        ExportFormat::Png => built
            .scene
            .rasterize(options.scale)?
            .encode_png()
            .map_err(|_| RenderError("PNG encoding failed"))?,
        ExportFormat::Svg => {
            let svg = built.scene.to_svg();
            let old = format!(
                "width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
                built.scene.width(),
                built.scene.height(),
                built.scene.width(),
                built.scene.height()
            );
            let new = format!(
                "width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><g transform=\"scale({})\">",
                options.scale
            );
            // Reported after the acknowledgement gate: nothing is lost relative to
            // the scene, so this is information rather than a lossy diagnostic.
            built.diagnostics.push(crate::scene::SVG_TEXT_NOTICE.into());
            svg.replacen(&old, &new, 1)
                .replacen("</svg>", "</g></svg>", 1)
                .into_bytes()
        }
        ExportFormat::Pdf => {
            let (bytes, pages) = crate::pdf::export(&built.scene, options.scale)?;
            built.diagnostics.push(if pages == 1 {
                "PDF uses 1 A4 landscape page with 24-point margins; a diagram that fits at half the selected scale or more is shrunk onto that page.".into()
            } else {
                format!("PDF uses {pages} A4 landscape page(s), tiled at the selected scale with 24-point margins. Page boundaries can split shapes and labels; reduce scale for fewer pages.")
            });
            bytes
        }
    };
    Ok(ExportedDiagram {
        bytes,
        origin: built.origin,
        logical_size: (built.scene.width(), built.scene.height()),
        pixel_size: (width, height),
        diagnostics: built.diagnostics,
    })
}
