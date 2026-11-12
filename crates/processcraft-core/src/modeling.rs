//! Compound modeling operations shared by the GUI, CLI, and MCP: bpmn-js default sizes,
//! auto-place, append, and connect. Builders return mutations that apply as one atomic batch
//! (one undo step); they refuse with the reason from [`crate::rules`].
use crate::{
    bpmn_activity::ModelReference,
    bpmn_data::{DataAssociation, DataAssociationKind, DataPort, IoSet},
    bpmn_model::{EventDefinition, EventIo, NodeDetails, TimerKind},
    commands::Mutation,
    geometry::{RouteLayout, Side, layout_route},
    ids::{ElementIdentity, ElementSelector},
    model::*,
    rules::{self, ConnectionKind, Located},
};

/// bpmn-js default size of a new element.
pub fn default_size(kind: NodeKind) -> (f64, f64) {
    match kind {
        _ if rules::is_event(kind) => (36.0, 36.0),
        _ if rules::is_gateway(kind) => (50.0, 50.0),
        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction => (350.0, 200.0),
        NodeKind::DataObjectReference => (36.0, 50.0),
        NodeKind::DataStoreReference => (50.0, 50.0),
        NodeKind::TextAnnotation => (100.0, 30.0),
        _ => (100.0, 80.0),
    }
}
/// Events, gateways, and data references carry their name in an external label.
pub fn has_external_label(kind: NodeKind) -> bool {
    rules::is_event(kind)
        || rules::is_gateway(kind)
        || matches!(
            kind,
            NodeKind::DataObjectReference | NodeKind::DataStoreReference
        )
}
/// External label: 90x20, centred below the shape with a 10 px gap.
pub fn external_label_bounds(shape: &Bounds) -> Bounds {
    Bounds {
        x: shape.x + shape.width / 2.0 - 45.0,
        y: shape.y + shape.height + 10.0,
        width: 90.0,
        height: 20.0,
    }
}
/// A new boundary event sits on the host's bottom edge at three quarters of its width.
pub fn boundary_bounds(host: &Bounds) -> Bounds {
    let (width, height) = default_size(NodeKind::BoundaryEvent);
    Bounds {
        x: host.x + host.width * 0.75 - width / 2.0,
        y: host.y + host.height - height / 2.0,
        width,
        height,
    }
}
/// A new node with the details its kind needs to be valid; boundary events attach to `host`.
pub fn new_node(
    kind: NodeKind,
    id: ElementIdentity,
    name: Option<String>,
    host: Option<ElementSelector>,
) -> Node {
    let details = match kind {
        NodeKind::TextAnnotation => Some(NodeDetails::TextAnnotation {
            text_format: None,
            text: Some(String::new().into()),
        }),
        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction => {
            Some(NodeDetails::SubProcess {
                content: Box::default(),
            })
        }
        NodeKind::DataStoreReference => Some(NodeDetails::DataStoreReference {
            data_store_ref: None,
        }),
        NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent => Some(NodeDetails::Event {
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
            definitions: vec![EventDefinition::Timer {
                base: None,
                id: None,
                kind: Some(TimerKind::Duration),
                expression: Some("PT1M".into()),
            }],
            attached_to: host
                .filter(|_| kind == NodeKind::BoundaryEvent)
                .map(ModelReference::Local),
            cancel_activity: (kind == NodeKind::BoundaryEvent).then_some(true),
        }),
        _ => None,
    };
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        activity: None,
        io_specification: None,
        loop_characteristics: None,
        details,
        id,
        name,
        documentation: String::new().into(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
/// The diagram shape of a new node, with an external label where the kind has one.
pub fn new_shape(
    kind: NodeKind,
    id: ElementIdentity,
    element: ElementSelector,
    bounds: Bounds,
) -> Shape {
    Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: Some(DiLabelVisibility::Explicit),
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: matches!(
            kind,
            NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
        )
        .then_some(true),
        id,
        element,
        label: Some(Label {
            bounds: has_external_label(kind).then(|| external_label_bounds(&bounds)),
            ..Default::default()
        }),
        bounds,
        style: None,
    }
}

fn endpoint(reference: &Option<ModelReference>) -> Option<&ElementSelector> {
    match reference {
        Some(ModelReference::Local(id)) => Some(id),
        _ => None,
    }
}
fn all_flows(doc: &BpmnDocument) -> Vec<&Flow> {
    crate::queries::scopes(doc)
        .into_iter()
        .flat_map(|scope| scope.flows)
        .chain(doc.collaborations().flat_map(|c| &c.flows))
        .collect()
}
pub(crate) fn shape_bounds<'a>(diagram: &'a Diagram, id: &ElementSelector) -> Option<&'a Bounds> {
    diagram
        .shapes
        .iter()
        .find(|s| s.element == *id)
        .map(|s| &s.bounds)
}
fn mid(b: &Bounds) -> (f64, f64) {
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}
fn centred(center: (f64, f64), (width, height): (f64, f64)) -> Bounds {
    Bounds {
        x: center.0 - width / 2.0,
        y: center.1 - height / 2.0,
        width,
        height,
    }
}
/// Boundary event side relative to its host (bpmn-js orientation with padding -25).
fn boundary_side(event: &Bounds, host: &Bounds) -> Option<Side> {
    if event.y + event.height - 25.0 <= host.y {
        Some(Side::Top)
    } else if event.y + 25.0 >= host.y + host.height {
        Some(Side::Bottom)
    } else if event.x + event.width - 25.0 <= host.x {
        Some(Side::Left)
    } else if event.x + 25.0 >= host.x + host.width {
        Some(Side::Right)
    } else {
        None
    }
}

