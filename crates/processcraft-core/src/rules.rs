//! BPMN modeling rules answered before a mutation is built, so the GUI, CLI, and MCP refuse
//! the same edits for the same reasons. Behavior follows bpmn-js `BpmnRules` (re-implemented,
//! not copied) mapped onto this model: data associations bind generated activity/event ports,
//! message flows live in a collaboration root, and associations stay inside one scope.
use crate::{
    bpmn_activity::ModelReference,
    bpmn_model::{EventDefinition, NodeDetails},
    ids::ElementSelector,
    model::{BpmnDocument, Node, NodeKind, Participant},
};

/// The connection a source/target pair produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionKind {
    Sequence,
    Message,
    /// Data object/store reference into an activity or throw event.
    DataInput,
    /// Activity or catch event into a data object/store reference.
    DataOutput,
    /// Plain association between a text annotation and another element.
    Association,
}

/// Where an element sits: its node or pool, direct scope, enclosing subprocesses, and the
/// organisation (root process, or a pool's process) that separates message from sequence flows.
pub struct Located<'a> {
    pub node: Option<&'a Node>,
    pub participant: Option<&'a Participant>,
    /// Direct container: a process or an expanded subprocess.
    pub scope: Option<ElementSelector>,
    /// Enclosing subprocess IDs, innermost first (includes `scope` when it is a subprocess).
    pub ancestors: Vec<ElementSelector>,
    pub organisation: Option<ElementSelector>,
}

pub fn locate<'a>(doc: &'a BpmnDocument, id: &ElementSelector) -> Option<Located<'a>> {
    fn walk<'a>(
        nodes: &'a [Node],
        id: &ElementSelector,
        scope: &ElementSelector,
        ancestors: &mut Vec<ElementSelector>,
        depth: usize,
    ) -> Option<(&'a Node, ElementSelector)> {
        if depth >= 32 {
            return None;
        }
        if let Some(node) = nodes.iter().find(|n| n.id == *id) {
            return Some((node, scope.clone()));
        }
        for node in nodes {
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                let inner = node.id.selector();
                ancestors.insert(0, inner.clone());
                if let Some(found) = walk(&content.nodes, id, &inner, ancestors, depth + 1) {
                    return Some(found);
                }
                ancestors.remove(0);
            }
        }
        None
    }
    for process in &doc.processes {
        let mut ancestors = Vec::new();
        if let Some((node, scope)) = walk(
            &process.nodes,
            id,
            &process.id.selector(),
            &mut ancestors,
            0,
        ) {
            return Some(Located {
                node: Some(node),
                participant: None,
                scope: Some(scope),
                ancestors,
                organisation: Some(process.id.selector()),
            });
        }
    }
    let participant = doc
        .collaborations()
        .flat_map(|c| &c.participants)
        .find(|p| p.id == *id)?;
    Some(Located {
        node: None,
        participant: Some(participant),
        scope: None,
        ancestors: Vec::new(),
        organisation: Some(match &participant.process {
            Some(ModelReference::Local(process)) => process.clone(),
            _ => participant.id.selector(),
        }),
    })
}

fn definitions(node: &Node) -> &[EventDefinition] {
    match &node.details {
        Some(NodeDetails::Event { definitions, .. }) => definitions,
        _ => &[],
    }
}
fn has_definition(node: &Node, test: fn(&EventDefinition) -> bool) -> bool {
    definitions(node).iter().any(test)
}
fn message_or_none(node: &Node) -> bool {
    definitions(node)
        .iter()
        .all(|d| matches!(d, EventDefinition::Message { .. }))
}
pub fn is_event(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::StartEvent
            | NodeKind::EndEvent
            | NodeKind::IntermediateCatchEvent
            | NodeKind::IntermediateThrowEvent
            | NodeKind::BoundaryEvent
    )
}
pub fn is_gateway(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::ExclusiveGateway
            | NodeKind::ParallelGateway
            | NodeKind::InclusiveGateway
            | NodeKind::ComplexGateway
            | NodeKind::EventBasedGateway
    )
}
fn is_throw(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
}
fn is_catch(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
    )
}
fn is_data(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::DataObjectReference | NodeKind::DataStoreReference
    )
}
pub fn is_flow_node(kind: NodeKind) -> bool {
    kind != NodeKind::TextAnnotation && !is_data(kind)
}
pub fn is_event_subprocess(node: &Node) -> bool {
    node.activity.as_ref().and_then(|a| a.triggered_by_event) == Some(true)
}
pub fn is_for_compensation(node: &Node) -> bool {
    node.activity.as_ref().and_then(|a| a.is_for_compensation) == Some(true)
}
fn is_link(node: &Node) -> bool {
    has_definition(node, |d| matches!(d, EventDefinition::Link { .. }))
}
/// The activity a boundary event is attached to.
pub fn attached_to(node: &Node) -> Option<&ElementSelector> {
    match &node.details {
        Some(NodeDetails::BoundaryEvent {
            attached_to: ModelReference::Local(host),
            ..
        })
        | Some(NodeDetails::Event {
            attached_to: Some(ModelReference::Local(host)),
            ..
        }) => Some(host),
        _ => None,
    }
}
fn interrupting(node: &Node) -> bool {
    !matches!(
        &node.details,
        Some(NodeDetails::BoundaryEvent {
            cancel_activity: false,
            ..
        }) | Some(NodeDetails::Event {
            cancel_activity: Some(false),
            ..
        })
    )
}

