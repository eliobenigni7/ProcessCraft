//! Custom attribute values in the ProcessCraft extension namespace (M5.1b).
//!
//! Export appends one `pc:attributes` element as the last child of an owner's
//! `extensionElements`. Import lifts well-formed elements out of the parsed
//! extension content; anything malformed stays ordinary extension content and
//! is named in the import limitations (preserved, never reinterpreted).
//! Import input is a trust boundary: every value goes through the same
//! validators as the command pipeline (`attributes::check_value`).
use super::{B, PC_ATTRIBUTES_NS as NS};
use processcraft_core::{
    attributes::{
        AttributeAssignment, AttributeDefinition, AttributeTarget, AttributeType, AttributeValue,
        AttributeValueInput, MAX_DEFINITIONS, MAX_ENUM_OPTIONS, MAX_TEXT, check_value, target_of,
    },
    bpmn_metadata::{BaseExtras, BaseMetadata, ExtensionElements},
    bpmn_xml::{NamespaceBinding, XmlAttribute, XmlContent, XmlElement, XmlNode},
    commands::Mutation,
    ids::{AttributeDefinitionId, DictionaryEntryId, ElementIdentity, ElementKey, ElementSelector},
    metadata_visit::{visit_base_metadata_mut, visit_legacy_metadata_mut},
    model::{BpmnDocument, DictionaryEntry, DictionaryKind, DictionaryTarget, ProjectData},
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Definitions and values read from `pc:attributes` elements of one imported document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedAttributes {
    /// `applies_to` is the union of the targets that carry values; enum
    /// `options` are the options seen in values (the file carries no more).
    pub definitions: Vec<AttributeDefinition>,
    pub values: Vec<ImportedAttributeValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedAttributeValue {
    pub owner: ElementSelector,
    pub definition: AttributeDefinitionId,
    pub value: AttributeValue,
    /// Names of the `pc:ref` elements, parallel to a `DictionaryRefs` value
    /// (empty otherwise); used when an entry ID is unknown to the project.
    pub ref_labels: Vec<String>,
    /// The `pc:attribute` element as read, so a value the project cannot take
    /// can be put back into the document's extension content.
    pub source: XmlElement,
}

fn type_name(value_type: AttributeType) -> &'static str {
    match value_type {
        AttributeType::Text => "text",
        AttributeType::Number => "number",
        AttributeType::Duration => "duration",
        AttributeType::Currency => "currency",
        AttributeType::Enum => "enum",
        AttributeType::Date => "date",
        AttributeType::DictionaryRef => "dictionaryRef",
        AttributeType::Url => "url",
    }
}
fn type_from(name: &str) -> Option<AttributeType> {
    Some(match name {
        "text" => AttributeType::Text,
        "number" => AttributeType::Number,
        "duration" => AttributeType::Duration,
        "currency" => AttributeType::Currency,
        "enum" => AttributeType::Enum,
        "date" => AttributeType::Date,
        "dictionaryRef" => AttributeType::DictionaryRef,
        "url" => AttributeType::Url,
        _ => return None,
    })
}
fn kind_name(kind: DictionaryKind) -> &'static str {
    match kind {
        DictionaryKind::Role => "role",
        DictionaryKind::System => "system",
        DictionaryKind::Document => "document",
    }
}
fn kind_from(name: &str) -> Option<DictionaryKind> {
    Some(match name {
        "role" => DictionaryKind::Role,
        "system" => DictionaryKind::System,
        "document" => DictionaryKind::Document,
        _ => return None,
    })
}

fn is_xml_whitespace(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}
fn is_pc(element: &XmlElement, local: &str) -> bool {
    element.namespace == NS && element.local.to_string() == local
}
fn plain_attributes(
    attributes: &[XmlAttribute],
    allowed: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for attribute in attributes {
        let local = attribute.local.to_string();
        if !attribute.namespace.is_empty() || attribute.prefix.is_some() {
            return Err(format!("namespaced attribute {local}"));
        }
        if !allowed.contains(&local.as_str()) {
            return Err(format!("unknown attribute {local}"));
        }
        if map.insert(local.clone(), attribute.value.clone()).is_some() {
            return Err(format!("duplicate attribute {local}"));
        }
    }
    Ok(map)
}

