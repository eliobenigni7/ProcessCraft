//! Toolkit-independent, bounded M0 text and CPU scene spike.
mod bpmn;
mod raster;
mod scene;
mod text;

pub use bpmn::{
    Appearance, BpmnScene, BpmnViewport, HitGeometry, HitRegion, Viewport, contrast_ratio,
    overlay_fill, overlay_ink,
};
pub use scene::{DiagramScene, Label, ScenePath};
pub use text::{Glyph, TextEngine, TextLayout, TextRequest, VisibleLine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(pub &'static str);

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for RenderError {}

mod export;
mod pdf;
pub use export::{
    ExportFormat, ExportOptions, ExportedDiagram, export_diagram, export_diagram_with_overlay,
    preflight_diagram,
};
