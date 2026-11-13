//! Attribute overlay projection (M5.4): pure data, no colours.
//!
//! An [`Overlay`] says which element gets which bucket index or badge text; the
//! renderer chooses colours per appearance. M7 and M9 build an `Overlay`
//! directly from their results, so this is the whole shared mechanism.
//!
//! Only activities are covered: the heatmap colours an activity's fill and a
//! badge sits on an activity's top-right corner. Stored decimals are parsed to
//! `f64` here and only here, for ranking; the stored values stay strings.
use crate::attributes::{AttributeType, AttributeValue, MAX_ENUM_OPTIONS, target_of};
use crate::ids::{AttributeDefinitionId, DocumentId, ElementSelector};
use crate::model::ProjectData;

/// Number of equal-width buckets for numeric heatmaps.
pub const NUMERIC_BUCKETS: u8 = 5;
/// An enum with more options than this falls back to badges.
pub const MAX_CATEGORICAL: usize = 8;
const MAX_BADGE_CHARS: usize = 24;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum OverlayMode {
    Heatmap,
    Badge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlaySpec {
    pub definition: AttributeDefinitionId,
    pub mode: OverlayMode,
}

/// How buckets map to colours: an ordered ramp or unordered categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayScale {
    Sequential,
    Categorical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayEntry {
    pub element: ElementSelector,
    pub bucket: Option<u8>,
    pub badge: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegendItem {
    pub bucket: u8,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub entries: Vec<OverlayEntry>,
    pub legend: Vec<LegendItem>,
    pub title: String,
    pub scale: OverlayScale,
}

impl Overlay {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            legend: Vec::new(),
            title: String::new(),
            scale: OverlayScale::Sequential,
        }
    }
}

/// Seconds of a canonical duration (`PnDTnHnMnS`, see `attributes::parse_duration`).
fn duration_seconds(text: &str) -> Option<f64> {
    let rest = text.strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((date, time)) => (date, time),
        None => (rest, ""),
    };
    let mut total = 0.0;
    let date_units: &[(char, f64)] = &[('D', 86_400.0)];
    let time_units: &[(char, f64)] = &[('H', 3_600.0), ('M', 60.0), ('S', 1.0)];
    for (part, units) in [(date, date_units), (time, time_units)] {
        let mut digits = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else {
                let unit = units.iter().find(|(u, _)| *u == c)?.1;
                total += digits.parse::<f64>().ok()? * unit;
                digits.clear();
            }
        }
        if !digits.is_empty() {
            return None;
        }
    }
    Some(total)
}

fn rank(value: &AttributeValue) -> Option<f64> {
    match value {
        AttributeValue::Number(text) | AttributeValue::Currency(text) => text.parse().ok(),
        AttributeValue::Duration(text) => duration_seconds(text),
        _ => None,
    }
}