struct Parsed {
    /// A single-value definition candidate (an enum carries only its own option).
    definition: AttributeDefinition,
    value: AttributeValue,
    labels: Vec<String>,
}

fn parse_attribute(element: &XmlElement) -> Result<Parsed, String> {
    if !is_pc(element, "attribute") {
        return Err("unexpected child element".into());
    }
    let map = plain_attributes(
        &element.attributes,
        &["definition", "name", "type", "unit", "dictionaryKind"],
    )?;
    let id: AttributeDefinitionId = map
        .get("definition")
        .ok_or("missing definition")?
        .parse()
        .map_err(|_| "definition is not a UUID".to_owned())?;
    let name = map.get("name").ok_or("missing name")?.clone();
    let type_text = map.get("type").ok_or("missing type")?;
    let value_type = type_from(type_text).ok_or_else(|| format!("unknown type {type_text}"))?;
    let dictionary_kind = map
        .get("dictionaryKind")
        .map(|kind| kind_from(kind).ok_or_else(|| format!("unknown dictionaryKind {kind}")))
        .transpose()?;
    let mut labels = Vec::new();
    let value = if value_type == AttributeType::DictionaryRef {
        let mut refs = Vec::new();
        for child in &element.children {
            match child {
                XmlNode::Text { text } if is_xml_whitespace(text) => {}
                XmlNode::Element { element: reference } if is_pc(reference, "ref") => {
                    let map = plain_attributes(&reference.attributes, &["entry", "name"])?;
                    if !reference.children.is_empty() {
                        return Err("pc:ref must be empty".into());
                    }
                    let label = map.get("name").ok_or("pc:ref needs a name")?.clone();
                    if label.len() > MAX_TEXT {
                        return Err("pc:ref name exceeds the text limit".into());
                    }
                    refs.push(match map.get("entry") {
                        Some(entry) => {
                            let entry: DictionaryEntryId = entry
                                .parse()
                                .map_err(|_| "pc:ref entry is not a UUID".to_owned())?;
                            DictionaryTarget::Entry(entry)
                        }
                        None => DictionaryTarget::Literal(label.clone()),
                    });
                    labels.push(label);
                }
                _ => return Err("unexpected child".into()),
            }
        }
        AttributeValue::DictionaryRefs(refs)
    } else {
        let mut text = String::new();
        for child in &element.children {
            match child {
                XmlNode::Text { text: part } => text.push_str(part),
                _ => return Err("unexpected child".into()),
            }
        }
        match value_type {
            AttributeType::Text => AttributeValue::Text(text),
            AttributeType::Number => AttributeValue::Number(text),
            AttributeType::Duration => AttributeValue::Duration(text),
            AttributeType::Currency => AttributeValue::Currency(text),
            AttributeType::Enum => AttributeValue::Enum(text),
            AttributeType::Date => AttributeValue::Date(text),
            AttributeType::Url => AttributeValue::Url(text),
            AttributeType::DictionaryRef => unreachable!("handled above"),
        }
    };
    let options = match &value {
        AttributeValue::Enum(option) => vec![option.clone()],
        _ => Vec::new(),
    };
    let definition = AttributeDefinition {
        id,
        name,
        description: String::new(),
        value_type,
        unit: map.get("unit").cloned(),
        options,
        dictionary_kind,
        applies_to: vec![AttributeTarget::Activity],
        required: false,
    };
    shape(&definition)?;
    let value = check_value(&definition, &value)?;
    Ok(Parsed {
        definition,
        value,
        labels,
    })
}

fn shape(definition: &AttributeDefinition) -> Result<(), String> {
    use processcraft_core::commands::CommandError;
    definition.validate_shape().map_err(|e| match e {
        CommandError::InvalidParams(message) => message,
        other => format!("{other:?}"),
    })
}

