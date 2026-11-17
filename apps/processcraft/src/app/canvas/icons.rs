//! Canvas painters: context pad and replace icons, palette ghost, snap guides and drop feedback.
use super::*;
/// Snap guide lines across the canvas.
pub(super) fn paint_guides(
    painter: &egui::Painter,
    rect: Rect,
    camera: (f64, f64),
    zoom: f32,
    snap: &processcraft_core::geometry::Snap,
    stroke: egui::Stroke,
) {
    if let Some(x) = snap.guide_x {
        let x = rect.left() + (x - camera.0) as f32 * zoom;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
    }
    if let Some(y) = snap.guide_y {
        let y = rect.top() + (y - camera.1) as f32 * zoom;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
}

/// Highlight where `node` would go if dropped with its centre at `center`: the border of the
/// activity it would attach to, else the sequence flow it would be inserted into.
pub(super) fn drop_feedback(
    painter: &egui::Painter,
    d: &processcraft_core::model::BpmnDocument,
    diagram: &processcraft_core::model::Diagram,
    node: &processcraft_core::model::Node,
    center: &Point,
    rect_of: impl Fn(&Bounds) -> Rect,
    pos_of: impl Fn(&Point) -> Pos2,
) {
    use processcraft_core::manipulate;
    let stroke = egui::Stroke::new(3.0, CONNECT_OK);
    if let Some(host) = manipulate::attach_host(d, diagram, node, center)
        && let Some(shape) = diagram.shapes.iter().find(|s| s.element == host.id)
    {
        painter.rect_stroke(
            rect_of(&shape.bounds).expand(2.0),
            4.0,
            stroke,
            egui::StrokeKind::Outside,
        );
        return;
    }
    let scope = processcraft_core::rules::locate(d, &node.id.selector())
        .and_then(|l| l.scope)
        .or_else(|| manipulate::container_at(d, diagram, center, None).scope);
    if let Some(scope) = scope
        && let Some(flow) = manipulate::flow_under(d, diagram, node, &scope, center)
        && let Some(edge) = diagram.edges.iter().find(|e| e.element == flow.id)
    {
        painter.add(egui::Shape::line(
            edge.waypoints.iter().map(pos_of).collect(),
            stroke,
        ));
    }
}
pub(super) fn paint_pad_icon(painter: &egui::Painter, action: PadAction, r: Rect, color: Color32) {
    let stroke = egui::Stroke::new(1.5, color);
    let c = r.center();
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    match action {
        PadAction::Append(NodeKind::Task) => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::new(14.0, 10.0)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        PadAction::Append(NodeKind::ExclusiveGateway) => {
            painter.add(egui::Shape::closed_line(
                vec![at(0.0, -7.0), at(7.0, 0.0), at(0.0, 7.0), at(-7.0, 0.0)],
                stroke,
            ));
            painter.line_segment([at(-2.5, -2.5), at(2.5, 2.5)], stroke);
            painter.line_segment([at(-2.5, 2.5), at(2.5, -2.5)], stroke);
        }
        PadAction::Append(NodeKind::IntermediateThrowEvent) => {
            painter.circle_stroke(c, 6.5, stroke);
            painter.circle_stroke(c, 4.5, stroke);
        }
        PadAction::Append(NodeKind::EndEvent) => {
            painter.circle_stroke(c, 6.0, egui::Stroke::new(2.5, color));
        }
        PadAction::Append(_) => {
            painter.add(egui::Shape::line(
                vec![at(-2.0, -7.0), at(-6.0, -7.0), at(-6.0, 7.0), at(-2.0, 7.0)],
                stroke,
            ));
            painter.line_segment([at(-2.0, -2.0), at(6.0, -2.0)], stroke);
            painter.line_segment([at(-2.0, 2.0), at(4.0, 2.0)], stroke);
        }
        PadAction::Connect => painter.arrow(at(-6.0, 6.0), Vec2::new(11.0, -11.0), stroke),
        PadAction::Flow(variant) => paint_replace_icon(painter, &variant.target(), r, color),
        PadAction::Replace => {
            // Wrench: a handle and a ring-shaped head.
            painter.line_segment(
                [at(-6.0, 6.0), at(1.0, -1.0)],
                egui::Stroke::new(2.5, color),
            );
            painter.circle_stroke(at(3.5, -3.5), 3.5, stroke);
        }
        PadAction::Delete => {
            painter.rect_stroke(
                Rect::from_min_max(at(-5.0, -3.0), at(5.0, 7.0)),
                1.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([at(-7.0, -5.0), at(7.0, -5.0)], stroke);
            painter.line_segment([at(-2.0, -7.0), at(2.0, -7.0)], stroke);
        }
        // A pool outline with the new lane filled (above/below) or divided into lanes.
        PadAction::AddLane(_) | PadAction::Split(_) => {
            let body = Rect::from_center_size(c, Vec2::new(16.0, 12.0));
            painter.rect_stroke(body, 1.0, stroke, egui::StrokeKind::Middle);
            match action {
                PadAction::AddLane(location) => {
                    let y = if location == LaneLocation::Above {
                        body.top()
                    } else {
                        body.bottom() - 4.0
                    };
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(16.0, 4.0)),
                        0.0,
                        color,
                    );
                }
                PadAction::Split(n) => {
                    for i in 1..n {
                        let y = body.top() + body.height() * i as f32 / n as f32;
                        painter.line_segment(
                            [Pos2::new(body.left() + 4.0, y), Pos2::new(body.right(), y)],
                            stroke,
                        );
                    }
                    painter.line_segment(
                        [
                            Pos2::new(body.left() + 4.0, body.top()),
                            Pos2::new(body.left() + 4.0, body.bottom()),
                        ],
                        stroke,
                    );
                }
                _ => {}
            }
        }
    }
}