/// bpmn-js auto-place: where a new `kind` element appended to `source` goes. Flow nodes go
/// right of the source at the most common existing sequence-flow gap (default 50), boundary
/// sources shift 80 up or down, annotations go above-right, data references below-right; a
/// spot taken by an element connected to the source (or its host/attachers) steps aside.
pub fn auto_place(
    doc: &BpmnDocument,
    diagram: &Diagram,
    source: &ElementSelector,
    kind: NodeKind,
) -> Option<Bounds> {
    let s = shape_bounds(diagram, source)?;
    let size = default_size(kind);
    let flows = all_flows(doc);
    let connected = |id: &ElementSelector| -> Vec<&Bounds> {
        flows
            .iter()
            .filter_map(|f| {
                let (from, to) = (endpoint(&f.source)?, endpoint(&f.target)?);
                if from == id {
                    Some(to)
                } else if to == id {
                    Some(from)
                } else {
                    None
                }
            })
            .filter_map(|other| shape_bounds(diagram, other))
            .collect()
    };
    let node = rules::locate(doc, source).and_then(|l| l.node);
    let host = node.and_then(rules::attached_to);
    let mut closure = connected(source);
    if let Some(host) = host {
        closure.extend(connected(host));
    }
    for scope in crate::queries::scopes(doc) {
        for attacher in scope
            .nodes
            .iter()
            .filter(|n| rules::attached_to(n) == Some(source))
        {
            closure.extend(connected(&attacher.id.selector()));
        }
    }
    // Step along `vertical` (else horizontal) until no closure element is within 10 px.
    let place = |mut center: (f64, f64), vertical: bool, margin: f64, min_distance: f64| {
        for _ in 0..64 {
            let candidate = centred(center, size);
            let Some(hit) = closure.iter().find(|r| {
                !(r.y + r.height + 10.0 <= candidate.y
                    || r.y - 10.0 >= candidate.y + candidate.height
                    || r.x + r.width + 10.0 <= candidate.x
                    || r.x - 10.0 >= candidate.x + candidate.width)
            }) else {
                break;
            };
            let (axis, start, length, extent) = if vertical {
                (&mut center.1, hit.y, hit.height, size.1)
            } else {
                (&mut center.0, hit.x, hit.width, size.0)
            };
            *axis = if margin < 0.0 {
                (start + margin - extent / 2.0).min(*axis - min_distance + margin)
            } else {
                (start + length + margin + extent / 2.0).max(*axis + min_distance + margin)
            };
        }
        centred(center, size)
    };
    let (left, right, top, bottom) = (s.x, s.x + s.width, s.y, s.y + s.height);
    // Vertical pools and lanes place south of the source (minimum distance 90, steps along x).
    let vertical = vertical_at(diagram, s);
    Some(match kind {
        NodeKind::TextAnnotation if vertical => place(
            (right + 50.0 + size.0 / 2.0, bottom + size.1 / 2.0),
            false,
            30.0,
            20.0,
        ),
        NodeKind::TextAnnotation => place(
            (right + size.0 / 2.0, top - 50.0 - size.1 / 2.0),
            true,
            -30.0,
            20.0,
        ),
        NodeKind::DataObjectReference | NodeKind::DataStoreReference if vertical => place(
            (left - 40.0 - size.0 / 2.0, bottom - 10.0 + size.1 / 2.0),
            true,
            30.0,
            30.0,
        ),
        NodeKind::DataObjectReference | NodeKind::DataStoreReference => place(
            (right - 10.0 + size.0 / 2.0, bottom + 40.0 + size.0 / 2.0),
            false,
            30.0,
            30.0,
        ),
        _ => {
            let side = host
                .and_then(|h| shape_bounds(diagram, h))
                .and_then(|h| boundary_side(s, h));
            let min_distance = if vertical { 90.0 } else { 80.0 };
            let (start, end) = if vertical {
                (Side::Left, Side::Right)
            } else {
                (Side::Top, Side::Bottom)
            };
            let (offset, margin) = match side {
                Some(side) if side == start => (-min_distance, -30.0),
                Some(side) if side == end => (min_distance, 30.0),
                _ => (0.0, 30.0),
            };
            let gap = learned_gap(&flows, diagram, source, s, vertical);
            let center = if vertical {
                (mid(s).0 + offset, bottom + gap + size.1 / 2.0)
            } else {
                (right + gap + size.0 / 2.0, mid(s).1 + offset)
            };
            place(center, !vertical, margin, min_distance)
        }
    })
}
/// Most common gap (0-250 px) from the source to its sequence-flow neighbours to the east
/// (south when `vertical`): outgoing targets count +1, incoming sources -1 (bpmn-js
/// `getConnectedDistance`).
fn learned_gap(
    flows: &[&Flow],
    diagram: &Diagram,
    source: &ElementSelector,
    s: &Bounds,
    vertical: bool,
) -> f64 {
    let span = |b: &Bounds| {
        if vertical {
            (b.y, b.y + b.height)
        } else {
            (b.x, b.x + b.width)
        }
    };
    let (s_start, s_end) = span(s);
    let mut seen = Vec::new();
    for f in flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
        let (Some(from), Some(to)) = (endpoint(&f.source), endpoint(&f.target)) else {
            continue;
        };
        let entry = if from == source {
            shape_bounds(diagram, to).map(|t| (to, span(t).0 - s_end, 1))
        } else if to == source {
            shape_bounds(diagram, from).map(|o| (from, s_start - span(o).1, -1))
        } else {
            None
        };
        if let Some(entry) = entry
            && !seen
                .iter()
                .any(|(id, _, w)| *id == entry.0 && *w == entry.2)
        {
            seen.push(entry);
        }
    }
    let mut totals: Vec<(f64, i32)> = Vec::new();
    let mut best: Option<f64> = None;
    for (_, distance, weight) in seen {
        if !(0.0..=250.0).contains(&distance) {
            continue;
        }
        let index = match totals.iter().position(|(d, _)| *d == distance) {
            Some(index) => index,
            None => {
                totals.push((distance, 0));
                totals.len() - 1
            }
        };
        totals[index].1 += weight;
        let score = |d: f64| totals.iter().find(|(v, _)| *v == d).map_or(0, |t| t.1);
        if best.is_none_or(|b| score(b) < totals[index].1) {
            best = Some(distance);
        }
    }
    best.unwrap_or(50.0)
}