fn merge_definition(
    definitions: &mut Vec<AttributeDefinition>,
    candidate: AttributeDefinition,
    target: AttributeTarget,
) -> Result<(), String> {
    let Some(existing) = definitions.iter_mut().find(|d| d.id == candidate.id) else {
        if definitions.len() >= MAX_DEFINITIONS {
            return Err("attribute definition limit exceeded".into());
        }
        let mut candidate = candidate;
        candidate.applies_to = vec![target];
        definitions.push(candidate);
        return Ok(());
    };
    if existing.name != candidate.name
        || existing.value_type != candidate.value_type
        || existing.unit != candidate.unit
        || existing.dictionary_kind != candidate.dictionary_kind
    {
        return Err(format!(
            "definition {} is declared inconsistently",
            candidate.id
        ));
    }
    for option in candidate.options {
        if !existing.options.contains(&option) {
            existing.options.push(option);
        }
    }
    if existing.options.len() > MAX_ENUM_OPTIONS {
        return Err("attribute options exceed the enum option limit".into());
    }
    if !existing.applies_to.contains(&target) {
        existing.applies_to.push(target);
    }
    Ok(())
}

fn parse_all(
    element: &XmlElement,
    owner: &ElementSelector,
    target: AttributeTarget,
    definitions: &mut Vec<AttributeDefinition>,
    seen: &mut HashSet<(ElementSelector, AttributeDefinitionId)>,
) -> Result<Vec<ImportedAttributeValue>, String> {
    if !element.attributes.is_empty() {
        return Err("pc:attributes takes no attributes".into());
    }
    let mut values = Vec::new();
    for child in &element.children {
        match child {
            XmlNode::Text { text } if is_xml_whitespace(text) => {}
            XmlNode::Element { element } => {
                if values.len() >= MAX_DEFINITIONS {
                    return Err("too many attributes on one element".into());
                }
                let parsed = parse_attribute(element)?;
                if !seen.insert((owner.clone(), parsed.definition.id)) {
                    return Err("an attribute definition is set twice on one element".into());
                }
                let definition = parsed.definition.id;
                merge_definition(definitions, parsed.definition, target)?;
                values.push(ImportedAttributeValue {
                    owner: owner.clone(),
                    definition,
                    value: parsed.value,
                    ref_labels: parsed.labels,
                    source: element.clone(),
                });
            }
            _ => return Err("unexpected child".into()),
        }
    }
    Ok(values)
}

fn collect(
    out: &mut Vec<(Option<ElementSelector>, XmlElement)>,
    owner: Option<&ElementIdentity>,
    extension: Option<&ExtensionElements>,
) {
    for node in extension.iter().flat_map(|e| &e.content.nodes) {
        if let XmlNode::Element { element } = node
            && is_pc(element, "attributes")
        {
            let selector = owner.filter(|o| o.xml_id().is_some()).map(|o| o.selector());
            out.push((selector, element.clone()));
        }
    }
}

fn strip(extension: &mut Option<ExtensionElements>, mut remove: Vec<XmlElement>) {
    let Some(content) = extension else {
        return;
    };
    content.content.nodes.retain(|node| {
        let XmlNode::Element { element } = node else {
            return true;
        };
        match remove.iter().position(|candidate| candidate == element) {
            Some(index) => {
                remove.swap_remove(index);
                false
            }
            None => true,
        }
    });
    if content
        .content
        .nodes
        .iter()
        .all(|node| matches!(node, XmlNode::Text { text } if is_xml_whitespace(text)))
    {
        *extension = None;
    }
}