/// Why `node` cannot start a sequence flow.
pub fn sequence_source_refusal(node: &Node) -> Option<&'static str> {
    if !is_flow_node(node.kind) {
        Some("Only flow nodes start sequence flows")
    } else if node.kind == NodeKind::EndEvent {
        Some("End events cannot have outgoing sequence flows")
    } else if is_event_subprocess(node) {
        Some("Event subprocesses cannot have sequence flows")
    } else if node.kind == NodeKind::IntermediateThrowEvent && is_link(node) {
        Some("Link throw events cannot have outgoing sequence flows")
    } else if node.kind == NodeKind::BoundaryEvent
        && has_definition(node, |d| matches!(d, EventDefinition::Compensate { .. }))
    {
        Some("Compensation boundary events connect by association, not sequence flow")
    } else if is_for_compensation(node) {
        Some("Compensation activities cannot have sequence flows")
    } else {
        None
    }
}
/// Why `node` cannot receive a sequence flow.
pub fn sequence_target_refusal(node: &Node) -> Option<&'static str> {
    if !is_flow_node(node.kind) {
        Some("Only flow nodes receive sequence flows")
    } else if node.kind == NodeKind::StartEvent {
        Some("Start events cannot have incoming sequence flows")
    } else if node.kind == NodeKind::BoundaryEvent {
        Some("Boundary events cannot have incoming sequence flows")
    } else if is_event_subprocess(node) {
        Some("Event subprocesses cannot have sequence flows")
    } else if node.kind == NodeKind::IntermediateCatchEvent && is_link(node) {
        Some("Link catch events cannot have incoming sequence flows")
    } else if is_for_compensation(node) {
        Some("Compensation activities cannot have sequence flows")
    } else {
        None
    }
}
fn event_based_target(node: &Node) -> bool {
    node.kind == NodeKind::ReceiveTask
        || (node.kind == NodeKind::IntermediateCatchEvent
            && has_definition(node, |d| {
                matches!(
                    d,
                    EventDefinition::Message { .. }
                        | EventDefinition::Timer { .. }
                        | EventDefinition::Conditional { .. }
                        | EventDefinition::Signal { .. }
                )
            }))
}
/// Why a sequence flow from `source` to `target` (same scope) is not allowed.
pub fn sequence_refusal(source: &Node, target: &Node) -> Option<&'static str> {
    sequence_source_refusal(source)
        .or_else(|| sequence_target_refusal(target))
        .or_else(|| {
            (source.kind == NodeKind::EventBasedGateway && !event_based_target(target)).then_some(
                "After an event-based gateway, connect only to receive tasks or message, timer, conditional, or signal catch events",
            )
        })
}

fn interaction_node(kind: NodeKind) -> bool {
    kind.is_activity() || is_event(kind)
}
fn message_source(element: &Located) -> bool {
    element.node.is_none_or(|n| {
        interaction_node(n.kind)
            && n.kind != NodeKind::BoundaryEvent
            && (!is_event(n.kind) || (is_throw(n.kind) && message_or_none(n)))
    })
}
fn message_target(element: &Located) -> bool {
    element.node.is_none_or(|n| {
        interaction_node(n.kind)
            && !is_for_compensation(n)
            && (!is_event(n.kind) || (is_catch(n.kind) && message_or_none(n)))
            && !(n.kind == NodeKind::BoundaryEvent
                && !has_definition(n, |d| matches!(d, EventDefinition::Message { .. })))
    })
}

