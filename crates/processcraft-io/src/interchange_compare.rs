//! Read-only comparison of XML-representable model content across reimports.
//!
//! Native persistence must still compare the exact native content identity. This
//! projection only removes import-local handles: it never grants export eligibility,
//! invents an XML ID, resolves an external QName, or rewrites payload text. Callers
//! validate their project and XML reference requirements before using it.
use processcraft_core::{
    bpmn_model::NodeDetails,
    bpmn_xml::MixedXml,
    ids::{ElementIdentity, ElementSelector, NativeElementId, XmlId},
    model::{
        BpmnDocument, DefinitionsMetadata, DiLabelVisibility, GENERATED_TARGET_NAMESPACE, Label,
    },
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

static EMPTY_FLOW_METADATA: LazyLock<Value> =
    LazyLock::new(|| json!(processcraft_core::bpmn_activity::FlowElementMetadata::default()));

struct Target {
    identity_path: String,
    xml_id: Option<XmlId>,
}
// These are identity slots in the closed, typed BpmnDocument serialization.
// XML payload attributes are typed XmlAttribute records, never arbitrary JSON
// properties. New identity slot names must be explicitly added here.
fn identity_slot(key: &str) -> bool {
    matches!(
        key,
        "id" | "definitions_id" | "plane_id" | "lane_set_id" | "child_lane_set_id"
    )
}
fn child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn native_identity(value: &Value) -> Option<ElementIdentity> {
    let object = value.as_object()?;
    (object.len() == 2 && object.contains_key("native_id") && object.contains_key("xml_id"))
        .then(|| serde_json::from_value(value.clone()).ok())
        .flatten()
}
fn collect(
    value: &Value,
    path: &str,
    known: &HashSet<NativeElementId>,
    targets: &mut HashMap<NativeElementId, Target>,
) -> Result<(), String> {
    if let Some(ElementIdentity::Native { native_id, xml_id }) = native_identity(value) {
        if !identity_slot(path.rsplit('/').next().unwrap_or_default())
            || !known.contains(&native_id)
        {
            return Err(format!("Unrecognized native identity owner at {path}"));
        }
        if targets
            .insert(
                native_id,
                Target {
                    identity_path: path.into(),
                    xml_id,
                },
            )
            .is_some()
        {
            return Err("Native identity has multiple serialized owners".into());
        }
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                collect(value, &child(path, key), known, targets)?;
            }
        }
        Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                collect(value, &child(path, &index.to_string()), known, targets)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn normalize_annotation(object: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    if object.get("kind").and_then(Value::as_str) != Some("TextAnnotation") {
        return Ok(());
    }
    let details = object.get("details").filter(|value| !value.is_null());
    let mut annotation = match details {
        Some(value) => serde_json::from_value::<NodeDetails>(value.clone())
            .map_err(|error| error.to_string())?,
        None => NodeDetails::TextAnnotation {
            text_format: None,
            text: None,
        },
    };
    let NodeDetails::TextAnnotation { text, .. } = &mut annotation else {
        return Err("Text annotation has incompatible node details".into());
    };
    if text.is_none()
        && let Some(legacy_body) = object.get("name").and_then(Value::as_str)
    {
        *text = Some(MixedXml::Text(legacy_body.into()));
        object.insert("name".into(), Value::Null);
    }
    object.insert(
        "details".into(),
        serde_json::to_value(annotation).map_err(|error| error.to_string())?,
    );
    Ok(())
}
fn namespace_prefix(value: &Value) -> Option<&str> {
    match value {
        Value::Array(pair) if pair.len() == 2 && pair[1].is_string() => pair[0].as_str(),
        Value::Object(binding)
            if binding.len() == 2 && binding.get("uri").is_some_and(Value::is_string) =>
        {
            binding.get("prefix").and_then(Value::as_str)
        }
        _ => None,
    }
}
fn normalize_namespaces(object: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let Some(Value::Array(bindings)) = object.get_mut("namespaces") else {
        return Ok(());
    };
    let mut prefixes = HashSet::new();
    for binding in bindings.iter() {
        let prefix =
            namespace_prefix(binding).ok_or("Unrecognized namespace binding representation")?;
        if !prefixes.insert(prefix) {
            return Err("Duplicate namespace prefix in one captured scope".into());
        }
    }
    bindings.retain(|binding| {
        let uri = match binding {
            Value::Array(pair) => pair[1].as_str(),
            Value::Object(value) => value.get("uri").and_then(Value::as_str),
            _ => None,
        };
        !(namespace_prefix(binding) == Some("xml")
            && uri == Some("http://www.w3.org/XML/1998/namespace"))
    });
    bindings.sort_by(|a, b| namespace_prefix(a).cmp(&namespace_prefix(b)));
    Ok(())
}
fn text_segment(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    (object.len() == 2 && object.get("type").and_then(Value::as_str) == Some("text"))
        .then(|| object.get("text").and_then(Value::as_str))
        .flatten()
}
fn coalesce_text(values: &mut Vec<Value>) {
    let mut result: Vec<Value> = Vec::with_capacity(values.len());
    for value in std::mem::take(values) {
        if let Some(text) = text_segment(&value) {
            if text.is_empty() {
                continue;
            }
            if let Some(previous) = result.last_mut()
                && text_segment(previous).is_some()
            {
                let text_value = previous.get_mut("text").expect("text segment");
                let mut joined = text_value.as_str().expect("text segment").to_owned();
                joined.push_str(text);
                *text_value = Value::String(joined);
                continue;
            }
        }
        result.push(value);
    }
    *values = result;
}
fn simple_bpmn_scope(value: &Value) -> bool {
    let Some(bindings) = value.as_array() else {
        return false;
    };
    bindings.len() == 1
        && namespace_prefix(&bindings[0]) == Some("")
        && match &bindings[0] {
            Value::Array(pair) => {
                pair[1].as_str() == Some("http://www.omg.org/spec/BPMN/20100524/MODEL")
            }
            Value::Object(binding) => {
                binding.get("uri").and_then(Value::as_str)
                    == Some("http://www.omg.org/spec/BPMN/20100524/MODEL")
            }
            _ => false,
        }
}
fn joined_text(values: &[Value]) -> Option<String> {
    let mut result = String::new();
    for value in values {
        result.push_str(text_segment(value)?);
    }
    Some(result)
}
fn canonical_text_representation(object: &serde_json::Map<String, Value>) -> Option<Value> {
    if object.len() == 2
        && simple_bpmn_scope(object.get("namespaces")?)
        && let Some(nodes) = object.get("nodes").and_then(Value::as_array)
    {
        return joined_text(nodes).map(Value::String);
    }
    if object.len() == 1
        && let Some(parts) = object.get("parts").and_then(Value::as_array)
    {
        return joined_text(parts).map(Value::String);
    }
    None
}
fn normalize_legacy_documentation(object: &mut serde_json::Map<String, Value>) {
    let legacy_owner = object.contains_key("documentation")
        && (object.contains_key("owner") && object.contains_key("contact")
            || object.contains_key("nodes")
                && object.contains_key("flows")
                && object.contains_key("id")
            || object.contains_key("source")
                && object.contains_key("target")
                && object.contains_key("id"));
    if !legacy_owner {
        return;
    }
    let Some(Value::Array(docs)) = object.get("documentation") else {
        return;
    };
    let text = if docs.is_empty() {
        Some(String::new())
    } else if docs.len() == 1 {
        let doc = docs[0].as_object();
        doc.filter(|doc| {
            !doc.contains_key("id") && doc.get("text_format").is_none_or(Value::is_null)
        })
        .and_then(|doc| doc.get("content")?.as_str())
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
    } else {
        None
    };
    if let Some(text) = text {
        object.insert("documentation".into(), Value::String(text));
    }
}
fn bare_expression(object: &serde_json::Map<String, Value>) -> Option<Value> {
    if object.contains_key("id")
        || object.contains_key("foreign_attributes")
        || !simple_bpmn_scope(object.get("namespaces")?)
    {
        return None;
    }
    let kind = object.get("kind")?.as_object()?;
    if kind.get("type")?.as_str()? != "base" || kind.get("explicit_type")?.as_bool()? {
        return None;
    }
    object
        .get("text")?
        .as_str()
        .map(|text| Value::String(text.into()))
}
fn normalize(value: &mut Value, targets: &HashMap<NativeElementId, Target>) -> Result<(), String> {
    if let Some(ElementIdentity::Native { xml_id, .. }) = native_identity(value) {
        *value = xml_id.map_or(Value::Null, |id| json!(id));
        return Ok(());
    }
    if value
        .as_object()
        .is_some_and(|object| object.len() == 1 && object.contains_key("native_id"))
    {
        let ElementSelector::Native { native_id } = serde_json::from_value(value.clone())
            .map_err(|error| format!("Invalid native selector: {error}"))?
        else {
            return Err("Invalid native selector shape".into());
        };
        let target = targets
            .get(&native_id)
            .ok_or("Native selector has no typed identity owner")?;
        *value = target.xml_id.as_ref().map_or_else(
            || json!({"nativeTargetAt": target.identity_path}),
            |id| json!(id),
        );
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            normalize_annotation(object)?;
            normalize_namespaces(object)?;
            for value in object.values_mut() {
                normalize(value, targets)?;
            }
            // This aggregate has no XML wrapper. Empty inherited metadata emits
            // no XML, regardless of the native legacy/explicit bookkeeping mode.
            // Nonempty incoming/outgoing lists, wrappers and explicit false are
            // retained; native persistence compares the original values exactly.
            if object.get("flow_metadata") == Some(&*EMPTY_FLOW_METADATA) {
                object.remove("flow_metadata");
            }
            if object.get("flow") == Some(&*EMPTY_FLOW_METADATA) {
                object.insert("flow".into(), Value::Null);
            }
            // A missing XML ID is omitted in XML whether the native optional slot
            // was null, skipped, or held an anonymous stable native identity.
            object.retain(|key, value| !(identity_slot(key) && value.is_null()));
            for key in ["nodes", "parts"] {
                if let Some(Value::Array(values)) = object.get_mut(key) {
                    // Only text records are coalesced. Semantic node arrays, XML
                    // elements, comments, PIs and wrapper ordering remain intact.
                    coalesce_text(values);
                }
            }
            normalize_legacy_documentation(object);
            if object.get("type").and_then(Value::as_str) == Some("processingInstruction")
                && object
                    .get("data")
                    .is_none_or(|data| data.is_null() || data.as_str() == Some(""))
            {
                // An empty PI data string and no PI data have the same XML infoset.
                object.insert("data".into(), Value::String(String::new()));
            }
            if object.contains_key("owner")
                && object.contains_key("contact")
                && object.contains_key("documentation")
                && object.contains_key("kind")
            {
                // Export's explicit native-field acknowledgement is checked before
                // comparison. Exact native archive checks still retain these fields.
                object.remove("owner");
                object.remove("contact");
            }
            object.retain(|key, value| {
                !matches!(key.as_str(), "base" | "base_extras")
                    || !value.as_object().is_some_and(serde_json::Map::is_empty)
            });
            if let Some(text) =
                canonical_text_representation(object).or_else(|| bare_expression(object))
            {
                *value = text;
            }
        }
        Value::Array(array) => {
            for value in array {
                normalize(value, targets)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn normalize_legacy_di(value: &mut Value) -> Result<(), String> {
    let Some(diagrams) = value.get_mut("diagrams").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for diagram in diagrams {
        for key in ["shapes", "edges"] {
            let Some(representations) = diagram.get_mut(key).and_then(Value::as_array_mut) else {
                continue;
            };
            for representation in representations {
                let object = representation
                    .as_object_mut()
                    .ok_or("DI representation must be a typed object")?;
                if object.get("label_visibility").is_none_or(Value::is_null) {
                    // Historical native documents displayed implicit labels. The
                    // export boundary explicitly reports their BPMNLabel bridge;
                    // imported explicit absence remains hidden and is not changed.
                    object.insert(
                        "label_visibility".into(),
                        json!(DiLabelVisibility::Explicit),
                    );
                    if object.get("label").is_none_or(Value::is_null) {
                        object.insert("label".into(), json!(Label::default()));
                    }
                }
            }
        }
    }
    Ok(())
}
fn preserve_lane_wrapper_presence(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            if object.contains_key("nodes")
                && object.contains_key("flows")
                && let Some(Value::Array(lanes)) = object.get("lanes")
            {
                let present = !lanes.is_empty()
                    || object.get("lane_set_id").is_some_and(|id| !id.is_null())
                    || object
                        .get("lane_set_base")
                        .is_some_and(|base| !base.is_null());
                if present && object.get("lane_set_base").is_none_or(Value::is_null) {
                    object.insert("lane_set_base".into(), json!({}));
                }
                let Some(Value::Array(lanes)) = object.get_mut("lanes") else {
                    unreachable!()
                };
                let parents: HashSet<ElementSelector> = lanes
                    .iter()
                    .filter_map(|lane| {
                        lane.get("parent")
                            .filter(|parent| !parent.is_null())
                            .and_then(|parent| serde_json::from_value(parent.clone()).ok())
                    })
                    .collect();
                for lane in lanes {
                    let object = lane.as_object_mut().ok_or("Lane must be a typed object")?;
                    let identity: ElementIdentity = serde_json::from_value(
                        object.get("id").ok_or("Lane identity missing")?.clone(),
                    )
                    .map_err(|error| format!("Invalid lane identity: {error}"))?;
                    let present = parents.contains(&identity.selector())
                        || object
                            .get("child_lane_set_id")
                            .is_some_and(|id| !id.is_null())
                        || object
                            .get("child_lane_set_base")
                            .is_some_and(|base| !base.is_null());
                    if present && object.get("child_lane_set_base").is_none_or(Value::is_null) {
                        object.insert("child_lane_set_base".into(), json!({}));
                    }
                }
            }
            for value in object.values_mut() {
                preserve_lane_wrapper_presence(value)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                preserve_lane_wrapper_presence(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Normalize import-local identities at their exact typed owner paths. Ordered
/// siblings remain distinct and references retain their owning target paths.
/// Workspace identity/name and retained-source bookkeeping are excluded. Legacy
/// definition/annotation representations and namespace-binding order are compared
/// by their emitted meaning; XML omission, payload order and QName URIs stay intact.
pub fn xml_model_value(document: &BpmnDocument) -> Result<Value, String> {
    let namespace = match &document.definitions {
        Some(metadata) => metadata.target_namespace.as_str(),
        None if document.provenance.is_none() => GENERATED_TARGET_NAMESPACE,
        None => {
            return Err(
                "Legacy definitions require an explicitly recovered target namespace".into(),
            );
        }
    };
    xml_model_value_for_export(document, namespace)
}
/// Supply the namespace already recovered by the export boundary from authoritative
/// Definitions metadata or validated, local retained provenance. This function never
/// opens that provenance or guesses its namespace. Missing legacy metadata is compared
/// with the actual attributes that the exporter emits, preserving explicit values.
pub fn xml_model_value_for_export(
    document: &BpmnDocument,
    effective_namespace: &str,
) -> Result<Value, String> {
    if document
        .definitions
        .as_ref()
        .is_some_and(|metadata| metadata.target_namespace != effective_namespace)
    {
        return Err("Recovered namespace disagrees with declared Definitions metadata".into());
    }
    if document.definitions.is_none()
        && document.provenance.is_none()
        && effective_namespace != GENERATED_TARGET_NAMESPACE
    {
        return Err("New document has no source for a different target namespace".into());
    }
    let known: HashSet<_> = document
        .element_identities()
        .into_iter()
        .filter_map(|identity| match identity {
            ElementIdentity::Native { native_id, .. } => Some(*native_id),
            _ => None,
        })
        .collect();
    let mut value = serde_json::to_value(document).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or("Document must serialize as an object")?;
    for key in ["id", "name", "folder", "provenance", "capability"] {
        object.remove(key);
    }
    if document.definitions.is_none() {
        object.insert(
            "definitions".into(),
            serde_json::to_value(DefinitionsMetadata {
                target_namespace: effective_namespace.into(),
                name: None,
                expression_language: None,
                type_language: None,
                exporter: None,
                exporter_version: None,
                imports: vec![],
                namespaces: vec![],
                foreign_attributes: vec![],
            })
            .map_err(|error| error.to_string())?,
        );
    }
    normalize_legacy_di(&mut value)?;
    preserve_lane_wrapper_presence(&mut value)?;
    let mut targets = HashMap::new();
    collect(&value, "", &known, &mut targets)?;
    if targets.len() != known.len() {
        return Err("Native identity is missing its typed serialized owner".into());
    }
    normalize(&mut value, &targets)?;
    Ok(value)
}
