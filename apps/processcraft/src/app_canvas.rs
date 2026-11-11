use super::{ProcessCraft, advance_drag, begin_drag, diagram_extents, finish_drag};
use crate::{
    theme::Theme,
    widgets::{Icon, icon_button},
};
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use processcraft_render::{Appearance, Viewport};

fn fit(diagram: &processcraft_core::model::Diagram, size: Vec2) -> ((f64, f64), f32) {
    if let Some((minx, miny, maxx, maxy)) = diagram_extents(diagram) {
        let zoom = (((size.x as f64 - 48.0).max(1.0) / (maxx - minx).max(1.0))
            .min((size.y as f64 - 48.0).max(1.0) / (maxy - miny).max(1.0))
            as f32)
            .clamp(0.125, 8.0);
        (
            (
                (minx + maxx - size.x as f64 / zoom as f64) * 0.5,
                (miny + maxy - size.y as f64 / zoom as f64) * 0.5,
            ),
            zoom,
        )
    } else {
        ((0.0, 0.0), 1.0)
    }
}
impl ProcessCraft {
    pub(super) fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let preview = self.layout_preview_document();
        let display = preview
            .as_ref()
            .or_else(|| snap.project.documents.get(self.doc));
        let diagram = display.and_then(|d| {
            d.diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        });
        let mut fit_clicked = false;
        if preview.is_some() {
            ui.label("Auto-fit preview");
        } else {
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::ZoomOut, "Zoom out", false).clicked() {
                    self.zoom = (self.zoom / 1.25).max(0.125);
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                if icon_button(ui, Icon::ZoomIn, "Zoom in", false).clicked() {
                    self.zoom = (self.zoom * 1.25).min(8.0);
                }
                fit_clicked = ui.button("Fit").on_hover_text("Fit diagram").clicked();
                if ui.button("Reset").on_hover_text("Reset view").clicked() {
                    self.camera = (0.0, 0.0);
                    self.zoom = 1.0;
                }
            });
        }
        let size = Vec2::new(
            (ui.available_width() - 24.0).max(1.0),
            (ui.available_height() - 24.0).max(1.0),
        );
        let fit_id = egui::Id::unique("processcraft-editor-fit");
        let document_key = snap.project.documents.get(self.doc).and_then(|document| {
            diagram
                .map(|diagram| format!("{}:{}:{}", self.ctl.session.id(), document.id, diagram.id))
        });
        let previous = ui.ctx().data(|data| data.get_temp::<String>(fit_id));
        if preview.is_none()
            && let (Some(diagram), Some(key)) = (diagram, document_key)
            && (fit_clicked || previous.as_ref() != Some(&key))
        {
            (self.camera, self.zoom) = fit(diagram, size);
            ui.ctx().data_mut(|data| data.insert_temp(fit_id, key));
        }
        egui::Frame::new()
            .fill(self.theme.palette().canvas)
            .inner_margin(12)
            .show(ui, |ui| self.canvas_render(ui, size, true));
    }
    pub(super) fn diagram_preview(&mut self, ui: &mut egui::Ui, height: f32) {
        self.canvas_render(
            ui,
            Vec2::new(ui.available_width().max(1.0), height.max(1.0)),
            false,
        );
    }
    fn canvas_render(&mut self, ui: &mut egui::Ui, size: Vec2, interactive: bool) {
        let ctx = ui.ctx().clone();
        let snap = self.ctl.session.snapshot();
        let preview = self.layout_preview_document();
        let interactive = interactive && preview.is_none();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        let (mut camera, zoom) = if interactive {
            (self.camera, self.zoom)
        } else {
            preview
                .as_ref()
                .or_else(|| snap.project.documents.get(self.doc))
                .and_then(|d| {
                    d.diagrams
                        .iter()
                        .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
                })
                .map_or(((0.0, 0.0), 1.0), |d| fit(d, size))
        };
        let (rect, response) = ui.allocate_exact_size(
            size,
            if interactive {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        let palette = self.theme.palette();
        ui.painter().rect_filled(rect, 4, palette.canvas);
        if let Some(d) = preview
            .as_ref()
            .or_else(|| snap.project.documents.get(self.doc))
        {
            if let Some(diagram) = d
                .diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
            {
                let key = format!(
                    "{}:{}:{}:{}:{:?}:{}:{}:{}:{}:{}",
                    self.ctl.session.id(),
                    if preview.is_some() {
                        "layout-preview"
                    } else if interactive {
                        "editor"
                    } else {
                        "preview"
                    },
                    snap.generation,
                    self.doc,
                    self.diagram_id,
                    camera.0,
                    camera.1,
                    zoom,
                    size.x,
                    size.y
                ) + if self.theme == Theme::Dark { "d" } else { "l" }
                    + &format!(":{}", ctx.pixels_per_point());
                if self.render_key != key {
                    let view = Viewport {
                        x: camera.0,
                        y: camera.1,
                        width: size.x as f64 / zoom as f64,
                        height: size.y as f64 / zoom as f64,
                        zoom: (zoom * ctx.pixels_per_point()).clamp(0.125, 8.0),
                    };
                    self.renderer.submit(crate::jobs::RenderJob {
                        key: key.clone(),
                        document: d.clone(),
                        diagram: diagram.id.selector(),
                        appearance: if self.theme == Theme::Dark {
                            Appearance::Dark
                        } else {
                            Appearance::Light
                        },
                        view,
                    });
                    self.render_failed = false;
                    self.render_key = key.clone();
                    if interactive {
                        self.hits.clear();
                    }
                    self.texture = None;
                }
                while let Some(result) = self.renderer.poll() {
                    if crate::jobs::is_current(&result.key, &key) {
                        match result.result {
                            Ok((pixels, w, h, hits)) => {
                                let image =
                                    egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
                                self.texture =
                                    Some(ctx.load_texture("diagram", image, Default::default()));
                                if interactive {
                                    self.hits = hits;
                                }
                            }
                            Err(e) => {
                                self.render_failed = true;
                                self.ctl.error = e;
                            }
                        }
                    }
                }
                if self.texture.is_none() && !self.render_failed {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
                if self.texture.is_none() {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if self.render_failed {
                            "Diagram unavailable"
                        } else {
                            "Rendering diagram…"
                        },
                        egui::FontId::proportional(13.0),
                        palette.muted,
                    );
                }
                if let Some(t) = &self.texture {
                    ui.painter().image(
                        t.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                if diagram.shapes.is_empty()
                    && diagram.edges.is_empty()
                    && d.capability.allows_structural_editing()
                {
                    ui.painter().text(
                        rect.center() - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        "Build your process",
                        egui::FontId::proportional(20.0),
                        palette.text,
                    );
                    ui.painter().text(
                        rect.center() + egui::vec2(0.0, 16.0),
                        egui::Align2::CENTER_CENTER,
                        "Add an element from the BPMN palette to begin.",
                        egui::FontId::proportional(12.0),
                        palette.muted,
                    );
                }
                if interactive {
                    for shape in diagram
                        .shapes
                        .iter()
                        .filter(|s| self.multi.contains(&s.element))
                    {
                        let b = &shape.bounds;
                        let min = Pos2::new(
                            rect.left() + ((b.x - camera.0) as f32 * zoom),
                            rect.top() + ((b.y - camera.1) as f32 * zoom),
                        );
                        let selection = Rect::from_min_size(
                            min,
                            Vec2::new(b.width as f32 * zoom, b.height as f32 * zoom),
                        );
                        ui.painter().rect_stroke(
                            selection,
                            0.0,
                            egui::Stroke::new(2.0, Color32::from_rgb(50, 120, 220)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if (response.drag_started() || response.clicked())
                        && self.commit_draft()
                        && let Some(pos) = if response.drag_started() {
                            ctx.input(|i| i.pointer.press_origin())
                        } else {
                            response.interact_pointer_pos()
                        }
                    {
                        let x = camera.0 + (pos.x - rect.left()) as f64 / zoom as f64;
                        let y = camera.1 + (pos.y - rect.top()) as f64 / zoom as f64;
                        let hit = self
                            .hits
                            .iter()
                            .rev()
                            .find(|h| h.contains(x, y, 5.0 / zoom as f64))
                            .cloned();
                        if let Some(h) = hit {
                            let id = h.key.xml_id;
                            if ctx.input(|i| i.modifiers.shift) && self.multi.is_empty() {
                                self.select(Some(id.clone()));
                            } else if ctx.input(|i| i.modifiers.shift) {
                                if !self.multi.contains(&id) {
                                    self.multi.push(id.clone());
                                }
                                self.ctl.session.set_selection(
                                    self.multi
                                        .iter()
                                        .cloned()
                                        .map(|xml_id| processcraft_core::ids::ElementKey {
                                            document_id: d.id,
                                            xml_id,
                                        })
                                        .collect(),
                                );
                            } else if !self.multi.contains(&id) {
                                self.select(Some(id.clone()));
                            }
                            if editable && response.drag_started() {
                                self.drag = begin_drag(
                                    &response,
                                    &ctx,
                                    diagram
                                        .shapes
                                        .iter()
                                        .filter(|s| self.multi.contains(&s.element))
                                        .map(|s| (s.id.selector(), s.bounds.clone()))
                                        .collect(),
                                );
                            }
                        } else {
                            self.select(None);
                        }
                    }
                    advance_drag(&mut self.drag, &ctx);
                    if response.dragged()
                        && let Some(drag) = &self.drag
                    {
                        for (_, bounds) in &drag.items {
                            let delta = drag.offset;
                            let min = Pos2::new(
                                rect.left() + ((bounds.x - camera.0) as f32 * zoom) + delta.x,
                                rect.top() + ((bounds.y - camera.1) as f32 * zoom) + delta.y,
                            );
                            let preview = Rect::from_min_size(
                                min,
                                Vec2::new(bounds.width as f32 * zoom, bounds.height as f32 * zoom),
                            );
                            ui.painter().rect_stroke(
                                preview,
                                0.0,
                                egui::Stroke::new(2.0, Color32::from_rgb(50, 120, 220)),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    if let Some(drag) = finish_drag(&response, &mut self.drag) {
                        let delta = drag.offset / zoom;
                        if delta.length() > 1.0 {
                            self.move_shapes(drag.items, delta.x as f64, delta.y as f64);
                        }
                    }
                    if response.hovered() {
                        let scroll = ctx.input(|i| i.smooth_scroll_delta);
                        if scroll != Vec2::ZERO {
                            camera.0 -= scroll.x as f64 / zoom as f64;
                            camera.1 -= scroll.y as f64 / zoom as f64;
                        }
                    }
                }
            } else {
                ui.label("No diagram in this document.");
            }
        }
        ui.painter().rect_stroke(
            rect,
            4,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        if interactive {
            self.camera = camera;
        }
    }
}