/// Lifts every well-formed `pc:attributes` element out of the document's
/// extension content. Elements that fail validation stay in place and add a
/// line to `limitations`.
pub(super) fn lift(
    document: &mut BpmnDocument,
    limitations: &mut Vec<String>,
) -> ImportedAttributes {
    let mut candidates = Vec::new();
    visit_base_metadata_mut(document, |owner, base| {
        collect(
            &mut candidates,
            owner,
            base.as_ref().and_then(|b| b.extension_elements.as_ref()),
        )
    });
    visit_legacy_metadata_mut(document, |owner, _, extras| {
        collect(
            &mut candidates,
            Some(owner),
            extras.as_ref().and_then(|e| e.extension_elements.as_ref()),
        )
    });
    let mut definitions = Vec::new();
    let mut seen = HashSet::new();
    let mut values = Vec::new();
    let mut removal: HashMap<ElementSelector, Vec<XmlElement>> = HashMap::new();
    for (owner, element) in candidates {
        let outcome = (|| {
            let owner = owner.clone().ok_or("the owner has no XML ID")?;
            let target =
                target_of(document, &owner).ok_or("the owner carries no attribute target")?;
            let mut trial_definitions = definitions.clone();
            let mut trial_seen = seen.clone();
            let parsed = parse_all(
                &element,
                &owner,
                target,
                &mut trial_definitions,
                &mut trial_seen,
            )?;
            for definition in &trial_definitions {
                shape(definition)?;
            }
            Ok::<_, String>((owner, parsed, trial_definitions, trial_seen))
        })();
        match outcome {
            Ok((owner, parsed, trial_definitions, trial_seen)) => {
                definitions = trial_definitions;
                seen = trial_seen;
                values.extend(parsed);
                removal.entry(owner).or_default().push(element);
            }
            Err(reason) => {
                let at = match owner {
                    Some(ElementSelector::Xml(id)) => format!("<{id}>"),
                    _ => "an element".into(),
                };
                limitations.push(format!(
                    "pc:attributes on {at} not read ({reason}); kept as extension content"
                ));
            }
        }
    }
    if !removal.is_empty() {
        let mut take = |owner: &ElementIdentity| removal.remove(&owner.selector());
        visit_base_metadata_mut(document, |owner, base| {
            if let Some(remove) = owner.and_then(&mut take) {
                if let Some(metadata) = base.as_mut() {
                    strip(&mut metadata.extension_elements, remove);
                }
                if base.as_ref() == Some(&BaseMetadata::default()) {
                    *base = None;
                }
            }
        });
        visit_legacy_metadata_mut(document, |owner, _, extras| {
            if let Some(remove) = take(owner) {
                if let Some(extras) = extras.as_mut() {
                    strip(&mut extras.extension_elements, remove);
                }
                if extras.as_ref() == Some(&BaseExtras::default()) {
                    *extras = None;
                }
            }
        });
    }
    ImportedAttributes {
        definitions,
        values,
    }
}

/// Values of one document, with the project data needed to write them.
pub(super) struct ExportAttributes<'a> {
    pub definitions: &'a [AttributeDefinition],
    pub entries: &'a [DictionaryEntry],
    pub values: Vec<&'a AttributeAssignment>,
}

fn xml_name(name: &str) -> processcraft_core::ids::XmlId {
    name.parse().expect("constant NCName")
}
fn plain(name: &str, value: String) -> XmlAttribute {
    XmlAttribute {
        namespace: String::new(),
        local: xml_name(name),
        prefix: None,
        value,
    }
}
fn pc_element(local: &str, attributes: Vec<XmlAttribute>, children: Vec<XmlNode>) -> XmlElement {
    XmlElement {
        namespace: NS.into(),
        local: xml_name(local),
        prefix: Some(xml_name("pc")),
        namespaces: vec![NamespaceBinding {
            prefix: "pc".into(),
            uri: NS.into(),
        }],
        attributes,
        children,
    }
}

