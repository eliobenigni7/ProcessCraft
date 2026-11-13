//! Project-level custom attribute definitions and values (M5.1a).
//!
//! Values live at project level keyed by [`ElementKey`], like dictionary links;
//! `Node`, `Flow` and `Lane` are not changed. Every persistent change enters
//! through a `commands::Mutation`; the application logic lives here so
//! `commands.rs` stays a dispatcher.
//!
//! Numbers and currency amounts are canonical decimal strings, never `f64`
//! (solution design §5.3). `Duration` is the ISO 8601 subset `PnDTnHnMnS`
//! (no years or months); `P1D` is exactly 24 hours of elapsed time, never a
//! working day — working-time semantics belong to the M7 scenario calendars.
//! `Date` is `YYYY-MM-DD`. URLs are stored as text; the application never
//! fetches them.
use crate::bpmn_model::NodeDetails;
use crate::commands::CommandError;
use crate::ids::{AttributeDefinitionId, DocumentId, ElementKey, ElementSelector};
use crate::model::{
    BpmnDocument, DictionaryKind, DictionaryTarget, FlowKind, NodeKind, ProjectData,
};
use serde::{Deserialize, Serialize};

pub const MAX_DEFINITIONS: usize = 200;
pub const MAX_ENUM_OPTIONS: usize = 64;
pub const MAX_DICTIONARY_REFS: usize = 16;
/// Same bound as every other short label (solution design §17.2).
pub const MAX_TEXT: usize = 16_384;
/// Digits per duration component; `P999999999D` is far past any real process.
const MAX_DURATION_DIGITS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum AttributeType {
    Text,
    Number,
    Duration,
    Currency,
    Enum,
    Date,
    DictionaryRef,
    Url,
}

/// One element family a definition can apply to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum AttributeTarget {
    Activity,
    Event,
    Gateway,
    SequenceFlow,
    Lane,
    Participant,
    Process,
    DataObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttributeDefinition {
    pub id: AttributeDefinitionId,
    pub name: String,
    pub description: String,
    pub value_type: AttributeType,
    /// Free label for `Number`, an ISO 4217 code for `Currency`.
    pub unit: Option<String>,
    /// Used only by `Enum`.
    pub options: Vec<String>,
    /// Used only by `DictionaryRef`.
    pub dictionary_kind: Option<DictionaryKind>,
    /// Enforced when a value is set and reported as a diagnostic otherwise;
    /// never a structural invariant of `ProjectData::validate`.
    pub applies_to: Vec<AttributeTarget>,
    pub required: bool,
}