fn trim_decimal(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn duration_label(seconds: f64) -> String {
    crate::attributes::parse_duration(&format!("PT{}S", seconds.round() as u64))
        .unwrap_or_else(|_| format!("{seconds}s"))
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_BADGE_CHARS {
        text.to_owned()
    } else {
        let head: String = text.chars().take(MAX_BADGE_CHARS - 1).collect();
        format!("{head}\u{2026}")
    }
}

fn badge_text(value: &AttributeValue, unit: Option<&str>) -> Option<String> {
    Some(truncate(&match value {
        AttributeValue::Number(v) | AttributeValue::Currency(v) => match unit {
            Some(unit) => format!("{v} {unit}"),
            None => v.clone(),
        },
        AttributeValue::Duration(v)
        | AttributeValue::Enum(v)
        | AttributeValue::Date(v)
        | AttributeValue::Text(v)
        | AttributeValue::Url(v) => v.clone(),
        AttributeValue::DictionaryRefs(_) => return None,
    }))
}

/// The overlay a CLI or MCP export asks for: heatmap unless stated, and an error when the
/// definition does not exist (an unknown ID must not silently export without colours).
pub fn export_overlay(
    project: &ProjectData,
    document_id: DocumentId,
    definition: AttributeDefinitionId,
    mode: Option<OverlayMode>,
) -> Result<Overlay, String> {
    if !project
        .attribute_definitions
        .iter()
        .any(|d| d.id == definition)
    {
        return Err(format!("attribute definition {definition} not found"));
    }
    let mode = mode.unwrap_or(OverlayMode::Heatmap);
    Ok(attribute_overlay(
        project,
        document_id,
        &OverlaySpec { definition, mode },
    ))
}

/// The overlay of one attribute over the activities of one document. Unknown
/// document or definition yields an empty overlay. Deterministic: entries are
/// sorted by element selector.
pub fn attribute_overlay(
    project: &ProjectData,
    document_id: DocumentId,
    spec: &OverlaySpec,
) -> Overlay {
    let Some(document) = project.documents.iter().find(|d| d.id == document_id) else {
        return Overlay::empty();
    };
    let Some(definition) = project
        .attribute_definitions
        .iter()
        .find(|d| d.id == spec.definition)
    else {
        return Overlay::empty();
    };
    let unit = definition.unit.as_deref();
    let mut title = definition.name.clone();
    if let Some(unit) = unit {
        title = format!("{title} ({unit})");
    }
    let mut values: Vec<(&ElementSelector, &AttributeValue)> = project
        .attribute_values
        .iter()
        .filter(|v| {
            v.definition == definition.id
                && v.source.document_id == document_id
                && target_of(document, &v.source.xml_id)
                    == Some(crate::attributes::AttributeTarget::Activity)
        })
        .map(|v| (&v.source.xml_id, &v.value))
        .collect();
    values.sort_by(|a, b| a.0.cmp(b.0));

    let mut overlay = Overlay {
        entries: Vec::new(),
        legend: Vec::new(),
        title,
        scale: OverlayScale::Sequential,
    };
    let badges = |overlay: &mut Overlay| {
        overlay.entries = values
            .iter()
            .filter_map(|(element, value)| {
                Some(OverlayEntry {
                    element: (*element).clone(),
                    bucket: None,
                    badge: Some(badge_text(value, unit)?),
                })
            })
            .collect();
    };
    if spec.mode == OverlayMode::Badge {
        badges(&mut overlay);
        return overlay;
    }
    match definition.value_type {
        AttributeType::Number | AttributeType::Currency | AttributeType::Duration => {
            let ranked: Vec<(&ElementSelector, f64)> = values
                .iter()
                .filter_map(|(element, value)| Some((*element, rank(value)?)))
                .collect();
            let min = ranked.iter().map(|r| r.1).fold(f64::INFINITY, f64::min);
            let max = ranked.iter().map(|r| r.1).fold(f64::NEG_INFINITY, f64::max);
            let buckets = f64::from(NUMERIC_BUCKETS);
            let bucket_of = |v: f64| -> u8 {
                if max > min {
                    (((v - min) * buckets / (max - min)).floor() as u8).min(NUMERIC_BUCKETS - 1)
                } else {
                    0
                }
            };
            overlay.entries = ranked
                .iter()
                .map(|(element, v)| OverlayEntry {
                    element: (*element).clone(),
                    bucket: Some(bucket_of(*v)),
                    badge: None,
                })
                .collect();
            if !ranked.is_empty() {
                let label = |v: f64| match definition.value_type {
                    AttributeType::Duration => duration_label(v),
                    _ => trim_decimal(v),
                };
                let count = if max > min { NUMERIC_BUCKETS } else { 1 };
                overlay.legend = (0..count)
                    .map(|bucket| {
                        let low = min + (max - min) * f64::from(bucket) / buckets;
                        let high = min + (max - min) * f64::from(bucket + 1) / buckets;
                        let text = if max > min {
                            format!("{} \u{2013} {}", label(low), label(high))
                        } else {
                            label(min)
                        };
                        LegendItem {
                            bucket,
                            label: text,
                        }
                    })
                    .collect();
            }
        }
        AttributeType::Enum
            if definition.options.len() <= MAX_CATEGORICAL.min(MAX_ENUM_OPTIONS) =>
        {
            overlay.scale = OverlayScale::Categorical;
            overlay.entries = values
                .iter()
                .filter_map(|(element, value)| {
                    let AttributeValue::Enum(option) = value else {
                        return None;
                    };
                    let index = definition.options.iter().position(|o| o == option)?;
                    Some(OverlayEntry {
                        element: (*element).clone(),
                        bucket: Some(index as u8),
                        badge: None,
                    })
                })
                .collect();
            overlay.legend = definition
                .options
                .iter()
                .enumerate()
                .map(|(i, option)| LegendItem {
                    bucket: i as u8,
                    label: truncate(option),
                })
                .collect();
        }
        _ => badges(&mut overlay),
    }
    overlay
}
