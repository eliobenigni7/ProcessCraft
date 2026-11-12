//! Repair tests that close skeptic findings with value assertions on import,
//! on exported bytes, on re-import, and an XSD run over each exported file.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::ChoreographyFlowElement,
    bpmn_metadata::DocumentationSet,
    ids::ElementSelector,
    model::{DiStyleRef, ParticipantBandKind, ProjectData},
};
use processcraft_io::interchange::{PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const CHOREO: &str =
    include_str!("../../../tests/fixtures/bpmn/di-choreography-representation.bpmn");
const CALLABLE: &str = include_str!("../../../tests/fixtures/bpmn/activity-callable-roles.bpmn");
const DOCS: &str = include_str!("../../../tests/fixtures/bpmn/process-documentation.bpmn");
const NESTED: &[u8] = include_bytes!("../../../tests/fixtures/bpmn/choreography-graph/nested.bpmn");

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(sel(s))
}
fn export(b: &PreparedImport) -> Vec<u8> {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    export_bpmn(&p, b.document.id, false).unwrap().bytes
}
/// XSD-validates `bytes` with the pinned schemas (skipped only without python).
fn assert_xsd_valid(name: &str, bytes: &[u8]) {
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&out, bytes).unwrap();
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
            "{name}: {s}{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
}

/// The pinned choreography DI fixture minus its vendor extensions, so the
/// export is XSD-checkable.
fn plain_choreo() -> String {
    let start = CHOREO.find("<di:extension>").unwrap();
    let end = CHOREO.find("</di:extension>").unwrap() + "</di:extension>".len();
    let mut s = String::from(&CHOREO[..start]);
    s.push_str(&CHOREO[end..]);
    s.replace(" v:plane=\"A\"", "")
        .replace(" v:band=\"B\"", "")
        .replace(" v:label=\"C\"", "")
}

#[test]
fn band_kinds_message_visibility_and_activity_shape_import_export_and_xsd() {
    let kinds = [
        ("top_initiating", ParticipantBandKind::TopInitiating),
        ("middle_initiating", ParticipantBandKind::MiddleInitiating),
        ("bottom_initiating", ParticipantBandKind::BottomInitiating),
        ("top_non_initiating", ParticipantBandKind::TopNonInitiating),
        (
            "middle_non_initiating",
            ParticipantBandKind::MiddleNonInitiating,
        ),
        (
            "bottom_non_initiating",
            ParticipantBandKind::BottomNonInitiating,
        ),
    ];
    for (text, expected) in kinds {
        let source = plain_choreo().replace(
            "participantBandKind=\"top_initiating\"",
            &format!("participantBandKind=\"{text}\""),
        );
        let parsed = prepare_import(source.as_bytes(), "bands").unwrap();
        assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
        let d = &parsed.document.diagrams[0];
        // choreographyActivityShape resolves to the TaskShape local id.
        for shape in &d.shapes[1..3] {
            assert_eq!(
                shape.choreography_activity_shape,
                Some(DiStyleRef::Local(sel("TaskShape")))
            );
        }
        assert_eq!(d.shapes[1].participant_band_kind, Some(expected));
        assert_eq!(d.shapes[1].is_message_visible, Some(false));
        assert_eq!(d.shapes[2].is_message_visible, Some(true));

        let bytes = export(&parsed);
        let xml = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            xml.contains(&format!("participantBandKind=\"{text}\"")),
            "{xml}"
        );
        assert!(xml.contains("isMessageVisible=\"false\""), "{xml}");
        assert!(xml.contains("isMessageVisible=\"true\""), "{xml}");
        assert!(xml.contains("choreographyActivityShape="), "{xml}");
        let again = prepare_import(&bytes, "again").unwrap();
        assert!(again.report.editable, "{:?}", again.report.limitations);
        let a = &again.document.diagrams[0];
        assert_eq!(a.shapes[1].participant_band_kind, Some(expected));
        assert_eq!(a.shapes[1].is_message_visible, Some(false));
        assert_eq!(a.shapes[2].is_message_visible, Some(true));
        assert_eq!(
            a.shapes[1].choreography_activity_shape,
            Some(DiStyleRef::Local(sel("TaskShape")))
        );
        assert_xsd_valid(&format!("m3-repair-band-{text}.bpmn"), &bytes);
    }
}