/// A canonical, validated value. Numbers and currency amounts are decimal
/// strings as produced by [`parse_decimal`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum AttributeValue {
    Text(String),
    Number(String),
    Duration(String),
    Currency(String),
    Enum(String),
    Date(String),
    /// Reuses the dictionary targets so an imported label can stay an unlinked
    /// literal (solution design §10.1); a list because RACI assigns several roles.
    DictionaryRefs(Vec<DictionaryTarget>),
    Url(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttributeAssignment {
    pub source: ElementKey,
    pub definition: AttributeDefinitionId,
    pub value: AttributeValue,
}

/// What to do when an update would invalidate existing values (removed or
/// renamed enum option, narrowed `applies_to`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "type",
    content = "options",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum AttributeUpdateResolution {
    Reject,
    Clear,
    MapOptions(Vec<AttributeOptionMapping>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttributeOptionMapping {
    pub from: String,
    pub to: String,
}

/// What to do when a definition in use is deleted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "type",
    content = "definition_id",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum AttributeDeleteResolution {
    Reject,
    Discard,
    /// Move the values onto another definition of the same type.
    MoveTo(AttributeDefinitionId),
}

/// One entry of [`crate::commands::Mutation::SetAttributeValues`]; `None` clears.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttributeValueInput {
    pub definition_id: AttributeDefinitionId,
    pub value: Option<AttributeValue>,
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::InvalidParams(message.into())
}

impl AttributeDefinition {
    pub fn validate_shape(&self) -> Result<(), CommandError> {
        if self.name.trim().is_empty() || self.name.len() > MAX_TEXT {
            return Err(invalid("attribute definition name must be 1..16384 bytes"));
        }
        if self.name.trim() != self.name {
            return Err(invalid(
                "attribute definition name must not start or end with whitespace",
            ));
        }
        if self.unit.as_ref().is_some_and(|unit| unit.len() > MAX_TEXT) {
            return Err(invalid("attribute unit exceeds the text limit"));
        }
        if self.value_type == AttributeType::Currency
            && !self.unit.as_ref().is_some_and(|unit| {
                unit.len() == 3 && unit.bytes().all(|byte| byte.is_ascii_uppercase())
            })
        {
            return Err(invalid(
                "a Currency definition needs an ISO 4217 unit (three uppercase letters)",
            ));
        }
        if self.applies_to.is_empty() {
            return Err(invalid(
                "attribute definition must apply to at least one target",
            ));
        }
        if self.description.len() > MAX_TEXT {
            return Err(invalid("attribute description exceeds the text limit"));
        }
        if self.options.len() > MAX_ENUM_OPTIONS {
            return Err(invalid("attribute options exceed the enum option limit"));
        }
        let mut unique = std::collections::HashSet::new();
        for option in &self.options {
            if option.is_empty() || option.len() > MAX_TEXT {
                return Err(invalid("attribute option must be 1..16384 bytes"));
            }
            if !unique.insert(option.as_str()) {
                return Err(invalid("duplicate attribute option"));
            }
        }
        match self.value_type {
            AttributeType::Number | AttributeType::Currency => {}
            _ if self.unit.is_some() => {
                return Err(invalid("unit requires the Number or Currency type"));
            }
            _ => {}
        }
        match self.value_type {
            AttributeType::Enum if self.options.is_empty() => {
                return Err(invalid("an Enum definition needs at least one option"));
            }
            AttributeType::Enum => {}
            _ if !self.options.is_empty() => {
                return Err(invalid("options require the Enum type"));
            }
            _ => {}
        }
        match self.value_type {
            AttributeType::DictionaryRef if self.dictionary_kind.is_none() => {
                return Err(invalid(
                    "a DictionaryRef definition needs a dictionary kind",
                ));
            }
            AttributeType::DictionaryRef => {}
            _ if self.dictionary_kind.is_some() => {
                return Err(invalid("dictionary kind requires the DictionaryRef type"));
            }
            _ => {}
        }
        let mut targets = std::collections::HashSet::new();
        for target in &self.applies_to {
            if !targets.insert(target) {
                return Err(invalid("duplicate attribute target"));
            }
        }
        Ok(())
    }
}

/// The element family of a model element, or `None` for element kinds that
/// carry no attribute target (annotations, groups, choreography, DI-only IDs).
/// Every caller of attribute `applies_to` checks goes through this function.
pub fn target_of(document: &BpmnDocument, selector: &ElementSelector) -> Option<AttributeTarget> {
    fn node_target(kind: NodeKind) -> Option<AttributeTarget> {
        Some(match kind {
            NodeKind::Task
            | NodeKind::UserTask
            | NodeKind::ManualTask
            | NodeKind::ServiceTask
            | NodeKind::SendTask
            | NodeKind::ReceiveTask
            | NodeKind::ScriptTask
            | NodeKind::BusinessRuleTask
            | NodeKind::SubProcess
            | NodeKind::AdHocSubProcess
            | NodeKind::Transaction
            | NodeKind::CallActivity => AttributeTarget::Activity,
            NodeKind::StartEvent
            | NodeKind::EndEvent
            | NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent => AttributeTarget::Event,
            NodeKind::InclusiveGateway
            | NodeKind::ComplexGateway
            | NodeKind::EventBasedGateway
            | NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway => AttributeTarget::Gateway,
            NodeKind::TextAnnotation => return None,
            NodeKind::DataObjectReference | NodeKind::DataStoreReference => {
                AttributeTarget::DataObject
            }
        })
    }
    fn walk_nodes(
        nodes: &[crate::model::Node],
        selector: &ElementSelector,
    ) -> Option<AttributeTarget> {
        for node in nodes {
            if node.id.selector() == *selector {
                return node_target(node.kind);
            }
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                if let Some(found) = walk_nodes(&content.nodes, selector) {
                    return Some(found);
                }
                for lane in &content.lanes {
                    if lane.id.selector() == *selector {
                        return Some(AttributeTarget::Lane);
                    }
                }
                for flow in &content.flows {
                    if flow.kind == FlowKind::Sequence && flow.id.selector() == *selector {
                        return Some(AttributeTarget::SequenceFlow);
                    }
                }
                for data_object in &content.data_objects {
                    if data_object
                        .id
                        .as_ref()
                        .is_some_and(|identity| identity.selector() == *selector)
                    {
                        return Some(AttributeTarget::DataObject);
                    }
                }
            }
        }
        None
    }
    for process in &document.processes {
        if process.id.selector() == *selector {
            return Some(AttributeTarget::Process);
        }
        for lane in &process.lanes {
            if lane.id.selector() == *selector {
                return Some(AttributeTarget::Lane);
            }
        }
        for data_object in &process.data_objects {
            if data_object
                .id
                .as_ref()
                .is_some_and(|identity| identity.selector() == *selector)
            {
                return Some(AttributeTarget::DataObject);
            }
        }
        for flow in &process.flows {
            if flow.kind == FlowKind::Sequence && flow.id.selector() == *selector {
                return Some(AttributeTarget::SequenceFlow);
            }
        }
        if let Some(found) = walk_nodes(&process.nodes, selector) {
            return Some(found);
        }
    }
    for collaboration in document.collaborations() {
        for participant in &collaboration.participants {
            if participant.id.selector() == *selector {
                return Some(AttributeTarget::Participant);
            }
        }
    }
    None
}

/// Canonical bounded decimal: optional sign, digits with optional fraction, no
/// exponent. Leading zeros, `+` signs, trailing fraction zeros and `-0` are
/// normalized away.
pub fn parse_decimal(text: &str) -> Result<String, String> {
    if text.len() > MAX_TEXT {
        return Err("decimal exceeds the text limit".into());
    }
    let bytes = text.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'-') | Some(b'+')));
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_end = i;
    let mut fraction = "";
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let fraction_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        fraction = &text[fraction_start..i];
    }
    if i != bytes.len() || (int_end == int_start && fraction.is_empty()) {
        return Err("not a decimal number".into());
    }
    let digits = text[int_start..int_end].trim_start_matches('0');
    let fraction = fraction.trim_end_matches('0');
    let negative = bytes[0] == b'-' && (!digits.is_empty() || !fraction.is_empty());
    let sign = if negative { "-" } else { "" };
    let integer = if digits.is_empty() { "0" } else { digits };
    Ok(if fraction.is_empty() {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction}")
    })
}

