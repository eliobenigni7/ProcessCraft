//! Qualify activity message/operation bindings as inert model data.
use processcraft_core::model::ProjectData;
use processcraft_io::{interchange, native};
use std::{path::PathBuf, sync::Arc};

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/m3-qualification/activity-bindings"));
    let input = include_bytes!("../../../tests/fixtures/bpmn/activity-bindings.bpmn");
    let imported = interchange::prepare_import(input, "Activity binding fixture").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    let document_id = imported.document.id;
    let processes = serde_json::to_value(&imported.document.processes).unwrap();
    let catalog = imported.document.catalog.clone();
    let mut project = ProjectData::new("Activity binding qualification".into());
    project.documents.push(Arc::new(imported.document));
    project.resources = imported.resources;
    project.validate().unwrap();
    let identity = project.content_identity().unwrap();
    let package = native::write_bytes(&project).unwrap();
    let reopened = native::load_bytes(&package).unwrap().project;
    assert_eq!(reopened.content_identity().unwrap(), identity);
    let exported = interchange::export_bpmn(&reopened, document_id, true).unwrap();
    let reimported =
        interchange::prepare_import(&exported.bytes, "Reimported activity binding fixture")
            .unwrap();
    assert!(reimported.report.editable);
    assert_eq!(reimported.document.catalog, catalog);
    assert_eq!(
        serde_json::to_value(&reimported.document.processes).unwrap(),
        processes
    );
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("input.bpmn"), input).unwrap();
    std::fs::write(output.join("output.bpmn"), exported.bytes).unwrap();
    std::fs::write(output.join("fixture.processcraft"), package).unwrap();
    println!(
        "Activity binding native and BPMN codec checks passed; artifacts: {}",
        output.display()
    );
}
