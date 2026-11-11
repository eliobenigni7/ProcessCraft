//! Reproduce supporting-catalog codec evidence without loading schemas or services.
use processcraft_core::model::ProjectData;
use processcraft_io::{interchange, native};
use std::{path::PathBuf, sync::Arc};

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/m3-qualification/catalog"));
    let input = include_bytes!("../../../tests/fixtures/bpmn/supporting-catalog.bpmn");
    let imported = interchange::prepare_import(input, "Catalog fixture").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    let document_id = imported.document.id;
    let catalog = imported.document.catalog.clone();
    let mut project = ProjectData::new("Catalog qualification".into());
    project.documents.push(Arc::new(imported.document));
    project.resources = imported.resources;
    project.validate().unwrap();
    let identity = project.content_identity().unwrap();
    let package = native::write_bytes(&project).unwrap();
    let reopened = native::load_bytes(&package).unwrap().project;
    assert_eq!(reopened.content_identity().unwrap(), identity);
    assert_eq!(reopened.documents[0].catalog, catalog);
    let exported = interchange::export_bpmn(&reopened, document_id, true).unwrap();
    let reimported =
        interchange::prepare_import(&exported.bytes, "Reimported catalog fixture").unwrap();
    assert!(reimported.report.editable);
    assert_eq!(reimported.document.catalog, catalog);
    // Codec artifacts do not qualify Windows file-session persistence or WSDL execution.
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("input.bpmn"), input).unwrap();
    std::fs::write(output.join("output.bpmn"), exported.bytes).unwrap();
    std::fs::write(output.join("fixture.processcraft"), package).unwrap();
    println!(
        "Supporting catalog native and BPMN codec checks passed; artifacts: {}",
        output.display()
    );
}