pub(crate) fn route(
    diagram: &Diagram,
    kind: ConnectionKind,
    source: Option<&Node>,
    target_kind: Option<NodeKind>,
    a: &Bounds,
    b: &Bounds,
) -> Vec<Point> {
    route_from(diagram, kind, source, target_kind, a, b, Side::Bottom)
}
/// [`route`] where a self-loop leaves from `loop_side` (bpmn-js keeps a loop's first side).
pub(crate) fn route_from(
    diagram: &Diagram,
    kind: ConnectionKind,
    source: Option<&Node>,
    target_kind: Option<NodeKind>,
    a: &Bounds,
    b: &Bounds,
    loop_side: Side,
) -> Vec<Point> {
    let layout = match kind {
        _ if a == b => RouteLayout::Loop(loop_side),
        ConnectionKind::Message => RouteLayout::Message,
        ConnectionKind::Sequence | ConnectionKind::CompensationAssociation => {
            let side = source
                .and_then(rules::attached_to)
                .and_then(|host| shape_bounds(diagram, host))
                .and_then(|host| boundary_side(a, host));
            // bpmn-js lays out flows into or out of an expanded subprocess h:h.
            let expanded = |bounds: &Bounds| {
                diagram
                    .shapes
                    .iter()
                    .any(|s| s.bounds == *bounds && s.is_expanded == Some(true))
            };
            match (source.map(|n| n.kind), side) {
                (_, Some(side)) => RouteLayout::FromBoundary(side),
                _ if expanded(a) || expanded(b) => RouteLayout::Sequence,
                (Some(k), _) if rules::is_gateway(k) => RouteLayout::FromGateway,
                _ if target_kind.is_some_and(rules::is_gateway) => RouteLayout::IntoGateway,
                _ => RouteLayout::Sequence,
            }
        }
        _ => RouteLayout::Auto,
    };
    let shapes: Vec<Bounds> = diagram.shapes.iter().map(|s| s.bounds.clone()).collect();
    if vertical_at(diagram, a) {
        crate::geometry::layout_route_vertical(a, b, &shapes, layout)
    } else {
        layout_route(a, b, &shapes, layout)
    }
}
/// Whether the innermost pool or lane under `b`'s centre is vertical (bpmn-js
/// `isDirectionHorizontal`); only pools and lanes carry `isHorizontal`.
pub(crate) fn vertical_at(diagram: &Diagram, b: &Bounds) -> bool {
    let (x, y) = mid(b);
    diagram
        .shapes
        .iter()
        .filter(|s| {
            let c = &s.bounds;
            s.is_horizontal.is_some()
                && x >= c.x
                && x <= c.x + c.width
                && y >= c.y
                && y <= c.y + c.height
        })
        .min_by(|p, q| {
            (p.bounds.width * p.bounds.height).total_cmp(&(q.bounds.width * q.bounds.height))
        })
        .is_some_and(|s| s.is_horizontal == Some(false))
}
pub(crate) fn edge(id: ElementIdentity, element: ElementSelector, waypoints: Vec<Point>) -> Edge {
    Edge {
        di_metadata: None,
        message_visible_kind: None,
        label_visibility: Some(DiLabelVisibility::Explicit),
        source_element: None,
        target_element: None,
        id,
        element,
        waypoints,
        label: Some(Label::default()),
        style: None,
    }
}
pub(crate) fn flow(
    id: ElementIdentity,
    kind: FlowKind,
    source: &ElementSelector,
    target: &ElementSelector,
) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        base_extras: None,
        association_direction: None,
        id,
        kind,
        source: Some(ModelReference::Local(source.clone())),
        target: Some(ModelReference::Local(target.clone())),
        name: (kind != FlowKind::Association).then(String::new),
        documentation: String::new().into(),
        condition: None,
    }
}
pub(crate) fn find_diagram<'a>(
    doc: &'a BpmnDocument,
    id: &ElementSelector,
) -> Result<&'a Diagram, String> {
    doc.diagrams
        .iter()
        .find(|d| d.id == *id)
        .ok_or_else(|| "diagram not found".into())
}

