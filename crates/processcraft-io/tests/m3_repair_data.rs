//! Repair evidence (export) for ioSpecification and data associations.
use processcraft_core::bpmn_data::DataAssociationKind;
use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../../tests/fixtures/bpmn/m3-repair-data/data-repair.bpmn");

fn sel(v: &[processcraft_core::ids::ElementSelector]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn io_specification_and_data_associations_export_values_and_reimport_equal() {
    let parsed = prepare_import(FIXTURE.as_bytes(), "repair data").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(parsed.document.clone()));
    p.resources = parsed.resources.clone();
    let out = export_bpmn(&p, parsed.document.id, false).unwrap();
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    // consumed by scripts/check_bpmn20_xsd.py (see tests/fixtures/bpmn/m3-repair-data/manifest.md)
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join("m3-repair-data-export.bpmn"), &out.bytes).unwrap();
    for needle in [
        "<ioSpecification",
        "<dataInput ",
        "<dataOutput ",
        "<inputSet ",
        "<outputSet ",
        "<dataInputRefs>in1</dataInputRefs>",
        "<dataOutputRefs>out1</dataOutputRefs>",
        "<outputSetRefs>oset1</outputSetRefs>",
        "<inputSetRefs>set1</inputSetRefs>",
        "<dataInputAssociation",
        "<sourceRef>objectRef</sourceRef>",
        "<targetRef>in1</targetRef>",
        "<dataOutputAssociation",
        "<sourceRef>out1</sourceRef>",
        "<targetRef>storeRef</targetRef>",
    ] {
        assert!(xml.contains(needle), "missing {needle} in export:\n{xml}");
    }
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    let p0 = &again.document.processes[0];
    let io = p0
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "work")
        .unwrap()
        .io_specification
        .as_ref()
        .unwrap();
    assert_eq!(io.inputs.len(), 3);
    assert_eq!(io.outputs.len(), 3);
    assert_eq!(sel(&io.input_sets[0].data_refs), ["in1"]);
    assert_eq!(sel(&io.output_sets[0].counterpart_refs), ["set1"]);
    let a_in = p0
        .data_associations
        .iter()
        .find(|a| a.kind == DataAssociationKind::Input)
        .unwrap();
    assert_eq!(sel(&a_in.sources), ["objectRef"]);
    assert_eq!(a_in.target.to_string(), "in1");
    assert_eq!(
        p0.data_associations,
        parsed.document.processes[0].data_associations
    );
    assert_eq!(p0.nodes, parsed.document.processes[0].nodes);
}
