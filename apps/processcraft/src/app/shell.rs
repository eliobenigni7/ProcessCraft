use super::*;
use crate::widgets::{self, Icon};
use processcraft_core::ids::DocumentId;

fn new_choreography_element(
    kind: &str,
    id: processcraft_core::ids::XmlId,
) -> processcraft_core::bpmn_collaboration::ChoreographyFlowElement {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivity, ChoreographyActivityDetails, ChoreographyFlowElement,
        ChoreographyGraph,
    };
    match kind {
        "Task" | "Sub-choreography" | "Call choreography" => {
            ChoreographyFlowElement::Activity(Box::new(ChoreographyActivity {
                id: id.into(),
                name: None,
                base: None,
                flow_metadata: None,
                correlation_keys: Vec::new(),
                participant_refs: Vec::new(),
                initiating_participant: None,
                message_flow_refs: Vec::new(),
                loop_type: None,
                details: match kind {
                    "Sub-choreography" => Some(ChoreographyActivityDetails::Sub {
                        graph: Box::new(ChoreographyGraph::default()),
                        artifacts: Vec::new(),
                    }),
                    "Call choreography" => Some(ChoreographyActivityDetails::Call {
                        participant_associations: Vec::new(),
                        called_choreography_ref: None,
                    }),
                    _ => None,
                },
            }))
        }
        "Sequence flow" => ChoreographyFlowElement::SequenceFlow(Box::new(Flow {
            id: id.into(),
            kind: FlowKind::Sequence,
            source: None,
            target: None,
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
            association_direction: None,
            flow_metadata: None,
            message_ref: None,
        })),
        _ => {
            let node_kind = match kind {
                "Start event" => NodeKind::StartEvent,
                "End event" => NodeKind::EndEvent,
                "Intermediate catch event" => NodeKind::IntermediateCatchEvent,
                "Intermediate throw event" => NodeKind::IntermediateThrowEvent,
                "Boundary event" => NodeKind::BoundaryEvent,
                "Parallel gateway" => NodeKind::ParallelGateway,
                "Inclusive gateway" => NodeKind::InclusiveGateway,
                "Complex gateway" => NodeKind::ComplexGateway,
                "Event-based gateway" => NodeKind::EventBasedGateway,
                _ => NodeKind::ExclusiveGateway,
            };
            let node = Box::new(Node {
                id: id.into(),
                name: None,
                documentation: String::new().into(),
                base_extras: None,
                kind: node_kind,
                details: None,
                loop_characteristics: None,
                io_specification: None,
                activity: None,
                flow_metadata: None,
                item_aware: None,
                owner: String::new(),
                contact: String::new(),
            });
            if matches!(
                node_kind,
                NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent
            ) {
                ChoreographyFlowElement::Event(node)
            } else {
                ChoreographyFlowElement::Gateway(node)
            }
        }
    }
}

fn vacant_choreography_bounds(diagram: &Diagram, camera: (f64, f64), kind: &str) -> Option<Bounds> {
    if !camera.0.is_finite() || !camera.1.is_finite() {
        return None;
    }
    let (width, height) = if kind.contains("event") {
        (36.0, 36.0)
    } else if kind.contains("gateway") {
        (50.0, 50.0)
    } else {
        (120.0, 80.0)
    };
    (0..1024)
        .map(|index| Bounds {
            x: camera.0 + 100.0 + (index % 8) as f64 * 180.0,
            y: camera.1 + 100.0 + (index / 8) as f64 * 120.0,
            width,
            height,
        })
        .find(|candidate| {
            diagram.shapes.iter().all(|shape| {
                let bounds = &shape.bounds;
                candidate.x + candidate.width + 20.0 <= bounds.x
                    || bounds.x + bounds.width + 20.0 <= candidate.x
                    || candidate.y + candidate.height + 20.0 <= bounds.y
                    || bounds.y + bounds.height + 20.0 <= candidate.y
            })
        })
}

#[cfg(test)]
mod choreography_graph_tests {
    use super::*;
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement,
    };

    #[test]
    fn new_sub_uses_free_position_and_is_collapsed() {
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let choreography = super::super::collaboration::fresh_choreography(
            [
                "Root".parse().unwrap(),
                "Task".parse().unwrap(),
                "Buyer".parse().unwrap(),
                "Seller".parse().unwrap(),
            ],
            [
                "Orders".into(),
                "Submit".into(),
                "Buyer".into(),
                "Seller".into(),
            ],
        );
        let task_shape = Shape {
            id: "TaskShape".parse().unwrap(),
            element: "Task".parse().unwrap(),
            bounds: Bounds {
                x: 80.0,
                y: 80.0,
                width: 180.0,
                height: 100.0,
            },
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            label: None,
            label_visibility: None,
            style: None,
        };
        let diagram = Diagram {
            id: "Diagram".parse().unwrap(),
            plane_id: "Plane".parse().unwrap(),
            plane: "Root".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: vec![task_shape],
            edges: vec![],
        };
        let event_bounds = vacant_choreography_bounds(&diagram, (0.0, 0.0), "Start event").unwrap();
        let gateway_bounds =
            vacant_choreography_bounds(&diagram, (0.0, 0.0), "Exclusive gateway").unwrap();
        assert_eq!((event_bounds.width, event_bounds.height), (36.0, 36.0));
        assert_eq!((gateway_bounds.width, gateway_bounds.height), (50.0, 50.0));
        assert!(app.ctl.mutate(vec![
            Mutation::CreateChoreography {
                document_id,
                choreography
            },
            Mutation::CreateDiagram {
                document_id,
                diagram
            }
        ]));
        app.diagram_id = Some("Diagram".parse().unwrap());
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let scope = processcraft_core::queries::choreography_scopes(document)
            .into_iter()
            .find(|scope| scope.owner == "Root".parse::<ElementSelector>().unwrap())
            .unwrap();
        assert!(
            app.add_choreography_element(document, &scope, "Sub-choreography", snapshot.generation),
            "{}",
            app.ctl.error
        );
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let diagram = document
            .diagrams
            .iter()
            .find(|diagram| diagram.id == "Diagram".parse::<ElementSelector>().unwrap())
            .unwrap();
        let new_shape = &diagram.shapes[1];
        assert_eq!(new_shape.is_expanded, Some(false));
        assert_eq!(
            (new_shape.bounds.width, new_shape.bounds.height),
            (120.0, 80.0)
        );
        assert!(
            new_shape.bounds.x
                >= diagram.shapes[0].bounds.x + diagram.shapes[0].bounds.width + 20.0
                || new_shape.bounds.y
                    >= diagram.shapes[0].bounds.y + diagram.shapes[0].bounds.height + 20.0
        );
        let scope = processcraft_core::queries::choreography_scopes(document)
            .into_iter()
            .find(|scope| scope.owner == "Root".parse::<ElementSelector>().unwrap())
            .unwrap();
        let sub = scope
            .activities
            .iter()
            .find(|activity| activity.id == new_shape.element)
            .unwrap();
        assert!(matches!(
            sub.details,
            Some(ChoreographyActivityDetails::Sub { .. })
        ));
        assert!(sub.participant_refs.is_empty() && sub.initiating_participant.is_none());
    }

    #[test]
    fn new_graph_items_keep_incomplete_references_explicit() {
        let ChoreographyFlowElement::Activity(task) =
            new_choreography_element("Task", controller::id("Task"))
        else {
            panic!("task")
        };
        assert!(task.initiating_participant.is_none());
        assert!(task.participant_refs.is_empty());
        let ChoreographyFlowElement::Activity(sub) =
            new_choreography_element("Sub-choreography", controller::id("Sub"))
        else {
            panic!("sub")
        };
        assert!(matches!(
            sub.details,
            Some(ChoreographyActivityDetails::Sub { .. })
        ));
        let ChoreographyFlowElement::SequenceFlow(flow) =
            new_choreography_element("Sequence flow", controller::id("Flow"))
        else {
            panic!("flow")
        };
        assert!(flow.source.is_none() && flow.target.is_none());
    }

    #[test]
    fn boundary_event_is_an_unattached_draft_with_scoped_activity_choices() {
        let boundary = new_choreography_element("Boundary event", "Boundary".parse().unwrap());
        assert!(
            matches!(&boundary, ChoreographyFlowElement::Event(event) if event.kind == NodeKind::BoundaryEvent && event.details.is_none())
        );
        let mut document = crate::controller::blank_document();
        let mut root = super::super::collaboration::fresh_choreography(
            [
                "Root".parse().unwrap(),
                "Task".parse().unwrap(),
                "Buyer".parse().unwrap(),
                "Seller".parse().unwrap(),
            ],
            [
                "Orders".into(),
                "Submit".into(),
                "Buyer".into(),
                "Seller".into(),
            ],
        );
        root.graph.as_mut().unwrap().flow_elements.push(boundary);
        document.choreographies.push(root);
        let scope = processcraft_core::queries::choreography_scopes(&document)
            .into_iter()
            .find(|scope| scope.owner == "Root".parse::<ElementSelector>().unwrap())
            .unwrap();
        assert!(
            scope
                .events
                .iter()
                .any(|event| event.id == "Boundary".parse::<ElementSelector>().unwrap())
        );
        let choices = super::super::event::local_choices(&document, "Activity", Some(&scope.owner));
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].0, "Task".parse::<ElementSelector>().unwrap());
        let diagram = &document.diagrams[0];
        let bounds = vacant_choreography_bounds(diagram, (0.0, 0.0), "Boundary event").unwrap();
        assert_eq!((bounds.width, bounds.height), (36.0, 36.0));
    }
}