/// IDs of the elements an append creates.
#[derive(Clone)]
pub struct AppendIds {
    pub node: ElementIdentity,
    pub shape: ElementIdentity,
    pub flow: ElementIdentity,
    pub edge: ElementIdentity,
}
/// Append a new `kind` node to `source`: node, shape (auto-placed, with label), and a sequence
/// flow (an association for a text annotation) with a routed edge, in the source's scope.
pub fn append(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    source: &ElementSelector,
    kind: NodeKind,
    name: Option<String>,
    ids: AppendIds,
) -> Result<Vec<Mutation>, String> {
    let diagram = find_diagram(doc, diagram_id)?;
    let located = rules::locate(doc, source).ok_or("Append starts from a flow element")?;
    let (Some(node), Some(scope)) = (located.node, located.scope) else {
        return Err("Append starts from a flow element, not a pool".into());
    };
    let source_bounds =
        shape_bounds(diagram, source).ok_or("The source is not shown in this diagram")?;
    let new = new_node(kind, ids.node.clone(), name, None);
    let connection = if kind == NodeKind::TextAnnotation {
        ConnectionKind::Association
    } else if rules::is_flow_node(kind) {
        if let Some(reason) = rules::sequence_refusal(node, &new) {
            return Err(reason.into());
        }
        ConnectionKind::Sequence
    } else {
        return Err("Only flow nodes and text annotations can be appended".into());
    };
    let bounds =
        auto_place(doc, diagram, source, kind).ok_or("The source is not shown in this diagram")?;
    let waypoints = route(
        diagram,
        connection,
        Some(node),
        Some(kind),
        source_bounds,
        &bounds,
    );
    let new_id = ids.node.selector();
    let flow_kind = if connection == ConnectionKind::Association {
        FlowKind::Association
    } else {
        FlowKind::Sequence
    };
    Ok(vec![
        Mutation::CreateElement {
            document_id: doc.id,
            process_id: scope.clone(),
            element: new,
        },
        Mutation::CreateShape {
            document_id: doc.id,
            diagram_id: diagram.id.selector(),
            shape: new_shape(kind, ids.shape, new_id.clone(), bounds),
        },
        Mutation::CreateFlow {
            document_id: doc.id,
            process_id: Some(scope),
            flow: flow(ids.flow.clone(), flow_kind, source, &new_id),
        },
        Mutation::CreateEdge {
            document_id: doc.id,
            diagram_id: diagram.id.selector(),
            edge: edge(ids.edge, ids.flow.selector(), waypoints),
        },
    ])
}

