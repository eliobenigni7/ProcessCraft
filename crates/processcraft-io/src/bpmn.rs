//! Bounded, non-executing BPMN source preservation. This is not an XSD validator.
//!
//! Every accepted input remains read-only; the original bytes are the authority.
//! No URL, schema, external entity or document-stored path is ever opened here.
use std::collections::{BTreeSet, HashSet};
use std::fmt;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::name::{PrefixDeclaration, QName, ResolveResult};
use quick_xml::reader::NsReader;

const BPMN: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";
const BPMNDI: &str = "http://www.omg.org/spec/BPMN/20100524/DI";
const DC: &str = "http://www.omg.org/spec/DD/20100524/DC";
const DI: &str = "http://www.omg.org/spec/DD/20100524/DI";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub const MAX_RAW_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DEPTH: usize = 128;
pub const MAX_ELEMENTS: usize = 10_000;
/// Namespace declarations per document and attributes per element.
pub const MAX_NAMESPACE_DECLARATIONS: usize = 1024;
pub const MAX_ATTRIBUTES_PER_ELEMENT: usize = 256;
pub const MAX_LABEL_BYTES: usize = 16 * 1024;
pub const MAX_DOCUMENTATION_BYTES: usize = 1024 * 1024;
/// Conservative M0 import bound in diagram units; sizes are also non-negative.
pub const MAX_GEOMETRY: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    PreservedSource,
}

/// Expanded identity; prefixes are deliberately not used as identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpandedName {
    pub namespace_uri: Option<String>,
    pub local_name: String,
}

#[derive(Debug)]
pub struct CapabilityReport {
    pub mode: ImportMode,
    pub semantic_edits_allowed: bool,
    pub limitations: Vec<&'static str>,
    pub element_count: usize,
    pub diagram_count: usize,
    pub standard_id_count: usize,
    /// Resolved QName-valued standard reference attributes and xsi:type values.
    pub qname_values: BTreeSet<ExpandedName>,
    pub foreign_element_names: BTreeSet<ExpandedName>,
}

#[derive(Debug)]
pub struct PreservedBpmn {
    source: Vec<u8>,
    report: CapabilityReport,
}

impl PreservedBpmn {
    pub fn report(&self) -> &CapabilityReport {
        &self.report
    }

    /// Complete unchanged XML, including BOM, whitespace and prefix spelling.
    /// This function performs no filesystem writes. Use an explicit output grant
    /// and the storage boundary when committing these bytes to a destination.
    pub fn export_source(&self) -> &[u8] {
        &self.source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorKind {
    RawBytes,
    Depth,
    Elements,
    Label,
    Documentation,
    Metadata,
    DtdForbidden,
    Xml,
    Id,
    Reference,
    Geometry,
}

#[derive(Debug)]
pub struct ImportError {
    pub kind: ImportErrorKind,
    pub detail: String,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: {}. Keep the existing document and inspect a smaller or corrected source file",
            self.kind, self.detail
        )
    }
}
impl std::error::Error for ImportError {}

fn error(kind: ImportErrorKind, detail: impl Into<String>) -> ImportError {
    ImportError {
        kind,
        detail: detail.into(),
    }
}
fn xml(e: impl fmt::Display) -> ImportError {
    error(ImportErrorKind::Xml, e.to_string())
}

#[derive(Default)]
struct Frame {
    semantic_name: Option<String>,
    /// Descendants are XML payload, not declarations in the BPMN model.
    opaque_children: bool,
    documentation: bool,
    label: bool,
    text_bytes: usize,
    reference_text: Option<String>,
    idref_text: bool,
    external_reference_allowed: bool,
    link_definition: bool,
    operation: bool,
    message_definition: bool,
}

