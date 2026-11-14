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
                            "Fit diagram (Shift+1)",
                            false,
                            30.0,
                        )
                        .clicked();
                    });
                });
            });
        if !snap.project.attribute_definitions.is_empty() || self.canvas.overlay.is_some() {
            egui::Area::new(egui::Id::unique("processcraft-overlay-capsule"))
                .order(egui::Order::Middle)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(canvas.right_bottom() - Vec2::new(INSET, INSET + TOOL + 14.0))
                .show(ctx, |ui| {
                    capsule_frame(theme).show(ui, |ui| {
                        self.overlay_capsule(ui, &snap.project, document.id, snap.generation);
                    });
                });
        }
        // Empty diagram: one obvious first step under the "Build your process" message.
        let empty = document
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| d.id == *id))
            .is_some_and(|d| d.shapes.is_empty() && d.edges.is_empty());
        if empty && editable {
            egui::Area::new(egui::Id::unique("processcraft-empty-canvas"))
                .order(egui::Order::Middle)
                .pivot(egui::Align2::CENTER_CENTER)
                .fixed_pos(canvas.center() + Vec2::new(0.0, 16.0))
                .show(ctx, |ui| {
                    if widgets::primary_button(ui, "Add start event").clicked() {
                        self.add_entry(NodeKind::StartEvent.into());
                    }
                });
        }
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

    /// The Overlay capsule: pick an attribute and mode, see the legend. View state only.
    fn overlay_capsule(
        &mut self,
        ui: &mut egui::Ui,
        project: &ProjectData,
        document_id: processcraft_core::ids::DocumentId,
        generation: u64,
    ) {
        use processcraft_core::overlay::{OverlayMode, OverlaySpec};
        let theme = self.theme;
        let active = self.canvas.overlay;
        let mut definitions: Vec<_> = project
            .attribute_definitions
            .iter()
            .map(|d| (d.id, d.name.clone()))
            .collect();
        definitions.sort_by(|a, b| a.1.cmp(&b.1));
        let title = active
            .and_then(|spec| definitions.iter().find(|d| d.0 == spec.definition))
            .map_or_else(|| "Overlay".to_owned(), |d| format!("Overlay: {}", d.1));
        let button = ui.add(
            egui::Button::new(egui::RichText::new(title).font(crate::theme::medium(12.0)))
                .selected(active.is_some())
                .min_size(Vec2::new(0.0, 28.0)),
        );
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(200.0);
            if ui.selectable_label(active.is_none(), "Off").clicked() {
                self.canvas.overlay = None;
                self.canvas.export_overlay = false;
                ui.close();
            }
            ui.separator();
            let mut badges = self.canvas.overlay_badges;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut badges, false, "Heatmap");
                ui.selectable_value(&mut badges, true, "Badge");
            });
            if badges != self.canvas.overlay_badges {
                self.canvas.overlay_badges = badges;
                if let Some(spec) = &mut self.canvas.overlay {
                    spec.mode = overlay_mode(badges);
                }
            }
            ui.separator();
            if definitions.is_empty() {
                ui.label(egui::RichText::new("No attributes defined").color(theme.palette().muted));
            }
            for (id, name) in &definitions {
                if ui
                    .selectable_label(active.is_some_and(|spec| spec.definition == *id), name)
                    .clicked()
                {
                    self.canvas.overlay = Some(OverlaySpec {
                        definition: *id,
                        mode: overlay_mode(self.canvas.overlay_badges),
                    });
                    ui.close();
                }
            }
        });
        let session = self.ctl.session.id();
        let Some(overlay) = self
            .canvas
            .overlay_for(&session, generation, project, document_id)
        else {
            return;
        };
        let heatmap = self
            .canvas
            .overlay
            .is_some_and(|spec| spec.mode == OverlayMode::Heatmap);
        if overlay.legend.is_empty() {
            if heatmap && overlay.entries.is_empty() {
                ui.label(
                    egui::RichText::new("No values on activities").color(theme.palette().muted),
                );
            }
            return;
        }
        let appearance = if theme == Theme::Dark {
            Appearance::CanvasDark
        } else {
            Appearance::CanvasLight
        };
        ui.label(egui::RichText::new(&overlay.title).font(crate::theme::medium(12.0)));
        for item in &overlay.legend {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                let [r, g, b] =
                    processcraft_render::overlay_fill(appearance, overlay.scale, item.bucket);
                ui.painter()
                    .rect_filled(rect, 2, Color32::from_rgb(r, g, b));
                ui.painter().rect_stroke(
                    rect,
                    2,
                    egui::Stroke::new(1.0, theme.palette().border),
                    egui::StrokeKind::Inside,
                );
                ui.label(egui::RichText::new(&item.label).size(12.0));
            });
        }
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
                let hint = match tool {
                    Tool::Element(entry) => canvas::element_key(entry.kind)
                        .map(|key| format!("{label} ({key}) · click or drag")),
                    _ => None,
                };
                let label = hint.as_deref().unwrap_or(label);
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
                    .chain(std::iter::once((
                        "Text annotation (N)",
                        NodeKind::TextAnnotation.into(),
                    )))
                    .chain(
                        super::shell::more_palette_entries()
                            .into_iter()
                            .filter(|(_, e)| e.kind != NodeKind::TextAnnotation),
                    )
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