/// A port ID derived from the association ID, so one connect needs only flow and edge IDs.
fn port_id(association: &ElementIdentity) -> ElementIdentity {
    association
        .xml_id()
        .and_then(|id| format!("{id}_port").parse().ok())
        .unwrap_or_else(ElementIdentity::anonymous)
}
/// Mutations giving `owner` a new data port (input or output) with ID `port`.
fn add_port(
    doc: &BpmnDocument,
    owner: &Node,
    port: &ElementIdentity,
    input: bool,
) -> Result<Mutation, String> {
    let value = DataPort {
        base: None,
        id: Some(port.clone()),
        name: None,
        item_aware: None,
        is_collection: None,
    };
    if owner.kind.is_activity() {
        let mut io = owner.io_specification.clone().unwrap_or_default();
        let (ports, sets) = if input {
            (&mut io.inputs, &mut io.input_sets)
        } else {
            (&mut io.outputs, &mut io.output_sets)
        };
        ports.push(value);
        if sets.is_empty() {
            sets.push(IoSet::default());
        }
        sets[0].data_refs.push(port.selector());
        // The specification needs both sets, as when the inspector creates a port.
        if io.input_sets.is_empty() {
            io.input_sets.push(IoSet::default());
        }
        if io.output_sets.is_empty() {
            io.output_sets.push(IoSet::default());
        }
        return Ok(Mutation::SetNodeIoSpecification {
            document_id: doc.id,
            element_id: owner.id.selector(),
            io_specification: Some(io),
        });
    }
    let mut details = owner.details.clone().unwrap_or(NodeDetails::Event {
        definitions: Vec::new(),
        attached_to: None,
        cancel_activity: None,
        definition_refs: Vec::new(),
        parallel_multiple: None,
        is_interrupting: None,
        properties: Vec::new(),
        io: None,
    });
    let NodeDetails::Event { io, .. } = &mut details else {
        return Err("This event cannot hold data ports; edit it in the inspector".into());
    };
    let io = io.get_or_insert_with(|| {
        if input {
            EventIo::Throw {
                inputs: Vec::new(),
                input_set: None,
            }
        } else {
            EventIo::Catch {
                outputs: Vec::new(),
                output_set: None,
            }
        }
    });
    let (ports, set) = match io {
        EventIo::Throw { inputs, input_set } if input => (inputs, input_set),
        EventIo::Catch {
            outputs,
            output_set,
        } if !input => (outputs, output_set),
        _ => return Err("The event's data ports do not match this direction".into()),
    };
    ports.push(value);
    set.get_or_insert_with(IoSet::default)
        .data_refs
        .push(port.selector());
    Ok(Mutation::SetNodeDetails {
        document_id: doc.id,
        element_id: owner.id.selector(),
        details: Some(details),
    })
}

