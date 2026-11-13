//! M5.4 attribute overlay projection: buckets, ties, gaps, enum fallback, ordering.
use processcraft_core::{
    attributes::{
        AttributeAssignment, AttributeDefinition, AttributeTarget, AttributeType, AttributeValue,
    },
    ids::{AttributeDefinitionId, DocumentId, ElementKey, ElementSelector},
    model::*,
    overlay::{
        MAX_CATEGORICAL, Overlay, OverlayMode, OverlayScale, OverlaySpec, attribute_overlay,
    },
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    id("11111111-1111-4111-8111-111111111111")
}
fn node(name: &str, kind: NodeKind) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        id: id(name),
        name: Some(name.into()),
        documentation: String::new().into(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
/// Activities `a0..a{n-1}` plus one start event `event`.
fn project(activities: usize) -> ProjectData {
    let mut d = BpmnDocument::new("Doc".into(), id("defs"));
    d.id = docid();
    let mut nodes: Vec<Node> = (0..activities)
        .map(|i| node(&format!("a{i}"), NodeKind::Task))
        .collect();
    nodes.push(node("event", NodeKind::StartEvent));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some("Process".into()),
        documentation: String::new().into(),
        nodes,
        lanes: vec![],
        flows: vec![],
    });
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    p
}
fn definition(
    value_type: AttributeType,
    unit: Option<&str>,
    options: &[&str],
) -> AttributeDefinition {
    AttributeDefinition {
        id: AttributeDefinitionId::new(),
        name: "Cost".into(),
        description: String::new(),
        value_type,
        unit: unit.map(Into::into),
        options: options.iter().map(|o| (*o).into()).collect(),
        dictionary_kind: None,
        applies_to: vec![AttributeTarget::Activity, AttributeTarget::Event],
        required: false,
    }
}
fn put(
    project: &mut ProjectData,
    definition: &AttributeDefinition,
    element: &str,
    v: AttributeValue,
) {
    project.attribute_values.push(AttributeAssignment {
        source: ElementKey {
            document_id: docid(),
            xml_id: id::<ElementSelector>(element),
        },
        definition: definition.id,
        value: v,
    });
}
fn spec(definition: &AttributeDefinition, mode: OverlayMode) -> OverlaySpec {
    OverlaySpec {
        definition: definition.id,
        mode,
    }
}
fn buckets(overlay: &Overlay) -> Vec<(String, Option<u8>)> {
    overlay
        .entries
        .iter()
        .map(|e| (e.element.to_string(), e.bucket))
        .collect()
}
fn labels(overlay: &Overlay) -> Vec<&str> {
    overlay.legend.iter().map(|l| l.label.as_str()).collect()
}
fn heat(values: &[&str]) -> Overlay {
    let (p, d) = numeric(values);
    attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap))
}
fn bucket_list(overlay: &Overlay) -> Vec<u8> {
    overlay.entries.iter().map(|e| e.bucket.unwrap()).collect()
}
fn numeric(values: &[&str]) -> (ProjectData, AttributeDefinition) {
    let mut p = project(values.len());
    let d = definition(AttributeType::Number, Some("EUR"), &[]);
    p.attribute_definitions.push(d.clone());
    for (i, v) in values.iter().enumerate() {
        put(
            &mut p,
            &d,
            &format!("a{i}"),
            AttributeValue::Number((*v).into()),
        );
    }
    (p, d)
}

#[test]
fn five_equal_width_buckets_have_exact_boundaries() {
    // 0..10 in five buckets of width 2: boundaries at 2, 4, 6, 8; the maximum
    // belongs to the last bucket.
    let (p, d) = numeric(&["0", "1.99", "2", "3.9", "4", "6", "7", "8", "9.99", "10"]);
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    let got: Vec<u8> = overlay.entries.iter().map(|e| e.bucket.unwrap()).collect();
    assert_eq!(got, vec![0, 0, 1, 1, 2, 3, 3, 4, 4, 4]);
    assert_eq!(overlay.scale, OverlayScale::Sequential);
    // Legend items name the stored values of each bucket, exactly as written.
    assert_eq!(
        labels(&overlay),
        [
            "0 \u{2013} 1.99",
            "2 \u{2013} 3.9",
            "4",
            "6 \u{2013} 7",
            "8 \u{2013} 10"
        ]
    );
    assert_eq!(overlay.title, "Cost (EUR)");
    assert!(overlay.entries.iter().all(|e| e.badge.is_none()));
}

