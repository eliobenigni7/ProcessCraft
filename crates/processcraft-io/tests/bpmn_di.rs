use processcraft_core::ids::ElementSelector;
use processcraft_core::model::{DiStyleRef, MessageVisibleKind, ProjectData};
use processcraft_io::interchange::{ExportedBpmn, PreparedImport, export_bpmn, prepare_import};
use std::sync::Arc;

const VALUES: &str = include_str!("../../../tests/fixtures/bpmn/di/di-values.bpmn");
const CHOREO: &str =
    include_str!("../../../tests/fixtures/bpmn/di-choreography-representation.bpmn");

fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn project(b: &PreparedImport) -> ProjectData {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    p
}
fn export(b: &PreparedImport) -> ExportedBpmn {
    export_bpmn(&project(b), b.document.id, false).unwrap()
}
fn codes(b: &PreparedImport) -> Vec<String> {
    project(b)
        .diagnostics()
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn di_values_import_and_export_reimport_equal() {
    let parsed = prepare_import(VALUES.as_bytes(), "values").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report.limitations);
    let check = |b: &PreparedImport| {
        let d = &b.document.diagrams[0];
        // BPMNDiagram
        assert_eq!(d.name.as_deref(), Some("Values"));
        assert_eq!(d.documentation.as_deref(), Some("Value diagram"));
        assert_eq!(d.resolution, Some(72.0));
        // BPMNPlane bpmnElement
        assert_eq!(d.plane, sel("proc"));
        // BPMNShape bpmnElement + Bounds x/y/width/height
        assert_eq!(d.shapes[0].element, sel("start"));
        let b0 = &d.shapes[0].bounds;
        assert_eq!((b0.x, b0.y, b0.width, b0.height), (12.5, -7.0, 36.0, 36.0));
        assert_eq!(d.shapes[1].element, sel("sub"));
        assert_eq!(d.shapes[1].is_expanded, Some(true));
        let b1 = &d.shapes[1].bounds;
        assert_eq!(
            (b1.x, b1.y, b1.width, b1.height),
            (100.0, -40.0, 300.5, 180.0)
        );
        // BPMNLabel Bounds + labelStyle
        let l = d.shapes[0].label.as_ref().unwrap();
        let lb = l.bounds.as_ref().unwrap();
        assert_eq!((lb.x, lb.y, lb.width, lb.height), (1.5, 40.25, 70.0, 14.0));
        assert_eq!(l.style_ref, Some(DiStyleRef::Local(sel("style"))));
        // BPMNEdge bpmnElement + Point x/y
        assert_eq!(d.edges[0].element, sel("flow"));
        let pts: Vec<_> = d.edges[0].waypoints.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(pts, vec![(48.5, 11.0), (74.25, 11.0), (100.0, 50.0)]);
    };
    check(&parsed);
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert!(again.report.editable);
    check(&again);
}

#[test]
fn choreography_fixture_imports_message_visibility_values() {
    let parsed = prepare_import(CHOREO.as_bytes(), "choreo").unwrap();
    let d = &parsed.document.diagrams[0];
    assert_eq!(d.shapes[0].element, sel("Task"));
    assert_eq!(d.shapes[1].is_message_visible, Some(false));
    assert_eq!(d.shapes[2].is_message_visible, Some(true));
    assert_eq!(
        d.edges[0].message_visible_kind,
        Some(MessageVisibleKind::NonInitiating)
    );
    assert_eq!(d.edges[0].element, sel("Response"));
    let again = prepare_import(&export(&parsed).bytes, "again").unwrap();
    assert_eq!(
        again.document.diagrams[0].shapes[2].is_message_visible,
        Some(true)
    );
    assert_eq!(
        again.document.diagrams[0].shapes[1].is_message_visible,
        Some(false)
    );
}

#[test]
fn message_decorator_on_non_message_flow_is_diagnosed() {
    let mut parsed = prepare_import(VALUES.as_bytes(), "values").unwrap();
    assert!(
        !codes(&parsed)
            .iter()
            .any(|c| c == "messageDecoratorWrongElement")
    );
    parsed.document.diagrams[0].edges[0].message_visible_kind =
        Some(MessageVisibleKind::Initiating);
    assert!(
        codes(&parsed)
            .iter()
            .any(|c| c == "messageDecoratorWrongElement")
    );
    // the same decorator on a real Message Flow is accepted
    let ok = prepare_import(CHOREO.as_bytes(), "choreo").unwrap();
    assert!(
        !codes(&ok)
            .iter()
            .any(|c| c == "messageDecoratorWrongElement")
    );
}

#[test]
fn plane_order_with_missing_representation_is_a_reference_issue() {
    let mut parsed = prepare_import(CHOREO.as_bytes(), "choreo").unwrap();
    let doc = &parsed.document;
    assert!(
        !processcraft_core::validation::activity_family_reference_issues(doc)
            .iter()
            .any(|i| i.field == "di.planeOrder")
    );
    parsed.document.diagrams[0]
        .plane_order
        .as_mut()
        .unwrap()
        .push(sel("NoSuchShape"));
    let issues = processcraft_core::validation::activity_family_reference_issues(&parsed.document);
    assert!(issues.iter().any(|i| i.field == "di.planeOrder"));
}