/// The collaboration `diagram` shows, else the document's first one.
pub(crate) fn diagram_collaboration(
    doc: &BpmnDocument,
    diagram: &Diagram,
) -> Result<Collaboration, String> {
    doc.collaborations()
        .find(|c| c.id == diagram.plane)
        .or_else(|| doc.collaborations().next())
        .cloned()
        .ok_or_else(|| "Message flows need pools; add a pool for each process first".into())
}
/// bpmn-js CompensateBoundaryEventBehavior for a new compensation association `event ->
/// activity`: the activity becomes `isForCompensation`, losing its sequence flows and boundary
/// events (no longer allowed), and the event's previous compensation association goes.
fn compensation_handler(
    doc: &BpmnDocument,
    event: &ElementSelector,
    activity: &ElementSelector,
) -> Result<Vec<Mutation>, String> {
    let target = crate::queries::find_node(doc, activity).ok_or("element not found")?;
    let mut batch: Vec<Mutation> = all_flows(doc)
        .into_iter()
        .filter(|f| {
            let (from, to) = (endpoint(&f.source), endpoint(&f.target));
            (f.kind == FlowKind::Sequence && (from == Some(activity) || to == Some(activity)))
                || (f.kind == FlowKind::Association
                    && from == Some(event)
                    && to
                        .and_then(|t| crate::queries::find_node(doc, t))
                        .is_some_and(rules::is_for_compensation))
        })
        .map(|f| f.id.selector())
        .chain(
            crate::queries::scopes(doc)
                .into_iter()
                .flat_map(|s| s.nodes)
                .filter(|n| rules::attached_to(n) == Some(activity))
                .map(|n| n.id.selector()),
        )
        .map(|element_id| Mutation::Delete {
            document_id: doc.id,
            element_id,
        })
        .collect();
    if !rules::is_for_compensation(target) {
        let mut activity_properties = target.activity.clone().unwrap_or_default();
        activity_properties.is_for_compensation = Some(true);
        batch.push(Mutation::SetActivityProperties {
            document_id: doc.id,
            element_id: activity.clone(),
            activity: Some(activity_properties),
        });
    }
    Ok(batch)
}

