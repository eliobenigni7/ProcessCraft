use super::*;
use crate::widgets::{self, Icon};
use processcraft_core::ids::DocumentId;

#[derive(Default)]
pub(super) struct ShellState {
    session: String,
    tabs: Vec<DocumentId>,
    automation: bool,
    properties: bool,
    project: bool,
    problems: bool,
    search: String,
}

impl ProcessCraft {
    pub(super) fn workbench_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.m3_poll(&ctx);
        let p = self.theme.palette();
        let snap = self.ctl.session.snapshot();
        let wide = ui.available_width() >= 980.0;
        let toolbar_room = ui.available_width() >= 500.0;
        ui.spacing_mut().item_spacing.y = 3.0;
        let can_undo = snap.undo_count > 0 || self.has_drafts();
        let can_redo = snap.redo_count > 0 && !self.has_drafts();
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
                });
                ui.menu_button("View", |ui| {
                    if wide {
                        ui.checkbox(&mut self.show_left, "Project panel");
                        ui.checkbox(&mut self.show_right, "Properties panel");
                    } else {
                        ui.checkbox(&mut self.shell.project, "Project panel");
                        ui.checkbox(&mut self.shell.properties, "Properties panel");
                    }
                    ui.checkbox(&mut self.shell.problems, "Problems");
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
                    if ui.button("Collaboration and data…").clicked() {
                        self.open_collaboration();
                        ui.close();
                    }
                    if ui.button("Auto layout…").clicked() {
                        self.start_layout(&ctx);
                        ui.close();
                    }
                    if ui.button("Element comments…").clicked() {
                        self.open_comments();
                        ui.close();
                    }
                    if ui.button("Checkpoints and reference…").clicked() {
                        self.open_checkpoints();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("New model").clicked() {
                        self.new_model();
                        ui.close();
                    }
                    self.model_selectors(ui);
                    if ui.button("Validate model").clicked() {
                        self.shell.problems = true;
                        ui.close();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Local automation…").clicked() {
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
        ui.separator();
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, Icon::FolderOpen, "Open…", false).clicked() {
                self.new_or_open("open");
            }
            let save = if toolbar_room {
                widgets::action_button(ui, Icon::Save, "Save")
            } else {
                widgets::icon_button(ui, Icon::Save, "Save", false)
            };
            if save.clicked() {
                self.save(false);
            }
            if toolbar_room {
                ui.separator();
                if ui
                    .add_enabled_ui(can_undo, |ui| {
                        widgets::icon_button(ui, Icon::Undo, "Undo", false)
                    })
                    .inner
                    .clicked()
                {
                    self.history(Request::Undo);
                }
                if ui
                    .add_enabled_ui(can_redo, |ui| {
                        widgets::icon_button(ui, Icon::Redo, "Redo", false)
                    })
                    .inner
                    .clicked()
                {
                    self.history(Request::Redo);
                }
                ui.separator();
                if widgets::icon_button(ui, Icon::PanelLeft, "Project panel", self.show_left)
                    .clicked()
                {
                    if !wide {
                        self.shell.project = !self.shell.project;
                    } else {
                        self.show_left = !self.show_left;
                    }
                }
                if widgets::icon_button(ui, Icon::PanelRight, "Properties", self.show_right)
                    .clicked()
                {
                    if !wide {
                        self.shell.properties = !self.shell.properties;
                    } else {
                        self.show_right = !self.show_right;
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Export", |ui| self.interchange_ui(ui, false));
                if ui.available_width() > 180.0
                    && widgets::secondary_button(ui, "Validate model").clicked()
                {
                    self.shell.problems = !self.shell.problems;
                }
            });
        });
        ui.separator();
        self.document_tabs_ui(ui);
        ui.separator();
        let body = ui.available_rect_before_wrap();
        let footer_height = 34.0;
        let content = Rect::from_min_max(
            body.min,
            egui::pos2(
                body.right(),
                (body.bottom() - footer_height).max(body.top()),
            ),
        );

        let project_width = if self.show_left && wide {
            self.left_width.clamp(180.0, 300.0)
        } else {
            0.0
        };
        let inspector_width = if self.view == 0 && self.show_right && wide {
            self.right_width.clamp(260.0, 380.0)
        } else {
            0.0
        };
        let rail = Rect::from_min_size(content.min, egui::vec2(46.0, content.height()));
        self.shell_panel(ui, "navigation", rail, p.panel, |app, ui| {
            app.navigation_ui(ui)
        });
        let project_rect = Rect::from_min_size(
            egui::pos2(rail.right(), content.top()),
            egui::vec2(project_width, content.height()),
        );
        if project_width > 0.0 {
            self.shell_panel(ui, "project", project_rect, p.panel, |app, ui| {
                app.project_ui(ui)
            });
        }
        let central = Rect::from_min_max(
            egui::pos2(project_rect.right() + 1.0, content.top()),
            egui::pos2(content.right() - inspector_width, content.bottom()),
        );
        self.shell_panel(ui, "content", central, p.base, |app, ui| {
            if app.view == 0 {
                app.document_header(ui);
                app.layout_bar(ui);
                if app.shell.problems {
                    let max_height = (ui.available_height() * 0.28).clamp(60.0, 150.0);
                    egui::Panel::bottom("model_problems")
                        .resizable(true)
                        .default_size(max_height)
                        .size_range(40.0..=200.0)
                        .show(ui, |ui| app.problems_ui(ui));
                }
                app.canvas_ui(ui);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(("data_view", app.view))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        app.m2_panel(ui);
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
            self.shell_panel(ui, "properties", rect, p.panel, |app, ui| {
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
                    (self.right_width - ctx.input(|i| i.pointer.delta().x)).clamp(260.0, 380.0);
            }
        }
        let footer = Rect::from_min_max(egui::pos2(body.left(), content.bottom()), body.max);
        self.shell_panel(ui, "footer", footer, p.panel, |app, ui| {
            ui.horizontal(|ui| {
                widgets::icon(ui, Icon::HardDrive, 16.0, p.muted);
                if let Some(file) = &app.ctl.file {
                    ui.add_sized(
                        [if toolbar_room { 220.0 } else { 120.0 }, 20.0],
                        egui::Label::new(
                            file.target()
                                .file_name()
                                .map_or_else(|| "Project".into(), |n| n.to_string_lossy()),
                        )
                        .truncate(),
                    )
                    .on_hover_text(file.target().display().to_string());
                    if !file.is_writable() {
                        ui.colored_label(p.warning, "Read only · use Save As");
                    }
                } else {
                    ui.label("Local project");
                }
                ui.separator();
                ui.label(if app.ctl.is_saving() {
                    "Saving…"
                } else if snap.dirty || app.has_drafts() {
                    "Unsaved changes"
                } else if app.ctl.file.is_none() {
                    "New project"
                } else {
                    "Saved"
                });
                if app.ctl.migration {
                    ui.colored_label(p.warning, "Migration · Save As required");
                }
                if toolbar_room {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Offline workspace")
                                .small()
                                .color(p.muted),
                        );
                    });
                }
            });
        });
        ui.advance_cursor_after_rect(body);
        self.m3_windows(&ctx);
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
            egui::Window::new("Local automation")
                .open(&mut open)
                .default_width(530.0)
                .max_width(window_width)
                .default_height(window_height.min(560.0))
                .max_height(window_height)
                .vscroll(true)
                .show(&ctx, |ui| self.automation_ui(ui));
            self.shell.automation = open;
        }
    }

    fn shell_panel(
        &mut self,
        parent: &mut egui::Ui,
        id: &'static str,
        rect: Rect,
        fill: Color32,
        add: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        parent.painter().rect_filled(rect, 0.0, fill);
        parent.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, self.theme.palette().border),
            egui::StrokeKind::Inside,
        );
        let mut child =
            parent.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect.shrink2(
                egui::vec2(10.0, if rect.height() < 50.0 { 4.0 } else { 10.0 }),
            )));
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
        if self.m3.has_draft() {
            self.ctl.error = "Post or discard the comment draft before navigating.".into();
            return false;
        }
        if self.dictionary_draft_changed(&self.ctl.session.snapshot().project) {
            self.ctl.error = "Save or discard the dictionary draft before navigating.".into();
            false
        } else {
            self.commit_draft()
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
        ui.spacing_mut().item_spacing.y = 10.0;
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
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Settings, "Local automation", false).clicked() {
                self.shell.automation = true;
            }
        });
    }
    fn project_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let p = self.theme.palette();
        ui.strong(&snap.project.name);
        ui.label(
            egui::RichText::new("LOCAL WORKSPACE")
                .size(10.0)
                .color(p.muted),
        );
        ui.add_space(8.0);
        let search_label = ui.label(egui::RichText::new("Search project").small().color(p.muted));
        ui.add(
            egui::TextEdit::singleline(&mut self.shell.search)
                .hint_text("Search project")
                .desired_width(f32::INFINITY),
        )
        .labelled_by(search_label.id);
        ui.add_space(4.0);
        widgets::section_label(ui, "Models");
        let search = self.shell.search.clone();
        egui::ScrollArea::both()
            .id_salt("project_tree")
            .max_height(if self.view == 0 {
                (ui.available_height() * 0.27).max(90.0)
            } else {
                ui.available_height()
            })
            .show(ui, |ui| self.project_tree_ui(ui, &search));
        if self.view != 0 {
            return;
        }
        ui.separator();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        widgets::section_label(ui, "BPMN elements");
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Pool").clicked() {
                    self.add_pool();
                }
                if ui.button("Lane").clicked() {
                    self.add_lane();
                }
            });
            for (label, kind) in [
                ("Task", NodeKind::Task),
                ("User task", NodeKind::UserTask),
                ("Manual task", NodeKind::ManualTask),
                ("Start event", NodeKind::StartEvent),
                ("End event", NodeKind::EndEvent),
                ("Exclusive gateway", NodeKind::ExclusiveGateway),
                ("Parallel gateway", NodeKind::ParallelGateway),
            ] {
                if palette_item(ui, kind, label).clicked() {
                    self.add(kind);
                }
            }
        });
        ui.menu_button("More BPMN elements…", |ui| {
            ui.add_enabled_ui(editable, |ui| {
                for (label, kind) in [
                    ("Text annotation", NodeKind::TextAnnotation),
                    ("Service task", NodeKind::ServiceTask),
                    ("Send task", NodeKind::SendTask),
                    ("Receive task", NodeKind::ReceiveTask),
                    ("Script task", NodeKind::ScriptTask),
                    ("Business rule task", NodeKind::BusinessRuleTask),
                    ("Inclusive gateway", NodeKind::InclusiveGateway),
                    ("Complex gateway", NodeKind::ComplexGateway),
                    ("Event-based gateway", NodeKind::EventBasedGateway),
                    ("Intermediate throw event", NodeKind::IntermediateThrowEvent),
                    ("Intermediate catch event", NodeKind::IntermediateCatchEvent),
                    (
                        "Boundary timer on selected activity",
                        NodeKind::BoundaryEvent,
                    ),
                    ("Subprocess", NodeKind::SubProcess),
                    ("Ad-hoc subprocess", NodeKind::AdHocSubProcess),
                    ("Transaction", NodeKind::Transaction),
                ] {
                    if ui.button(label).clicked() {
                        self.add(kind);
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        egui::CollapsingHeader::new("Outline").show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("model_outline")
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    if let Some(doc) = snap.project.documents.get(self.doc) {
                        for process in processcraft_core::queries::scopes(doc) {
                            for (id, name) in process
                                .lanes
                                .iter()
                                .map(|l| (&l.id, l.name.as_str()))
                                .chain(process.nodes.iter().map(|n| (&n.id, n.name.as_str())))
                                .chain(process.flows.iter().map(|f| (&f.id, f.name.as_str())))
                            {
                                if ui
                                    .selectable_label(
                                        self.selected.as_ref() == Some(id),
                                        if name.is_empty() {
                                            id.to_string()
                                        } else {
                                            name.to_owned()
                                        },
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
    }
    fn document_header(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let p = self.theme.palette();
        ui.add_space(6.0);
        if let Some(doc) = snap.project.documents.get(self.doc) {
            ui.label(
                egui::RichText::new(format!(
                    "{} / {}",
                    snap.project.name,
                    doc.folder
                        .as_ref()
                        .and_then(|id| snap.project.folders.iter().find(|f| f.id == *id))
                        .map_or("Models", |f| f.name.as_str())
                ))
                .small()
                .color(p.muted),
            );
            ui.horizontal_wrapped(|ui| {
                ui.heading(&doc.name);
                let (label, fg, bg) = match &doc.capability {
                    Capability::Editable => ("BPMN 2.0 subset", p.muted, p.secondary),
                    Capability::RetainedExtensions { .. } => {
                        ("Read-only structure", p.warning, p.warning_soft)
                    }
                    Capability::PreservedSource { .. } => {
                        ("Preserved source", p.warning, p.warning_soft)
                    }
                };
                widgets::badge(ui, label, fg, bg).on_hover_text(match &doc.capability { Capability::Editable=>"Editable BPMN model", Capability::RetainedExtensions{..}=>"Source extensions are retained. Metadata editing remains available; structural editing is disabled.", Capability::PreservedSource{..}=>"Original BPMN source is retained. A semantic preview is unavailable." });
                ui.add_space(4.0);
            });
        }
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(p.secondary)
                .corner_radius(5)
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let _ = ui.selectable_label(true, "Model");
                        if ui.selectable_label(false, "Table").clicked()
                            && self.prepare_navigation()
                        {
                            self.view = 3;
                        }
                    });
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Diagram", |ui| self.model_selectors(ui));
            });
        });
        ui.separator();
    }
    fn model_selectors(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        if let Some(doc) = snap.project.documents.get(self.doc) {
            ui.strong("Processes and subprocesses");
            for process in processcraft_core::queries::scopes(doc) {
                if ui
                    .selectable_label(
                        self.process_id
                            .as_ref()
                            .or_else(|| doc.processes.first().map(|p| &p.id))
                            == Some(process.id),
                        process.name,
                    )
                    .clicked()
                    && self.prepare_navigation()
                {
                    self.process_id = Some(process.id.clone());
                    self.select(None);
                }
            }
            ui.separator();
            ui.strong("Diagrams");
            for diagram in &doc.diagrams {
                if ui
                    .selectable_label(
                        self.diagram_id
                            .as_ref()
                            .or_else(|| doc.diagrams.first().map(|d| &d.id))
                            == Some(&diagram.id),
                        diagram.id.to_string(),
                    )
                    .clicked()
                    && self.prepare_navigation()
                {
                    self.diagram_id = Some(diagram.id.clone());
                    self.select(None);
                }
            }
        }
    }
    fn problems_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(format!("Problems · {}", self.problems.len()));
            if widgets::icon_button(ui, Icon::X, "Close problems", false).clicked() {
                self.shell.problems = false;
            }
        });
        if self.problems.is_empty() {
            ui.label("No model problems found.");
        }
        egui::ScrollArea::vertical()
            .id_salt("problem_rows")
            .show(ui, |ui| {
                for problem in self.problems.clone() {
                    if ui.button(&problem.message).clicked() && self.prepare_navigation() {
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
            });
    }
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
                                .map(|d| d.id.clone());
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
                        Ok(Some(true)) => self.ctl.error = "Attachment export confirmed.".into(),
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
            if self.draft_dirty
                || self.dictionary_draft_changed(&self.ctl.session.snapshot().project)
            {
                ui.label("Attachment requests wait until the current draft is applied.");
            }
        }
    }
}

