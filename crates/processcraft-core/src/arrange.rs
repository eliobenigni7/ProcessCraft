//! Align and distribute selected shapes (diagram-js AlignElements / DistributeElements
//! behavior, re-implemented). Results are ordinary prepared moves: one batch, one undo step.
use crate::{
    commands::ShapeBounds,
    geometry::{PreparedMove, prepare_move},
    ids::ElementSelector,
    model::{Bounds, BpmnDocument, NodeKind},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Horizontal,
    Vertical,
}

/// Elements whose ranges overlap by more than this are distributed as one group.
const DISTRIBUTE_THRESHOLD: f64 = 5.0;

/// JavaScript `Math.round`: halves round towards positive infinity.
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// (start, size) along the axis.
fn span(b: &Bounds, vertical: bool) -> (f64, f64) {
    if vertical {
        (b.y, b.height)
    } else {
        (b.x, b.width)
    }
}

fn with_start(b: &Bounds, vertical: bool, start: f64) -> Bounds {
    let mut b = b.clone();
    if vertical {
        b.y = start;
    } else {
        b.x = start;
    }
    b
}

/// New bounds for `items` (same order); only the alignment axis changes.
pub fn align_bounds(items: &[Bounds], alignment: Alignment) -> Vec<Bounds> {
    let vertical = matches!(
        alignment,
        Alignment::Top | Alignment::Middle | Alignment::Bottom
    );
    let spans: Vec<_> = items.iter().map(|b| span(b, vertical)).collect();
    if spans.is_empty() {
        return Vec::new();
    }
    let min_start = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let max_end = spans
        .iter()
        .map(|s| s.0 + s.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let starts: Vec<f64> = match alignment {
        Alignment::Left | Alignment::Top => spans.iter().map(|_| min_start).collect(),
        Alignment::Right | Alignment::Bottom => spans.iter().map(|s| max_end - s.1).collect(),
        Alignment::Center | Alignment::Middle => {
            // A center shared by several shapes wins; otherwise the middle of the whole range.
            let centers: Vec<f64> = spans.iter().map(|s| s.0 + round(s.1 / 2.0)).collect();
            let shared = centers
                .iter()
                .map(|c| (centers.iter().filter(|o| *o == c).count(), *c))
                .filter(|(count, _)| *count > 1)
                .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
                .map(|(_, c)| c);
            let first = spans
                .iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map_or(0.0, |s| s.0);
            let center = shared.unwrap_or_else(|| round((first + max_end) / 2.0));
            spans.iter().map(|s| center - round(s.1 / 2.0)).collect()
        }
    };
    items
        .iter()
        .zip(starts)
        .map(|(b, start)| with_start(b, vertical, start))
        .collect()
}

/// New bounds for `items` (same order), or `None` when there is nothing to distribute:
/// fewer than three range groups, or not enough room between the outer groups.
pub fn distribute_bounds(items: &[Bounds], distribution: Distribution) -> Option<Vec<Bounds>> {
    let vertical = distribution == Distribution::Vertical;
    let spans: Vec<_> = items.iter().map(|b| span(b, vertical)).collect();
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|a, b| spans[*a].0.total_cmp(&spans[*b].0));
    // Group shapes whose (threshold-shrunk) ranges overlap the group's first range.
    let mut groups: Vec<((f64, f64), Vec<usize>)> = Vec::new();
    for i in order {
        let (start, size) = spans[i];
        let range = (
            start + DISTRIBUTE_THRESHOLD,
            start + size - DISTRIBUTE_THRESHOLD,
        );
        match groups.last_mut() {
            Some(((min, max), members))
                if min.max(*max) >= range.0.min(range.1)
                    && min.min(*max) <= range.0.max(range.1) =>
            {
                members.push(i)
            }
            _ => groups.push((range, vec![i])),
        }
    }
    if groups.len() <= 2 {
        return None;
    }
    let mut starts: Vec<f64> = spans.iter().map(|s| s.0).collect();
    let last_group = groups.len() - 1;
    // Center the members of each group on its first shape (last shape for the last group).
    let ranges: Vec<(f64, f64)> = groups
        .iter()
        .enumerate()
        .map(|(index, (_, members))| {
            let reference = if index == last_group {
                members[members.len() - 1]
            } else {
                members[0]
            };
            let center = starts[reference] + spans[reference].1 / 2.0;
            if members.len() > 1 {
                for &m in members {
                    starts[m] = center - spans[m].1 / 2.0;
                }
            }
            let min = members
                .iter()
                .map(|&m| starts[m])
                .fold(f64::INFINITY, f64::min);
            let max = members
                .iter()
                .map(|&m| starts[m] + spans[m].1)
                .fold(f64::NEG_INFINITY, f64::max);
            (min, max)
        })
        .collect();
    let inner: f64 = ranges[1..last_group].iter().map(|(a, b)| b - a).sum();
    let space = ranges[last_group].0 - ranges[0].1;
    let margin = round((space - inner) / last_group as f64);
    if margin < last_group as f64 {
        return None;
    }
    let mut previous_end = ranges[0].1;
    for (index, (_, members)) in groups.iter().enumerate().take(last_group).skip(1) {
        let shift = previous_end + margin - ranges[index].0;
        for &m in members {
            starts[m] += shift;
        }
        previous_end = ranges[index].1 + shift;
    }
    Some(
        items
            .iter()
            .zip(starts)
            .map(|(b, start)| with_start(b, vertical, start))
            .collect(),
    )
}

