//! Repair tests for the artifacts family: strong value assertions on import,
//! on the exported bytes, on re-import, and an XSD run over the exported file.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_model::{AssociationDirection, NodeDetails},
    ids::XmlId,
    model::{FlowKind, NodeKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/artifacts/artifacts-all.bpmn");

fn xid(s: &str) -> XmlId {
    s.parse().unwrap()
}

fn sel(s: &str) -> processcraft_core::ids::ElementSelector {
    s.parse().unwrap()
}

fn export(b: &PreparedImport) -> Vec<u8> {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap().bytes
}

#[test]
fn artifact_kinds_import_with_typed_values() {
    let b = prepare_import(FIXTURE, "artifacts").unwrap();
    assert!(b.report.editable);
    let p = &b.document.processes[0];
    let note = p
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::TextAnnotation)
        .unwrap();
    assert_eq!(note.id.xml_id(), Some(&xid("note")));
    assert!(matches!(
        &note.details,
        Some(NodeDetails::TextAnnotation { text_format, .. }) if text_format.as_deref() == Some("text/plain")
    ));
    let a = p.flows.iter().find(|f| f.id == xid("a_both")).unwrap();
    assert_eq!(a.kind, FlowKind::Association);
    assert_eq!(a.source, Some(ModelReference::Local(sel("note"))));
    assert_eq!(a.target, Some(ModelReference::Local(sel("work"))));
    assert_eq!(a.association_direction, Some(AssociationDirection::Both));
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.groups[0].id.xml_id(), Some(&xid("grp")));
    assert_eq!(
        p.groups[0].category_value_ref,
        Some(ModelReference::Local(sel("catval")))
    );
}

#[test]
fn artifacts_export_values_are_in_bytes_reimport_and_xsd_valid() {
    let b = prepare_import(FIXTURE, "artifacts").unwrap();
    let bytes = export(&b);
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("m3-repair-artifacts.bpmn");
    std::fs::write(&out, &bytes).unwrap();
    let xml = String::from_utf8(bytes).unwrap();

    assert!(xml.contains("<category "), "{xml}");
    assert!(xml.contains("name=\"Stage\""), "{xml}");
    assert!(xml.contains("<categoryValue "), "{xml}");
    assert!(xml.contains("value=\"Review\""), "{xml}");
    assert!(xml.contains("<group "), "{xml}");
    assert!(
        xml.contains("categoryValueRef=\"pc_cref0:catval\""),
        "{xml}"
    );
    assert!(xml.contains("<association "), "{xml}");
    assert!(
        xml.contains("sourceRef=\"pc_cref0:work\"") && xml.contains("targetRef=\"pc_cref0:note\""),
        "{xml}"
    );
    assert!(
        xml.contains("sourceRef=\"pc_cref0:note\"") && xml.contains("targetRef=\"pc_cref0:work\""),
        "{xml}"
    );
    for dir in ["One", "Both", "None"] {
        assert!(
            xml.contains(&format!("associationDirection=\"{dir}\"")),
            "{xml}"
        );
    }

    let again = prepare_import(xml.as_bytes(), "again").unwrap();
    assert!(again.report.editable);
    let c = &again.document.categories;
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].name.as_deref(), Some("Stage"));
    assert_eq!(c[0].values.len(), 1);
    assert_eq!(c[0].values[0].value.as_deref(), Some("Review"));
    let p = &again.document.processes[0];
    assert_eq!(p.groups.len(), 1);
    assert_eq!(
        p.groups[0].category_value_ref,
        Some(ModelReference::Local(sel("catval")))
    );
    let a = p.flows.iter().find(|f| f.id == xid("a_one")).unwrap();
    assert_eq!(a.source, Some(ModelReference::Local(sel("work"))));
    assert_eq!(a.target, Some(ModelReference::Local(sel("note"))));
    assert_eq!(a.association_direction, Some(AssociationDirection::One));

    // XSD validity of the exported bytes (skipped only if python is unavailable).
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_bpmn20_xsd.py");
    if let Ok(o) = std::process::Command::new("python")
        .arg(&script)
        .arg(&out)
        .output()
    {
        let s = String::from_utf8_lossy(&o.stdout);
        assert!(
            o.status.success() && s.contains("XSD valid"),
            "{s}{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
}
