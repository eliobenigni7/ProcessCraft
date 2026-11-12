use processcraft_core::bpmn_model::*;
use processcraft_core::ids::ElementIdentity;
use processcraft_core::model::{Node, NodeKind, ProjectData};
use processcraft_io::interchange::{ExportedBpmn, PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const TIMERS: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/events/timers-boundary-terminate.bpmn");
const REFS: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/events/references-links-conditions.bpmn");

fn export(b: &PreparedImport) -> ExportedBpmn {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap()
}
fn import(bytes: &[u8]) -> PreparedImport {
    let p = prepare_import(bytes, "m3 events").unwrap();
    assert!(p.report.editable, "{:?}", p.report.limitations);
    p
}
fn node<'a>(p: &'a PreparedImport, name: &str) -> &'a Node {
    let want: ElementIdentity = name.parse().unwrap();
    p.document.processes[0]
        .nodes
        .iter()
        .find(|n| n.id == want)
        .unwrap_or_else(|| panic!("node {name}"))
}
fn ev<'a>(p: &'a PreparedImport, name: &str) -> (&'a NodeKind, &'a NodeDetails) {
    let n = node(p, name);
    (&n.kind, n.details.as_ref().expect("event details"))
}
fn defs<'a>(p: &'a PreparedImport, name: &str) -> &'a [EventDefinition] {
    let (_, NodeDetails::Event { definitions, .. }) = ev(p, name) else {
        panic!("event details expected for {name}")
    };
    definitions
}
fn timer<'a>(p: &'a PreparedImport, name: &str) -> (TimerKind, &'a str) {
    let [
        EventDefinition::Timer {
            kind, expression, ..
        },
    ] = defs(p, name)
    else {
        panic!("single timer expected for {name}")
    };
    (
        kind.unwrap(),
        expression.as_ref().unwrap().plain_text().unwrap(),
    )
}
/// Export, re-import, and require identical typed nodes and root resources.
fn round_trip(p: &PreparedImport) -> (PreparedImport, String) {
    let out = export(p);
    let again = import(&out.bytes);
    assert_eq!(
        p.document.processes[0]
            .nodes
            .iter()
            .map(|n| (&n.id, &n.kind, &n.details))
            .collect::<Vec<_>>(),
        again.document.processes[0]
            .nodes
            .iter()
            .map(|n| (&n.id, &n.kind, &n.details))
            .collect::<Vec<_>>()
    );
    assert_eq!(p.document.event_resources, again.document.event_resources);
    assert_eq!(
        p.document.root_event_definitions,
        again.document.root_event_definitions
    );
    let xml = String::from_utf8(out.bytes).unwrap();
    if let Ok(dir) = std::env::var("M3_EVENTS_EXPORT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let n = std::fs::read_dir(&dir).unwrap().count();
        std::fs::write(format!("{dir}/export-{n}.bpmn"), &xml).unwrap();
    }
    (again, xml)
}

#[test]
fn timer_date_duration_cycle_import_typed_and_export_round_trip() {
    let p = import(TIMERS);
    assert_eq!(
        timer(&p, "start"),
        (TimerKind::Date, "2026-11-12T10:00:00Z")
    );
    assert_eq!(timer(&p, "boundary"), (TimerKind::Duration, "PT5M"));
    assert_eq!(timer(&p, "wait"), (TimerKind::Cycle, "R3/PT5M"));
    let (again, xml) = round_trip(&p);
    assert_eq!(
        timer(&again, "start"),
        (TimerKind::Date, "2026-11-12T10:00:00Z")
    );
    assert!(xml.contains(">2026-11-12T10:00:00Z</timeDate>"), "{xml}");
    assert!(xml.contains(">PT5M</timeDuration>"));
    assert!(xml.contains(">R3/PT5M</timeCycle>"));
}

#[test]
fn boundary_cancel_activity_false_and_attachment_import_explicitly() {
    let p = import(TIMERS);
    let (
        kind,
        NodeDetails::Event {
            attached_to,
            cancel_activity,
            ..
        },
    ) = ev(&p, "boundary")
    else {
        panic!()
    };
    assert_eq!(*kind, NodeKind::BoundaryEvent);
    assert_eq!(*cancel_activity, Some(false));
    assert!(
        attached_to
            .as_ref()
            .map(|r| format!("{r:?}"))
            .unwrap()
            .contains("work")
    );
    // the default is absent, not silently true
    let default = String::from_utf8(TIMERS.to_vec())
        .unwrap()
        .replace(" cancelActivity=\"false\"", "");
    let q = import(default.as_bytes());
    let (
        _,
        NodeDetails::Event {
            cancel_activity, ..
        },
    ) = ev(&q, "boundary")
    else {
        panic!()
    };
    assert_eq!(*cancel_activity, None);
    let (again, xml) = round_trip(&p);
    let (
        _,
        NodeDetails::Event {
            cancel_activity, ..
        },
    ) = ev(&again, "boundary")
    else {
        panic!()
    };
    assert_eq!(*cancel_activity, Some(false));
    assert!(xml.contains("cancelActivity=\"false\""));
}

