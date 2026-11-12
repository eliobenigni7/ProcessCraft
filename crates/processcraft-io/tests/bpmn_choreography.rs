use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, ChoreographyLoopType,
        ChoreographyRootKind,
    },
    model::ProjectData,
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const VALUES: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/choreography/values.bpmn");
const BAD: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/choreography/bad-references.bpmn");

fn local(s: &str) -> ModelReference {
    ModelReference::Local(s.parse().unwrap())
}
fn re_export(b: &PreparedImport) -> PreparedImport {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    let out = export_bpmn(&p, b.document.id, false).unwrap();
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    again
}
fn check(b: &PreparedImport) {
    let main = &b.document.choreographies[0];
    let elements = &main.graph.as_ref().unwrap().flow_elements;
    let (ChoreographyFlowElement::Activity(task), ChoreographyFlowElement::Activity(call)) =
        (&elements[0], &elements[1])
    else {
        panic!("task and call expected")
    };
    assert_eq!(task.initiating_participant, Some(local("seller")));
    assert_eq!(
        task.participant_refs,
        vec![local("seller"), local("carrier")]
    );
    assert_eq!(task.message_flow_refs, vec![local("ship")]);
    assert_eq!(
        task.loop_type,
        Some(ChoreographyLoopType::MultiInstanceSequential)
    );
    let multiplicity = main.participants[2].multiplicity.as_ref().unwrap();
    assert_eq!(
        (multiplicity.minimum, multiplicity.maximum),
        (Some(2), Some(5))
    );
    assert_eq!(call.initiating_participant, Some(local("buyer")));
    assert!(
        matches!(&call.details, Some(ChoreographyActivityDetails::Call { called_choreography_ref: Some(r), .. }) if *r == local("global"))
    );
    assert!(
        matches!(&b.document.choreographies[1].kind, Some(ChoreographyRootKind::Global { initiating_participant_ref: Some(r) }) if *r == local("globalSeller"))
    );
}

#[test]
fn choreography_field_values_import_and_export_reimport_equal() {
    let imported = prepare_import(VALUES, "values").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    check(&imported);
    // export re-import preserves every field value (the exported XML is XSD-checked by
    // scripts/check_bpmn20_xsd.py in the qualification run).
    let again = re_export(&imported);
    check(&again);
    assert_eq!(
        again.document.choreographies.len(),
        imported.document.choreographies.len()
    );
}

#[test]
fn wrong_type_call_target_and_foreign_global_initiator_stay_preserved_source() {
    let imported = prepare_import(BAD, "bad").unwrap();
    assert!(!imported.report.editable);
    assert!(matches!(
        imported.document.capability,
        processcraft_core::model::Capability::PreservedSource { .. }
    ));
    assert_eq!(&*imported.resources[0].bytes, BAD);
}

#[test]
fn unresolved_local_call_target_is_refused_on_import() {
    let source = std::str::from_utf8(BAD).unwrap().replace(
        "calledChoreographyRef=\"t:seller\"",
        "calledChoreographyRef=\"t:absent\"",
    );
    assert!(source.contains("t:absent"));
    assert!(prepare_import(source.as_bytes(), "absent target").is_err());
}

#[test]
fn out_of_enum_choreography_loop_type_is_not_imported_as_editable() {
    let source = std::str::from_utf8(VALUES)
        .unwrap()
        .replace("loopType=\"MultiInstanceSequential\"", "loopType=\"Bogus\"");
    assert!(source.contains("Bogus"));
    let imported = prepare_import(source.as_bytes(), "bogus loop").unwrap();
    assert!(!imported.report.editable);
    assert_eq!(&*imported.resources[0].bytes, source.as_bytes());
    // The refusal names the attribute and the offending value.
    assert!(
        imported
            .report
            .limitations
            .iter()
            .any(|l| l.contains("loopType \"Bogus\"") && l.contains("tChoreographyLoopType")),
        "{:?}",
        imported.report.limitations
    );
    // The unmodified fixture is editable, so the refusal is attributable to the value.
    let ok = prepare_import(VALUES, "values").unwrap();
    assert!(ok.report.editable, "{:?}", ok.report.limitations);
}