/// Canonical duration in the ISO 8601 subset `PnDTnHnMnS`: no years or months,
/// non-negative integer components, case-insensitive input, zero components
/// dropped (`PT0S` is the zero duration). `P1D` is exactly 24 elapsed hours.
pub fn parse_duration(text: &str) -> Result<String, String> {
    if text.len() > MAX_TEXT {
        return Err("duration exceeds the text limit".into());
    }
    let upper = text.to_ascii_uppercase();
    let rest = upper
        .strip_prefix('P')
        .ok_or("duration must start with P (PnDTnHnMnS)")?;
    let (days, rest) = take_duration_component(rest, 'D');
    let (hours, minutes, seconds, rest, timed) = match rest.strip_prefix('T') {
        Some(time) => {
            let (hours, time) = take_duration_component(time, 'H');
            let (minutes, time) = take_duration_component(time, 'M');
            let (seconds, time) = take_duration_component(time, 'S');
            (hours, minutes, seconds, time, true)
        }
        None => (None, None, None, rest, false),
    };
    if !rest.is_empty() {
        return Err("duration must follow PnDTnHnMnS".into());
    }
    if timed && hours.is_none() && minutes.is_none() && seconds.is_none() {
        return Err("duration time section needs a component".into());
    }
    if [days, hours, minutes, seconds]
        .iter()
        .all(|part| part.is_none())
    {
        return Err("duration needs at least one component".into());
    }
    // Equal elapsed times get one form: carry seconds into minutes, minutes into
    // hours and hours into days (`PT90M` → `PT1H30M`, `PT24H` → `P1D`).
    let total = days.unwrap_or(0) * 86_400
        + hours.unwrap_or(0) * 3_600
        + minutes.unwrap_or(0) * 60
        + seconds.unwrap_or(0);
    let (days, rest) = (total / 86_400, total % 86_400);
    let (hours, minutes, seconds) = (rest / 3_600, rest % 3_600 / 60, rest % 60);
    let mut canonical = String::from("P");
    if days != 0 {
        canonical.push_str(&format!("{days}D"));
    }
    if rest != 0 {
        canonical.push('T');
        for (value, unit) in [(hours, 'H'), (minutes, 'M'), (seconds, 'S')] {
            if value != 0 {
                canonical.push_str(&format!("{value}{unit}"));
            }
        }
    }
    if canonical == "P" {
        canonical.push_str("T0S");
    }
    Ok(canonical)
}

