use super::{ProcessCraft, advance_drag, begin_drag, diagram_extents, finish_drag};
use crate::{
    theme::Theme,
    widgets::{Icon, icon_button},
};
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use processcraft_core::{
    arrange::{Alignment, Distribution},
    commands::Mutation,
    ids::ElementSelector,
    model::{Bounds, FlowKind, NodeKind, Shape},
};
use processcraft_render::{Appearance, HitGeometry, Viewport};

const MIN_ZOOM: f32 = 0.125;
const MAX_ZOOM: f32 = 8.0;
const ZOOM_STEP: f32 = 1.25;

/// Camera of a rendered texture, so an older frame can be shown scaled until the new one arrives.
#[derive(Clone, Copy)]
pub(super) struct TextureView {
    camera: (f64, f64),
    zoom: f32,
    size: Vec2,
}

enum Gesture {
    Pan,
    BoxSelect(Pos2),
    Connect {
        element: ElementSelector,
        bounds: Bounds,
    },
    Resize {
        shape: ElementSelector,
        bounds: Bounds,
        corner: usize,
    },
}

struct LabelEdit {
    element: ElementSelector,
    text: String,
    original: String,
    center: (f64, f64),
    width: f64,
    focused: bool,
}

#[derive(Default)]
pub(super) struct CanvasState {
    gesture: Option<Gesture>,
    label_edit: Option<LabelEdit>,
    revealed: Option<ElementSelector>,
    view: Option<TextureView>,
    pending: Vec<(String, TextureView)>,
    scene: String,
    size: Vec2,
    rect: Option<Rect>,
    /// In-process clipboard of copied diagram content.
    clipboard: Option<processcraft_core::clipboard::Clipboard>,
    /// Canvas search query while the search overlay is open, and whether it still needs focus.
    search: Option<String>,
    search_focus: bool,
}

fn fit(diagram: &processcraft_core::model::Diagram, size: Vec2) -> ((f64, f64), f32) {
    if let Some((minx, miny, maxx, maxy)) = diagram_extents(diagram) {
        let zoom = (((size.x as f64 - 48.0).max(1.0) / (maxx - minx).max(1.0))
            .min((size.y as f64 - 48.0).max(1.0) / (maxy - miny).max(1.0))
            as f32)
            .clamp(MIN_ZOOM, MAX_ZOOM);
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

/// Change zoom while keeping the model point under `anchor` (offset from the canvas origin) fixed.
fn zoom_at(camera: (f64, f64), zoom: f32, new_zoom: f32, anchor: Vec2) -> ((f64, f64), f32) {
    let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let (ax, ay) = (anchor.x as f64, anchor.y as f64);
    (
        (
            camera.0 + ax / zoom as f64 - ax / new_zoom as f64,
            camera.1 + ay / zoom as f64 - ay / new_zoom as f64,
        ),
        new_zoom,
    )
}

fn to_model(rect: Rect, camera: (f64, f64), zoom: f32, pos: Pos2) -> (f64, f64) {
    (
        camera.0 + (pos.x - rect.left()) as f64 / zoom as f64,
        camera.1 + (pos.y - rect.top()) as f64 / zoom as f64,
    )
}

fn to_screen(rect: Rect, camera: (f64, f64), zoom: f32, b: &Bounds) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            rect.left() + ((b.x - camera.0) as f32 * zoom),
            rect.top() + ((b.y - camera.1) as f32 * zoom),
        ),
        Vec2::new(b.width as f32 * zoom, b.height as f32 * zoom),
    )
}

/// Where a texture rendered for `view` lands under the current camera.
fn texture_rect(rect: Rect, view: TextureView, camera: (f64, f64), zoom: f32) -> Rect {
    Rect::from_min_size(
        rect.min
            + Vec2::new(
                (view.camera.0 - camera.0) as f32 * zoom,
                (view.camera.1 - camera.1) as f32 * zoom,
            ),
        view.size * (zoom / view.zoom),
    )
}

/// Corners in order top-left, top-right, bottom-right, bottom-left.
fn corners(r: Rect) -> [Pos2; 4] {
    [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ]
}

