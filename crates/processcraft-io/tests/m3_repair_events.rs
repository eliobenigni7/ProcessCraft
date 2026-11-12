//! Repair evidence for the events family: typed import values and XSD-checked export.
use processcraft_core::bpmn_model::*;
use processcraft_core::ids::ElementIdentity;
use processcraft_core::model::{Node, NodeKind, ProjectData};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const INLINE: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/m3-repair-events/events-inline.bpmn");
const REFS: &[u8] =
    include_bytes!("../../../tests/fixtures/bpmn/m3-events/references-links-conditions.bpmn");

fn import(bytes: &[u8]) -> PreparedImport {
    let p = prepare_import(bytes, "m3 repair events").unwrap();
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
fn local(s: &str) -> EventReference {
    EventReference::Local(s.parse().unwrap())
}
fn event(p: &PreparedImport, name: &str) -> (NodeKind, NodeDetails) {
    let n = node(p, name);
    (n.kind, n.details.clone().expect("event details"))
}
fn export(p: &PreparedImport) -> String {
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(p.document.clone()));
    project.resources = p.resources.clone();
    let out = export_bpmn(&project, p.document.id, false).unwrap();
    String::from_utf8(out.bytes).unwrap()
}
fn write_for_xsd(name: &str, xml: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), xml).unwrap();
}
fn check(p: &PreparedImport) {
    // signal start with an inline definition
    let (kind, NodeDetails::Event { definitions, .. }) = event(p, "s1") else {
        panic!()
    };
    assert_eq!(kind, NodeKind::StartEvent);
    let [EventDefinition::Signal { id, signal_ref, .. }] = definitions.as_slice() else {
        panic!("one signal definition expected")
    };
    assert_eq!(id.as_ref(), Some(&"sd1".parse().unwrap()));
    assert_eq!(signal_ref.as_ref(), Some(&local("sig")));
    // intermediate throw with an inline signal
    let (kind, NodeDetails::Event { definitions, .. }) = event(p, "t1") else {
        panic!()
    };
    assert_eq!(kind, NodeKind::IntermediateThrowEvent);
    assert!(matches!(
        definitions.as_slice(),
        [EventDefinition::Signal { signal_ref: Some(r), .. }] if *r == local("sig")
    ));
    // boundary: attachment, cancelActivity and message reference
    let (
        kind,
        NodeDetails::Event {
            definitions,
            attached_to,
            cancel_activity,
            ..
        },
    ) = event(p, "b1")
    else {
        panic!()
    };
    assert_eq!(kind, NodeKind::BoundaryEvent);
    assert_eq!(cancel_activity, Some(false));
    assert_eq!(
        attached_to,
        Some(processcraft_core::bpmn_activity::ModelReference::Local(
            "work".parse().unwrap()
        ))
    );
    assert!(matches!(
        definitions.as_slice(),
        [EventDefinition::Message { message_ref: Some(m), .. }] if *m == local("msg")
    ));
    // end event referencing the root signal definition
    let (
        kind,
        NodeDetails::Event {
            definitions,
            definition_refs,
            ..
        },
    ) = event(p, "e1")
    else {
        panic!()
    };
    assert_eq!(kind, NodeKind::EndEvent);
    assert!(definitions.is_empty());
    assert_eq!(definition_refs, vec![local("rootSig")]);
    // the root definition itself
    assert!(matches!(
        p.document.root_event_definitions.as_slice(),
        [EventDefinition::Signal { id: Some(i), signal_ref: Some(r), .. }]
            if *i == "rootSig".parse::<ElementIdentity>().unwrap() && *r == local("sig")
    ));
}

#[test]
fn signal_end_throw_boundary_import_values_and_export_reimport_equal() {
    let p = import(INLINE);
    check(&p);
    let xml = export(&p);
    write_for_xsd("m3-repair-events-inline-export.bpmn", &xml);
    assert!(xml.contains("id=\"sd1\" signalRef=\"tns:sig\""), "{xml}");
    assert!(xml.contains("<endEvent id=\"e1\""), "{xml}");
    assert!(
        xml.contains(">rootSig</pc_event:eventDefinitionRef>"),
        "{xml}"
    );
    assert!(xml.contains("id=\"md1\" messageRef=\"tns:msg\""), "{xml}");
    assert!(xml.contains("cancelActivity=\"false\""), "{xml}");
    assert!(xml.contains("<intermediateThrowEvent id=\"t1\""), "{xml}");
    assert!(xml.contains("<startEvent id=\"s1\""), "{xml}");
    let again = import(xml.as_bytes());
    check(&again);
}

#[test]
fn referenced_fixture_end_event_and_signal_values_round_trip() {
    let p = import(REFS);
    // endEvent with eventDefinitionRef to the root signal definition
    let (
        kind,
        NodeDetails::Event {
            definition_refs, ..
        },
    ) = event(&p, "signalEnd")
    else {
        panic!()
    };
    assert_eq!(kind, NodeKind::EndEvent);
    assert_eq!(definition_refs, vec![local("sharedSignal")]);
    // endEvent with an inline escalation definition
    let (kind, NodeDetails::Event { definitions, .. }) = event(&p, "escalationEnd") else {
        panic!()
    };
    assert_eq!(kind, NodeKind::EndEvent);
    assert!(matches!(
        definitions.as_slice(),
        [EventDefinition::Escalation { escalation_ref: Some(r), .. }] if *r == local("escalation")
    ));
    assert!(matches!(
        p.document.root_event_definitions.as_slice(),
        [EventDefinition::Signal { signal_ref: Some(r), .. }] if *r == local("signal")
    ));
    let xml = export(&p);
    write_for_xsd("m3-repair-events-refs-export.bpmn", &xml);
    assert!(xml.contains("<endEvent id=\"signalEnd\""), "{xml}");
    assert!(xml.contains("<endEvent id=\"escalationEnd\""), "{xml}");
    assert!(
        xml.contains("id=\"sharedSignal\" signalRef=\"tns:signal\""),
        "{xml}"
    );
    let again = import(xml.as_bytes());
    assert_eq!(event(&again, "signalEnd").1, event(&p, "signalEnd").1);
    assert_eq!(
        event(&again, "escalationEnd").1,
        event(&p, "escalationEnd").1
    );
    assert_eq!(
        again.document.root_event_definitions,
        p.document.root_event_definitions
    );
}
