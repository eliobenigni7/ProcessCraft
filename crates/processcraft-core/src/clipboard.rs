//! Copy and paste of diagram content (bpmn-js copy-paste behavior, re-implemented).
//!
//! A clipboard holds copied process nodes with everything they own (subprocess content,
//! event definitions, ports), the flows whose ends are both copied, and their DI in one
//! diagram. Pasting remaps every identity defined inside the clipboard to a fresh one and
//! returns ordinary create mutations, applied as one batch and one undo step.
use crate::{
    bpmn_model::{EventReference, NodeDetails},
    commands::Mutation,
    ids::{ElementIdentity, ElementSelector, NativeElementId, XmlId},
    model::{Bounds, BpmnDocument, Diagram, Edge, Flow, Label, Node, Process, Shape},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clipboard {
    /// Copied nodes with the scope (process or subprocess) they were copied from.
    pub nodes: Vec<(ElementSelector, Node)>,
    pub flows: Vec<(ElementSelector, Flow)>,
    pub shapes: Vec<Shape>,
    pub edges: Vec<Edge>,
}

/// Mutations creating the pasted content, and the new top-level elements to select.
pub struct Pasted {
    pub mutations: Vec<Mutation>,
    pub selection: Vec<ElementSelector>,
}

fn host(node: &Node) -> Option<&ElementSelector> {
    match &node.details {
        Some(NodeDetails::BoundaryEvent {
            attached_to: EventReference::Local(host),
            ..
        })
        | Some(NodeDetails::Event {
            attached_to: Some(EventReference::Local(host)),
            ..
        }) => Some(host),
        _ => None,
    }
}

fn local(reference: &Option<EventReference>) -> Option<&ElementSelector> {
    match reference {
        Some(EventReference::Local(id)) => Some(id),
        _ => None,
    }
}

/// Every identity defined by the clipboard content, found with the model's own enumeration
/// on a scratch document so nested identities (definitions, ports, subprocess content,
/// DI labels) are never missed.
fn defined_ids(clipboard: &Clipboard) -> Vec<ElementIdentity> {
    let mut scratch = BpmnDocument::new(String::new(), ElementIdentity::anonymous());
    let process = Process {
        activity_metadata: None,
        is_executable: None,
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: ElementIdentity::anonymous(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: clipboard.nodes.iter().map(|(_, n)| n.clone()).collect(),
        lanes: vec![],
        flows: clipboard.flows.iter().map(|(_, f)| f.clone()).collect(),
    };
    let diagram = Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![],
        id: ElementIdentity::anonymous(),
        plane_id: ElementIdentity::anonymous(),
        plane: process.id.selector(),
        shapes: clipboard.shapes.clone(),
        edges: clipboard.edges.clone(),
    };
    let skip = [
        scratch.definitions_id.clone(),
        process.id.clone(),
        diagram.id.clone(),
        diagram.plane_id.clone(),
    ];
    scratch.processes.push(process);
    scratch.diagrams.push(diagram);
    scratch
        .element_identities()
        .into_iter()
        .filter(|id| !skip.contains(id))
        .cloned()
        .collect()
}

/// Copy the selected nodes of one diagram. can_copy: a boundary event travels only with its
/// host; lanes and participants are not copied, so message flows (which need both
/// participants) never are either. Flows are copied when both of their ends are.
pub fn copy(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    selection: &[ElementSelector],
) -> Result<Clipboard, &'static str> {
    let diagram = document
        .diagrams
        .iter()
        .find(|d| &d.id == diagram_id)
        .ok_or("diagram not found")?;
    let wanted: HashSet<&ElementSelector> = selection.iter().collect();
    let scopes = crate::queries::scopes(document);
    // Containing scope of each subprocess, to skip content already copied with its owner.
    let parents: HashMap<ElementSelector, ElementSelector> = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .nodes
                .iter()
                .filter(|n| matches!(n.details, Some(NodeDetails::SubProcess { .. })))
                .map(|n| (n.id.selector(), scope.id.selector()))
        })
        .collect();
    let inside_copied = |scope: &ElementSelector| {
        let mut cursor = Some(scope);
        for _ in 0..64 {
            let Some(current) = cursor else { break };
            if wanted.contains(current) {
                return true;
            }
            cursor = parents.get(current);
        }
        false
    };
    let drawn = |id: &ElementIdentity| diagram.shapes.iter().any(|s| s.element == *id);
    let mut nodes = Vec::new();
    let mut flows = Vec::new();
    for scope in &scopes {
        let scope_id = scope.id.selector();
        if inside_copied(&scope_id) {
            continue;
        }
        let start = nodes.len();
        for node in scope.nodes {
            let copied = match host(node) {
                Some(host) => wanted.contains(host),
                None => wanted.contains(&node.id.selector()),
            };
            if copied && drawn(&node.id) {
                nodes.push((scope_id.clone(), node.clone()));
            }
        }
        let ids: HashSet<ElementSelector> = nodes[start..]
            .iter()
            .map(|(_, n)| n.id.selector())
            .collect();
        for flow in scope.flows {
            if let (Some(source), Some(target)) = (local(&flow.source), local(&flow.target))
                && ids.contains(source)
                && ids.contains(target)
            {
                flows.push((scope_id.clone(), flow.clone()));
            }
        }
    }
    if nodes.is_empty() {
        return Err("nothing to copy");
    }
    let mut clipboard = Clipboard {
        nodes,
        flows,
        shapes: vec![],
        edges: vec![],
    };
    let defined: HashSet<ElementSelector> = defined_ids(&clipboard)
        .iter()
        .map(|id| id.selector())
        .collect();
    clipboard.shapes = diagram
        .shapes
        .iter()
        .filter(|s| defined.contains(&s.element))
        .cloned()
        .collect();
    clipboard.edges = diagram
        .edges
        .iter()
        .filter(|e| defined.contains(&e.element))
        .cloned()
        .collect();
    Ok(clipboard)
}

