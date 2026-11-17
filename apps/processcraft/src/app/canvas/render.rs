//! The canvas frame: one `canvas_render` pass split into the sections that run in order each frame.
use super::*;

/// What every section of one canvas frame shares. `camera` and `zoom` start as the frame's view and
/// are updated by pan, zoom and wheel handling; `canvas_render` stores them back.
pub(super) struct CanvasFrame<'a> {
    pub(super) ctx: egui::Context,
    pub(super) snap: &'a processcraft_core::commands::Snapshot,
    pub(super) d: &'a Arc<processcraft_core::model::BpmnDocument>,
    pub(super) diagram: &'a processcraft_core::model::Diagram,
    pub(super) response: egui::Response,
    pub(super) painter: egui::Painter,
    pub(super) palette: crate::theme::Palette,
    /// Canvas selection is blue; amber stays the app's identity colour.
    pub(super) accent: Color32,
    pub(super) rect: Rect,
    pub(super) size: Vec2,
    pub(super) hover: Option<Pos2>,
    pub(super) camera: (f64, f64),
    pub(super) zoom: f32,
    pub(super) interactive: bool,
    pub(super) editable: bool,
    /// A layout preview replaces the document (and disables interaction).
    pub(super) previewing: bool,
}

/// What the selection section found, for the sections after it.
pub(super) struct Selection<'a> {
    /// The single selected shape when its handles and pad are shown.
    single: Option<&'a Shape>,
    rule: Option<processcraft_core::rules::MinSize>,
    selected_edge: Option<&'a processcraft_core::model::Edge>,
    bend_handles: Vec<(EdgePart, Pos2)>,
    handles: Vec<((i8, i8), Pos2)>,
    hovered_handle: Option<(i8, i8)>,
}