/// Small icon of a replace target: event circles with a trigger letter, gateway diamonds with
/// their marker, activity boxes with a type letter or subprocess marker, data shapes, flows.
pub(in crate::app) fn paint_replace_icon(
    painter: &egui::Painter,
    target: &processcraft_core::replace::ReplaceTarget,
    r: Rect,
    color: Color32,
) {
    use processcraft_core::{replace::ReplaceTarget, rules};
    let stroke = egui::Stroke::new(1.2, color);
    let c = r.center();
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    let letter = |text: &str, size: f32| {
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(size),
            color,
        );
    };
    let ReplaceTarget::Node {
        kind,
        trigger,
        non_interrupting,
        event_sub_process,
        collapsed,
    } = target
    else {
        painter.arrow(at(-7.0, 5.0), Vec2::new(14.0, -10.0), stroke);
        match target {
            ReplaceTarget::DefaultFlow => {
                painter.line_segment([at(-6.0, -1.0), at(-2.0, 5.0)], stroke);
            }
            ReplaceTarget::ConditionalFlow => {
                painter.add(egui::Shape::closed_line(
                    vec![at(-7.0, 5.0), at(-4.0, 1.0), at(-1.0, 3.0), at(-4.0, 7.0)],
                    stroke,
                ));
            }
            _ => {}
        }
        return;
    };
    if rules::is_event(*kind) {
        if *non_interrupting {
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    at(7.5 * a.cos(), 7.5 * a.sin())
                })
                .collect();
            painter.extend(egui::Shape::dashed_line(&points, stroke, 2.0, 2.0));
        } else {
            let width = if *kind == NodeKind::EndEvent {
                2.4
            } else {
                1.2
            };
            painter.circle_stroke(c, 7.5, egui::Stroke::new(width, color));
        }
        if matches!(
            kind,
            NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ) {
            painter.circle_stroke(c, 5.5, stroke);
        }
        if let Some(t) = trigger {
            use processcraft_core::replace::Trigger as T;
            let mark = match t {
                T::Message => "M",
                T::Timer => "T",
                T::Conditional => "C",
                T::Signal => "S",
                T::Error => "E",
                T::Escalation => "Es",
                T::Compensate => "Cp",
                T::Cancel => "X",
                T::Link => "L",
                T::Terminate => "Te",
            };
            letter(mark, 8.0);
        }
    } else if rules::is_gateway(*kind) {
        painter.add(egui::Shape::closed_line(
            vec![at(0.0, -8.0), at(8.0, 0.0), at(0.0, 8.0), at(-8.0, 0.0)],
            stroke,
        ));
        let cross = || {
            painter.line_segment([at(-2.5, -2.5), at(2.5, 2.5)], stroke);
            painter.line_segment([at(-2.5, 2.5), at(2.5, -2.5)], stroke);
        };
        let plus = || {
            painter.line_segment([at(0.0, -3.5), at(0.0, 3.5)], stroke);
            painter.line_segment([at(-3.5, 0.0), at(3.5, 0.0)], stroke);
        };
        match kind {
            NodeKind::ExclusiveGateway => cross(),
            NodeKind::ParallelGateway => plus(),
            NodeKind::ComplexGateway => {
                cross();
                plus();
            }
            NodeKind::InclusiveGateway => {
                painter.circle_stroke(c, 3.0, stroke);
            }
            _ => {
                painter.circle_stroke(c, 3.5, stroke);
                painter.circle_stroke(c, 2.0, stroke);
            }
        }
    } else if *kind == NodeKind::DataObjectReference {
        painter.add(egui::Shape::closed_line(
            vec![
                at(-5.0, -7.0),
                at(2.0, -7.0),
                at(5.0, -4.0),
                at(5.0, 7.0),
                at(-5.0, 7.0),
            ],
            stroke,
        ));
    } else if *kind == NodeKind::DataStoreReference {
        painter.rect_stroke(
            Rect::from_center_size(c, Vec2::new(14.0, 12.0)),
            3.0,
            stroke,
            egui::StrokeKind::Middle,
        );
        painter.line_segment([at(-7.0, -3.0), at(7.0, -3.0)], stroke);
    } else {
        let expanded = matches!(
            kind,
            NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
        ) && !collapsed;
        let outline = Rect::from_center_size(
            c,
            if expanded {
                Vec2::new(17.0, 13.0)
            } else {
                Vec2::new(15.0, 11.0)
            },
        );
        if *event_sub_process {
            let corners = [
                outline.left_top(),
                outline.right_top(),
                outline.right_bottom(),
                outline.left_bottom(),
                outline.left_top(),
            ];
            painter.extend(egui::Shape::dashed_line(&corners, stroke, 2.0, 1.5));
        } else {
            let width = if *kind == NodeKind::CallActivity {
                2.4
            } else {
                1.2
            };
            painter.rect_stroke(
                outline,
                2.0,
                egui::Stroke::new(width, color),
                egui::StrokeKind::Middle,
            );
        }
        if *kind == NodeKind::Transaction {
            painter.rect_stroke(outline.shrink(2.0), 1.0, stroke, egui::StrokeKind::Middle);
        }
        let marker = match kind {
            NodeKind::UserTask => "U",
            NodeKind::ServiceTask => "S",
            NodeKind::SendTask => "Sd",
            NodeKind::ReceiveTask => "Rc",
            NodeKind::ManualTask => "M",
            NodeKind::BusinessRuleTask => "B",
            NodeKind::ScriptTask => "Sc",
            NodeKind::AdHocSubProcess => "~",
            _ => "",
        };
        if !marker.is_empty() {
            letter(marker, 7.5);
        }
        if *collapsed {
            let plus = Rect::from_center_size(at(0.0, 3.5), Vec2::splat(4.0));
            painter.rect_stroke(plus, 0.0, stroke, egui::StrokeKind::Middle);
        }
    }
}