/// Whether a connection may start at `id` (flow nodes, data references, annotations, pools).
pub fn can_start_connection(doc: &BpmnDocument, id: &ElementSelector) -> bool {
    locate(doc, id).is_some()
}

/// The connection `source -> target` creates, or why it is refused. Checked in bpmn-js order:
/// message flow, sequence flow, data association, association.
pub fn can_connect(
    doc: &BpmnDocument,
    source: &ElementSelector,
    target: &ElementSelector,
) -> Result<ConnectionKind, &'static str> {
    if source == target {
        return Err("Connecting an element to itself is not supported");
    }
    let (Some(s), Some(t)) = (locate(doc, source), locate(doc, target)) else {
        return Err("Connect flow elements, data references, annotations, or pools");
    };
    if s.organisation != t.organisation {
        if !message_source(&s) || !message_target(&t) {
            return Err(
                "Across pools, connect a sending activity, message throw event, or pool to a receiving activity, message catch event, or pool",
            );
        }
        let nested = |e: &Located| e.node.is_some() && e.scope != e.organisation;
        if nested(&s) || nested(&t) {
            return Err("Message flows into or out of a subprocess are not supported");
        }
        if doc.collaborations().next().is_none() {
            return Err("Message flows need pools; add a pool for each process first");
        }
        return Ok(ConnectionKind::Message);
    }
    let (Some(sn), Some(tn)) = (s.node, t.node) else {
        return Err("A pool connects to other pools only by message flow");
    };
    if is_flow_node(sn.kind) && is_flow_node(tn.kind) {
        if s.scope != t.scope {
            return Err("Sequence flows must stay within one process or subprocess");
        }
        return match sequence_refusal(sn, tn) {
            Some(reason) => Err(reason),
            None => Ok(ConnectionKind::Sequence),
        };
    }
    // The data reference must be visible from the owner's scope (its own or an enclosing one).
    let visible = |data: &Located, owner: &Located| {
        data.scope == owner.organisation
            || data
                .scope
                .as_ref()
                .is_some_and(|d| owner.ancestors.contains(d))
    };
    if is_data(sn.kind) && (tn.kind.is_activity() || is_throw(tn.kind)) {
        return if visible(&s, &t) {
            Ok(ConnectionKind::DataInput)
        } else {
            Err("The data reference is not visible from the target's scope")
        };
    }
    if is_data(tn.kind) && (sn.kind.is_activity() || is_catch(sn.kind)) {
        return if visible(&t, &s) {
            Ok(ConnectionKind::DataOutput)
        } else {
            Err("The data reference is not visible from the source's scope")
        };
    }
    if (sn.kind == NodeKind::TextAnnotation) != (tn.kind == NodeKind::TextAnnotation) {
        if s.ancestors.contains(target) || t.ancestors.contains(source) {
            return Err("An annotation cannot connect to its own container");
        }
        if s.scope != t.scope {
            return Err("Associations must stay within one process or subprocess");
        }
        return Ok(ConnectionKind::Association);
    }
    Err("These elements cannot be connected")
}

