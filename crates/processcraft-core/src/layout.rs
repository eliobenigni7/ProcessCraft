//! Deterministic, explicitly requested BPMN geometry proposals.
use crate::{
    geometry::{valid_bounds, valid_point},
    ids::{DocumentId, XmlId},
    model::{Bounds, BpmnDocument, Capability, Diagram, FlowKind, NodeKind, Point},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeGeometry {
    pub id: XmlId,
    pub bounds: Bounds,
    pub label_bounds: Option<Bounds>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeGeometry {
    pub id: XmlId,
    pub waypoints: Vec<Point>,
    pub label_bounds: Option<Bounds>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutGeometry {
    pub shapes: Vec<ShapeGeometry>,
    pub edges: Vec<EdgeGeometry>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutProposal {
    pub session_id: String,
    pub generation: u64,
    pub document_id: DocumentId,
    pub diagram_id: XmlId,
    pub original: LayoutGeometry,
    pub proposed: LayoutGeometry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LayoutError {
    pub code: String,
    pub message: String,
}
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for LayoutError {}
fn err(code: &str, message: &str) -> LayoutError {
    LayoutError {
        code: code.into(),
        message: message.into(),
    }
}
fn cancelled(c: &AtomicBool) -> Result<(), LayoutError> {
    if c.load(Ordering::Relaxed) {
        Err(err("cancelled", "Layout was cancelled"))
    } else {
        Ok(())
    }
}
fn geometry(d: &Diagram) -> LayoutGeometry {
    let mut shapes: Vec<_> = d
        .shapes
        .iter()
        .map(|s| ShapeGeometry {
            id: s.id.clone(),
            bounds: s.bounds.clone(),
            label_bounds: s.label.as_ref().map(|l| l.bounds.clone()),
        })
        .collect();
    let mut edges: Vec<_> = d
        .edges
        .iter()
        .map(|e| EdgeGeometry {
            id: e.id.clone(),
            waypoints: e.waypoints.clone(),
            label_bounds: e.label.as_ref().map(|l| l.bounds.clone()),
        })
        .collect();
    shapes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    LayoutGeometry { shapes, edges }
}
fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}
fn valid_geometry(g: &LayoutGeometry) -> bool {
    let mut ids = BTreeSet::new();
    g.shapes.iter().all(|s| {
        ids.insert(&s.id)
            && valid_bounds(&s.bounds)
            && s.bounds.width > 0.0
            && s.bounds.height > 0.0
            && s.label_bounds.as_ref().is_none_or(valid_bounds)
    }) && {
        let mut ids = BTreeSet::new();
        g.edges.iter().all(|e| {
            ids.insert(&e.id)
                && (2..=16).contains(&e.waypoints.len())
                && e.waypoints.iter().all(valid_point)
                && e.label_bounds.as_ref().is_none_or(valid_bounds)
        })
    }
}
/// Check identity, exact base geometry and complete bounded output before atomic application.
pub fn validate_proposal(
    document: &BpmnDocument,
    proposal: &LayoutProposal,
) -> Result<(), LayoutError> {
    if !matches!(document.capability, Capability::Editable) || document.id != proposal.document_id {
        return Err(err(
            "identity",
            "Document is not the editable proposal target",
        ));
    }
    let diagram = document
        .diagrams
        .iter()
        .find(|d| d.id == proposal.diagram_id)
        .ok_or_else(|| err("identity", "Diagram not found"))?;
    let base = geometry(diagram);
    if !same(&base, &proposal.original) {
        return Err(err("stale", "Base geometry changed"));
    }
    if !valid_geometry(&proposal.original)
        || !valid_geometry(&proposal.proposed)
        || proposal.proposed.shapes.len() != base.shapes.len()
        || proposal.proposed.edges.len() != base.edges.len()
    {
        return Err(err("invalid_geometry", "Incomplete or invalid geometry"));
    }
    for (a, b) in base.shapes.iter().zip(&proposal.proposed.shapes) {
        if a.id != b.id || a.label_bounds.is_some() != b.label_bounds.is_some() {
            return Err(err("invalid_geometry", "Shape IDs or labels changed"));
        }
    }
    for (a, b) in base.edges.iter().zip(&proposal.proposed.edges) {
        if a.id != b.id || a.label_bounds.is_some() != b.label_bounds.is_some() {
            return Err(err("invalid_geometry", "Edge IDs or labels changed"));
        }
    }
    let cancelled = AtomicBool::new(false);
    let computed = propose_layout(
        document,
        &proposal.diagram_id,
        &proposal.session_id,
        proposal.generation,
        &cancelled,
    )?;
    if !same(&computed.proposed, &proposal.proposed) {
        return Err(err(
            "invalid_geometry",
            "Proposal differs from deterministic layout",
        ));
    }
    Ok(())
}

fn overlap(a: &Bounds, b: &Bounds) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
fn segment_hits(a: &Point, b: &Point, r: &Bounds) -> bool {
    if a.x == b.x {
        a.x > r.x && a.x < r.x + r.width && a.y.max(b.y) > r.y && a.y.min(b.y) < r.y + r.height
    } else if a.y == b.y {
        a.y > r.y && a.y < r.y + r.height && a.x.max(b.x) > r.x && a.x.min(b.x) < r.x + r.width
    } else {
        true
    }
}
/// Build a deterministic proposal without changing the document. Unsupported diagrams fail closed.
pub fn propose_layout(
    document: &BpmnDocument,
    diagram_id: &XmlId,
    session_id: &str,
    generation: u64,
    cancel: &AtomicBool,
) -> Result<LayoutProposal, LayoutError> {
    cancelled(cancel)?;
    if !matches!(document.capability, Capability::Editable) {
        return Err(err(
            "unsupported",
            "Document contains preserved source or extensions",
        ));
    }
    if document.processes.len() != 1 {
        return Err(err("unsupported", "Exactly one process is required"));
    }
    let process = &document.processes[0];
    if !process.data_objects.is_empty()
        || !process.data_associations.is_empty()
        || process
            .nodes
            .iter()
            .any(|node| node.io_specification.is_some())
    {
        return Err(err(
            "unsupported",
            "Data objects, associations and I/O specifications are outside the layout profile",
        ));
    }

    if process.lanes.len() > 64 || process.lanes.iter().any(|l| l.parent.is_some()) {
        return Err(err("unsupported", "Nested or too many lanes"));
    }
    if process.nodes.iter().any(|n| {
        n.details.is_some()
            || !matches!(
                n.kind,
                NodeKind::Task
                    | NodeKind::UserTask
                    | NodeKind::ManualTask
                    | NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::ExclusiveGateway
                    | NodeKind::ParallelGateway
            )
    }) || process.flows.iter().any(|f| f.kind != FlowKind::Sequence)
    {
        return Err(err("unsupported", "Unsupported node or flow"));
    }
    let pool = if let Some(c) = &document.collaboration {
        if c.participants.len() != 1
            || !c.flows.is_empty()
            || !c.conversation_nodes.is_empty()
            || !c.conversation_links.is_empty()
            || c.participants[0].process.as_ref() != Some(&process.id)
        {
            return Err(err("unsupported", "Multiple pools or collaboration flows"));
        }
        Some(&c.participants[0].id)
    } else {
        None
    };
    if document
        .diagrams
        .iter()
        .filter(|d| &d.id == diagram_id)
        .count()
        != 1
    {
        return Err(err(
            "invalid_di",
            "Diagram identity is missing or duplicated",
        ));
    }
    let d = document
        .diagrams
        .iter()
        .find(|d| &d.id == diagram_id)
        .ok_or_else(|| err("identity", "Diagram not found"))?;
    if d.shapes.len() > 400 || d.edges.len() > 500 || d.shapes.len() + d.edges.len() > 1000 {
        return Err(err("limit", "Diagram exceeds layout limits"));
    }
    if d.plane != process.id
        && pool.is_none_or(|id| d.plane != *id)
        && document
            .collaboration
            .as_ref()
            .is_none_or(|c| d.plane != c.id)
    {
        return Err(err(
            "unsupported",
            "Diagram plane does not identify process or pool",
        ));
    }
    let original = geometry(d);
    if !valid_geometry(&original) {
        return Err(err("invalid_geometry", "Malformed shape or edge geometry"));
    }
    let nodes: BTreeMap<_, _> = process.nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let flows: BTreeMap<_, _> = process.flows.iter().map(|f| (f.id.clone(), f)).collect();
    if nodes.len() != process.nodes.len() || flows.len() != process.flows.len() {
        return Err(err("invalid_di", "Duplicate semantic IDs"));
    }
    let mut shape_by_element = BTreeMap::new();
    for (index, s) in d.shapes.iter().enumerate() {
        if !nodes.contains_key(&s.element)
            && !process.lanes.iter().any(|l| l.id == s.element)
            && pool != Some(&s.element)
        {
            return Err(err("unsupported", "Unknown diagram shape"));
        }
        if shape_by_element.insert(s.element.clone(), index).is_some() {
            return Err(err("invalid_di", "Duplicate diagram shape mapping"));
        }
    }
    if nodes.keys().any(|id| !shape_by_element.contains_key(id))
        || process
            .lanes
            .iter()
            .any(|l| !shape_by_element.contains_key(&l.id))
        || pool.is_some_and(|id| !shape_by_element.contains_key(id))
    {
        return Err(err("invalid_di", "Missing shape mapping"));
    }
    let mut lane_ids = BTreeSet::new();
    if process.lanes.iter().any(|lane| {
        !lane_ids.insert(&lane.id) || nodes.contains_key(&lane.id) || pool == Some(&lane.id)
    }) {
        return Err(err("invalid_di", "Duplicate semantic lane identity"));
    }
    let mut edge_by_element = BTreeMap::new();
    for e in &d.edges {
        if !flows.contains_key(&e.element) || edge_by_element.insert(e.element.clone(), e).is_some()
        {
            return Err(err("invalid_di", "Unknown or duplicate edge mapping"));
        }
    }
    if flows.keys().any(|id| !edge_by_element.contains_key(id)) {
        return Err(err("invalid_di", "Missing edge mapping"));
    }
    let ids: Vec<_> = nodes.keys().cloned().collect();
    let indices: BTreeMap<_, _> = ids
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let n = ids.len();
    let mut graph = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for f in flows.values() {
        let (Some(&a), Some(&b)) = (indices.get(&f.source), indices.get(&f.target)) else {
            return Err(err("invalid_di", "Flow endpoint is missing"));
        };
        graph[a].push(b);
        reverse[b].push(a);
    }
    for list in &mut graph {
        list.sort_unstable();
        list.dedup();
    }
    for list in &mut reverse {
        list.sort_unstable();
        list.dedup();
    }
    // Iterative Kosaraju, with stable sorted traversal and no recursion on imported data.
    let mut seen = vec![false; n];
    let mut order = Vec::new();
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((v, i)) = stack.pop() {
            if i < graph[v].len() {
                stack.push((v, i + 1));
                let w = graph[v][i];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
            }
        }
    }
    cancelled(cancel)?;
    let mut component = vec![usize::MAX; n];
    let mut count = 0;
    for &root in order.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        component[root] = count;
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            for &w in &reverse[v] {
                if component[w] == usize::MAX {
                    component[w] = count;
                    stack.push(w);
                }
            }
        }
        count += 1;
    }
    let mut dag = vec![BTreeSet::new(); count];
    let mut indegree = vec![0; count];
    for v in 0..n {
        for &w in &graph[v] {
            let (a, b) = (component[v], component[w]);
            if a != b && dag[a].insert(b) {
                indegree[b] += 1;
            }
        }
    }
    let mut ready: BTreeSet<_> = (0..count).filter(|&i| indegree[i] == 0).collect();
    let mut rank = vec![0usize; count];
    while let Some(v) = ready.pop_first() {
        for &w in &dag[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                ready.insert(w);
            }
        }
    }
    cancelled(cancel)?;
    let mut membership = BTreeMap::new();
    for (i, lane) in process.lanes.iter().enumerate() {
        for member in &lane.members {
            if !nodes.contains_key(member) || membership.insert(member.clone(), i).is_some() {
                return Err(err("unsupported", "Invalid or overlapping lane membership"));
            }
        }
    }
    let bands = process.lanes.len() + 1;
    let mut cells: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        cells
            .entry((
                membership.get(id).copied().unwrap_or(bands - 1),
                rank[component[i]],
            ))
            .or_default()
            .push(i);
    }
    let max_width = d
        .shapes
        .iter()
        .filter(|s| nodes.contains_key(&s.element))
        .map(|s| s.bounds.width)
        .fold(0.0, f64::max);
    let mut band_heights = vec![120.0_f64; bands];
    for (&(band, _), cell) in &cells {
        let h = cell
            .iter()
            .map(|&i| d.shapes[shape_by_element[&ids[i]]].bounds.height + 80.0)
            .sum::<f64>()
            + 40.0;
        band_heights[band] = band_heights[band].max(h);
    }
    let mut band_y = vec![0.0; bands];
    for i in 1..bands {
        band_y[i] = band_y[i - 1] + band_heights[i - 1];
    }
    let mut result = original.clone();
    let index: BTreeMap<_, _> = result
        .shapes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    for (&(band, layer), cell) in &cells {
        let mut y = band_y[band] + 60.0;
        for &i in cell {
            let shape = &d.shapes[shape_by_element[&ids[i]]];
            let out = &mut result.shapes[index[&shape.id]];
            let x = 80.0 + layer as f64 * (max_width + 160.0);
            let dx = x - out.bounds.x;
            let dy = y - out.bounds.y;
            out.bounds.x = x;
            out.bounds.y = y;
            if let Some(label) = &mut out.label_bounds {
                label.x += dx;
                label.y += dy;
            }
            y += out.bounds.height + 80.0;
        }
    }
    let width = 160.0 + (rank.iter().copied().max().unwrap_or(0) + 1) as f64 * (max_width + 160.0);
    for (i, lane) in process.lanes.iter().enumerate() {
        let shape = &d.shapes[shape_by_element[&lane.id]];
        let out = &mut result.shapes[index[&shape.id]];
        let dx = -out.bounds.x;
        let dy = band_y[i] - out.bounds.y;
        out.bounds = Bounds {
            x: 0.0,
            y: band_y[i],
            width,
            height: band_heights[i],
        };
        if let Some(label) = &mut out.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    if let Some(pool) = pool {
        let shape = &d.shapes[shape_by_element[pool]];
        let out = &mut result.shapes[index[&shape.id]];
        let dx = -out.bounds.x;
        let dy = -out.bounds.y;
        out.bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width,
            height: band_heights.iter().sum(),
        };
        if let Some(label) = &mut out.label_bounds {
            label.x += dx;
            label.y += dy;
        }
    }
    cancelled(cancel)?;
    let edge_index: BTreeMap<_, _> = result
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();
    let mut outgoing = BTreeMap::<XmlId, Vec<XmlId>>::new();
    let mut incoming = BTreeMap::<XmlId, Vec<XmlId>>::new();
    for flow in flows.values() {
        outgoing
            .entry(flow.source.clone())
            .or_default()
            .push(flow.id.clone());
        incoming
            .entry(flow.target.clone())
            .or_default()
            .push(flow.id.clone());
    }
    let mut route_work = 0usize;
    const ROUTE_WORK_LIMIT: usize = 500_000;
    for (number, flow) in flows.values().enumerate() {
        cancelled(cancel)?;
        let source = &result.shapes[index[&d.shapes[shape_by_element[&flow.source]].id]].bounds;
        let target = &result.shapes[index[&d.shapes[shape_by_element[&flow.target]].id]].bounds;
        let port_y = |id: &XmlId, bounds: &Bounds, list: &[XmlId]| {
            if matches!(
                nodes[id].kind,
                NodeKind::Task | NodeKind::UserTask | NodeKind::ManualTask
            ) {
                let position = list.iter().position(|edge| edge == &flow.id).unwrap_or(0);
                bounds.y + bounds.height * (position + 1) as f64 / (list.len() + 1) as f64
            } else {
                bounds.y + bounds.height / 2.0
            }
        };
        let port = |id: &XmlId, bounds: &Bounds, list: &[XmlId], source: bool| {
            if matches!(
                nodes[id].kind,
                NodeKind::Task | NodeKind::UserTask | NodeKind::ManualTask
            ) {
                Point {
                    x: if source {
                        bounds.x + bounds.width
                    } else {
                        bounds.x
                    },
                    y: port_y(id, bounds, list),
                }
            } else {
                let position = list.iter().position(|edge| edge == &flow.id).unwrap_or(0);
                match (source, position % 4) {
                    (true, 0) | (false, 3) => Point {
                        x: bounds.x + bounds.width,
                        y: bounds.y + bounds.height / 2.0,
                    },
                    (false, 0) | (true, 3) => Point {
                        x: bounds.x,
                        y: bounds.y + bounds.height / 2.0,
                    },
                    (true, 2) | (false, 2) => Point {
                        x: bounds.x + bounds.width / 2.0,
                        y: bounds.y + bounds.height,
                    },
                    _ => Point {
                        x: bounds.x + bounds.width / 2.0,
                        y: bounds.y,
                    },
                }
            }
        };
        let start = port(&flow.source, source, &outgoing[&flow.source], true);
        let end = port(&flow.target, target, &incoming[&flow.target], false);
        let points = if flow.source == flow.target {
            let gap = 40.0 + number as f64 * 2.0;
            vec![
                start.clone(),
                Point {
                    x: start.x + gap,
                    y: start.y,
                },
                Point {
                    x: start.x + gap,
                    y: source.y - gap,
                },
                Point {
                    x: source.x - gap,
                    y: source.y - gap,
                },
                Point {
                    x: source.x - gap,
                    y: end.y,
                },
                end.clone(),
            ]
        } else if rank[component[indices[&flow.source]]] >= rank[component[indices[&flow.target]]]
            && flow.source > flow.target
        {
            let gap = 40.0 + number as f64 * 16.0;
            let channel_y = -gap;
            vec![
                start.clone(),
                Point {
                    x: start.x + gap,
                    y: start.y,
                },
                Point {
                    x: start.x + gap,
                    y: channel_y,
                },
                Point {
                    x: end.x - gap,
                    y: channel_y,
                },
                Point {
                    x: end.x - gap,
                    y: end.y,
                },
                end.clone(),
            ]
        } else {
            let mut obstacles: Vec<_> = ids
                .iter()
                .filter(|id| **id != flow.source && **id != flow.target)
                .map(|id| {
                    result.shapes[index[&d.shapes[shape_by_element[id]].id]]
                        .bounds
                        .clone()
                })
                .collect();
            // External labels are real obstacles too; routing around them avoids
            // constructing a proposal which the later collision check must reject.
            obstacles.extend(
                result
                    .shapes
                    .iter()
                    .filter_map(|shape| shape.label_bounds.clone()),
            );
            route_work = route_work.saturating_add(obstacles.len());
            if route_work > ROUTE_WORK_LIMIT {
                return Err(err("budget", "Routing work limit exceeded"));
            }
            let straight = start.y == end.y
                && start.x < end.x
                && obstacles.iter().all(|b| {
                    let inflated = Bounds {
                        x: b.x - 8.0,
                        y: b.y - 8.0,
                        width: b.width + 16.0,
                        height: b.height + 16.0,
                    };
                    valid_bounds(&inflated) && !segment_hits(&start, &end, &inflated)
                });
            if straight {
                vec![start, end]
            } else {
                route_work = route_work.saturating_add(4 + 4 * obstacles.len());
                if route_work > ROUTE_WORK_LIMIT {
                    return Err(err("budget", "Routing work limit exceeded"));
                }
                crate::geometry::route(start, end, &obstacles, 8.0, 1000)
                    .map_err(|_| err("routing", "Invalid route geometry"))?
                    .waypoints
                    .ok_or_else(|| err("routing", "No collision-free route within budget"))?
            }
        };
        let original_edge = edge_by_element[&flow.id];
        let out = &mut result.edges[edge_index[&original_edge.id]];
        out.waypoints = points;
        if let Some(label) = &mut out.label_bounds {
            let segment = out
                .waypoints
                .windows(2)
                .max_by(|a, b| {
                    let length = |pair: &[Point]| {
                        (pair[0].x - pair[1].x).abs() + (pair[0].y - pair[1].y).abs()
                    };
                    length(a).total_cmp(&length(b))
                })
                .ok_or_else(|| err("routing", "Edge has no label segment"))?;
            let a = &segment[0];
            let b = &segment[1];
            if a.y == b.y {
                label.x = (a.x + b.x - label.width) / 2.0;
                label.y = a.y - label.height - 8.0;
            } else {
                label.x = a.x + 8.0;
                label.y = (a.y + b.y - label.height) / 2.0;
            }
        }
    }
    let proposal = LayoutProposal {
        session_id: session_id.into(),
        generation,
        document_id: document.id,
        diagram_id: diagram_id.clone(),
        original,
        proposed: result,
    };
    if !valid_geometry(&proposal.proposed) {
        return Err(err("invalid_geometry", "Computed geometry exceeds bounds"));
    }
    let node_shapes: Vec<_> = ids
        .iter()
        .map(|id| &proposal.proposed.shapes[index[&d.shapes[shape_by_element[id]].id]])
        .collect();
    for (i, shape) in node_shapes.iter().enumerate() {
        if node_shapes
            .iter()
            .skip(i + 1)
            .any(|other| overlap(&shape.bounds, &other.bounds))
        {
            return Err(err("collision", "Node placement overlaps"));
        }
    }
    let labels: Vec<_> = proposal
        .proposed
        .shapes
        .iter()
        .filter_map(|shape| shape.label_bounds.as_ref())
        .chain(
            proposal
                .proposed
                .edges
                .iter()
                .filter_map(|edge| edge.label_bounds.as_ref()),
        )
        .collect();
    for (i, label) in labels.iter().enumerate() {
        if labels.iter().skip(i + 1).any(|other| overlap(label, other))
            || node_shapes
                .iter()
                .any(|shape| overlap(label, &shape.bounds))
        {
            return Err(err("collision", "Label overlaps another label or node"));
        }
    }
    for edge in &proposal.proposed.edges {
        for segment in edge.waypoints.windows(2) {
            if labels
                .iter()
                .any(|label| segment_hits(&segment[0], &segment[1], label))
            {
                return Err(err("collision", "Route crosses a label"));
            }
        }
    }
    for flow in flows.values() {
        let edge = &proposal.proposed.edges[edge_index[&edge_by_element[&flow.id].id]];
        for (segment_index, segment) in edge.waypoints.windows(2).enumerate() {
            let source = &proposal.proposed.shapes
                [index[&d.shapes[shape_by_element[&flow.source]].id]]
                .bounds;
            let target = &proposal.proposed.shapes
                [index[&d.shapes[shape_by_element[&flow.target]].id]]
                .bounds;
            if (segment_index > 0 && segment_hits(&segment[0], &segment[1], source))
                || (segment_index + 2 < edge.waypoints.len()
                    && segment_hits(&segment[0], &segment[1], target))
            {
                return Err(err("routing", "Route re-enters an endpoint node"));
            }
            if ids
                .iter()
                .filter(|id| **id != flow.source && **id != flow.target)
                .any(|id| {
                    segment_hits(
                        &segment[0],
                        &segment[1],
                        &proposal.proposed.shapes[index[&d.shapes[shape_by_element[id]].id]].bounds,
                    )
                })
            {
                return Err(err("routing", "Route crosses a node"));
            }
        }
    }
    for (index, edge) in proposal.proposed.edges.iter().enumerate() {
        for other in proposal.proposed.edges.iter().skip(index + 1) {
            for a in edge.waypoints.windows(2) {
                for b in other.waypoints.windows(2) {
                    route_work += 1;
                    if route_work > ROUTE_WORK_LIMIT {
                        return Err(err("budget", "Routing work limit exceeded"));
                    }
                    let shared = if a[0].x == a[1].x && b[0].x == b[1].x && a[0].x == b[0].x {
                        a[0].y.min(a[1].y).max(b[0].y.min(b[1].y))
                            < a[0].y.max(a[1].y).min(b[0].y.max(b[1].y))
                    } else if a[0].y == a[1].y && b[0].y == b[1].y && a[0].y == b[0].y {
                        a[0].x.min(a[1].x).max(b[0].x.min(b[1].x))
                            < a[0].x.max(a[1].x).min(b[0].x.max(b[1].x))
                    } else {
                        false
                    };
                    if shared {
                        return Err(err("routing", "Routes share a channel"));
                    }
                    let crosses = if a[0].x == a[1].x && b[0].y == b[1].y {
                        a[0].x > b[0].x.min(b[1].x)
                            && a[0].x < b[0].x.max(b[1].x)
                            && b[0].y > a[0].y.min(a[1].y)
                            && b[0].y < a[0].y.max(a[1].y)
                    } else if a[0].y == a[1].y && b[0].x == b[1].x {
                        b[0].x > a[0].x.min(a[1].x)
                            && b[0].x < a[0].x.max(a[1].x)
                            && a[0].y > b[0].y.min(b[1].y)
                            && a[0].y < b[0].y.max(b[1].y)
                    } else {
                        false
                    };
                    if crosses {
                        return Err(err("routing", "Routes cross"));
                    }
                }
            }
        }
    }
    Ok(proposal)
}