/// Validate into temporary bounded state and publish a source document only after
/// the entire input and its known local references pass. No session is mutated.
pub fn import_bpmn(bytes: &[u8]) -> Result<PreservedBpmn, ImportError> {
    if bytes.len() > MAX_RAW_BYTES {
        return Err(error(ImportErrorKind::RawBytes, "raw XML exceeds 32 MiB"));
    }
    let source = std::str::from_utf8(bytes).map_err(xml)?;
    if !source.chars().all(xml_char) {
        return Err(xml("invalid XML 1.0 character"));
    }
    let mut reader = NsReader::from_str(source);
    reader.config_mut().check_comments = true;
    reader.config_mut().expand_empty_elements = true;
    let mut stack: Vec<Frame> = Vec::new();
    let mut seen_root = false;
    let mut namespace_declarations = 0usize;
    let mut seen_declaration = false;
    let mut preceding_content = false;
    let mut ids = HashSet::new();
    let mut target_namespace = None;
    let mut references = Vec::new();
    let mut metadata_bytes = 0usize;
    let mut report = CapabilityReport {
        mode: ImportMode::PreservedSource,
        semantic_edits_allowed: false,
        limitations: vec![
            "M0 preserves all accepted imports as read-only original XML bytes; semantic edits are unavailable",
            "Structural checks cover known IDs, references and DI numbers; this is not complete BPMN XSD or semantic validation",
            "No rendering or visual compatibility is qualified by this importer",
            "M0 refuses whitespace-containing namespace names rather than normalizing namespace identity",
        ],
        element_count: 0,
        diagram_count: 0,
        standard_id_count: 0,
        qname_values: BTreeSet::new(),
        foreign_element_names: BTreeSet::new(),
    };
    loop {
        let event = reader.read_event().map_err(xml)?;
        match event {
            Event::DocType(_) => {
                return Err(error(
                    ImportErrorKind::DtdForbidden,
                    "DTDs and external entities are forbidden",
                ));
            }
            Event::Decl(decl) => {
                if seen_declaration
                    || preceding_content
                    || seen_root
                    || decl.version().map_err(xml)?.as_ref() != "1.0"
                {
                    return Err(xml("only an initial XML 1.0 declaration is accepted"));
                }
                if let Some(encoding) = decl.encoding()
                    && !encoding.map_err(xml)?.eq_ignore_ascii_case("UTF-8")
                {
                    return Err(xml("M0 accepts UTF-8 XML only"));
                }
                seen_declaration = true;
            }
            Event::Start(start) => {
                preceding_content = true;
                if stack.len() >= MAX_DEPTH {
                    return Err(error(
                        ImportErrorKind::Depth,
                        "XML nesting exceeds 128 levels",
                    ));
                }
                report.element_count += 1;
                if report.element_count > MAX_ELEMENTS {
                    return Err(error(
                        ImportErrorKind::Elements,
                        "XML elements exceed 10,000 per document",
                    ));
                }
                validate_qname(start.name().0)?;
                let name = expanded(reader.resolver().resolve_element(start.name()))?;
                account(&mut metadata_bytes, &name)?;
                let ns = name.namespace_uri.as_deref();
                let local = name.local_name.as_str();
                if stack.is_empty() {
                    if seen_root || ns != Some(BPMN) || local != "definitions" {
                        return Err(xml("expected exactly one BPMN definitions root"));
                    }
                    seen_root = true;
                }
                let standard = matches!(ns, Some(BPMN | BPMNDI | DC | DI));
                let opaque = stack.last().is_some_and(|parent| parent.opaque_children);
                let modeled = !opaque;
                let activity_qname_text = modeled
                    && ns == Some(BPMN)
                    && activity_qname_text_reference(
                        stack
                            .last()
                            .and_then(|parent| parent.semantic_name.as_deref()),
                        local,
                    );

                if !standard && report.foreign_element_names.insert(name.clone()) {
                    account(&mut metadata_bytes, &name)?;
                }
                if modeled && ns == Some(BPMNDI) && local == "BPMNDiagram" {
                    report.diagram_count += 1;
                }
                let mut frame = Frame {
                    semantic_name: (modeled && ns == Some(BPMN)).then(|| local.to_owned()),
                    opaque_children: opaque
                        || !standard
                        || (ns == Some(DI) && local == "extension")
                        || (ns == Some(BPMN)
                            && matches!(
                                local,
                                "documentation" | "script" | "text" | "extensionElements"
                            )),
                    idref_text: modeled && ns == Some(BPMN) && idref_text_reference(local),
                    external_reference_allowed: activity_qname_text
                        || matches!(
                            local,
                            "incoming"
                                | "outgoing"
                                | "source"
                                | "target"
                                | "inMessageRef"
                                | "outMessageRef"
                                | "errorRef"
                                | "eventDefinitionRef"
                                | "operationRef"
                        ),
                    link_definition: modeled && ns == Some(BPMN) && local == "linkEventDefinition",
                    operation: modeled && ns == Some(BPMN) && local == "operation",
                    message_definition: modeled
                        && ns == Some(BPMN)
                        && local == "messageEventDefinition",
                    documentation: modeled && ns == Some(BPMN) && local == "documentation",
                    label: modeled && ns == Some(BPMN) && local == "text",
                    reference_text: (modeled
                        && ns == Some(BPMN)
                        && (matches!(local, "incoming" | "outgoing" | "eventDefinitionRef")
                            || idref_text_reference(local)
                            || (matches!(local, "source" | "target")
                                && stack.last().is_some_and(|parent| parent.link_definition))
                            || (matches!(local, "inMessageRef" | "outMessageRef" | "errorRef")
                                && stack.last().is_some_and(|parent| parent.operation))
                            || activity_qname_text
                            || (local == "operationRef"
                                && stack.last().is_some_and(|parent| parent.message_definition))))
                    .then(String::new),
                    ..Frame::default()
                };
                let mut attrs = std::collections::HashMap::new();
                let mut expanded_attrs = HashSet::new();
                let mut attribute_count = 0usize;
                for attr in start.attributes() {
                    let attr = attr.map_err(xml)?;
                    attribute_count += 1;
                    if attribute_count > MAX_ATTRIBUTES_PER_ELEMENT {
                        return Err(error(
                            ImportErrorKind::Metadata,
                            format!("XML element exceeds {MAX_ATTRIBUTES_PER_ELEMENT} attributes"),
                        ));
                    }
                    validate_qname(attr.key.0)?;
                    let value = attr
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(xml)?;
                    if !value.chars().all(xml_char) || attr.value.contains('<') {
                        return Err(xml("invalid XML attribute value"));
                    }
                    if let Some(binding) = attr.key.as_namespace_binding() {
                        namespace_declarations += 1;
                        if namespace_declarations > MAX_NAMESPACE_DECLARATIONS {
                            return Err(error(
                                ImportErrorKind::Metadata,
                                format!(
                                    "XML namespace declarations exceed {MAX_NAMESPACE_DECLARATIONS} per document"
                                ),
                            ));
                        }
                        if value.len() > MAX_LABEL_BYTES {
                            return Err(error(
                                ImportErrorKind::Metadata,
                                "namespace URI exceeds 16 KiB",
                            ));
                        }
                        validate_namespace_name(&value)?;
                        // NsReader checks raw declarations. Check decoded values
                        // too so character/entity escapes cannot hide reserved URIs.
                        let xml_uri = "http://www.w3.org/XML/1998/namespace";
                        let xmlns_uri = "http://www.w3.org/2000/xmlns/";
                        let xml_prefix = binding == PrefixDeclaration::Named("xml");
                        if binding == PrefixDeclaration::Named("xmlns")
                            || value == xmlns_uri
                            || (xml_prefix && value != xml_uri)
                            || (!xml_prefix && value == xml_uri)
                        {
                            return Err(xml("invalid normalized reserved namespace binding"));
                        }
                        continue;
                    }
                    let attr_name = expanded(reader.resolver().resolve_attribute(attr.key))?;
                    account(&mut metadata_bytes, &attr_name)?;
                    if !expanded_attrs.insert(attr_name.clone()) {
                        return Err(xml("duplicate expanded attribute name"));
                    }
                    if attr_name.namespace_uri.is_none() {
                        let key = attr_name.local_name.as_str();
                        if stack.is_empty() && key == "targetNamespace" {
                            if value.len() > MAX_LABEL_BYTES {
                                return Err(error(
                                    ImportErrorKind::Metadata,
                                    "target namespace exceeds 16 KiB",
                                ));
                            }
                            target_namespace = Some(value.to_string());
                        }
                        if standard
                            && key == "id"
                            && (!ncname(&value)
                                || value.len() > MAX_LABEL_BYTES
                                || !ids.insert(value.to_string()))
                        {
                            return Err(error(
                                ImportErrorKind::Id,
                                "standard ID is invalid or duplicated",
                            ));
                        }
                        if ns == Some(BPMN) && key == "name" && value.len() > MAX_LABEL_BYTES {
                            return Err(error(
                                ImportErrorKind::Label,
                                "individual label exceeds 16 KiB",
                            ));
                        }
                        if modeled && known_reference(ns, local, key) {
                            let idref = idref_reference(ns, local, key);
                            let resolved = resolve_reference(&reader, &value, idref)?;
                            push_reference(
                                &mut references,
                                &resolved,
                                !idref,
                                &mut metadata_bytes,
                            )?;
                            report.qname_values.insert(resolved);
                        }
                        if modeled
                            && matches!(ns, Some(DC | DI))
                            && matches!(key, "x" | "y" | "width" | "height")
                        {
                            attrs.insert(key.to_owned(), value.to_string());
                        }
                    }
                    if (attr_name.namespace_uri.as_deref() == Some(XSI)
                        && attr_name.local_name == "type")
                        || (attr_name.namespace_uri.is_none()
                            && modeled
                            && metadata_qname(ns, local, &attr_name.local_name))
                    {
                        let resolved = resolve_value(&reader, &value)?;
                        // callableElement is not a rootElement substitution in
                        // the pinned XSD. Its concrete root form uses xsi:type.
                        if modeled
                            && ns == Some(BPMN)
                            && local == "rootElement"
                            && attr_name.namespace_uri.as_deref() == Some(XSI)
                            && attr_name.local_name == "type"
                            && resolved.namespace_uri.as_deref() == Some(BPMN)
                            && resolved.local_name == "tCallableElement"
                        {
                            frame.semantic_name = Some("callableElement".into());
                        }
                        if report.qname_values.insert(resolved.clone()) {
                            account(&mut metadata_bytes, &resolved)?;
                        }
                    }
                }
                if modeled {
                    geometry(ns, local, &attrs)?;
                }
                // Keep the frame, including empty tags expanded by quick-xml.
                frame.text_bytes = 0;
                stack.push(frame);
            }
            Event::End(_) => {
                let frame = stack
                    .pop()
                    .ok_or_else(|| xml("unexpected closing element"))?;
                if let Some(value) = frame.reference_text {
                    let resolved = resolve_reference(&reader, &value, frame.idref_text)?;
                    push_reference(
                        &mut references,
                        &resolved,
                        frame.external_reference_allowed,
                        &mut metadata_bytes,
                    )?;
                    report.qname_values.insert(resolved);
                }
            }
            Event::Text(text) => {
                preceding_content = true;
                if text.contains("]]>") {
                    return Err(xml("CDATA terminator in plain text"));
                }
                add_text(&mut stack, &text)?;
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err(xml("CDATA outside root"));
                }
                add_text(&mut stack, &text)?;
            }
            Event::GeneralRef(value) => {
                if stack.is_empty() {
                    return Err(xml("entity reference outside root"));
                }
                let ch = value.resolve_char_ref().map_err(xml)?;
                let text = if let Some(ch) = ch {
                    if !xml_char(ch) {
                        return Err(xml("invalid XML character reference"));
                    }
                    ch.to_string()
                } else {
                    match value.as_ref() {
                        "lt" => "<",
                        "gt" => ">",
                        "amp" => "&",
                        "apos" => "'",
                        "quot" => "\"",
                        _ => return Err(xml("unknown or external entity reference")),
                    }
                    .to_owned()
                };
                add_text(&mut stack, &text)?;
            }
            Event::PI(instruction) => {
                processcraft_core::bpmn_xml::validate_processing_instruction(
                    instruction.target(),
                    None,
                )
                .map_err(xml)?;
                preceding_content = true;
            }
            Event::Comment(_) => {
                preceding_content = true;
            }
            Event::Eof => break,
            Event::Empty(_) => unreachable!("empty tags are expanded"),
        }
    }
    if !seen_root || !stack.is_empty() {
        return Err(xml("incomplete XML document"));
    }
    let mut unresolved_external = false;
    for (reference, external_allowed) in references {
        let outside = if external_allowed {
            target_namespace.as_ref().map_or_else(
                || !matches!(reference.namespace_uri.as_deref(), None | Some(BPMN)),
                |target| reference.namespace_uri.as_deref().unwrap_or("") != target,
            )
        } else {
            !matches!(reference.namespace_uri.as_deref(), None | Some(BPMN))
                && reference.namespace_uri != target_namespace
        };
        if outside {
            if external_allowed {
                // QName targets may belong to imported definitions. Preserve the
                // source without fetching them or claiming reference validation.
                unresolved_external = true;
                continue;
            }
            return Err(error(
                ImportErrorKind::Reference,
                "local ID reference points outside this definitions namespace",
            ));
        }
        if !ids.contains(&reference.local_name) {
            return Err(error(
                ImportErrorKind::Reference,
                format!("known local reference {reference:?} has no standard ID"),
            ));
        }
    }
    if unresolved_external {
        report.limitations.push(
            "External QName targets are preserved without loading or resolving them; their existence and semantic kind are unverified",
        );
    }
    report.standard_id_count = ids.len();
    Ok(PreservedBpmn {
        source: bytes.to_vec(),
        report,
    })
}

