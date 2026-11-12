use processcraft_core::model::ProjectData;
use processcraft_io::interchange::{PreparedImport, prepare_import};
use std::sync::Arc;

const VALUES: &str = include_str!("../../../tests/fixtures/bpmn/di/di-values.bpmn");

fn project(b: &PreparedImport) -> ProjectData {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(b.document.clone()));
    p.resources = b.resources.clone();
    p
}

#[test]
fn valid_geometry_has_no_geometry_error() {
    let b = prepare_import(VALUES.as_bytes(), "v").unwrap();
    project(&b).validate().unwrap();
}

#[test]
fn bounds_validation_names_invalid_geometry() {
    for (label, f) in [
        (
            "negative width",
            (|b: &mut PreparedImport| b.document.diagrams[0].shapes[0].bounds.width = -1.0)
                as fn(&mut PreparedImport),
        ),
        ("negative height", |b| {
            b.document.diagrams[0].shapes[0].bounds.height = -0.5
        }),
        ("nan x", |b| {
            b.document.diagrams[0].shapes[0].bounds.x = f64::NAN
        }),
        ("infinite y", |b| {
            b.document.diagrams[0].shapes[0].bounds.y = f64::INFINITY
        }),
        ("huge x", |b| {
            b.document.diagrams[0].shapes[1].bounds.x = 2.0e9
        }),
    ] {
        let mut b = prepare_import(VALUES.as_bytes(), "v").unwrap();
        f(&mut b);
        assert_eq!(
            project(&b).validate().unwrap_err(),
            "invalid geometry",
            "{label}"
        );
    }
    // label bounds use the same rule
    let mut b = prepare_import(VALUES.as_bytes(), "v").unwrap();
    b.document.diagrams[0].shapes[0]
        .label
        .as_mut()
        .unwrap()
        .bounds
        .as_mut()
        .unwrap()
        .width = -3.0;
    assert_eq!(project(&b).validate().unwrap_err(), "invalid geometry");
}

#[test]
fn point_validation_names_invalid_waypoint() {
    for (label, f) in [
        (
            "nan x",
            (|b: &mut PreparedImport| b.document.diagrams[0].edges[0].waypoints[1].x = f64::NAN)
                as fn(&mut PreparedImport),
        ),
        ("infinite y", |b| {
            b.document.diagrams[0].edges[0].waypoints[0].y = f64::NEG_INFINITY
        }),
        ("huge y", |b| {
            b.document.diagrams[0].edges[0].waypoints[2].y = -2.0e9
        }),
    ] {
        let mut b = prepare_import(VALUES.as_bytes(), "v").unwrap();
        f(&mut b);
        assert_eq!(
            project(&b).validate().unwrap_err(),
            "invalid waypoint",
            "{label}"
        );
    }
}

#[test]
fn import_refuses_bad_bounds_and_points_with_geometry_message() {
    for (from, to) in [
        ("width=\"36\"", "width=\"-36\""),
        ("x=\"12.5\"", "x=\"NaN\""),
        (
            "y=\"11\"/><di:waypoint x=\"74.25\"",
            "y=\"INF\"/><di:waypoint x=\"74.25\"",
        ),
        ("x=\"48.5\"", "x=\"1e12\""),
    ] {
        assert!(VALUES.contains(from), "{from}");
        let xml = VALUES.replacen(from, to, 1);
        match prepare_import(xml.as_bytes(), "bad") {
            Err(e) => assert!(format!("{e:?}").contains("Geometry"), "{to}: {e:?}"),
            Ok(b) => assert!(!b.report.editable, "{to} imported editable"),
        }
    }
}

#[test]
fn export_emits_di_values_and_reimports_exact() {
    use processcraft_io::interchange::export_bpmn;
    let parsed = prepare_import(VALUES.as_bytes(), "v").unwrap();
    let out = export_bpmn(&project(&parsed), parsed.document.id, false).unwrap();
    let xml = String::from_utf8(out.bytes.clone()).unwrap();
    for needle in [
        "id=\"diag\"",
        "name=\"Values\"",
        "documentation=\"Value diagram\"",
        "resolution=\"72\"",
        "x=\"12.5\" y=\"-7\" width=\"36\" height=\"36\"",
        "x=\"1.5\" y=\"40.25\" width=\"70\" height=\"14\"",
        "x=\"74.25\" y=\"11\"/>",
        "x=\"100\" y=\"50\"/>",
    ] {
        assert!(xml.contains(needle), "missing {needle}\n{xml}");
    }
    let again = prepare_import(&out.bytes, "v2").unwrap();
    let d = &again.document.diagrams[0];
    let wp: Vec<_> = d.edges[0].waypoints.iter().map(|p| (p.x, p.y)).collect();
    assert_eq!(wp, [(48.5, 11.0), (74.25, 11.0), (100.0, 50.0)]);
    let b = &d.shapes[0].bounds;
    assert_eq!((b.x, b.y, b.width, b.height), (12.5, -7.0, 36.0, 36.0));
    let lb = d.shapes[0].label.as_ref().unwrap().bounds.as_ref().unwrap();
    assert_eq!((lb.x, lb.y, lb.width, lb.height), (1.5, 40.25, 70.0, 14.0));
    assert_eq!(
        (d.name.as_deref(), d.resolution),
        (Some("Values"), Some(72.0))
    );
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m3-repair-di-export.bpmn"), &out.bytes).unwrap();
}