#[test]
fn equal_values_share_one_bucket_and_one_legend_item() {
    let (p, d) = numeric(&["7", "7", "7"]);
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    assert!(overlay.entries.iter().all(|e| e.bucket == Some(0)));
    assert_eq!(overlay.legend.len(), 1);
    assert_eq!(overlay.legend[0].label, "7");
}

#[test]
fn elements_without_a_value_or_outside_activities_are_not_listed() {
    let (mut p, d) = numeric(&["1", "9"]);
    // a2 does not exist as a value; the start event has one but is not an activity.
    put(&mut p, &d, "event", AttributeValue::Number("1000".into()));
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    assert_eq!(
        buckets(&overlay),
        vec![("a0".into(), Some(0)), ("a1".into(), Some(4))]
    );
    // The event's 1000 does not stretch the range; empty buckets have no legend item.
    assert_eq!(labels(&overlay), ["1", "9"]);
}

#[test]
fn unknown_document_or_definition_gives_an_empty_overlay() {
    let (p, d) = numeric(&["1"]);
    let other = DocumentId::new();
    assert_eq!(
        attribute_overlay(&p, other, &spec(&d, OverlayMode::Heatmap)),
        Overlay::empty()
    );
    let missing = definition(AttributeType::Number, None, &[]);
    assert_eq!(
        attribute_overlay(&p, docid(), &spec(&missing, OverlayMode::Heatmap)),
        Overlay::empty()
    );
}

#[test]
fn duration_ranks_by_elapsed_seconds_not_by_text() {
    let mut p = project(4);
    let mut d = definition(AttributeType::Duration, None, &[]);
    d.name = "Lead time".into();
    p.attribute_definitions.push(d.clone());
    // Text order would put "P1D" before "PT2H"; elapsed time puts it last.
    for (i, v) in ["P1D", "PT2H", "PT30M", "P1DT12H"].into_iter().enumerate() {
        put(
            &mut p,
            &d,
            &format!("a{i}"),
            AttributeValue::Duration(v.into()),
        );
    }
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    let got: Vec<u8> = overlay.entries.iter().map(|e| e.bucket.unwrap()).collect();
    assert_eq!(got, vec![3, 0, 0, 4]);
    assert_eq!(labels(&overlay), ["PT30M \u{2013} PT2H", "P1D", "P1DT12H"]);
}

#[test]
fn enum_gets_one_bucket_per_option_in_option_order() {
    let mut p = project(3);
    let d = definition(AttributeType::Enum, None, &["Low", "Medium", "High"]);
    p.attribute_definitions.push(d.clone());
    put(&mut p, &d, "a0", AttributeValue::Enum("High".into()));
    put(&mut p, &d, "a1", AttributeValue::Enum("Low".into()));
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    assert_eq!(overlay.scale, OverlayScale::Categorical);
    assert_eq!(
        buckets(&overlay),
        vec![("a0".into(), Some(2)), ("a1".into(), Some(0))]
    );
    let labels: Vec<_> = overlay.legend.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(labels, ["Low", "Medium", "High"]);
}

#[test]
fn enum_beyond_eight_options_falls_back_to_badges() {
    let names: Vec<String> = (0..=MAX_CATEGORICAL).map(|i| format!("o{i}")).collect();
    let options: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut p = project(1);
    let d = definition(AttributeType::Enum, None, &options);
    p.attribute_definitions.push(d.clone());
    put(&mut p, &d, "a0", AttributeValue::Enum("o8".into()));
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    assert!(overlay.legend.is_empty());
    assert_eq!(overlay.entries.len(), 1);
    assert_eq!(overlay.entries[0].bucket, None);
    assert_eq!(overlay.entries[0].badge.as_deref(), Some("o8"));
    // Exactly eight options still colour.
    let mut p = project(1);
    let eight = definition(AttributeType::Enum, None, &options[..MAX_CATEGORICAL]);
    p.attribute_definitions.push(eight.clone());
    put(&mut p, &eight, "a0", AttributeValue::Enum("o7".into()));
    let overlay = attribute_overlay(&p, docid(), &spec(&eight, OverlayMode::Heatmap));
    assert_eq!(overlay.entries[0].bucket, Some(7));
    assert_eq!(overlay.legend.len(), 8);
}