fn overlay_mode(badges: bool) -> processcraft_core::overlay::OverlayMode {
    if badges {
        processcraft_core::overlay::OverlayMode::Badge
    } else {
        processcraft_core::overlay::OverlayMode::Heatmap
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

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::attributes::{
        AttributeDefinition, AttributeTarget, AttributeType, AttributeValue, AttributeValueInput,
    };
    use processcraft_core::ids::{AttributeDefinitionId, ElementKey};
    use processcraft_core::overlay::{OverlayMode, OverlaySpec};

    #[test]
    fn switching_the_overlay_changes_neither_generation_nor_dirty_state() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let task = document.processes[0]
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Task)
            .unwrap()
            .id
            .selector();
        let definition = AttributeDefinition {
            id: AttributeDefinitionId::new(),
            name: "Cost".into(),
            description: String::new(),
            value_type: AttributeType::Number,
            unit: None,
            options: vec![],
            dictionary_kind: None,
            applies_to: vec![AttributeTarget::Activity],
            required: false,
        };
        let id = definition.id;
        assert!(app.ctl.mutate(vec![
            Mutation::CreateAttributeDefinition { definition },
            Mutation::SetAttributeValues {
                source: ElementKey {
                    document_id: document.id,
                    xml_id: task.clone(),
                },
                values: vec![AttributeValueInput {
                    definition_id: id,
                    value: Some(AttributeValue::Number("12".into())),
                }],
            },
        ]));
        let before = app.ctl.session.snapshot();
        let (generation, dirty, content, undo) = (
            before.generation,
            before.dirty,
            before.content_id,
            before.undo_count,
        );
        let session = app.ctl.session.id();
        for mode in [OverlayMode::Heatmap, OverlayMode::Badge] {
            app.canvas.overlay = Some(OverlaySpec {
                definition: id,
                mode,
            });
            let overlay = app
                .canvas
                .overlay_for(&session, generation, &before.project, document.id)
                .unwrap();
            assert_eq!(overlay.entries.len(), 1);
            assert_eq!(overlay.entries[0].element, task);
            let after = app.ctl.session.snapshot();
            assert_eq!(after.generation, generation);
            assert_eq!(after.dirty, dirty);
            assert_eq!(after.content_id, content);
            assert_eq!(after.undo_count, undo);
        }
        // No overlay selected: nothing is projected.
        app.canvas.overlay = None;
        assert!(
            app.canvas
                .overlay_for(&session, generation, &before.project, document.id)
                .is_none()
        );
        // A deleted definition drops the overlay instead of drawing stale colours.
        app.canvas.overlay = Some(OverlaySpec {
            definition: AttributeDefinitionId::new(),
            mode: OverlayMode::Heatmap,
        });
        assert!(
            app.canvas
                .overlay_for(&session, generation, &before.project, document.id)
                .is_none()
        );
        assert!(app.canvas.overlay.is_none());
    }

    #[test]
    fn export_option_is_cleared_when_the_overlay_is_dropped() {
        let mut app = ProcessCraft::default();
        let snap = app.ctl.session.snapshot();
        let document = snap.project.documents[0].id;
        let session = app.ctl.session.id();
        app.canvas.export_overlay = true;
        app.canvas.overlay = Some(OverlaySpec {
            definition: AttributeDefinitionId::new(),
            mode: OverlayMode::Heatmap,
        });
        // The definition does not exist, so the overlay is dropped and the flag with it.
        assert!(
            app.canvas
                .overlay_for(&session, snap.generation, &snap.project, document)
                .is_none()
        );
        assert!(!app.canvas.export_overlay);
    }
}
