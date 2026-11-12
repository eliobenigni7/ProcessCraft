//! Copy and paste of diagram content (bpmn-js copy-paste behavior, re-implemented).
//!
//! A clipboard holds copied process nodes with everything they own (subprocess content,
//! event definitions, ports), the flows whose ends are both copied, groups, the data objects
//! behind copied data object references, data associations whose ends are copied, pools with
//! their whole process (lanes included), message flows between copied elements, and their DI
//! in one diagram. Pasting remaps every identity defined inside the clipboard to a fresh one
//! and returns ordinary create mutations, applied as one batch and one undo step.
use crate::{
    bpmn_data::{DataAssociation, DataObject},
    bpmn_model::{EventReference, Group, NodeDetails},
    commands::{Mutation, ShapeBounds},
    ids::{ElementIdentity, ElementSelector, NativeElementId, XmlId},
    model::{
        Bounds, BpmnDocument, Collaboration, Diagram, Edge, Flow, FlowKind, Label, Node,
        Participant, Point, Process, Shape,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clipboard {
    /// Copied nodes with the scope (process or subprocess) they were copied from.
    pub nodes: Vec<(ElementSelector, Node)>,
    pub flows: Vec<(ElementSelector, Flow)>,
    /// Groups, data objects (kept with the scope of their reference), and data associations
    /// (with their owner's scope).
    #[serde(default)]
    pub groups: Vec<(ElementSelector, Group)>,
    #[serde(default)]
    pub data_objects: Vec<(ElementSelector, DataObject)>,
    #[serde(default)]
    pub data_associations: Vec<(ElementSelector, DataAssociation)>,
    /// Copied pools, each with its whole process (nodes, lanes, flows, data) when it has one.
    #[serde(default)]
    pub pools: Vec<(Participant, Option<Process>)>,
    /// Message flows whose two ends are copied.
    #[serde(default)]
    pub message_flows: Vec<Flow>,
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

fn empty_process(id: ElementIdentity) -> Process {
    Process {
        activity_metadata: None,
        is_executable: None,
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id,
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    }
}

/// Every identity defined by the clipboard content, found with the model's own enumeration
/// on a scratch document so nested identities (definitions, ports, subprocess content,
/// lanes, pools, DI labels) are never missed.
fn defined_ids(clipboard: &Clipboard) -> Vec<ElementIdentity> {
    let mut scratch = BpmnDocument::new(String::new(), ElementIdentity::anonymous());
    let process = Process {
        groups: clipboard.groups.iter().map(|(_, g)| g.clone()).collect(),
        data_objects: clipboard
            .data_objects
            .iter()
            .map(|(_, o)| o.clone())
            .collect(),
        data_associations: clipboard
            .data_associations
            .iter()
            .map(|(_, a)| a.clone())
            .collect(),
        nodes: clipboard.nodes.iter().map(|(_, n)| n.clone()).collect(),
        flows: clipboard.flows.iter().map(|(_, f)| f.clone()).collect(),
        ..empty_process(ElementIdentity::anonymous())
    };
    let collaboration = Collaboration {
        kind: None,
        extras: None,
        correlation_keys: vec![],
        base: None,
        name: None,
        id: ElementIdentity::anonymous(),
        participants: clipboard.pools.iter().map(|(p, _)| p.clone()).collect(),
        flows: clipboard.message_flows.clone(),
        conversation_nodes: vec![],
        conversation_links: vec![],
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
        collaboration.id.clone(),
        diagram.id.clone(),
        diagram.plane_id.clone(),
    ];
    scratch.processes.push(process);
    scratch
        .processes
        .extend(clipboard.pools.iter().filter_map(|(_, p)| p.clone()));
    scratch.collaboration = Some(collaboration);
    scratch.diagrams.push(diagram);
    scratch
        .element_identities()
        .into_iter()
        .filter(|id| !skip.contains(id))
        .cloned()
        .collect()
}

/// Copy the selected elements of one diagram. can_copy: a boundary event travels only with
/// its host; a lane only with its pool (which brings its whole process); a message flow only
/// with both of its ends. Flows are copied when both of their ends are, data associations when
/// their owner and data reference are.
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
    let drawn = |id: &ElementSelector| diagram.shapes.iter().any(|s| s.element == *id);
    let pools: Vec<(Participant, Option<Process>)> = document
        .collaborations()
        .flat_map(|c| &c.participants)
        .filter(|p| wanted.contains(&p.id.selector()) && drawn(&p.id.selector()))
        .map(|p| {
            let process = local(&p.process)
                .and_then(|id| document.processes.iter().find(|x| x.id == *id))
                .cloned();
            (p.clone(), process)
        })
        .collect();
    let pool_processes: HashSet<ElementSelector> = pools
        .iter()
        .filter_map(|(_, p)| p.as_ref().map(|p| p.id.selector()))
        .collect();
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
            if wanted.contains(current) || pool_processes.contains(current) {
                return true;
            }
            cursor = parents.get(current);
        }
        false
    };
    let mut clipboard = Clipboard {
        nodes: Vec::new(),
        flows: Vec::new(),
        groups: Vec::new(),
        data_objects: Vec::new(),
        data_associations: Vec::new(),
        pools,
        message_flows: Vec::new(),
        shapes: Vec::new(),
        edges: Vec::new(),
    };
    for scope in &scopes {
        let scope_id = scope.id.selector();
        if inside_copied(&scope_id) {
            continue;
        }
        let start = clipboard.nodes.len();
        for node in scope.nodes {
            let copied = match host(node) {
                Some(host) => wanted.contains(host),
                None => wanted.contains(&node.id.selector()),
            };
            if copied && drawn(&node.id.selector()) {
                clipboard.nodes.push((scope_id.clone(), node.clone()));
                // A data object reference brings its data object along (from any enclosing
                // scope), kept with the reference's scope.
                if let Some(NodeDetails::DataObjectReference {
                    data_object_ref: Some(object),
                }) = &node.details
                    && let Some(value) = scopes
                        .iter()
                        .flat_map(|s| s.data_objects)
                        .find(|o| o.id.as_ref().is_some_and(|id| id == object))
                    && !clipboard.data_objects.iter().any(|(_, o)| o.id == value.id)
                {
                    clipboard
                        .data_objects
                        .push((scope_id.clone(), value.clone()));
                }
            }
        }
        let ids: HashSet<ElementSelector> = clipboard.nodes[start..]
            .iter()
            .map(|(_, n)| n.id.selector())
            .collect();
        for flow in scope.flows {
            if let (Some(source), Some(target)) = (local(&flow.source), local(&flow.target))
                && ids.contains(source)
                && ids.contains(target)
            {
                clipboard.flows.push((scope_id.clone(), flow.clone()));
            }
        }
        for group in scope.groups {
            if wanted.contains(&group.id.selector()) && drawn(&group.id.selector()) {
                clipboard.groups.push((scope_id.clone(), group.clone()));
            }
        }
    }
    // Data associations and message flows whose ends are all copied.
    let defined: HashSet<ElementSelector> = defined_ids(&clipboard)
        .iter()
        .map(|id| id.selector())
        .collect();
    for scope in &scopes {
        if inside_copied(&scope.id.selector()) {
            continue;
        }
        for association in scope.data_associations {
            let owner = clipboard
                .nodes
                .iter()
                .find(|(_, n)| n.id == association.owner);
            if let Some((owner_scope, _)) = owner
                && association
                    .sources
                    .iter()
                    .chain([&association.target])
                    .all(|end| defined.contains(end))
            {
                clipboard
                    .data_associations
                    .push((owner_scope.clone(), association.clone()));
            }
        }
    }
    clipboard.message_flows = document
        .collaborations()
        .flat_map(|c| &c.flows)
        .filter(|f| {
            f.kind == FlowKind::Message
                && [local(&f.source), local(&f.target)]
                    .iter()
                    .all(|end| end.is_some_and(|e| defined.contains(e)))
        })
        .cloned()
        .collect();
    if clipboard.nodes.is_empty() && clipboard.pools.is_empty() && clipboard.groups.is_empty() {
        return Err("nothing to copy");
    }
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

/// The scope pasted content goes to when its own scope is not shown in `diagram_id`: the
/// diagram's process or subprocess, else the process of its collaboration's first pool, else
/// the document's first scope.
pub fn default_scope(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
) -> Option<ElementSelector> {
    let scopes = crate::queries::scopes(document);
    let plane = &document
        .diagrams
        .iter()
        .find(|d| d.id == *diagram_id)?
        .plane;
    if scopes.iter().any(|s| *s.id == *plane) {
        return Some(plane.clone());
    }
    document
        .collaborations()
        .filter(|c| c.id == *plane)
        .flat_map(|c| &c.participants)
        .find_map(|p| local(&p.process).cloned())
        .or_else(|| scopes.first().map(|s| s.id.selector()))
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

/// Rewrite references to copied identities. A QName reference (`{namespace, local}`, written
/// with a prefix in XML) is rewritten only when it names the document's own namespace, so an
/// external element that shares a local name keeps its reference.
fn remap(value: &mut Value, map: &HashMap<String, String>, namespace: Option<&str>) {
    match value {
        Value::String(s) => {
            if let Some(new) = map.get(s.as_str()) {
                *s = new.clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| remap(v, map, namespace)),
        Value::Object(fields) => {
            if fields.len() == 2
                && let (Some(Value::String(ns)), Some(Value::String(_))) =
                    (fields.get("namespace"), fields.get("local"))
            {
                if Some(ns.as_str()) == namespace
                    && let Some(Value::String(local)) = fields.get_mut("local")
                    && let Some(new) = map.get(local.as_str())
                {
                    *local = new.clone();
                }
                return;
            }
            // A node's `owner` is user text (next to `contact`); a data association's is its
            // owning activity.
            let node_text = fields.contains_key("contact");
            for (key, v) in fields.iter_mut() {
                let text = TEXT_KEYS.contains(&key.as_str()) && (key != "owner" || node_text);
                if !(v.is_string() && text) {
                    remap(v, map, namespace);
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

/// Paste `clipboard` into a diagram moved by (dx, dy). Nodes, groups, and data return to their
/// original scope when `document` shows it there, otherwise they go to `fallback_scope`; pasted
/// flow nodes join the lane under them. Pools need a collaboration diagram.
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
    let namespace = document
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.as_str())
        .or_else(|| {
            document
                .provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let mut value = serde_json::to_value(clipboard).map_err(|_| "cannot read the clipboard")?;
    remap(&mut value, &map, namespace);
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
        .chain(pasted.groups.iter().map(|(_, g)| g.id.selector()))
        .chain(pasted.pools.iter().map(|(p, _)| p.id.selector()))
        .chain(pasted.message_flows.iter().map(|f| f.id.selector()))
        .collect();
    // Lanes of the target diagram that pasted flow nodes land in (bpmn-js: pasted elements join
    // the lane under them).
    let mut lanes = Vec::new();
    for (scope_id, node) in &pasted.nodes {
        if !crate::rules::is_flow_node(node.kind) || host(node).is_some() {
            continue;
        }
        let Some(shape) = pasted.shapes.iter().find(|s| s.element == node.id) else {
            continue;
        };
        let centre = Point {
            x: shape.bounds.x + shape.bounds.width / 2.0,
            y: shape.bounds.y + shape.bounds.height / 2.0,
        };
        let target = scope(scope_id);
        let container = crate::manipulate::container_at(document, diagram, &centre, None);
        if container.scope.as_ref() == Some(&target)
            && let Some(lane) = container.lane
        {
            lanes.push(Mutation::AssignLane {
                document_id,
                process_id: target,
                element_id: node.id.selector(),
                lane_id: Some(lane),
                bounds: vec![ShapeBounds {
                    diagram_id: diagram_id.clone(),
                    shape_id: shape.id.selector(),
                    bounds: shape.bounds.clone(),
                }],
            });
        }
    }
    let mut mutations = Vec::new();
    if !pasted.pools.is_empty() || !pasted.message_flows.is_empty() {
        let mut root = document
            .collaborations()
            .find(|c| c.id == diagram.plane)
            .cloned()
            .ok_or("Paste pools and message flows into a collaboration diagram")?;
        for (participant, process) in pasted.pools {
            if let Some(process) = process {
                mutations.push(Mutation::CreateProcess {
                    document_id,
                    process,
                });
            }
            root.participants.push(participant);
        }
        root.flows.extend(pasted.message_flows);
        mutations.push(Mutation::SetCollaborationRoot {
            document_id,
            collaboration: root,
        });
    }
    for (scope_id, element) in pasted.nodes {
        mutations.push(Mutation::CreateElement {
            document_id,
            process_id: scope(&scope_id),
            element,
        });
    }
    for (scope_id, data_object) in pasted.data_objects {
        mutations.push(Mutation::CreateDataObject {
            document_id,
            scope_id: scope(&scope_id),
            data_object,
        });
    }
    for (scope_id, flow) in pasted.flows {
        mutations.push(Mutation::CreateFlow {
            document_id,
            process_id: Some(scope(&scope_id)),
            flow,
        });
    }
    for (scope_id, group) in pasted.groups {
        mutations.push(Mutation::CreateGroup {
            document_id,
            scope_id: scope(&scope_id),
            group,
        });
    }
    for (scope_id, association) in pasted.data_associations {
        mutations.push(Mutation::CreateDataAssociation {
            document_id,
            scope_id: scope(&scope_id),
            association,
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
    mutations.extend(lanes);
    Ok(Pasted {
        mutations,
        selection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qname_references_remap_only_in_the_document_namespace() {
        let map = HashMap::from([("Task_1".to_string(), "Task_2".to_string())]);
        let mut value = serde_json::json!({
            "own": {"namespace": "urn:local", "local": "Task_1"},
            "external": {"namespace": "urn:other", "local": "Task_1"},
            "plain": "Task_1",
            "node": {"owner": "Task_1", "contact": ""},
            "association": {"owner": "Task_1", "target": "x"},
        });
        remap(&mut value, &map, Some("urn:local"));
        assert_eq!(value["own"]["local"], "Task_2");
        assert_eq!(value["external"]["local"], "Task_1");
        assert_eq!(value["plain"], "Task_2");
        assert_eq!(value["node"]["owner"], "Task_1");
        assert_eq!(value["association"]["owner"], "Task_2");
    }
}