fn add_text(stack: &mut [Frame], text: &str) -> Result<(), ImportError> {
    if stack.is_empty() && !text.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
        return Err(xml("non-whitespace outside root"));
    }
    for frame in stack.iter_mut() {
        if frame.documentation || frame.label {
            frame.text_bytes += text.len();
            let (max, kind) = if frame.documentation {
                (MAX_DOCUMENTATION_BYTES, ImportErrorKind::Documentation)
            } else {
                (MAX_LABEL_BYTES, ImportErrorKind::Label)
            };
            if frame.text_bytes > max {
                return Err(error(kind, format!("individual text exceeds {max} bytes")));
            }
        }
    }
    if let Some(frame) = stack.last_mut()
        && let Some(value) = &mut frame.reference_text
    {
        if value.len() + text.len() > MAX_LABEL_BYTES {
            return Err(error(
                ImportErrorKind::Reference,
                "reference exceeds 16 KiB",
            ));
        }
        value.push_str(text);
    }
    Ok(())
}

fn expanded(
    (ns, local): (ResolveResult<'_>, quick_xml::name::LocalName<'_>),
) -> Result<ExpandedName, ImportError> {
    let namespace_uri = match ns {
        ResolveResult::Unbound => None,
        ResolveResult::Bound(ns) => {
            let value = quick_xml::escape::unescape(ns.0).map_err(xml)?;
            validate_namespace_name(&value)?;
            if value.len() > MAX_LABEL_BYTES {
                return Err(error(
                    ImportErrorKind::Metadata,
                    "namespace URI exceeds 16 KiB",
                ));
            }
            Some(value.into_owned())
        }
        ResolveResult::Unknown(prefix) => {
            return Err(xml(format!("unbound namespace prefix {prefix}")));
        }
    };
    if !ncname(local.into_inner()) || local.into_inner().len() > MAX_LABEL_BYTES {
        return Err(xml("invalid or excessive XML local name"));
    }
    Ok(ExpandedName {
        namespace_uri,
        local_name: local.into_inner().to_owned(),
    })
}

