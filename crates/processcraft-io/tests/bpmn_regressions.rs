//! Round-3 repair: DI export of BPMNDiagram is checked against the pinned XSD.
use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{export_bpmn, prepare_import};
use std::sync::Arc;

const VALUES: &str = include_str!("../../../tests/fixtures/bpmn/di/di-values.bpmn");

#[test]
fn bpmn_diagram_export_is_xsd_valid_and_carries_diagram_values() {
    let parsed = prepare_import(VALUES.as_bytes(), "v").unwrap();
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(parsed.document.clone()));
    p.resources = parsed.resources.clone();
    let out = export_bpmn(&p, parsed.document.id, false).unwrap();
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    for needle in [
        "id=\"diag\"",
        "name=\"Values\"",
        "documentation=\"Value diagram\"",
        "resolution=\"72\"",
    ] {
        assert!(xml.contains(needle), "missing {needle}\n{xml}");
    }
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("m3-r3-di-export.bpmn");
    std::fs::write(&path, &out.bytes).unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_bpmn20_xsd.py");
    let o = std::process::Command::new("python")
        .arg(&script)
        .arg(&path)
        .output()
        .expect("python is required for the XSD check");
    let s = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success() && s.contains("XSD valid"),
        "{s}{}",
        String::from_utf8_lossy(&o.stderr)
    );
}
