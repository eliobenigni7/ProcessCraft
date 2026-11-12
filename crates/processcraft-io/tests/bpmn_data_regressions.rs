//! Repair evidence (export) for ioSpecification and data associations.
use processcraft_core::bpmn_data::DataAssociationKind;
use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &str =
    include_str!("../../../tests/fixtures/bpmn/data-regressions/data-repair.bpmn");

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
    // consumed by scripts/check_bpmn20_xsd.py (see tests/fixtures/bpmn/data-regressions/manifest.md)
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

/// bpmn.io writes no ioSpecification: an input association targets the activity's
/// `__targetRef_placeholder` property, an output association has no sourceRef, and a
/// Camunda-style calledElement names a process key that is not defined in the file.
const BPMN_IO_STYLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="Process_1">
    <bpmn:task id="Task_1" name="Check"><bpmn:property id="Property_1" name="__targetRef_placeholder" /><bpmn:dataInputAssociation id="DIA_1"><bpmn:sourceRef>DOR1</bpmn:sourceRef><bpmn:targetRef>Property_1</bpmn:targetRef></bpmn:dataInputAssociation></bpmn:task>
    <bpmn:task id="Task_2" name="Write"><bpmn:dataOutputAssociation id="DOA_1"><bpmn:targetRef>DOR1</bpmn:targetRef></bpmn:dataOutputAssociation></bpmn:task>
    <bpmn:callActivity id="Call_1" name="Call" calledElement="Other" />
    <bpmn:dataObjectReference id="DOR1" name="Invoice" dataObjectRef="DO1" /><bpmn:dataObject id="DO1" />
  </bpmn:process>
</bpmn:definitions>"#;

#[test]
fn bpmn_io_associations_and_external_called_element_import_editable_and_round_trip() {
    let parsed = prepare_import(BPMN_IO_STYLE.as_bytes(), "bpmn.io").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let process = &parsed.document.processes[0];
    assert_eq!(process.data_associations.len(), 2);
    assert!(process.nodes.iter().all(|n| n.io_specification.is_none()));
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(parsed.document.clone()));
    p.resources = parsed.resources.clone();
    let out = export_bpmn(&p, parsed.document.id, false).unwrap();
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    assert!(!xml.contains("<ioSpecification"), "{xml}");
    for needle in [
        "name=\"__targetRef_placeholder\"",
        "<targetRef>Property_1</targetRef>",
        "<dataOutputAssociation id=\"DOA_1\"><targetRef>DOR1</targetRef>",
        "calledElement=\"Other\"",
    ] {
        assert!(xml.contains(needle), "missing {needle} in export:\n{xml}");
    }
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    let (a, b) = (&again.document.processes[0], &parsed.document.processes[0]);
    assert_eq!(a.data_associations, b.data_associations);
    assert_eq!(a.nodes, b.nodes);
}

#[test]
fn input_association_without_target_ref_stays_preserved_source() {
    // XSD requires targetRef on every data association: nothing to edit, so keep bytes.
    let xml = BPMN_IO_STYLE.replace("<bpmn:targetRef>Property_1</bpmn:targetRef>", "");
    let parsed = prepare_import(xml.as_bytes(), "no target").unwrap();
    assert!(!parsed.report.editable);
}