fn take_duration_component(text: &str, unit: char) -> (Option<u128>, &str) {
    let digits: usize = text
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > MAX_DURATION_DIGITS {
        return (None, text);
    }
    let (digits, rest) = text.split_at(digits);
    if rest.starts_with(unit) {
        (digits.parse().ok(), &rest[unit.len_utf8()..])
    } else {
        (None, text)
    }
}

/// `YYYY-MM-DD` with a real calendar date (leap years included).
pub fn parse_date(text: &str) -> Result<String, String> {
    if text.len() > MAX_TEXT {
        return Err("date exceeds the text limit".into());
    }
    let bytes = text.as_bytes();
    let digits = |mut range: std::ops::Range<usize>| {
        range.all(|i| bytes.get(i).is_some_and(|b| b.is_ascii_digit()))
    };
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !digits(0..4)
        || !digits(5..7)
        || !digits(8..10)
    {
        return Err("date must be YYYY-MM-DD".into());
    }
    let year: u32 = text[..4].parse().unwrap();
    let month: u32 = text[5..7].parse().unwrap();
    let day: u32 = text[8..10].parse().unwrap();
    if month == 0 {
        return Err("date month must be 01..12".into());
    }
    if month > 12 {
        return Err("date month must be 01..12".into());
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    // January..July start on 31; August..December start on 31 as well, so
    // parity flips between the two halves.
    let long = if month <= 7 {
        !month.is_multiple_of(2)
    } else {
        month.is_multiple_of(2)
    };
    let days = 30 + u32::from(long);

    let days = if month == 2 {
        u32::from(leap) + 28
    } else {
        days
    };
    if day == 0 || day > days {
        return Err("date day does not exist in that month".into());
    }
    Ok(text.to_string())
}

fn check_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT {
        Err("attribute value exceeds the text limit".into())
    } else {
        Ok(())
    }
}

/// Checks the value against the definition shape and returns the canonical form.
pub fn check_value(
    definition: &AttributeDefinition,
    value: &AttributeValue,
) -> Result<AttributeValue, String> {
    let matched = match (definition.value_type, value) {
        (AttributeType::Text, AttributeValue::Text(text)) => {
            check_text(text)?;
            AttributeValue::Text(text.clone())
        }
        (AttributeType::Number, AttributeValue::Number(text)) => {
            check_text(text)?;
            AttributeValue::Number(parse_decimal(text)?)
        }
        (AttributeType::Duration, AttributeValue::Duration(text)) => {
            check_text(text)?;
            AttributeValue::Duration(parse_duration(text)?)
        }
        (AttributeType::Currency, AttributeValue::Currency(text)) => {
            check_text(text)?;
            AttributeValue::Currency(parse_decimal(text)?)
        }
        (AttributeType::Enum, AttributeValue::Enum(text)) => {
            check_text(text)?;
            if !definition.options.iter().any(|option| option == text) {
                return Err("enum value is not one of the definition options".into());
            }
            AttributeValue::Enum(text.clone())
        }
        (AttributeType::Date, AttributeValue::Date(text)) => {
            check_text(text)?;
            AttributeValue::Date(parse_date(text)?)
        }
        (AttributeType::DictionaryRef, AttributeValue::DictionaryRefs(refs)) => {
            if refs.len() > MAX_DICTIONARY_REFS {
                return Err("attribute references exceed the dictionary reference limit".into());
            }
            for (index, reference) in refs.iter().enumerate() {
                if let DictionaryTarget::Literal(label) = reference {
                    check_text(label)?;
                }
                if refs[..index].contains(reference) {
                    return Err("duplicate dictionary reference in one value".into());
                }
            }
            AttributeValue::DictionaryRefs(refs.clone())
        }
        (AttributeType::Url, AttributeValue::Url(text)) => {
            check_text(text)?;
            if text.is_empty() || text.chars().any(char::is_whitespace) {
                return Err("a URL value must be whitespace-free text".into());
            }
            AttributeValue::Url(text.clone())
        }
        _ => return Err("attribute value does not match the definition type".into()),
    };
    Ok(matched)
}

