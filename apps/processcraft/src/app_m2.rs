use super::*;
use processcraft_core::{commands::DictionaryDeleteResolution, queries};

use egui_extras::{Column, TableBuilder};

struct TableItem {
    key: String,
    cells: Vec<String>,
    tooltip: String,
    selected: bool,
}

fn data_table(
    ui: &mut egui::Ui,
    id: &'static str,
    headings: &[&str],
    widths: &[f32],
    rows: &[TableItem],
    height: f32,
) -> Option<(usize, bool)> {
    let mut clicked = None;
    let text_color = ui.visuals().text_color();
    let muted_color = ui.visuals().weak_text_color();
    let header_fill = ui.visuals().faint_bg_color;
    let divider = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::ScrollArea::horizontal()
        .id_salt((id, "horizontal"))
        .show(ui, |ui| {
            ui.set_min_width(widths.iter().sum::<f32>());
            let header_rect =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 34.0));
            ui.painter().rect_filled(header_rect, 0.0, header_fill);
            let mut table = TableBuilder::new(ui)
                .id_salt(id)
                .resizable(true)
                .sense(egui::Sense::click())
                .max_scroll_height(height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for (index, width) in widths.iter().enumerate() {
                let column = if index + 1 == widths.len() {
                    Column::remainder().at_least(*width)
                } else {
                    Column::initial(*width).at_least(65.0)
                };
                table = table.column(column.clip(true));
            }
            table
                .header(34.0, |mut header| {
                    for heading in headings {
                        header.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter()
                                .line_segment([rect.left_bottom(), rect.right_bottom()], divider);
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(*heading)
                                    .size(12.0)
                                    .strong()
                                    .color(muted_color),
                            );
                        });
                    }
                })
                .body(|body| {
                    body.rows(44.0, rows.len(), |mut row| {
                        let index = row.index();
                        let item = &rows[index];
                        row.set_selected(item.selected);
                        let mut keyboard_clicked = false;
                        let mut cell_double_clicked = false;
                        for (column, text) in item.cells.iter().enumerate() {
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                ui.painter().line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    divider,
                                );
                                if column == 0 {
                                    let response = ui.interact(
                                        ui.max_rect(),
                                        egui::Id::unique((id, &item.key)),
                                        egui::Sense::click(),
                                    );
                                    if response.has_focus() {
                                        ui.painter().rect_stroke(
                                            response.rect.shrink(1.0),
                                            3.0,
                                            ui.visuals().selection.stroke,
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    response.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::Role::Button,
                                            ui.is_enabled(),
                                            item.selected,
                                            item.cells.join(", "),
                                        )
                                    });
                                    cell_double_clicked = response.double_clicked();
                                    keyboard_clicked =
                                        response.on_hover_text(&item.tooltip).clicked();
                                }
                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(text).color(text_color))
                                        .truncate(),
                                )
                                .on_hover_text(text);
                            });
                        }
                        let response = row.response().on_hover_text(&item.tooltip);
                        if keyboard_clicked || response.clicked() {
                            clicked =
                                Some((index, cell_double_clicked || response.double_clicked()));
                        }
                    });
                });
        });
    clicked
}

