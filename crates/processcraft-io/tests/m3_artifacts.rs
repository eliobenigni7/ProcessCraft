use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_model::{AssociationDirection, NodeDetails},
    model::{FlowKind, NodeKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/m3-artifacts/artifacts-all.bpmn");

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn export(b: &PreparedImport) -> Vec<u8> {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap().bytes
}

#[test]
fn artifacts_import_typed_association_category_and_annotation_fields() {
    let b = prepare_import(FIXTURE, "m3 artifacts").unwrap();
    assert!(b.report.editable);
    let process = &b.document.processes[0];
    for (name, source, target, direction) in [
        ("a_one", "work", "note", AssociationDirection::One),
        ("a_both", "note", "work", AssociationDirection::Both),
        ("a_none", "work", "note", AssociationDirection::None),
    ] {
        let f = process
            .flows
            .iter()
            .find(|f| f.id == id::<processcraft_core::ids::XmlId>(name))
            .unwrap();
        assert_eq!(f.kind, FlowKind::Association);
        assert_eq!(f.source, Some(ModelReference::Local(id(source))));
        assert_eq!(f.target, Some(ModelReference::Local(id(target))));
        assert_eq!(f.association_direction, Some(direction));
    }
    assert_eq!(b.document.categories.len(), 1);
    let category = &b.document.categories[0];
    assert_eq!(category.name.as_deref(), Some("Stage"));
    assert_eq!(category.values.len(), 1);
    assert_eq!(category.values[0].value.as_deref(), Some("Review"));
    let note = process
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::TextAnnotation)
        .unwrap();
    match &note.details {
        Some(NodeDetails::TextAnnotation { text_format, .. }) => {
            assert_eq!(text_format.as_deref(), Some("text/plain"))
        }
        other => panic!("unexpected details {other:?}"),
    }
}

#[test]
fn artifacts_export_text_annotation_and_association_reimport_equal() {
    let b = prepare_import(FIXTURE, "m3 artifacts").unwrap();
    let bytes = export(&b);
    // Written for the external XSD check (scripts/check_bpmn20_xsd.py).
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("m3-artifacts-export.bpmn");
    std::fs::write(&out, &bytes).unwrap();
    let xml = String::from_utf8(bytes).unwrap();
    assert!(xml.contains("<textAnnotation"), "{xml}");
    assert!(xml.contains("textFormat=\"text/plain\""), "{xml}");
    assert!(xml.contains("Check &amp; approve"), "{xml}");
    for dir in ["One", "Both", "None"] {
        assert!(
            xml.contains(&format!("associationDirection=\"{dir}\"")),
            "{xml}"
        );
    }
    let again = prepare_import(xml.as_bytes(), "again").unwrap();
    assert!(again.report.editable);
    assert_eq!(
        again.document.processes[0].nodes,
        b.document.processes[0].nodes
    );
    assert_eq!(
        again.document.processes[0].flows,
        b.document.processes[0].flows
    );
    assert_eq!(again.document.categories, b.document.categories);
}
