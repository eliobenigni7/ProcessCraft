use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{ChoreographyFlowElement, ChoreographyLoopType, ChoreographyRootKind},
};
use processcraft_io::interchange::prepare_import;

const VALUES: &str = include_str!("../../../tests/fixtures/bpmn/m3-choreography/values.bpmn");

fn local(s: &str) -> ModelReference {
    ModelReference::Local(s.parse().unwrap())
}

#[test]
fn global_choreography_initiator_imports_exact_value_and_absence() {
    let imported = prepare_import(VALUES.as_bytes(), "values").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    assert_eq!(
        imported.document.choreographies[1].kind,
        Some(ChoreographyRootKind::Global {
            initiating_participant_ref: Some(local("globalSeller"))
        })
    );
    let absent = VALUES.replace(" initiatingParticipantRef=\"t:globalSeller\"", "");
    assert_ne!(absent, VALUES);
    let imported = prepare_import(absent.as_bytes(), "absent").unwrap();
    assert!(
        imported.report.editable,
        "{:?}",
        imported.report.limitations
    );
    assert_eq!(
        imported.document.choreographies[1].kind,
        Some(ChoreographyRootKind::Global {
            initiating_participant_ref: None
        })
    );
}

#[test]
fn choreography_loop_type_accepts_each_enum_value_and_refuses_others() {
    for (text, expected) in [
        ("None", ChoreographyLoopType::None),
        ("Standard", ChoreographyLoopType::Standard),
        (
            "MultiInstanceSequential",
            ChoreographyLoopType::MultiInstanceSequential,
        ),
        (
            "MultiInstanceParallel",
            ChoreographyLoopType::MultiInstanceParallel,
        ),
    ] {
        let source = VALUES.replace(
            "loopType=\"MultiInstanceSequential\"",
            &format!("loopType=\"{text}\""),
        );
        let imported = prepare_import(source.as_bytes(), text).unwrap();
        assert!(imported.report.editable, "{text}");
        let ChoreographyFlowElement::Activity(task) = &imported.document.choreographies[0]
            .graph
            .as_ref()
            .unwrap()
            .flow_elements[0]
        else {
            panic!("task expected")
        };
        assert_eq!(task.loop_type, Some(expected), "{text}");
    }
    // The only attributable difference from the editable control above is the out-of-enum value.
    let bogus = VALUES.replace("loopType=\"MultiInstanceSequential\"", "loopType=\"Bogus\"");
    let imported = prepare_import(bogus.as_bytes(), "bogus").unwrap();
    assert!(!imported.report.editable);
    // The refusal now names the attribute and the offending value.
    assert_eq!(imported.report.limitations.len(), 2);
    assert!(imported.report.limitations[0].starts_with("Unsupported or ambiguous content"));
    assert!(imported.report.limitations[1].contains("loopType \"Bogus\""));
    assert_eq!(&*imported.resources[0].bytes, bogus.as_bytes());
}