#[derive(Default)]
pub(super) struct ShellState {
    session: String,
    tabs: Vec<DocumentId>,
    automation: bool,
    properties: bool,
    project: bool,
    /// The open bottom-panel tab, or `None` when the panel is closed.
    pub(super) bottom: Option<BottomTab>,
    search: String,
    /// Focus mode (Ctrl+.): only the canvas and its capsules.
    pub(super) focus: bool,
    /// The window is wide enough to dock the side panels.
    wide: bool,
}

/// How long a success notice stays in the status bar.
const INFO_SECONDS: f32 = 4.0;

impl ProcessCraft {
    pub(super) fn workbench_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.review_poll(&ctx);
        let p = self.theme.palette();
        let snap = self.ctl.session.snapshot();
        let wide = ui.available_width() >= 980.0;
        let compact_height = ui.available_height() < 600.0;
        let toolbar_room = ui.available_width() >= 500.0;
        self.shell.wide = wide;
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Period)) {
            self.shell.focus = !self.shell.focus;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::J)) {
            self.shell.bottom = match self.shell.bottom {
                Some(_) => None,
                None => Some(BottomTab::Problems),
            };
        }
        let focus = self.shell.focus && self.view == 0;
        ui.spacing_mut().item_spacing.y = 0.0;
        let can_undo = snap.undo_count > 0 || self.has_drafts();
        // Redo first applies open drafts; an unchanged draft leaves the redo stack intact.
        let can_redo = snap.redo_count > 0;
        let session = self.ctl.session.id();
        if self.shell.session != session {
            self.shell.session = session;
            self.shell.tabs.clear();
            self.shell.search.clear();
        }
        self.shell
            .tabs
            .retain(|id| snap.project.documents.iter().any(|d| d.id == *id));
        if matches!(self.view, 0 | 3)
            && let Some(doc) = snap.project.documents.get(self.doc)
            && !self.shell.tabs.contains(&doc.id)
        {
            self.shell.tabs.push(doc.id);
        }

        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 24.0;
            ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project    Ctrl+N").clicked() {
                        self.new_or_open("new");
                        ui.close();
                    }
                    if ui.button("Open…              Ctrl+O").clicked() {
                        self.new_or_open("open");
                        ui.close();
                    }
                    ui.add_enabled_ui(!self.settings.recent.is_empty(), |ui| {
                        ui.menu_button("Open recent", |ui| {
                            for path in self.settings.recent.clone() {
                                let name = path.file_name().map_or_else(
                                    || path.display().to_string(),
                                    |n| n.to_string_lossy().into_owned(),
                                );
                                if ui
                                    .button(name)
                                    .on_hover_text(path.display().to_string())
                                    .clicked()
                                {
                                    self.pending_open = Some(path);
                                    self.new_or_open("open");
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.separator();
                    if ui.button("Save                  Ctrl+S").clicked() {
                        self.save(false);
                        ui.close();
                    }
                    if ui.button("Save As…").clicked() {
                        self.save(true);
                        ui.close();
                    }
                    ui.separator();
                    self.interchange_ui(ui, true);
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo    Ctrl+Z"))
                        .clicked()
                    {
                        self.history(Request::Undo);
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo    Ctrl+Y"))
                        .clicked()
                    {
                        self.history(Request::Redo);
                        ui.close();
                    }
                    self.edit_menu_items(ui);
                });
                ui.menu_button("View", |ui| {
                    if wide {
                        ui.checkbox(&mut self.show_left, "Project panel");
                        ui.checkbox(&mut self.show_right, "Properties panel");
                    } else {
                        ui.checkbox(&mut self.shell.project, "Project panel");
                        ui.checkbox(&mut self.shell.properties, "Properties panel");
                    }
                    let mut problems = self.shell.bottom.is_some();
                    if ui.checkbox(&mut problems, "Bottom panel    Ctrl+J").changed() {
                        self.shell.bottom = problems.then_some(BottomTab::Problems);
                    }
                    ui.checkbox(&mut self.shell.focus, "Focus mode    Ctrl+.");
                    ui.checkbox(&mut self.canvas.grid_snap, "Snap to 10 px grid");
                    ui.checkbox(&mut self.canvas.space_tool, "Space tool (S)")
                        .on_hover_text("Drag on the canvas to insert space; drag back to remove it. Ctrl moves the shapes before the line.");
                    ui.separator();
                    if ui
                        .selectable_value(&mut self.theme, Theme::Light, "Light theme")
                        .changed()
                        || ui
                            .selectable_value(&mut self.theme, Theme::Dark, "Dark theme")
                            .changed()
                    {
                        self.apply_theme(&ctx);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("UI 100%").clicked() {
                        ctx.set_zoom_factor(1.0);
                        ui.close();
                    }
                    if ui.button("UI 200%").clicked() {
                        ctx.set_zoom_factor(2.0);
                        ui.close();
                    }
                });
                ui.menu_button("Model", |ui| {
                    if ui.button("Data & collaboration…").clicked() {
                        self.open_collaboration();
                        ui.close();
                    }
                    if ui.button("Auto layout…").clicked() {
                        self.start_layout(&ctx);
                        ui.close();
                    }
                    if ui
                        .button("Auto layout selected sub-choreography…")
                        .clicked()
                    {
                        self.start_selected_choreography_layout(&ctx);
                        ui.close();
                    }
                    if ui.button("Element comments…").clicked() {
                        self.open_comments();
                        self.shell.bottom = Some(BottomTab::Comments);
                        ui.close();
                    }
                    if ui.button("Checkpoints and reference…").clicked() {
                        self.shell.bottom = Some(BottomTab::Revisions);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("New model").clicked() {
                        self.new_model();
                        ui.close();
                    }
                    self.model_selectors(ui);
                    if ui.button("Validate model").clicked() {
                        self.shell.bottom = Some(BottomTab::Problems);
                        ui.close();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Settings…").clicked() {
                        self.shell.automation = true;
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if toolbar_room {
                        ui.label(egui::RichText::new("ProcessCraft").strong().color(p.muted));
                    }
                });
            });
        });
        if !compact_height && !focus {
            self.document_tabs_ui(ui);
        }
        let body = ui.available_rect_before_wrap();
        let footer_height = 28.0;
        let content = Rect::from_min_max(
            body.min,
            egui::pos2(
                body.right(),
                (body.bottom() - footer_height).max(body.top()),
            ),
        );

        let project_width = if self.show_left && wide && !focus {
            self.left_width.clamp(180.0, 300.0)
        } else {
            0.0
        };
        let inspector_width = if self.view == 0 && self.show_right && wide && !focus {
            self.right_width.clamp(280.0, 420.0)
        } else {
            0.0
        };
        let rail = Rect::from_min_size(
            content.min,
            egui::vec2(if focus { 0.0 } else { 48.0 }, content.height()),
        );
        if !focus {
            self.shell_panel(ui, "navigation", rail, p.panel, true, |app, ui| {
                app.navigation_ui(ui)
            });
        }
        let project_rect = Rect::from_min_size(
            egui::pos2(rail.right(), content.top()),
            egui::vec2(project_width, content.height()),
        );
        if project_width > 0.0 {
            self.shell_panel(ui, "project", project_rect, p.panel, true, |app, ui| {
                app.project_ui(ui)
            });
        }
        let central = Rect::from_min_max(
            egui::pos2(project_rect.right(), content.top()),
            egui::pos2(content.right() - inspector_width, content.bottom()),
        );
        self.shell_panel(ui, "content", central, p.canvas, false, |app, ui| {
            if app.view == 0 {
                if !focus {
                    egui::Frame::new()
                        .fill(p.base)
                        .inner_margin(egui::Margin {
                            left: 16,
                            right: 12,
                            top: 8,
                            bottom: 8,
                        })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
                            if compact_height {
                                app.compact_document_header(ui);
                            } else {
                                app.document_header(ui);
                            }
                            app.layout_bar(ui);
                        });
                    let line = ui.available_rect_before_wrap();
                    ui.painter().hline(
                        line.x_range(),
                        line.top(),
                        egui::Stroke::new(1.0, p.border),
                    );
                }
                if app.shell.bottom.is_some() && !focus {
                    let max = (ui.available_height() * 0.6).max(80.0);
                    egui::Panel::bottom("bottom_panel")
                        .resizable(true)
                        .frame(
                            egui::Frame::new()
                                .fill(p.base)
                                .inner_margin(egui::Margin::symmetric(12, 4)),
                        )
                        .default_size((ui.available_height() * 0.3).clamp(80.0, 260.0))
                        .size_range(80.0..=max)
                        .show(ui, |ui| app.bottom_panel_ui(ui));
                }
                app.canvas_ui(ui);
            } else {
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .id_salt(("data_view", app.view))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        app.analyst_views_panel(ui);
                    });
            }
        });
        if project_width > 0.0 {
            let handle = Rect::from_center_size(
                egui::pos2(project_rect.right(), content.center().y),
                egui::vec2(6.0, content.height()),
            );
            if ui
                .interact(
                    handle,
                    egui::Id::unique("project-pane-resize"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .dragged()
            {
                self.left_width =
                    (self.left_width + ctx.input(|i| i.pointer.delta().x)).clamp(180.0, 300.0);
            }
        }
        if inspector_width > 0.0 {
            let rect = Rect::from_min_max(
                egui::pos2(content.right() - inspector_width, content.top()),
                content.max,
            );
            self.shell_panel(ui, "properties", rect, p.base, true, |app, ui| {
                app.inspector_ui(ui, true)
            });
            let handle = Rect::from_center_size(
                egui::pos2(rect.left(), content.center().y),
                egui::vec2(6.0, content.height()),
            );
            if ui
                .interact(
                    handle,
                    egui::Id::unique("properties-pane-resize"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .dragged()
            {
                self.right_width =
                    (self.right_width - ctx.input(|i| i.pointer.delta().x)).clamp(280.0, 420.0);
            }
        }
        let footer = Rect::from_min_max(egui::pos2(body.left(), content.bottom()), body.max);
        self.shell_panel(ui, "footer", footer, p.panel, true, |app, ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.horizontal_centered(|ui| {
                let small = |text: &str| egui::RichText::new(text).size(11.0).color(p.muted);
                if app.ctl.uncertain && ui.small_button("Check save outcome").clicked() {
                    app.ctl.reconcile();
                }
                widgets::icon(ui, Icon::HardDrive, 14.0, p.faint);
                if let Some(file) = &app.ctl.file {
                    ui.add_sized(
                        [if toolbar_room { 200.0 } else { 110.0 }, 18.0],
                        egui::Label::new(
                            small(
                                &file
                                    .target()
                                    .file_name()
                                    .map_or_else(|| "Project".into(), |n| n.to_string_lossy()),
                            )
                            .color(p.text),
                        )
                        .truncate(),
                    )
                    .on_hover_text(file.target().display().to_string());
                    if !file.is_writable() {
                        ui.label(small("Read only · use Save As").color(p.warning));
                    }
                } else {
                    ui.label(small("Local project").color(p.text));
                }
                let (state, color) = if app.ctl.is_saving() {
                    ("Saving…", p.muted)
                } else if snap.dirty || app.has_drafts() {
                    ("Unsaved changes", p.accent_ink)
                } else if app.ctl.file.is_none() {
                    ("New project", p.muted)
                } else {
                    ("Saved", p.muted)
                };
                ui.label(small(state).color(color));
                if app.ctl.migration {
                    ui.label(small("Migration · Save As required").color(p.warning));
                }
                if app.view == 0
                    && toolbar_room
                    && let Some(stats) = app.diagram_stats(&snap)
                {
                    ui.label(small("·"));
                    ui.label(small(&stats));
                }
                if app.effects.busy() {
                    ui.spinner();
                    ui.label(small(app.effects.label));
                }
                // Success notices are transient: shown for a few seconds, then fade out.
                let age = app.ctl.info_at.elapsed().as_secs_f32();
                if !app.ctl.info.is_empty() {
                    if age >= INFO_SECONDS {
                        app.ctl.info.clear();
                    } else {
                        let alpha = (INFO_SECONDS - age).min(1.0);
                        ui.label(small(&app.ctl.info).color(p.success.gamma_multiply(alpha)));
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(50));
                    }
                }
                if toolbar_room {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(small("BPMN 2.0 · Local, offline"));
                    });
                }
            });
        });
        ui.advance_cursor_after_rect(body);
        {
            // Errors float above the footer so they never move the canvas, wrap instead of
            // truncating, and stay until dismissed. The area exists every frame (empty when there
            // is no error) so a new message is visible on its first frame.
            // Fixed width and top anchor: the position never depends on the wrapped height.
            let error_width = (body.width() - 40.0).clamp(150.0, 520.0);
            egui::Area::new(egui::Id::unique("status_error"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(
                    body.right() - 12.0 - error_width - 16.0,
                    body.top() + 8.0,
                ))
                .show(&ctx, |ui| {
                    if self.ctl.error.is_empty() {
                        return;
                    }
                    egui::Frame::popup(ui.style())
                        .fill(p.warning_soft)
                        .stroke(egui::Stroke::new(1.0, p.warning))
                        .show(ui, |ui| {
                            ui.set_width(error_width);
                            let message = self.ctl.error.clone();
                            let label = ui.add(
                                egui::Label::new(egui::RichText::new(&message).color(p.warning))
                                    .wrap(),
                            );
                            label.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Label, true, &message)
                            });
                            ui.horizontal(|ui| {
                                let dismiss = ui.small_button("Dismiss message");
                                #[cfg(test)]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::unique("test_dismiss_rect"),
                                        dismiss.rect,
                                    )
                                });
                                if dismiss.clicked() {
                                    self.ctl.error.clear();
                                }
                                if self.ctl.uncertain
                                    && ui.small_button("Check save outcome").clicked()
                                {
                                    self.ctl.reconcile();
                                }
                            });
                        });
                });
        }
        self.review_windows(&ctx);
        self.collaboration_windows(&ctx);
        let window_height = (ctx.content_rect().height() - 64.0).max(100.0);
        let window_width = (ctx.content_rect().width() - 32.0).max(150.0);
        if self.shell.project {
            let mut open = true;
            egui::Window::new("Project")
                .open(&mut open)
                .default_width(260.0)
                .max_width(window_width)
                .default_height(window_height.min(560.0))
                .max_height(window_height)
                .show(&ctx, |ui| self.project_ui(ui));
            self.shell.project = open;
        }
        if self.shell.properties {
            let mut open = true;
            egui::Window::new("Properties")
                .open(&mut open)
                .default_width(310.0)
                .max_width(window_width)
                .default_height(window_height.min(560.0))
                .max_height(window_height)
                .show(&ctx, |ui| self.inspector_ui(ui, false));
            self.shell.properties = open;
        }
        if self.shell.automation {
            let mut open = true;
            egui::Window::new("Settings")
                .open(&mut open)
                .collapsible(false)
                .default_width(530.0)
                .max_width(window_width)
                .default_height(window_height.min(560.0))
                .max_height(window_height)
                .vscroll(true)
                .show(&ctx, |ui| {
                    widgets::section(ui, "Appearance", None, true, |ui| {
                        ui.horizontal(|ui| {
                            let before = self.theme;
                            ui.selectable_value(&mut self.theme, Theme::Light, "Light");
                            ui.selectable_value(&mut self.theme, Theme::Dark, "Dark");
                            if before != self.theme {
                                self.apply_theme(ui.ctx());
                            }
                        });
                    });
                    widgets::section(ui, "Local automation", None, true, |ui| {
                        self.automation_ui(ui)
                    });
                });
            self.shell.automation = open;
        }
    }

    fn shell_panel(
        &mut self,
        parent: &mut egui::Ui,
        id: &'static str,
        rect: Rect,
        fill: Color32,
        padded: bool,
        add: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        let p = self.theme.palette();
        parent.painter().rect_filled(rect, 0.0, fill);
        // Hairline seams between panels instead of boxed outlines.
        let seam = egui::Stroke::new(1.0, p.border);
        parent
            .painter()
            .vline(rect.right() - 0.5, rect.y_range(), seam);
        parent
            .painter()
            .hline(rect.x_range(), rect.top() + 0.5, seam);
        let inner = if padded {
            rect.shrink2(egui::vec2(
                10.0,
                if rect.height() < 50.0 { 4.0 } else { 10.0 },
            ))
        } else {
            rect.shrink2(egui::vec2(0.0, 1.0))
        };
        let mut child = parent.new_child(egui::UiBuilder::new().id_salt(id).max_rect(inner));
        child.set_clip_rect(rect);
        child.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        add(self, &mut child);
    }
    fn document_tabs_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let p = self.theme.palette();
        let mut close = None;
        egui::ScrollArea::horizontal()
            .id_salt("document_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for id in self.shell.tabs.clone() {
                        let Some((index, doc)) = snap
                            .project
                            .documents
                            .iter()
                            .enumerate()
                            .find(|(_, d)| d.id == id)
                        else {
                            continue;
                        };
                        let selected = index == self.doc && matches!(self.view, 0 | 3);
                        let tab = egui::Frame::new()
                            .fill(if selected { p.base } else { p.panel })
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    widgets::icon(
                                        ui,
                                        Icon::Workflow,
                                        16.0,
                                        if selected { p.accent } else { p.muted },
                                    );
                                    if ui.selectable_label(selected, &doc.name).clicked()
                                        && self.prepare_navigation()
                                    {
                                        self.doc = index;
                                        self.process_id = None;
                                        self.diagram_id = None;
                                        self.select(None);
                                        self.view = 0;
                                    }
                                    if widgets::icon_button_sized(
                                        ui,
                                        Icon::X,
                                        &format!("Close {} tab", doc.name),
                                        false,
                                        24.0,
                                    )
                                    .clicked()
                                    {
                                        close = Some(id);
                                    }
                                });
                            });
                        if selected {
                            ui.painter().line_segment(
                                [
                                    tab.response.rect.left_bottom(),
                                    tab.response.rect.right_bottom(),
                                ],
                                egui::Stroke::new(2.0, p.accent),
                            );
                        }
                    }
                    if self.shell.tabs.is_empty() {
                        ui.label(egui::RichText::new("No open models").small().color(p.muted));
                    }
                    if widgets::icon_button(ui, Icon::Plus, "New model", false).clicked() {
                        self.new_model();
                    }
                });
            });
        if let Some(id) = close {
            self.close_tab(id);
        }
    }
    fn close_tab(&mut self, id: DocumentId) {
        let snap = self.ctl.session.snapshot();
        let active = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.id == id);
        if active && !self.prepare_navigation() {
            return;
        }
        self.shell.tabs.retain(|tab| *tab != id);
        if active {
            if let Some(index) = self
                .shell
                .tabs
                .first()
                .and_then(|id| snap.project.documents.iter().position(|d| d.id == *id))
            {
                self.doc = index;
                self.process_id = None;
                self.diagram_id = None;
                self.select(None);
            } else {
                self.view = 1;
            }
        }
    }
    pub(super) fn prepare_navigation(&mut self) -> bool {
        if self.collaboration.has_draft() {
            self.ctl.error = "Apply or discard the collaboration draft before navigating.".into();
            return false;
        }
        if self.review.has_draft() {
            self.ctl.error = "Post or discard the comment draft before navigating.".into();
            return false;
        }
        if self.dictionary_draft_changed(&self.ctl.session.snapshot().project) {
            self.ctl.error = "Save or discard the dictionary draft before navigating.".into();
            false
        } else {
            self.commit_all_drafts()
        }
    }
    fn new_model(&mut self) {
        if self.prepare_navigation()
            && self.ctl.mutate(vec![Mutation::CreateDocument {
                document: controller::blank_document(),
            }])
        {
            self.doc = self
                .ctl
                .session
                .snapshot()
                .project
                .documents
                .len()
                .saturating_sub(1);
            self.process_id = None;
            self.diagram_id = None;
            self.select(None);
            self.view = 0;
        }
    }
    fn navigation_ui(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.vertical_centered(|ui| {
            for (index, icon, label) in [
                (1, Icon::Folder, "Library"),
                (0, Icon::Workflow, "Model"),
                (2, Icon::Book, "Dictionary"),
                (3, Icon::Table, "Table"),
            ] {
                if widgets::icon_button(ui, icon, label, self.view == index).clicked()
                    && self.prepare_navigation()
                {
                    self.view = index;
                }
            }
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Settings, "Settings", self.shell.automation).clicked()
            {
                self.shell.automation = true;
            }
            let (left, right) = if self.shell.wide {
                (self.show_left, self.show_right)
            } else {
                (self.shell.project, self.shell.properties)
            };
            if widgets::icon_button(ui, Icon::PanelRight, "Properties panel", right).clicked() {
                if self.shell.wide {
                    self.show_right = !self.show_right;
                } else {
                    self.shell.properties = !self.shell.properties;
                }
            }
            if widgets::icon_button(ui, Icon::PanelLeft, "Project panel", left).clicked() {
                if self.shell.wide {
                    self.show_left = !self.show_left;
                } else {
                    self.shell.project = !self.shell.project;
                }
            }
            if widgets::icon_button(ui, Icon::Focus, "Focus mode (Ctrl+.)", self.shell.focus)
                .clicked()
            {
                self.shell.focus = !self.shell.focus;
                self.view = 0;
            }
        });
    }
    fn project_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let p = self.theme.palette();
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(egui::RichText::new(&snap.project.name).font(crate::theme::semibold(14.0)));
        ui.label(
            egui::RichText::new("Local workspace")
                .size(11.0)
                .color(p.faint),
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.shell.search)
                .hint_text("Search project")
                .desired_width(f32::INFINITY),
        )
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::SearchInput, true, "Search project"));
        ui.add_space(4.0);
        widgets::section_label(ui, "Models");
        let search = self.shell.search.clone();
        let outline = self.view == 0;
        egui::ScrollArea::both()
            .id_salt("project_tree")
            .max_height(if outline {
                (ui.available_height() - 44.0).max(90.0)
            } else {
                ui.available_height()
            })
            .show(ui, |ui| self.project_tree_ui(ui, &search));
        if !outline {
            return;
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            egui::CollapsingHeader::new(
                egui::RichText::new("Outline").font(crate::theme::medium(12.0)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("model_outline")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        if let Some(doc) = snap.project.documents.get(self.doc) {
                            for process in processcraft_core::queries::scopes(doc) {
                                // Unnamed elements show their type, never a raw XML ID.
                                for (id, name, kind) in process
                                    .lanes
                                    .iter()
                                    .map(|l| (l.id.selector(), l.name.as_deref(), "Lane"))
                                    .chain(process.nodes.iter().map(|n| {
                                        (
                                            n.id.selector(),
                                            n.name.as_deref(),
                                            super::node_kind_label(n.kind),
                                        )
                                    }))
                                    .chain(process.flows.iter().map(|f| {
                                        (
                                            f.id.selector(),
                                            f.name.as_deref(),
                                            flow_kind_label(f.kind),
                                        )
                                    }))
                                {
                                    if ui
                                        .selectable_label(
                                            self.selected.as_ref() == Some(&id),
                                            name.filter(|n| !n.is_empty()).unwrap_or(kind),
                                        )
                                        .clicked()
                                    {
                                        self.select(Some(id.clone()));
                                    }
                                }
                            }
                        }
                    });
            });
        });
    }
    /// Lane and activity counts of the shown process, for the status bar.
    fn diagram_stats(&self, snap: &processcraft_core::commands::Snapshot) -> Option<String> {
        let doc = snap.project.documents.get(self.doc)?;
        let scopes = processcraft_core::queries::scopes(doc);
        let process = scopes.iter().find(|scope| {
            self.process_id
                .as_ref()
                .is_none_or(|id| scope.id.selector() == *id)
        })?;
        let activities = process
            .nodes
            .iter()
            .filter(|n| {
                use processcraft_core::rules::{is_event, is_flow_node, is_gateway};
                is_flow_node(n.kind) && !is_event(n.kind) && !is_gateway(n.kind)
            })
            .count();
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        Some(format!(
            "{} · {}",
            plural(process.lanes.len(), "lane"),
            plural(activities, "activity").replace("activitys", "activities")
        ))
    }
    /// Undo, Redo, Save, Export and Validate: the document's own actions, beside its title.
    fn document_actions(&mut self, ui: &mut egui::Ui, labels: bool) {
        let snap = self.ctl.session.snapshot();
        let can_undo = snap.undo_count > 0 || self.has_drafts();
        let can_redo = snap.redo_count > 0;
        let validate = if labels {
            widgets::primary_button(ui, "Validate")
        } else {
            widgets::icon_button(
                ui,
                Icon::Check,
                "Validate model",
                self.shell.bottom == Some(BottomTab::Problems),
            )
        };
        if validate.on_hover_text("Validate model").clicked() {
            self.shell.bottom = if self.shell.bottom == Some(BottomTab::Problems) {
                None
            } else {
                Some(BottomTab::Problems)
            };
        }
        let export = if labels {
            ui.menu_button("Export", |ui| self.interchange_ui(ui, false))
                .response
        } else {
            let response = widgets::icon_button(ui, Icon::Download, "Export", false);
            egui::Popup::menu(&response).show(|ui| self.interchange_ui(ui, false));
            response
        };
        let _ = export;
        if widgets::icon_button(ui, Icon::Save, "Save (Ctrl+S)", false).clicked() {
            self.save(false);
        }
        ui.add_space(4.0);
        if ui
            .add_enabled_ui(can_redo, |ui| {
                widgets::icon_button(ui, Icon::Redo, "Redo (Ctrl+Y)", false)
            })
            .inner
            .clicked()
        {
            self.history(Request::Redo);
        }
        if ui
            .add_enabled_ui(can_undo, |ui| {
                widgets::icon_button(ui, Icon::Undo, "Undo (Ctrl+Z)", false)
            })
            .inner
            .clicked()
        {
            self.history(Request::Undo);
        }
    }
    fn compact_document_header(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.ctl.session.snapshot();
        ui.horizontal(|ui| {
            if let Some(document) = snapshot.project.documents.get(self.doc) {
                ui.add_sized(
                    [ui.available_width().min(220.0), 24.0],
                    egui::Label::new(
                        egui::RichText::new(&document.name).font(crate::theme::semibold(14.0)),
                    )
                    .truncate(),
                )
                .on_hover_text(&document.name);
            }
            ui.menu_button("View", |ui| {
                if ui.button("Table").clicked() && self.prepare_navigation() {
                    self.view = 3;
                    ui.close();
                }
                ui.separator();
                self.model_selectors(ui);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.document_actions(ui, false)
            });
        });
    }
    fn document_header(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let p = self.theme.palette();
        if let Some(doc) = snap.project.documents.get(self.doc) {
            ui.label(
                egui::RichText::new(format!(
                    "{}  ›  {}",
                    snap.project.name,
                    doc.folder
                        .as_ref()
                        .and_then(|id| snap.project.folders.iter().find(|f| f.id == *id))
                        .map_or("Models", |f| f.name.as_str())
                ))
                .size(11.0)
                .color(p.faint),
            );
        }
        ui.horizontal(|ui| {
            if let Some(doc) = snap.project.documents.get(self.doc) {
                ui.label(egui::RichText::new(&doc.name).font(crate::theme::semibold(18.0)));
                // Only limited documents get a badge; an editable model needs no label.
                let badge = match &doc.capability {
                    Capability::Editable => None,
                    Capability::RetainedExtensions { .. } => Some(("Read-only structure", "Source extensions are retained. Metadata editing remains available; structural editing is disabled.")),
                    Capability::PreservedSource { .. } => Some(("Preserved source", "Original BPMN source is retained. A semantic preview is unavailable.")),
                };
                if let Some((label, hint)) = badge {
                    widgets::badge(ui, label, p.warning, p.warning_soft).on_hover_text(hint);
                }
            }
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(p.secondary)
                .corner_radius(crate::theme::RADIUS)
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.spacing_mut().button_padding = egui::vec2(10.0, 2.0);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal(|ui| {
                        let _ = segment(ui, true, "Model");
                        if segment(ui, false, "Table").clicked() && self.prepare_navigation() {
                            self.view = 3;
                        }
                    });
                });
            ui.menu_button("Diagram ▾", |ui| self.model_selectors(ui));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.document_actions(ui, ui.available_width() > 360.0)
            });
        });
    }
    fn model_selectors(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        if let Some(doc) = snap.project.documents.get(self.doc) {
            ui.strong("Processes and subprocesses");
            for process in processcraft_core::queries::scopes(doc) {
                if ui
                    .selectable_label(
                        self.process_id
                            .clone()
                            .or_else(|| doc.processes.first().map(|p| p.id.selector()))
                            == Some(process.id.selector()),
                        process.name,
                    )
                    .clicked()
                    && self.prepare_navigation()
                {
                    self.process_id = Some(process.id.selector());
                    self.select(None);
                }
            }
            ui.separator();
            ui.strong("Collaboration and choreography scopes");
            for root in doc
                .collaboration
                .iter()
                .chain(&doc.additional_collaborations)
            {
                let label = format!(
                    "Collaboration · {}",
                    root.name.as_deref().unwrap_or("Unnamed")
                );
                ui.push_id(root.id.selector(), |ui| {
                    self.scope_diagram_ui(ui, doc, &root.id.selector(), &label, snap.generation)
                });
            }
            for scope in processcraft_core::queries::choreography_scopes(doc) {
                let label = if scope.parent.is_none() {
                    format!(
                        "{} · {}",
                        if scope.root.kind.is_some() {
                            "Global choreography"
                        } else {
                            "Choreography"
                        },
                        scope.root.name.as_deref().unwrap_or("Unnamed")
                    )
                } else {
                    let name = processcraft_core::queries::choreography_scopes(doc)
                        .iter()
                        .flat_map(|parent| parent.activities.iter())
                        .find(|activity| activity.id == scope.owner)
                        .and_then(|activity| activity.name.clone())
                        .unwrap_or_else(|| "Unnamed".to_owned());
                    format!("Sub-choreography · {name}")
                };
                ui.push_id(&scope.owner, |ui| {
                self.scope_diagram_ui(ui, doc, &scope.owner, &label, snap.generation);
                ui.menu_button(format!("Browse {label}"), |ui| {
                    self.graph_data_ui(ui, doc, &scope, doc.capability.allows_structural_editing(), snap.generation);
                    self.collaboration_artifacts_ui(ui, doc, &super::collaboration::choreography_view(scope.root), snap.generation, Some(&scope));
                    if scope.parent.is_none() && scope.root.graph.is_none() {
                        ui.scope(|ui| { ui.set_max_width(280.0); ui.add(egui::Label::new("Convert the legacy task and flow lists into an ordered graph. Their current list order is retained; original interleaving cannot be recovered.").wrap()); });
                        if ui.button("Convert legacy choreography to graph").clicked() && self.prepare_navigation() {
                            if self.ctl.session.snapshot().generation != snap.generation {
                                self.ctl.error = "The model changed; reopen this menu before converting.".into();
                            } else if self.ctl.mutate_at_generation(snap.generation, vec![Mutation::PromoteChoreographyGraph { document_id: doc.id, choreography_id: scope.root.id.selector() }]) {
                                ui.close();
                            }
                        }
                    }
                    if scope.root.graph.is_some() {
                        ui.menu_button("Add flow element", |ui| {
                            for kind in ["Task", "Sub-choreography", "Call choreography", "Start event", "End event", "Intermediate catch event", "Intermediate throw event", "Boundary event", "Exclusive gateway", "Parallel gateway", "Inclusive gateway", "Complex gateway", "Event-based gateway", "Sequence flow"] {
                                if ui.button(kind).clicked() && self.add_choreography_element(doc, &scope, kind, snap.generation) { ui.close(); }
                            }
                        });
                    }
                    for (index, element) in scope.ordered.iter().enumerate() {
                        use processcraft_core::queries::ChoreographyElementRef;
                        let (id, name, kind) = match element {
                            ChoreographyElementRef::Activity(value) => (&value.id, value.name.as_deref(), "Activity"),
                            ChoreographyElementRef::Event(value) => (&value.id, value.name.as_deref(), "Event"),
                            ChoreographyElementRef::Gateway(value) => (&value.id, value.name.as_deref(), "Gateway"),
                            ChoreographyElementRef::SequenceFlow(value) => (&value.id, value.name.as_deref(), "Sequence flow"),
                        };
                        let element_label = super::event::local_choice_label(name, kind, id, index + 1);
                        if ui.button(&element_label).clicked() && self.prepare_navigation() {
                            if let Some(diagram) = doc.diagrams.iter().find(|diagram| diagram.plane == scope.owner) {
                                self.diagram_id = Some(diagram.id.selector());
                            }
                            self.process_id = None;
                            self.select(Some(id.selector()));
                            ui.close();
                        }
                        if scope.root.graph.is_some() {
                            ui.menu_button(format!("Actions for {element_label}"), |ui| {
                                let element_id = id.selector();
                                let mut action = None;
                                if index > 0 && ui.button("Move earlier").clicked() {
                                    action = Some(Mutation::ReorderChoreographyFlowElement { document_id: doc.id, choreography_id: scope.root.id.selector(), scope_id: scope.owner.clone(), element_id: element_id.clone(), index: index - 1 });
                                }
                                if index + 1 < scope.ordered.len() && ui.button("Move later").clicked() {
                                    action = Some(Mutation::ReorderChoreographyFlowElement { document_id: doc.id, choreography_id: scope.root.id.selector(), scope_id: scope.owner.clone(), element_id: element_id.clone(), index: index + 1 });
                                }
                                for destination in processcraft_core::queries::choreography_scopes(doc).into_iter().filter(|candidate| candidate.root.id == scope.root.id && candidate.owner != scope.owner) {
                                    let destination_name = if destination.parent.is_none() { scope.root.name.as_deref().unwrap_or("Root").to_owned() } else { processcraft_core::queries::choreography_scopes(doc).iter().flat_map(|s| s.activities.iter()).find(|a| a.id == destination.owner).and_then(|a| a.name.clone()).unwrap_or_else(|| "Unnamed sub-choreography".to_owned()) };
                                    if ui.button(format!("Move to {destination_name}")).clicked() {
                                        action = Some(Mutation::ReparentChoreographyFlowElement { document_id: doc.id, choreography_id: scope.root.id.selector(), source_scope_id: scope.owner.clone(), target_scope_id: destination.owner, element_id: element_id.clone(), index: destination.ordered.len() });
                                    }
                                }
                                if ui.button("Delete element").clicked() {
                                    action = Some(Mutation::DeleteChoreographyFlowElement { document_id: doc.id, choreography_id: scope.root.id.selector(), scope_id: scope.owner.clone(), element_id });
                                }
                                if let Some(action) = action && self.prepare_navigation() {
                                    if self.ctl.session.snapshot().generation != snap.generation {
                                        self.ctl.error = "The model changed; reopen this menu before editing.".into();
                                    } else if self.ctl.mutate_at_generation(snap.generation, vec![action]) { ui.close(); }
                                }
                            });
                        }
                    }
                    if scope.ordered.is_empty() { ui.label("No flow elements in this scope."); }
                });
                });
            }
            ui.separator();
            ui.strong("Diagrams");
            for (index, diagram) in doc.diagrams.iter().enumerate() {
                if ui
                    .selectable_label(
                        self.diagram_id
                            .clone()
                            .or_else(|| doc.diagrams.first().map(|d| d.id.selector()))
                            == Some(diagram.id.selector()),
                        diagram_label(doc, diagram, index),
                    )
                    .clicked()
                    && self.prepare_navigation()
                {
                    self.diagram_id = Some(diagram.id.selector());
                    self.select(None);
                }
            }
        }
    }
    pub(super) fn add_choreography_element(
        &mut self,
        document: &BpmnDocument,
        scope: &processcraft_core::queries::ChoreographyScope<'_>,
        kind: &str,
        generation: u64,
    ) -> bool {
        if !self.prepare_navigation() {
            return false;
        }
        if self.ctl.session.snapshot().generation != generation {
            self.ctl.error = "The model changed; select this scope again before editing.".into();
            return false;
        }
        let id = controller::id("ChoreographyElement");
        let mut mutations = vec![Mutation::CreateChoreographyFlowElement {
            document_id: document.id,
            choreography_id: scope.root.id.selector(),
            scope_id: scope.owner.clone(),
            index: scope.ordered.len(),
            element: new_choreography_element(kind, id.clone()),
        }];
        if kind != "Sequence flow"
            && let Some(diagram) = document.diagrams.iter().find(|diagram| {
                self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id)
                    && diagram.plane == scope.owner
            })
        {
            let Some(bounds) = vacant_choreography_bounds(diagram, self.camera, kind) else {
                self.ctl.error = "No free position is available in this diagram.".into();
                return false;
            };
            mutations.push(Mutation::CreateShape {
                document_id: document.id,
                diagram_id: diagram.id.selector(),
                shape: Shape {
                    id: controller::id("Shape").into(),
                    element: id.clone().into(),
                    bounds,
                    di_metadata: None,
                    is_message_visible: None,
                    participant_band_kind: None,
                    choreography_activity_shape: None,
                    is_expanded: (kind == "Sub-choreography").then_some(false),
                    is_horizontal: None,
                    is_marker_visible: None,
                    label: None,
                    label_visibility: None,
                    style: None,
                },
            });
        }
        if self.ctl.mutate_at_generation(generation, mutations) {
            self.select(Some(id.into()));
            true
        } else {
            false
        }
    }
    pub(super) fn scope_diagram_ui(
        &mut self,
        ui: &mut egui::Ui,
        doc: &BpmnDocument,
        owner: &ElementSelector,
        label: &str,
        generation: u64,
    ) {
        ui.menu_button(label, |ui| {
            for (index, diagram) in doc
                .diagrams
                .iter()
                .filter(|diagram| &diagram.plane == owner)
                .enumerate()
            {
                let title = super::event::local_choice_label(
                    diagram.name.as_deref(),
                    "Diagram",
                    &diagram.id,
                    index + 1,
                );
                if ui.button(format!("Open {title}")).clicked() && self.prepare_navigation() {
                    self.diagram_id = Some(diagram.id.selector());
                    self.process_id = None;
                    self.select(None);
                    ui.close();
                }
            }
            if ui
                .add_enabled(
                    doc.capability.allows_structural_editing(),
                    egui::Button::new("Create diagram for this scope"),
                )
                .clicked()
                && self.prepare_navigation()
            {
                let id = controller::id("Diagram");
                let diagram = Diagram {
                    id: id.clone().into(),
                    plane_id: controller::id("Plane").into(),
                    plane: owner.clone(),
                    name: Some(label.to_owned()),
                    documentation: None,
                    resolution: None,
                    label_styles: Vec::new(),
                    plane_metadata: None,
                    plane_order: None,
                    shapes: Vec::new(),
                    edges: Vec::new(),
                };
                if self.ctl.mutate_at_generation(
                    generation,
                    vec![Mutation::CreateDiagram {
                        document_id: doc.id,
                        diagram,
                    }],
                ) {
                    self.diagram_id = Some(id.into());
                    self.process_id = None;
                    self.select(None);
                    ui.close();
                }
            }
        });
    }
    /// The docked bottom panel: Problems, Comments and Revisions as tabs.
    fn bottom_panel_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.theme.palette();
        let errors = self
            .problems
            .iter()
            .filter(|d| d.severity == processcraft_core::validation::DiagnosticSeverity::Error)
            .count();
        let comments = self.ctl.session.snapshot().project.comments.len();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (tab, icon, label, count) in [
                (
                    BottomTab::Problems,
                    Icon::Alert,
                    "Problems",
                    Some(self.problems.len()),
                ),
                (
                    BottomTab::Comments,
                    Icon::Comment,
                    "Comments",
                    Some(comments),
                ),
                (BottomTab::Revisions, Icon::History, "Revisions", None),
            ] {
                let selected = self.shell.bottom == Some(tab);
                let text = egui::RichText::new(label).font(crate::theme::medium(12.0));
                let galley = ui.painter().layout_no_wrap(
                    label.to_owned(),
                    crate::theme::medium(12.0),
                    p.text,
                );
                let badge = count.filter(|n| *n > 0).map(|n| n.to_string());
                let width = 32.0
                    + galley.size().x
                    + badge
                        .as_ref()
                        .map_or(0.0, |b| 10.0 + 7.0 * b.len() as f32 + 8.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, 32.0), egui::Sense::hover());
                let response = craft_ui::tabs::Tab::new(
                    egui::Id::unique(("bottom-tab", label)),
                    label,
                    selected,
                )
                .focus_stroke(egui::Stroke::new(1.5, p.accent))
                .show_at(ui, rect, |ui, response| {
                    let color = if selected || response.hovered() {
                        p.text
                    } else {
                        p.muted
                    };
                    widgets::paint_icon(
                        ui,
                        icon,
                        Rect::from_center_size(
                            rect.left_center() + egui::vec2(16.0, 0.0),
                            egui::Vec2::splat(15.0),
                        ),
                        if tab == BottomTab::Problems && errors > 0 {
                            p.danger
                        } else {
                            color
                        },
                    );
                    ui.painter().text(
                        rect.left_center() + egui::vec2(28.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        text.text(),
                        crate::theme::medium(12.0),
                        color,
                    );
                    if let Some(badge) = &badge {
                        let pill = Rect::from_min_size(
                            egui::pos2(rect.left() + 32.0 + galley.size().x, rect.center().y - 8.0),
                            egui::vec2(7.0 * badge.len() as f32 + 8.0, 16.0),
                        );
                        let (fill, ink) = if tab == BottomTab::Problems && errors > 0 {
                            (p.danger_soft, p.danger)
                        } else {
                            (p.secondary, p.muted)
                        };
                        ui.painter().rect_filled(pill, 8.0, fill);
                        ui.painter().text(
                            pill.center(),
                            egui::Align2::CENTER_CENTER,
                            badge,
                            crate::theme::medium(10.5),
                            ink,
                        );
                    }
                    if selected {
                        ui.painter().hline(
                            rect.x_range(),
                            rect.bottom() - 1.0,
                            egui::Stroke::new(2.0, p.accent),
                        );
                    }
                });
                if response.clicked() {
                    self.shell.bottom = Some(tab);
                    if tab == BottomTab::Comments {
                        self.open_comments();
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button_sized(ui, Icon::X, "Close panel", false, 26.0).clicked() {
                    self.shell.bottom = None;
                }
            });
        });
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(
            line.x_range(),
            line.top() - 3.0,
            egui::Stroke::new(1.0, p.border),
        );
        egui::ScrollArea::vertical()
            .id_salt(("bottom-panel", self.shell.bottom))
            .auto_shrink(false)
            .show(ui, |ui| match self.shell.bottom {
                Some(BottomTab::Problems) => self.problems_ui(ui),
                Some(BottomTab::Comments) => self.comments_ui(ui),
                Some(BottomTab::Revisions) => self.checkpoints_ui(ui),
                None => {}
            });
    }
    fn problems_ui(&mut self, ui: &mut egui::Ui) {
        let p = self.theme.palette();
        if self.problems.is_empty() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                widgets::icon(ui, Icon::Check, 16.0, p.success);
                ui.label(egui::RichText::new("No model problems found.").color(p.muted));
            });
            return;
        }
        for problem in self.problems.clone() {
            let error =
                problem.severity == processcraft_core::validation::DiagnosticSeverity::Error;
            let (row, response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, &problem.message)
            });
            if response.hovered() {
                ui.painter()
                    .rect_filled(row, crate::theme::RADIUS, p.secondary);
            }
            widgets::paint_icon(
                ui,
                Icon::Alert,
                Rect::from_center_size(
                    row.left_center() + egui::vec2(14.0, 0.0),
                    egui::Vec2::splat(14.0),
                ),
                if error { p.danger } else { p.warning },
            );
            ui.painter().text(
                row.left_center() + egui::vec2(30.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &problem.message,
                crate::theme::regular(12.5),
                p.text,
            );
            if response.clicked() && self.prepare_navigation() {
                let snap = self.ctl.session.snapshot();
                if let Some(index) = snap
                    .project
                    .documents
                    .iter()
                    .position(|d| d.id == problem.document_id)
                {
                    self.doc = index;
                    self.process_id = None;
                    self.diagram_id = None;
                    self.select(problem.element_id);
                }
            }
        }
    }
}