fn attribute_node(
    attributes: &ExportAttributes,
    assignment: &AttributeAssignment,
) -> Result<XmlNode, String> {
    let definition = attributes
        .definitions
        .iter()
        .find(|d| d.id == assignment.definition)
        .ok_or("attribute definition not found")?;
    let mut xml = vec![
        plain("definition", definition.id.to_string()),
        plain("name", definition.name.clone()),
        plain("type", type_name(definition.value_type).into()),
    ];
    if let Some(unit) = &definition.unit {
        xml.push(plain("unit", unit.clone()));
    }
    if let Some(kind) = definition.dictionary_kind {
        xml.push(plain("dictionaryKind", kind_name(kind).into()));
    }
    let children = match &assignment.value {
        AttributeValue::Text(text)
        | AttributeValue::Number(text)
        | AttributeValue::Duration(text)
        | AttributeValue::Currency(text)
        | AttributeValue::Enum(text)
        | AttributeValue::Date(text)
        | AttributeValue::Url(text) => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![XmlNode::Text { text: text.clone() }]
            }
        }
        AttributeValue::DictionaryRefs(refs) => {
            let mut nodes = Vec::new();
            for reference in refs {
                let (entry, name) = match reference {
                    DictionaryTarget::Entry(id) => {
                        let entry = attributes
                            .entries
                            .iter()
                            .find(|entry| entry.id == *id)
                            .ok_or("dictionary entry not found")?;
                        (Some(plain("entry", id.to_string())), entry.name.clone())
                    }
                    DictionaryTarget::Literal(name) => (None, name.clone()),
                };
                let mut reference_attributes: Vec<_> = entry.into_iter().collect();
                reference_attributes.push(plain("name", name));
                nodes.push(XmlNode::Element {
                    element: pc_element("ref", reference_attributes, Vec::new()),
                });
            }
            nodes
        }
    };
    Ok(XmlNode::Element {
        element: pc_element("attribute", xml, children),
    })
}

fn append(extension: &mut Option<ExtensionElements>, element: XmlElement) {
    extension
        .get_or_insert_with(|| ExtensionElements {
            content: XmlContent {
                namespaces: vec![NamespaceBinding {
                    prefix: String::new(),
                    uri: B.into(),
                }],
                nodes: Vec::new(),
            },
        })
        .content
        .nodes
        .push(XmlNode::Element { element });
}

/// Appends `pc:attributes` as the last child of each owner's `extensionElements`.
/// An owner without a qualified metadata writer blocks the export.
pub(super) fn inject(
    document: &mut BpmnDocument,
    attributes: &ExportAttributes,
) -> Result<(), String> {
    let mut pending: Vec<(ElementSelector, Vec<XmlNode>)> = Vec::new();
    for assignment in &attributes.values {
        let node = attribute_node(attributes, assignment)?;
        match pending
            .iter_mut()
            .find(|(selector, _)| *selector == assignment.source.xml_id)
        {
            Some((_, nodes)) => nodes.push(node),
            None => pending.push((assignment.source.xml_id.clone(), vec![node])),
        }
    }
    let elements: Vec<(ElementSelector, XmlElement)> = pending
        .into_iter()
        .map(|(selector, nodes)| (selector, pc_element("attributes", Vec::new(), nodes)))
        .collect();
    attach(document, elements)
}

/// Appends each element as the last extension child of its owner.
fn attach(
    document: &mut BpmnDocument,
    mut elements: Vec<(ElementSelector, XmlElement)>,
) -> Result<(), String> {
    let mut take = |owner: &ElementIdentity| {
        let index = elements
            .iter()
            .position(|(selector, _)| *selector == owner.selector())?;
        Some(elements.swap_remove(index).1)
    };
    let mut invalid = None;
    let mut check = |extension: &Option<ExtensionElements>| {
        if let Some(extension) = extension
            && let Err(error) = processcraft_core::bpmn_xml::validate_extension(&extension.content)
        {
            invalid.get_or_insert(error);
        }
    };
    visit_base_metadata_mut(document, |owner, base| {
        if let Some(owner) = owner
            && let Some(element) = take(owner)
        {
            let base = base.get_or_insert_with(BaseMetadata::default);
            append(&mut base.extension_elements, element);
            check(&base.extension_elements);
        }
    });
    visit_legacy_metadata_mut(document, |owner, _, extras| {
        if let Some(element) = take(owner) {
            let extras = extras.get_or_insert_with(BaseExtras::default);
            append(&mut extras.extension_elements, element);
            check(&extras.extension_elements);
        }
    });
    if let Some(error) = invalid {
        return Err(format!(
            "attribute values are not valid XML content: {error}"
        ));
    }
    if !elements.is_empty() {
        let owners: Vec<String> = elements
            .iter()
            .map(|(selector, _)| match selector {
                ElementSelector::Xml(id) => id.to_string(),
                ElementSelector::Native { native_id } => native_id.to_string(),
            })
            .collect();
        return Err(format!(
            "Attribute values on {} have no qualified metadata writer; remove them before export",
            owners.join(", ")
        ));
    }
    Ok(())
}