/// Selected shapes that may be arranged: not lanes or boundary events (they follow their
/// container or host), not text annotations when distributing, and not shapes inside another
/// selected shape (moving the container already moves them).
fn arrangeable(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    shape_ids: &[ElementSelector],
    distributing: bool,
) -> Result<Vec<(ElementSelector, Bounds)>, &'static str> {
    let diagram = document
        .diagrams
        .iter()
        .find(|d| &d.id == diagram_id)
        .ok_or("diagram not found")?;
    let scopes = crate::queries::scopes(document);
    let excluded = |element: &ElementSelector| {
        scopes.iter().any(|scope| {
            scope.lanes.iter().any(|lane| &lane.id == element)
                || scope.nodes.iter().any(|node| {
                    &node.id == element
                        && (node.kind == NodeKind::BoundaryEvent
                            || (distributing && node.kind == NodeKind::TextAnnotation))
                })
        })
    };
    let candidates: Vec<_> = diagram
        .shapes
        .iter()
        .filter(|s| shape_ids.contains(&s.id.selector()) && !excluded(&s.element))
        .collect();
    let inside = |inner: &Bounds, outer: &Bounds| {
        outer.x <= inner.x
            && outer.y <= inner.y
            && outer.x + outer.width >= inner.x + inner.width
            && outer.y + outer.height >= inner.y + inner.height
    };
    Ok(candidates
        .iter()
        .filter(|s| {
            !candidates
                .iter()
                .any(|o| o.id != s.id && inside(&s.bounds, &o.bounds))
        })
        .map(|s| (s.id.selector(), s.bounds.clone()))
        .collect())
}

fn prepare(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    items: Vec<(ElementSelector, Bounds)>,
    moved: Vec<Bounds>,
) -> Result<PreparedMove, &'static str> {
    let bounds: Vec<_> = items
        .into_iter()
        .zip(moved)
        .filter(|((_, old), new)| old != new)
        .map(|((shape_id, _), bounds)| ShapeBounds {
            diagram_id: diagram_id.clone(),
            shape_id,
            bounds,
        })
        .collect();
    if bounds.is_empty() {
        return Err("shapes are already arranged");
    }
    prepare_move(document, diagram_id, bounds)
}

