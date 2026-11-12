use processcraft_core::{
    bpmn_activity::MultiInstanceFlowCondition,
    bpmn_model::{EventReference, LoopCharacteristics},
    model::ProjectData,
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/activity-callable-roles.bpmn");

fn loop_of(imp: &PreparedImport, node_id: &str) -> LoopCharacteristics {
    imp.document
        .processes
        .iter()
        .flat_map(|p| &p.nodes)
        .find(|n| n.id.to_string() == node_id)
        .and_then(|n| n.loop_characteristics.clone())
        .unwrap_or_else(|| panic!("{node_id} has no typed loop"))
}

fn reimport(imp: &PreparedImport) -> PreparedImport {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(imp.document.clone()));
    p.resources = imp.resources.clone();
    let out = export_bpmn(&p, imp.document.id, false).unwrap();
    let again = prepare_import(&out.bytes, "again").unwrap();
    assert!(again.report.editable);
    again
}

fn local(r: &Option<EventReference>) -> String {
    match r {
        Some(EventReference::Local(s)) => s.to_string(),
        other => panic!("expected local reference, got {other:?}"),
    }
}

fn check_multi_instance(imp: &PreparedImport) {
    let LoopCharacteristics::MultiInstance {
        behavior,
        loop_data_input_ref,
        loop_data_output_ref,
        input_data_item,
        output_data_item,
        completion_condition,
        ..
    } = loop_of(imp, "MultiComplex")
    else {
        panic!("MultiComplex must be multi-instance");
    };
    assert_eq!(behavior, Some(MultiInstanceFlowCondition::Complex));
    assert!(local(&loop_data_input_ref).ends_with("TaskIn"));
    assert!(local(&loop_data_output_ref).ends_with("TaskOut"));
    assert_eq!(
        input_data_item.unwrap().id.unwrap().to_string(),
        "IterationIn"
    );
    assert_eq!(
        output_data_item.unwrap().id.unwrap().to_string(),
        "IterationOut"
    );
    assert_eq!(
        completion_condition.and_then(|c| c.plain_text().map(str::to_owned)),
        Some("complete".into())
    );

    let LoopCharacteristics::MultiInstance {
        behavior,
        one_behavior_event_ref,
        none_behavior_event_ref,
        ..
    } = loop_of(imp, "MultiSequential")
    else {
        panic!()
    };
    assert_eq!(behavior, Some(MultiInstanceFlowCondition::One));
    assert!(one_behavior_event_ref.is_some() && none_behavior_event_ref.is_none());
    let LoopCharacteristics::MultiInstance {
        behavior,
        none_behavior_event_ref,
        one_behavior_event_ref,
        ..
    } = loop_of(imp, "MultiNone")
    else {
        panic!()
    };
    assert_eq!(behavior, Some(MultiInstanceFlowCondition::None));
    assert!(none_behavior_event_ref.is_some() && one_behavior_event_ref.is_none());

    let LoopCharacteristics::Standard {
        test_before,
        maximum,
        condition,
        ..
    } = loop_of(imp, "StandardExplicit")
    else {
        panic!()
    };
    assert_eq!(test_before, Some(true));
    assert_eq!(maximum.as_deref(), Some("123456789012345678901234567890"));
    assert_eq!(condition.unwrap().plain_text(), Some("again"));
    let LoopCharacteristics::Standard { test_before, .. } = loop_of(imp, "StandardOmitted") else {
        panic!()
    };
    assert_eq!(test_before, None);
}

#[test]
fn multi_instance_and_standard_loop_fields_import_with_values_and_survive_export() {
    let imp = prepare_import(FIXTURE, "loops").unwrap();
    assert!(imp.report.editable);
    check_multi_instance(&imp);
    let again = reimport(&imp);
    check_multi_instance(&again);
    for id in [
        "MultiComplex",
        "MultiSequential",
        "MultiNone",
        "StandardExplicit",
    ] {
        assert_eq!(loop_of(&imp, id), loop_of(&again, id), "{id}");
    }
}