impl ProcessCraft {
    pub(super) fn canvas_render(&mut self, ui: &mut egui::Ui, size: Vec2, interactive: bool) {
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
                .or_else(|| self.current_document(&snap))
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
        if let Some(d) = preview.as_ref().or_else(|| self.current_document(&snap)) {
            if let Some(diagram) = d
                .diagrams
                .iter()
                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
            {
                let mut frame = CanvasFrame {
                    hover: response.hover_pos(),
                    ctx,
                    snap: &snap,
                    d,
                    diagram,
                    response,
                    painter: painter.clone(),
                    palette,
                    accent: palette.selection,
                    rect,
                    size,
                    camera,
                    zoom,
                    interactive,
                    editable,
                    previewing: preview.is_some(),
                };
                self.schedule_raster(&frame);
                self.paint_texture(&frame);
                if interactive {
                    let sel = self.paint_selection(&frame);
                    self.paint_hover_outline(&frame, &sel);
                    self.paint_resize_handles(&frame, &sel);
                    let (pad, pad_owner) = self.paint_context_pad(&frame, &sel);
                    let click_consumed = self.click_to_connect(&frame);
                    self.begin_gesture(&frame, &sel, &pad, click_consumed);
                    let pad_pressed = self.pad_click(&frame, &sel, &pad, pad_owner, click_consumed);
                    let on_handle = self.select_click(&frame, &sel, click_consumed, pad_pressed);
                    self.double_click(&frame, &sel, on_handle);
                    self.advance_move_drag(&frame);
                    self.paint_drag_preview(&frame);
                    self.finish_move_drag(&frame);
                    self.paint_gestures(&mut frame);
                    self.finish_gesture(&frame);
                    self.palette_drop(&frame);
                    self.label_editor(&frame, ui);
                    self.wheel_zoom(&mut frame);
                }
                (camera, zoom) = (frame.camera, frame.zoom);
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
    fn schedule_raster(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            snap,
            d,
            diagram,
            size,
            camera,
            zoom,
            interactive,
            previewing,
            ..
        } = *f;
        let scene = format!(
            "{}:{}:{}:{:?}:{}",
            self.ctl.session.id(),
            if previewing {
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
        let overlay = if previewing {
            None
        } else {
            self.canvas
                .overlay_for(&self.ctl.session.id(), snap.generation, &snap.project, d.id)
        };
        let overlay_tag = match (&overlay, self.canvas.overlay) {
            (Some(_), Some(spec)) => format!("{}:{:?}", spec.definition, spec.mode),
            _ => String::new(),
        };
        // Everything but the camera: a change needs a new raster now.
        let base = format!(
            "{scene}:{overlay_tag}:{}:{}:{}:{}",
            snap.generation,
            size.x,
            size.y,
            ctx.pixels_per_point()
        );
        let now = std::time::Instant::now();
        match self.canvas.moved {
            Some((c, z, _)) if (c, z) == (camera, zoom) => {}
            _ => self.canvas.moved = Some((camera, zoom, now)),
        }
        let still = self.canvas.moved.map_or(SETTLE, |(_, _, at)| now - at);
        // Another document, diagram, or theme: the previous frame is not a valid placeholder.
        if self.canvas.scene != scene {
            self.canvas.scene = scene;
            self.canvas.pending.clear();
            self.canvas.submitted = None;
            // Hidden without `view`; the texture handle is reused for the next frame.
            self.canvas.view = None;
            if interactive {
                self.hits.clear();
            }
        }
        let submitted = self.canvas.submitted.as_ref().map(|(_, view)| *view);
        if self.render_key != base || needs_raster(submitted, camera, zoom, size, still >= SETTLE) {
            let texture = raster_view(camera, zoom, size, ctx.pixels_per_point());
            let view = Viewport {
                x: texture.camera.0,
                y: texture.camera.1,
                width: texture.size.x as f64 / zoom as f64,
                height: texture.size.y as f64 / zoom as f64,
                zoom: (zoom * ctx.pixels_per_point()).clamp(MIN_ZOOM, MAX_ZOOM),
            };
            let key = format!("{base}:{}:{}:{zoom}", camera.0, camera.1);
            self.renderer.submit(crate::jobs::RenderJob {
                key: key.clone(),
                document: d.clone(),
                diagram: diagram.id.selector(),
                appearance: match (self.theme == Theme::Dark, interactive) {
                    (false, false) => Appearance::Light,
                    (true, false) => Appearance::Dark,
                    (false, true) => Appearance::CanvasLight,
                    (true, true) => Appearance::CanvasDark,
                },
                view,
                overlay,
                context: ctx.clone(),
            });
            self.render_failed = false;
            self.render_key = base;
            self.canvas.pending.push((key.clone(), texture, now));
            self.canvas.submitted = Some((key, texture));
            if self.canvas.pending.len() > 32 {
                self.canvas.pending.remove(0);
            }
        } else if still < SETTLE && submitted.is_some_and(|v| v.request != (camera, zoom)) {
            // Pan/zoom is served by the GPU; re-raster at exact scale once input settles.
            ctx.request_repaint_after(SETTLE - still);
        }
        // Results arrive in submission order; any pending one is newer than what is shown.
        while let Some(result) = self.renderer.poll() {
            let Some(index) = self
                .canvas
                .pending
                .iter()
                .position(|(pending, ..)| *pending == result.key)
            else {
                continue;
            };
            let (_, view, submitted_at) = self.canvas.pending[index];
            self.canvas.pending.drain(..=index);
            match result.result {
                Ok((image, hits)) => {
                    let started = std::time::Instant::now();
                    let pixels = image.size;
                    match &mut self.texture {
                        Some(texture) => texture.set(image, Default::default()),
                        None => {
                            self.texture =
                                Some(ctx.load_texture("diagram", image, Default::default()))
                        }
                    }
                    if trace_canvas() {
                        eprintln!(
                            "canvas: {}x{} px shown {:.1} ms after submit; texture update {:.2} ms on the UI thread",
                            pixels[0],
                            pixels[1],
                            (started - submitted_at).as_secs_f64() * 1000.0,
                            started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    self.canvas.view = Some(view);
                    if interactive {
                        self.hits = hits;
                    }
                }
                Err(e) => {
                    let current = self.canvas.submitted.as_ref().map(|(key, _)| key);
                    if current.is_some_and(|key| crate::jobs::is_current(&result.key, key)) {
                        self.render_failed = true;
                        self.ctl.error = e;
                    }
                }
            }
        }
    }
    fn paint_texture(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            d,
            diagram,
            ref painter,
            palette,
            rect,
            camera,
            zoom,
            interactive,
            ..
        } = *f;
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
                rect.center() - egui::vec2(0.0, 44.0),
                egui::Align2::CENTER_CENTER,
                "Build your process",
                crate::theme::semibold(20.0),
                palette.text,
            );
            painter.text(
                rect.center() - egui::vec2(0.0, 16.0),
                egui::Align2::CENTER_CENTER,
                "Pick an element from the palette, drag it in, or press 1 for a start event.",
                egui::FontId::proportional(12.5),
                palette.muted,
            );
            if interactive {
                painter.text(
                            rect.center() + egui::vec2(0.0, 58.0),
                            egui::Align2::CENTER_CENTER,
                            "Drag empty space to pan · Shift-drag to select · Ctrl+wheel to zoom · Double-click, E or F2 to rename · Select an element for its context pad",
                            egui::FontId::proportional(11.0),
                            palette.muted,
                        );
            }
        }
    }
    fn paint_selection<'a>(&mut self, f: &CanvasFrame<'a>) -> Selection<'a> {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref painter,
            palette,
            accent,
            rect,
            hover,
            camera,
            zoom,
            editable,
            ..
        } = *f;
        // Canvas selection is blue; amber stays the app's identity colour.
        if self.canvas.space_tool {
            painter.text(
                        rect.left_top() + egui::vec2(12.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        "Space tool: drag to insert or remove space (Ctrl: shapes before the line) · S or Esc to leave",
                        egui::FontId::proportional(12.0),
                        accent,
                    );
        }
        let diagram_id = diagram.id.selector();
        let by_representation = self.representation_diagram.as_ref() == Some(&diagram_id)
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
            let outline = to_screen(rect, camera, zoom, &shape.bounds).expand(3.0);
            painter.rect_filled(outline, 6.0, accent.gamma_multiply(0.08));
            painter.rect_stroke(
                outline,
                6.0,
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
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(3.0, accent.gamma_multiply(0.85)),
            ));
        }
        // Handles of a single selected shape: resize corners and the context pad,
        // hidden while a gesture or label edit is running.
        let single = (editable && selected_shapes.len() == 1 && self.drag.is_none())
            .then(|| selected_shapes[0]);
        // Resize handles only where the rules allow (bpmn-js `canResize`).
        let rule = single.and_then(|shape| processcraft_core::manipulate::resize_rule(d, shape));
        // Bendpoint and segment handles of a single selected edge.
        let edge_selection: Vec<_> = diagram
            .edges
            .iter()
            .filter(|e| {
                if by_representation {
                    self.representation_selection.contains(&e.id.selector())
                } else {
                    self.multi.contains(&e.element)
                }
            })
            .collect();
        let selected_edge = (editable
            && selected_shapes.is_empty()
            && edge_selection.len() == 1
            && self.drag.is_none())
        .then(|| edge_selection[0]);
        let bend_handles = selected_edge
            .map(|edge| edge_handles(&edge_screen(rect, camera, zoom, edge)))
            .unwrap_or_default();
        if !matches!(self.canvas.gesture, Some(Gesture::Bendpoint { .. })) {
            for (part, pos) in &bend_handles {
                match part {
                    EdgePart::Point(_) => {
                        painter.circle_filled(*pos, 4.0, palette.base);
                        painter.circle_stroke(*pos, 4.0, egui::Stroke::new(1.5, accent));
                    }
                    EdgePart::Segment(_) => {
                        painter.rect_filled(
                            Rect::from_center_size(*pos, Vec2::splat(6.0)),
                            1.0,
                            accent,
                        );
                    }
                }
            }
        }
        // Edge handles (ends included) win over the shape under them.
        if self.canvas.gesture.is_none()
            && hover.is_some_and(|pos| bend_handles.iter().any(|(_, h)| h.distance(pos) <= 8.0))
        {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        let handles = single
            .zip(rule)
            .map(|(shape, rule)| {
                resize_handles(
                    to_screen(rect, camera, zoom, &shape.bounds),
                    rule.keep_aspect,
                )
            })
            .unwrap_or_default();
        let hovered_handle = hover
            .filter(|_| self.canvas.gesture.is_none())
            .and_then(|pos| handle_at(&handles, pos));
        if let Some(h) = hovered_handle {
            ctx.set_cursor_icon(resize_cursor(h));
        }
        Selection {
            single,
            rule,
            selected_edge,
            bend_handles,
            handles,
            hovered_handle,
        }
    }
    fn paint_hover_outline(&mut self, f: &CanvasFrame<'_>, sel: &Selection<'_>) {
        let CanvasFrame {
            ref ctx,
            ref painter,
            accent,
            rect,
            hover,
            camera,
            zoom,
            ..
        } = *f;
        let Selection { hovered_handle, .. } = *sel;
        // Hover outline of the shape or edge under the pointer (not the selection).
        if let Some(pos) = hover.filter(|_| {
            hovered_handle.is_none()
                && self.canvas.gesture.is_none()
                && self.drag.is_none()
                && self.canvas.palette_drag.is_none()
        }) {
            let (x, y) = to_model(rect, camera, zoom, pos);
            let alt = ctx.input(|i| i.modifiers.alt);
            if let Some(h) = self.hit_at(x, y, zoom, alt)
                && !h.is_label
                && !self.multi.contains(&h.key.xml_id)
            {
                let stroke = egui::Stroke::new(1.5, accent.gamma_multiply(0.5));
                match &h.geometry {
                    HitGeometry::Edge(points) => {
                        painter.add(egui::Shape::line(
                            points
                                .iter()
                                .map(|p| {
                                    Pos2::new(
                                        rect.left() + (p.x - camera.0) as f32 * zoom,
                                        rect.top() + (p.y - camera.1) as f32 * zoom,
                                    )
                                })
                                .collect(),
                            egui::Stroke::new(3.0, stroke.color),
                        ));
                    }
                    HitGeometry::Shape(b)
                    | HitGeometry::Ellipse(b)
                    | HitGeometry::Diamond(b)
                    | HitGeometry::Hexagon(b)
                    | HitGeometry::DataObject(b)
                    | HitGeometry::DataStore(b) => {
                        painter.rect_stroke(
                            to_screen(rect, camera, zoom, b).expand(3.0),
                            6.0,
                            stroke,
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            }
        }
    }
    fn paint_resize_handles(&mut self, f: &CanvasFrame<'_>, sel: &Selection<'_>) {
        let CanvasFrame {
            ref painter,
            palette,
            accent,
            ..
        } = *f;
        let Selection {
            hovered_handle,
            ref handles,
            ..
        } = *sel;
        for (h, pos) in handles {
            let handle = Rect::from_center_size(*pos, Vec2::splat(8.0));
            let fill = if hovered_handle == Some(*h) {
                accent
            } else {
                palette.base
            };
            painter.rect_filled(handle, 2.0, fill);
            painter.rect_stroke(
                handle,
                2.0,
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Inside,
            );
        }
    }
    fn paint_context_pad(
        &mut self,
        f: &CanvasFrame<'_>,
        sel: &Selection<'_>,
    ) -> (Vec<(PadAction, Rect)>, Option<ElementSelector>) {
        let CanvasFrame {
            ref ctx,
            d,
            ref response,
            ref painter,
            palette,
            accent,
            rect,
            hover,
            camera,
            zoom,
            ..
        } = *f;
        let Selection {
            single,
            selected_edge,
            ..
        } = *sel;
        let mut pad = Vec::new();
        let pad_idle = self.canvas.gesture.is_none()
            && self.canvas.label_edit.is_none()
            && self.canvas.connect_click.is_none();
        // The element the pad belongs to: the single selected shape or flow.
        let mut pad_owner = None;
        if let Some(shape) = single
            && pad_idle
        {
            let screen = to_screen(rect, camera, zoom, &shape.bounds);
            pad = context_pad(d, &shape.element, screen, rect);
            pad_owner = Some(shape.element.clone());
        } else if let Some(edge) = selected_edge
            && pad_idle
            && processcraft_core::queries::find_flow(d, &edge.element)
                .is_some_and(|f| f.kind == FlowKind::Sequence)
            && let Some(mid) = edge_screen(rect, camera, zoom, edge).get(edge.waypoints.len() / 2)
        {
            pad = flow_pad(d, &edge.element, *mid, rect);
            pad_owner = Some(edge.element.clone());
        }
        for (action, r) in &pad {
            {
                let hovered = hover.is_some_and(|pos| r.contains(pos));
                painter.rect_filled(
                    *r,
                    crate::theme::RADIUS,
                    if hovered {
                        palette.selection_soft
                    } else {
                        palette.base
                    },
                );
                painter.rect_stroke(
                    *r,
                    crate::theme::RADIUS,
                    egui::Stroke::new(1.0, palette.border),
                    egui::StrokeKind::Inside,
                );
                paint_pad_icon(
                    painter,
                    *action,
                    *r,
                    if hovered { accent } else { palette.text },
                );
                if hovered {
                    if *action == PadAction::Connect {
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                    response.clone().on_hover_text_at_pointer(action.label());
                }
            }
        }
        (pad, pad_owner)
    }
    fn click_to_connect(&mut self, f: &CanvasFrame<'_>) -> bool {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            ref painter,
            accent,
            rect,
            hover,
            camera,
            zoom,
            ..
        } = *f;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.canvas.gesture = None;
            self.canvas.space_tool = false;
            self.canvas.connect_click = None;
            self.canvas.append_menu = None;
        }
        // Click-to-connect: the next click picks the target (empty canvas offers the
        // append popup); the preview follows the pointer meanwhile.
        let mut click_consumed = false;
        if let Some((source, bounds)) = self.canvas.connect_click.clone() {
            if response.clicked() || response.drag_started() {
                click_consumed = true;
                self.canvas.connect_click = None;
                if let Some(pos) = response.interact_pointer_pos() {
                    self.finish_connect(source, pos, rect, camera, zoom);
                }
            } else if let Some(pos) = hover {
                self.paint_connect(
                    painter, response, d, diagram, &source, &bounds, pos, rect, camera, zoom,
                    accent,
                );
            }
        }
        click_consumed
    }
    fn begin_gesture(
        &mut self,
        f: &CanvasFrame<'_>,
        sel: &Selection<'_>,
        pad: &[(PadAction, Rect)],
        click_consumed: bool,
    ) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            rect,
            camera,
            zoom,
            editable,
            ..
        } = *f;
        let Selection {
            single,
            rule,
            selected_edge,
            ref bend_handles,
            ref handles,
            ..
        } = *sel;
        let pad_at = |pos: Pos2| {
            pad.iter()
                .find(|(_, r)| r.contains(pos))
                .map(|(action, _)| *action)
        };
        if response.drag_started()
            && !click_consumed
            && let Some(pos) = ctx.input(|i| i.pointer.press_origin())
        {
            let pan = ctx.input(|i| {
                i.pointer.middle_down()
                    || (i.key_down(egui::Key::Space) && !ctx.egui_wants_keyboard_input())
            });
            if pan {
                self.canvas.gesture = Some(Gesture::Pan);
            } else if self.canvas.space_tool && editable {
                self.canvas.gesture = Some(Gesture::Space(to_model(rect, camera, zoom, pos)));
            } else if let Some(edge) = selected_edge
                && let Some((part, _)) = bend_handles.iter().find(|(_, h)| h.distance(pos) <= 8.0)
            {
                let (from, to) = processcraft_core::modeling::edge_ends(d, diagram, edge);
                self.canvas.gesture = Some(Gesture::Bendpoint {
                    edge: edge.id.selector(),
                    flow: edge.element.clone(),
                    points: edge.waypoints.clone(),
                    part: *part,
                    ends: [from, to].map(|s| s.map(|s| s.bounds.clone())),
                    kinds: [from, to].map(|s| {
                        s.and_then(|s| processcraft_core::queries::find_node(d, &s.element))
                            .map(|n| n.kind)
                    }),
                    start: to_model(rect, camera, zoom, pos),
                });
            } else if let Some(shape) = single {
                if let Some(rule) = rule
                    && let Some(handle) = handle_at(handles, pos)
                {
                    self.canvas.gesture = Some(Gesture::Resize(ResizeDrag {
                        shape: shape.id.selector(),
                        bounds: shape.bounds.clone(),
                        handle,
                        min: (rule.width, rule.height),
                        aspect: rule.keep_aspect,
                        start: to_model(rect, camera, zoom, pos),
                        keep: child_extent(d, diagram, shape),
                    }));
                } else if pad_at(pos) == Some(PadAction::Connect) {
                    self.canvas.gesture = Some(Gesture::Connect {
                        element: shape.element.clone(),
                        bounds: shape.bounds.clone(),
                    });
                }
            }
        }
    }
    fn pad_click(
        &mut self,
        f: &CanvasFrame<'_>,
        sel: &Selection<'_>,
        pad: &[(PadAction, Rect)],
        pad_owner: Option<ElementSelector>,
        click_consumed: bool,
    ) -> Option<PadAction> {
        let CanvasFrame {
            ref ctx,
            ref response,
            ..
        } = *f;
        let Selection { single, .. } = *sel;
        let pad_at = |pos: Pos2| {
            pad.iter()
                .find(|(_, r)| r.contains(pos))
                .map(|(action, _)| *action)
        };
        let pad_pressed = if response.drag_started() {
            ctx.input(|i| i.pointer.press_origin())
        } else if response.clicked() {
            response.interact_pointer_pos()
        } else {
            None
        }
        .and_then(pad_at);
        if response.clicked()
            && let (Some(action), Some(owner)) = (pad_pressed, pad_owner)
        {
            match (action, single) {
                // Click-to-connect: wait for the target click.
                (PadAction::Connect, Some(shape)) => {
                    self.canvas.connect_click = Some((shape.element.clone(), shape.bounds.clone()));
                }
                _ => self.pad_action(owner, action),
            }
        }
        // Any other canvas click or drag closes the append popup.
        if !click_consumed && (response.clicked() || response.drag_started()) {
            self.canvas.append_menu = None;
        }
        pad_pressed
    }
    fn select_click(
        &mut self,
        f: &CanvasFrame<'_>,
        sel: &Selection<'_>,
        click_consumed: bool,
        pad_pressed: Option<PadAction>,
    ) -> bool {
        let CanvasFrame {
            ref ctx,
            snap,
            d,
            diagram,
            ref response,
            rect,
            camera,
            zoom,
            editable,
            ..
        } = *f;
        let Selection { ref handles, .. } = *sel;
        let diagram_id = diagram.id.selector();
        let alt = ctx.input(|i| i.modifiers.alt);
        let shift = ctx.input(|i| i.modifiers.shift);
        // A click on a resize handle keeps the selection (double-click fits it).
        let on_handle = (response.clicked() || response.double_clicked())
            && response
                .interact_pointer_pos()
                .and_then(|pos| handle_at(handles, pos))
                .is_some();
        if !click_consumed
            && pad_pressed.is_none()
            && !on_handle
            && ((response.drag_started() && self.canvas.gesture.is_none()) || response.clicked())
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
                let same_diagram = self.representation_diagram.as_ref() == Some(&diagram_id);
                if !shift
                    && (!same_diagram || !self.representation_selection.contains(&representation))
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
                            .filter(|s| self.representation_selection.contains(&s.id.selector()))
                            .map(|s| (s.id.selector(), s.bounds.clone()))
                            .collect()
                    };
                    if label_drag && items.is_empty() {
                        self.ctl.error = "Set label bounds in Diagram representation properties before dragging the label.".into();
                    } else if self.ctl.session.snapshot().generation != snap.generation {
                        self.ctl.error = "Properties changed; start the move again.".into();
                    } else if !items.is_empty() {
                        self.drag = begin_drag(response, ctx, items);
                        if let Some(drag) = &mut self.drag {
                            drag.context = Some((snap.generation, d.id, diagram_id.clone()));
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
        on_handle
    }
    fn double_click(&mut self, f: &CanvasFrame<'_>, sel: &Selection<'_>, on_handle: bool) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            rect,
            camera,
            zoom,
            editable,
            ..
        } = *f;
        let Selection {
            single,
            selected_edge,
            ref bend_handles,
            ..
        } = *sel;
        let diagram_id = diagram.id.selector();
        let alt = ctx.input(|i| i.modifiers.alt);
        // Double-click on a bendpoint removes it (bpmn-js).
        let bend = (response.double_clicked())
            .then(|| response.interact_pointer_pos())
            .flatten()
            .zip(selected_edge)
            .and_then(|(pos, edge)| {
                bend_handles.iter().find_map(|(part, h)| match part {
                    EdgePart::Point(i)
                        if *i > 0 && *i + 1 < edge.waypoints.len() && h.distance(pos) <= 8.0 =>
                    {
                        Some((edge, *i))
                    }
                    _ => None,
                })
            });
        if let Some((edge, i)) = bend {
            let mut waypoints = edge.waypoints.clone();
            waypoints.remove(i);
            self.ctl.mutate(vec![Mutation::Reroute {
                document_id: d.id,
                diagram_id: diagram_id.clone(),
                edge_id: edge.id.selector(),
                waypoints,
            }]);
        } else if on_handle && response.double_clicked() {
            if let Some(shape) = single
                && fits_content(d, shape)
            {
                self.fit_to_content(&shape.element);
            }
        } else if editable
            && response.double_clicked()
            && self.canvas.label_edit.is_none()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (x, y) = to_model(rect, camera, zoom, pos);
            if let Some(h) = self.hit_at(x, y, zoom, alt) {
                let bounds = match &h.geometry {
                    HitGeometry::Edge(_) => None,
                    HitGeometry::Shape(b)
                    | HitGeometry::Ellipse(b)
                    | HitGeometry::Diamond(b)
                    | HitGeometry::Hexagon(b)
                    | HitGeometry::DataObject(b)
                    | HitGeometry::DataStore(b) => Some(b.clone()),
                };
                match label_edit(d, diagram, &h.key.xml_id, bounds.as_ref()) {
                    Ok(edit) => {
                        self.drag = None;
                        self.canvas.label_edit = Some(edit);
                    }
                    Err(reason) => {
                        if let Some(reason) = reason {
                            self.ctl.error = reason.into();
                        }
                    }
                }
            }
        }
    }
    fn advance_move_drag(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            zoom,
            ..
        } = *f;
        let shift = ctx.input(|i| i.modifiers.shift);
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
            self.ctl.error = "The diagram changed while dragging. Start the move again.".into();
        }
        advance_drag(&mut self.drag, ctx);
        if shift && let Some(drag) = &mut self.drag {
            drag.offset = lock_axis(drag.offset);
        }
        // Snapping (Alt or Ctrl moves freely) and its guide lines.
        let free = ctx.input(|i| i.modifiers.alt || i.modifiers.ctrl);
        if response.dragged()
            && let Some(drag) = &mut self.drag
            && !drag.label
        {
            drag.snap = match drag.items.first() {
                Some((_, first)) if !free => {
                    let moved = Bounds {
                        x: first.x + (drag.offset.x / zoom) as f64,
                        y: first.y + (drag.offset.y / zoom) as f64,
                        ..first.clone()
                    };
                    snap_drag(
                        d,
                        diagram,
                        &drag.items,
                        &moved,
                        7.0 / zoom as f64,
                        self.canvas.grid_snap,
                    )
                }
                _ => Default::default(),
            };
            // An axis locked by Shift does not snap either.
            if shift && drag.offset.x == 0.0 {
                (drag.snap.dx, drag.snap.guide_x) = (0.0, None);
            } else if shift && drag.offset.y == 0.0 {
                (drag.snap.dy, drag.snap.guide_y) = (0.0, None);
            }
        }
    }
    fn paint_drag_preview(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            d,
            diagram,
            ref response,
            ref painter,
            palette,
            accent,
            rect,
            camera,
            zoom,
            ..
        } = *f;
        if response.dragged()
            && let Some(drag) = &self.drag
        {
            let offset = drag.offset + Vec2::new(drag.snap.dx as f32, drag.snap.dy as f32) * zoom;
            let guide = egui::Stroke::new(1.0, palette.selection.gamma_multiply(0.6));
            paint_guides(painter, rect, camera, zoom, &drag.snap, guide);
            // Attach and drop-on-flow feedback for a single dragged element.
            if let [(shape_id, bounds)] = drag.items.as_slice()
                && !drag.label
                && let Some(node) = diagram
                    .shapes
                    .iter()
                    .find(|s| s.id == *shape_id)
                    .and_then(|s| processcraft_core::queries::find_node(d, &s.element))
            {
                let center = Point {
                    x: bounds.x + bounds.width / 2.0 + (offset.x / zoom) as f64,
                    y: bounds.y + bounds.height / 2.0 + (offset.y / zoom) as f64,
                };
                drop_feedback(
                    painter,
                    d,
                    diagram,
                    node,
                    &center,
                    |b| to_screen(rect, camera, zoom, b),
                    |p| {
                        Pos2::new(
                            rect.left() + (p.x - camera.0) as f32 * zoom,
                            rect.top() + (p.y - camera.1) as f32 * zoom,
                        )
                    },
                );
            }
            for (_, bounds) in &drag.items {
                let preview = to_screen(rect, camera, zoom, bounds).translate(offset);
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
    }
    fn finish_move_drag(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            d,
            diagram,
            ref response,
            zoom,
            ..
        } = *f;
        if let Some(drag) = finish_drag(response, &mut self.drag) {
            let delta = drag.offset / zoom;
            if delta.length() > 1.0 {
                if drag.label {
                    if let Some((representation_id, mut bounds)) = drag.items.into_iter().next() {
                        bounds.x += delta.x as f64;
                        bounds.y += delta.y as f64;
                        let mutation = if let Some(shape) = diagram
                            .shapes
                            .iter()
                            .find(|s| s.id == representation_id && s.label.is_none())
                        {
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
                        } else if let Some(edge) = diagram
                            .edges
                            .iter()
                            .find(|e| e.id == representation_id && e.label.is_none())
                        {
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
                    self.drag_move(
                        drag.items,
                        delta.x as f64 + drag.snap.dx,
                        delta.y as f64 + drag.snap.dy,
                    );
                }
            }
        }
    }
    fn paint_gestures(&mut self, f: &mut CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            ref painter,
            accent,
            rect,
            ..
        } = *f;
        let (mut camera, zoom) = (f.camera, f.zoom);
        // Canvas gestures: pan, box select, connect, resize.
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        if response.dragged() {
            match &self.canvas.gesture {
                Some(Gesture::Pan) => {
                    let delta = response.drag_delta();
                    camera.0 -= delta.x as f64 / zoom as f64;
                    camera.1 -= delta.y as f64 / zoom as f64;
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                Some(Gesture::Space(origin)) => {
                    // The line where space opens and how far the shapes past it go.
                    if let Some(pos) = pointer {
                        let start = &Pos2::new(
                            rect.left() + (origin.0 - camera.0) as f32 * zoom,
                            rect.top() + (origin.1 - camera.1) as f32 * zoom,
                        );
                        let d = pos - *start;
                        let stroke = egui::Stroke::new(1.5, accent);
                        let (line, moved, cursor) = if d.x.abs() >= d.y.abs() {
                            (
                                [
                                    Pos2::new(start.x, rect.top()),
                                    Pos2::new(start.x, rect.bottom()),
                                ],
                                Rect::from_x_y_ranges(
                                    start.x.min(pos.x)..=start.x.max(pos.x),
                                    rect.y_range(),
                                ),
                                egui::CursorIcon::ResizeHorizontal,
                            )
                        } else {
                            (
                                [
                                    Pos2::new(rect.left(), start.y),
                                    Pos2::new(rect.right(), start.y),
                                ],
                                Rect::from_x_y_ranges(
                                    rect.x_range(),
                                    start.y.min(pos.y)..=start.y.max(pos.y),
                                ),
                                egui::CursorIcon::ResizeVertical,
                            )
                        };
                        painter.rect_filled(moved, 0.0, accent.gamma_multiply(0.12));
                        painter.extend(egui::Shape::dashed_line(&line, stroke, 6.0, 4.0));
                        ctx.set_cursor_icon(cursor);
                    }
                }
                Some(Gesture::BoxSelect(start)) => {
                    if let Some(pos) = pointer {
                        let area = Rect::from_two_pos(*start, pos);
                        painter.rect_filled(area, 0.0, accent.gamma_multiply(0.12));
                        let stroke = egui::Stroke::new(1.0, accent);
                        if pos.x < start.x {
                            // Right to left selects on touch: dashed outline.
                            let corners = [
                                area.left_top(),
                                area.right_top(),
                                area.right_bottom(),
                                area.left_bottom(),
                                area.left_top(),
                            ];
                            painter.extend(egui::Shape::dashed_line(&corners, stroke, 5.0, 3.0));
                        } else {
                            painter.rect_stroke(area, 0.0, stroke, egui::StrokeKind::Inside);
                        }
                    }
                }
                Some(Gesture::Connect { element, bounds }) => {
                    if let Some(pos) = pointer {
                        self.paint_connect(
                            painter, response, d, diagram, element, bounds, pos, rect, camera,
                            zoom, accent,
                        );
                    }
                }
                Some(Gesture::Bendpoint {
                    points,
                    part,
                    ends,
                    kinds,
                    start,
                    ..
                }) => {
                    if let Some(pos) = pointer {
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        // An end over its own shape previews its new dock.
                        let docked = end_dock(points, *part, ends, kinds, (x, y), zoom);
                        if let EdgePart::Point(i) = part
                            && let Some(end) = docked.as_ref().and_then(|route| {
                                if *i == 0 { route.first() } else { route.last() }
                            })
                        {
                            painter.circle_filled(
                                Pos2::new(
                                    rect.left() + (end.x - camera.0) as f32 * zoom,
                                    rect.top() + (end.y - camera.1) as f32 * zoom,
                                ),
                                4.0,
                                accent,
                            );
                        }
                        let edited = docked.unwrap_or_else(|| {
                            edit_route(
                                points,
                                *part,
                                x - start.0,
                                y - start.1,
                                ends,
                                self.canvas.grid_snap,
                            )
                        });
                        let screen: Vec<Pos2> = edited
                            .iter()
                            .map(|p| {
                                Pos2::new(
                                    rect.left() + (p.x - camera.0) as f32 * zoom,
                                    rect.top() + (p.y - camera.1) as f32 * zoom,
                                )
                            })
                            .collect();
                        painter.add(egui::Shape::line(screen, egui::Stroke::new(2.0, accent)));
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }
                Some(Gesture::Resize(resize)) => {
                    if let Some(pos) = pointer {
                        let (resized, snap) = resize.preview(
                            d,
                            diagram,
                            to_model(rect, camera, zoom, pos),
                            ctx.input(|i| i.modifiers),
                            7.0 / zoom as f64,
                            self.canvas.grid_snap,
                        );
                        let guide = egui::Stroke::new(1.0, accent.gamma_multiply(0.6));
                        paint_guides(painter, rect, camera, zoom, &snap, guide);
                        painter.rect_stroke(
                            to_screen(rect, camera, zoom, &resized),
                            0.0,
                            egui::Stroke::new(2.0, accent),
                            egui::StrokeKind::Inside,
                        );
                        ctx.set_cursor_icon(resize_cursor(resize.handle));
                    }
                }
                None => {}
            }
        }
        f.camera = camera;
    }
    fn finish_gesture(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref response,
            rect,
            camera,
            zoom,
            ..
        } = *f;
        let diagram_id = diagram.id.selector();
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        if response.drag_stopped()
            && let Some(gesture) = self.canvas.gesture.take()
        {
            match gesture {
                Gesture::Pan => {}
                Gesture::Space((x0, y0)) => {
                    if let Some(pos) = pointer {
                        let (x1, y1) = to_model(rect, camera, zoom, pos);
                        let (axis, from, delta) = if (x1 - x0).abs() >= (y1 - y0).abs() {
                            (Axis::X, x0, x1 - x0)
                        } else {
                            (Axis::Y, y0, y1 - y0)
                        };
                        // Under 5 screen px is a click, not a space drag.
                        if delta.abs() * zoom as f64 >= 5.0 {
                            self.ctl.mutate(vec![Mutation::MakeSpace {
                                document_id: d.id,
                                diagram_id: diagram_id.clone(),
                                axis,
                                start: from.round(),
                                delta: delta.round(),
                                before: ctx.input(|i| i.modifiers.command),
                                at: Some(Point { x: x0, y: y0 }),
                            }]);
                        }
                    }
                }
                Gesture::BoxSelect(start) => {
                    if let Some(pos) = pointer {
                        let (x0, y0) = to_model(rect, camera, zoom, start);
                        let (x1, y1) = to_model(rect, camera, zoom, pos);
                        let found = marquee(diagram, (x0, y0), (x1, y1));
                        if !found.is_empty() {
                            // Shift starts the marquee, so it adds to the selection.
                            let same = self.representation_diagram.as_ref() == Some(&diagram_id);
                            let mut multi = self.multi.clone();
                            let mut representations = if same {
                                self.representation_selection.clone()
                            } else {
                                Vec::new()
                            };
                            for (element, representation) in found {
                                if !multi.contains(&element) {
                                    multi.push(element);
                                }
                                if !representations.contains(&representation) {
                                    representations.push(representation);
                                }
                            }
                            self.select(multi.first().cloned());
                            self.multi = multi;
                            self.representation_selection = representations;
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
                        self.finish_connect(element, pos, rect, camera, zoom);
                    }
                }
                Gesture::Resize(resize) => {
                    if let Some(pos) = pointer {
                        let (resized, _) = resize.preview(
                            d,
                            diagram,
                            to_model(rect, camera, zoom, pos),
                            ctx.input(|i| i.modifiers),
                            7.0 / zoom as f64,
                            self.canvas.grid_snap,
                        );
                        if resized != resize.bounds {
                            self.ctl.mutate(vec![Mutation::ResizeShape {
                                document_id: d.id,
                                diagram_id: diagram_id.clone(),
                                shape_id: resize.shape,
                                bounds: resized,
                            }]);
                        }
                    }
                }
                Gesture::Bendpoint {
                    edge,
                    flow,
                    points,
                    part,
                    ends,
                    kinds,
                    start,
                } => {
                    if let Some(pos) = pointer {
                        let last = points.len().saturating_sub(1);
                        let (x, y) = to_model(rect, camera, zoom, pos);
                        let delta = Vec2::new((x - start.0) as f32, (y - start.1) as f32);
                        let flow_ends = processcraft_core::queries::find_flow(d, &flow)
                            .map(|f| (f.source.clone(), f.target.clone()));
                        let own = end_dock(&points, part, &ends, &kinds, (x, y), zoom);
                        let local = |r: &Option<
                            processcraft_core::bpmn_activity::ModelReference,
                        >| match r {
                            Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) => {
                                Some(id.clone())
                            }
                            _ => None,
                        };
                        match (part, flow_ends, own) {
                            // An end released on or near its own shape re-docks
                            // where the preview showed it.
                            (_, _, Some(waypoints)) => {
                                if waypoints != points {
                                    self.ctl.mutate(vec![Mutation::Reroute {
                                        document_id: d.id,
                                        diagram_id: diagram_id.clone(),
                                        edge_id: edge,
                                        waypoints,
                                    }]);
                                }
                            }
                            // Dragging an end onto another shape reconnects the flow;
                            // released anywhere else, the end stays.
                            (EdgePart::Point(i), flow_ends, _) if i == 0 || i == last => {
                                if let Some((source, target)) = flow_ends
                                    && let (Some(source), Some(target)) =
                                        (local(&source), local(&target))
                                    && let Some(hit) = self.connect_target_at(
                                        x,
                                        y,
                                        zoom,
                                        if i == 0 { &source } else { &target },
                                    )
                                {
                                    let element = hit.key.xml_id;
                                    let (source_id, target_id) = if i == 0 {
                                        (element, target)
                                    } else {
                                        (source, element)
                                    };
                                    self.ctl.mutate(vec![Mutation::Reconnect {
                                        document_id: d.id,
                                        diagram_id: diagram_id.clone(),
                                        flow_id: flow,
                                        source_id,
                                        target_id,
                                    }]);
                                }
                            }
                            _ if delta.length() > 1.0 => {
                                self.ctl.mutate(vec![Mutation::Reroute {
                                    document_id: d.id,
                                    diagram_id: diagram_id.clone(),
                                    edge_id: edge,
                                    waypoints: edit_route(
                                        &points,
                                        part,
                                        delta.x as f64,
                                        delta.y as f64,
                                        &ends,
                                        self.canvas.grid_snap,
                                    ),
                                }]);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    fn palette_drop(&mut self, f: &CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            d,
            diagram,
            ref painter,
            accent,
            rect,
            camera,
            zoom,
            editable,
            ..
        } = *f;
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        // A palette entry dragged over the canvas: ghost, feedback, drop on release.
        if let Some(entry) = self.canvas.palette_drag {
            let kind = entry.kind;
            let released = ctx.input(|i| !i.pointer.primary_down());
            let over = pointer.filter(|p| rect.contains(*p));
            if let Some(pos) = over {
                let (x, y) = to_model(rect, camera, zoom, pos);
                let (w, h) = processcraft_core::modeling::default_size(kind);
                let ghost = to_screen(
                    rect,
                    camera,
                    zoom,
                    &Bounds {
                        x: x - w / 2.0,
                        y: y - h / 2.0,
                        width: w,
                        height: h,
                    },
                );
                paint_ghost(painter, entry, ghost, accent);
                let probe = processcraft_core::modeling::new_node(
                    kind,
                    processcraft_core::ids::ElementIdentity::anonymous(),
                    None,
                    None,
                );
                drop_feedback(
                    painter,
                    d,
                    diagram,
                    &probe,
                    &Point { x, y },
                    |b| to_screen(rect, camera, zoom, b),
                    |p| {
                        Pos2::new(
                            rect.left() + (p.x - camera.0) as f32 * zoom,
                            rect.top() + (p.y - camera.1) as f32 * zoom,
                        )
                    },
                );
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                if released && editable {
                    self.drop_new(entry, Point { x, y });
                }
            }
            if released {
                self.canvas.palette_drag = None;
            } else {
                ctx.request_repaint();
            }
        }
    }
    fn label_editor(&mut self, f: &CanvasFrame<'_>, ui: &mut egui::Ui) {
        let CanvasFrame {
            ref ctx,
            d,
            rect,
            camera,
            zoom,
            ..
        } = *f;
        // Inline editor: a name commits on Enter, an annotation text (multi-line) on
        // Ctrl+Enter; focus loss commits, Escape cancels.
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if let Some(edit) = &mut self.canvas.label_edit {
            let width = (edit.width as f32 * zoom).max(140.0);
            let center = Pos2::new(
                rect.left() + (edit.center.0 - camera.0) as f32 * zoom,
                rect.top() + (edit.center.1 - camera.1) as f32 * zoom,
            );
            let id = egui::Id::unique("processcraft-canvas-label-edit");
            let (field, submit) = if edit.annotation {
                // Consumed before the field sees it, so no newline is inserted.
                let submit =
                    ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
                let height = (edit.height as f32 * zoom).max(60.0);
                let field = ui.put(
                    Rect::from_center_size(center, Vec2::new(width, height)),
                    egui::TextEdit::multiline(&mut edit.text)
                        .id(id)
                        .hint_text("Annotation text · Ctrl+Enter to apply"),
                );
                (field, submit)
            } else {
                let field = ui.put(
                    Rect::from_center_size(center, Vec2::new(width, 28.0)),
                    egui::TextEdit::singleline(&mut edit.text).id(id),
                );
                (field, false)
            };
            if !edit.focused {
                field.request_focus();
                edit.focused = true;
            } else if escape {
                self.canvas.label_edit = None;
            } else if (field.lost_focus() || submit)
                && let Some(edit) = self.canvas.label_edit.take()
                && edit.text != edit.original
                && self.ctl.mutate(vec![if edit.annotation {
                    Mutation::SetTextAnnotation {
                        document_id: d.id,
                        element_id: edit.element.clone(),
                        text: Some(edit.text.clone()),
                    }
                } else {
                    Mutation::Rename {
                        document_id: d.id,
                        element_id: edit.element.clone(),
                        name: Some(edit.text.clone()),
                    }
                }])
                && self.selected.as_ref() == Some(&edit.element)
                && !self.draft_dirty
            {
                self.draft = edit.text;
                self.draft_name_present = true;
            }
        }
    }
    fn wheel_zoom(&mut self, f: &mut CanvasFrame<'_>) {
        let CanvasFrame {
            ref ctx,
            ref response,
            rect,
            size,
            hover,
            ..
        } = *f;
        let (mut camera, mut zoom) = (f.camera, f.zoom);
        if response.hovered() {
            let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom_delta != 1.0 {
                let anchor = hover.map_or(size / 2.0, |pos| pos - rect.min);
                (camera, zoom) = zoom_at(camera, zoom, zoom * zoom_delta, anchor);
            } else if scroll != Vec2::ZERO {
                camera.0 -= scroll.x as f64 / zoom as f64;
                camera.1 -= scroll.y as f64 / zoom as f64;
            }
        }
        f.camera = camera;
        f.zoom = zoom;
    }
}
