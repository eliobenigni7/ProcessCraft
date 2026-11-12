//! Explicit synthetic BPMN -> native codec -> shared scene qualification.
//! Writes only a new named artifact directory; this is not a qualified file save.
use processcraft_core::model::ProjectData;
use processcraft_io::{interchange, native};
use processcraft_render::{
    Appearance, BpmnScene, ExportFormat, ExportOptions, TextEngine, export_diagram,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(std::env::args_os().nth(1).ok_or("input BPMN required")?);
    let output = PathBuf::from(
        std::env::args_os()
            .nth(2)
            .ok_or("output directory required")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be a new absolute directory".into());
    }
    let bytes = fs::read(&source)?;
    let imported = interchange::prepare_import(&bytes, "Shared scene qualification")?;
    if !imported.report.editable {
        return Err("source is protected; no editable scene is qualified".into());
    }
    let mut project = ProjectData::new("Shared scene qualification".into());
    project.documents.push(Arc::new(imported.document));
    project.resources = imported.resources;
    project.validate()?;
    let before = project.content_identity()?;
    let archive = native::write_bytes(&project)?;
    let reopened = native::load_bytes(&archive)?.project;
    if reopened.content_identity()? != before {
        return Err("Native identity changed".into());
    }
    let document = &reopened.documents[0];
    if document.diagrams.is_empty() {
        return Err("fixture has no diagram".into());
    }
    fs::create_dir(&output)?;
    fs::write(output.join("input.bpmn"), &bytes)?;
    fs::write(output.join("fixture.processcraft"), archive)?;
    let mut reports = vec![];
    let mut fonts = TextEngine::new();
    for (index, diagram) in document.diagrams.iter().enumerate() {
        for (theme, appearance) in [("light", Appearance::Light), ("dark", Appearance::Dark)] {
            let scene = BpmnScene::build(
                document,
                &diagram.id.selector(),
                appearance,
                &mut fonts,
                "Noto Sans",
            )?;
            let centers: Vec<_> = diagram
                .shapes
                .iter()
                .map(|shape| {
                    let x = shape.bounds.x + shape.bounds.width / 2.0;
                    let y = shape.bounds.y + shape.bounds.height / 2.0;
                    json!({"representation":shape.id.selector(),"point":[x,y],
                    "topHit":scene.hit_test(x,y,1.0).map(|hit| &hit.diagram_element_id)})
                })
                .collect();
            for scale in [1.0, 2.0] {
                for (extension, format) in [
                    ("svg", ExportFormat::Svg),
                    ("png", ExportFormat::Png),
                    ("pdf", ExportFormat::Pdf),
                ] {
                    let options = ExportOptions {
                        format,
                        appearance,
                        scale,
                        acknowledge_diagnostics: false,
                    };
                    let unacknowledged = export_diagram(
                        document,
                        &diagram.id.selector(),
                        options,
                        &mut fonts,
                        "Noto Sans",
                    );
                    let exported = export_diagram(
                        document,
                        &diagram.id.selector(),
                        ExportOptions {
                            acknowledge_diagnostics: true,
                            ..options
                        },
                        &mut fonts,
                        "Noto Sans",
                    )?;
                    let file = format!("diagram-{index}-{theme}-{}x.{extension}", scale as u8);
                    fs::write(output.join(&file), &exported.bytes)?;
                    reports.push(
                        json!({"file":file,"sha256":format!("{:x}",Sha256::digest(&exported.bytes)),
                        "diagnostics":exported.diagnostics,"diagnosticsAcknowledged":true,
                        "unacknowledgedRefused":unacknowledged.is_err(),"origin":exported.origin,
                        "logicalSize":exported.logical_size,"pixelSize":exported.pixel_size,
                        "hits":scene.hits.len(),"shapeCenterHits":centers}),
                    );
                }
            }
        }
    }
    if project.content_identity()? != before || reopened.content_identity()? != before {
        return Err("Rendering changed native model identity".into());
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "source":source,"sourceSha256":format!("{:x}",Sha256::digest(&bytes)),
            "nativeIdentityPreserved":true,"exports":reports,
            "limits":"In-memory codecs and explicit synthetic artifact writes; diagnostics acknowledged; no Windows safe-save, accessibility or full interoperability claim."
        }))?,
    )?;
    println!(
        "Native identity preserved; shared SVG/PNG/PDF artifacts: {}",
        output.display()
    );
    Ok(())
}
