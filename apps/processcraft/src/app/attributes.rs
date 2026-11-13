//! Desktop editing of custom attributes (M5.1c): the Dictionary "Attributes" tab, the inspector
//! section and the typed value editor the activity table reuses for its cells.
//!
//! Panels never mutate the model: they build `Mutation`s and submit them through the controller.
//! Value text is parsed by the core validators (`check_value`), so their messages are shown as is.
use super::*;
use processcraft_core::{
    attributes::{
        self as core, AttributeDefinition, AttributeDeleteResolution, AttributeOptionMapping,
        AttributeTarget, AttributeType, AttributeUpdateResolution, AttributeValue,
        AttributeValueInput,
    },
    commands::CommandError,
    ids::{AttributeDefinitionId, ElementKey},
    queries,
};
use std::collections::HashMap;

pub(super) type Cells = HashMap<(ElementKey, AttributeDefinitionId), CellDraft>;

pub(super) const TYPES: [(AttributeType, &str); 8] = [
    (AttributeType::Text, "Text"),
    (AttributeType::Number, "Number"),
    (AttributeType::Duration, "Duration"),
    (AttributeType::Currency, "Currency"),
    (AttributeType::Enum, "Option list"),
    (AttributeType::Date, "Date"),
    (AttributeType::DictionaryRef, "Dictionary entry"),
    (AttributeType::Url, "URL"),
];
pub(super) const TARGETS: [(AttributeTarget, &str); 8] = [
    (AttributeTarget::Activity, "Activities"),
    (AttributeTarget::Event, "Events"),
    (AttributeTarget::Gateway, "Gateways"),
    (AttributeTarget::SequenceFlow, "Sequence flows"),
    (AttributeTarget::Lane, "Lanes"),
    (AttributeTarget::Participant, "Participants"),
    (AttributeTarget::Process, "Processes"),
    (AttributeTarget::DataObject, "Data objects"),
];

pub(super) fn type_label(value_type: AttributeType) -> &'static str {
    TYPES
        .iter()
        .find(|(kind, _)| *kind == value_type)
        .map_or("", |(_, label)| label)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Entries,
    Attributes,
}

/// Text typed into a value field that is not stored yet.
pub(super) struct CellDraft {
    text: String,
    error: Option<String>,
}

/// The definition editor form; compared with the stored definition to find unsaved changes.
#[derive(Clone, PartialEq)]
pub(super) struct DefDraft {
    pub(super) name: String,
    description: String,
    value_type: AttributeType,
    unit: String,
    pub(super) options: String,
    kind: DictionaryKind,
    pub(super) applies: [bool; 8],
    required: bool,
}

impl DefDraft {
    fn blank() -> Self {
        let mut applies = [false; 8];
        applies[0] = true;
        Self {
            name: String::new(),
            description: String::new(),
            value_type: AttributeType::Text,
            unit: String::new(),
            options: String::new(),
            kind: DictionaryKind::Role,
            applies,
            required: false,
        }
    }
    fn from(definition: &AttributeDefinition) -> Self {
        Self {
            name: definition.name.clone(),
            description: definition.description.clone(),
            value_type: definition.value_type,
            unit: definition.unit.clone().unwrap_or_default(),
            options: definition.options.join("\n"),
            kind: definition.dictionary_kind.unwrap_or(DictionaryKind::Role),
            applies: TARGETS.map(|(target, _)| definition.applies_to.contains(&target)),
            required: definition.required,
        }
    }
    fn build(&self, id: AttributeDefinitionId) -> AttributeDefinition {
        let unit = self.unit.trim();
        AttributeDefinition {
            id,
            name: self.name.trim().to_owned(),
            description: self.description.clone(),
            value_type: self.value_type,
            unit: (matches!(
                self.value_type,
                AttributeType::Number | AttributeType::Currency
            ) && !unit.is_empty())
            .then(|| unit.to_owned()),
            options: if self.value_type == AttributeType::Enum {
                self.options
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect()
            } else {
                Vec::new()
            },
            dictionary_kind: (self.value_type == AttributeType::DictionaryRef).then_some(self.kind),
            applies_to: TARGETS
                .iter()
                .zip(self.applies)
                .filter(|(_, on)| *on)
                .map(|((target, _), _)| *target)
                .collect(),
            required: self.required,
        }
    }
}

/// An explicit choice the user must make before a destructive definition change is submitted.
#[allow(clippy::large_enum_variant)] // one short-lived UI value
pub(super) enum Prompt {
    Update {
        definition: AttributeDefinition,
        /// The form the prompt was opened for; editing it closes the prompt.
        form: DefDraft,
        /// Removed or renamed options and the option each one maps to.
        mappings: Vec<(String, Option<String>)>,
    },
    Delete(AttributeDefinitionId),
}

#[derive(Default)]
pub(super) struct State {
    pub(super) tab: Tab,
    /// Unsaved value text, keyed by element and definition.
    pub(super) cells: Cells,
    selected: Option<AttributeDefinitionId>,
    pub(super) draft: Option<DefDraft>,
    original: Option<DefDraft>,
    pub(super) prompt: Option<Prompt>,
    /// Table columns: view state, never project content.
    pub(super) columns: Vec<AttributeDefinitionId>,
    /// Rows picked for a multi-row edit.
    pub(super) marked: Vec<ElementKey>,
}

impl State {
    /// New or opened project: nothing carries over except the chosen tab.
    pub(super) fn reset(&mut self) {
        *self = Self {
            tab: self.tab,
            ..Self::default()
        };
    }
    pub(super) fn has_cell_drafts(&self) -> bool {
        !self.cells.is_empty()
    }
    pub(super) fn definition_changed(&self) -> bool {
        self.draft != self.original
    }
    pub(super) fn has_draft(&self) -> bool {
        self.has_cell_drafts() || self.definition_changed()
    }
}

// ---- value text <-> value ----

/// `2h 30m` -> `PT2H30M`; ISO 8601 input passes through. The core validator canonicalizes.
pub(super) fn friendly_to_iso(text: &str) -> Result<String, String> {
    const ERROR: &str =
        "duration must look like 2h 30m (units d, h, m, s) or ISO 8601 such as PT2H30M";
    let text = text.trim();
    if text.starts_with(['P', 'p']) {
        return Ok(text.to_owned());
    }
    let mut parts: [Option<String>; 4] = Default::default();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let slot = match chars.next().map(|c| c.to_ascii_lowercase()) {
            Some('d') => 0,
            Some('h') => 1,
            Some('m') => 2,
            Some('s') => 3,
            _ => return Err(ERROR.into()),
        };
        if digits.is_empty() || parts[slot].is_some() {
            return Err(ERROR.into());
        }
        parts[slot] = Some(digits);
    }
    if parts.iter().all(Option::is_none) {
        return Err(ERROR.into());
    }
    let mut iso = String::from("P");
    if let Some(days) = &parts[0] {
        iso.push_str(&format!("{days}D"));
    }
    if parts[1..].iter().any(Option::is_some) {
        iso.push('T');
        for (part, unit) in parts[1..].iter().zip(['H', 'M', 'S']) {
            if let Some(value) = part {
                iso.push_str(&format!("{value}{unit}"));
            }
        }
    }
    Ok(iso)
}