impl ProcessCraft {
    pub(super) fn m2_panel(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let project = &snap.project;
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
        match self.view {
            1 => {
                ui.heading("Library");
                let folder_label = self
                    .folder_selected
                    .as_ref()
                    .and_then(|id| project.folders.iter().find(|f| &f.id == id))
                    .map(|f| f.name.as_str());
                ui.label(
                    egui::RichText::new(format!(
                        "Project / {}",
                        folder_label.unwrap_or("All models")
                    ))
                    .weak(),
                );
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    let search = ui.label("Search");
                    ui.add_sized(
                        [ui.available_width().clamp(100.0, 240.0), 28.0],
                        egui::TextEdit::singleline(&mut self.library_filter)
                            .hint_text("Search models"),
                    )
                    .labelled_by(search.id);
                    if self.folder_selected.is_some() && ui.button("All models").clicked() {
                        self.folder_selected = None;
                    }
                    let count = queries::library_search(project, &self.library_filter)
                        .into_iter()
                        .filter(|id| {
                            project.documents.iter().any(|d| {
                                d.id == *id
                                    && self
                                        .folder_selected
                                        .as_ref()
                                        .is_none_or(|folder| d.folder.as_ref() == Some(folder))
                            })
                        })
                        .count();
                    ui.label(format!(
                        "{count} {}",
                        if count == 1 { "model" } else { "models" }
                    ));
                    ui.menu_button("Folders…", |ui| {
                        let label = ui.label("Folder name");
                        ui.text_edit_singleline(&mut self.folder_name)
                            .labelled_by(label.id);
                        if ui.button("Create folder").clicked() {
                            self.ctl.mutate(vec![Mutation::CreateFolder {
                                folder: Folder {
                                    id: controller::id("Folder"),
                                    name: self.folder_name.clone(),
                                    parent: self.folder_selected.clone(),
                                },
                            }]);
                        }
                        if let Some(id) = &self.folder_selected {
                            if ui.button("Rename selected folder").clicked() {
                                self.ctl.mutate(vec![Mutation::RenameFolder {
                                    folder_id: id.clone(),
                                    name: self.folder_name.clone(),
                                }]);
                            }
                            if ui.button("Delete empty folder").clicked()
                                && self.ctl.mutate(vec![Mutation::DeleteFolder {
                                    folder_id: id.clone(),
                                }])
                            {
                                self.folder_selected = None;
                            }
                        }
                        ui.separator();
                        for folder in &project.folders {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(
                                        self.folder_selected.as_ref() == Some(&folder.id),
                                        &folder.name,
                                    )
                                    .clicked()
                                {
                                    self.folder_selected = Some(folder.id.clone());
                                    self.folder_name = folder.name.clone();
                                }
                                ui.menu_button("Move…", |ui| {
                                    if ui.button("Root").clicked() {
                                        self.ctl.mutate(vec![Mutation::MoveFolder {
                                            folder_id: folder.id.clone(),
                                            parent: None,
                                        }]);
                                        ui.close();
                                    }
                                    for parent in
                                        project.folders.iter().filter(|p| p.id != folder.id)
                                    {
                                        if ui.button(&parent.name).clicked() {
                                            self.ctl.mutate(vec![Mutation::MoveFolder {
                                                folder_id: folder.id.clone(),
                                                parent: Some(parent.id.clone()),
                                            }]);
                                            ui.close();
                                        }
                                    }
                                });
                            });
                        }
                    });
                });
                ui.add_space(8.0);
                let wide = ui.available_width() >= 850.0;
                ui.horizontal_top(|ui| {
                    let list_width = if wide {
                        (ui.available_width() - 312.0).max(300.0)
                    } else {
                        ui.available_width()
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(list_width, if wide { ui.available_height() } else { 220.0 }),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let docs: Vec<_> =
                                queries::library_search(project, &self.library_filter)
                                    .into_iter()
                                    .filter_map(|id| {
                                        project.documents.iter().enumerate().find(|(_, d)| {
                                            d.id == id
                                                && self.folder_selected.as_ref().is_none_or(
                                                    |folder| d.folder.as_ref() == Some(folder),
                                                )
                                        })
                                    })
                                    .collect();
                            let items: Vec<_> = docs
                                .iter()
                                .map(|(index, doc)| TableItem {
                                    key: doc.id.to_string(),
                                    cells: vec![
                                        doc.name.clone(),
                                        doc.folder
                                            .as_ref()
                                            .and_then(|id| {
                                                project.folders.iter().find(|f| &f.id == id)
                                            })
                                            .map_or("Root", |f| f.name.as_str())
                                            .into(),
                                        "BPMN".into(),
                                        format!("{:?}", doc.capability),
                                    ],
                                    tooltip: doc
                                        .provenance
                                        .as_ref()
                                        .map_or("Local model", |p| p.source_name.as_str())
                                        .into(),
                                    selected: self.doc == *index,
                                })
                                .collect();
                            let height = (ui.available_height() - 48.0).max(120.0);
                            if let Some((row, double)) = data_table(
                                ui,
                                "library_rows",
                                &["NAME", "FOLDER", "NOTATION", "CAPABILITY"],
                                &[220.0, 125.0, 95.0, 120.0],
                                &items,
                                height,
                            ) {
                                let index = docs[row].0;
                                if self.prepare_navigation() {
                                    self.doc = index;
                                    self.process_id = None;
                                    self.diagram_id = None;
                                    self.select(None);
                                    if double {
                                        self.view = 0;
                                    }
                                }
                            }
                            if items.is_empty() {
                                ui.label("No models match this search.");
                            }
                        },
                    );
                    if wide {
                        ui.separator();
                        ui.allocate_ui_with_layout(
                            egui::vec2(290.0, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.library_details(ui, project, true),
                        );
                    }
                });
                if !wide {
                    self.library_details(ui, project, false);
                }
            }
            2 => {
                ui.horizontal(|ui| {
                    ui.heading("Dictionary");
                    if crate::widgets::primary_button(ui, "New entry").clicked()
                        && !self.dictionary_draft_changed(project)
                    {
                        self.dictionary_selected = None;
                        self.dictionary_name.clear();
                        self.dictionary_description.clear();
                    }
                });
                ui.label(egui::RichText::new("Shared terms used across your models").weak());
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    for (kind, label) in [
                        (None, "All"),
                        (Some(DictionaryKind::Role), "Roles"),
                        (Some(DictionaryKind::System), "IT systems"),
                        (Some(DictionaryKind::Document), "Documents"),
                    ] {
                        let count = project
                            .dictionary_entries
                            .iter()
                            .filter(|e| kind.is_none_or(|k| e.kind == k))
                            .count();
                        ui.selectable_value(
                            &mut self.dictionary_kind_filter,
                            kind,
                            format!("{label}  {count}"),
                        );
                    }
                });
                let search = ui.label("Search entries");
                ui.add_sized(
                    [ui.available_width().min(400.0), 28.0],
                    egui::TextEdit::singleline(&mut self.dictionary_filter)
                        .hint_text("Search dictionary"),
                )
                .labelled_by(search.id);
                ui.add_space(8.0);
                let wide = ui.available_width() >= 850.0;
                ui.horizontal_top(|ui| {
                    let list_width = if wide {
                        (ui.available_width() - 312.0).max(300.0)
                    } else {
                        ui.available_width()
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(list_width, if wide { ui.available_height() } else { 220.0 }),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let entries: Vec<_> = project
                                .dictionary_entries
                                .iter()
                                .filter(|entry| {
                                    self.dictionary_kind_filter
                                        .is_none_or(|kind| kind == entry.kind)
                                        && entry
                                            .name
                                            .to_lowercase()
                                            .contains(&self.dictionary_filter.to_lowercase())
                                })
                                .collect();
                            let items: Vec<_> = entries
                                .iter()
                                .map(|entry| {
                                    let count = queries::dictionary_usages(project, entry.id).len();
                                    TableItem {
                                        key: entry.id.to_string(),
                                        cells: vec![
                                            entry.name.clone(),
                                            format!(
                                                "{:?}  ·  {count} {}",
                                                entry.kind,
                                                if count == 1 { "usage" } else { "usages" }
                                            ),
                                        ],
                                        tooltip: entry.description.clone(),
                                        selected: self.dictionary_selected == Some(entry.id),
                                    }
                                })
                                .collect();
                            let height = (ui.available_height() - 48.0).max(120.0);
                            if let Some((index, _)) = data_table(
                                ui,
                                "dictionary_rows",
                                &["NAME", "KIND / USAGES"],
                                &[240.0, 190.0],
                                &items,
                                height,
                            ) {
                                self.choose_dictionary_entry(project, entries[index]);
                            }
                            if items.is_empty() {
                                ui.label("No entries match this search.");
                            }
                        },
                    );
                    if wide {
                        ui.separator();
                        ui.allocate_ui_with_layout(
                            egui::vec2(290.0, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.dictionary_editor(ui, project, true),
                        );
                    }
                });
                if !wide {
                    self.dictionary_editor(ui, project, false);
                }
            }
            3 => self.activity_table_ui(ui, project),
            _ => {}
        }
    }

    fn library_details(&mut self, ui: &mut egui::Ui, project: &ProjectData, scroll: bool) {
        let mut contents = |ui: &mut egui::Ui| {
            ui.heading("Model details");
            if let Some(doc) = project.documents.get(self.doc) {
                ui.add_space(12.0);
                ui.strong(&doc.name);
                ui.label(format!("Capability: {:?}", doc.capability));
                let processes = doc.processes.len();
                let diagrams = doc.diagrams.len();
                ui.label(format!(
                    "{processes} {} · {diagrams} {}",
                    if processes == 1 {
                        "process"
                    } else {
                        "processes"
                    },
                    if diagrams == 1 { "diagram" } else { "diagrams" }
                ));
                let activities = doc
                    .processes
                    .iter()
                    .flat_map(|p| &p.nodes)
                    .filter(|n| {
                        matches!(
                            n.kind,
                            NodeKind::Task | NodeKind::UserTask | NodeKind::ManualTask
                        )
                    })
                    .count();
                ui.label(format!(
                    "{activities} {}",
                    if activities == 1 {
                        "activity"
                    } else {
                        "activities"
                    }
                ));
                ui.label(format!(
                    "Source: {}",
                    doc.provenance
                        .as_ref()
                        .map_or("Local model", |p| p.source_name.as_str())
                ));
                if crate::widgets::primary_button(ui, "Open in modeler").clicked()
                    && self.prepare_navigation()
                {
                    self.view = 0;
                }
                ui.menu_button("Move model to…", |ui| {
                    if ui.button("Root").clicked() {
                        self.ctl.mutate(vec![Mutation::DocumentProperties {
                            document_id: doc.id,
                            name: doc.name.clone(),
                            folder: None,
                        }]);
                        ui.close();
                    }
                    for folder in &project.folders {
                        if ui.button(&folder.name).clicked() {
                            self.ctl.mutate(vec![Mutation::DocumentProperties {
                                document_id: doc.id,
                                name: doc.name.clone(),
                                folder: Some(folder.id.clone()),
                            }]);
                            ui.close();
                        }
                    }
                });
            } else {
                ui.label("Select a model to see its details.");
            }
        };
        if scroll {
            egui::ScrollArea::vertical()
                .id_salt("library_details")
                .show(ui, &mut contents);
        } else {
            contents(ui);
        }
    }

    fn dictionary_editor(&mut self, ui: &mut egui::Ui, project: &ProjectData, scroll: bool) {
        let snap = self.ctl.session.snapshot();
        let mut contents = |ui: &mut egui::Ui| {
            ui.heading(if self.dictionary_selected.is_some() {
                "Entry details"
            } else {
                "New entry"
            });
            if self.dictionary_draft_changed(project) {
                ui.label("Save or discard changes before selecting another entry.");
                if ui.button("Discard changes").clicked() {
                    if let Some(entry) = self
                        .dictionary_selected
                        .and_then(|id| project.dictionary_entries.iter().find(|e| e.id == id))
                    {
                        self.dictionary_name = entry.name.clone();
                        self.dictionary_description = entry.description.clone();
                        self.dictionary_kind = entry.kind;
                    } else {
                        self.dictionary_name.clear();
                        self.dictionary_description.clear();
                    }
                }
            }
            let label = ui.label("Name");
            ui.add_sized(
                [ui.available_width(), 28.0],
                egui::TextEdit::singleline(&mut self.dictionary_name),
            )
            .labelled_by(label.id);
            let label = ui.label("Description");
            ui.add_sized(
                [ui.available_width(), 72.0],
                egui::TextEdit::multiline(&mut self.dictionary_description),
            )
            .labelled_by(label.id);
            ui.label("Kind");
            ui.horizontal_wrapped(|ui| {
                for kind in [
                    DictionaryKind::Role,
                    DictionaryKind::System,
                    DictionaryKind::Document,
                ] {
                    ui.add_enabled_ui(self.dictionary_selected.is_none(), |ui| {
                        ui.selectable_value(&mut self.dictionary_kind, kind, format!("{kind:?}"));
                    });
                }
            });
            if crate::widgets::primary_button(ui, "Save entry").clicked() {
                self.commit_dictionary_draft();
            }
            if let Some(id) = self.dictionary_selected {
                if !self.dictionary_draft_changed(&snap.project)
                    && let Some(entry) = snap.project.dictionary_entries.iter().find(|e| e.id == id)
                {
                    ui.menu_button("Delete entry…", |ui| {
                        if ui.button("Delete only if unused").clicked()
                            && self.ctl.mutate(vec![Mutation::DeleteDictionaryEntry {
                                entry_id: id,
                                resolution: DictionaryDeleteResolution::Reject,
                            }])
                        {
                            self.dictionary_selected = None;
                            self.dictionary_name.clear();
                            self.dictionary_description.clear();
                            ui.close();
                        }
                        if ui.button("Unlink and retain text").clicked()
                            && self.ctl.mutate(vec![Mutation::DeleteDictionaryEntry {
                                entry_id: id,
                                resolution: DictionaryDeleteResolution::Unlink,
                            }])
                        {
                            self.dictionary_selected = None;
                            self.dictionary_name.clear();
                            self.dictionary_description.clear();
                            ui.close();
                        }
                        for other in snap
                            .project
                            .dictionary_entries
                            .iter()
                            .filter(|e| e.kind == entry.kind && e.id != id)
                        {
                            if ui.button(format!("Replace with {}", other.name)).clicked()
                                && self.ctl.mutate(vec![Mutation::DeleteDictionaryEntry {
                                    entry_id: id,
                                    resolution: DictionaryDeleteResolution::Replace(other.id),
                                }])
                            {
                                self.dictionary_selected = None;
                                self.dictionary_name.clear();
                                self.dictionary_description.clear();
                                ui.close();
                            }
                        }
                    });
                }
                ui.add_space(12.0);
                ui.strong("Linked usages");
                let usages = queries::dictionary_usages(project, id);
                if usages.is_empty() {
                    ui.label("No linked usages");
                }
                for key in usages {
                    let doc = project.documents.iter().find(|d| d.id == key.document_id);
                    let element = doc
                        .and_then(|d| queries::element_name(d, &key.xml_id))
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Unnamed element");
                    let title = format!(
                        "{} / {}",
                        doc.map_or("Unknown model", |d| d.name.as_str()),
                        element
                    );
                    if ui
                        .button(title)
                        .on_hover_text(format!("{} / {}", key.document_id, key.xml_id))
                        .clicked()
                        && self.prepare_navigation()
                        && let Some(index) = project
                            .documents
                            .iter()
                            .position(|d| d.id == key.document_id)
                    {
                        self.doc = index;
                        self.process_id = None;
                        self.diagram_id = None;
                        self.select(Some(key.xml_id));
                        self.view = 0;
                    }
                }
            }
        };
        if scroll {
            egui::ScrollArea::vertical()
                .id_salt("dictionary_editor")
                .show(ui, &mut contents);
        } else {
            contents(ui);
        }
    }

    fn activity_table_ui(&mut self, ui: &mut egui::Ui, project: &ProjectData) {
        ui.heading("Activity table");
        let Some(d) = project.documents.get(self.doc) else {
            return;
        };
        ui.label(egui::RichText::new(format!("{} / Activities", d.name)).weak());
        let Some(p) = queries::scopes(d)
            .into_iter()
            .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        else {
            return;
        };
        let Some(rows) = queries::activity_table(project, d.id, p.id) else {
            return;
        };
        ui.add_space(12.0);
        let items: Vec<_> = rows
            .iter()
            .map(|row| {
                let lane = row
                    .lane
                    .as_ref()
                    .and_then(|id| p.lanes.iter().find(|l| &l.id == id))
                    .map_or("—", |l| l.name.as_str());
                let systems = if row.systems.is_empty() {
                    "—".into()
                } else {
                    row.systems.join(", ")
                };
                let documents = if row.documents.is_empty() {
                    "—".into()
                } else {
                    row.documents.join(", ")
                };
                let summary = row
                    .outgoing
                    .iter()
                    .map(|flow| {
                        let target = p.nodes.iter().find(|n| n.id == flow.target).map_or(
                            "Unnamed element",
                            |n| {
                                if n.name.is_empty() {
                                    "Unnamed element"
                                } else {
                                    &n.name
                                }
                            },
                        );
                        match &flow.condition {
                            Some(c) if !c.is_empty() => format!("{target} ({})", c.text()),
                            _ => target.to_owned(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                let outgoing = if summary.is_empty() {
                    "—".to_owned()
                } else {
                    summary
                };
                TableItem {
                    key: format!("{} / {}", row.key.document_id, row.key.xml_id),
                    cells: vec![
                        row.name.clone(),
                        row.role.clone().unwrap_or_else(|| "—".into()),
                        lane.into(),
                        systems,
                        documents,
                        outgoing.clone(),
                    ],
                    tooltip: format!("{} / {}\n{}", row.key.document_id, row.key.xml_id, outgoing),
                    selected: self.selected.as_ref() == Some(&row.key.xml_id),
                }
            })
            .collect();
        let table_height = (ui.available_height() * 0.5).max(150.0);
        if let Some((index, _)) = data_table(
            ui,
            "activity_table",
            &["NAME", "ROLE", "LANE", "SYSTEMS", "DOCUMENTS", "OUTGOING"],
            &[190.0, 110.0, 110.0, 125.0, 125.0, 180.0],
            &items,
            table_height,
        ) {
            self.select(Some(rows[index].key.xml_id.clone()));
        }
        if items.is_empty() {
            ui.label("No activities in this process.");
        }
        if self.selected.is_some() && self.is_node {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let label = ui.label("Activity name");
                if ui
                    .text_edit_singleline(&mut self.draft)
                    .labelled_by(label.id)
                    .changed()
                {
                    self.draft_dirty = true;
                }
                if ui.button("Apply activity name").clicked() {
                    self.commit_draft();
                }
                if ui.button("Edit selected activity").clicked() {
                    self.view = 0;
                }
            });
            self.dictionary_links_ui(ui);
        }
        ui.label(
            egui::RichText::new("Outgoing flows and conditions are read-only in this table.")
                .weak(),
        );
        ui.add_space(12.0);
        ui.strong("Model preview");
        let preview_height = ui.available_height().clamp(180.0, 260.0);
        self.diagram_preview(ui, preview_height);
    }
    pub(super) fn dictionary_links_ui(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let (Some(d), Some(element)) =
            (snap.project.documents.get(self.doc), self.selected.clone())
        else {
            return;
        };
        let Some(p) = queries::scopes(d)
            .into_iter()
            .find(|p| p.nodes.iter().any(|n| n.id == element))
        else {
            return;
        };
        let key = processcraft_core::ids::ElementKey {
            document_id: d.id,
            xml_id: element.clone(),
        };
        for kind in [DictionaryKind::System, DictionaryKind::Document] {
            ui.collapsing(format!("{kind:?} links"), |ui| {
                let targets: Vec<_> = snap
                    .project
                    .dictionary_links
                    .iter()
                    .filter(|l| l.source == key && l.kind == kind)
                    .map(|l| l.target.clone())
                    .collect();
                for entry in snap
                    .project
                    .dictionary_entries
                    .iter()
                    .filter(|e| e.kind == kind)
                {
                    let target = DictionaryTarget::Entry(entry.id);
                    let mut checked = targets.contains(&target);
                    if ui.checkbox(&mut checked, &entry.name).changed() && self.commit_draft() {
                        let mut updated = targets.clone();
                        if checked {
                            updated.push(target);
                        } else {
                            updated.retain(|t| t != &target);
                        }
                        self.ctl.mutate(vec![Mutation::SetDictionaryLinks {
                            source: key.clone(),
                            kind,
                            targets: updated,
                        }]);
                    }
                }
                for literal in targets.iter().filter_map(|t| {
                    if let DictionaryTarget::Literal(s) = t {
                        Some(s)
                    } else {
                        None
                    }
                }) {
                    ui.label(format!("Retained text: {literal}"));
                }
            });
        }
        ui.menu_button("Assign role…", |ui| {
            for role in snap.project.dictionary_entries.iter().filter(|e| e.kind == DictionaryKind::Role) {
                ui.menu_button(&role.name, |ui| {
                    match queries::role_lane_choices(&snap.project, d.id, p.id, role.id) {
                        Some(queries::RoleLaneChoices::CreateLane) => {
                            if ui.button("Create lane and assign").clicked() && self.commit_draft() {
                                let lane = controller::id("Lane");
                                let lane_bounds = Bounds { x: self.camera.0 + 30.0, y: self.camera.1 + 80.0 + p.lanes.len() as f64 * 160.0, width: 600.0, height: 150.0 };
                                let Some((bounds, moves)) = self.role_assignment_bounds(d, &element, &lane_bounds) else { return };
                                let mut mutations = vec![Mutation::CreateLane { document_id: d.id, process_id: p.id.clone(), lane: Lane {
    child_lane_set_id: None, id: lane.clone(), name: role.name.clone(), parent: None, members: vec![] } }, Mutation::SetDictionaryLinks { source: processcraft_core::ids::ElementKey { document_id: d.id, xml_id: lane.clone() }, kind: DictionaryKind::Role, targets: vec![DictionaryTarget::Entry(role.id)] }];
                                if let Some(diagram) = d.diagrams.iter().find(|diagram| self.diagram_id.as_ref().is_none_or(|id| &diagram.id == id)) {
                                    mutations.push(Mutation::CreateShape { document_id: d.id, diagram_id: diagram.id.clone(), shape: Shape {
    is_expanded: None, id: controller::id("LaneShape"), element: lane.clone(), bounds: lane_bounds, label: None, style: None } });
                                }
                                mutations.extend(moves);
                                mutations.push(Mutation::AssignLane { document_id: d.id, process_id: p.id.clone(), element_id: element.clone(), lane_id: Some(lane), bounds });
                                self.ctl.mutate(mutations); ui.close();
                            }
                        }
                        Some(choice) => {
                            let lanes = match choice { queries::RoleLaneChoices::Assign(id) => vec![id], queries::RoleLaneChoices::Choose(ids) => ids, _ => vec![] };
                            for lane in lanes {
                                let name = p.lanes.iter().find(|l| l.id == lane).map_or_else(|| lane.to_string(), |l| l.name.clone());
                                if ui.button(name).clicked() && self.commit_draft() {
                                    let Some(lane_bounds) = d.diagrams.first().and_then(|diagram| diagram.shapes.iter().find(|shape| shape.element == lane)).map(|shape| &shape.bounds) else { self.ctl.error = "The lane must have a diagram shape before assigning a role.".into(); return };
                                    let Some((bounds, mut moves)) = self.role_assignment_bounds(d, &element, lane_bounds) else { return };
                                    moves.push(Mutation::AssignLane { document_id: d.id, process_id: p.id.clone(), element_id: element.clone(), lane_id: Some(lane), bounds });
                                    self.ctl.mutate(moves); ui.close();
                                }
                            }
                        }
                        None => {}
                    }
                });
            }
        });
    }

    pub(super) fn splice_selected(&mut self) {
        if !self.commit_draft() {
            return;
        }
        let snap = self.ctl.session.snapshot();
        let (Some(d), Some(flow_id)) =
            (snap.project.documents.get(self.doc), self.selected.clone())
        else {
            return;
        };
        let Some(p) = queries::scopes(d)
            .into_iter()
            .find(|p| p.flows.iter().any(|f| f.id == flow_id))
        else {
            return;
        };
        let Some(flow) = p
            .flows
            .iter()
            .find(|f| f.id == flow_id && f.kind == FlowKind::Sequence)
        else {
            self.ctl.error = "Select a sequence flow to insert an activity.".into();
            return;
        };
        let Some(diagram) = d
            .diagrams
            .iter()
            .find(|d| self.diagram_id.as_ref().is_none_or(|id| &d.id == id))
        else {
            return;
        };
        let Some(edge) = diagram.edges.iter().find(|e| e.element == flow_id) else {
            self.ctl.error = "The selected flow has no diagram edge.".into();
            return;
        };
        if edge.waypoints.len() != 2 {
            self.ctl.error = "Insertion requires a straight flow with two waypoints.".into();
            return;
        }
        let (a, b) = (&edge.waypoints[0], &edge.waypoints[1]);
        if (a.y - b.y).abs() > 0.01 || (b.x - a.x).abs() < 140.0 {
            self.ctl.error =
                "Insertion requires a horizontal flow with at least 140 units of space.".into();
            return;
        }
        let x = (a.x + b.x) / 2.0;
        let y = (a.y + b.y) / 2.0;
        let left_to_right = b.x > a.x;
        let enter = Point {
            x: x + if left_to_right { -50.0 } else { 50.0 },
            y,
        };
        let exit = Point {
            x: x + if left_to_right { 50.0 } else { -50.0 },
            y,
        };
        let node_id = controller::id("Node");
        let new_flow_id = controller::id("Flow");
        let mutation = Mutation::SpliceActivity {
            document_id: d.id,
            process_id: p.id.clone(),
            flow_id,
            node: Node {
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: node_id.clone(),
                name: "Task".into(),
                documentation: String::new(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            },
            new_flow: Flow {
                association_direction: None,
                id: new_flow_id.clone(),
                kind: FlowKind::Sequence,
                source: node_id.clone(),
                target: flow.target.clone(),
                name: String::new(),
                documentation: String::new(),
                condition: None,
            },
            diagram_id: diagram.id.clone(),
            shape: Shape {
                is_expanded: None,
                id: controller::id("Shape"),
                element: node_id.clone(),
                bounds: Bounds {
                    x: x - 50.0,
                    y: y - 35.0,
                    width: 100.0,
                    height: 70.0,
                },
                label: None,
                style: None,
            },
            old_waypoints: vec![a.clone(), enter],
            new_edge: Edge {
                id: controller::id("Edge"),
                element: new_flow_id,
                waypoints: vec![exit, b.clone()],
                label: None,
                style: None,
            },
        };
        if self.ctl.mutate(vec![mutation]) {
            self.select(Some(node_id));
        }
    }

    fn role_assignment_bounds(
        &mut self,
        d: &BpmnDocument,
        element: &XmlId,
        lane: &Bounds,
    ) -> Option<(Vec<processcraft_core::commands::ShapeBounds>, Vec<Mutation>)> {
        if d.diagrams.len() != 1 {
            self.ctl.error =
                "Role assignment requires one diagram so its lane placement stays consistent."
                    .into();
            return None;
        }
        let diagram = &d.diagrams[0];
        let shapes: Vec<_> = diagram
            .shapes
            .iter()
            .filter(|s| &s.element == element)
            .collect();
        if shapes.len() != 1
            || shapes[0].bounds.width + 40.0 > lane.width
            || shapes[0].bounds.height + 40.0 > lane.height
        {
            self.ctl.error =
                "Role assignment requires one activity shape that fits inside the lane.".into();
            return None;
        }
        let shape = shapes[0];
        let mut bounds = shape.bounds.clone();
        bounds.x = lane.x + 30.0;
        bounds.y = lane.y + 20.0;
        let bounds = vec![processcraft_core::commands::ShapeBounds {
            diagram_id: diagram.id.clone(),
            shape_id: shape.id.clone(),
            bounds,
        }];
        match processcraft_core::geometry::prepare_move(d, &diagram.id, bounds.clone()) {
            Ok(prepared) => {
                if !prepared.warnings.is_empty() {
                    self.ctl.error = prepared.warnings.join("; ");
                }
                match prepared.request {
                    Request::Batch { mutations } => Some((bounds, mutations)),
                    _ => {
                        self.ctl.error = "Could not prepare the activity move.".into();
                        None
                    }
                }
            }
            Err(error) => {
                self.ctl.error = format!("Could not prepare the activity move: {error}");
                None
            }
        }
    }

    fn choose_dictionary_entry(&mut self, project: &ProjectData, entry: &DictionaryEntry) {
        if self.dictionary_selected == Some(entry.id) || self.dictionary_draft_changed(project) {
            return;
        }
        self.dictionary_selected = Some(entry.id);
        self.dictionary_kind = entry.kind;
        self.dictionary_name = entry.name.clone();
        self.dictionary_description = entry.description.clone();
    }
    pub(super) fn dictionary_draft_changed(&self, project: &ProjectData) -> bool {
        match self.dictionary_selected.and_then(|id| {
            project
                .dictionary_entries
                .iter()
                .find(|entry| entry.id == id)
        }) {
            Some(entry) => {
                entry.name != self.dictionary_name
                    || entry.description != self.dictionary_description
                    || entry.kind != self.dictionary_kind
            }
            None => !self.dictionary_name.is_empty() || !self.dictionary_description.is_empty(),
        }
    }

    pub(super) fn refresh_dictionary_editor(&mut self) {
        let Some(id) = self.dictionary_selected else {
            return;
        };
        let snapshot = self.ctl.session.snapshot();
        if let Some(entry) = snapshot
            .project
            .dictionary_entries
            .iter()
            .find(|entry| entry.id == id)
        {
            self.dictionary_kind = entry.kind;
            self.dictionary_name = entry.name.clone();
            self.dictionary_description = entry.description.clone();
        } else {
            self.dictionary_selected = None;
            self.dictionary_name.clear();
            self.dictionary_description.clear();
        }
    }
    pub(super) fn commit_dictionary_draft(&mut self) -> bool {
        let snap = self.ctl.session.snapshot();
        if !self.dictionary_draft_changed(&snap.project) {
            return true;
        }
        let entry = DictionaryEntry {
            id: self.dictionary_selected.unwrap_or_default(),
            kind: self.dictionary_kind,
            name: self.dictionary_name.clone(),
            description: self.dictionary_description.clone(),
        };
        let mutation = if self.dictionary_selected.is_some() {
            Mutation::UpdateDictionaryEntry {
                entry: entry.clone(),
            }
        } else {
            Mutation::CreateDictionaryEntry {
                entry: entry.clone(),
            }
        };
        if self.ctl.mutate(vec![mutation]) {
            self.dictionary_selected = Some(entry.id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtered_table_click_selects_matching_stable_key() {
        let ctx = egui::Context::default();
        let items = vec![TableItem {
            key: "second-id".into(),
            cells: vec!["Second".into(), "Role".into()],
            tooltip: "Second".into(),
            selected: false,
        }];
        let mut selected = None;
        let pointer = egui::pos2(75.0, 60.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [
            vec![],
            vec![egui::Event::PointerMoved(pointer), button(true)],
            vec![button(false)],
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if let Some((index, _)) = data_table(
                        ui,
                        "test_rows",
                        &["NAME", "KIND"],
                        &[200.0, 150.0],
                        &items,
                        200.0,
                    ) {
                        selected = Some(items[index].key.clone());
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(selected.as_deref(), Some("second-id"));
    }
    #[test]
    fn table_keyboard_focus_follows_key_after_filtering() {
        let ctx = egui::Context::default();
        let make = |key: &str| TableItem {
            key: key.into(),
            cells: vec![key.into()],
            tooltip: key.into(),
            selected: false,
        };
        let mut items = vec![make("first"), make("second")];
        let mut selected = None;
        let render = |ctx: &egui::Context,
                      items: &[TableItem],
                      events: Vec<egui::Event>,
                      selected: &mut Option<String>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if let Some((index, _)) =
                        data_table(ui, "focus_test", &["NAME"], &[200.0], items, 200.0)
                    {
                        *selected = Some(items[index].key.clone());
                    }
                },
            );
            output.textures_delta.clear();
        };
        render(&ctx, &items, vec![], &mut selected);
        let id = egui::Id::unique(("focus_test", &items[1].key));
        ctx.memory_mut(|m| m.request_focus(id));
        items.remove(0);
        render(&ctx, &items, vec![], &mut selected);
        assert!(ctx.memory(|m| m.has_focus(id)));
        render(
            &ctx,
            &items,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut selected,
        );
        assert_eq!(selected.as_deref(), Some("second"));
    }
    #[test]
    fn narrow_data_views_expose_stacked_content_to_page_scroll() {
        let ctx = egui::Context::default();
        let mut app = ProcessCraft::default();
        for view in [1, 2, 3] {
            app.view = view;
            let mut content_height = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 250.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("narrow_test", view))
                        .show(ui, |ui| {
                            app.m2_panel(ui);
                            content_height = ui.min_rect().height();
                        });
                },
            );
            output.textures_delta.clear();
            assert!(
                content_height.is_finite() && content_height > 250.0,
                "view {view}: {content_height}"
            );
        }
    }
    #[test]
    fn reselecting_dictionary_entry_preserves_its_draft() {
        let mut app = ProcessCraft::default();
        let entry = DictionaryEntry {
            id: processcraft_core::ids::DictionaryEntryId::new(),
            kind: DictionaryKind::System,
            name: "ERP".into(),
            description: "Original".into(),
        };
        assert!(app.ctl.mutate(vec![Mutation::CreateDictionaryEntry {
            entry: entry.clone()
        }]));
        app.dictionary_selected = Some(entry.id);
        app.dictionary_name = "ERP Cloud".into();
        app.dictionary_description = "In progress".into();
        let snap = app.ctl.session.snapshot();
        app.choose_dictionary_entry(&snap.project, &entry);
        assert_eq!(app.dictionary_name, "ERP Cloud");
        assert_eq!(app.dictionary_description, "In progress");
        assert!(app.dictionary_draft_changed(&snap.project));
    }
    #[test]
    fn assigning_role_moves_connected_activity_and_undo_restores_it() {
        let mut app = ProcessCraft::default();
        let mut project = controller::blank_project();
        let d = std::sync::Arc::make_mut(&mut project.documents[0]);
        let p = &mut d.processes[0];
        let node = controller::id("Node");
        let target = controller::id("Node");
        let flow = controller::id("Flow");
        let lane = controller::id("Lane");
        for id in [&node, &target] {
            p.nodes.push(Node {
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: id.clone(),
                name: "Task".into(),
                documentation: String::new(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            });
        }
        p.lanes.push(Lane {
            child_lane_set_id: None,
            id: lane.clone(),
            name: "Role".into(),
            parent: None,
            members: vec![],
        });
        p.flows.push(Flow {
            association_direction: None,
            id: flow.clone(),
            kind: FlowKind::Sequence,
            source: node.clone(),
            target: target.clone(),
            name: String::new(),
            documentation: String::new(),
            condition: None,
        });
        let diagram = &mut d.diagrams[0];
        for (id, x) in [(&node, 0.0), (&target, 300.0)] {
            diagram.shapes.push(Shape {
                is_expanded: None,
                id: controller::id("Shape"),
                element: id.clone(),
                bounds: Bounds {
                    x,
                    y: 0.0,
                    width: 100.0,
                    height: 70.0,
                },
                label: None,
                style: None,
            });
        }
        diagram.edges.push(Edge {
            id: controller::id("Edge"),
            element: flow,
            waypoints: vec![Point { x: 100.0, y: 35.0 }, Point { x: 300.0, y: 35.0 }],
            label: None,
            style: None,
        });
        let doc_id = d.id;
        let process_id = d.processes[0].id.clone();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let snap = app.ctl.session.snapshot();
        let d = &snap.project.documents[0];
        let (bounds, mut moves) = app
            .role_assignment_bounds(
                d,
                &node,
                &Bounds {
                    x: 0.0,
                    y: 200.0,
                    width: 600.0,
                    height: 150.0,
                },
            )
            .unwrap();
        moves.push(Mutation::AssignLane {
            document_id: doc_id,
            process_id,
            element_id: node.clone(),
            lane_id: Some(lane),
            bounds,
        });
        assert!(app.ctl.mutate(moves));
        let changed = app.ctl.session.snapshot();
        let d = &changed.project.documents[0];
        assert_eq!(d.processes[0].lanes[0].members, vec![node]);
        assert_eq!(d.diagrams[0].shapes[0].bounds.y, 220.0);
        assert_ne!(d.diagrams[0].edges[0].waypoints[0].y, 35.0);
        assert!(app.ctl.apply(Request::Undo));
        let undone = app.ctl.session.snapshot();
        let d = &undone.project.documents[0];
        assert!(d.processes[0].lanes[0].members.is_empty());
        assert_eq!(d.diagrams[0].shapes[0].bounds.y, 0.0);
        assert_eq!(d.diagrams[0].edges[0].waypoints[0].y, 35.0);
    }
}