fn validate_namespace_name(value: &str) -> Result<(), ImportError> {
    if value.chars().any(char::is_whitespace) {
        return Err(xml("M0 refuses whitespace-containing namespace names"));
    }
    Ok(())
}

fn resolve_value(reader: &NsReader<&[u8]>, value: &str) -> Result<ExpandedName, ImportError> {
    let value = value.trim();
    validate_qname(value)?;
    expanded(reader.resolver().resolve_element(QName(value)))
}

/// XML IDREF has NCName lexical space and does not inherit the default namespace.
/// QName references, in contrast, retain their expanded namespace identity.
fn resolve_reference(
    reader: &NsReader<&[u8]>,
    value: &str,
    idref: bool,
) -> Result<ExpandedName, ImportError> {
    if !idref {
        return resolve_value(reader, value);
    }
    let value = value.trim();
    if !ncname(value) || value.len() > MAX_LABEL_BYTES {
        return Err(error(ImportErrorKind::Reference, "invalid XML IDREF"));
    }
    Ok(ExpandedName {
        namespace_uri: None,
        local_name: value.to_owned(),
    })
}

fn push_reference(
    refs: &mut Vec<(ExpandedName, bool)>,
    value: &ExpandedName,
    external_allowed: bool,
    budget: &mut usize,
) -> Result<(), ImportError> {
    account(budget, value)?;
    refs.push((value.clone(), external_allowed));
    Ok(())
}