/// `PT2H30M` -> `2h 30m`; anything unexpected is shown unchanged.
pub(super) fn iso_to_friendly(iso: &str) -> String {
    let mut parts = Vec::new();
    let mut digits = String::new();
    for c in iso.chars().skip(1) {
        match c {
            '0'..='9' => digits.push(c),
            'T' => {}
            'D' | 'H' | 'M' | 'S' => {
                parts.push(format!("{digits}{}", c.to_ascii_lowercase()));
                digits.clear();
            }
            _ => return iso.to_owned(),
        }
    }
    if parts.is_empty() || !digits.is_empty() || !iso.starts_with('P') {
        iso.to_owned()
    } else {
        parts.join(" ")
    }
}

pub(super) fn display_value(project: &ProjectData, value: &AttributeValue) -> String {
    match value {
        AttributeValue::Text(text)
        | AttributeValue::Number(text)
        | AttributeValue::Currency(text)
        | AttributeValue::Enum(text)
        | AttributeValue::Date(text)
        | AttributeValue::Url(text) => text.clone(),
        AttributeValue::Duration(iso) => iso_to_friendly(iso),
        AttributeValue::DictionaryRefs(refs) => refs
            .iter()
            .map(|target| queries::target_name(project, target).unwrap_or("Unknown entry"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Parses typed text into a canonical value; blank text clears (`None`). Messages come from core.
pub(super) fn parse_text(
    definition: &AttributeDefinition,
    text: &str,
) -> Result<Option<AttributeValue>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let raw = match definition.value_type {
        AttributeType::Text => AttributeValue::Text(text.to_owned()),
        AttributeType::Number => AttributeValue::Number(trimmed.to_owned()),
        AttributeType::Currency => AttributeValue::Currency(trimmed.to_owned()),
        AttributeType::Duration => AttributeValue::Duration(friendly_to_iso(trimmed)?),
        AttributeType::Date => AttributeValue::Date(trimmed.to_owned()),
        AttributeType::Url => AttributeValue::Url(trimmed.to_owned()),
        AttributeType::Enum => AttributeValue::Enum(trimmed.to_owned()),
        AttributeType::DictionaryRef => return Err("choose entries from the list".into()),
    };
    core::check_value(definition, &raw).map(Some)
}

fn hint(definition: &AttributeDefinition) -> String {
    match definition.value_type {
        AttributeType::Number => "e.g. 12.5".into(),
        AttributeType::Currency => {
            format!("amount in {}", definition.unit.as_deref().unwrap_or(""))
        }
        AttributeType::Duration => "e.g. 2h 30m".into(),
        AttributeType::Date => "YYYY-MM-DD".into(),
        AttributeType::Url => "https://".into(),
        _ => String::new(),
    }
}

/// The rows an edit of one table row applies to: all marked rows when it is one of several.
pub(super) fn table_targets(marked: &[ElementKey], edited: &ElementKey) -> Vec<ElementKey> {
    if marked.len() > 1 && marked.contains(edited) {
        marked.to_vec()
    } else {
        vec![edited.clone()]
    }
}

// ---- typed value editor ----

pub(super) enum Edit {
    Idle,
    Commit(Option<AttributeValue>),
    Invalid(String),
}

pub(super) struct Field<'a> {
    pub(super) project: &'a ProjectData,
    pub(super) key: &'a ElementKey,
    pub(super) definition: &'a AttributeDefinition,
    pub(super) current: Option<&'a AttributeValue>,
    pub(super) enabled: bool,
    /// Table cell: no extra labels or buttons.
    pub(super) compact: bool,
    /// The row is one of several marked rows: setting the value it already has still applies.
    pub(super) force: bool,
    pub(super) name: String,
    pub(super) label: Option<egui::Id>,
}

/// One typed editor. Text-like types keep a draft and commit when the field loses focus; option
/// and dictionary choices commit as soon as they change.
pub(super) fn value_editor(ui: &mut egui::Ui, cells: &mut Cells, field: Field) -> Edit {
    let Field {
        project,
        key,
        definition,
        current,
        enabled,
        compact,
        force,
        name,
        label,
    } = field;
    let error_color = ui.visuals().error_fg_color;
    match definition.value_type {
        AttributeType::Enum => {
            let selected = match current {
                Some(AttributeValue::Enum(option)) => option.clone(),
                _ => String::new(),
            };
            let mut pick: Option<Option<String>> = None;
            let combo = ui.add_enabled_ui(enabled, |ui| {
                egui::ComboBox::from_id_salt((key, definition.id))
                    .selected_text(if selected.is_empty() {
                        "—"
                    } else {
                        &selected
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(selected.is_empty(), "—").clicked() {
                            pick = Some(None);
                        }
                        for option in &definition.options {
                            if ui.selectable_label(&selected == option, option).clicked() {
                                pick = Some(Some(option.clone()));
                            }
                        }
                    })
            });
            combo.inner.response.accessible_name(name);
            match pick {
                Some(option) => {
                    let value = option.map(AttributeValue::Enum);
                    if value.as_ref() == current && !force {
                        Edit::Idle
                    } else {
                        Edit::Commit(value)
                    }
                }
                None => Edit::Idle,
            }
        }
        AttributeType::DictionaryRef => {
            let refs = match current {
                Some(AttributeValue::DictionaryRefs(refs)) => refs.clone(),
                _ => Vec::new(),
            };
            let entries: Vec<_> = project
                .dictionary_entries
                .iter()
                .filter(|entry| Some(entry.kind) == definition.dictionary_kind)
                .collect();
            let mut next: Option<Vec<DictionaryTarget>> = None;
            let mut body = |ui: &mut egui::Ui| {
                for entry in &entries {
                    let target = DictionaryTarget::Entry(entry.id);
                    let mut on = refs.contains(&target);
                    if ui
                        .add_enabled(enabled, egui::Checkbox::new(&mut on, &entry.name))
                        .changed()
                    {
                        let mut updated = refs.clone();
                        if on {
                            updated.push(target);
                        } else {
                            updated.retain(|candidate| candidate != &target);
                        }
                        next = Some(updated);
                    }
                }
                for target in refs
                    .iter()
                    .filter(|target| matches!(target, DictionaryTarget::Literal(_)))
                {
                    if let DictionaryTarget::Literal(text) = target {
                        let mut on = true;
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Checkbox::new(&mut on, format!("Retained text: {text}")),
                            )
                            .changed()
                        {
                            let mut updated = refs.clone();
                            updated.retain(|candidate| candidate != target);
                            next = Some(updated);
                        }
                    }
                }
                if entries.is_empty() && refs.is_empty() {
                    ui.label("No dictionary entries of this kind yet.");
                }
            };
            if compact {
                let summary = current.map_or_else(String::new, |v| display_value(project, v));
                let shown = if summary.is_empty() { "—" } else { &summary };
                let response = ui.menu_button(shown, body).response;
                response.accessible_name(format!("{name}: {shown}"));
            } else {
                body(ui);
            }
            match next {
                Some(updated) => Edit::Commit(
                    (!updated.is_empty()).then_some(AttributeValue::DictionaryRefs(updated)),
                ),
                None => Edit::Idle,
            }
        }
        _ => {
            let current_text = current.map_or_else(String::new, |v| display_value(project, v));
            let slot = (key.clone(), definition.id);
            let mut text = cells
                .get(&slot)
                .map_or_else(|| current_text.clone(), |draft| draft.text.clone());
            let failed = cells.get(&slot).is_some_and(|draft| draft.error.is_some());
            let mut edit = egui::TextEdit::singleline(&mut text)
                .hint_text(hint(definition))
                .desired_width(f32::INFINITY);
            if failed {
                edit = edit.text_color(error_color);
            }
            let mut response = ui.add_enabled(enabled, edit);
            response = match label {
                Some(id) => response.labelled_by(id),
                None => response.accessible_name(name),
            };
            if response.changed() {
                if text == current_text && !force {
                    cells.remove(&slot);
                } else {
                    cells.insert(
                        slot.clone(),
                        CellDraft {
                            text: text.clone(),
                            error: None,
                        },
                    );
                }
            }
            let mut outcome = Edit::Idle;
            if response.lost_focus() && cells.contains_key(&slot) {
                match parse_text(definition, &text) {
                    Ok(value) => {
                        cells.remove(&slot);
                        if value.as_ref() != current || force {
                            outcome = Edit::Commit(value);
                        }
                    }
                    Err(message) => {
                        if let Some(draft) = cells.get_mut(&slot) {
                            draft.error = Some(message.clone());
                        }
                        outcome = Edit::Invalid(message);
                    }
                }
            }
            if let Some(message) = cells.get(&slot).and_then(|draft| draft.error.clone()) {
                if compact {
                    response.on_hover_text(message);
                } else {
                    ui.colored_label(error_color, message);
                    if ui.button("Discard edit").clicked() {
                        cells.remove(&slot);
                    }
                }
            }
            outcome
        }
    }
}

// ---- application: commits ----

impl ProcessCraft {
    /// Submits value edits as one batch (one undo step), one `SetAttributeValues` per element.
    pub(super) fn submit_attribute_edits(
        &mut self,
        edits: Vec<(ElementKey, AttributeDefinitionId, Option<AttributeValue>)>,
    ) -> bool {
        let mut grouped: Vec<(ElementKey, Vec<AttributeValueInput>)> = Vec::new();
        for (key, definition_id, value) in edits {
            let inputs = match grouped.iter().position(|(existing, _)| *existing == key) {
                Some(index) => &mut grouped[index].1,
                None => {
                    grouped.push((key, Vec::new()));
                    &mut grouped.last_mut().expect("just pushed").1
                }
            };
            inputs.retain(|input| input.definition_id != definition_id);
            inputs.push(AttributeValueInput {
                definition_id,
                value,
            });
        }
        if grouped.is_empty() {
            return true;
        }
        self.ctl.mutate(
            grouped
                .into_iter()
                .map(|(source, values)| Mutation::SetAttributeValues { source, values })
                .collect(),
        )
    }

    /// Applies one value to the edited row, or to every marked row when it is one of several.
    pub(super) fn apply_table_edit(
        &mut self,
        edited: &ElementKey,
        definition: AttributeDefinitionId,
        value: Option<AttributeValue>,
    ) -> bool {
        let targets = table_targets(&self.attributes.marked, edited);
        self.submit_attribute_edits(
            targets
                .into_iter()
                .map(|key| (key, definition, value.clone()))
                .collect(),
        )
    }

    /// Commits every pending value draft in one batch; an invalid one blocks and stays until it
    /// is fixed or discarded ("Discard attribute edits" in the message area). A draft whose
    /// element or definition is gone, or whose definition no longer applies, is dropped. Drafts
    /// commit to their own element only, so row marks are cleared first.
    pub(super) fn commit_attribute_drafts(&mut self) -> bool {
        if self.attributes.cells.is_empty() {
            return true;
        }
        let snap = self.ctl.session.snapshot();
        let pending: Vec<_> = self
            .attributes
            .cells
            .iter()
            .map(|(slot, draft)| (slot.clone(), draft.text.clone()))
            .collect();
        let mut edits = Vec::new();
        for ((key, definition_id), text) in pending {
            let definition = snap
                .project
                .attribute_definitions
                .iter()
                .find(|definition| definition.id == definition_id);
            let element = snap
                .project
                .documents
                .iter()
                .find(|document| document.id == key.document_id)
                .and_then(|document| {
                    core::target_of(document, &key.xml_id)
                        .map(|target| (target, queries::element_name(document, &key.xml_id)))
                });
            let (Some(definition), Some((target, name))) = (definition, element) else {
                self.attributes.cells.remove(&(key, definition_id));
                continue;
            };
            if !definition.applies_to.contains(&target) {
                self.attributes.cells.remove(&(key, definition_id));
                continue;
            }
            match parse_text(definition, &text) {
                Ok(value) => edits.push((key, definition_id, value)),
                Err(message) => {
                    self.ctl.error = format!(
                        "The {} attribute draft for {} is not valid: {}. Fix it or discard it first.",
                        definition.name,
                        name.filter(|name| !name.is_empty())
                            .unwrap_or("an unnamed element"),
                        message.trim_end_matches('.')
                    );
                    return false;
                }
            }
        }
        edits.sort_by(|a, b| a.0.xml_id.cmp(&b.0.xml_id));
        self.attributes.marked.clear();
        if self.submit_attribute_edits(edits) {
            self.attributes.cells.clear();
            true
        } else {
            false
        }
    }

    /// False for element kinds that carry no attributes and hold no values.
    pub(super) fn attributes_section_visible(&self, element: &ElementSelector) -> bool {
        let snap = self.ctl.session.snapshot();
        let Some(document) = snap.project.documents.get(self.doc) else {
            return false;
        };
        core::target_of(document, element).is_some()
            || snap.project.attribute_values.iter().any(|value| {
                value.source.document_id == document.id && value.source.xml_id == *element
            })
    }

    /// The Inspector "Attributes" section body for one element.
    pub(super) fn attributes_inspector_ui(
        &mut self,
        ui: &mut egui::Ui,
        element: &ElementSelector,
        editable: bool,
    ) {
        let snap = self.ctl.session.snapshot();
        let project = &snap.project;
        let Some(document) = project.documents.get(self.doc) else {
            return;
        };
        let key = ElementKey {
            document_id: document.id,
            xml_id: element.clone(),
        };
        let target = core::target_of(document, element);
        let mut applicable: Vec<&AttributeDefinition> = project
            .attribute_definitions
            .iter()
            .filter(|definition| target.is_some_and(|t| definition.applies_to.contains(&t)))
            .collect();
        applicable.sort_by(|a, b| a.name.cmp(&b.name));
        let value_of = |id: AttributeDefinitionId| {
            project
                .attribute_values
                .iter()
                .find(|value| value.source == key && value.definition == id)
                .map(|value| &value.value)
        };
        let stale: Vec<_> = project
            .attribute_definitions
            .iter()
            .filter(|definition| !applicable.iter().any(|a| a.id == definition.id))
            .filter_map(|definition| value_of(definition.id).map(|value| (definition, value)))
            .collect();
        if applicable.is_empty() && stale.is_empty() {
            ui.label(egui::RichText::new(if target.is_some() {
                "No attribute applies to this element. Define attributes under Dictionary, Attributes."
            } else {
                "This kind of element carries no attributes."
            })
            .small()
            .weak());
            return;
        }
        let mut edits = Vec::new();
        let mut invalid = None;
        for definition in applicable {
            let title = match (&definition.unit, definition.required) {
                (Some(unit), true) => format!("{} ({unit}) *", definition.name),
                (Some(unit), false) => format!("{} ({unit})", definition.name),
                (None, true) => format!("{} *", definition.name),
                (None, false) => definition.name.clone(),
            };
            let label = ui.label(egui::RichText::new(title).small().weak());
            let outcome = value_editor(
                ui,
                &mut self.attributes.cells,
                Field {
                    project,
                    key: &key,
                    definition,
                    current: value_of(definition.id),
                    enabled: editable,
                    compact: false,
                    force: false,
                    name: definition.name.clone(),
                    label: Some(label.id),
                },
            );
            match outcome {
                Edit::Commit(value) => edits.push((key.clone(), definition.id, value)),
                Edit::Invalid(message) => invalid = Some(message),
                Edit::Idle => {}
            }
        }
        if !stale.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("No longer applicable to this element")
                    .small()
                    .weak(),
            );
            for (definition, value) in stale {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{}: {}",
                        definition.name,
                        display_value(project, value)
                    ));
                    if ui
                        .add_enabled(editable, egui::Button::new("Clear"))
                        .on_hover_text(format!("Clear {}", definition.name))
                        .accessible_name(format!("Clear {}", definition.name))
                        .clicked()
                    {
                        edits.push((key.clone(), definition.id, None));
                    }
                });
            }
        }
        if let Some(message) = invalid {
            self.ctl.error = message;
        }
        if !edits.is_empty() {
            self.submit_attribute_edits(edits);
        }
    }

    // ---- Dictionary "Attributes" tab ----

    pub(super) fn add_starter_attributes(&mut self) -> bool {
        let snap = self.ctl.session.snapshot();
        let missing: Vec<_> = core::starter_definitions()
            .into_iter()
            .filter(|starter| {
                !snap
                    .project
                    .attribute_definitions
                    .iter()
                    .any(|existing| existing.name == starter.name)
            })
            .map(|definition| Mutation::CreateAttributeDefinition { definition })
            .collect();
        missing.is_empty() || self.ctl.mutate(missing)
    }

    pub(super) fn choose_definition(
        &mut self,
        project: &ProjectData,
        id: Option<AttributeDefinitionId>,
    ) {
        if self.attributes.definition_changed() {
            self.ctl.error = "Save or discard the attribute changes first.".into();
            return;
        }
        self.attributes.prompt = None;
        let form = match id.and_then(|id| project.attribute_definitions.iter().find(|d| d.id == id))
        {
            Some(definition) => DefDraft::from(definition),
            None => DefDraft::blank(),
        };
        self.attributes.selected = id;
        self.attributes.original = Some(form.clone());
        self.attributes.draft = Some(form);
    }

    /// Creates or updates the edited definition. An update that would change values in use opens
    /// the resolution prompt instead of being submitted.
    pub(super) fn save_definition_draft(&mut self) -> bool {
        let Some(draft) = self.attributes.draft.clone() else {
            return true;
        };
        if self.attributes.selected.is_some() && !self.attributes.definition_changed() {
            return true;
        }
        let snap = self.ctl.session.snapshot();
        let id = self.attributes.selected.unwrap_or_default();
        let definition = draft.build(id);
        if let Err(CommandError::InvalidParams(message)) = definition.validate_shape() {
            self.ctl.error = message;
            return false;
        }
        let Some(current) = self.attributes.selected.and_then(|id| {
            snap.project
                .attribute_definitions
                .iter()
                .find(|d| d.id == id)
        }) else {
            let created = self
                .ctl
                .mutate(vec![Mutation::CreateAttributeDefinition { definition }]);
            if created {
                self.attributes.selected = Some(id);
                self.attributes.original = Some(draft);
            }
            return created;
        };
        // Refused as is but accepted when values may be cleared: the restricted case.
        let mut trial = (*snap.project).clone();
        let refused = core::apply_update(
            &mut trial,
            definition.clone(),
            AttributeUpdateResolution::Reject,
        )
        .is_err();
        if refused {
            let mut trial = (*snap.project).clone();
            if core::apply_update(
                &mut trial,
                definition.clone(),
                AttributeUpdateResolution::Clear,
            )
            .is_ok()
            {
                let mappings = current
                    .options
                    .iter()
                    .filter(|option| !definition.options.contains(option))
                    .map(|option| (option.clone(), None))
                    .collect();
                self.attributes.prompt = Some(Prompt::Update {
                    definition,
                    form: draft,
                    mappings,
                });
                return false;
            }
        }
        self.submit_definition_update(definition, AttributeUpdateResolution::Reject)
    }

    fn submit_definition_update(
        &mut self,
        definition: AttributeDefinition,
        resolution: AttributeUpdateResolution,
    ) -> bool {
        let form = DefDraft::from(&definition);
        let saved = self.ctl.mutate(vec![Mutation::UpdateAttributeDefinition {
            definition,
            resolution,
        }]);
        if saved {
            self.attributes.prompt = None;
            self.attributes.original = Some(form.clone());
            self.attributes.draft = Some(form);
        }
        saved
    }

    pub(super) fn request_definition_delete(&mut self) {
        if self.attributes.definition_changed() {
            self.ctl.error = "Save or discard the attribute changes first.".into();
        } else if let Some(id) = self.attributes.selected {
            self.attributes.prompt = Some(Prompt::Delete(id));
        }
    }

    pub(super) fn resolve_definition_delete(
        &mut self,
        resolution: AttributeDeleteResolution,
    ) -> bool {
        let Some(Prompt::Delete(id)) = self.attributes.prompt else {
            return false;
        };
        let deleted = self.ctl.mutate(vec![Mutation::DeleteAttributeDefinition {
            definition_id: id,
            resolution,
        }]);
        if deleted {
            self.attributes.prompt = None;
            self.attributes.selected = None;
            self.attributes.draft = None;
            self.attributes.original = None;
            self.attributes.columns.retain(|column| *column != id);
        }
        deleted
    }

    pub(super) fn resolve_definition_update(
        &mut self,
        resolution: AttributeUpdateResolution,
    ) -> bool {
        let Some(Prompt::Update {
            definition,
            form,
            mappings,
        }) = self.attributes.prompt.take()
        else {
            return false;
        };
        let saved = self.submit_definition_update(definition.clone(), resolution);
        if !saved {
            // Keep the prompt and the mapping choices: pick another resolution or cancel.
            self.attributes.prompt = Some(Prompt::Update {
                definition,
                form,
                mappings,
            });
        }
        saved
    }

    pub(super) fn attribute_definitions_ui(&mut self, ui: &mut egui::Ui, project: &ProjectData) {
        // Keep the editor in step with undo/redo and external changes while nothing is unsaved.
        if let Some(id) = self.attributes.selected {
            match project.attribute_definitions.iter().find(|d| d.id == id) {
                None => {
                    self.attributes.selected = None;
                    self.attributes.draft = None;
                    self.attributes.original = None;
                    self.attributes.prompt = None;
                }
                Some(definition) if !self.attributes.definition_changed() => {
                    let form = DefDraft::from(definition);
                    self.attributes.original = Some(form.clone());
                    self.attributes.draft = Some(form);
                }
                Some(_) => {}
            }
        }
        let usage = queries::attribute_definitions(project);
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::primary_button(ui, "New attribute").clicked() {
                self.choose_definition(project, None);
            }
            let missing = starter_names()
                .iter()
                .filter(|name| usage.iter().all(|row| row.definition.name != **name))
                .count();
            if ui
                .add_enabled(missing > 0, egui::Button::new("Add starter attributes"))
                .on_hover_text(
                    "Duration, Cost, SLA, Risk and Control for activities, Process owner for processes. Existing names are skipped.",
                )
                .clicked()
            {
                self.add_starter_attributes();
            }
        });
        ui.label(
            egui::RichText::new(
                "Custom attributes you can set on model elements and show in the table",
            )
            .weak(),
        );
        let items: Vec<_> = usage
            .iter()
            .map(|row| {
                let definition = &row.definition;
                TableItem {
                    key: definition.id.to_string(),
                    cells: vec![
                        definition.name.clone(),
                        type_label(definition.value_type).into(),
                        TARGETS
                            .iter()
                            .filter(|(target, _)| definition.applies_to.contains(target))
                            .map(|(_, label)| *label)
                            .collect::<Vec<_>>()
                            .join(", "),
                        row.usage_count.to_string(),
                    ],
                    tooltip: definition.description.clone(),
                    selected: self.attributes.selected == Some(definition.id),
                }
            })
            .collect();
        if let Some((index, _)) = data_table(
            ui,
            "attribute_rows",
            &["NAME", "TYPE", "APPLIES TO", "VALUES"],
            &[170.0, 120.0, 220.0, 70.0],
            &items,
            200.0,
        ) {
            self.choose_definition(project, Some(usage[index].definition.id));
        }
        if items.is_empty() {
            ui.label(
                egui::RichText::new("No attributes yet. Create one, or add the starter set.")
                    .weak(),
            );
        }
        ui.separator();
        self.attribute_definition_editor(ui, project);
    }

    fn attribute_definition_editor(&mut self, ui: &mut egui::Ui, project: &ProjectData) {
        let Some(mut draft) = self.attributes.draft.clone() else {
            ui.label("Select an attribute or create a new one.");
            return;
        };
        let existing = self.attributes.selected.is_some();
        ui.heading(if existing {
            "Attribute details"
        } else {
            "New attribute"
        });
        if self.attributes.definition_changed() {
            ui.label("Save or discard changes before selecting another attribute.");
        }
        let label = ui.label("Name");
        ui.add_sized(
            [ui.available_width().min(400.0), 28.0],
            egui::TextEdit::singleline(&mut draft.name),
        )
        .labelled_by(label.id);
        let label = ui.label("Description");
        ui.add_sized(
            [ui.available_width().min(400.0), 56.0],
            egui::TextEdit::multiline(&mut draft.description),
        )
        .labelled_by(label.id);
        ui.label("Type");
        ui.horizontal_wrapped(|ui| {
            for (kind, text) in TYPES {
                ui.add_enabled_ui(!existing, |ui| {
                    ui.selectable_value(&mut draft.value_type, kind, text);
                });
            }
        });
        if existing {
            ui.label(
                egui::RichText::new(
                    "The type cannot change. Create a new attribute and re-enter the values instead.",
                )
                .small()
                .weak(),
            );
        }
        match draft.value_type {
            AttributeType::Number | AttributeType::Currency => {
                let label = ui.label(if draft.value_type == AttributeType::Currency {
                    "Currency code (three capital letters)"
                } else {
                    "Unit (optional)"
                });
                ui.add_sized([160.0, 28.0], egui::TextEdit::singleline(&mut draft.unit))
                    .labelled_by(label.id);
            }
            AttributeType::Enum => {
                let label = ui.label("Options (one per line)");
                ui.add_sized(
                    [ui.available_width().min(400.0), 80.0],
                    egui::TextEdit::multiline(&mut draft.options),
                )
                .labelled_by(label.id);
            }
            AttributeType::DictionaryRef => {
                ui.label("Dictionary kind");
                ui.horizontal_wrapped(|ui| {
                    for kind in [
                        DictionaryKind::Role,
                        DictionaryKind::System,
                        DictionaryKind::Document,
                    ] {
                        ui.selectable_value(
                            &mut draft.kind,
                            kind,
                            super::analyst_views::dictionary_kind_label(kind),
                        );
                    }
                });
            }
            _ => {}
        }
        ui.label("Applies to");
        ui.horizontal_wrapped(|ui| {
            for ((_, text), on) in TARGETS.iter().zip(draft.applies.iter_mut()) {
                ui.checkbox(on, *text);
            }
        });
        ui.checkbox(&mut draft.required, "Required");
        if let Some(Prompt::Update { form, .. }) = &self.attributes.prompt
            && *form != draft
        {
            self.attributes.prompt = None;
        }
        self.attributes.draft = Some(draft);
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::primary_button(ui, "Save attribute").clicked() {
                self.save_definition_draft();
            }
            if ui
                .add_enabled(
                    self.attributes.definition_changed(),
                    egui::Button::new("Discard changes"),
                )
                .clicked()
            {
                self.attributes.draft = self.attributes.original.clone();
                self.attributes.prompt = None;
            }
            if existing
                && ui
                    .add_enabled(
                        !self.attributes.definition_changed(),
                        egui::Button::new("Delete attribute…"),
                    )
                    .clicked()
            {
                self.request_definition_delete();
            }
        });
        self.definition_prompt_ui(ui, project);
    }

    fn definition_prompt_ui(&mut self, ui: &mut egui::Ui, project: &ProjectData) {
        let palette = self.theme.palette();
        match &mut self.attributes.prompt {
            None => {}
            Some(Prompt::Delete(id)) => {
                let id = *id;
                let Some(definition) = project.attribute_definitions.iter().find(|d| d.id == id)
                else {
                    return;
                };
                let used = project
                    .attribute_values
                    .iter()
                    .filter(|value| value.definition == id)
                    .count();
                let targets: Vec<_> = project
                    .attribute_definitions
                    .iter()
                    .filter(|other| move_target_ok(project, definition, other))
                    .map(|other| (other.id, other.name.clone()))
                    .collect();
                let mut choice = None;
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(crate::theme::RADIUS_CARD)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        if used == 0 {
                            ui.label(format!("Delete {}? No values use it.", definition.name));
                            if ui.button("Delete attribute").clicked() {
                                choice = Some(Some(AttributeDeleteResolution::Reject));
                            }
                        } else {
                            ui.label(format!(
                                "{} is set on {used} {}. Choose what happens to them.",
                                definition.name,
                                if used == 1 { "value" } else { "values" }
                            ));
                            if ui.button("Delete and discard the values").clicked() {
                                choice = Some(Some(AttributeDeleteResolution::Discard));
                            }
                            ui.menu_button("Move the values to…", |ui| {
                                for (target, name) in &targets {
                                    if ui.button(name).clicked() {
                                        choice =
                                            Some(Some(AttributeDeleteResolution::MoveTo(*target)));
                                        ui.close();
                                    }
                                }
                                if targets.is_empty() {
                                    ui.label("No other attribute of the same type.");
                                }
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(None);
                        }
                    });
                match choice {
                    Some(Some(resolution)) => {
                        self.resolve_definition_delete(resolution);
                    }
                    Some(None) => self.attributes.prompt = None,
                    None => {}
                }
            }
            Some(Prompt::Update {
                definition,
                mappings,
                ..
            }) => {
                let options = definition.options.clone();
                let mut choice: Option<Option<AttributeUpdateResolution>> = None;
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(crate::theme::RADIUS_CARD)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.label(
                            "This change affects values that are already set on elements. Choose what happens to them.",
                        );
                        if ui.button("Clear the affected values").clicked() {
                            choice = Some(Some(AttributeUpdateResolution::Clear));
                        }
                        for (from, to) in mappings.iter_mut() {
                            egui::ComboBox::from_label(format!("Map option {from} to"))
                                .selected_text(to.as_deref().unwrap_or("Choose an option"))
                                .show_ui(ui, |ui| {
                                    for option in &options {
                                        ui.selectable_value(to, Some(option.clone()), option);
                                    }
                                });
                        }
                        if !mappings.is_empty()
                            && ui
                                .add_enabled(
                                    mappings.iter().any(|(_, to)| to.is_some()),
                                    egui::Button::new("Apply option mapping"),
                                )
                                .clicked()
                        {
                            choice = Some(Some(AttributeUpdateResolution::MapOptions(
                                mappings
                                    .iter()
                                    .filter_map(|(from, to)| {
                                        to.clone().map(|to| AttributeOptionMapping {
                                            from: from.clone(),
                                            to,
                                        })
                                    })
                                    .collect(),
                            )));
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(None);
                        }
                    });
                match choice {
                    Some(Some(resolution)) => {
                        self.resolve_definition_update(resolution);
                    }
                    Some(None) => self.attributes.prompt = None,
                    None => {}
                }
            }
        }
    }

    /// Prompt the user must answer before more definition edits (for tests and navigation).
    #[cfg(test)]
    pub(super) fn prompt_open(&self) -> bool {
        self.attributes.prompt.is_some()
    }
}

