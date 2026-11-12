//! Regenerate event and diagram interoperability evidence in an explicit directory.
use processcraft_core::model::ProjectData;
use processcraft_io::{
    interchange::{export_bpmn, prepare_import},
    native::{load_bytes, write_bytes},
};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    if !output.is_absolute() {
        return Err("output directory must be absolute".into());
    }
    std::fs::create_dir_all(&output)?;
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bpmn");
    for name in [
        "event-ownership",
        "di-endpoints",
        "di-label-styles",
        "process-documentation",
    ] {
        let input = std::fs::read(fixture_dir.join(format!("{name}.bpmn")))?;
        let parsed = prepare_import(&input, name)?;
        if !parsed.report.editable {
            return Err(format!(
                "{name}: source not editable: {:?}",
                parsed.report.limitations
            )
            .into());
        }
        let mut project = ProjectData::new("Event qualification".into());
        project.documents.push(Arc::new(parsed.document.clone()));
        project.resources = parsed.resources;
        let before = project.content_identity()?;
        let archive = write_bytes(&project)?;
        let reopened = load_bytes(&archive)?.project;
        if reopened.content_identity()? != before {
            return Err(format!("{name}: native identity changed").into());
        }
        let output_bpmn = export_bpmn(&reopened, parsed.document.id, false)
            .map_err(|e| format!("{name}: {:?}", e.report.blocked))?;
        std::fs::write(output.join(format!("{name}.bpmn")), &output_bpmn.bytes)?;
        std::fs::write(output.join(format!("{name}.processcraft")), archive)?;
        println!("{name}: native identity preserved and BPMN exported");
    }
    Ok(())
}