/// These references use xs:IDREF, not xs:QName, in the pinned Semantic.xsd.
/// Keep local resolution even when other QName-valued references are preserved.
fn idref_reference(ns: Option<&str>, element: &str, attr: &str) -> bool {
    ns == Some(BPMN)
        && (matches!(
            (element, attr),
            ("sequenceFlow", "sourceRef" | "targetRef")
                | ("dataObjectReference", "dataObjectRef")
                | ("ioBinding", "inputDataRef" | "outputDataRef")
        ) || attr == "default")
}
fn idref_text_reference(element: &str) -> bool {
    matches!(
        element,
        "flowNodeRef"
            | "sourceRef"
            | "targetRef"
            | "dataInputRefs"
            | "optionalInputRefs"
            | "whileExecutingInputRefs"
            | "outputSetRefs"
            | "dataOutputRefs"
            | "optionalOutputRefs"
            | "whileExecutingOutputRefs"
            | "inputSetRefs"
    )
}
/// Service/type bindings name declarations outside the BPMN ID catalog, even
/// when a schema or WSDL declaration shares the definitions target namespace.
fn metadata_qname(ns: Option<&str>, element: &str, attr: &str) -> bool {
    ns == Some(BPMN)
        && matches!(
            (element, attr),
            ("itemDefinition", "structureRef")
                | ("interface" | "operation", "implementationRef")
                | ("resourceParameter", "type")
                | ("correlationProperty", "type")
        )
}

