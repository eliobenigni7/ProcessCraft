use processcraft_core::bpmn_data::{DataAssociationKind, DataStoreRef};
use processcraft_core::bpmn_model::NodeDetails;
use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{ExportedBpmn, PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

fn export(b: &PreparedImport) -> ExportedBpmn {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap()
}

const FIXTURE: &str = include_str!("../../../tests/fixtures/bpmn/data/data-attributes.bpmn");

fn item(r: &Option<processcraft_core::bpmn_data::ItemAwareMetadata>) -> Option<(String, String)> {
    r.as_ref()
        .and_then(|i| i.item_subject_ref.as_ref())
        .map(|q| (q.namespace.clone(), q.local.to_string()))
}
fn sel(v: &[processcraft_core::ids::ElementSelector]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn check(parsed: &PreparedImport) {
    let doc = &parsed.document;
    let store = &doc.data_stores[0];
    assert_eq!(store.name.as_deref(), Some("Ledger"));
    assert_eq!(store.capacity.as_deref(), Some("500"));
    assert_eq!(store.is_unlimited, Some(false));
    assert_eq!(
        item(&store.item_aware),
        Some(("urn:m3-data".into(), "item".into()))
    );

    let p = &doc.processes[0];
    assert_eq!(p.data_objects[0].is_collection, Some(true));
    assert_eq!(
        item(&p.data_objects[0].item_aware),
        Some(("urn:m3-data".into(), "item".into()))
    );
    let sref = p
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "storeRef")
        .unwrap();
    assert_eq!(
        item(&sref.item_aware),
        Some(("urn:m3-data".into(), "item".into()))
    );
    let Some(NodeDetails::DataStoreReference {
        data_store_ref: Some(DataStoreRef::Qualified(q)),
    }) = &sref.details
    else {
        panic!("qualified dataStoreRef expected")
    };
    assert_eq!(
        (q.namespace.as_str(), q.local.to_string()),
        ("urn:m3-data", "store".into())
    );

    let io = p
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "work")
        .unwrap()
        .io_specification
        .as_ref()
        .unwrap();
    let in1 = &io.inputs[0];
    assert_eq!(in1.name.as_deref(), Some("Primary"));
    assert_eq!(in1.is_collection, Some(true));
    assert_eq!(
        item(&in1.item_aware),
        Some(("urn:m3-data".into(), "item".into()))
    );
    assert_eq!(io.inputs[1].is_collection, None);
    let out1 = &io.outputs[0];
    assert_eq!(out1.name.as_deref(), Some("Result"));
    assert_eq!(out1.is_collection, Some(true));
    assert_eq!(
        item(&out1.item_aware),
        Some(("urn:m3-data".into(), "item".into()))
    );

    let (is, os) = (&io.input_sets[0], &io.output_sets[0]);
    assert_eq!(is.name.as_deref(), Some("Main"));
    assert_eq!(sel(&is.data_refs), ["in1"]);
    assert_eq!(sel(&is.optional_refs), ["in2"]);
    assert_eq!(sel(&is.while_executing_refs), ["in3"]);
    assert_eq!(sel(&is.counterpart_refs), ["oset1"]);
    assert_eq!(os.name.as_deref(), Some("Done"));
    assert_eq!(sel(&os.data_refs), ["out1"]);
    assert_eq!(sel(&os.optional_refs), ["out2"]);
    assert_eq!(sel(&os.while_executing_refs), ["out3"]);
    assert_eq!(sel(&os.counterpart_refs), ["set1"]);

    let a_in = p
        .data_associations
        .iter()
        .find(|a| a.kind == DataAssociationKind::Input)
        .unwrap();
    assert_eq!(
        (sel(&a_in.sources), a_in.target.to_string()),
        (vec!["objectRef".to_string()], "in1".into())
    );
    let a_out = p
        .data_associations
        .iter()
        .find(|a| a.kind == DataAssociationKind::Output)
        .unwrap();
    assert_eq!(
        (sel(&a_out.sources), a_out.target.to_string()),
        (vec!["out1".to_string()], "storeRef".into())
    );
}

#[test]
fn data_attributes_import_with_values_and_export_reimports_equal() {
    let parsed = prepare_import(FIXTURE.as_bytes(), "m3 data").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    check(&parsed);
    let out = export(&parsed);
    // consumed by scripts/check_bpmn20_xsd.py (see tests/fixtures/bpmn/data/manifest.md)
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join("m3-data-export.bpmn"), &out.bytes).unwrap();
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    check(&again);
    assert_eq!(again.document.data_stores, parsed.document.data_stores);
    assert_eq!(
        again.document.processes[0].nodes,
        parsed.document.processes[0].nodes
    );
    assert_eq!(
        again.document.processes[0].data_associations,
        parsed.document.processes[0].data_associations
    );
    assert_eq!(
        again.document.processes[0].data_objects,
        parsed.document.processes[0].data_objects
    );
}
