//! Image export pipeline shared by `export-image` (CLI) and the MCP `export` tool.
use processcraft_core::{
    ids::{AttributeDefinitionId, DocumentId, ElementSelector},
    model::ProjectData,
    overlay::OverlayMode,
};
use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine};
use serde_json::{Value, json};

/// What to render. `overlay` carries an already-parsed definition, or the parse error text.
pub(crate) struct ImageExport<'a> {
    pub document_id: DocumentId,
    pub diagram_id: Option<&'a ElementSelector>,
    pub format: ExportFormat,
    pub dark: bool,
    pub scale: f32,
    pub acknowledge_diagnostics: bool,
    pub overlay: Option<Result<(AttributeDefinitionId, Option<OverlayMode>), String>>,
}

/// Why a render stopped; each transport maps this to its own exit code or JSON status.
pub(crate) enum ExportFailure {
    DocumentNotFound,
    DiagramRequired,
    Overlay(String),
    /// Preflight or rendering failed.
    Render(String),
    AcknowledgementRequired(Vec<String>),
}

/// Rendered bytes plus the report that goes beside them.
pub(crate) fn render_export(
    project: &ProjectData,
    options: ImageExport,
) -> Result<(Vec<u8>, Value), ExportFailure> {
    let document = project
        .documents
        .iter()
        .find(|d| d.id == options.document_id)
        .ok_or(ExportFailure::DocumentNotFound)?;
    let diagram = options.diagram_id.ok_or(ExportFailure::DiagramRequired)?;
    let appearance = if options.dark {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    let overlay = options
        .overlay
        .map(|requested| {
            let (definition, mode) = requested?;
            processcraft_core::overlay::export_overlay(
                project,
                options.document_id,
                definition,
                mode,
            )
        })
        .transpose()
        .map_err(ExportFailure::Overlay)?;
    let mut fonts = TextEngine::new();
    let diagnostics = processcraft_render::preflight_diagram(
        document,
        diagram,
        appearance,
        &mut fonts,
        crate::run::FONT_FAMILY,
    )
    .map_err(|e| ExportFailure::Render(e.to_string()))?;
    if !options.acknowledge_diagnostics && !diagnostics.is_empty() {
        return Err(ExportFailure::AcknowledgementRequired(diagnostics));
    }
    let output = processcraft_render::export_diagram_with_overlay(
        document,
        diagram,
        ExportOptions {
            format: options.format,
            appearance,
            scale: options.scale,
            acknowledge_diagnostics: options.acknowledge_diagnostics,
        },
        &mut fonts,
        crate::run::FONT_FAMILY,
        overlay.as_ref(),
    )
    .map_err(|e| ExportFailure::Render(e.to_string()))?;
    Ok((
        output.bytes,
        json!({"origin":output.origin,"logicalSize":output.logical_size,"pixelSize":output.pixel_size,"diagnostics":output.diagnostics}),
    ))
}