/// Values written by `inject` plus the ones the document already kept must be
/// exactly the values `lift` reads back.
pub(super) fn matches_written(
    imported: &ImportedAttributes,
    kept: &ImportedAttributes,
    written: &ExportAttributes,
) -> bool {
    imported.values.len() == written.values.len() + kept.values.len()
        && kept.values.iter().all(|k| {
            imported
                .values
                .iter()
                .any(|v| v.owner == k.owner && v.definition == k.definition && v.value == k.value)
        })
        && written.values.iter().all(|assignment| {
            imported.values.iter().any(|value| {
                value.owner == assignment.source.xml_id
                    && value.definition == assignment.definition
                    && value.value == assignment.value
            })
        })
        && imported.definitions.iter().all(|definition| {
            kept.definitions.iter().any(|d| d.id == definition.id)
                || written.definitions.iter().any(|d| {
                    d.id == definition.id
                        && d.name == definition.name
                        && d.value_type == definition.value_type
                        && d.unit == definition.unit
                        && d.dictionary_kind == definition.dictionary_kind
                })
        })
}

/// Mutations that add imported attributes to a project, and what was reported.
#[derive(Debug, Default)]
pub struct AttributeImportPlan {
    /// Append after the `CreateDocument` of `document`: definitions first, then values.
    pub mutations: Vec<Mutation>,
    pub notes: Vec<String>,
}

fn compatible(existing: &AttributeDefinition, imported: &AttributeDefinition) -> bool {
    existing.value_type == imported.value_type
        && existing.unit == imported.unit
        && existing.dictionary_kind == imported.dictionary_kind
}

