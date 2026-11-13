//! Attribute overlay projection (M5.4): pure data, no colours.
//!
//! An [`Overlay`] says which element gets which bucket index or badge text; the
//! renderer chooses colours per appearance. M7 and M9 build an `Overlay`
//! directly from their results, so this is the whole shared mechanism.
//!
//! Only activities are covered: the heatmap colours an activity's fill and a
//! badge sits on an activity's top-right corner. Stored decimals are parsed to
//! `f64` here and only here, for ranking; the stored values stay strings.
use crate::attributes::{AttributeType, AttributeValue, MAX_ENUM_OPTIONS};
use crate::bpmn_model::NodeDetails;
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

/// Elapsed seconds of a canonical duration (`PnDTnHnMnS`) as a decimal string.
fn duration_seconds(text: &str) -> Option<String> {
    let rest = text.strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((date, time)) => (date, time),
        None => (rest, ""),
    };
    let mut total: u128 = 0;
    let date_units: &[(char, u128)] = &[('D', 86_400)];
    let time_units: &[(char, u128)] = &[('H', 3_600), ('M', 60), ('S', 1)];
    for (part, units) in [(date, date_units), (time, time_units)] {
        let mut digits = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else {
                let unit = units.iter().find(|(u, _)| *u == c)?.1;
                total = total.checked_add(digits.parse::<u128>().ok()?.checked_mul(unit)?)?;
                digits.clear();
            }
        }
        if !digits.is_empty() {
            return None;
        }
    }
    Some(total.to_string())
}

/// The canonical decimal text that ranks a value: the stored string itself, or
/// elapsed seconds for a duration. Ranking never goes through floating point.
fn rank_text(value: &AttributeValue) -> Option<(String, String)> {
    match value {
        AttributeValue::Number(text) | AttributeValue::Currency(text) => {
            Some((text.clone(), text.clone()))
        }
        AttributeValue::Duration(text) => Some((duration_seconds(text)?, text.clone())),
        _ => None,
    }
}

/// An exact signed decimal: magnitude digits (most significant first, no leading
/// zeros, empty for zero) scaled by a common number of fraction digits.
#[derive(Clone)]
struct Exact {
    negative: bool,
    magnitude: Vec<u8>,
}

fn fraction_digits(text: &str) -> usize {
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
}