#[test]
fn terminate_end_event_imports_as_typed_definition_and_exports() {
    let p = import(TIMERS);
    let [EventDefinition::Terminate { id, .. }] = defs(&p, "end") else {
        panic!("terminate definition expected")
    };
    assert_eq!(
        id.as_ref().unwrap(),
        &"terminate".parse::<ElementIdentity>().unwrap()
    );
    let (again, xml) = round_trip(&p);
    assert!(matches!(
        defs(&again, "end"),
        [EventDefinition::Terminate { .. }]
    ));
    assert!(
        xml.contains("<terminateEventDefinition id=\"terminate\""),
        "{xml}"
    );
}

#[test]
fn error_and_escalation_refs_import_typed_and_export_round_trip() {
    let p = import(REFS);
    let [
        EventDefinition::Error {
            error_ref: Some(e), ..
        },
    ] = defs(&p, "errorBoundary")
    else {
        panic!("error definition expected")
    };
    assert!(format!("{e:?}").contains("error"));
    let [
        EventDefinition::Escalation {
            escalation_ref: Some(e),
            ..
        },
    ] = defs(&p, "escalationEnd")
    else {
        panic!("escalation definition expected")
    };
    assert!(format!("{e:?}").contains("escalation"));
    assert!(
        p.document
            .event_resources
            .iter()
            .any(|r| matches!(r, EventResource::Error { code: Some(c), .. } if c == "E1"))
    );
    assert!(
        p.document
            .event_resources
            .iter()
            .any(|r| matches!(r, EventResource::Escalation { code: Some(c), .. } if c == "S1"))
    );
    let (_, xml) = round_trip(&p);
    assert!(xml.contains("errorRef="), "{xml}");
    assert!(xml.contains("escalationRef="));
    // a dropped definition would fail the typed asserts above; a dangling ref is never editable
    let dangling = String::from_utf8(REFS.to_vec())
        .unwrap()
        .replace("errorRef=\"tns:error\"", "errorRef=\"tns:nope\"");
    if let Ok(q) = prepare_import(dangling.as_bytes(), "dangling") {
        assert!(!q.report.editable);
    }
}

#[test]
fn throw_event_definition_ref_imports_and_exports() {
    let p = import(REFS);
    let (
        _,
        NodeDetails::Event {
            definition_refs, ..
        },
    ) = ev(&p, "signalEnd")
    else {
        panic!()
    };
    assert_eq!(definition_refs.len(), 1);
    assert!(format!("{:?}", definition_refs[0]).contains("sharedSignal"));
    assert!(p.document.root_event_definitions.iter().any(|d| matches!(
        d,
        EventDefinition::Signal {
            signal_ref: Some(_),
            ..
        }
    )));
    let (again, xml) = round_trip(&p);
    let (
        _,
        NodeDetails::Event {
            definition_refs, ..
        },
    ) = ev(&again, "signalEnd")
    else {
        panic!()
    };
    assert_eq!(definition_refs.len(), 1);
    assert!(
        xml.contains("eventDefinitionRef") && xml.contains(">sharedSignal</"),
        "{xml}"
    );
}

#[test]
fn message_operation_ref_imports_and_exports() {
    let p = import(REFS);
    let [
        EventDefinition::Message {
            message_ref: Some(m),
            operation_ref: Some(o),
            ..
        },
    ] = defs(&p, "messageThrow")
    else {
        panic!("message definition with operationRef expected")
    };
    assert!(format!("{m:?}").contains("message"));
    assert!(format!("{o:?}").contains("operation"));
    let (_, xml) = round_trip(&p);
    assert!(
        xml.contains("operationRef") && xml.contains(">operation</"),
        "{xml}"
    );
    let dangling = String::from_utf8(REFS.to_vec())
        .unwrap()
        .replace("<operationRef>tns:operation", "<operationRef>tns:nope");
    if let Ok(q) = prepare_import(dangling.as_bytes(), "dangling") {
        assert!(!q.report.editable);
    }
}

#[test]
fn link_pair_name_source_target_import_and_export() {
    let p = import(REFS);
    let [
        EventDefinition::Link {
            name,
            sources,
            target,
            ..
        },
    ] = defs(&p, "linkThrow")
    else {
        panic!()
    };
    assert_eq!(name, "Jump");
    assert!(sources.is_empty());
    assert!(format!("{:?}", target.as_ref().unwrap()).contains("catchLink"));
    let [
        EventDefinition::Link {
            name,
            sources,
            target,
            ..
        },
    ] = defs(&p, "linkCatch")
    else {
        panic!()
    };
    assert_eq!(name, "Jump");
    assert_eq!(sources.len(), 1);
    assert!(format!("{:?}", sources[0]).contains("throwLink"));
    assert!(target.is_none());
    let (_, xml) = round_trip(&p);
    assert!(xml.contains("name=\"Jump\""), "{xml}");
    assert!(xml.contains(">throwLink</") && xml.contains(">catchLink</"));
}

#[test]
fn conditional_condition_imports_and_exports() {
    let p = import(REFS);
    let [EventDefinition::Conditional { expression, .. }] = defs(&p, "conditionCatch") else {
        panic!()
    };
    assert_eq!(expression.plain_text(), Some("amount > 10"));
    let (again, xml) = round_trip(&p);
    let [EventDefinition::Conditional { expression, .. }] = defs(&again, "conditionCatch") else {
        panic!()
    };
    assert_eq!(expression.plain_text(), Some("amount > 10"));
    assert!(xml.contains("amount &gt; 10"), "{xml}");
}