/// Dictionary entries referenced by a value must exist with the definition kind.
pub fn check_dictionary_refs(
    project: &ProjectData,
    definition: &AttributeDefinition,
    refs: &[DictionaryTarget],
) -> Result<(), String> {
    for reference in refs {
        if let DictionaryTarget::Entry(entry_id) = reference {
            let entry = project
                .dictionary_entries
                .iter()
                .find(|entry| entry.id == *entry_id)
                .ok_or("attribute references a missing dictionary entry")?;
            if Some(entry.kind) != definition.dictionary_kind {
                return Err("attribute references a dictionary entry of the wrong kind".into());
            }
        }
    }
    Ok(())
}

/// Creates one definition. Fails on duplicates, limit and shape violations.
pub fn apply_create(
    project: &mut ProjectData,
    definition: AttributeDefinition,
) -> Result<(), CommandError> {
    definition.validate_shape()?;
    if project.attribute_definitions.len() >= MAX_DEFINITIONS {
        return Err(invalid("attribute definition limit exceeded"));
    }
    if project
        .attribute_definitions
        .iter()
        .any(|existing| existing.id == definition.id)
    {
        return Err(invalid("duplicate attribute definition ID"));
    }
    if project
        .attribute_definitions
        .iter()
        .any(|existing| existing.name == definition.name)
    {
        return Err(invalid("duplicate attribute definition name"));
    }
    project.attribute_definitions.push(definition);
    Ok(())
}

/// Why one existing value would not survive an update.
enum Affected {
    /// Survives unchanged.
    Keep,
    /// Dropped by `Clear`, refused by `Reject`, remapped by `MapOptions`.
    Drop,
}

fn update_effect(
    project: &ProjectData,
    current: &AttributeDefinition,
    definition: &AttributeDefinition,
    assignment: &AttributeAssignment,
) -> Affected {
    if assignment.definition != current.id {
        return Affected::Keep;
    }
    match &assignment.value {
        AttributeValue::Enum(option)
            if !definition
                .options
                .iter()
                .any(|candidate| candidate == option) =>
        {
            return Affected::Drop;
        }
        AttributeValue::DictionaryRefs(refs)
            if check_dictionary_refs(project, definition, refs).is_err() =>
        {
            return Affected::Drop;
        }
        _ => {}
    }
    // Only a narrowing that this update makes counts; a value that was already
    // outside `applies_to` (left by an earlier change) is not this update's effect.
    if let Some(target) = source_target(project, &assignment.source)
        && current.applies_to.contains(&target)
        && !definition.applies_to.contains(&target)
    {
        return Affected::Drop;
    }
    Affected::Keep
}

fn source_target(project: &ProjectData, source: &ElementKey) -> Option<AttributeTarget> {
    project
        .documents
        .iter()
        .find(|document| document.id == source.document_id)
        .and_then(|document| target_of(document, &source.xml_id))
}