fn palette_item(ui: &mut egui::Ui, kind: NodeKind, label: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 29.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4, visuals.bg_fill);
    }
    let center = egui::pos2(rect.left() + 13.0, rect.center().y);
    let stroke = egui::Stroke::new(1.3, visuals.fg_stroke.color);
    match kind {
        NodeKind::StartEvent | NodeKind::EndEvent => {
            ui.painter().circle_stroke(
                center,
                7.0,
                if matches!(kind, NodeKind::EndEvent) {
                    egui::Stroke::new(2.2, stroke.color)
                } else {
                    stroke
                },
            );
        }
        NodeKind::ExclusiveGateway | NodeKind::ParallelGateway => {
            ui.painter().add(egui::Shape::closed_line(
                vec![
                    center + egui::vec2(0.0, -8.0),
                    center + egui::vec2(8.0, 0.0),
                    center + egui::vec2(0.0, 8.0),
                    center + egui::vec2(-8.0, 0.0),
                ],
                stroke,
            ));
            if matches!(kind, NodeKind::ParallelGateway) {
                ui.painter().line_segment(
                    [center - egui::vec2(4.0, 0.0), center + egui::vec2(4.0, 0.0)],
                    stroke,
                );
                ui.painter().line_segment(
                    [center - egui::vec2(0.0, 4.0), center + egui::vec2(0.0, 4.0)],
                    stroke,
                );
            }
        }
        _ => {
            ui.painter().rect_stroke(
                Rect::from_center_size(center, egui::vec2(18.0, 13.0)),
                3,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
    ui.painter().text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        visuals.fg_stroke.color,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

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
