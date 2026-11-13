//! Floating capsules over the canvas (docs/plans/ux-shell.md): the element palette on the left,
//! zoom at the bottom right and, with two or more shapes selected, Align/Distribute at the top.

use super::*;
use crate::theme::{RADIUS, RADIUS_FLOAT};
use crate::widgets::{self, Icon};
use egui::Sense;
use processcraft_core::replace::ReplaceTarget;

const TOOL: f32 = 34.0;
const INSET: f32 = 12.0;
/// Canvas width the palette capsule covers, left out when fitting the diagram.
pub(super) const PALETTE_INSET: f32 = INSET + TOOL + 8.0 + 8.0;

/// A palette tool: what it adds, or a mode it toggles.
#[derive(Clone, Copy)]
enum Tool {
    Element(canvas::PaletteEntry),
    Pool,
    Lane,
    Space,
}

const PALETTE: &[&[(&str, Tool)]] = &[
    &[("Space tool (S)", Tool::Space)],
    &[
        ("Start event", node(NodeKind::StartEvent)),
        (
            "Intermediate throw event",
            node(NodeKind::IntermediateThrowEvent),
        ),
        ("End event", node(NodeKind::EndEvent)),
    ],
    &[
        ("Exclusive gateway", node(NodeKind::ExclusiveGateway)),
        ("Parallel gateway", node(NodeKind::ParallelGateway)),
    ],
    &[
        ("Task", node(NodeKind::Task)),
        ("User task", node(NodeKind::UserTask)),
        ("Subprocess", node(NodeKind::SubProcess)),
    ],
    &[("Pool", Tool::Pool), ("Lane", Tool::Lane)],
];

const fn node(kind: NodeKind) -> Tool {
    Tool::Element(canvas::PaletteEntry {
        kind,
        trigger: None,
    })
}

const CHOREOGRAPHY_ELEMENTS: [&str; 14] = [
    "Task",
    "Sub-choreography",
    "Call choreography",
    "Start event",
    "End event",
    "Exclusive gateway",
    "Parallel gateway",
    "Sequence flow",
    "Intermediate catch event",
    "Intermediate throw event",
    "Boundary event",
    "Inclusive gateway",
    "Complex gateway",
    "Event-based gateway",
];

pub(super) fn capsule_frame(theme: Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.palette().base)
        .stroke(theme.float_stroke())
        .corner_radius(RADIUS_FLOAT)
        .shadow(theme.float_shadow())
        .inner_margin(4)
}

/// Paints the icon of a palette entry in a square.
pub(super) fn paint_entry_icon(
    painter: &egui::Painter,
    entry: canvas::PaletteEntry,
    rect: Rect,
    color: Color32,
) {
    canvas::paint_replace_icon(
        painter,
        &ReplaceTarget::Node {
            kind: entry.kind,
            trigger: entry.trigger,
            non_interrupting: false,
            event_sub_process: false,
            collapsed: entry.kind == NodeKind::SubProcess,
        },
        rect,
        color,
    );
}

fn paint_tool(painter: &egui::Painter, tool: Tool, rect: Rect, color: Color32) {
    let stroke = egui::Stroke::new(1.3, color);
    let c = rect.center();
    match tool {
        Tool::Element(entry) => paint_entry_icon(painter, entry, rect, color),
        Tool::Pool | Tool::Lane => {
            let body = Rect::from_center_size(c, Vec2::new(18.0, 13.0));
            painter.rect_stroke(body, 2, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    body.left_top() + Vec2::new(4.0, 0.0),
                    body.left_bottom() + Vec2::new(4.0, 0.0),
                ],
                stroke,
            );
            if matches!(tool, Tool::Lane) {
                painter.line_segment(
                    [
                        Pos2::new(body.left() + 4.0, c.y),
                        Pos2::new(body.right(), c.y),
                    ],
                    stroke,
                );
            }
        }
        Tool::Space => {
            painter.line_segment([c - Vec2::new(0.0, 8.0), c + Vec2::new(0.0, 8.0)], stroke);
            painter.arrow(c + Vec2::new(2.0, 0.0), Vec2::new(7.0, 0.0), stroke);
            painter.arrow(c - Vec2::new(2.0, 0.0), Vec2::new(-7.0, 0.0), stroke);
        }
    }
}