#[test]
fn callable_supported_interface_refs_import_and_export_with_values() {
    // The fixture's 30-digit loopMaximum is not a libxml2 xs:integer, so the
    // XSD run uses a copy with an ordinary value; the refs are unaffected.
    let source = CALLABLE.replace("123456789012345678901234567890", "12345");
    assert_ne!(source, CALLABLE);
    let parsed = prepare_import(source.as_bytes(), "callable").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let check = |b: &PreparedImport| {
        let doc = &b.document;
        assert_eq!(
            doc.processes[0]
                .activity_metadata
                .as_ref()
                .unwrap()
                .callable
                .supported_interface_refs,
            vec![local("Interface")]
        );
        let global = doc
            .global_tasks
            .iter()
            .find(|g| g.name.as_deref() == Some("Reusable task"))
            .unwrap();
        assert_eq!(
            global.callable.supported_interface_refs,
            vec![local("Interface")]
        );
    };
    check(&parsed);
    let bytes = export(&parsed);
    let xml = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        xml.matches("supportedInterfaceRef xmlns").count()
            + xml.matches("<supportedInterfaceRef>").count()
            >= 3
    );
    assert!(xml.contains("Interface</pc_event:supportedInterfaceRef>"));
    let again = prepare_import(&bytes, "again").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    check(&again);
    assert_xsd_valid("m3-repair-callable.bpmn", &bytes);
}

#[test]
fn documentation_id_imports_the_fixture_value() {
    let parsed = prepare_import(DOCS.as_bytes(), "docs").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let DocumentationSet::Structured(docs) = &parsed.document.processes[0].documentation else {
        panic!("structured docs expected")
    };
    assert_eq!(
        docs[0].id.as_ref().and_then(|id| id.xml_id()),
        Some(&"documentation".parse().unwrap())
    );
    assert!(docs[1].id.is_none());
}

#[test]
fn flow_node_incoming_and_outgoing_survive_export_with_values() {
    // Process scope.
    let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:tns="urn:flow" id="d" targetNamespace="urn:flow">
 <process id="p" isExecutable="false">
  <startEvent id="s"><outgoing>tns:f1</outgoing></startEvent>
  <task id="t"><incoming>tns:f1</incoming><outgoing>tns:f2</outgoing></task>
  <endEvent id="e"><incoming>tns:f2</incoming></endEvent>
  <sequenceFlow id="f1" sourceRef="s" targetRef="t"/>
  <sequenceFlow id="f2" sourceRef="t" targetRef="e"/>
 </process>
</definitions>"#;
    let parsed = prepare_import(source.as_bytes(), "flow").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let bytes = export(&parsed);
    let xml = String::from_utf8(bytes.clone()).unwrap();
    assert!(xml.contains(":incoming") && xml.contains(":outgoing"));
    let again = prepare_import(&bytes, "again").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    let meta = |b: &PreparedImport, i: usize| {
        let m = b.document.processes[0].nodes[i]
            .flow_metadata
            .clone()
            .unwrap();
        (m.incoming, m.outgoing)
    };
    let first = meta(&parsed, 1);
    assert_eq!(first.0.len(), 1);
    assert_eq!(first.1.len(), 1);
    assert_eq!(meta(&again, 1), first);
    assert_eq!(meta(&again, 0), meta(&parsed, 0));
    assert_eq!(meta(&again, 2), meta(&parsed, 2));
    assert_xsd_valid("m3-repair-flow-process.bpmn", &bytes);

    // Nested choreography scope.
    let nested = prepare_import(NESTED, "nested").unwrap();
    let bytes = export(&nested);
    let xml = String::from_utf8(bytes.clone()).unwrap();
    assert!(xml.contains("outgoing>") && xml.contains("incoming>"));
    let again = prepare_import(&bytes, "again").unwrap();
    assert!(again.report.editable, "{:?}", again.report.limitations);
    let pair = |b: &PreparedImport| {
        let g = b.document.choreographies[0].graph.as_ref().unwrap();
        let ChoreographyFlowElement::Activity(sub) = &g.flow_elements[1] else {
            panic!("sub expected")
        };
        let Some(processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Sub {
            graph,
            ..
        }) = &sub.details
        else {
            panic!("sub content expected")
        };
        let ChoreographyFlowElement::Event(ev) = &graph.flow_elements[0] else {
            panic!("event expected")
        };
        let ChoreographyFlowElement::Gateway(gw) = &graph.flow_elements[1] else {
            panic!("gateway expected")
        };
        (
            ev.flow_metadata.as_ref().unwrap().outgoing.clone(),
            gw.flow_metadata.as_ref().unwrap().incoming.clone(),
        )
    };
    let before = pair(&nested);
    assert_eq!(before.0.len(), 1);
    assert_eq!(before.1.len(), 1);
    assert_eq!(pair(&again), before);
}