/// Drag-create ghost: the BPMN shape of `entry` filling `r` (event circle, gateway diamond,
/// rounded activity box, data shapes, annotation bracket) with its marker.
pub(super) fn paint_ghost(painter: &egui::Painter, entry: PaletteEntry, r: Rect, color: Color32) {
    use processcraft_core::{replace::ReplaceTarget, rules};
    let kind = entry.kind;
    let fill = color.gamma_multiply(0.15);
    let stroke = egui::Stroke::new(2.0, color);
    let c = r.center();
    let marker = |painter: &egui::Painter| {
        // The replace-menu icon carries the type and trigger marker; drawn at the centre.
        let target = ReplaceTarget::Node {
            kind,
            trigger: entry.trigger,
            non_interrupting: false,
            event_sub_process: false,
            collapsed: false,
        };
        paint_replace_icon(
            painter,
            &target,
            Rect::from_center_size(c, Vec2::splat(18.0)),
            color,
        );
    };
    if rules::is_event(kind) {
        let radius = r.width().min(r.height()) / 2.0;
        let width = if kind == NodeKind::EndEvent { 4.0 } else { 2.0 };
        painter.circle(c, radius, fill, egui::Stroke::new(width, color));
        if matches!(
            kind,
            NodeKind::IntermediateCatchEvent
                | NodeKind::IntermediateThrowEvent
                | NodeKind::BoundaryEvent
        ) {
            painter.circle_stroke(c, (radius - 3.0).max(1.0), egui::Stroke::new(1.0, color));
        }
        if entry.trigger.is_some() {
            marker(painter);
        }
    } else if rules::is_gateway(kind) {
        let (w, h) = (r.width() / 2.0, r.height() / 2.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + Vec2::new(0.0, -h),
                c + Vec2::new(w, 0.0),
                c + Vec2::new(0.0, h),
                c + Vec2::new(-w, 0.0),
            ],
            fill,
            stroke,
        ));
        let s = w.min(h) * 0.4;
        let cross = || {
            painter.line_segment([c + Vec2::new(-s, -s), c + Vec2::new(s, s)], stroke);
            painter.line_segment([c + Vec2::new(-s, s), c + Vec2::new(s, -s)], stroke);
        };
        let plus = || {
            painter.line_segment([c + Vec2::new(0.0, -s), c + Vec2::new(0.0, s)], stroke);
            painter.line_segment([c + Vec2::new(-s, 0.0), c + Vec2::new(s, 0.0)], stroke);
        };
        match kind {
            NodeKind::ExclusiveGateway => cross(),
            NodeKind::ParallelGateway => plus(),
            NodeKind::ComplexGateway => {
                cross();
                plus();
            }
            NodeKind::InclusiveGateway => {
                painter.circle_stroke(c, s, stroke);
            }
            _ => {
                painter.circle_stroke(c, s, egui::Stroke::new(1.0, color));
                painter.circle_stroke(c, s * 0.7, egui::Stroke::new(1.0, color));
            }
        }
    } else if kind == NodeKind::TextAnnotation {
        painter.rect_filled(r, 0.0, fill);
        painter.add(egui::Shape::line(
            vec![
                r.left_top() + Vec2::new(12.0, 0.0),
                r.left_top(),
                r.left_bottom(),
                r.left_bottom() + Vec2::new(12.0, 0.0),
            ],
            stroke,
        ));
    } else if matches!(
        kind,
        NodeKind::DataObjectReference | NodeKind::DataStoreReference
    ) {
        painter.rect(r, 2.0, fill, stroke, egui::StrokeKind::Inside);
        marker(painter);
    } else {
        painter.rect(r, 10.0, fill, stroke, egui::StrokeKind::Inside);
        marker(painter);
    }
}