/// A container an element is dropped onto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    Process,
    Collaboration { has_process_pool: bool },
    Participant { collapsed: bool, has_process: bool },
    Lane,
    SubProcess { expanded: bool },
}
/// An element being dropped.
#[derive(Clone, Copy, Debug)]
pub enum Dropped<'a> {
    Node(&'a Node),
    Participant,
    Lane,
    Group,
    Label,
}
/// Whether `element` may be placed in `target`.
pub fn can_drop(element: Dropped, target: DropTarget) -> Result<(), &'static str> {
    use DropTarget as T;
    if matches!(element, Dropped::Label | Dropped::Group) {
        return Ok(());
    }
    if matches!(
        target,
        T::Participant {
            collapsed: true,
            ..
        }
    ) {
        return Err("Nothing can be placed in a collapsed pool");
    }
    let node = match element {
        Dropped::Participant => {
            return matches!(target, T::Process | T::Collaboration { .. })
                .then_some(())
                .ok_or("Pools belong on the collaboration");
        }
        Dropped::Lane => {
            return matches!(target, T::Participant { .. } | T::Lane)
                .then_some(())
                .ok_or("Lanes belong in a pool or lane");
        }
        Dropped::Node(node) => node,
        Dropped::Label | Dropped::Group => return Ok(()),
    };
    if node.kind == NodeKind::BoundaryEvent
        && !(interrupting(node)
            && definitions(node).iter().all(|d| {
                matches!(
                    d,
                    EventDefinition::Message { .. }
                        | EventDefinition::Timer { .. }
                        | EventDefinition::Signal { .. }
                        | EventDefinition::Conditional { .. }
                )
            }))
    {
        return Err("This boundary event must stay attached to an activity");
    }
    let artifact = matches!(
        node.kind,
        NodeKind::TextAnnotation | NodeKind::DataStoreReference
    );
    match target {
        T::Process | T::Lane | T::SubProcess { expanded: true } => Ok(()),
        T::Participant {
            has_process: true, ..
        } => Ok(()),
        T::Collaboration { has_process_pool } if artifact => {
            (node.kind == NodeKind::TextAnnotation || has_process_pool)
                .then_some(())
                .ok_or("A data store needs a pool with a process")
        }
        T::Participant { .. } if node.kind == NodeKind::TextAnnotation => Ok(()),
        T::SubProcess { expanded: false } => Err("Expand the subprocess before adding content"),
        _ => Err("Flow elements belong in a process, pool, lane, or expanded subprocess"),
    }
}

/// Whether `element` may attach to `host` as a boundary event; `on_border` reports whether the
/// drop point lies on the host's border band, `after_event_gateway` whether a receive-task host
/// follows an event-based gateway.
pub fn can_attach(
    element: &Node,
    host: &Node,
    on_border: bool,
    after_event_gateway: bool,
) -> Result<(), &'static str> {
    let candidate = match element.kind {
        NodeKind::BoundaryEvent => true,
        NodeKind::IntermediateThrowEvent => definitions(element).is_empty(),
        NodeKind::IntermediateCatchEvent => has_definition(element, |d| {
            matches!(
                d,
                EventDefinition::Message { .. }
                    | EventDefinition::Timer { .. }
                    | EventDefinition::Signal { .. }
                    | EventDefinition::Conditional { .. }
            )
        }),
        _ => false,
    };
    if !candidate {
        Err("Only boundary-capable intermediate events attach to activities")
    } else if !host.kind.is_activity() || is_event_subprocess(host) || is_for_compensation(host) {
        Err("Boundary events attach to activities only")
    } else if !on_border {
        Err("Drop the event on the activity's border")
    } else if host.kind == NodeKind::ReceiveTask && after_event_gateway {
        Err("A receive task after an event-based gateway cannot have boundary events")
    } else {
        Ok(())
    }
}