use super::analyst_views::{TableItem, data_table};

/// Names of the starter set, computed once (the definitions themselves get fresh IDs per call).
fn starter_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        core::starter_definitions()
            .into_iter()
            .map(|definition| definition.name)
            .collect()
    })
}

/// Mirrors the `MoveTo` checks of core's `apply_delete`, so the menu lists valid targets only.
fn move_target_ok(
    project: &ProjectData,
    current: &AttributeDefinition,
    target: &AttributeDefinition,
) -> bool {
    if target.id == current.id || target.value_type != current.value_type {
        return false;
    }
    if current.value_type == AttributeType::DictionaryRef
        && target.dictionary_kind != current.dictionary_kind
    {
        return false;
    }
    if matches!(
        current.value_type,
        AttributeType::Number | AttributeType::Currency
    ) && target.unit != current.unit
    {
        return false;
    }
    project
        .attribute_values
        .iter()
        .filter(|value| value.definition == current.id)
        .all(|value| {
            let element = project
                .documents
                .iter()
                .find(|document| document.id == value.source.document_id)
                .and_then(|document| core::target_of(document, &value.source.xml_id));
            let covered = element.is_none_or(|element| {
                !current.applies_to.contains(&element) || target.applies_to.contains(&element)
            });
            let free = !project
                .attribute_values
                .iter()
                .any(|other| other.definition == target.id && other.source == value.source);
            let option = match &value.value {
                AttributeValue::Enum(option) if current.value_type == AttributeType::Enum => {
                    target.options.contains(option)
                }
                _ => true,
            };
            covered && free && option
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::ids::DocumentId;

    fn definition(name: &str, value_type: AttributeType) -> AttributeDefinition {
        AttributeDefinition {
            id: AttributeDefinitionId::new(),
            name: name.into(),
            description: String::new(),
            value_type,
            unit: None,
            options: if value_type == AttributeType::Enum {
                vec!["Low".into(), "High".into()]
            } else {
                Vec::new()
            },
            dictionary_kind: None,
            applies_to: vec![AttributeTarget::Activity],
            required: false,
        }
    }

    /// An app whose first document has `count` tasks and one Number definition `Cost`.
    fn app_with_tasks(
        count: usize,
    ) -> (
        ProcessCraft,
        DocumentId,
        Vec<ElementSelector>,
        AttributeDefinitionId,
    ) {
        let mut app = ProcessCraft::default();
        let mut project = controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        let mut tasks = Vec::new();
        for _ in 0..count {
            let id = controller::id("Node");
            document.processes[0].nodes.push(Node {
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: id.clone().into(),
                name: Some("Task".into()),
                documentation: String::new().into(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            });
            tasks.push(id.into());
        }
        let document_id = document.id;
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let cost = definition("Cost", AttributeType::Number);
        let id = cost.id;
        assert!(app.ctl.mutate(vec![Mutation::CreateAttributeDefinition {
            definition: cost
        }]));
        (app, document_id, tasks, id)
    }

    fn generation(app: &ProcessCraft) -> u64 {
        app.ctl.session.snapshot().generation
    }

    #[test]
    fn invalid_draft_does_not_submit_and_is_kept() {
        let (mut app, document_id, tasks, cost) = app_with_tasks(1);
        let key = ElementKey {
            document_id,
            xml_id: tasks[0].clone(),
        };
        app.attributes.cells.insert(
            (key.clone(), cost),
            CellDraft {
                text: "abc".into(),
                error: None,
            },
        );
        let before = generation(&app);
        assert!(!app.commit_attribute_drafts());
        assert_eq!(generation(&app), before);
        assert!(app.attributes.has_cell_drafts());
        assert!(
            app.ctl.error.contains("not a decimal number"),
            "{}",
            app.ctl.error
        );
        // Navigation is blocked while the draft is invalid.
        assert!(!app.prepare_navigation());
        app.attributes.cells.get_mut(&(key, cost)).unwrap().text = "12.50".into();
        assert!(app.commit_attribute_drafts());
        assert!(!app.attributes.has_cell_drafts());
        let snap = app.ctl.session.snapshot();
        assert_eq!(
            snap.project.attribute_values[0].value,
            AttributeValue::Number("12.5".into())
        );
    }

    #[test]
    fn multi_row_edit_is_one_undo_step() {
        let (mut app, document_id, tasks, cost) = app_with_tasks(3);
        let keys: Vec<ElementKey> = tasks
            .iter()
            .map(|xml_id| ElementKey {
                document_id,
                xml_id: xml_id.clone(),
            })
            .collect();
        app.attributes.marked = keys.clone();
        let before = generation(&app);
        assert!(app.apply_table_edit(&keys[1], cost, Some(AttributeValue::Number("5".into()))));
        assert_eq!(generation(&app), before + 1);
        assert_eq!(app.ctl.session.snapshot().project.attribute_values.len(), 3);
        assert!(app.ctl.apply(Request::Undo));
        assert!(
            app.ctl
                .session
                .snapshot()
                .project
                .attribute_values
                .is_empty()
        );
        // An unmarked row edits only itself.
        app.attributes.marked.clear();
        assert!(app.apply_table_edit(&keys[0], cost, Some(AttributeValue::Number("7".into()))));
        assert_eq!(app.ctl.session.snapshot().project.attribute_values.len(), 1);
        // A marked key of another document never joins the edit.
        let foreign = ElementKey {
            document_id: DocumentId::new(),
            xml_id: tasks[0].clone(),
        };
        assert_eq!(
            table_targets(&[keys[0].clone(), foreign.clone()], &keys[0]),
            vec![keys[0].clone(), foreign]
        );
        assert_eq!(
            table_targets(&[keys[0].clone()], &keys[0]),
            vec![keys[0].clone()]
        );
    }

    #[test]
    fn marked_rows_are_limited_to_visible_rows_each_frame() {
        let (mut app, document_id, tasks, _) = app_with_tasks(2);
        app.view = 3;
        app.attributes.marked = vec![
            ElementKey {
                document_id,
                xml_id: tasks[0].clone(),
            },
            ElementKey {
                document_id: DocumentId::new(),
                xml_id: tasks[1].clone(),
            },
        ];
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| app.analyst_views_panel(ui),
        );
        output.textures_delta.clear();
        assert_eq!(app.attributes.marked.len(), 1);
        assert_eq!(app.attributes.marked[0].document_id, document_id);
    }

    #[test]
    fn new_or_opened_project_discards_attribute_state() {
        let (mut app, document_id, tasks, cost) = app_with_tasks(1);
        app.attributes.cells.insert(
            (
                ElementKey {
                    document_id,
                    xml_id: tasks[0].clone(),
                },
                cost,
            ),
            CellDraft {
                text: "x".into(),
                error: None,
            },
        );
        app.choose_definition(&app.ctl.session.snapshot().project.clone(), Some(cost));
        app.attributes.draft.as_mut().unwrap().name = "Changed".into();
        app.attributes.marked = vec![ElementKey {
            document_id,
            xml_id: tasks[0].clone(),
        }];
        assert!(app.has_drafts());
        app.discard_property_drafts();
        assert!(!app.has_drafts());
        assert!(app.attributes.marked.is_empty() && !app.attributes.definition_changed());
        assert!(app.prepare_navigation());
    }

    #[test]
    fn unusable_drafts_are_pruned_and_errors_name_the_element() {
        let (mut app, document_id, tasks, cost) = app_with_tasks(1);
        let key = ElementKey {
            document_id,
            xml_id: tasks[0].clone(),
        };
        let slot = (key.clone(), cost);
        let bad = || CellDraft {
            text: "abc".into(),
            error: None,
        };
        app.attributes.cells.insert(slot.clone(), bad());
        assert!(!app.commit_attribute_drafts());
        assert!(app.ctl.error.contains("for Task"), "{}", app.ctl.error);
        // The definition stops applying to activities: the draft is dropped, not a trap.
        let mut narrowed = app.ctl.session.snapshot().project.attribute_definitions[0].clone();
        narrowed.applies_to = vec![AttributeTarget::Event];
        assert!(app.ctl.mutate(vec![Mutation::UpdateAttributeDefinition {
            definition: narrowed,
            resolution: AttributeUpdateResolution::Clear,
        }]));
        assert!(app.commit_attribute_drafts());
        assert!(!app.attributes.has_cell_drafts());
        // A draft of a vanished element is dropped too.
        app.attributes.cells.insert(
            (
                ElementKey {
                    document_id,
                    xml_id: controller::id("Node").into(),
                },
                cost,
            ),
            bad(),
        );
        assert!(app.commit_attribute_drafts());
        assert!(!app.attributes.has_cell_drafts());
    }

    #[test]
    fn move_targets_mirror_core_checks() {
        let (app, _, _, cost) = app_with_tasks(1);
        let project = app.ctl.session.snapshot().project.clone();
        let current = project.attribute_definitions[0].clone();
        let mut euro = definition("Euro cost", AttributeType::Number);
        euro.unit = Some("EUR".into());
        let same = definition("Other cost", AttributeType::Number);
        let text = definition("Note", AttributeType::Text);
        assert!(move_target_ok(&project, &current, &same));
        assert!(!move_target_ok(&project, &current, &euro));
        assert!(!move_target_ok(&project, &current, &text));
        assert!(!move_target_ok(&project, &current, &current));
        let _ = cost;
    }

    #[test]
    fn drafts_survive_view_changes() {
        let (mut app, document_id, tasks, cost) = app_with_tasks(1);
        let key = ElementKey {
            document_id,
            xml_id: tasks[0].clone(),
        };
        app.attributes.cells.insert(
            (key.clone(), cost),
            CellDraft {
                text: "12".into(),
                error: None,
            },
        );
        app.choose_definition(&app.ctl.session.snapshot().project.clone(), Some(cost));
        app.attributes.draft.as_mut().unwrap().name = "Cost renamed".into();
        let before = generation(&app);
        let ctx = egui::Context::default();
        for (view, tab) in [
            (3, Tab::Entries),
            (1, Tab::Entries),
            (2, Tab::Attributes),
            (2, Tab::Entries),
            (3, Tab::Entries),
        ] {
            app.view = view;
            app.attributes.tab = tab;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| app.analyst_views_panel(ui),
            );
            output.textures_delta.clear();
        }
        assert_eq!(app.attributes.cells[&(key, cost)].text, "12");
        assert_eq!(app.attributes.draft.as_ref().unwrap().name, "Cost renamed");
        assert!(app.attributes.has_draft());
        assert_eq!(generation(&app), before);
    }

    #[test]
    fn durations_use_friendly_text_and_core_messages() {
        let duration = definition("Time", AttributeType::Duration);
        assert_eq!(
            parse_text(&duration, "2h 30m").unwrap(),
            Some(AttributeValue::Duration("PT2H30M".into()))
        );
        assert_eq!(
            parse_text(&duration, "90m").unwrap(),
            Some(AttributeValue::Duration("PT1H30M".into()))
        );
        assert_eq!(
            parse_text(&duration, "1d").unwrap(),
            Some(AttributeValue::Duration("P1D".into()))
        );
        assert!(parse_text(&duration, "soon").is_err());
        assert!(parse_text(&duration, "2h 3h").is_err());
        assert_eq!(iso_to_friendly("PT2H30M"), "2h 30m");
        assert_eq!(iso_to_friendly("P1DT30S"), "1d 30s");
        assert_eq!(parse_text(&duration, "  ").unwrap(), None);
        let number = definition("Cost", AttributeType::Number);
        assert_eq!(
            parse_text(&number, "x").unwrap_err(),
            "not a decimal number"
        );
        let mut date = definition("Due", AttributeType::Date);
        date.applies_to = vec![AttributeTarget::Event];
        assert!(parse_text(&date, "2026-02-30").is_err());
        assert!(parse_text(&date, "2026-02-28").unwrap().is_some());
    }

    #[test]
    fn starter_attributes_are_added_once() {
        let mut app = ProcessCraft::default();
        assert!(app.add_starter_attributes());
        let count = app
            .ctl
            .session
            .snapshot()
            .project
            .attribute_definitions
            .len();
        assert_eq!(count, core::starter_definitions().len());
        let before = generation(&app);
        assert!(app.add_starter_attributes());
        assert_eq!(generation(&app), before);
    }

    #[test]
    fn stale_value_is_shown_and_clearable() {
        let mut project = controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        let id = controller::id("Node");
        document.processes[0].nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: id.clone().into(),
            name: Some("Task".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        });
        let key = ElementKey {
            document_id: document.id,
            xml_id: id.into(),
        };
        // The definition no longer applies to activities, but the value is still stored.
        let mut stale = definition("Old", AttributeType::Text);
        stale.applies_to = vec![AttributeTarget::Event];
        project.attribute_values.push(core::AttributeAssignment {
            source: key.clone(),
            definition: stale.id,
            value: AttributeValue::Text("left over".into()),
        });
        let stale_id = stale.id;
        project.attribute_definitions.push(stale);
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        app.doc = 0;
        let ctx = egui::Context::default();
        let selector = key.xml_id.clone();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.attributes_inspector_ui(ui, &selector, true)
        });
        output.textures_delta.clear();
        assert!(app.submit_attribute_edits(vec![(key, stale_id, None)]));
        assert!(
            app.ctl
                .session
                .snapshot()
                .project
                .attribute_values
                .is_empty()
        );
    }

    #[test]
    fn editable_cell_callback_replaces_the_default_label() {
        let ctx = egui::Context::default();
        let items = vec![TableItem {
            key: "row".into(),
            cells: vec!["Row".into(), "extra".into()],
            tooltip: String::new(),
            selected: false,
        }];
        let mut seen = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| {
                super::super::analyst_views::data_table_editable(
                    ui,
                    "callback_test",
                    &["A", "B"],
                    &[150.0, 150.0],
                    &items,
                    200.0,
                    &mut |ui, row, column| {
                        seen.push((row, column));
                        if column == 1 {
                            ui.label("edited");
                            true
                        } else {
                            false
                        }
                    },
                );
            },
        );
        output.textures_delta.clear();
        assert!(seen.contains(&(0, 0)) && seen.contains(&(0, 1)));
    }
}