/// Connect `source` to `target` with the connection [`rules::can_connect`] picks: a sequence
/// flow or association in their scope, a message flow in the collaboration root, or a data
/// association bound to a new port of the activity/event. The edge is routed in `diagram_id`.
pub fn connect(
    doc: &BpmnDocument,
    diagram_id: &ElementSelector,
    source: &ElementSelector,
    target: &ElementSelector,
    flow_id: ElementIdentity,
    edge_id: ElementIdentity,
) -> Result<Vec<Mutation>, String> {
    let kind = rules::can_connect(doc, source, target)?;
    let diagram = find_diagram(doc, diagram_id)?;
    let (Some(a), Some(b)) = (shape_bounds(diagram, source), shape_bounds(diagram, target)) else {
        return Err("Both elements must be shown in this diagram".into());
    };
    let (Some(s), Some(t)) = (rules::locate(doc, source), rules::locate(doc, target)) else {
        return Err("element not found".into());
    };
    let waypoints = route(diagram, kind, s.node, t.node.map(|n| n.kind), a, b);
    let mut batch = Vec::new();
    match kind {
        ConnectionKind::Sequence => {
            batch.push(Mutation::CreateFlow {
                document_id: doc.id,
                process_id: s.scope.clone(),
                flow: flow(flow_id.clone(), FlowKind::Sequence, source, target),
            });
        }
        ConnectionKind::Association => {
            let value = flow(flow_id.clone(), FlowKind::Association, source, target);
            if let (Some(source_scope), Some(target_scope)) = (&s.scope, &t.scope) {
                // The association lives with the annotation; its other end may be in any scope.
                let annotation = s.node.is_some_and(|n| n.kind == NodeKind::TextAnnotation);
                batch.push(Mutation::CreateFlow {
                    document_id: doc.id,
                    process_id: Some(
                        if annotation {
                            source_scope
                        } else {
                            target_scope
                        }
                        .clone(),
                    ),
                    flow: value,
                });
            } else {
                // A pool or a collaboration annotation: a collaboration artifact.
                let mut root = diagram_collaboration(doc, diagram)?;
                root.extras
                    .get_or_insert_with(Default::default)
                    .artifacts
                    .push(
                        crate::bpmn_collaboration::CollaborationArtifact::Association(Box::new(
                            Flow {
                                name: None,
                                ..value
                            },
                        )),
                    );
                batch.push(Mutation::SetCollaborationRoot {
                    document_id: doc.id,
                    collaboration: root,
                });
            }
        }
        ConnectionKind::CompensationAssociation => {
            batch.extend(compensation_handler(doc, source, target)?);
            batch.push(Mutation::CreateFlow {
                document_id: doc.id,
                process_id: s.scope.clone(),
                flow: Flow {
                    association_direction: Some(crate::bpmn_model::AssociationDirection::One),
                    ..flow(flow_id.clone(), FlowKind::Association, source, target)
                },
            });
        }
        ConnectionKind::Message => {
            let mut root = diagram_collaboration(doc, diagram)?;
            root.flows
                .push(flow(flow_id.clone(), FlowKind::Message, source, target));
            batch.push(Mutation::SetCollaborationRoot {
                document_id: doc.id,
                collaboration: root,
            });
        }
        ConnectionKind::DataInput | ConnectionKind::DataOutput => {
            let input = kind == ConnectionKind::DataInput;
            let (data, owner): (&Located, &Located) = if input { (&s, &t) } else { (&t, &s) };
            let (Some(owner_node), Some(scope)) = (owner.node, owner.scope.clone()) else {
                return Err("element not found".into());
            };
            let port = port_id(&flow_id);
            batch.push(add_port(doc, owner_node, &port, input)?);
            let reference = data
                .node
                .map(|n| n.id.selector())
                .ok_or("element not found")?;
            batch.push(Mutation::CreateDataAssociation {
                document_id: doc.id,
                scope_id: scope,
                association: DataAssociation {
                    base: None,
                    id: Some(flow_id.clone()),
                    owner: owner_node.id.selector(),
                    kind: if input {
                        DataAssociationKind::Input
                    } else {
                        DataAssociationKind::Output
                    },
                    sources: vec![if input {
                        reference.clone()
                    } else {
                        port.selector()
                    }],
                    target: if input { port.selector() } else { reference },
                    transformation: None,
                    assignments: Vec::new(),
                },
            });
        }
    }
    batch.push(Mutation::CreateEdge {
        document_id: doc.id,
        diagram_id: diagram.id.selector(),
        edge: edge(edge_id, flow_id.selector(), waypoints),
    });
    Ok(batch)
}