fn parse_exact(text: &str, scale: usize) -> Option<Exact> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let mut digits: Vec<u8> = Vec::with_capacity(integer.len() + scale);
    for byte in integer.bytes().chain(fraction.bytes()) {
        if !byte.is_ascii_digit() {
            return None;
        }
        digits.push(byte - b'0');
    }
    digits.resize(integer.len() + scale, 0);
    let first = digits.iter().position(|d| *d != 0).unwrap_or(digits.len());
    digits.drain(..first);
    Some(Exact {
        negative: negative && !digits.is_empty(),
        magnitude: digits,
    })
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn cmp_exact(a: &Exact, b: &Exact) -> std::cmp::Ordering {
    match (a.negative, b.negative) {
        (false, true) => std::cmp::Ordering::Greater,
        (true, false) => std::cmp::Ordering::Less,
        (false, false) => cmp_magnitude(&a.magnitude, &b.magnitude),
        (true, true) => cmp_magnitude(&b.magnitude, &a.magnitude),
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let (mut out, mut carry) = (Vec::with_capacity(a.len().max(b.len()) + 1), 0u8);
    let (mut ai, mut bi) = (a.iter().rev(), b.iter().rev());
    loop {
        let (x, y) = (ai.next(), bi.next());
        if x.is_none() && y.is_none() {
            break;
        }
        let sum = x.copied().unwrap_or(0) + y.copied().unwrap_or(0) + carry;
        out.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        out.push(carry);
    }
    out.reverse();
    out
}

/// `a - b` for `a >= b`.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let (mut out, mut borrow) = (Vec::with_capacity(a.len()), 0i8);
    let mut bi = b.iter().rev();
    for x in a.iter().rev() {
        let mut d = *x as i8 - borrow - bi.next().copied().unwrap_or(0) as i8;
        borrow = i8::from(d < 0);
        if d < 0 {
            d += 10;
        }
        out.push(d as u8);
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out.reverse();
    out
}

fn mul_small(a: &[u8], factor: u8) -> Vec<u8> {
    let (mut out, mut carry) = (Vec::with_capacity(a.len() + 1), 0u8);
    for x in a.iter().rev() {
        let product = x * factor + carry;
        out.push(product % 10);
        carry = product / 10;
    }
    if carry > 0 {
        out.push(carry);
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out.reverse();
    out
}

/// `value - minimum` for `value >= minimum`, as a magnitude.
fn distance(value: &Exact, minimum: &Exact) -> Vec<u8> {
    match (minimum.negative, value.negative) {
        (true, true) => sub_magnitude(&minimum.magnitude, &value.magnitude),
        (true, false) => add_magnitude(&minimum.magnitude, &value.magnitude),
        _ => sub_magnitude(&value.magnitude, &minimum.magnitude),
    }
}

/// The bucket of `value` between `minimum` and `maximum`: the number of interior
/// boundaries `minimum + k * range / 5` (k = 1..4) that the value reaches, by exact
/// comparison of `5 * (value - minimum)` with `k * range`. A value on a boundary
/// belongs to the upper bucket; the maximum belongs to the last.
fn exact_bucket(value: &Exact, minimum: &Exact, range: &[u8]) -> u8 {
    if range.is_empty() {
        return 0;
    }
    let scaled = mul_small(&distance(value, minimum), NUMERIC_BUCKETS);
    (1..NUMERIC_BUCKETS)
        .filter(|k| cmp_magnitude(&scaled, &mul_small(range, *k)) != std::cmp::Ordering::Less)
        .count() as u8
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

/// Every activity node of the document (sub-process content included), found in
/// one walk so the projection stays linear in nodes plus values.
fn activity_selectors(
    document: &crate::model::BpmnDocument,
) -> std::collections::HashSet<ElementSelector> {
    fn walk(nodes: &[crate::model::Node], out: &mut std::collections::HashSet<ElementSelector>) {
        for node in nodes {
            if node.kind.is_activity() {
                out.insert(node.id.selector());
            }
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                walk(&content.nodes, out);
            }
        }
    }
    let mut out = std::collections::HashSet::new();
    for process in &document.processes {
        walk(&process.nodes, &mut out);
    }
    out
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
    let activities = activity_selectors(document);
    let mut values: Vec<(&ElementSelector, &AttributeValue)> = project
        .attribute_values
        .iter()
        .filter(|v| {
            v.definition == definition.id
                && v.source.document_id == document_id
                && activities.contains(&v.source.xml_id)
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
            let texts: Vec<(&ElementSelector, (String, String))> = values
                .iter()
                .filter_map(|(element, value)| Some((*element, rank_text(value)?)))
                .collect();
            let scale = texts
                .iter()
                .map(|t| fraction_digits(&t.1.0))
                .max()
                .unwrap_or(0);
            let ranked: Vec<(&ElementSelector, &String, Exact)> = texts
                .iter()
                .filter_map(|(element, (rank, shown))| {
                    Some((*element, shown, parse_exact(rank, scale)?))
                })
                .collect();
            let extreme = |want: std::cmp::Ordering| {
                ranked
                    .iter()
                    .map(|r| &r.2)
                    .reduce(|a, b| if cmp_exact(b, a) == want { b } else { a })
            };
            if let (Some(minimum), Some(maximum)) = (
                extreme(std::cmp::Ordering::Less),
                extreme(std::cmp::Ordering::Greater),
            ) {
                let range = distance(maximum, minimum);
                let bucket_of = |v: &Exact| exact_bucket(v, minimum, &range);
                overlay.entries = ranked
                    .iter()
                    .map(|(element, _, exact)| OverlayEntry {
                        element: (*element).clone(),
                        bucket: Some(bucket_of(exact)),
                        badge: None,
                    })
                    .collect();
                // Each legend item names the stored values of its bucket: the lowest and
                // highest member as written, so nothing is rounded or invented. Empty
                // buckets are left out.
                overlay.legend = (0..NUMERIC_BUCKETS)
                    .filter_map(|bucket| {
                        let mut members = ranked.iter().filter(|r| bucket_of(&r.2) == bucket);
                        let first = members.next()?;
                        let (mut low, mut high) = (first, first);
                        for member in members {
                            if cmp_exact(&member.2, &low.2).is_lt() {
                                low = member;
                            }
                            if cmp_exact(&member.2, &high.2).is_gt() {
                                high = member;
                            }
                        }
                        let label = if low.1 == high.1 {
                            low.1.clone()
                        } else {
                            format!("{} \u{2013} {}", low.1, high.1)
                        };
                        Some(LegendItem { bucket, label })
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