impl Clipboard {
    /// Bounding box of the copied shapes.
    pub fn bounds(&self) -> Option<Bounds> {
        let (x0, y0, x1, y1) = self.shapes.iter().map(|s| &s.bounds).fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(x0, y0, x1, y1), b| {
                (
                    x0.min(b.x),
                    y0.min(b.y),
                    x1.max(b.x + b.width),
                    y1.max(b.y + b.height),
                )
            },
        );
        (x0 <= x1).then_some(Bounds {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }
}

/// `Task_0abc` becomes `Task_<fresh>`; the prefix keeps the element's naming convention.
fn fresh_xml(old: &XmlId) -> Result<XmlId, &'static str> {
    let old = old.to_string();
    let prefix = match old.rsplit_once('_') {
        Some(("", _)) => "Element",
        Some((prefix, _)) => prefix,
        None => old.as_str(),
    };
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
        .parse()
        .map_err(|_| "cannot allocate a pasted XML ID")
}

/// String values under these keys are user text, never identity references.
const TEXT_KEYS: [&str; 7] = [
    "name",
    "documentation",
    "text",
    "body",
    "owner",
    "contact",
    "condition",
];

fn remap(value: &mut Value, map: &HashMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(new) = map.get(s.as_str()) {
                *s = new.clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| remap(v, map)),
        Value::Object(fields) => {
            for (key, v) in fields.iter_mut() {
                if !(v.is_string() && TEXT_KEYS.contains(&key.as_str())) {
                    remap(v, map);
                }
            }
        }
        _ => {}
    }
}

fn offset_label(label: &mut Option<Label>, dx: f64, dy: f64) {
    if let Some(b) = label.as_mut().and_then(|l| l.bounds.as_mut()) {
        b.x += dx;
        b.y += dy;
    }
}

/// Paste `clipboard` into a diagram moved by (dx, dy). Nodes return to their original scope
/// when it exists in `document`, otherwise they go to `fallback_scope`.
pub fn paste(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    fallback_scope: &ElementSelector,
    clipboard: &Clipboard,
    dx: f64,
    dy: f64,
) -> Result<Pasted, &'static str> {
    let diagram = document
        .diagrams
        .iter()
        .find(|d| &d.id == diagram_id)
        .ok_or("diagram not found")?;
    if !(dx.is_finite() && dy.is_finite()) {
        return Err("invalid geometry");
    }
    let mut map = HashMap::new();
    for id in defined_ids(clipboard) {
        match id {
            ElementIdentity::Legacy(xml) => {
                map.insert(xml.to_string(), fresh_xml(&xml)?.to_string());
            }
            ElementIdentity::Native { native_id, xml_id } => {
                map.insert(native_id.to_string(), NativeElementId::new().to_string());
                if let Some(xml) = xml_id {
                    map.insert(xml.to_string(), fresh_xml(&xml)?.to_string());
                }
            }
        }
    }
    let mut value = serde_json::to_value(clipboard).map_err(|_| "cannot read the clipboard")?;
    remap(&mut value, &map);
    let mut pasted: Clipboard =
        serde_json::from_value(value).map_err(|_| "cannot remap the clipboard")?;
    for shape in &mut pasted.shapes {
        shape.bounds.x += dx;
        shape.bounds.y += dy;
        offset_label(&mut shape.label, dx, dy);
    }
    for edge in &mut pasted.edges {
        for point in &mut edge.waypoints {
            point.x += dx;
            point.y += dy;
        }
        offset_label(&mut edge.label, dx, dy);
    }
    // A scope is kept only when the target diagram shows it (as its plane, its own shape, or
    // the shape of one of its nodes); pasting into another process's diagram uses the fallback.
    let drawn: HashSet<&ElementSelector> = diagram.shapes.iter().map(|s| &s.element).collect();
    let scopes: HashSet<ElementSelector> = crate::queries::scopes(document)
        .iter()
        .filter(|s| {
            diagram.plane == *s.id
                || drawn.contains(&s.id.selector())
                || s.nodes.iter().any(|n| drawn.contains(&n.id.selector()))
        })
        .map(|s| s.id.selector())
        .collect();
    let scope = |id: &ElementSelector| {
        if scopes.contains(id) {
            id.clone()
        } else {
            fallback_scope.clone()
        }
    };
    let document_id = document.id;
    let selection = pasted
        .nodes
        .iter()
        .map(|(_, n)| n.id.selector())
        .chain(pasted.flows.iter().map(|(_, f)| f.id.selector()))
        .collect();
    let mut mutations = Vec::new();
    for (scope_id, element) in pasted.nodes {
        mutations.push(Mutation::CreateElement {
            document_id,
            process_id: scope(&scope_id),
            element,
        });
    }
    for (scope_id, flow) in pasted.flows {
        mutations.push(Mutation::CreateFlow {
            document_id,
            process_id: Some(scope(&scope_id)),
            flow,
        });
    }
    for shape in pasted.shapes {
        mutations.push(Mutation::CreateShape {
            document_id,
            diagram_id: diagram_id.clone(),
            shape,
        });
    }
    for edge in pasted.edges {
        mutations.push(Mutation::CreateEdge {
            document_id,
            diagram_id: diagram_id.clone(),
            edge,
        });
    }
    Ok(Pasted {
        mutations,
        selection,
    })
}
