//! Qualify typed BPMN data metadata without evaluating transformations or assignments.
use processcraft_core::{model::ProjectData, queries};
use processcraft_io::{interchange, native};
use std::{path::PathBuf, sync::Arc};

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/m3-qualification/data"));
    for (name, input) in [
        (
            "metadata",
            include_bytes!("../../../tests/fixtures/bpmn/data-metadata.bpmn").as_slice(),
        ),
        (
            "optional",
            include_bytes!("../../../tests/fixtures/bpmn/data-optional.bpmn").as_slice(),
        ),
        (
            "root-only",
            include_bytes!("../../../tests/fixtures/bpmn/root-only.bpmn").as_slice(),
        ),
    ] {
        let imported = interchange::prepare_import(input, "Data fixture").unwrap();
        assert!(
            imported.report.editable,
            "{:?}",
            imported.report.limitations
        );
        let id = imported.document.id;
        let mut project = ProjectData::new("Data qualification".into());
        project.documents.push(Arc::new(imported.document));
        project.resources = imported.resources;
        project.validate().unwrap();
        let expected = serde_json::to_value(queries::data_fields(&project, id).unwrap()).unwrap();
        let identity = project.content_identity().unwrap();
        let package = native::write_bytes(&project).unwrap();
        let reopened = native::load_bytes(&package).unwrap().project;
        assert_eq!(reopened.content_identity().unwrap(), identity);
        let exported = interchange::export_bpmn(&reopened, id, true).unwrap();
        let reimported =
            interchange::prepare_import(&exported.bytes, "Reimported data fixture").unwrap();
        assert!(reimported.report.editable);
        let reimported_id = reimported.document.id;
        let mut comparison = ProjectData::new("Comparison".into());
        comparison.documents.push(Arc::new(reimported.document));
        comparison.resources = reimported.resources;
        assert_eq!(
            serde_json::to_value(queries::data_fields(&comparison, reimported_id).unwrap())
                .unwrap(),
            expected
        );
        let directory = output.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("input.bpmn"), input).unwrap();
        std::fs::write(directory.join("output.bpmn"), exported.bytes).unwrap();
        std::fs::write(directory.join("fixture.processcraft"), package).unwrap();
    }
    println!(
        "Data codec identity and field roundtrips passed; artifacts: {}",
        output.display()
    );
}
