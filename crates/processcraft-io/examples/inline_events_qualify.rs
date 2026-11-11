//! Qualify local inline Link, Cancel and Compensation data without process execution.
use processcraft_core::model::ProjectData;
use processcraft_io::{interchange, native};
use std::{path::PathBuf, sync::Arc};

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/m3-qualification/inline-events"));
    let input = include_bytes!("../../../tests/fixtures/bpmn/inline-events.bpmn");
    let imported = interchange::prepare_import(input, "Inline event fixture").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    let document_id = imported.document.id;
    let processes = serde_json::to_value(&imported.document.processes).unwrap();
    let mut project = ProjectData::new("Inline event qualification".into());
    project.documents.push(Arc::new(imported.document));
    project.resources = imported.resources;
    project.validate().unwrap();
    let identity = project.content_identity().unwrap();
    let package = native::write_bytes(&project).unwrap();
    let reopened = native::load_bytes(&package).unwrap().project;
    assert_eq!(reopened.content_identity().unwrap(), identity);
    let exported = interchange::export_bpmn(&reopened, document_id, true).unwrap();
    let reimported =
        interchange::prepare_import(&exported.bytes, "Reimported inline event fixture").unwrap();
    assert!(reimported.report.editable);
    assert_eq!(
        serde_json::to_value(&reimported.document.processes).unwrap(),
        processes
    );
    // Codec evidence only: no Windows file-session or execution semantics are exercised.
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("input.bpmn"), input).unwrap();
    std::fs::write(output.join("output.bpmn"), exported.bytes).unwrap();
    std::fs::write(output.join("fixture.processcraft"), package).unwrap();
    println!(
        "Inline event native and BPMN codec checks passed; artifacts: {}",
        output.display()
    );
}