/// Applies an unrestricted update (rename, description, `required`, `unit`,
/// added options, widened `applies_to`) or resolves the restricted ones.
pub fn apply_update(
    project: &mut ProjectData,
    definition: AttributeDefinition,
    resolution: AttributeUpdateResolution,
) -> Result<(), CommandError> {
    definition.validate_shape()?;
    let index = project
        .attribute_definitions
        .iter()
        .position(|existing| existing.id == definition.id)
        .ok_or_else(|| invalid("attribute definition not found"))?;
    let current = project.attribute_definitions[index].clone();
    if current.value_type != definition.value_type {
        return Err(invalid("attribute value type cannot change"));
    }
    if project
        .attribute_definitions
        .iter()
        .any(|existing| existing.id != definition.id && existing.name == definition.name)
    {
        return Err(invalid("duplicate attribute definition name"));
    }
    let mut mappings: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    if let AttributeUpdateResolution::MapOptions(options) = &resolution {
        for mapping in options {
            if !current.options.iter().any(|option| option == &mapping.from) {
                return Err(invalid("option mapping source is not a current option"));
            }
            if !definition
                .options
                .iter()
                .any(|option| option == &mapping.to)
            {
                return Err(invalid("option mapping target is not a definition option"));
            }
            mappings.insert(mapping.from.as_str(), mapping.to.as_str());
        }
    }
    let effects: Vec<Affected> = project
        .attribute_values
        .iter()
        .map(|assignment| update_effect(project, &current, &definition, assignment))
        .collect();
    if effects
        .iter()
        .any(|effect| matches!(effect, Affected::Drop))
    {
        match &resolution {
            AttributeUpdateResolution::Reject => {
                return Err(invalid("attribute update affects existing values"));
            }
            AttributeUpdateResolution::Clear => {
                let mut kept = Vec::new();
                for (assignment, effect) in std::mem::take(&mut project.attribute_values)
                    .into_iter()
                    .zip(effects)
                {
                    if matches!(effect, Affected::Keep) {
                        kept.push(assignment);
                    }
                }
                project.attribute_values = kept;
            }
            AttributeUpdateResolution::MapOptions(_) => {
                for (assignment, effect) in project.attribute_values.iter_mut().zip(&effects) {
                    if matches!(effect, Affected::Keep) {
                        continue;
                    }
                    if let AttributeValue::Enum(option) = &assignment.value
                        && let Some(to) = mappings.get(option.as_str())
                    {
                        assignment.value = AttributeValue::Enum((*to).to_owned());
                        continue;
                    }
                    return Err(invalid("attribute update affects existing values"));
                }
            }
        }
    }
    project.attribute_definitions[index] = definition;
    Ok(())
}

/// Deletes a definition, refusing, discarding or moving its values.
pub fn apply_delete(
    project: &mut ProjectData,
    definition_id: AttributeDefinitionId,
    resolution: AttributeDeleteResolution,
) -> Result<(), CommandError> {
    let index = project
        .attribute_definitions
        .iter()
        .position(|definition| definition.id == definition_id)
        .ok_or_else(|| invalid("attribute definition not found"))?;
    let current = project.attribute_definitions[index].clone();
    let used = project
        .attribute_values
        .iter()
        .any(|assignment| assignment.definition == definition_id);
    if used {
        match resolution {
            AttributeDeleteResolution::Reject => {
                return Err(invalid("attribute definition is in use"));
            }
            AttributeDeleteResolution::Discard => {
                project
                    .attribute_values
                    .retain(|assignment| assignment.definition != definition_id);
            }
            AttributeDeleteResolution::MoveTo(target_id) => {
                let target = project
                    .attribute_definitions
                    .iter()
                    .find(|definition| definition.id == target_id)
                    .ok_or_else(|| invalid("invalid replacement"))?
                    .clone();
                if target.id == definition_id {
                    return Err(invalid("invalid replacement"));
                }
                if target.value_type != current.value_type {
                    return Err(invalid("invalid replacement"));
                }
                if current.value_type == AttributeType::DictionaryRef
                    && target.dictionary_kind != current.dictionary_kind
                {
                    return Err(invalid("invalid replacement"));
                }
                if matches!(
                    current.value_type,
                    AttributeType::Number | AttributeType::Currency
                ) && target.unit != current.unit
                {
                    return Err(invalid("invalid replacement: units differ"));
                }
                let moved = project
                    .attribute_values
                    .iter()
                    .filter(|assignment| assignment.definition == definition_id);
                for assignment in moved {
                    if source_target(project, &assignment.source).is_some_and(|element| {
                        current.applies_to.contains(&element)
                            && !target.applies_to.contains(&element)
                    }) {
                        return Err(invalid(
                            "invalid replacement: target does not apply to a valued element",
                        ));
                    }
                    if project.attribute_values.iter().any(|other| {
                        other.definition == target_id && other.source == assignment.source
                    }) {
                        return Err(invalid(
                            "invalid replacement: an element already has a value for the target",
                        ));
                    }
                }
                if current.value_type == AttributeType::Enum {
                    let movable = project
                        .attribute_values
                        .iter()
                        .filter(|assignment| assignment.definition == definition_id)
                        .all(|assignment| match &assignment.value {
                            AttributeValue::Enum(option) => {
                                target.options.iter().any(|candidate| candidate == option)
                            }
                            _ => false,
                        });
                    if !movable {
                        return Err(invalid("invalid replacement"));
                    }
                }
                for assignment in &mut project.attribute_values {
                    if assignment.definition == definition_id {
                        assignment.definition = target_id;
                    }
                }
            }
        }
    }
    project.attribute_definitions.remove(index);
    Ok(())
}