fn resize_bounds(b: &Bounds, corner: usize, dx: f64, dy: f64) -> Bounds {
    const MIN: f64 = 20.0;
    let (mut left, mut top, mut right, mut bottom) = (b.x, b.y, b.x + b.width, b.y + b.height);
    let west = corner == 0 || corner == 3;
    let north = corner < 2;
    if west {
        left = (left + dx).min(right - MIN);
    } else {
        right = (right + dx).max(left + MIN);
    }
    if north {
        top = (top + dy).min(bottom - MIN);
    } else {
        bottom = (bottom + dy).max(top + MIN);
    }
    Bounds {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// Nearest free spot around `center`; shapes that fully contain a candidate (pools, lanes,
/// expanded subprocesses) are containers, not obstacles.
pub(super) fn vacant_bounds(
    shapes: &[Shape],
    center: (f64, f64),
    width: f64,
    height: f64,
) -> Bounds {
    let mut offsets: Vec<(i32, i32)> = (-8..=8)
        .flat_map(|i| (-8..=8).map(move |j| (i, j)))
        .collect();
    offsets.sort_by_key(|(i, j)| i * i + j * j);
    let candidate = |(i, j): (i32, i32)| Bounds {
        x: center.0 - width / 2.0 + i as f64 * (width + 40.0),
        y: center.1 - height / 2.0 + j as f64 * (height + 40.0),
        width,
        height,
    };
    offsets
        .into_iter()
        .map(candidate)
        .find(|c| {
            shapes.iter().all(|shape| {
                let b = &shape.bounds;
                let contains = b.x <= c.x
                    && b.y <= c.y
                    && b.x + b.width >= c.x + c.width
                    && b.y + b.height >= c.y + c.height;
                contains
                    || c.x + c.width + 20.0 <= b.x
                    || b.x + b.width + 20.0 <= c.x
                    || c.y + c.height + 20.0 <= b.y
                    || b.y + b.height + 20.0 <= c.y
            })
        })
        .unwrap_or_else(|| candidate((0, 0)))
}

impl ProcessCraft {
    /// Visible model-space center of the editor canvas.
    pub(super) fn viewport_center(&self) -> (f64, f64) {
        let size = if self.canvas.size == Vec2::ZERO {
            Vec2::new(800.0, 600.0)
        } else {
            self.canvas.size
        };
        (
            self.camera.0 + size.x as f64 / self.zoom as f64 / 2.0,
            self.camera.1 + size.y as f64 / self.zoom as f64 / 2.0,
        )
    }
    fn zoom_canvas(&mut self, new_zoom: f32, anchor: Option<Vec2>) {
        let anchor = anchor.unwrap_or(self.canvas.size / 2.0);
        (self.camera, self.zoom) = zoom_at(self.camera, self.zoom, new_zoom, anchor);
    }
    /// Editor keyboard shortcuts; skipped while a text field has focus.
    pub(super) fn canvas_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        // Ctrl+Plus/Minus/0 zoom the diagram, not the whole UI (View menu keeps UI scale).
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        if self.view != 0 || ctx.egui_wants_keyboard_input() || self.canvas.label_edit.is_some() {
            return;
        }
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let zoom = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL, Key::Plus)
                || i.consume_key(Modifiers::CTRL, Key::Equals)
            {
                Some(self.zoom * ZOOM_STEP)
            } else if i.consume_key(Modifiers::CTRL, Key::Minus) {
                Some(self.zoom / ZOOM_STEP)
            } else if i.consume_key(Modifiers::CTRL, Key::Num0) {
                Some(1.0)
            } else {
                None
            }
        });
        if let Some(zoom) = zoom {
            self.zoom_canvas(zoom, None);
        }
        if ctx.input_mut(|i| {
            i.consume_key(ctrl_shift, Key::Z) || i.consume_key(Modifiers::CTRL, Key::Y)
        }) {
            self.history(processcraft_core::commands::Request::Redo);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            self.history(processcraft_core::commands::Request::Undo);
        }
        let editable = self
            .ctl
            .session
            .snapshot()
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        if editable
            && !self.multi.is_empty()
            && ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace)
            })
        {
            self.delete();
        }
        self.edit_shortcuts(ctx, editable);
        if editable && !self.multi.is_empty() && self.drag.is_none() {
            let nudge = ctx.input_mut(|i| {
                let mut step = None;
                for (key, x, y) in [
                    (Key::ArrowLeft, -1.0, 0.0),
                    (Key::ArrowRight, 1.0, 0.0),
                    (Key::ArrowUp, 0.0, -1.0),
                    (Key::ArrowDown, 0.0, 1.0),
                ] {
                    // bpmn-js keyboard move: 1 px, Shift 10 px.
                    if i.consume_key(Modifiers::SHIFT, key) {
                        step = Some((x * 10.0, y * 10.0));
                    } else if i.consume_key(Modifiers::NONE, key) {
                        step = Some((x, y));
                    }
                }
                step
            });
            if let Some((dx, dy)) = nudge {
                self.move_selected(dx, dy);
            }
        }
        if self.drag.is_none()
            && self.canvas.gesture.is_none()
            && !self.multi.is_empty()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.select(None);
        }
    }
    /// Scroll a selection made outside the canvas (outline, problems) into view.
    fn reveal_selection(&mut self, diagram: &processcraft_core::model::Diagram, size: Vec2) {
        if self.canvas.revealed == self.selected {
            return;
        }
        self.canvas.revealed = self.selected.clone();
        let Some(selected) = &self.selected else {
            return;
        };
        let target = diagram
            .shapes
            .iter()
            .find(|s| &s.element == selected)
            .map(|s| s.bounds.clone())
            .or_else(|| {
                let edge = diagram.edges.iter().find(|e| &e.element == selected)?;
                let xs = edge.waypoints.iter().map(|p| p.x);
                let ys = edge.waypoints.iter().map(|p| p.y);
                let (x0, x1) = (
                    xs.clone().fold(f64::MAX, f64::min),
                    xs.fold(f64::MIN, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::MAX, f64::min),
                    ys.fold(f64::MIN, f64::max),
                );
                (x0 <= x1).then_some(Bounds {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                })
            });
        let Some(b) = target else {
            return;
        };
        let (w, h) = (
            size.x as f64 / self.zoom as f64,
            size.y as f64 / self.zoom as f64,
        );
        let visible = b.x < self.camera.0 + w
            && b.x + b.width > self.camera.0
            && b.y < self.camera.1 + h
            && b.y + b.height > self.camera.1;
        if !visible {
            self.camera = (
                b.x + b.width / 2.0 - w / 2.0,
                b.y + b.height / 2.0 - h / 2.0,
            );
        }
    }
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
            if let (Some(document), Some(diagram)) = (snap.project.documents.get(self.doc), diagram)
            {
                let scopes = processcraft_core::queries::choreography_scopes(document);
                if let Some(scope) = scopes.iter().find(|scope| scope.owner == diagram.plane) {
                    let mut path = Vec::new();
                    let mut cursor = Some(scope.owner.clone());
                    while let Some(owner) = cursor.take() {
                        let Some(current) = scopes.iter().find(|candidate| {
                            candidate.owner == owner && candidate.root.id == scope.root.id
                        }) else {
                            break;
                        };
                        let label = if current.parent.is_none() {
                            current
                                .root
                                .name
                                .clone()
                                .filter(|name| !name.is_empty())
                                .unwrap_or_else(|| "Unnamed choreography".into())
                        } else {
                            scopes
                                .iter()
                                .flat_map(|parent| parent.activities.iter())
                                .find(|activity| activity.id == owner)
                                .and_then(|activity| activity.name.clone())
                                .filter(|name| !name.is_empty())
                                .unwrap_or_else(|| "Unnamed sub-choreography".into())
                        };
                        path.push((owner, label));
                        cursor = current.parent.clone();
                        if path.len() > scopes.len() {
                            break;
                        }
                    }
                    path.reverse();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Scope:");
                        for (index, (owner, label)) in path.iter().enumerate() {
                            if index > 0 {
                                ui.label("›");
                            }
                            ui.push_id(owner, |ui| {
                                self.scope_diagram_ui(ui, document, owner, label, snap.generation)
                            });
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::ZoomOut, "Zoom out (Ctrl+Minus)", false).clicked() {
                    self.zoom_canvas(self.zoom / ZOOM_STEP, None);
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                if icon_button(ui, Icon::ZoomIn, "Zoom in (Ctrl+Plus)", false).clicked() {
                    self.zoom_canvas(self.zoom * ZOOM_STEP, None);
                }
                fit_clicked = ui.button("Fit").on_hover_text("Fit diagram").clicked();
                if ui.button("Reset").on_hover_text("Reset view").clicked() {
                    self.camera = (0.0, 0.0);
                    self.zoom = 1.0;
                }
                if self.multi.len() >= 2 {
                    ui.separator();
                    self.arrange_menus(ui);
                }
            });
        }
        let size = Vec2::new(
            (ui.available_width() - 24.0).max(1.0),
            (ui.available_height() - 24.0).max(1.0),
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("test_canvas_size"), size));
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
        if preview.is_none()
            && let Some(diagram) = diagram
        {
            self.reveal_selection(diagram, size);
        }
        let frame = egui::Frame::new()
            .fill(self.theme.palette().canvas)
            .inner_margin(12)
            .show(ui, |ui| self.canvas_render(ui, size, true));
        self.auto_scroll(ui.ctx());
        self.search_ui(ui.ctx());
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("test_canvas_rect"), frame.response.rect)
        });
        #[cfg(not(test))]
        let _ = frame;
    }
    pub(super) fn diagram_preview(&mut self, ui: &mut egui::Ui, height: f32) {
        self.canvas_render(
            ui,
            Vec2::new(ui.available_width().max(1.0), height.max(1.0)),
            false,
        );
    }
    fn hit_at(
        &self,
        x: f64,
        y: f64,
        zoom: f32,
        labels_first: bool,
    ) -> Option<processcraft_render::HitRegion> {
        let tolerance = 5.0 / zoom as f64;
        if labels_first {
            self.hits
                .iter()
                .rev()
                .find(|h| h.is_label && h.contains(x, y, tolerance))
                .or_else(|| self.hits.iter().rev().find(|h| h.contains(x, y, tolerance)))
        } else {
            self.hits.iter().rev().find(|h| h.contains(x, y, tolerance))
        }
        .cloned()
    }
    /// Topmost shape (never a label region) under the pointer other than `exclude`.
    fn connect_target_at(
        &self,
        x: f64,
        y: f64,
        zoom: f32,
        exclude: &ElementSelector,
    ) -> Option<processcraft_render::HitRegion> {
        let tolerance = 5.0 / zoom as f64;
        self.hits
            .iter()
            .rev()
            .find(|h| !h.is_label && &h.key.xml_id != exclude && h.contains(x, y, tolerance))
            .cloned()
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
        let (mut camera, mut zoom) = if interactive {
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
        if interactive {
            self.canvas.size = size;
        }
        let (rect, response) = ui.allocate_exact_size(
            size,
            if interactive {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        if interactive {
            self.canvas.rect = Some(rect);
        }
        let palette = self.theme.palette();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4, palette.canvas);
        if let Some(d) = preview
            .as_ref()
            .or_else(|| snap.project.documents.get(self.doc))
        {
            if let Some(diagram) = d
                .diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
            {
                let scene = format!(
                    "{}:{}:{}:{:?}:{}",
                    self.ctl.session.id(),
                    if preview.is_some() {
                        "layout-preview"
                    } else if interactive {
                        "editor"
                    } else {
                        "preview"
                    },
                    self.doc,
                    self.diagram_id,
                    if self.theme == Theme::Dark { "d" } else { "l" }
                );
                let key = format!(
                    "{scene}:{}:{}:{}:{}:{}:{}:{}",
                    snap.generation,
                    camera.0,
                    camera.1,
                    zoom,
                    size.x,
                    size.y,
                    ctx.pixels_per_point()
                );
                // Another document, diagram, or theme: the previous frame is not a valid placeholder.
                if self.canvas.scene != scene {
                    self.canvas.scene = scene;
                    self.canvas.pending.clear();
                    self.canvas.view = None;
                    self.texture = None;
                    if interactive {
                        self.hits.clear();
                    }
                }
                if self.render_key != key {
                    let view = Viewport {
                        x: camera.0,
                        y: camera.1,
                        width: size.x as f64 / zoom as f64,
                        height: size.y as f64 / zoom as f64,
                        zoom: (zoom * ctx.pixels_per_point()).clamp(MIN_ZOOM, MAX_ZOOM),
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
                    self.canvas
                        .pending
                        .push((key.clone(), TextureView { camera, zoom, size }));
                    if self.canvas.pending.len() > 32 {
                        self.canvas.pending.remove(0);
                    }
                }
                // Results arrive in submission order; any pending one is newer than what is shown.
                while let Some(result) = self.renderer.poll() {
                    let Some(index) = self
                        .canvas
                        .pending
                        .iter()
                        .position(|(pending, _)| *pending == result.key)
                    else {
                        continue;
                    };
                    let view = self.canvas.pending[index].1;
                    self.canvas.pending.drain(..=index);
                    match result.result {
                        Ok((pixels, w, h, hits)) => {
                            let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
                            self.texture =
                                Some(ctx.load_texture("diagram", image, Default::default()));
                            self.canvas.view = Some(view);
                            if interactive {
                                self.hits = hits;
                            }
                        }
                        Err(e) => {
                            if crate::jobs::is_current(&result.key, &key) {
                                self.render_failed = true;
                                self.ctl.error = e;
                            }
                        }
                    }
                }
                if !self.canvas.pending.is_empty() && !self.render_failed {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
                match (&self.texture, self.canvas.view) {
                    (Some(t), Some(view)) => {
                        painter.image(
                            t.id(),
                            texture_rect(rect, view, camera, zoom),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    _ => {
                        painter.text(
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
                }
                if diagram.shapes.is_empty()
                    && diagram.edges.is_empty()
                    && d.capability.allows_structural_editing()
                {
                    painter.text(
                        rect.center() - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        "Build your process",
                        egui::FontId::proportional(20.0),
                        palette.text,
                    );
                    painter.text(
                        rect.center() + egui::vec2(0.0, 16.0),
                        egui::Align2::CENTER_CENTER,
                        "Add an element from the BPMN palette to begin.",
                        egui::FontId::proportional(12.0),
                        palette.muted,
                    );
                    if interactive {
                        painter.text(
                            rect.center() + egui::vec2(0.0, 38.0),
                            egui::Align2::CENTER_CENTER,
                            "Drag empty space to pan · Shift-drag to select · Ctrl+wheel to zoom · Double-click to rename · Drag the blue handle to connect",
                            egui::FontId::proportional(11.0),
                            palette.muted,
                        );
                    }
                }
                if interactive {
                    let accent = ui.visuals().selection.stroke.color;
                    let diagram_id = diagram.id.selector();
                    let by_representation = self.representation_diagram.as_ref()
                        == Some(&diagram_id)
                        && !self.representation_selection.is_empty();
                    let selected_shapes: Vec<&Shape> = diagram
                        .shapes
                        .iter()
                        .filter(|s| {
                            if by_representation {
                                self.representation_selection.contains(&s.id.selector())
                            } else {
                                self.multi.contains(&s.element)
                            }
                        })
                        .collect();
                    for shape in &selected_shapes {
                        painter.rect_stroke(
                            to_screen(rect, camera, zoom, &shape.bounds).expand(2.0),
                            2.0,
                            egui::Stroke::new(2.0, accent),
                            egui::StrokeKind::Outside,
                        );
                    }
                    for edge in diagram.edges.iter().filter(|e| {
                        if by_representation {
                            self.representation_selection.contains(&e.id.selector())
                        } else {
                            self.multi.contains(&e.element)
                        }
                    }) {
                        let points = edge
                            .waypoints
                            .iter()
                            .map(|p| {
                                Pos2::new(
                                    rect.left() + (p.x - camera.0) as f32 * zoom,
                                    rect.top() + (p.y - camera.1) as f32 * zoom,
                                )
                            })
                            .collect();
                        painter.add(egui::Shape::line(points, egui::Stroke::new(3.0, accent)));
                    }
                    // Handles of a single selected shape: resize corners and a connect handle.
                    let single = (editable && selected_shapes.len() == 1 && self.drag.is_none())
                        .then(|| selected_shapes[0]);
                    let mut connect_handle = None;
                    if let Some(shape) = single {
                        let screen = to_screen(rect, camera, zoom, &shape.bounds);
                        for corner in corners(screen) {
                            let handle = Rect::from_center_size(corner, Vec2::splat(7.0));
                            painter.rect_filled(handle, 1.0, palette.base);
                            painter.rect_stroke(
                                handle,
                                1.0,
                                egui::Stroke::new(1.5, accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if processcraft_core::queries::find_node(d, &shape.element).is_some() {
                            let center = Pos2::new(screen.right() + 16.0, screen.center().y);
                            painter.circle_filled(center, 8.0, accent);
                            painter.arrow(
                                center - Vec2::new(4.0, 0.0),
                                Vec2::new(8.0, 0.0),
                                egui::Stroke::new(1.5, palette.base),
                            );
                            connect_handle = Some(center);
                        }
                    }
                    let hover = response.hover_pos();
                    if let (Some(center), Some(pos)) = (connect_handle, hover)
                        && pos.distance(center) <= 10.0
                        && self.canvas.gesture.is_none()
                    {
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                        response
                            .clone()
                            .on_hover_text_at_pointer("Drag to another element to connect");
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.canvas.gesture = None;
                    }
                    let alt = ctx.input(|i| i.modifiers.alt);
                    let shift = ctx.input(|i| i.modifiers.shift);
                    if response.drag_started()
                        && let Some(pos) = ctx.input(|i| i.pointer.press_origin())
                    {
                        let pan = ctx.input(|i| {
                            i.pointer.middle_down()
                                || (i.key_down(egui::Key::Space)
                                    && !ctx.egui_wants_keyboard_input())
                        });
                        if pan {
                            self.canvas.gesture = Some(Gesture::Pan);
                        } else if let Some(shape) = single {
                            let screen = to_screen(rect, camera, zoom, &shape.bounds);
                            if let Some(corner) =
                                corners(screen).iter().position(|c| c.distance(pos) <= 6.0)
                            {
                                self.canvas.gesture = Some(Gesture::Resize {
                                    shape: shape.id.selector(),
                                    bounds: shape.bounds.clone(),
                                    corner,
                                });
                            } else if connect_handle.is_some_and(|c| c.distance(pos) <= 10.0) {
                                self.canvas.gesture = Some(Gesture::Connect {
                                    element: shape.element.clone(),
                                    bounds: shape.bounds.clone(),
                                });
                            }
                        }
                    }
                    if ((response.drag_started() && self.canvas.gesture.is_none())
                        || response.clicked())
                        && self.commit_draft()
                        && let Some(pos) = if response.drag_started() {
                            ctx.input(|i| i.pointer.press_origin())
                        } else {
                            response.interact_pointer_pos()
                        }
                    {
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        if let Some(h) = self.hit_at(x, y, zoom, alt) {
                            let id = h.key.xml_id.clone();
                            let representation = h.diagram_element_id.clone();
                            let same_diagram =
                                self.representation_diagram.as_ref() == Some(&diagram_id);
                            if !shift
                                && (!same_diagram
                                    || !self.representation_selection.contains(&representation))
                            {
                                self.select(Some(id.clone()));
                                self.representation_selection = vec![representation.clone()];
                            } else if shift {
                                let mut multi = self.multi.clone();
                                let mut representations = if same_diagram {
                                    self.representation_selection.clone()
                                } else {
                                    Vec::new()
                                };
                                if response.clicked() && multi.contains(&id) {
                                    multi.retain(|value| value != &id);
                                    representations.retain(|value| value != &representation);
                                } else {
                                    if !multi.contains(&id) {
                                        multi.push(id.clone());
                                    }
                                    if !representations.contains(&representation) {
                                        representations.push(representation.clone());
                                    }
                                }
                                if multi.first() != self.selected.as_ref() {
                                    self.select(multi.first().cloned());
                                }
                                self.multi = multi;
                                self.representation_selection = representations;
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
                            }
                            self.representation_diagram = Some(diagram_id.clone());
                            self.canvas.revealed = self.selected.clone();
                            if editable && response.drag_started() {
                                let inside_shape = diagram
                                    .shapes
                                    .iter()
                                    .find(|s| s.id == representation)
                                    .is_some_and(|s| {
                                        x >= s.bounds.x
                                            && x <= s.bounds.x + s.bounds.width
                                            && y >= s.bounds.y
                                            && y <= s.bounds.y + s.bounds.height
                                    });
                                let label_drag = h.is_label && (!inside_shape || alt);
                                let items = if label_drag {
                                    diagram
                                        .shapes
                                        .iter()
                                        .find(|s| s.id == representation)
                                        .and_then(|s| s.label.as_ref())
                                        .or_else(|| {
                                            diagram
                                                .edges
                                                .iter()
                                                .find(|e| e.id == representation)
                                                .and_then(|e| e.label.as_ref())
                                        })
                                        .and_then(|l| l.bounds.clone())
                                        .or_else(|| match &h.geometry {
                                            HitGeometry::Shape(bounds) => Some(bounds.clone()),
                                            _ => None,
                                        })
                                        .map(|b| vec![(representation.clone(), b)])
                                        .unwrap_or_default()
                                } else {
                                    diagram
                                        .shapes
                                        .iter()
                                        .filter(|s| {
                                            self.representation_selection.contains(&s.id.selector())
                                        })
                                        .map(|s| (s.id.selector(), s.bounds.clone()))
                                        .collect()
                                };
                                if label_drag && items.is_empty() {
                                    self.ctl.error = "Set label bounds in Diagram representation properties before dragging the label.".into();
                                } else if self.ctl.session.snapshot().generation != snap.generation
                                {
                                    self.ctl.error =
                                        "Properties changed; start the move again.".into();
                                } else if !items.is_empty() {
                                    self.drag = begin_drag(&response, &ctx, items);
                                    if let Some(drag) = &mut self.drag {
                                        drag.context =
                                            Some((snap.generation, d.id, diagram_id.clone()));
                                        drag.label = label_drag;
                                    }
                                }
                            }
                        } else if response.drag_started() {
                            self.canvas.gesture = Some(if shift {
                                Gesture::BoxSelect(pos)
                            } else {
                                Gesture::Pan
                            });
                        } else {
                            self.select(None);
                            self.canvas.revealed = None;
                        }
                    }
                    if editable
                        && response.double_clicked()
                        && self.canvas.label_edit.is_none()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        if let Some(h) = self.hit_at(x, y, zoom, alt) {
                            let element = h.key.xml_id.clone();
                            let annotation = processcraft_core::queries::find_node(d, &element)
                                .is_some_and(|n| n.kind == NodeKind::TextAnnotation);
                            let association = processcraft_core::queries::find_flow(d, &element)
                                .is_some_and(|f| f.kind == FlowKind::Association);
                            let (center, width) = match &h.geometry {
                                HitGeometry::Edge(points) => {
                                    let mid = &points[points.len() / 2];
                                    let prev = &points[(points.len() / 2).saturating_sub(1)];
                                    (((mid.x + prev.x) / 2.0, (mid.y + prev.y) / 2.0), 120.0)
                                }
                                HitGeometry::Shape(b)
                                | HitGeometry::Ellipse(b)
                                | HitGeometry::Diamond(b)
                                | HitGeometry::Hexagon(b)
                                | HitGeometry::DataObject(b)
                                | HitGeometry::DataStore(b) => {
                                    ((b.x + b.width / 2.0, b.y + b.height / 2.0), b.width)
                                }
                            };
                            if annotation {
                                self.ctl.error =
                                    "Edit annotation text in the Properties panel.".into();
                            } else if !association {
                                let original =
                                    processcraft_core::queries::element_name(d, &element)
                                        .unwrap_or_default()
                                        .to_owned();
                                self.drag = None;
                                self.canvas.label_edit = Some(LabelEdit {
                                    element,
                                    text: original.clone(),
                                    original,
                                    center,
                                    width,
                                    focused: false,
                                });
                            }
                        }
                    }
                    if self
                        .drag
                        .as_ref()
                        .and_then(|drag| drag.context.as_ref())
                        .is_some_and(|(generation, document, diagram_id)| {
                            *generation != self.ctl.session.snapshot().generation
                                || *document != d.id
                                || *diagram_id != diagram.id.selector()
                        })
                    {
                        self.drag = None;
                        self.ctl.error =
                            "The diagram changed while dragging. Start the move again.".into();
                    }
                    advance_drag(&mut self.drag, &ctx);
                    if response.dragged()
                        && let Some(drag) = &self.drag
                    {
                        for (_, bounds) in &drag.items {
                            let preview =
                                to_screen(rect, camera, zoom, bounds).translate(drag.offset);
                            if let (Some(t), Some(view)) = (&self.texture, self.canvas.view) {
                                let uv = |x: f64, y: f64| {
                                    Pos2::new(
                                        (x - view.camera.0) as f32 * view.zoom / view.size.x,
                                        (y - view.camera.1) as f32 * view.zoom / view.size.y,
                                    )
                                };
                                painter.image(
                                    t.id(),
                                    preview,
                                    Rect::from_min_max(
                                        uv(bounds.x, bounds.y),
                                        uv(bounds.x + bounds.width, bounds.y + bounds.height),
                                    ),
                                    Color32::from_white_alpha(220),
                                );
                            }
                            painter.rect_stroke(
                                preview,
                                0.0,
                                egui::Stroke::new(2.0, accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    if let Some(drag) = finish_drag(&response, &mut self.drag) {
                        let delta = drag.offset / zoom;
                        if delta.length() > 1.0 {
                            if drag.label {
                                if let Some((representation_id, mut bounds)) =
                                    drag.items.into_iter().next()
                                {
                                    bounds.x += delta.x as f64;
                                    bounds.y += delta.y as f64;
                                    let mutation =
                                        if let Some(shape) = diagram.shapes.iter().find(|s| {
                                            s.id == representation_id && s.label.is_none()
                                        }) {
                                            Mutation::SetShapeLabel {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                shape_id: representation_id,
                                                label: Some(processcraft_core::model::Label {
                                                    bounds: Some(bounds),
                                                    ..Default::default()
                                                }),
                                                label_visibility: shape.label_visibility,
                                            }
                                        } else if let Some(edge) = diagram.edges.iter().find(|e| {
                                            e.id == representation_id && e.label.is_none()
                                        }) {
                                            Mutation::SetEdgeLabel {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                edge_id: representation_id,
                                                label: Some(processcraft_core::model::Label {
                                                    bounds: Some(bounds),
                                                    ..Default::default()
                                                }),
                                                label_visibility: edge.label_visibility,
                                            }
                                        } else {
                                            Mutation::SetLabelBounds {
                                                document_id: d.id,
                                                diagram_id: diagram.id.selector(),
                                                representation_id,
                                                bounds,
                                            }
                                        };
                                    self.ctl.mutate(vec![mutation]);
                                }
                            } else {
                                self.move_shapes(drag.items, delta.x as f64, delta.y as f64);
                            }
                        }
                    }
                    // Canvas gestures: pan, box select, connect, resize.
                    let pointer = ctx.input(|i| i.pointer.latest_pos());
                    let origin = ctx.input(|i| i.pointer.press_origin());
                    if response.dragged() {
                        match &self.canvas.gesture {
                            Some(Gesture::Pan) => {
                                let delta = response.drag_delta();
                                camera.0 -= delta.x as f64 / zoom as f64;
                                camera.1 -= delta.y as f64 / zoom as f64;
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            Some(Gesture::BoxSelect(start)) => {
                                if let Some(pos) = pointer {
                                    let area = Rect::from_two_pos(*start, pos);
                                    painter.rect_filled(area, 0.0, accent.gamma_multiply(0.12));
                                    painter.rect_stroke(
                                        area,
                                        0.0,
                                        egui::Stroke::new(1.0, accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                            Some(Gesture::Connect { element, bounds }) => {
                                if let Some(pos) = pointer {
                                    let from = to_screen(rect, camera, zoom, bounds).center();
                                    let (x, y) = to_model(rect, camera, zoom, pos);
                                    if let Some(target) =
                                        self.connect_target_at(x, y, zoom, element).and_then(|h| {
                                            diagram
                                                .shapes
                                                .iter()
                                                .find(|s| s.id == h.diagram_element_id)
                                        })
                                    {
                                        painter.rect_stroke(
                                            to_screen(rect, camera, zoom, &target.bounds)
                                                .expand(2.0),
                                            2.0,
                                            egui::Stroke::new(2.0, accent),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                    painter.extend(egui::Shape::dashed_line(
                                        &[from, pos],
                                        egui::Stroke::new(2.0, accent),
                                        6.0,
                                        4.0,
                                    ));
                                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                                }
                            }
                            Some(Gesture::Resize { bounds, corner, .. }) => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let resized = resize_bounds(
                                        bounds,
                                        *corner,
                                        delta.x as f64,
                                        delta.y as f64,
                                    );
                                    painter.rect_stroke(
                                        to_screen(rect, camera, zoom, &resized),
                                        0.0,
                                        egui::Stroke::new(2.0, accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                            None => {}
                        }
                    }
                    if response.drag_stopped()
                        && let Some(gesture) = self.canvas.gesture.take()
                    {
                        match gesture {
                            Gesture::Pan => {}
                            Gesture::BoxSelect(start) => {
                                if let Some(pos) = pointer {
                                    let (x0, y0) = to_model(rect, camera, zoom, start);
                                    let (x1, y1) = to_model(rect, camera, zoom, pos);
                                    let (x0, x1) = (x0.min(x1), x0.max(x1));
                                    let (y0, y1) = (y0.min(y1), y0.max(y1));
                                    let inside: Vec<&Shape> = diagram
                                        .shapes
                                        .iter()
                                        .filter(|s| {
                                            let b = &s.bounds;
                                            b.x >= x0
                                                && b.y >= y0
                                                && b.x + b.width <= x1
                                                && b.y + b.height <= y1
                                        })
                                        .collect();
                                    if let Some(first) = inside.first() {
                                        self.select(Some(first.element.clone()));
                                        let mut multi: Vec<ElementSelector> = Vec::new();
                                        for shape in &inside {
                                            if !multi.contains(&shape.element) {
                                                multi.push(shape.element.clone());
                                            }
                                        }
                                        self.multi = multi;
                                        self.representation_selection =
                                            inside.iter().map(|s| s.id.selector()).collect();
                                        self.representation_diagram = Some(diagram_id.clone());
                                        self.canvas.revealed = self.selected.clone();
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
                                    }
                                }
                            }
                            Gesture::Connect { element, .. } => {
                                if let Some(pos) = pointer {
                                    let (x, y) = to_model(rect, camera, zoom, pos);
                                    if let Some(target) =
                                        self.connect_target_at(x, y, zoom, &element)
                                    {
                                        self.connect_elements(element, target.key.xml_id, None);
                                    }
                                }
                            }
                            Gesture::Resize {
                                shape,
                                bounds,
                                corner,
                            } => {
                                if let (Some(pos), Some(start)) = (pointer, origin) {
                                    let delta = (pos - start) / zoom;
                                    let resized = resize_bounds(
                                        &bounds,
                                        corner,
                                        delta.x as f64,
                                        delta.y as f64,
                                    );
                                    if resized != bounds {
                                        self.move_shapes(vec![(shape, resized)], 0.0, 0.0);
                                    }
                                }
                            }
                        }
                    }
                    // Inline name editor: Enter or focus loss commits, Escape cancels.
                    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                    if let Some(edit) = &mut self.canvas.label_edit {
                        let width = (edit.width as f32 * zoom).max(140.0);
                        let center = Pos2::new(
                            rect.left() + (edit.center.0 - camera.0) as f32 * zoom,
                            rect.top() + (edit.center.1 - camera.1) as f32 * zoom,
                        );
                        let field = ui.put(
                            Rect::from_center_size(center, Vec2::new(width, 28.0)),
                            egui::TextEdit::singleline(&mut edit.text)
                                .id(egui::Id::unique("processcraft-canvas-label-edit")),
                        );
                        if !edit.focused {
                            field.request_focus();
                            edit.focused = true;
                        } else if escape {
                            self.canvas.label_edit = None;
                        } else if field.lost_focus()
                            && let Some(edit) = self.canvas.label_edit.take()
                            && edit.text != edit.original
                            && self.ctl.mutate(vec![Mutation::Rename {
                                document_id: d.id,
                                element_id: edit.element.clone(),
                                name: Some(edit.text.clone()),
                            }])
                            && self.selected.as_ref() == Some(&edit.element)
                            && !self.draft_dirty
                        {
                            self.draft = edit.text;
                            self.draft_name_present = true;
                        }
                    }
                    if response.hovered() {
                        let (scroll, zoom_delta) =
                            ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
                        if zoom_delta != 1.0 {
                            let anchor = hover.map_or(size / 2.0, |pos| pos - rect.min);
                            (camera, zoom) = zoom_at(camera, zoom, zoom * zoom_delta, anchor);
                        } else if scroll != Vec2::ZERO {
                            camera.0 -= scroll.x as f64 / zoom as f64;
                            camera.1 -= scroll.y as f64 / zoom as f64;
                        }
                    }
                }
            } else {
                ui.label("No diagram in this document.");
            }
        }
        painter.rect_stroke(
            rect,
            4,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        if interactive {
            self.camera = camera;
            self.zoom = zoom;
        }
    }
}

/// Pixels from the canvas border that start auto-scroll while dragging, and the scroll step
/// per frame (diagram-js AutoScroll: 20 and 10).
const AUTO_SCROLL_BAND: f32 = 20.0;
const AUTO_SCROLL_STEP: f32 = 10.0;

/// Screen-space camera motion for a drag at `pos` near the border of `rect`.
fn auto_scroll_step(rect: Rect, pos: Pos2) -> Vec2 {
    let near = |distance: f32| distance > 0.0 && distance < AUTO_SCROLL_BAND;
    let axis = |low: f32, high: f32| {
        if near(low) {
            -AUTO_SCROLL_STEP
        } else if near(high) {
            AUTO_SCROLL_STEP
        } else {
            0.0
        }
    };
    Vec2::new(
        axis(pos.x - rect.left(), rect.right() - pos.x),
        axis(pos.y - rect.top(), rect.bottom() - pos.y),
    )
}

const ALIGNMENTS: [(Alignment, &str); 6] = [
    (Alignment::Left, "Align left"),
    (Alignment::Center, "Align center"),
    (Alignment::Right, "Align right"),
    (Alignment::Top, "Align top"),
    (Alignment::Middle, "Align middle"),
    (Alignment::Bottom, "Align bottom"),
];

/// Selection productivity: select all, clipboard, align/distribute, search, auto-scroll.
impl ProcessCraft {
    fn current_diagram<'a>(
        &self,
        document: &'a processcraft_core::model::BpmnDocument,
    ) -> Option<&'a processcraft_core::model::Diagram> {
        document
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
    }
    /// Select several elements of the current diagram (with their representations).
    fn select_many(
        &mut self,
        document_id: processcraft_core::ids::DocumentId,
        diagram_id: ElementSelector,
        elements: Vec<ElementSelector>,
        representations: Vec<ElementSelector>,
    ) {
        self.select(elements.first().cloned());
        if self.selected != elements.first().cloned() {
            return;
        }
        self.ctl.session.set_selection(
            elements
                .iter()
                .cloned()
                .map(|xml_id| processcraft_core::ids::ElementKey {
                    document_id,
                    xml_id,
                })
                .collect(),
        );
        self.multi = elements;
        self.representation_selection = representations;
        self.representation_diagram = Some(diagram_id);
        self.canvas.revealed = self.selected.clone();
    }
    /// Select every shape and connection of the current diagram (Ctrl+A).
    pub(super) fn select_all(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let mut elements: Vec<ElementSelector> = Vec::new();
        for element in diagram
            .shapes
            .iter()
            .map(|s| &s.element)
            .chain(diagram.edges.iter().map(|e| &e.element))
        {
            if !elements.contains(element) {
                elements.push(element.clone());
            }
        }
        let representations = diagram
            .shapes
            .iter()
            .map(|s| s.id.selector())
            .chain(diagram.edges.iter().map(|e| e.id.selector()))
            .collect();
        if !elements.is_empty() {
            self.select_many(d.id, diagram.id.selector(), elements, representations);
        }
    }
    fn copy_selection(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<processcraft_core::clipboard::Clipboard> {
        let snap = self.ctl.session.snapshot();
        let d = snap.project.documents.get(self.doc)?;
        let diagram = self.current_diagram(d)?;
        match processcraft_core::clipboard::copy(d, &diagram.id.selector(), &self.multi) {
            Ok(clipboard) => {
                // The OS clipboard gets a readable text summary; pasting uses the in-process copy.
                let text: Vec<String> = clipboard
                    .nodes
                    .iter()
                    .map(|(_, n)| n.name.clone().unwrap_or_else(|| n.id.to_string()))
                    .collect();
                ctx.copy_text(text.join("\n"));
                Some(clipboard)
            }
            Err(e) => {
                self.ctl.error = format!("Could not copy the selection: {e}.");
                None
            }
        }
    }
    /// Paste centered on the model point `at`, or offset by 20/20 from the copied position.
    fn paste_clipboard(
        &mut self,
        clipboard: &processcraft_core::clipboard::Clipboard,
        at: Option<(f64, f64)>,
    ) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let (dx, dy) = match (at, clipboard.bounds()) {
            (Some((x, y)), Some(b)) => (
                (x - b.x - b.width / 2.0).round(),
                (y - b.y - b.height / 2.0).round(),
            ),
            _ => (20.0, 20.0),
        };
        let Some(fallback) = self.process_id.clone().or_else(|| {
            processcraft_core::queries::scopes(d)
                .first()
                .map(|s| s.id.selector())
        }) else {
            self.ctl.error = "Create a process before pasting.".into();
            return;
        };
        let diagram_id = diagram.id.selector();
        match processcraft_core::clipboard::paste(d, &diagram_id, &fallback, clipboard, dx, dy) {
            Ok(pasted) => {
                if self.ctl.mutate(pasted.mutations) {
                    let snap = self.ctl.session.snapshot();
                    let Some(d) = snap.project.documents.get(self.doc) else {
                        return;
                    };
                    let representations = self
                        .current_diagram(d)
                        .map(|diagram| {
                            diagram
                                .shapes
                                .iter()
                                .filter(|s| pasted.selection.contains(&s.element))
                                .map(|s| s.id.selector())
                                .chain(
                                    diagram
                                        .edges
                                        .iter()
                                        .filter(|e| pasted.selection.contains(&e.element))
                                        .map(|e| e.id.selector()),
                                )
                                .collect()
                        })
                        .unwrap_or_default();
                    self.select_many(d.id, diagram_id, pasted.selection, representations);
                }
            }
            Err(e) => self.ctl.error = format!("Could not paste: {e}."),
        }
    }
    fn cut_selection(&mut self, ctx: &egui::Context) {
        if let Some(clipboard) = self.copy_selection(ctx) {
            self.canvas.clipboard = Some(clipboard);
            self.delete();
        }
    }
    fn copy_to_clipboard(&mut self, ctx: &egui::Context) {
        if let Some(clipboard) = self.copy_selection(ctx) {
            self.canvas.clipboard = Some(clipboard);
        }
    }
    /// Paste at the pointer when it is over the canvas, otherwise at the viewport center.
    fn paste_here(&mut self, ctx: &egui::Context) {
        let Some(clipboard) = self.canvas.clipboard.clone() else {
            self.ctl.error = "Nothing to paste; copy diagram elements first.".into();
            return;
        };
        let at = match (self.canvas.rect, ctx.input(|i| i.pointer.hover_pos())) {
            (Some(rect), Some(pos)) if rect.contains(pos) => {
                to_model(rect, self.camera, self.zoom, pos)
            }
            _ => self.viewport_center(),
        };
        self.paste_clipboard(&clipboard, Some(at));
    }
    /// Duplicate the selection next to itself without touching the clipboard (Ctrl+D).
    fn duplicate_selection(&mut self) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        match processcraft_core::clipboard::copy(d, &diagram.id.selector(), &self.multi) {
            Ok(clipboard) => self.paste_clipboard(&clipboard, None),
            Err(e) => self.ctl.error = format!("Could not duplicate the selection: {e}."),
        }
    }
    fn open_search(&mut self) {
        self.canvas.search = Some(String::new());
        self.canvas.search_focus = true;
    }
    /// Ctrl+A/C/X/V/D/F; the caller has already ruled out focused text fields.
    fn edit_shortcuts(&mut self, ctx: &egui::Context, editable: bool) {
        use egui::{Key, Modifiers};
        // egui-winit turns Ctrl+C/X/V into clipboard events; plain key events cover the rest.
        let (copy, cut, paste) = ctx.input_mut(|i| {
            let mut found = (false, false, false);
            i.events.retain(|event| match event {
                egui::Event::Copy => {
                    found.0 = true;
                    false
                }
                egui::Event::Cut => {
                    found.1 = true;
                    false
                }
                egui::Event::Paste(_) => {
                    found.2 = true;
                    false
                }
                _ => true,
            });
            (
                found.0 | i.consume_key(Modifiers::CTRL, Key::C),
                found.1 | i.consume_key(Modifiers::CTRL, Key::X),
                found.2 | i.consume_key(Modifiers::CTRL, Key::V),
            )
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.select_all();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
            self.open_search();
        }
        let duplicate = ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D));
        if copy && !self.multi.is_empty() {
            self.copy_to_clipboard(ctx);
        }
        if editable && cut && !self.multi.is_empty() {
            self.cut_selection(ctx);
        }
        if editable && paste {
            self.paste_here(ctx);
        }
        if editable && duplicate && !self.multi.is_empty() {
            self.duplicate_selection();
        }
    }
    fn arrange(
        &mut self,
        prepare: impl FnOnce(
            &processcraft_core::model::BpmnDocument,
            &ElementSelector,
            &[ElementSelector],
        ) -> Result<processcraft_core::geometry::PreparedMove, &'static str>,
    ) {
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let diagram_id = diagram.id.selector();
        let shapes: Vec<ElementSelector> = if self.representation_diagram.as_ref()
            == Some(&diagram_id)
            && !self.representation_selection.is_empty()
        {
            self.representation_selection.clone()
        } else {
            diagram
                .shapes
                .iter()
                .filter(|s| self.multi.contains(&s.element))
                .map(|s| s.id.selector())
                .collect()
        };
        match prepare(d, &diagram_id, &shapes) {
            Ok(prepared) => {
                if self.ctl.apply(prepared.request) && !prepared.warnings.is_empty() {
                    self.ctl.error = prepared.warnings.join("; ");
                }
            }
            Err(e) => self.ctl.error = format!("Could not arrange the selection: {e}."),
        }
    }
    /// Align and Distribute submenus (Edit menu and canvas toolbar).
    pub(super) fn arrange_menus(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Align", |ui| {
            for (alignment, label) in ALIGNMENTS {
                if ui.button(label).clicked() {
                    self.arrange(|d, diagram, shapes| {
                        processcraft_core::arrange::prepare_align(d, diagram, shapes, alignment)
                    });
                    ui.close();
                }
            }
        });
        ui.menu_button("Distribute", |ui| {
            for (distribution, label) in [
                (Distribution::Horizontal, "Distribute horizontally"),
                (Distribution::Vertical, "Distribute vertically"),
            ] {
                if ui.button(label).clicked() {
                    self.arrange(|d, diagram, shapes| {
                        processcraft_core::arrange::prepare_distribute(
                            d,
                            diagram,
                            shapes,
                            distribution,
                        )
                    });
                    ui.close();
                }
            }
        });
    }
    /// Clipboard, selection, search, and arrange entries of the Edit menu.
    pub(super) fn edit_menu_items(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let selected = !self.multi.is_empty();
        ui.separator();
        let item = |ui: &mut egui::Ui, enabled: bool, label: &str, shortcut: &str| {
            ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
                .clicked()
        };
        if item(ui, selected, "Cut", "Ctrl+X") {
            self.cut_selection(&ctx);
            ui.close();
        }
        if item(ui, selected, "Copy", "Ctrl+C") {
            self.copy_to_clipboard(&ctx);
            ui.close();
        }
        if item(ui, self.canvas.clipboard.is_some(), "Paste", "Ctrl+V") {
            self.paste_here(&ctx);
            ui.close();
        }
        if item(ui, selected, "Duplicate", "Ctrl+D") {
            self.duplicate_selection();
            ui.close();
        }
        if item(ui, true, "Select all", "Ctrl+A") {
            self.select_all();
            ui.close();
        }
        if item(ui, true, "Find element…", "Ctrl+F") {
            self.open_search();
            ui.close();
        }
        ui.label(
            egui::RichText::new("Move selection: arrows 1 px, Shift+arrows 10 px")
                .small()
                .weak(),
        );
        ui.separator();
        self.arrange_menus(ui);
    }
    /// Scroll the canvas while a drag nears its border, keeping the dragged content under the
    /// pointer.
    fn auto_scroll(&mut self, ctx: &egui::Context) {
        let active = self.drag.is_some()
            || matches!(
                self.canvas.gesture,
                Some(Gesture::BoxSelect(_) | Gesture::Connect { .. })
            );
        let (Some(rect), Some(pos)) = (self.canvas.rect, ctx.input(|i| i.pointer.latest_pos()))
        else {
            return;
        };
        if !active || !ctx.input(|i| i.pointer.primary_down()) {
            return;
        }
        let step = auto_scroll_step(rect, pos);
        if step == Vec2::ZERO {
            return;
        }
        self.camera.0 += (step.x / self.zoom) as f64;
        self.camera.1 += (step.y / self.zoom) as f64;
        if let Some(drag) = &mut self.drag {
            drag.origin -= step;
        }
        if let Some(Gesture::BoxSelect(start)) = &mut self.canvas.gesture {
            *start -= step;
        }
        ctx.request_repaint();
    }
    /// Ctrl+F overlay: filter elements of the current diagram by name or ID; Enter or a click
    /// selects the element and scrolls it into the center.
    fn search_ui(&mut self, ctx: &egui::Context) {
        let (Some(rect), Some(mut query)) = (self.canvas.rect, self.canvas.search.take()) else {
            return;
        };
        let snap = self.ctl.session.snapshot();
        let Some(d) = snap.project.documents.get(self.doc) else {
            return;
        };
        let Some(diagram) = self.current_diagram(d) else {
            return;
        };
        let needle = query.to_lowercase();
        let mut results: Vec<(ElementSelector, String, Bounds)> = Vec::new();
        let edge_bounds = |points: &[processcraft_core::model::Point]| {
            let x0 = points.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let y0 = points.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let x1 = points.iter().map(|p| p.x).fold(f64::MIN, f64::max);
            let y1 = points.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            Bounds {
                x: x0,
                y: y0,
                width: (x1 - x0).max(0.0),
                height: (y1 - y0).max(0.0),
            }
        };
        let candidates = diagram
            .shapes
            .iter()
            .map(|s| (&s.element, s.bounds.clone()))
            .chain(
                diagram
                    .edges
                    .iter()
                    .filter(|e| !e.waypoints.is_empty())
                    .map(|e| (&e.element, edge_bounds(&e.waypoints))),
            );
        for (element, bounds) in candidates {
            if results.len() >= 12 || results.iter().any(|(id, _, _)| id == element) {
                continue;
            }
            let name = processcraft_core::queries::element_name(d, element)
                .unwrap_or_default()
                .to_owned();
            let id = element.to_string();
            if needle.is_empty()
                || name.to_lowercase().contains(&needle)
                || id.to_lowercase().contains(&needle)
            {
                let label = if name.is_empty() {
                    id
                } else {
                    format!("{name}  ({id})")
                };
                results.push((element.clone(), label, bounds));
            }
        }
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(egui::Id::unique("processcraft-canvas-search"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_top() + Vec2::new(8.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(320.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .hint_text("Find element by name or ID")
                            .desired_width(f32::INFINITY),
                    );
                    if self.canvas.search_focus {
                        field.request_focus();
                        self.canvas.search_focus = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    } else if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = results.first().map(|(id, _, b)| (id.clone(), b.clone()));
                        close = chosen.is_some();
                    }
                    if results.is_empty() {
                        ui.weak("No matching elements");
                    }
                    for (id, label, bounds) in &results {
                        if ui.selectable_label(false, label).clicked() {
                            chosen = Some((id.clone(), bounds.clone()));
                            close = true;
                        }
                    }
                });
            });
        if !close {
            self.canvas.search = Some(query);
        }
        if let Some((element, b)) = chosen {
            self.select(Some(element.clone()));
            if self.selected.as_ref() == Some(&element) {
                let (w, h) = (
                    self.canvas.size.x as f64 / self.zoom as f64,
                    self.canvas.size.y as f64 / self.zoom as f64,
                );
                self.camera = (
                    b.x + b.width / 2.0 - w / 2.0,
                    b.y + b.height / 2.0 - h / 2.0,
                );
                self.canvas.revealed = self.selected.clone();
            }
        }
    }
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
    fn zoom_keeps_anchor_fixed() {
        let anchor = Vec2::new(200.0, 100.0);
        let before = (10.0 + 200.0 / 1.0, 20.0 + 100.0 / 1.0);
        let (camera, zoom) = zoom_at((10.0, 20.0), 1.0, 2.0, anchor);
        assert_eq!(zoom, 2.0);
        let after = (camera.0 + 200.0 / 2.0, camera.1 + 100.0 / 2.0);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        assert_eq!(zoom_at((0.0, 0.0), 1.0, 100.0, anchor).1, MAX_ZOOM);
    }

    #[test]
    fn stale_texture_is_scaled_and_offset() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(100.0, 50.0));
        let view = TextureView {
            camera: (0.0, 0.0),
            zoom: 1.0,
            size: Vec2::new(100.0, 50.0),
        };
        assert_eq!(texture_rect(rect, view, (0.0, 0.0), 1.0), rect);
        let panned = texture_rect(rect, view, (5.0, 0.0), 2.0);
        assert_eq!(panned.min, Pos2::new(0.0, 10.0));
        assert_eq!(panned.size(), Vec2::new(200.0, 100.0));
    }

    #[test]
    fn resize_moves_only_the_dragged_corner_and_keeps_a_minimum() {
        let start = b(0.0, 0.0, 100.0, 80.0);
        assert_eq!(
            resize_bounds(&start, 2, 20.0, 10.0),
            b(0.0, 0.0, 120.0, 90.0)
        );
        assert_eq!(
            resize_bounds(&start, 0, 10.0, 10.0),
            b(10.0, 10.0, 90.0, 70.0)
        );
        assert_eq!(
            resize_bounds(&start, 1, -500.0, 0.0),
            b(0.0, 0.0, 20.0, 80.0)
        );
        assert_eq!(
            resize_bounds(&start, 3, 0.0, -500.0),
            b(0.0, 0.0, 100.0, 20.0)
        );
    }

    #[test]
    fn adding_after_a_selected_node_appends_and_connects_by_id() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let diagram = &document.diagrams[0];
        let bounds = |id: &ElementSelector| {
            diagram
                .shapes
                .iter()
                .find(|s| &s.element == id)
                .unwrap()
                .bounds
                .clone()
        };
        assert!(bounds(&task).x >= bounds(&start).x + bounds(&start).width);
        assert_eq!(diagram.edges.len(), 1);
        app.select(None);
        app.add(NodeKind::EndEvent);
        let end = app.selected.clone().unwrap();
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0]
                .edges
                .len(),
            1
        );
        assert!(app.connect_elements(task, end, None));
        let snap = app.ctl.session.snapshot();
        let edges = &snap.project.documents[0].diagrams[0].edges;
        assert_eq!(edges.len(), 2);
        assert!(
            edges[1]
                .waypoints
                .windows(2)
                .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
        );
    }

    #[test]
    fn empty_drag_pans_and_ctrl_plus_zooms_the_canvas() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        };
        run(&mut app, vec![]);
        let (camera, zoom) = (app.camera, app.zoom);
        let (from, to) = (Pos2::new(400., 300.), Pos2::new(500., 300.));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(
            &mut app,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        run(&mut app, vec![egui::Event::PointerMoved(to)]);
        run(&mut app, vec![button(to, false)]);
        assert!(app.camera.0 < camera.0);
        assert_eq!(app.zoom, zoom);
        run(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Plus,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
        );
        assert!((app.zoom - zoom * ZOOM_STEP).abs() < 1e-5);
        assert_eq!(context.zoom_factor(), 1.0);
    }

    #[test]
    fn dragging_the_connect_handle_onto_a_task_creates_a_flow() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        let start = app.selected.clone().unwrap();
        app.select(None);
        app.add(NodeKind::Task);
        let task = app.selected.clone().unwrap();
        app.select(Some(start.clone()));
        let context = egui::Context::default();
        let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| app.canvas_ui(ui),
            );
            output.textures_delta.clear();
        };
        // Wait for the background render that supplies hit regions.
        for _ in 0..200 {
            run(&mut app, vec![]);
            if !app.hits.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!app.hits.is_empty(), "render produced no hit regions");
        let snap = app.ctl.session.snapshot();
        let diagram = &snap.project.documents[0].diagrams[0];
        let bounds = |id: &ElementSelector| {
            diagram
                .shapes
                .iter()
                .find(|s| &s.element == id)
                .unwrap()
                .bounds
                .clone()
        };
        let edges_before = diagram.edges.len();
        let (rect, camera, zoom) = (app.canvas.rect.unwrap(), app.camera, app.zoom);
        let source = to_screen(rect, camera, zoom, &bounds(&start));
        let handle = Pos2::new(source.right() + 16.0, source.center().y);
        let target = to_screen(rect, camera, zoom, &bounds(&task)).center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![egui::Event::PointerMoved(handle)]);
        run(&mut app, vec![button(handle, true)]);
        for step in 1..=5 {
            let pos = handle + (target - handle) * (step as f32 / 5.0);
            run(&mut app, vec![egui::Event::PointerMoved(pos)]);
        }
        run(&mut app, vec![button(target, false)]);
        run(&mut app, vec![]);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        assert_eq!(document.diagrams[0].edges.len(), edges_before + 1);
        assert!(document.processes[0].flows.iter().any(|f| {
            matches!(&f.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == start)
                && matches!(&f.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == task)
        }));
    }

    #[test]
    fn ctrl_a_selects_the_diagram_and_ctrl_d_duplicates_it() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::StartEvent);
        app.add(NodeKind::Task);
        let context = egui::Context::default();
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.canvas_shortcuts(ui.ctx());
                    app.canvas_ui(ui)
                },
            );
            output.textures_delta.clear();
        };
        run(&mut app, vec![key(egui::Key::A)]);
        // Two shapes and the connecting flow.
        assert_eq!(app.multi.len(), 3);
        let first_x = |app: &ProcessCraft| {
            app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0]
                .bounds
                .x
        };
        // Arrow keys move the selection by 1 px.
        let x = first_x(&app);
        run(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(first_x(&app), x + 1.0, "{}", app.ctl.error);
        let before = app.ctl.session.snapshot();
        run(&mut app, vec![key(egui::Key::D)]);
        let after = app.ctl.session.snapshot();
        assert_eq!(after.undo_count, before.undo_count + 1);
        let process = &after.project.documents[0].processes[0];
        assert_eq!((process.nodes.len(), process.flows.len()), (4, 2));
        let diagram = &after.project.documents[0].diagrams[0];
        assert_eq!((diagram.shapes.len(), diagram.edges.len()), (4, 2));
        // The copies are selected, and offset by 20/20 from the originals.
        assert_eq!(app.multi.len(), 3);
        let shape = |id: &ElementSelector| diagram.shapes.iter().find(|s| &s.element == id);
        let copy = app.multi.iter().find_map(shape).unwrap();
        assert_eq!(copy.bounds.x, diagram.shapes[0].bounds.x + 20.0);
        assert_eq!(copy.bounds.y, diagram.shapes[0].bounds.y + 20.0);
    }
    #[test]
    fn auto_scroll_only_near_the_border() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        assert_eq!(auto_scroll_step(rect, Pos2::new(200.0, 150.0)), Vec2::ZERO);
        assert_eq!(
            auto_scroll_step(rect, Pos2::new(5.0, 295.0)),
            Vec2::new(-AUTO_SCROLL_STEP, AUTO_SCROLL_STEP)
        );
        assert_eq!(auto_scroll_step(rect, Pos2::new(-5.0, 150.0)), Vec2::ZERO);
    }

    #[test]
    fn vacant_bounds_avoids_shapes_but_not_containers() {
        let shape = |bounds: Bounds| Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: "S".parse::<processcraft_core::ids::XmlId>().unwrap().into(),
            element: "E".parse().unwrap(),
            bounds,
            label: None,
            style: None,
        };
        let free = vacant_bounds(&[], (100.0, 100.0), 100.0, 80.0);
        assert_eq!(free, b(50.0, 60.0, 100.0, 80.0));
        let pool = shape(b(-1000.0, -1000.0, 3000.0, 3000.0));
        assert_eq!(
            vacant_bounds(std::slice::from_ref(&pool), (100.0, 100.0), 100.0, 80.0),
            free
        );
        let blocker = shape(b(80.0, 80.0, 100.0, 80.0));
        let moved = vacant_bounds(&[pool, blocker.clone()], (100.0, 100.0), 100.0, 80.0);
        assert_ne!(moved, free);
        let o = &blocker.bounds;
        assert!(
            moved.x + moved.width + 20.0 <= o.x
                || o.x + o.width + 20.0 <= moved.x
                || moved.y + moved.height + 20.0 <= o.y
                || o.y + o.height + 20.0 <= moved.y
        );
    }
}