// QName text is classified by its owning declaration, rather than just its
// local name. Port/set references above remain IDREF-valued.
fn activity_qname_text_reference(parent: Option<&str>, element: &str) -> bool {
    match (parent, element) {
        (
            Some(
                "callableElement"
                | "process"
                | "globalTask"
                | "globalManualTask"
                | "globalBusinessRuleTask"
                | "globalScriptTask"
                | "globalUserTask",
            ),
            "supportedInterfaceRef",
        )
        | (Some("process"), "supports")
        | (
            Some("resourceRole" | "performer" | "humanPerformer" | "potentialOwner"),
            "resourceRef",
        )
        | (Some("correlationKey"), "correlationPropertyRef")
        | (Some("multiInstanceLoopCharacteristics"), "loopDataInputRef" | "loopDataOutputRef") => {
            true
        }
        (Some("participant"), "interfaceRef" | "endPointRef")
        | (Some("participantAssociation"), "innerParticipantRef" | "outerParticipantRef")
        | (
            Some("conversation" | "subConversation" | "callConversation"),
            "participantRef" | "messageFlowRef",
        )
        | (Some("choreographyTask" | "subChoreography" | "callChoreography"), "participantRef")
        | (Some("choreographyTask"), "messageFlowRef")
        | (Some("partnerEntity" | "partnerRole"), "participantRef")
        | (
            Some(
                "collaboration" | "globalConversation" | "choreography" | "globalChoreographyTask",
            ),
            "choreographyRef",
        ) => true,
        (Some(parent), "categoryValueRef") => flow_element_name(parent),
        _ => false,
    }
}

fn flow_element_name(name: &str) -> bool {
    matches!(
        name,
        "task"
            | "userTask"
            | "manualTask"
            | "serviceTask"
            | "scriptTask"
            | "sendTask"
            | "receiveTask"
            | "businessRuleTask"
            | "callActivity"
            | "subProcess"
            | "adHocSubProcess"
            | "transaction"
            | "startEvent"
            | "endEvent"
            | "intermediateCatchEvent"
            | "intermediateThrowEvent"
            | "boundaryEvent"
            | "implicitThrowEvent"
            | "event"
            | "exclusiveGateway"
            | "inclusiveGateway"
            | "complexGateway"
            | "parallelGateway"
            | "eventBasedGateway"
            | "sequenceFlow"
            | "dataObject"
            | "dataObjectReference"
            | "dataStoreReference"
            | "choreographyTask"
            | "callChoreography"
            | "subChoreography"
    )
}
fn account(budget: &mut usize, value: &ExpandedName) -> Result<(), ImportError> {
    *budget += value.local_name.len() + value.namespace_uri.as_ref().map_or(0, String::len);
    if *budget > MAX_RAW_BYTES {
        return Err(error(
            ImportErrorKind::Metadata,
            "expanded report metadata exceeds 32 MiB",
        ));
    }
    Ok(())
}

