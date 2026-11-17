//! Qualification of a whole document against the supported profile (`parse_profile`).
use crate::interchange::export::owner_metadata::collect_owner_metadata;
use crate::interchange::import::catalog::{catalog_interface, catalog_item, catalog_resource};
use crate::interchange::import::choreography::parse_choreography;
use crate::interchange::import::collaboration::{parse_collaboration, parse_partner_refs};
use crate::interchange::import::elements::{
    parse_correlation_property, parse_event_definition, parse_global_task, parse_item_aware,
    supported_event_details,
};
use crate::interchange::import::metadata::{
    documentation_set, extract_flow_metadata, extract_owner_metadata,
    has_unconsumed_anonymous_base, has_unconsumed_flow_metadata, parse_di_metadata, parse_label,
    take_anonymous_base,
};
use crate::interchange::import::process::parse_container;
use crate::interchange::import::tree::structured_documentation;
use crate::interchange::import::tree::{
    Elem, attrs, bounds, di_attrs, element_identity, number, optional_bool, optional_id, selector,
    tree, xml_namespaces,
};
use crate::interchange::refusal::{at, leave, refused};
use crate::interchange::shared::{
    activity_ports, event_definition_identity, is_process_port, scopes,
};
use crate::ns::{BPMN, BPMNDI, DC, DI};
use processcraft_core::bpmn_model::{EventDefinition, EventReference, EventResource, NodeDetails};
use processcraft_core::ids::{ElementIdentity, ElementSelector};
use processcraft_core::model::*;
use std::collections::BTreeMap;
use std::sync::Arc;
/// Names each out-of-enum `loopType` on a choreography activity so a refusal
/// is attributable (Semantic.xsd `tChoreographyLoopType`). A textual scan is
/// enough here: it only explains a refusal that parsing already decided.
pub(in crate::interchange) fn invalid_choreography_loop_types(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut found = Vec::new();
    const NEEDLE: &str = "loopType=\"";
    for (at, _) in text.match_indices(NEEDLE) {
        let tag_start = text[..at].rfind('<').unwrap_or(0);
        if !text[tag_start..at].contains("horeography") {
            continue;
        }
        let rest = &text[at + NEEDLE.len()..];
        let value = &rest[..rest.find('"').unwrap_or(rest.len())];
        if !matches!(
            value,
            "None" | "Standard" | "MultiInstanceSequential" | "MultiInstanceParallel"
        ) {
            found.push(format!(
                "Choreography activity loopType \"{value}\" is not one of None, Standard, MultiInstanceSequential, MultiInstanceParallel (Semantic.xsd tChoreographyLoopType)"
            ));
        }
    }
    found
}
// Called only while importing a new BPMN source; native archives are never promoted on read.
fn imported_root_selector(document: &mut BpmnDocument, name: &str) -> Option<ElementSelector> {
    fn ensure(id: &mut Option<ElementIdentity>) -> ElementSelector {
        id.get_or_insert_with(ElementIdentity::anonymous).selector()
    }
    Some(match name {
        "rootElement"
        | "globalTask"
        | "globalManualTask"
        | "globalBusinessRuleTask"
        | "globalScriptTask"
        | "globalUserTask" => ensure(&mut document.global_tasks.last_mut()?.id),
        "correlationProperty" => ensure(&mut document.correlation_properties.last_mut()?.id),
        "itemDefinition" => ensure(&mut document.catalog.item_definitions.last_mut()?.id),
        "interface" => ensure(&mut document.catalog.interfaces.last_mut()?.id),
        "resource" => ensure(&mut document.catalog.resources.last_mut()?.id),
        "dataStore" => ensure(&mut document.data_stores.last_mut()?.id),
        "partnerEntity" => ensure(&mut document.partner_entities.last_mut()?.id),
        "partnerRole" => ensure(&mut document.partner_roles.last_mut()?.id),
        "endPoint" => ensure(&mut document.endpoints.last_mut()?.id),
        "category" => document.categories.last()?.id.selector(),
        "message" | "signal" | "error" | "escalation" => match document.event_resources.last()? {
            EventResource::Message { id, .. }
            | EventResource::Signal { id, .. }
            | EventResource::Error { id, .. }
            | EventResource::Escalation { id, .. } => id.selector(),
        },
        "choreography" | "globalChoreographyTask" => document.choreographies.last()?.id.selector(),
        "collaboration" | "globalConversation" => document
            .additional_collaborations
            .last()
            .or(document.collaboration.as_ref())?
            .id
            .selector(),
        "process" => document.processes.last()?.id.selector(),
        name if name.ends_with("EventDefinition") => {
            let id = match document.root_event_definitions.last_mut()? {
                EventDefinition::Message { id, .. }
                | EventDefinition::Signal { id, .. }
                | EventDefinition::Error { id, .. }
                | EventDefinition::Escalation { id, .. }
                | EventDefinition::Conditional { id, .. }
                | EventDefinition::Link { id, .. }
                | EventDefinition::Cancel { id, .. }
                | EventDefinition::Compensate { id, .. }
                | EventDefinition::Terminate { id, .. }
                | EventDefinition::Timer { id, .. } => id,
            };
            ensure(id)
        }
        _ => return None,
    })
}
fn event_definition_resources_valid(document: &BpmnDocument, definition: &EventDefinition) -> bool {
    fn local_or_external(
        reference: &EventReference,
        matches_local: impl FnOnce(&ElementSelector) -> bool,
    ) -> bool {
        match reference {
            EventReference::Local(selector) => matches_local(selector),
            EventReference::Qualified(_) => true,
        }
    }
    match definition {
        EventDefinition::Signal { signal_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Signal { id, .. } if id.selector() == *selector))),
        EventDefinition::Message { message_ref, operation_ref, .. } => message_ref.as_ref().is_none_or(|reference| local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Message { id, .. } if id.selector() == *selector)))) && operation_ref.as_ref().is_none_or(|reference| local_or_external(reference, |selector| document.catalog.interfaces.iter().flat_map(|i| &i.operations).any(|op| op.id.as_ref().is_some_and(|id| id.selector() == *selector)))),
        EventDefinition::Error { error_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Error { id, .. } if id.selector() == *selector))),
        EventDefinition::Escalation { escalation_ref: Some(reference), .. } => local_or_external(reference, |selector| document.event_resources.iter().any(|r| matches!(r, EventResource::Escalation { id, .. } if id.selector() == *selector))),
        _ => true,
    }
}
pub(in crate::interchange) fn parse_profile(bytes: &[u8]) -> Option<BpmnDocument> {
    let mut root = tree(bytes)?;
    leave();
    let mut pending_metadata = BTreeMap::new();
    extract_owner_metadata(&mut root, &mut pending_metadata)?;
    let target_namespace = root.attrs.get("targetNamespace")?.clone();
    extract_flow_metadata(&mut root, &target_namespace)?;
    // Until every owner has a metadata writer, foreign attributes must remain source-preserved.
    fn has_foreign_attributes(e: &Elem) -> bool {
        if e.opaque || (e.ns == DI && e.name == "extension") {
            return false;
        }
        let supported_di = e.ns == BPMNDI
            && matches!(
                e.name.as_str(),
                "BPMNPlane" | "BPMNShape" | "BPMNEdge" | "BPMNLabel"
            );
        (!supported_di && !e.foreign_attributes.is_empty())
            || e.children.iter().any(has_foreign_attributes)
    }
    if let Some(owner) = root.children.iter().find(|e| has_foreign_attributes(e)) {
        at(owner);
        return refused(|| "extension attributes inside this element".into());
    }
    if root.ns != BPMN
        || root.name != "definitions"
        || !root.attrs.keys().all(|key| [
                "id",
                "targetNamespace",
                "name",
                "expressionLanguage",
                "typeLanguage",
                "exporter",
                "exporterVersion",
            ].contains(&key.as_str()))
        || !root.attrs.contains_key("targetNamespace")
        // The editable profile must fit the native metadata limits.
        || ["targetNamespace", "name", "expressionLanguage", "typeLanguage", "exporter", "exporterVersion"]
            .iter()
            .any(|name| root.attrs.get(*name).is_some_and(|value| value.len() > 16_384))
    {
        return refused(|| "unsupported <definitions> attributes".into());
    }
    let mut d = BpmnDocument::new(String::new(), element_identity(&root)?);
    d.definitions = Some(DefinitionsMetadata {
        // Foreign attributes are always prefixed; the default namespace is not their context.
        namespaces: if root.foreign_attributes.is_empty() {
            Vec::new()
        } else {
            let mut bindings = xml_namespaces(&root);
            bindings.retain(|binding| !binding.prefix.is_empty());
            bindings
        },
        foreign_attributes: root.foreign_attributes.clone(),
        target_namespace: root.attrs.get("targetNamespace")?.clone(),
        name: root.attrs.get("name").cloned(),
        expression_language: root.attrs.get("expressionLanguage").cloned(),
        type_language: root.attrs.get("typeLanguage").cloned(),
        exporter: root.attrs.get("exporter").cloned(),
        exporter_version: root.attrs.get("exporterVersion").cloned(),
        imports: Vec::new(),
    });
    let mut seen_non_import = false;
    let mut root_phase = 0;
    let mut imported_order = Vec::new();
    for e in &root.children {
        at(e);
        let phase = match (e.ns.as_str(), e.name.as_str()) {
            (BPMN, "import") => 0,
            (BPMN, "extension") => 1,
            (BPMNDI, "BPMNDiagram") => 3,
            (BPMN, "relationship") => 4,
            _ => 2,
        };
        if phase < root_phase {
            return None;
        }
        root_phase = phase;
        if e.ns == BPMN && e.name == "import" {
            if seen_non_import
                || !attrs(e, &["namespace", "location", "importType"])
                || !e.children.is_empty()
                || !e.text.trim().is_empty()
            {
                return None;
            }
            let imports = &mut d.definitions.as_mut()?.imports;
            if imports.len() >= 256 {
                return None;
            }
            let namespace = e.attrs.get("namespace")?.clone();
            let location = e.attrs.get("location")?.clone();
            let import_type = e.attrs.get("importType")?.clone();
            if [&namespace, &location, &import_type]
                .iter()
                .any(|value| value.len() > 16_384)
            {
                return None;
            }
            imports.push(DefinitionsImport {
                namespace,
                location,
                import_type,
            });
            continue;
        }
        seen_non_import = true;
        match (e.ns.as_str(), e.name.as_str()) {
            (BPMN, "correlationProperty") => {
                d.correlation_properties
                    .push(parse_correlation_property(e, &target_namespace)?);
            }
            (
                BPMN,
                "rootElement"
                | "globalTask"
                | "globalManualTask"
                | "globalBusinessRuleTask"
                | "globalScriptTask"
                | "globalUserTask",
            ) => {
                d.global_tasks
                    .push(parse_global_task(e, &target_namespace)?);
            }
            (BPMN, "extension") => {
                if !attrs(e, &["definition", "mustUnderstand"]) || d.extensions.len() >= 256 {
                    return None;
                }
                let must_understand = optional_bool(e.attrs.get("mustUnderstand"))?;
                if must_understand == Some(true) {
                    return None;
                }
                let documentation = e
                    .children
                    .iter()
                    .map(structured_documentation)
                    .collect::<Option<Vec<_>>>()?;
                d.extensions.push(DefinitionsExtension {
                    documentation,
                    definition: e.catalog_qnames.get("definition").cloned(),
                    must_understand,
                });
            }
            (BPMN, "relationship") => {
                if !attrs(e, &["id", "type", "direction"]) || d.relationships.len() >= 256 {
                    return None;
                }
                let direction = match e.attrs.get("direction").map(String::as_str) {
                    None => None,
                    Some("None") => Some(RelationshipDirection::None),
                    Some("Forward") => Some(RelationshipDirection::Forward),
                    Some("Backward") => Some(RelationshipDirection::Backward),
                    Some("Both") => Some(RelationshipDirection::Both),
                    _ => return None,
                };
                let mut sources = Vec::new();
                let mut targets = Vec::new();
                for child in &e.children {
                    if child.ns != BPMN || !attrs(child, &[]) || !child.children.is_empty() {
                        return None;
                    }
                    match child.name.as_str() {
                        "source" if targets.is_empty() => {
                            sources.push(child.catalog_text_qname.clone()?)
                        }
                        "target" => targets.push(child.catalog_text_qname.clone()?),
                        _ => return None,
                    }
                }
                if sources.is_empty() || targets.is_empty() {
                    return None;
                }
                d.relationships.push(DefinitionsRelationship {
                    id: optional_id(e)?,
                    base: take_anonymous_base(e),
                    relationship_type: e.attrs.get("type")?.clone(),
                    direction,
                    sources,
                    targets,
                });
            }
            (
                BPMN,
                "messageEventDefinition"
                | "signalEventDefinition"
                | "errorEventDefinition"
                | "escalationEventDefinition"
                | "conditionalEventDefinition"
                | "timerEventDefinition"
                | "linkEventDefinition"
                | "cancelEventDefinition"
                | "compensateEventDefinition"
                | "terminateEventDefinition",
            ) => d.root_event_definitions.push(parse_event_definition(
                e,
                root.attrs.get("targetNamespace")?,
            )?),
            (BPMN, "itemDefinition") if d.catalog.item_definitions.len() < 256 => {
                d.catalog.item_definitions.push(catalog_item(e)?)
            }
            (BPMN, "interface") if d.catalog.interfaces.len() < 256 => {
                d.catalog.interfaces.push(catalog_interface(e)?)
            }
            (BPMN, "resource") if d.catalog.resources.len() < 256 => {
                d.catalog.resources.push(catalog_resource(e)?)
            }
            (BPMN, "category") => {
                if !attrs(e, &["id", "name"]) {
                    return None;
                }
                let mut values = Vec::new();
                for child in &e.children {
                    if child.ns != BPMN
                        || child.name != "categoryValue"
                        || !attrs(child, &["id", "value"])
                        || !child.children.is_empty()
                    {
                        return None;
                    }
                    values.push(processcraft_core::bpmn_model::CategoryValue {
                        base: take_anonymous_base(child),
                        id: element_identity(child)?,
                        value: child.attrs.get("value").cloned(),
                    });
                }
                d.categories.push(processcraft_core::bpmn_model::Category {
                    base: take_anonymous_base(e),
                    id: element_identity(e)?,
                    name: e.attrs.get("name").cloned(),
                    values,
                });
            }
            (BPMN, "dataStore") => {
                if !attrs(
                    e,
                    &["id", "name", "capacity", "isUnlimited", "itemSubjectRef"],
                ) || e.children.iter().any(|c| c.name != "dataState")
                {
                    return None;
                }
                d.data_stores.push(processcraft_core::bpmn_data::DataStore {
                    base: take_anonymous_base(e),
                    id: optional_id(e)?,
                    name: e.attrs.get("name").cloned(),
                    capacity: e.attrs.get("capacity").cloned(),
                    is_unlimited: optional_bool(e.attrs.get("isUnlimited"))?,
                    item_aware: parse_item_aware(e)?,
                });
            }
            (BPMN, "error" | "escalation") => {
                if !attrs(
                    e,
                    &["id", "name", "errorCode", "escalationCode", "structureRef"],
                ) || !e.children.is_empty()
                {
                    return None;
                }
                let root_id = element_identity(e)?;
                let name = e.attrs.get("name").cloned();
                let code = e
                    .attrs
                    .get(if e.name == "error" {
                        "errorCode"
                    } else {
                        "escalationCode"
                    })
                    .cloned();
                if e.attrs.contains_key(if e.name == "error" {
                    "escalationCode"
                } else {
                    "errorCode"
                }) {
                    return None;
                }
                d.event_resources.push(if e.name == "error" {
                    EventResource::Error {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        code,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                } else {
                    EventResource::Escalation {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        code,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                });
            }
            (BPMN, "signal" | "message") => {
                if !attrs(
                    e,
                    if e.name == "signal" {
                        &["id", "name", "structureRef"]
                    } else {
                        &["id", "name", "itemRef"]
                    },
                ) || !e.children.is_empty()
                {
                    return None;
                }
                let root_id = element_identity(e)?;
                let name = e.attrs.get("name").cloned();
                d.event_resources.push(if e.name == "signal" {
                    EventResource::Signal {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
                    }
                } else {
                    EventResource::Message {
                        base: take_anonymous_base(e),
                        id: root_id,
                        name,
                        item_ref: e.catalog_qnames.get("itemRef").cloned(),
                    }
                });
            }
            (BPMN, "partnerEntity" | "partnerRole") => {
                if !attrs(e, &["id", "name"]) {
                    return None;
                }
                let refs = parse_partner_refs(e, &target_namespace)?;
                if e.name == "partnerEntity" {
                    d.partner_entities
                        .push(processcraft_core::bpmn_collaboration::PartnerEntity {
                            id: optional_id(e)?,
                            base: take_anonymous_base(e),
                            name: e.attrs.get("name").cloned(),
                            participant_refs: refs,
                        });
                } else {
                    d.partner_roles
                        .push(processcraft_core::bpmn_collaboration::PartnerRole {
                            id: optional_id(e)?,
                            base: take_anonymous_base(e),
                            name: e.attrs.get("name").cloned(),
                            participant_refs: refs,
                        });
                }
            }
            (BPMN, "endPoint") => {
                if !attrs(e, &["id"]) || !e.text.trim().is_empty() || !e.children.is_empty() {
                    return None;
                }
                d.endpoints
                    .push(processcraft_core::bpmn_collaboration::EndPoint {
                        id: optional_id(e)?,
                        base: take_anonymous_base(e),
                    });
            }
            (BPMN, "choreography" | "globalChoreographyTask") => {
                d.choreographies
                    .push(parse_choreography(e, &target_namespace)?);
            }
            (BPMN, "collaboration" | "globalConversation") => {
                let collaboration = parse_collaboration(e, &target_namespace)?;
                if d.collaboration.is_none() {
                    d.collaboration = Some(collaboration);
                } else {
                    d.additional_collaborations.push(collaboration);
                }
            }
            (BPMN, "process") => {
                if !attrs(
                    e,
                    &[
                        "id",
                        "name",
                        "isExecutable",
                        "processType",
                        "isClosed",
                        "definitionalCollaborationRef",
                    ],
                ) || optional_bool(e.attrs.get("isExecutable")).is_none()
                {
                    return None;
                }
                let p = parse_container(e, 0, root.attrs.get("targetNamespace")?)?;
                d.processes.push(p);
            }
            (BPMNDI, "BPMNDiagram") => {
                if !attrs(e, &["id", "name", "documentation", "resolution"])
                    || e.children.is_empty()
                {
                    return None;
                }
                let plane = &e.children[0];
                if plane.ns != BPMNDI
                    || plane.name != "BPMNPlane"
                    || !di_attrs(plane, &["id", "bpmnElement"])
                {
                    return None;
                }
                let (plane_metadata, plane_offset) = parse_di_metadata(plane)?;
                let mut diagram = Diagram {
                    plane_metadata,
                    plane_order: Some(Vec::new()),
                    id: element_identity(e)?,
                    name: e.attrs.get("name").cloned(),
                    documentation: e.attrs.get("documentation").cloned(),
                    resolution: e
                        .attrs
                        .get("resolution")
                        .map(|v| v.parse())
                        .transpose()
                        .ok()?,
                    label_styles: Vec::new(),
                    plane_id: element_identity(plane)?,
                    plane: selector(plane, "bpmnElement")?,
                    shapes: vec![],
                    edges: vec![],
                };
                for style in &e.children[1..] {
                    if style.ns != BPMNDI
                        || style.name != "BPMNLabelStyle"
                        || !attrs(style, &["id"])
                        || style.children.len() != 1
                    {
                        return None;
                    }
                    let font = &style.children[0];
                    if font.ns != DC
                        || font.name != "Font"
                        || !attrs(
                            font,
                            &[
                                "name",
                                "size",
                                "isBold",
                                "isItalic",
                                "isUnderline",
                                "isStrikeThrough",
                            ],
                        )
                        || !font.children.is_empty()
                    {
                        return None;
                    }
                    diagram.label_styles.push(LabelStyle {
                        id: optional_id(style)?,
                        font: Some(DiFont {
                            name: font.attrs.get("name").cloned(),
                            size: font.attrs.get("size").map(|v| v.parse()).transpose().ok()?,
                            is_bold: optional_bool(font.attrs.get("isBold"))?,
                            is_italic: optional_bool(font.attrs.get("isItalic"))?,
                            is_underline: optional_bool(font.attrs.get("isUnderline"))?,
                            is_strike_through: optional_bool(font.attrs.get("isStrikeThrough"))?,
                        }),
                    });
                }
                for item in &plane.children[plane_offset..] {
                    at(item);
                    if item.ns != BPMNDI
                        || !di_attrs(
                            item,
                            if item.name == "BPMNShape" {
                                &[
                                    "id",
                                    "bpmnElement",
                                    "isExpanded",
                                    "isHorizontal",
                                    "isMarkerVisible",
                                    "isMessageVisible",
                                    "participantBandKind",
                                    "choreographyActivityShape",
                                ]
                            } else {
                                &[
                                    "id",
                                    "bpmnElement",
                                    "sourceElement",
                                    "targetElement",
                                    "messageVisibleKind",
                                ]
                            },
                        )
                    {
                        return None;
                    }
                    let (di_metadata, offset) = parse_di_metadata(item)?;
                    let identity = element_identity(item)?;
                    diagram.plane_order.as_mut()?.push(identity.selector());
                    match item.name.as_str() {
                        "BPMNShape" => {
                            if !(offset + 1..=offset + 2).contains(&item.children.len()) {
                                return None;
                            }
                            let label = if item.children.len() == offset + 2 {
                                Some(parse_label(
                                    &item.children[offset + 1],
                                    root.attrs.get("targetNamespace")?,
                                )?)
                            } else {
                                None
                            };
                            let is_expanded = match item.attrs.get("isExpanded").map(String::as_str)
                            {
                                None => None,
                                Some("true" | "1") => Some(true),
                                Some("false" | "0") => Some(false),
                                _ => return None,
                            };
                            let participant_band_kind =
                                match item.attrs.get("participantBandKind").map(String::as_str) {
                                    None => None,
                                    Some("top_initiating") => {
                                        Some(ParticipantBandKind::TopInitiating)
                                    }
                                    Some("middle_initiating") => {
                                        Some(ParticipantBandKind::MiddleInitiating)
                                    }
                                    Some("bottom_initiating") => {
                                        Some(ParticipantBandKind::BottomInitiating)
                                    }
                                    Some("top_non_initiating") => {
                                        Some(ParticipantBandKind::TopNonInitiating)
                                    }
                                    Some("middle_non_initiating") => {
                                        Some(ParticipantBandKind::MiddleNonInitiating)
                                    }
                                    Some("bottom_non_initiating") => {
                                        Some(ParticipantBandKind::BottomNonInitiating)
                                    }
                                    _ => return None,
                                };
                            diagram.shapes.push(Shape {
                                di_metadata,
                                is_message_visible: optional_bool(
                                    item.attrs.get("isMessageVisible"),
                                )?,
                                participant_band_kind,
                                choreography_activity_shape: item
                                    .catalog_qnames
                                    .get("choreographyActivityShape")
                                    .map(|q| {
                                        if q.namespace == target_namespace {
                                            DiStyleRef::Local(q.local.clone().into())
                                        } else {
                                            DiStyleRef::Qualified(q.clone())
                                        }
                                    }),
                                id: identity,
                                element: selector(item, "bpmnElement")?,
                                is_expanded,
                                is_horizontal: optional_bool(item.attrs.get("isHorizontal"))?,
                                is_marker_visible: optional_bool(
                                    item.attrs.get("isMarkerVisible"),
                                )?,
                                bounds: bounds(&item.children[offset])?,
                                label,
                                label_visibility: Some(DiLabelVisibility::Explicit),
                                style: None,
                            });
                        }
                        "BPMNEdge" => {
                            let mut points = vec![];
                            let mut label = None;
                            for pt in &item.children[offset..] {
                                if pt.ns == BPMNDI && pt.name == "BPMNLabel" && label.is_none() {
                                    label =
                                        Some(parse_label(pt, root.attrs.get("targetNamespace")?)?);
                                    continue;
                                }
                                if label.is_some() {
                                    return None;
                                }
                                if pt.ns != DI
                                    || pt.name != "waypoint"
                                    || !attrs(pt, &["x", "y"])
                                    || !pt.children.is_empty()
                                {
                                    return None;
                                }
                                points.push(Point {
                                    x: number(pt, "x")?,
                                    y: number(pt, "y")?,
                                });
                            }
                            if points.len() < 2 {
                                return None;
                            }
                            let message_visible_kind =
                                match item.attrs.get("messageVisibleKind").map(String::as_str) {
                                    None => None,
                                    Some("initiating") => Some(MessageVisibleKind::Initiating),
                                    Some("non_initiating") => {
                                        Some(MessageVisibleKind::NonInitiating)
                                    }
                                    _ => return None,
                                };
                            diagram.edges.push(Edge {
                                di_metadata,
                                message_visible_kind,
                                id: identity,
                                element: selector(item, "bpmnElement")?,
                                source_element: item
                                    .attrs
                                    .get("sourceElement")
                                    .map(|v| v.parse())
                                    .transpose()
                                    .ok()?,
                                target_element: item
                                    .attrs
                                    .get("targetElement")
                                    .map(|v| v.parse())
                                    .transpose()
                                    .ok()?,
                                waypoints: points,
                                label,
                                label_visibility: Some(DiLabelVisibility::Explicit),
                                style: None,
                            });
                        }
                        _ => return None,
                    }
                }
                d.diagrams.push(diagram);
            }
            _ => return None,
        }
        if phase == 2 {
            imported_order.push(imported_root_selector(&mut d, &e.name)?);
        }
    }
    d.root_order = Some(imported_order);
    leave();
    if d.root_event_definitions
        .iter()
        .any(|definition| !event_definition_resources_valid(&d, definition))
    {
        return refused(|| "unresolved root event definition resource".into());
    }
    // Every flow and lane reference is validated in its owning scope.
    let all_scopes = scopes(&d)?;
    for p in &all_scopes {
        for node in p.nodes {
            let node_label = || format!("<{:?} id=\"{}\">", node.kind, node.id);
            if node.activity.as_ref().is_some_and(|activity| {
                !processcraft_core::validation::activity_reference_issues(&d, activity).is_empty()
            }) {
                return refused(|| format!("unresolved activity reference at {}", node_label()));
            }
            if node.details.is_some()
                && !matches!(
                    node.details,
                    Some(
                        NodeDetails::SubProcess { .. }
                            | NodeDetails::CallActivity { .. }
                            | NodeDetails::DataObjectReference { .. }
                            | NodeDetails::DataStoreReference { .. }
                            | NodeDetails::Gateway { .. }
                            | NodeDetails::TextAnnotation { .. }
                    )
                )
                && !supported_event_details(node)
            {
                return refused(|| format!("unsupported details at {}", node_label()));
            }
            if let Some(NodeDetails::Event {
                definitions,
                definition_refs,
                ..
            }) = &node.details
                && (definitions
                    .iter()
                    .any(|definition| !event_definition_resources_valid(&d, definition))
                    || definition_refs.iter().any(|reference| {
                        matches!(reference, EventReference::Local(selector)
                        if !d.root_event_definitions.iter().any(|definition| {
                            event_definition_identity(definition)
                                .is_some_and(|id| id.selector() == *selector)
                        }))
                    }))
            {
                return refused(|| format!("unresolved event definition at {}", node_label()));
            }
            if let Some(NodeDetails::Event {
                attached_to: Some(target),
                ..
            }) = &node.details
                && matches!(target, EventReference::Local(selector) if !p.nodes.iter().any(|n| n.id == *selector && n.kind.is_activity()))
            {
                return refused(|| format!("attachedToRef outside its scope at {}", node_label()));
            }
        }
        for lane in p.lanes {
            if lane
                .members
                .iter()
                .any(|id| !p.nodes.iter().any(|n| &n.id == id))
            {
                return refused(|| {
                    format!("lane {} references a node outside its process", lane.id)
                });
            }
        }
        for f in p.flows {
            for reference in [&f.source, &f.target] {
                match reference {
                    Some(EventReference::Local(selector)) => {
                        if !p.nodes.iter().any(|n| n.id == *selector)
                            && !(f.kind == FlowKind::Association
                                // Association refs are any BaseElement, so any scope's node.
                                && (all_scopes
                                    .iter()
                                    .any(|s| s.nodes.iter().any(|n| n.id == *selector))
                                    || p.groups.iter().any(|g| g.id == *selector)
                                    || p.flows.iter().any(|flow| {
                                        flow.kind == FlowKind::Sequence && flow.id == *selector
                                    })))
                        {
                            return refused(|| {
                                format!("flow {} has an endpoint outside its scope", f.id)
                            });
                        }
                    }
                    Some(EventReference::Qualified(_)) if f.kind == FlowKind::Association => {}
                    _ => return refused(|| format!("flow {} has an unsupported endpoint", f.id)),
                }
            }
        }
    }
    let activity_ports = activity_ports(&d)?;
    for dia in &d.diagrams {
        if dia
            .shapes
            .iter()
            .any(|shape| activity_ports.contains(&shape.element))
        {
            return refused(|| "a diagram shape targets an activity data port".into());
        }
        let choreography_scopes = processcraft_core::queries::choreography_scopes(&d);
        if let Some(first) = choreography_scopes
            .iter()
            .find(|scope| scope.owner == dia.plane)
        {
            let mut contained = std::collections::HashSet::new();
            contained.insert(first.owner.clone());
            for scope in &choreography_scopes {
                if scope
                    .parent
                    .as_ref()
                    .is_some_and(|parent| contained.contains(parent))
                {
                    contained.insert(scope.owner.clone());
                }
            }
            let scopes: Vec<_> = choreography_scopes
                .iter()
                .filter(|scope| contained.contains(&scope.owner))
                .collect();
            let c = first.root;
            use processcraft_core::bpmn_collaboration::CollaborationArtifact;
            if dia.shapes.iter().any(|s| {
                !c.participants.iter().any(|p| p.id == s.element)
                    && !scopes.iter().any(|scope| scope.activities.iter().any(|a| a.id == s.element)
                        || scope.events.iter().chain(&scope.gateways).any(|n| n.id == s.element)
                        || scope.artifacts.iter().any(|a| match a {
                            CollaborationArtifact::Group(g) => g.id == s.element,
                            CollaborationArtifact::TextAnnotation(n) => n.id == s.element,
                            CollaborationArtifact::Association(_) => false,
                        }))
            }) || dia.edges.iter().any(|e| {
                !c.message_flows.iter().any(|flow| flow.id == e.element && scopes.iter().any(|scope| scope.activities.iter().any(|activity| activity.message_flow_refs.iter().any(|reference| reference.matches_identity(&flow.id, Some(&target_namespace))))))
                    && !scopes.iter().any(|scope| scope.flows.iter().any(|f| f.id == e.element)
                        || scope.data_associations.iter().any(|a| a.id.as_ref().is_some_and(|id| id == &e.element))
                        || scope.artifacts.iter().any(|a| matches!(a, CollaborationArtifact::Association(f) if f.id == e.element)))
            }) {
                return refused(|| "a choreography diagram element has no qualified model element".into());
            }
            continue;
        }
        if let Some(collaboration) = &d.collaboration
            && collaboration.id == dia.plane
        {
            use processcraft_core::bpmn_collaboration::CollaborationArtifact;
            let artifacts = collaboration
                .extras
                .as_ref()
                .map_or(&[][..], |extras| &extras.artifacts[..]);
            if dia.shapes.iter().any(|s| {
                !collaboration.participants.iter().any(|p| p.id == s.element)
                    && !collaboration
                        .conversation_nodes
                        .iter()
                        .any(|n| n.id == s.element)
                    && !artifacts.iter().any(|a| match a {
                        CollaborationArtifact::Group(g) => g.id == s.element,
                        CollaborationArtifact::TextAnnotation(n) => n.id == s.element,
                        CollaborationArtifact::Association(_) => false,
                    })
                    && !all_scopes.iter().any(|p| {
                        p.nodes.iter().any(|n| n.id == s.element)
                            || p.lanes.iter().any(|l| l.id == s.element)
                            || p.groups.iter().any(|g| g.id == s.element)
                    })
            }) || dia.edges.iter().any(|e| {
                !collaboration.flows.iter().any(|f| f.id == e.element)
                    && !collaboration
                        .conversation_links
                        .iter()
                        .any(|f| f.id == e.element)
                    && !artifacts.iter().any(
                        |a| matches!(a, CollaborationArtifact::Association(f) if f.id == e.element),
                    )
                    && !all_scopes.iter().any(|p| {
                        p.flows.iter().any(|f| f.id == e.element)
                            || p.data_associations
                                .iter()
                                .any(|a| a.id.as_ref().is_some_and(|id| id.selector() == e.element))
                    })
            }) {
                return refused(|| {
                    "a collaboration diagram element has no qualified model element".into()
                });
            }
            continue;
        }
        let first = all_scopes.iter().find(|p| *p.id == dia.plane)?;
        let mut contained = Vec::new();
        let mut pending = vec![first];
        while let Some(scope) = pending.pop() {
            contained.push(scope);
            for node in scope.nodes {
                if matches!(node.details, Some(NodeDetails::SubProcess { .. })) {
                    pending.push(all_scopes.iter().find(|p| *p.id == node.id)?);
                }
            }
        }
        if dia.shapes.iter().any(|s| {
            !is_process_port(&d, &dia.plane, &s.element)
                && !contained.iter().any(|p| {
                    p.nodes.iter().any(|n| n.id == s.element)
                        || p.lanes.iter().any(|l| l.id == s.element)
                        || p.groups.iter().any(|g| g.id == s.element)
                })
        }) || dia.edges.iter().any(|e| {
            !contained.iter().any(|p| {
                p.flows.iter().any(|f| f.id == e.element)
                    || p.data_associations
                        .iter()
                        .any(|a| a.id.as_ref().is_some_and(|id| id.selector() == e.element))
            })
        }) {
            return refused(|| "a process diagram element has no qualified model element".into());
        }
    }
    let mut validation = ProjectData::new("validation".into());
    validation.documents.push(Arc::new(d.clone()));
    if let Err(error) = validation.validate() {
        return refused(|| format!("model validation failed: {error}"));
    }
    processcraft_core::metadata_visit::visit_base_metadata_mut(&mut d, |owner, base| {
        if let Some(id) = owner.and_then(ElementIdentity::xml_id)
            && let Some(pending) = pending_metadata.remove(&id.to_string())
        {
            *base = Some(processcraft_core::bpmn_metadata::BaseMetadata {
                namespaces: pending.extras.namespaces,
                documentation: pending.docs,
                foreign_attributes: pending.extras.foreign_attributes,
                extension_elements: pending.extras.extension_elements,
            });
        }
    });
    processcraft_core::metadata_visit::visit_legacy_metadata_mut(&mut d, |owner, docs, extras| {
        if let Some(id) = owner.xml_id()
            && let Some(pending) = pending_metadata.remove(&id.to_string())
        {
            *docs = documentation_set(pending.docs);
            if !pending.extras.namespaces.is_empty()
                || !pending.extras.foreign_attributes.is_empty()
                || pending.extras.extension_elements.is_some()
            {
                *extras = Some(pending.extras);
            }
        }
    });
    if !pending_metadata.is_empty()
        || has_unconsumed_anonymous_base(&root)
        || has_unconsumed_flow_metadata(&root)
        || collect_owner_metadata(&d).is_err()
    {
        return refused(|| {
            "documentation or extension metadata on an owner without a qualified writer".into()
        });
    }
    Some(d)
}