/// Align the selected shapes of a diagram (at least two after filtering).
pub fn prepare_align(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    shape_ids: &[ElementSelector],
    alignment: Alignment,
) -> Result<PreparedMove, &'static str> {
    let items = arrangeable(document, diagram_id, shape_ids, false)?;
    if items.len() < 2 {
        return Err("select at least two shapes to align");
    }
    let bounds: Vec<_> = items.iter().map(|(_, b)| b.clone()).collect();
    let moved = align_bounds(&bounds, alignment);
    prepare(document, diagram_id, items, moved)
}

/// Distribute the selected shapes of a diagram evenly (at least three after filtering).
pub fn prepare_distribute(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
    shape_ids: &[ElementSelector],
    distribution: Distribution,
) -> Result<PreparedMove, &'static str> {
    let items = arrangeable(document, diagram_id, shape_ids, true)?;
    if items.len() < 3 {
        return Err("select at least three shapes to distribute");
    }
    let bounds: Vec<_> = items.iter().map(|(_, b)| b.clone()).collect();
    let moved = distribute_bounds(&bounds, distribution)
        .ok_or("not enough room to distribute the selection")?;
    prepare(document, diagram_id, items, moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn align_edges_and_centers() {
        let items = [b(10.0, 0.0, 100.0, 80.0), b(50.0, 40.0, 36.0, 36.0)];
        let xs = |a| -> Vec<f64> { align_bounds(&items, a).iter().map(|b| b.x).collect() };
        let ys = |a| -> Vec<f64> { align_bounds(&items, a).iter().map(|b| b.y).collect() };
        assert_eq!(xs(Alignment::Left), [10.0, 10.0]);
        assert_eq!(xs(Alignment::Right), [10.0, 74.0]);
        // No shared center: middle of the whole range 10..110 is 60.
        assert_eq!(xs(Alignment::Center), [10.0, 42.0]);
        assert_eq!(ys(Alignment::Top), [0.0, 0.0]);
        assert_eq!(ys(Alignment::Bottom), [0.0, 44.0]);
        // A center shared by two shapes wins over the range middle.
        let shared = [
            b(0.0, 0.0, 100.0, 10.0),
            b(30.0, 0.0, 40.0, 10.0),
            b(400.0, 0.0, 20.0, 10.0),
        ];
        let centered = align_bounds(&shared, Alignment::Center);
        assert_eq!(
            centered.iter().map(|b| b.x).collect::<Vec<_>>(),
            [0.0, 30.0, 40.0]
        );
        // The other axis never changes.
        assert!(centered.iter().all(|b| b.y == 0.0));
    }

    #[test]
    fn distribute_spaces_groups_evenly_and_keeps_outer_shapes() {
        let items = [
            b(0.0, 0.0, 100.0, 80.0),
            b(130.0, 0.0, 50.0, 50.0),
            b(500.0, 0.0, 100.0, 80.0),
        ];
        let moved = distribute_bounds(&items, Distribution::Horizontal).unwrap();
        assert_eq!(moved[0].x, 0.0);
        assert_eq!(moved[2].x, 500.0);
        // Space 400 minus inner 50 over two gaps: margin 175.
        assert_eq!(moved[1].x, 275.0);
        // Overlapping ranges form one group: centered on its first shape and moved together.
        let grouped = [
            b(0.0, 0.0, 100.0, 80.0),
            b(200.0, 0.0, 40.0, 40.0),
            b(210.0, 100.0, 60.0, 40.0),
            b(600.0, 0.0, 100.0, 80.0),
        ];
        let moved = distribute_bounds(&grouped, Distribution::Horizontal).unwrap();
        assert_eq!(moved[1].x + 20.0, moved[2].x + 30.0);
        assert_eq!(moved[2].x - 100.0, 600.0 - (moved[2].x + 60.0));
        // Two groups, or no room, distribute nothing.
        assert!(distribute_bounds(&grouped[..2], Distribution::Horizontal).is_none());
        let crowded = [
            b(0.0, 0.0, 100.0, 10.0),
            b(100.0, 0.0, 100.0, 10.0),
            b(195.0, 0.0, 100.0, 10.0),
        ];
        assert!(distribute_bounds(&crowded, Distribution::Horizontal).is_none());
    }
}