/// Tabs of the bottom panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum BottomTab {
    Problems,
    Comments,
    Revisions,
}

impl ProcessCraft {
    fn interchange_ui(&mut self, ui: &mut egui::Ui, include_import: bool) {
        let ctx = ui.ctx().clone();
        ui.add_enabled_ui(
            !self.effects.busy()
                && self.export_ready.is_none()
                && self.export_uncertain.is_none()
                && self.export_lost.is_none()
                && self.after_save.is_none()
                && self.confirm.is_none(),
            |ui| {
                if include_import
                    && ui.button("Import BPMN…").clicked()
                    && self.commit_draft()
                    && self.commit_dictionary_draft()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("BPMN", &["bpmn", "xml"])
                        .pick_file()
                {
                    self.effects.import(
                        ctx.clone(),
                        path,
                        self.ctl.session.id(),
                        self.ctl.session.snapshot().generation,
                    );
                }
                for (label, format) in [
                    ("Export BPMN…", None),
                    ("Export SVG…", Some(processcraft_render::ExportFormat::Svg)),
                    ("Export PNG…", Some(processcraft_render::ExportFormat::Png)),
                    ("Export PDF…", Some(processcraft_render::ExportFormat::Pdf)),
                ] {
                    if ui.button(label).clicked()
                        && self.commit_draft()
                        && self.commit_dictionary_draft()
                    {
                        let snap = self.ctl.session.snapshot();
                        if let Some(doc) = snap.project.documents.get(self.doc) {
                            let diagram = doc
                                .diagrams
                                .iter()
                                .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
                                .map(|d| d.id.selector());
                            let appearance = if self.theme == Theme::Dark {
                                Appearance::Dark
                            } else {
                                Appearance::Light
                            };
                            self.effects.export(
                                ctx.clone(),
                                snap.project.clone(),
                                doc.id,
                                diagram,
                                format,
                                appearance,
                            );
                        }
                    }
                }
            },
        );
    }
    fn automation_ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(windows)]
        let ctx = ui.ctx().clone();
        ui.label(
            "Connect local tools to this open project. Folder access requires an explicit grant.",
        );
        #[cfg(windows)]
        if self.attachment.is_none() {
            if ui.button("Enable attachment").clicked()
                && self.commit_draft()
                && self.commit_dictionary_draft()
            {
                let wake_context = ctx.clone();
                match crate::attachment::Attachment::start(
                    self.ctl.session.id(),
                    std::sync::Arc::new(move || wake_context.request_repaint()),
                ) {
                    Ok(attachment) => self.attachment = Some(attachment),
                    Err(error) => self.ctl.error = format!("Attachment failed: {error}"),
                }
            }
        } else {
            let may_disable = self
                .attachment
                .as_ref()
                .is_some_and(|a| !a.has_pending_effect());
            if ui
                .add_enabled(may_disable, egui::Button::new("Disable attachment"))
                .clicked()
            {
                self.attachment = None;
            }
        }
        #[cfg(windows)]
        if let Some(attachment) = &mut self.attachment {
            ui.vertical(|ui| {
                ui.label("Attachment enabled:");
                ui.monospace(attachment.endpoint());
                ui.label("Session:");
                ui.monospace(self.ctl.session.id());
                ui.label("Token:");
                ui.monospace(attachment.token());
                if ui.button("Authorize automation write folder…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                    && let Err(error) = attachment.authorize_root(path)
                {
                    self.ctl.error = format!("Could not authorize folder: {error}");
                }
                if attachment.has_pending_effect()
                    && ui.button("Check attachment export outcome").clicked()
                {
                    match attachment.reconcile_export() {
                        Ok(Some(true)) => self.ctl.notify("Attachment export confirmed."),
                        Ok(Some(false)) => {
                            self.ctl.error = "Attachment export was not committed.".into()
                        }
                        Ok(None) => {
                            self.ctl.error = "The attachment operation is still running.".into()
                        }
                        Err(error) => {
                            self.ctl.error = format!("Attachment export remains uncertain: {error}")
                        }
                    }
                }
            });
            if self.has_drafts() {
                ui.label("Attachment requests wait until the current draft is applied.");
            }
        }
    }
}