fn known_reference(ns: Option<&str>, element: &str, attr: &str) -> bool {
    match ns {
        Some(BPMNDI) => matches!(
            (element, attr),
            ("BPMNPlane" | "BPMNShape" | "BPMNEdge", "bpmnElement")
                | ("BPMNShape", "choreographyActivityShape")
                | ("BPMNEdge", "sourceElement" | "targetElement")
                | ("BPMNLabel", "labelStyle")
        ),
        Some(BPMN) => matches!(
            (element, attr),
            (
                "sequenceFlow" | "messageFlow" | "association",
                "sourceRef" | "targetRef"
            ) | ("conversationLink", "sourceRef" | "targetRef")
                | ("callConversation", "calledCollaborationRef")
                | (
                    "choreographyTask"
                        | "subChoreography"
                        | "callChoreography"
                        | "globalChoreographyTask",
                    "initiatingParticipantRef"
                )
                | ("callChoreography", "calledChoreographyRef")
                | (
                    "conversationAssociation",
                    "innerConversationNodeRef" | "outerConversationNodeRef"
                )
                | (
                    "messageFlowAssociation",
                    "innerMessageFlowRef" | "outerMessageFlowRef"
                )
                | ("participant", "processRef")
                | ("callActivity", "calledElement")
                | ("process", "definitionalCollaborationRef")
                | ("correlationSubscription", "correlationKeyRef")
                | ("correlationPropertyBinding", "correlationPropertyRef")
                | ("correlationPropertyRetrievalExpression", "messageRef")
                | ("resourceParameterBinding", "parameterRef")
                | (
                    "multiInstanceLoopCharacteristics",
                    "oneBehaviorEventRef" | "noneBehaviorEventRef"
                )
                | ("messageFlow", "messageRef")
                | ("lane", "partitionElementRef")
                | ("boundaryEvent", "attachedToRef")
                | ("dataObjectReference", "dataObjectRef")
                | ("dataStoreReference", "dataStoreRef")
                | (
                    "ioBinding",
                    "inputDataRef" | "outputDataRef" | "operationRef"
                )
                | (
                    "exclusiveGateway"
                        | "inclusiveGateway"
                        | "complexGateway"
                        | "task"
                        | "userTask"
                        | "manualTask"
                        | "serviceTask"
                        | "scriptTask"
                        | "sendTask"
                        | "receiveTask"
                        | "businessRuleTask"
                        | "subProcess"
                        | "callActivity"
                        | "transaction"
                        | "adHocSubProcess",
                    "default"
                )
                | (
                    "messageEventDefinition" | "receiveTask" | "sendTask",
                    "messageRef"
                )
                | ("serviceTask" | "receiveTask" | "sendTask", "operationRef")
                | ("signalEventDefinition", "signalRef")
                | ("errorEventDefinition", "errorRef")
                | ("escalationEventDefinition", "escalationRef")
                | ("compensateEventDefinition", "activityRef")
                | ("message", "itemRef")
                | ("signal" | "error" | "escalation", "structureRef")
                | (
                    "dataObject"
                        | "dataObjectReference"
                        | "dataStore"
                        | "dataStoreReference"
                        | "dataInput"
                        | "dataOutput"
                        | "inputDataItem"
                        | "outputDataItem"
                        | "property",
                    "itemSubjectRef"
                )
        ),
        _ => false,
    }
}

fn geometry(
    ns: Option<&str>,
    name: &str,
    attrs: &std::collections::HashMap<String, String>,
) -> Result<(), ImportError> {
    let keys: &[&str] = match (ns, name) {
        (Some(DC), "Bounds") => &["x", "y", "width", "height"],
        (Some(DC), "Point") | (Some(DI), "waypoint") => &["x", "y"],
        _ => return Ok(()),
    };
    for key in keys {
        let value = attrs
            .get(*key)
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| {
                error(
                    ImportErrorKind::Geometry,
                    format!("missing or invalid {key}"),
                )
            })?;
        if !value.is_finite()
            || value.abs() > MAX_GEOMETRY
            || (matches!(*key, "width" | "height") && value < 0.0)
        {
            return Err(error(
                ImportErrorKind::Geometry,
                "DI geometry must be finite and bounded; sizes must be non-negative",
            ));
        }
    }
    Ok(())
}

fn validate_qname(value: &str) -> Result<(), ImportError> {
    let mut parts = value.split(':');
    if !parts.next().is_some_and(ncname)
        || parts.next().is_some_and(|s| !ncname(s))
        || parts.next().is_some()
        || value.len() > MAX_LABEL_BYTES
    {
        return Err(xml("invalid or excessive QName"));
    }
    Ok(())
}
fn ncname(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(name_start) && chars.all(|c| name_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}
fn name_start(c: char) -> bool {
    matches!(c, '_' | 'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}')
}
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}
