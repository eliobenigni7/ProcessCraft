use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/activity-callable-roles.bpmn");
const HUGE: &str = "123456789012345678901234567890";

fn export(bytes: &[u8]) -> String {
    let imp = prepare_import(bytes, "loops").unwrap();
    assert!(imp.report.editable);
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(imp.document.clone()));
    p.resources = imp.resources.clone();
    let out = export_bpmn(&p, imp.document.id, false).unwrap();
    String::from_utf8(out.bytes.to_vec()).unwrap()
}

/// Exported standardLoopCharacteristics carries testBefore, loopMaximum and loopCondition values
/// (arbitrary-precision loopMaximum is kept verbatim).
#[test]
fn standard_loop_export_writes_test_before_maximum_and_condition() {
    let xml = export(FIXTURE);
    assert!(xml.contains("standardLoopCharacteristics"));
    assert!(xml.contains("testBefore=\"true\""), "{xml}");
    assert!(xml.contains(&format!("loopMaximum=\"{HUGE}\"")));
    assert!(xml.contains(">again</"));
}

/// Same export with an XSD-checkable loopMaximum (libxml2 rejects 30-digit xs:integer); the bytes
/// are written for `python scripts/check_bpmn20_xsd.py target/tmp/m3-repair-loops-export.bpmn`.
#[test]
fn standard_loop_export_bytes_are_written_for_xsd_check() {
    let src = std::str::from_utf8(FIXTURE).unwrap().replace(HUGE, "12345");
    let xml = export(src.as_bytes());
    assert!(xml.contains("loopMaximum=\"12345\"") && xml.contains("testBefore=\"true\""));
    std::fs::create_dir_all("../../target/tmp").unwrap();
    std::fs::write("../../target/tmp/m3-repair-loops-export.bpmn", xml).unwrap();
}