/// Sets or clears values for one element. `doc(p, source.document_id, true)`
/// is checked by the caller exactly as for `SetDictionaryLinks`.
pub fn apply_set_values(
    project: &mut ProjectData,
    source: ElementKey,
    values: Vec<AttributeValueInput>,
) -> Result<(), CommandError> {
    let document = project
        .documents
        .iter()
        .find(|document| document.id == source.document_id)
        .ok_or_else(|| invalid("document not found"))?;
    // Clearing needs no target: a value left on an element its definition no
    // longer applies to (or that lost its target) must stay removable.
    let target = target_of(document, &source.xml_id);
    let mut input_ids = std::collections::HashSet::new();
    let mut applied: Vec<(AttributeDefinitionId, Option<AttributeValue>)> =
        Vec::with_capacity(values.len());
    for input in values {
        if !input_ids.insert(input.definition_id) {
            return Err(invalid("duplicate attribute definition in one set"));
        }
        let definition = project
            .attribute_definitions
            .iter()
            .find(|definition| definition.id == input.definition_id)
            .ok_or_else(|| invalid("attribute definition not found"))?;
        let value = match &input.value {
            Some(value) => {
                let target = target
                    .ok_or_else(|| invalid("element not found or carries no attribute target"))?;
                if !definition.applies_to.contains(&target) {
                    return Err(invalid(
                        "attribute definition does not apply to this element",
                    ));
                }
                let value = check_value(definition, value).map_err(invalid)?;
                if let AttributeValue::DictionaryRefs(refs) = &value {
                    check_dictionary_refs(project, definition, refs).map_err(invalid)?;
                }
                Some(value)
            }
            None => None,
        };
        applied.push((input.definition_id, value));
    }
    // Replace in place, so re-setting a value keeps the array order and an
    // unchanged value leaves the content identity unchanged.
    for (definition, value) in applied {
        let existing = project.attribute_values.iter().position(|assignment| {
            assignment.source == source && assignment.definition == definition
        });
        match (existing, value) {
            (Some(index), Some(value)) => project.attribute_values[index].value = value,
            (Some(index), None) => {
                project.attribute_values.remove(index);
            }
            (None, Some(value)) => project.attribute_values.push(AttributeAssignment {
                source: source.clone(),
                definition,
                value,
            }),
            (None, None) => {}
        }
    }
    Ok(())
}

/// Starter set (adopted decision 2): no "Owner" (overlaps `Node.owner`/`contact`)
/// and no "IT system" (overlaps `DictionaryKind::System` links). Fresh IDs on
/// every call; nothing creates these definitions implicitly.
pub fn starter_definitions() -> Vec<AttributeDefinition> {
    let definition = |name: &str,
                      description: &str,
                      value_type: AttributeType,
                      unit: Option<&str>,
                      options: &[&str],
                      dictionary_kind: Option<DictionaryKind>,
                      applies_to: &[AttributeTarget]| AttributeDefinition {
        id: AttributeDefinitionId::new(),
        name: name.to_owned(),
        description: description.to_owned(),
        value_type,
        unit: unit.map(str::to_owned),
        options: options.iter().map(|option| (*option).to_owned()).collect(),
        dictionary_kind,
        applies_to: applies_to.to_vec(),
        required: false,
    };
    let on_activities = [AttributeTarget::Activity];
    vec![
        definition(
            "Duration",
            "Elapsed time of the activity, e.g. PT2H30M (P1D is 24 elapsed hours).",
            AttributeType::Duration,
            None,
            &[],
            None,
            &on_activities,
        ),
        definition(
            "Cost",
            "Expected cost of the activity in EUR.",
            AttributeType::Currency,
            Some("EUR"),
            &[],
            None,
            &on_activities,
        ),
        definition(
            "SLA",
            "Service-level target for the activity, e.g. P1D.",
            AttributeType::Duration,
            None,
            &[],
            None,
            &on_activities,
        ),
        definition(
            "Risk",
            "Assessed process risk of the activity.",
            AttributeType::Enum,
            None,
            &["Low", "Medium", "High"],
            None,
            &on_activities,
        ),
        definition(
            "Control",
            "Internal control applied to the activity.",
            AttributeType::Text,
            None,
            &[],
            None,
            &on_activities,
        ),
        definition(
            "Process owner",
            "Role accountable for the whole process.",
            AttributeType::DictionaryRef,
            None,
            &[],
            Some(DictionaryKind::Role),
            &[AttributeTarget::Process],
        ),
    ]
}