#[test]
fn badge_mode_shows_values_with_units_and_no_buckets() {
    let (p, d) = numeric(&["12.5", "300"]);
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Badge));
    assert!(overlay.legend.is_empty());
    let badges: Vec<_> = overlay
        .entries
        .iter()
        .map(|e| (e.bucket, e.badge.as_deref()))
        .collect();
    assert_eq!(
        badges,
        vec![(None, Some("12.5 EUR")), (None, Some("300 EUR"))]
    );
}

#[test]
fn text_and_long_values_become_truncated_badges() {
    let mut p = project(1);
    let d = definition(AttributeType::Text, None, &[]);
    p.attribute_definitions.push(d.clone());
    put(&mut p, &d, "a0", AttributeValue::Text("x".repeat(40)));
    let overlay = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    let badge = overlay.entries[0].badge.as_deref().unwrap();
    assert_eq!(badge.chars().count(), 24);
    assert!(badge.ends_with('\u{2026}'));
}

#[test]
fn projection_is_deterministic_regardless_of_value_order() {
    let (p, d) = numeric(&["3", "1", "2", "9"]);
    let first = attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap));
    let mut shuffled = p.clone();
    shuffled.attribute_values.reverse();
    let second = attribute_overlay(&shuffled, docid(), &spec(&d, OverlayMode::Heatmap));
    assert_eq!(first, second);
    assert_eq!(
        first,
        attribute_overlay(&p, docid(), &spec(&d, OverlayMode::Heatmap))
    );
}

#[test]
fn small_decimals_keep_distinct_exact_labels() {
    let overlay = heat(&["0.001", "0.002", "0.004"]);
    assert_eq!(bucket_list(&overlay), vec![0, 1, 4]);
    assert_eq!(labels(&overlay), ["0.001", "0.002", "0.004"]);
}

#[test]
fn negative_values_never_render_as_negative_zero() {
    let overlay = heat(&["-0.001", "0.001"]);
    assert_eq!(bucket_list(&overlay), vec![0, 4]);
    assert_eq!(labels(&overlay), ["-0.001", "0.001"]);
    assert!(
        overlay
            .legend
            .iter()
            .all(|l| !l.label.contains("-0 ") && l.label != "-0")
    );
}

#[test]
fn values_beyond_f64_precision_stay_exact() {
    let values = ["9007199254740993", "9007199254740995", "9007199254740997"];
    let overlay = heat(&values);
    assert_eq!(bucket_list(&overlay), vec![0, 2, 4]);
    assert_eq!(labels(&overlay), values);
}

#[test]
fn very_long_decimals_are_ranked_and_labelled_without_nan_or_infinity() {
    let big = |lead: char| format!("{lead}{}", "0".repeat(399));
    let (one, two) = (big('1'), big('2'));
    let overlay = heat(&["0", &one, &two]);
    assert_eq!(bucket_list(&overlay), vec![0, 2, 4]);
    assert_eq!(labels(&overlay), ["0", one.as_str(), two.as_str()]);
    assert!(
        overlay
            .legend
            .iter()
            .all(|l| !l.label.contains("NaN") && !l.label.contains("inf"))
    );
}

#[test]
fn exact_boundary_values_land_in_the_upper_bucket() {
    // 0.1..0.6 split at 0.2, 0.3, 0.4, 0.5: with f64, 0.3 fell one bucket low.
    let overlay = heat(&["0.1", "0.3", "0.6"]);
    assert_eq!(bucket_list(&overlay), vec![0, 2, 4]);
    let overlay = heat(&["0.1", "0.2", "0.3", "0.4", "0.5", "0.6"]);
    assert_eq!(bucket_list(&overlay), vec![0, 1, 2, 3, 4, 4]);
}

#[test]
fn projection_scales_linearly_with_many_nodes() {
    // Many activities and values complete well under a second even unoptimized.
    let values: Vec<String> = (0..3000).map(|i| format!("{i}.5")).collect();
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    let started = std::time::Instant::now();
    let overlay = heat(&refs);
    assert_eq!(overlay.entries.len(), 3000);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}