/// Plans the mutations for the imported attributes against the project the
/// document is imported into. Nothing in it can make the batch fail: a value
/// the project cannot take is skipped and counted in `notes`.
///
/// * A definition ID the project does not have is created as imported
///   (`applies_to` = the targets that carry values).
/// * A definition ID the project has with another type, unit or dictionary
///   kind is created again under a fresh ID and a " (imported)" name suffix.
/// * A new name that collides with an existing one gets the same suffix.
/// * Values for an existing definition that does not apply to the element's
///   target are skipped; existing definitions are never widened.
/// * A `pc:ref` whose entry is not in the project (or has another kind)
///   becomes a `Literal`; entries are never matched by spelling.
pub fn plan_attribute_import(
    project: &ProjectData,
    document: &mut BpmnDocument,
    imported: &ImportedAttributes,
) -> AttributeImportPlan {
    let mut plan = AttributeImportPlan::default();
    let mut names: HashSet<String> = project
        .attribute_definitions
        .iter()
        .map(|d| d.name.clone())
        .collect();
    let mut count = project.attribute_definitions.len();
    let mut resolved: HashMap<AttributeDefinitionId, Option<AttributeDefinition>> = HashMap::new();
    let mut creates = Vec::new();
    let mut skipped_definitions = 0usize;
    for definition in &imported.definitions {
        let existing = project
            .attribute_definitions
            .iter()
            .find(|d| d.id == definition.id);
        if let Some(existing) = existing
            && compatible(existing, definition)
        {
            resolved.insert(definition.id, Some(existing.clone()));
            continue;
        }
        let mut fresh = definition.clone();
        let mut note = None;
        if existing.is_some() {
            fresh.id = AttributeDefinitionId::new();
            note = Some("has the ID of an existing definition with another type");
        }
        if note.is_some() || names.contains(&fresh.name) {
            let base = fresh.name.clone();
            let mut candidate = format!("{base} (imported)");
            let mut n = 2;
            while names.contains(&candidate) {
                candidate = format!("{base} (imported {n})");
                n += 1;
            }
            if note.is_none() {
                note = Some("has the name of an existing definition");
            }
            fresh.name = candidate;
        }
        if count >= MAX_DEFINITIONS || fresh.validate_shape().is_err() {
            skipped_definitions += 1;
            resolved.insert(definition.id, None);
            continue;
        }
        if let Some(note) = note {
            plan.notes.push(format!(
                "Attribute \"{}\" {note}; imported as \"{}\"",
                definition.name, fresh.name
            ));
        }
        names.insert(fresh.name.clone());
        count += 1;
        resolved.insert(definition.id, Some(fresh.clone()));
        creates.push(fresh);
    }
    if skipped_definitions > 0 {
        plan.notes.push(format!(
            "{skipped_definitions} attribute definition(s) skipped (definition limit or invalid name)"
        ));
    }
    let mut groups: Vec<(ElementSelector, Vec<AttributeValueInput>)> = Vec::new();
    // Values the project cannot take; they go back into the document as extension content.
    let mut kept: Vec<(&ImportedAttributeValue, &str)> = Vec::new();
    for imported_value in &imported.values {
        let Some(Some(definition)) = resolved.get(&imported_value.definition) else {
            kept.push((imported_value, "its definition could not be created"));
            continue;
        };
        let applies = target_of(document, &imported_value.owner)
            .is_some_and(|target| definition.applies_to.contains(&target));
        if !applies {
            kept.push((
                imported_value,
                "its existing definition does not apply to the element",
            ));
            continue;
        }
        let mut value = imported_value.value.clone();
        if let AttributeValue::DictionaryRefs(refs) = &mut value {
            let mut mapped: Vec<DictionaryTarget> = Vec::new();
            for (index, reference) in refs.iter().enumerate() {
                let reference = match reference {
                    DictionaryTarget::Entry(id)
                        if !project.dictionary_entries.iter().any(|entry| {
                            entry.id == *id && Some(entry.kind) == definition.dictionary_kind
                        }) =>
                    {
                        DictionaryTarget::Literal(
                            imported_value
                                .ref_labels
                                .get(index)
                                .cloned()
                                .unwrap_or_default(),
                        )
                    }
                    other => other.clone(),
                };
                if !mapped.contains(&reference) {
                    mapped.push(reference);
                }
            }
            *refs = mapped;
        }
        let Ok(value) = check_value(definition, &value) else {
            kept.push((imported_value, "it does not fit the existing definition"));
            continue;
        };
        let input = AttributeValueInput {
            definition_id: definition.id,
            value: Some(value),
        };
        match groups
            .iter_mut()
            .find(|(owner, _)| *owner == imported_value.owner)
        {
            Some((_, inputs)) => inputs.push(input),
            None => groups.push((imported_value.owner.clone(), vec![input])),
        }
    }
    let mut restore: Vec<(ElementSelector, XmlElement)> = Vec::new();
    for (value, reason) in &kept {
        let name = imported
            .definitions
            .iter()
            .find(|d| d.id == value.definition)
            .map_or("?", |d| d.name.as_str());
        let owner = match &value.owner {
            ElementSelector::Xml(id) => id.to_string(),
            ElementSelector::Native { native_id } => native_id.to_string(),
        };
        plan.notes.push(format!(
            "Attribute \"{name}\" on {owner} not imported because {reason}; kept as extension content"
        ));
        match restore.iter_mut().find(|(o, _)| *o == value.owner) {
            Some((_, wrapper)) => wrapper.children.push(XmlNode::Element {
                element: value.source.clone(),
            }),
            None => restore.push((
                value.owner.clone(),
                pc_element(
                    "attributes",
                    Vec::new(),
                    vec![XmlNode::Element {
                        element: value.source.clone(),
                    }],
                ),
            )),
        }
    }
    if !restore.is_empty() {
        let _ = attach(document, restore);
    }
    let applied: usize = groups.iter().map(|(_, inputs)| inputs.len()).sum();
    if applied > 0 || !creates.is_empty() {
        plan.notes.insert(
            0,
            format!(
                "Imported {applied} attribute value(s) and {} new attribute definition(s)",
                creates.len()
            ),
        );
    }
    plan.mutations.extend(
        creates
            .into_iter()
            .map(|definition| Mutation::CreateAttributeDefinition { definition }),
    );
    plan.mutations
        .extend(
            groups
                .into_iter()
                .map(|(xml_id, values)| Mutation::SetAttributeValues {
                    source: ElementKey {
                        document_id: document.id,
                        xml_id,
                    },
                    values,
                }),
        );
    plan
}