/// A square tool button that can be clicked or dragged onto the canvas.
fn tool_button(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    paint: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(TOOL), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    let p = crate::theme::current(ui);
    let fill = if selected {
        p.accent_soft
    } else if response.is_pointer_button_down_on() {
        ui.visuals().widgets.active.bg_fill
    } else if response.hovered() || response.has_focus() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, RADIUS + 2, fill);
    let color = if !ui.is_enabled() {
        p.faint
    } else if selected {
        p.accent_ink
    } else {
        p.text
    };
    paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            RADIUS + 2,
            egui::Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(label)
}

fn divider(ui: &mut egui::Ui, vertical: bool) {
    let p = crate::theme::current(ui);
    let size = if vertical {
        Vec2::new(TOOL, 9.0)
    } else {
        Vec2::new(9.0, TOOL - 8.0)
    };
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let (a, b) = if vertical {
        (
            rect.left_center() + Vec2::new(8.0, 0.0),
            rect.right_center() - Vec2::new(8.0, 0.0),
        )
    } else {
        (rect.center_top(), rect.center_bottom())
    };
    ui.painter()
        .line_segment([a, b], egui::Stroke::new(1.0, p.border));
}

impl ProcessCraft {
    /// Draws the floating capsules over `canvas` and returns whether Fit was clicked.
    pub(super) fn canvas_overlays(&mut self, ctx: &egui::Context, canvas: Rect) -> bool {
        let mut fit = false;
        let snap = self.ctl.session.snapshot();
        let Some(document) = snap.project.documents.get(self.doc) else {
            return false;
        };
        let editable = document.capability.allows_structural_editing();
        let theme = self.theme;
        egui::Area::new(egui::Id::unique("processcraft-palette-capsule"))
            .order(egui::Order::Middle)
            .fixed_pos(canvas.left_top() + Vec2::splat(INSET))
            .show(ctx, |ui| {
                capsule_frame(theme).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                    ui.set_width(TOOL);
                    ui.add_enabled_ui(editable, |ui| {
                        self.palette_tools(ui, document, snap.generation);
                    });
                });
            });
        egui::Area::new(egui::Id::unique("processcraft-zoom-capsule"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(canvas.right_bottom() - Vec2::splat(INSET))
            .show(ctx, |ui| {
                capsule_frame(theme).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                    ui.horizontal(|ui| {
                        if widgets::icon_button_sized(
                            ui,
                            Icon::ZoomOut,
                            "Zoom out (Ctrl+Minus)",
                            false,
                            30.0,
                        )
                        .clicked()
                        {
                            self.zoom_canvas(self.zoom / canvas::ZOOM_STEP, None);
                        }
                        let percent = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{:.0}%", self.zoom * 100.0))
                                        .font(crate::theme::medium(12.0)),
                                )
                                .frame(false)
                                .min_size(Vec2::new(46.0, 30.0)),
                            )
                            .on_hover_text("Reset to 100% (Ctrl+0)");
                        if percent.clicked() {
                            self.zoom_canvas(1.0, None);
                        }
                        if widgets::icon_button_sized(
                            ui,
                            Icon::ZoomIn,
                            "Zoom in (Ctrl+Plus)",
                            false,
                            30.0,
                        )
                        .clicked()
                        {
                            self.zoom_canvas(self.zoom * canvas::ZOOM_STEP, None);
                        }
                        divider(ui, false);
                        fit = widgets::icon_button_sized(
                            ui,
                            Icon::Maximize,
                            "Fit diagram",
                            false,
                            30.0,
                        )
                        .clicked();
                    });
                });
            });
        if self.multi.len() >= 2 {
            egui::Area::new(egui::Id::unique("processcraft-arrange-capsule"))
                .order(egui::Order::Middle)
                .pivot(egui::Align2::CENTER_TOP)
                .fixed_pos(canvas.center_top() + Vec2::new(0.0, INSET))
                .show(ctx, |ui| {
                    capsule_frame(theme).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{} selected", self.multi.len()))
                                    .color(theme.palette().muted),
                            );
                            ui.add_enabled_ui(editable, |ui| self.arrange_menus(ui));
                        });
                    });
                });
        }
        fit
    }

    fn palette_tools(&mut self, ui: &mut egui::Ui, document: &BpmnDocument, generation: u64) {
        let choreography = document
            .diagrams
            .iter()
            .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id))
            .and_then(|diagram| {
                self.choreography_scope_infos(document, generation)
                    .iter()
                    .find(|scope| scope.owner == diagram.plane)
                    .map(|scope| (scope.owner.clone(), scope.graph))
            });
        if let Some((owner, graph)) = choreography {
            ui.add_enabled_ui(graph, |ui| {
                let more = tool_button(ui, "Add choreography element", false, |painter, r, c| {
                    paint_entry_icon(painter, node(NodeKind::Task).entry(), r, c)
                });
                let more = if graph {
                    more
                } else {
                    more.on_disabled_hover_text(
                        "Convert the legacy choreography to a graph from the Diagram menu first.",
                    )
                };
                egui::Popup::menu(&more).show(|ui| {
                    for kind in CHOREOGRAPHY_ELEMENTS {
                        if ui.button(kind).clicked()
                            && let Some(scope) =
                                processcraft_core::queries::choreography_scopes(document)
                                    .into_iter()
                                    .find(|scope| scope.owner == owner)
                        {
                            self.add_choreography_element(document, &scope, kind, generation);
                            ui.close();
                        }
                    }
                });
            });
            return;
        }
        for (index, group) in PALETTE.iter().enumerate() {
            if index > 0 {
                divider(ui, true);
            }
            for (label, tool) in *group {
                let selected = matches!(tool, Tool::Space) && self.canvas.space_tool;
                let response = tool_button(ui, label, selected, |painter, r, c| {
                    paint_tool(painter, *tool, r, c)
                });
                #[cfg(test)]
                if matches!(tool, Tool::Element(e) if e.kind == NodeKind::Task) {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique("test_task_palette_rect"), response.rect)
                    });
                }
                match tool {
                    Tool::Element(entry) if response.clicked() => self.add_entry(*entry),
                    Tool::Element(entry) if response.drag_started() => {
                        self.canvas.palette_drag = Some(*entry);
                    }
                    Tool::Pool if response.clicked() => self.add_pool(),
                    Tool::Lane if response.clicked() => self.add_lane(),
                    Tool::Space if response.clicked() => {
                        self.canvas.space_tool = !self.canvas.space_tool;
                    }
                    _ => {}
                }
            }
        }
        divider(ui, true);
        let more = tool_button(ui, "More elements", false, |painter, r, c| {
            for dx in [-5.0, 0.0, 5.0] {
                painter.circle_filled(r.center() + Vec2::new(dx, 0.0), 1.6, c);
            }
        });
        egui::Popup::menu(&more)
            .align(egui::RectAlign::RIGHT_START)
            .show(|ui| {
                ui.set_min_width(240.0);
                for (label, entry) in std::iter::once(("Manual task", NodeKind::ManualTask.into()))
                    .chain(super::shell::more_palette_entries())
                {
                    let (row, item) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 26.0),
                        Sense::click_and_drag(),
                    );
                    item.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label)
                    });
                    let p = crate::theme::current(ui);
                    if item.hovered() {
                        ui.painter()
                            .rect_filled(row, RADIUS, ui.visuals().widgets.hovered.bg_fill);
                    }
                    paint_entry_icon(
                        ui.painter(),
                        entry,
                        Rect::from_center_size(
                            row.left_center() + Vec2::new(14.0, 0.0),
                            Vec2::splat(18.0),
                        ),
                        p.text,
                    );
                    ui.painter().text(
                        row.left_center() + Vec2::new(30.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        crate::theme::regular(13.0),
                        p.text,
                    );
                    let item = item.on_hover_text("Click to add, or drag onto the canvas");
                    if item.clicked() {
                        self.add_entry(entry);
                        ui.close();
                    } else if item.drag_started() {
                        self.canvas.palette_drag = Some(entry);
                        ui.close();
                    }
                }
            });
    }
}

impl Tool {
    fn entry(self) -> canvas::PaletteEntry {
        match self {
            Tool::Element(entry) => entry,
            _ => NodeKind::Task.into(),
        }
    }
}