pub(super) fn flow_kind_label(kind: processcraft_core::model::FlowKind) -> &'static str {
    match kind {
        processcraft_core::model::FlowKind::Sequence => "Sequence flow",
        processcraft_core::model::FlowKind::Message => "Message flow",
        processcraft_core::model::FlowKind::Association => "Association",
    }
}
/// Diagram name, else the name of the element it shows, else its position.
fn diagram_label(doc: &BpmnDocument, diagram: &Diagram, index: usize) -> String {
    diagram
        .name
        .clone()
        .or_else(|| {
            processcraft_core::queries::element_name(doc, &diagram.plane).map(str::to_owned)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("Diagram {}", index + 1))
}
/// "More BPMN elements" palette entries; catch and boundary events name their trigger, so no
/// trigger (or timer) is picked silently.
pub(super) fn more_palette_entries() -> Vec<(&'static str, super::canvas::PaletteEntry)> {
    use processcraft_core::replace::Trigger;
    let entry = |kind, trigger| super::canvas::PaletteEntry { kind, trigger };
    vec![
        ("Text annotation", NodeKind::TextAnnotation.into()),
        ("Service task", NodeKind::ServiceTask.into()),
        ("Send task", NodeKind::SendTask.into()),
        ("Receive task", NodeKind::ReceiveTask.into()),
        ("Script task", NodeKind::ScriptTask.into()),
        ("Business rule task", NodeKind::BusinessRuleTask.into()),
        ("Inclusive gateway", NodeKind::InclusiveGateway.into()),
        ("Complex gateway", NodeKind::ComplexGateway.into()),
        ("Event-based gateway", NodeKind::EventBasedGateway.into()),
        (
            "Intermediate throw event",
            NodeKind::IntermediateThrowEvent.into(),
        ),
        (
            "Message intermediate catch event",
            entry(NodeKind::IntermediateCatchEvent, Some(Trigger::Message)),
        ),
        (
            "Timer intermediate catch event",
            entry(NodeKind::IntermediateCatchEvent, Some(Trigger::Timer)),
        ),
        (
            "Message boundary event (selected activity)",
            entry(NodeKind::BoundaryEvent, Some(Trigger::Message)),
        ),
        (
            "Timer boundary event (selected activity)",
            entry(NodeKind::BoundaryEvent, Some(Trigger::Timer)),
        ),
        ("Subprocess", NodeKind::SubProcess.into()),
        ("Ad-hoc subprocess", NodeKind::AdHocSubProcess.into()),
        ("Transaction", NodeKind::Transaction.into()),
    ]
}
/// One option of a segmented control.
fn segment(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let p = crate::theme::current(ui);
    let text = egui::RichText::new(label).font(crate::theme::medium(12.0));
    ui.add(
        egui::Button::new(if selected {
            text.color(p.text)
        } else {
            text.color(p.muted)
        })
        .fill(if selected {
            p.base
        } else {
            Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE)
        .corner_radius(crate::theme::RADIUS_SMALL)
        .min_size(egui::vec2(0.0, 24.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_arrival_and_dismissal_do_not_move_canvas_or_task_controls() {
        for theme in [Theme::Light, Theme::Dark] {
            for size in [Vec2::new(1560.0, 975.0), Vec2::new(780.0, 487.5)] {
                let mut app = ProcessCraft {
                    theme,
                    ..Default::default()
                };
                app.shell.project = size.x < 980.0;
                let context = egui::Context::default();
                context.enable_accesskit();
                app.apply_theme(&context);
                let frame = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            events,
                            ..Default::default()
                        },
                        |ui| app.workbench_ui(ui),
                    );
                    let canvas = context
                        .data(|data| data.get_temp::<Rect>(egui::Id::unique("test_canvas_rect")))
                        .expect("canvas");
                    let task = context
                        .data(|data| {
                            data.get_temp::<Rect>(egui::Id::unique("test_task_palette_rect"))
                        })
                        .expect("task control");
                    output.textures_delta.clear();
                    (output, canvas, task)
                };
                // Stabilize window placement before sampling.
                let _ = frame(&mut app, vec![]);
                let (_, canvas_before, task_before) = frame(&mut app, vec![]);
                let message =
                    "Recovery snapshot failed: a detailed error that must stay accessible";
                app.ctl.error = message.into();
                let (output, canvas_error, task_error) = frame(&mut app, vec![]);
                assert_eq!(canvas_error, canvas_before);
                assert_eq!(task_error, task_before);
                let tree = output
                    .platform_output
                    .accesskit_update
                    .expect("accessibility update");
                assert!(
                    tree.nodes
                        .iter()
                        .any(|(_, node)| node.label() == Some(message)
                            || node.value() == Some(message)),
                    "full status text is accessible: {:?}",
                    tree.nodes
                        .iter()
                        .filter_map(|(_, node)| node.label())
                        .collect::<Vec<_>>()
                );
                let dismiss = context
                    .data(|data| data.get_temp::<Rect>(egui::Id::unique("test_dismiss_rect")))
                    .expect("dismiss control")
                    .center();
                let button = |pressed| egui::Event::PointerButton {
                    pos: dismiss,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                let _ = frame(
                    &mut app,
                    vec![egui::Event::PointerMoved(dismiss), button(true)],
                );
                let (_, canvas_after, task_after) = frame(&mut app, vec![button(false)]);
                assert!(
                    app.ctl.error.is_empty(),
                    "dismiss did not fire: size={size:?}, at={dismiss:?}"
                );
                assert_eq!(canvas_after, canvas_before);
                assert_eq!(task_after, task_before);
            }
        }
    }
    #[test]
    fn bottom_panel_tabs_replace_the_review_windows() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        context.enable_accesskit();
        app.apply_theme(&context);
        for (tab, expected) in [
            (BottomTab::Problems, "No model problems found."),
            (BottomTab::Comments, "Plain-text local notes"),
            (BottomTab::Revisions, "Named local snapshots"),
        ] {
            app.shell.bottom = Some(tab);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.workbench_ui(ui),
            );
            output.textures_delta.clear();
            let tree = output
                .platform_output
                .accesskit_update
                .expect("accessibility");
            let labels: Vec<_> = tree
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or(node.value()))
                .collect();
            assert!(
                labels.iter().any(|label| label.contains(expected)),
                "{tab:?}: {labels:?}"
            );
            // No floating window duplicates the docked review content.
            assert!(!labels.contains(&"Local comments"));
        }
        app.shell.focus = true;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| app.workbench_ui(ui),
        );
        output.textures_delta.clear();
        let tree = output
            .platform_output
            .accesskit_update
            .expect("accessibility");
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Close panel")),
            "focus mode hides the bottom panel"
        );
    }
    #[test]
    fn inspector_sections_host_collaboration_and_settings() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        context.enable_accesskit();
        app.apply_theme(&context);
        let labels = |app: &mut ProcessCraft| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.workbench_ui(ui),
            );
            output.textures_delta.clear();
            let tree = output
                .platform_output
                .accesskit_update
                .expect("accessibility");
            tree.nodes
                .iter()
                .filter_map(|(_, node)| node.label().map(str::to_owned))
                .collect::<Vec<_>>()
        };
        let closed = labels(&mut app);
        for section in ["General", "Data & collaboration", "Advanced BPMN"] {
            assert!(closed.iter().any(|l| l == section), "{section}: {closed:?}");
        }
        assert!(!closed.iter().any(|l| l == "Partners and endpoints"));
        // The Model menu command opens the section in place of the old floating window.
        app.open_collaboration();
        let _ = labels(&mut app);
        let open = labels(&mut app);
        assert!(
            open.iter().any(|l| l == "Partners and endpoints"),
            "{open:?}"
        );
        assert!(!open.iter().any(|l| l == "Collaboration and data"));
        app.shell.automation = true;
        let _ = labels(&mut app);
        let settings = labels(&mut app);
        assert!(
            settings.iter().any(|l| l == "Local automation"),
            "{settings:?}"
        );
        assert!(settings.iter().any(|l| l == "Appearance"));
    }
    #[test]
    fn short_viewport_keeps_canvas_usable_in_both_themes() {
        let mut app = ProcessCraft::default();
        let context = egui::Context::default();
        for theme in [Theme::Light, Theme::Dark] {
            app.theme = theme;
            app.apply_theme(&context);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(780.0, 487.0))),
                    ..Default::default()
                },
                |ui| app.workbench_ui(ui),
            );
            output.textures_delta.clear();
            let size = context
                .data(|data| data.get_temp::<Vec2>(egui::Id::unique("test_canvas_size")))
                .expect("canvas rendered");
            assert!(size.y >= 180.0, "canvas too short: {}", size.y);
        }
    }
    #[test]
    fn workbench_preserves_drafts_and_renders_both_layouts() {
        let mut app = ProcessCraft {
            dictionary_name: "Unsaved role".into(),
            ..Default::default()
        };
        let before = app.ctl.session.snapshot();
        assert!(!app.prepare_navigation());
        assert_eq!(app.dictionary_name, "Unsaved role");
        assert_eq!(app.ctl.session.snapshot().content_id, before.content_id);
        let id = before.project.documents[0].id;
        app.shell.tabs = vec![id];
        app.close_tab(id);
        assert_eq!(app.shell.tabs, vec![id]);
        app.dictionary_name.clear();
        assert!(app.prepare_navigation());
        app.close_tab(id);
        assert!(app.shell.tabs.is_empty());
        assert_eq!(app.view, 1);
        assert_eq!(
            app.ctl.session.snapshot().project.documents.len(),
            before.project.documents.len()
        );
        let context = egui::Context::default();
        for theme in [Theme::Light, Theme::Dark] {
            app.theme = theme;
            app.apply_theme(&context);
            for width in [320.0, 720.0, 1440.0] {
                app.shell.properties = width < 980.0;
                for view in 0..4 {
                    app.view = view;
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.workbench_ui(ui),
                    );
                    output.textures_delta.clear();
                }
            }
        }
        assert_eq!(app.ctl.session.snapshot().content_id, before.content_id);
    }
}