/// The three RACI attributes (M5.3). Their IDs are fixed so BPMN round trips
/// and other projects recognise them; Responsible is never stored, it follows
/// the lane's role. Nothing creates them implicitly: the RACI view issues a
/// normal `CreateAttributeDefinition` the first time it assigns one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RaciAttribute {
    Accountable,
    Consulted,
    Informed,
}

impl RaciAttribute {
    pub const ALL: [Self; 3] = [Self::Accountable, Self::Consulted, Self::Informed];

    pub fn name(self) -> &'static str {
        match self {
            Self::Accountable => "Accountable",
            Self::Consulted => "Consulted",
            Self::Informed => "Informed",
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::Accountable => 'A',
            Self::Consulted => 'C',
            Self::Informed => 'I',
        }
    }

    pub fn id(self) -> AttributeDefinitionId {
        let text = match self {
            Self::Accountable => "5a3c0001-7a1c-4c0e-9a11-00000000aca1",
            Self::Consulted => "5a3c0002-7a1c-4c0e-9a11-00000000aca2",
            Self::Informed => "5a3c0003-7a1c-4c0e-9a11-00000000aca3",
        };
        text.parse()
            .expect("fixed RACI attribute ID is a valid UUID")
    }

    pub fn from_id(id: AttributeDefinitionId) -> Option<Self> {
        Self::ALL.into_iter().find(|raci| raci.id() == id)
    }

    pub fn definition(self) -> AttributeDefinition {
        AttributeDefinition {
            id: self.id(),
            name: self.name().to_owned(),
            description: format!("RACI: roles that are {}.", self.name().to_lowercase()),
            value_type: AttributeType::DictionaryRef,
            unit: None,
            options: Vec::new(),
            dictionary_kind: Some(DictionaryKind::Role),
            applies_to: vec![AttributeTarget::Activity],
            required: false,
        }
    }
}

/// Structural checks for `ProjectData::validate` (called from `identity.rs`).
/// `applies_to` is deliberately not checked here: it is enforced when a value
/// is set, so `replaceElement` can never make a project unloadable.
pub fn validate_project(project: &ProjectData) -> Result<(), String> {
    if project.attribute_definitions.len() > MAX_DEFINITIONS {
        return Err("attribute definition limit exceeded".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for definition in &project.attribute_definitions {
        definition.validate_shape().map_err(|error| match error {
            CommandError::InvalidParams(message) => message,
            other => format!("{other:?}"),
        })?;
        if !ids.insert(definition.id) {
            return Err("duplicate attribute definition ID".into());
        }
        if !names.insert(definition.name.as_str()) {
            return Err("duplicate attribute definition name".into());
        }
    }
    let mut sources = std::collections::HashSet::new();
    let mut semantic_cache: std::collections::HashMap<
        DocumentId,
        std::collections::HashSet<ElementSelector>,
    > = std::collections::HashMap::new();
    for assignment in &project.attribute_values {
        let definition = project
            .attribute_definitions
            .iter()
            .find(|definition| definition.id == assignment.definition)
            .ok_or("attribute definition missing")?;
        let checked = check_value(definition, &assignment.value)?;
        if checked != assignment.value {
            return Err("attribute value is not canonical".into());
        }
        if let AttributeValue::DictionaryRefs(refs) = &assignment.value {
            check_dictionary_refs(project, definition, refs)?;
        }
        if !sources.insert((assignment.source.clone(), assignment.definition)) {
            return Err("duplicate attribute value for one element and definition".into());
        }
        let semantic = semantic_cache
            .entry(assignment.source.document_id)
            .or_insert_with(|| {
                project
                    .documents
                    .iter()
                    .find(|document| document.id == assignment.source.document_id)
                    .map(|document| crate::identity::semantic_ids(document))
                    .unwrap_or_default()
            });
        if semantic.is_empty() || !semantic.contains(&assignment.source.xml_id) {
            return Err("attribute source document or element is missing".into());
        }
    }
    Ok(())
}