/// Minimum size of a resizable element; `east_west_only` limits resizing to the width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinSize {
    pub width: f64,
    pub height: f64,
    pub east_west_only: bool,
}
/// An element being resized.
#[derive(Clone, Copy, Debug)]
pub enum Resized {
    Node { kind: NodeKind, expanded: bool },
    Participant { horizontal: bool },
    Lane { horizontal: bool },
    Group,
}
/// `None` when the element keeps its default size (tasks, events, gateways, collapsed
/// subprocesses, data references).
pub fn can_resize(element: Resized) -> Option<MinSize> {
    let size = |width, height| MinSize {
        width,
        height,
        east_west_only: false,
    };
    match element {
        Resized::Node {
            kind: NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction,
            expanded: true,
        }
        | Resized::Group => Some(size(140.0, 120.0)),
        Resized::Node {
            kind: NodeKind::TextAnnotation,
            ..
        } => Some(MinSize {
            east_west_only: true,
            ..size(100.0, 40.0)
        }),
        Resized::Node { .. } => None,
        Resized::Lane { horizontal: true } => Some(size(300.0, 60.0)),
        Resized::Lane { horizontal: false } => Some(size(60.0, 300.0)),
        Resized::Participant { horizontal: true } => Some(size(300.0, 150.0)),
        Resized::Participant { horizontal: false } => Some(size(150.0, 300.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, kind: &str, details: serde_json::Value) -> serde_json::Value {
        let mut value = json!({"id": id, "name": null, "documentation": "", "kind": kind, "owner": "", "contact": ""});
        if !details.is_null() {
            value["details"] = details;
        }
        value
    }
    fn event(defs: serde_json::Value) -> serde_json::Value {
        json!({"type": "event", "definitions": defs, "attached_to": null, "cancel_activity": null})
    }
    fn boundary(host: &str, defs: serde_json::Value) -> serde_json::Value {
        json!({"type": "event", "definitions": defs, "attached_to": host, "cancel_activity": true})
    }
    fn message() -> serde_json::Value {
        json!([{"type": "message", "id": null, "message_ref": null, "operation_ref": null}])
    }
    fn timer() -> serde_json::Value {
        json!([{"type": "timer", "id": null, "kind": null, "expression": null}])
    }
    fn link() -> serde_json::Value {
        json!([{"type": "link", "id": null, "name": "L", "sources": [], "target": null}])
    }
    fn compensate() -> serde_json::Value {
        json!([{"type": "compensate", "id": null, "wait_for_completion": null, "activity_ref": null}])
    }
    fn process(id: &str, nodes: Vec<serde_json::Value>) -> serde_json::Value {
        json!({"id": id, "name": null, "documentation": "", "nodes": nodes, "lanes": [], "flows": []})
    }
    /// Two pools (`p1`, `p2`) and a nested subprocess in `p1`.
    fn document() -> BpmnDocument {
        let p1 = process(
            "p1",
            vec![
                node("start", "StartEvent", json!(null)),
                node("task", "Task", json!(null)),
                node("receive", "ReceiveTask", json!(null)),
                node("end", "EndEvent", json!(null)),
                node("xor", "ExclusiveGateway", json!(null)),
                node("ebg", "EventBasedGateway", json!(null)),
                node("timerCatch", "IntermediateCatchEvent", event(timer())),
                node("linkThrow", "IntermediateThrowEvent", event(link())),
                node("linkCatch", "IntermediateCatchEvent", event(link())),
                node("msgThrow", "IntermediateThrowEvent", event(message())),
                node("timerBoundary", "BoundaryEvent", boundary("task", timer())),
                node(
                    "compBoundary",
                    "BoundaryEvent",
                    boundary("task", compensate()),
                ),
                node("data", "DataObjectReference", json!(null)),
                node("store", "DataStoreReference", json!(null)),
                node("note", "TextAnnotation", json!(null)),
                node(
                    "sub",
                    "SubProcess",
                    json!({"type": "subProcess", "content": {"nodes": [
                        node("inner", "Task", json!(null)),
                        node("innerData", "DataObjectReference", json!(null)),
                    ], "lanes": [], "flows": []}}),
                ),
            ],
        );
        let p2 = process(
            "p2",
            vec![
                node("other", "Task", json!(null)),
                node("otherStart", "StartEvent", event(message())),
                node("otherTimerStart", "StartEvent", event(timer())),
            ],
        );
        serde_json::from_value(json!({
            "id": "11111111-1111-4111-8111-111111111111", "name": "Rules", "folder": null,
            "definitions_id": "defs", "processes": [p1, p2],
            "collaboration": {"id": "c", "participants": [
                {"id": "pool1", "name": null, "process": "p1"},
                {"id": "pool2", "name": null, "process": "p2"},
                {"id": "blackBox", "name": null, "process": null}
            ], "flows": []},
            "diagrams": [], "provenance": null, "capability": "Editable"
        }))
        .unwrap()
    }
    fn check(
        doc: &BpmnDocument,
        source: &str,
        target: &str,
    ) -> Result<ConnectionKind, &'static str> {
        can_connect(doc, &source.parse().unwrap(), &target.parse().unwrap())
    }

    #[test]
    fn connection_kinds_follow_the_bpmn_rule_table() {
        use ConnectionKind::*;
        let d = document();
        let allowed = [
            ("start", "task", Sequence),
            ("task", "end", Sequence),
            ("xor", "task", Sequence),
            ("ebg", "receive", Sequence),
            ("ebg", "timerCatch", Sequence),
            ("timerBoundary", "end", Sequence),
            ("task", "other", Message),
            ("msgThrow", "otherStart", Message),
            ("pool1", "pool2", Message),
            ("task", "blackBox", Message),
            ("data", "task", DataInput),
            ("store", "end", DataInput),
            ("task", "store", DataOutput),
            ("timerCatch", "data", DataOutput),
            ("data", "inner", DataInput),
            ("note", "task", Association),
            ("task", "note", Association),
            ("note", "data", Association),
        ];
        for (source, target, kind) in allowed {
            assert_eq!(check(&d, source, target), Ok(kind), "{source} -> {target}");
        }
        let refused = [
            ("end", "task", "End events"),
            ("task", "start", "Start events"),
            ("task", "timerBoundary", "Boundary events"),
            ("ebg", "task", "event-based gateway"),
            ("linkThrow", "task", "Link throw"),
            ("task", "linkCatch", "Link catch"),
            ("compBoundary", "end", "Compensation boundary"),
            ("task", "inner", "one process or subprocess"),
            ("task", "otherTimerStart", "Across pools"),
            ("timerBoundary", "other", "Across pools"),
            ("inner", "other", "subprocess"),
            ("pool1", "task", "message flow"),
            ("innerData", "task", "not visible"),
            ("note", "inner", "one process or subprocess"),
            ("data", "store", "cannot be connected"),
            ("task", "task", "itself"),
        ];
        for (source, target, reason) in refused {
            let result = check(&d, source, target);
            assert!(
                result.is_err_and(|r| r.contains(reason)),
                "{source} -> {target}: {result:?}"
            );
        }
        assert!(can_start_connection(&d, &"pool2".parse().unwrap()));
        assert!(!can_start_connection(&d, &"missing".parse().unwrap()));
    }

    #[test]
    fn drop_attach_and_resize_tables() {
        let d = document();
        let n = |id: &str| locate(&d, &id.parse().unwrap()).unwrap().node.unwrap();
        let task = Dropped::Node(n("task"));
        assert!(can_drop(task, DropTarget::Process).is_ok());
        assert!(can_drop(task, DropTarget::SubProcess { expanded: false }).is_err());
        assert!(
            can_drop(
                task,
                DropTarget::Participant {
                    collapsed: true,
                    has_process: true
                }
            )
            .is_err()
        );
        assert!(
            can_drop(
                task,
                DropTarget::Collaboration {
                    has_process_pool: true
                }
            )
            .is_err()
        );
        assert!(
            can_drop(
                Dropped::Node(n("note")),
                DropTarget::Collaboration {
                    has_process_pool: false
                }
            )
            .is_ok()
        );
        assert!(
            can_drop(
                Dropped::Node(n("store")),
                DropTarget::Collaboration {
                    has_process_pool: false
                }
            )
            .is_err()
        );
        assert!(can_drop(Dropped::Node(n("timerBoundary")), DropTarget::Process).is_ok());
        assert!(can_drop(Dropped::Node(n("compBoundary")), DropTarget::Process).is_err());
        assert!(can_drop(Dropped::Participant, DropTarget::Lane).is_err());
        assert!(can_drop(Dropped::Lane, DropTarget::Lane).is_ok());
        assert!(can_drop(Dropped::Label, DropTarget::SubProcess { expanded: false }).is_ok());

        assert!(can_attach(n("timerCatch"), n("task"), true, false).is_ok());
        assert!(can_attach(n("msgThrow"), n("task"), true, false).is_err());
        assert!(can_attach(n("timerCatch"), n("task"), false, false).is_err());
        assert!(can_attach(n("timerCatch"), n("xor"), true, false).is_err());
        assert!(can_attach(n("timerCatch"), n("receive"), true, true).is_err());

        let sub = Resized::Node {
            kind: NodeKind::SubProcess,
            expanded: true,
        };
        assert_eq!(
            can_resize(sub).map(|m| (m.width, m.height)),
            Some((140.0, 120.0))
        );
        assert!(
            can_resize(Resized::Node {
                kind: NodeKind::Task,
                expanded: false
            })
            .is_none()
        );
        assert!(
            can_resize(Resized::Node {
                kind: NodeKind::TextAnnotation,
                expanded: false
            })
            .is_some_and(|m| m.east_west_only && m.height == 40.0)
        );
        assert_eq!(
            can_resize(Resized::Participant { horizontal: true }).map(|m| (m.width, m.height)),
            Some((300.0, 150.0))
        );
        assert_eq!(
            can_resize(Resized::Lane { horizontal: true }).map(|m| m.height),
            Some(60.0)
        );
    }
}
