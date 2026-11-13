use super::*;
use processcraft_core::{
    attributes::{AttributeDefinition, AttributeTarget},
    commands::DictionaryDeleteResolution,
    ids::{DictionaryEntryId, DocumentId, ElementKey},
    queries,
};

use egui_extras::{Column, TableBuilder};

pub(super) struct TableItem {
    pub(super) key: String,
    pub(super) cells: Vec<String>,
    pub(super) tooltip: String,
    pub(super) selected: bool,
}

/// The mutations of one RACI toggle: the fixed definition is created in the
/// same batch when the project has none yet, then the role is added to or
/// removed from the activity's list (an empty list clears the value).
pub(super) fn raci_toggle_mutations(
    project: &ProjectData,
    key: &ElementKey,
    raci: processcraft_core::attributes::RaciAttribute,
    target: &DictionaryTarget,
) -> Vec<Mutation> {
    use processcraft_core::attributes::{AttributeValue, AttributeValueInput};
    let mut refs = project
        .attribute_values
        .iter()
        .find(|v| v.source == *key && v.definition == raci.id())
        .and_then(|v| match &v.value {
            AttributeValue::DictionaryRefs(refs) => Some(refs.clone()),
            _ => None,
        })
        .unwrap_or_default();
    match refs.iter().position(|t| t == target) {
        Some(index) => {
            refs.remove(index);
        }
        None => refs.push(target.clone()),
    }
    let mut mutations = Vec::new();
    if !project
        .attribute_definitions
        .iter()
        .any(|d| d.id == raci.id())
    {
        mutations.push(Mutation::CreateAttributeDefinition {
            definition: raci.definition(),
        });
    }
    mutations.push(Mutation::SetAttributeValues {
        source: key.clone(),
        values: vec![AttributeValueInput {
            definition_id: raci.id(),
            value: (!refs.is_empty()).then_some(AttributeValue::DictionaryRefs(refs)),
        }],
    });
    mutations
}

pub(super) fn capability_label(capability: &Capability) -> &'static str {
    match capability {
        Capability::Editable => "Editable",
        Capability::RetainedExtensions { .. } => "Read-only structure",
        Capability::PreservedSource { .. } => "Preserved source",
    }
}
pub(super) fn dictionary_kind_label(kind: DictionaryKind) -> &'static str {
    match kind {
        DictionaryKind::Role => "Role",
        DictionaryKind::System => "IT system",
        DictionaryKind::Document => "Document",
    }
}
pub(super) fn data_table(
    ui: &mut egui::Ui,
    id: &'static str,
    headings: &[&str],
    widths: &[f32],
    rows: &[TableItem],
    height: f32,
) -> Option<(usize, bool)> {
    data_table_editable(ui, id, headings, widths, rows, height, &mut |_, _, _| false)
}

/// `cell(ui, row, column)` may draw an editor into a cell and return `true`; `false` leaves the
/// default text label.
pub(super) fn data_table_editable(
    ui: &mut egui::Ui,
    id: &'static str,
    headings: &[&str],
    widths: &[f32],
    rows: &[TableItem],
    height: f32,
    cell: &mut dyn FnMut(&mut egui::Ui, usize, usize) -> bool,
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
                                if cell(ui, index, column) {
                                    return;
                                }
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
    pub(super) fn analyst_views_panel(&mut self, ui: &mut egui::Ui) {
        let snap = self.ctl.session.snapshot();
        let project = &snap.project;
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
        match self.view {
            1 => {
                let library = queries::library_search(project, &self.library_filter);
                ui.horizontal(|ui| {
                    ui.heading("Library");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::widgets::primary_button(ui, "New model").clicked() {
                            self.new_model();
                        }
                    });
                });
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
                    let count = library
                        .iter()
                        .filter(|&&id| {
                            project.documents.iter().any(|d| {
                                d.id == id
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
                            let docs: Vec<_> = library
                                .iter()
                                .filter_map(|&id| {
                                    project.documents.iter().enumerate().find(|(_, d)| {
                                        d.id == id
                                            && self.folder_selected.as_ref().is_none_or(|folder| {
                                                d.folder.as_ref() == Some(folder)
                                            })
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
                                        capability_label(&doc.capability).into(),
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
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("No models match this search.").weak(),
                                    );
                                    if !self.library_filter.is_empty()
                                        && ui.button("Clear search").clicked()
                                    {
                                        self.library_filter.clear();
                                    }
                                    if crate::widgets::primary_button(ui, "New model").clicked() {
                                        self.new_model();
                                    }
                                });
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
                ui.horizontal(|ui| {
                    for (tab, text) in [
                        (attributes::Tab::Entries, "Entries"),
                        (attributes::Tab::Attributes, "Attributes"),
                    ] {
                        ui.selectable_value(&mut self.attributes.tab, tab, text);
                    }
                });
                if self.attributes.tab == attributes::Tab::Attributes {
                    ui.add_space(8.0);
                    self.attribute_definitions_ui(ui, project);
                    return;
                }
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
                            // One pass over the links instead of a query per entry.
                            let mut usages = std::collections::HashMap::new();
                            for link in &project.dictionary_links {
                                if let DictionaryTarget::Entry(id) = link.target {
                                    *usages.entry(id).or_insert(0usize) += 1;
                                }
                            }
                            let items: Vec<_> = entries
                                .iter()
                                .map(|entry| {
                                    let count = usages.get(&entry.id).copied().unwrap_or(0);
                                    TableItem {
                                        key: entry.id.to_string(),
                                        cells: vec![
                                            entry.name.clone(),
                                            format!(
                                                "{}  ·  {count} {}",
                                                dictionary_kind_label(entry.kind),
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
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(if project.dictionary_entries.is_empty() {
                                            "No shared terms yet. Add the roles, IT systems and documents your models use."
                                        } else {
                                            "No entries match this search."
                                        })
                                        .weak(),
                                    );
                                    if !self.dictionary_filter.is_empty()
                                        && ui.button("Clear search").clicked()
                                    {
                                        self.dictionary_filter.clear();
                                    }
                                });
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
                ui.add_space(8.0);
                let p = self.theme.palette();
                egui::Frame::new()
                    .fill(p.canvas)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(crate::theme::RADIUS_CARD)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if doc.diagrams.is_empty() {
                            ui.add_sized(
                                [ui.available_width(), 150.0],
                                egui::Label::new(egui::RichText::new("No diagram").weak()),
                            );
                        } else {
                            self.diagram_preview(ui, 150.0);
                        }
                    });
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&doc.name).font(crate::theme::semibold(15.0)));
                ui.label(format!("Capability: {}", capability_label(&doc.capability)));
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
                        ui.selectable_value(
                            &mut self.dictionary_kind,
                            kind,
                            dictionary_kind_label(kind),
                        );
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
        let Some(d) = project.documents.get(self.doc) else {
            ui.heading("Activity table");
            return;
        };
        ui.label(egui::RichText::new(format!("{}  ›  Activities", project.name)).weak());
        // View state only: never project content, never saved.
        let raci_flag = egui::Id::unique("activity_table_raci");
        let raci_on = ui.ctx().data(|data| data.get_temp::<bool>(raci_flag)) == Some(true);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&d.name).font(crate::theme::semibold(18.0)));
            ui.add_space(8.0);
            let p = self.theme.palette();
            egui::Frame::new()
                .fill(p.secondary)
                .corner_radius(crate::theme::RADIUS)
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal(|ui| {
                        if super::shell::segment(ui, false, "Model").clicked()
                            && self.prepare_navigation()
                        {
                            self.view = 0;
                        }
                        if super::shell::segment(ui, !raci_on, "Table").clicked() {
                            ui.ctx().data_mut(|data| data.insert_temp(raci_flag, false));
                        }
                        if super::shell::segment(ui, raci_on, "RACI").clicked() {
                            ui.ctx().data_mut(|data| data.insert_temp(raci_flag, true));
                        }
                    });
                });
        });
        let Some(p) = queries::scopes(d)
            .into_iter()
            .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        else {
            return;
        };
        if raci_on {
            let editable = d.capability.allows_structural_editing();
            self.raci_ui(ui, project, d.id, &p.id.selector(), editable);
            return;
        }
        // The table query walks the whole process; reuse it until the model changes.
        let key = (
            self.ctl.session.id(),
            self.ctl.session.snapshot().generation,
            d.id,
            p.id.selector(),
        );
        let slot = egui::Id::unique("activity_table_rows");
        type Cached = std::sync::Arc<(
            (
                String,
                u64,
                processcraft_core::ids::DocumentId,
                ElementSelector,
            ),
            Option<Vec<queries::ActivityRow>>,
        )>;
        let cached = ui
            .ctx()
            .data(|data| data.get_temp::<Cached>(slot))
            .filter(|cached| cached.0 == key);
        let cached = cached.unwrap_or_else(|| {
            let fresh: Cached = std::sync::Arc::new((
                key.clone(),
                queries::activity_table(project, d.id, &p.id.selector()),
            ));
            ui.ctx()
                .data_mut(|data| data.insert_temp(slot, fresh.clone()));
            fresh
        });
        let Some(rows) = &cached.1 else {
            return;
        };
        ui.add_space(12.0);
        // Attribute columns are view state; a deleted or no longer applicable one drops out.
        self.attributes.columns.retain(|id| {
            project
                .attribute_definitions
                .iter()
                .any(|d| d.id == *id && d.applies_to.contains(&AttributeTarget::Activity))
        });
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Attribute columns…", |ui| {
                let mut any = false;
                for definition in project
                    .attribute_definitions
                    .iter()
                    .filter(|d| d.applies_to.contains(&AttributeTarget::Activity))
                {
                    any = true;
                    let mut on = self.attributes.columns.contains(&definition.id);
                    if ui.checkbox(&mut on, &definition.name).changed() {
                        if on {
                            self.attributes.columns.push(definition.id);
                        } else {
                            self.attributes.columns.retain(|id| *id != definition.id);
                            // A hidden column keeps no unsaved text.
                            self.attributes
                                .cells
                                .retain(|(_, id), _| *id != definition.id);
                        }
                    }
                }
                if !any {
                    ui.label("No attribute applies to activities. Create one under Dictionary, Attributes.");
                }
            });
            if !self.attributes.marked.is_empty() && ui.button("Clear row selection").clicked() {
                self.attributes.marked.clear();
            }
            ui.label(
                egui::RichText::new(
                    "Ctrl+click rows to select several; editing an attribute cell in one of them sets it on all.",
                )
                .small()
                .weak(),
            );
        });
        // Marked rows must be visible rows of this document and process.
        self.attributes
            .marked
            .retain(|key| rows.iter().any(|row| row.key == *key));
        let columns: Vec<&AttributeDefinition> = self
            .attributes
            .columns
            .iter()
            .filter_map(|id| project.attribute_definitions.iter().find(|d| d.id == *id))
            .collect();
        let items: Vec<_> = rows
            .iter()
            .map(|row| {
                let lane = row
                    .lane
                    .as_ref()
                    .and_then(|id| p.lanes.iter().find(|l| &l.id == id))
                    .map_or("—", |l| l.name.as_deref().unwrap_or("—"));
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
                                if n.name.as_deref().is_none_or(str::is_empty) {
                                    "Unnamed element"
                                } else {
                                    n.name.as_deref().unwrap_or("")
                                }
                            },
                        );
                        match &flow.condition {
                            Some(c) if c.plain_text().is_none_or(|text| !text.is_empty()) => {
                                format!(
                                    "{target} ({})",
                                    c.plain_text().unwrap_or("Structured expression")
                                )
                            }
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
                let mut cells = vec![
                    row.name.clone().unwrap_or_default(),
                    row.role.clone().unwrap_or_else(|| "—".into()),
                    lane.into(),
                    systems,
                    documents,
                    outgoing.clone(),
                ];
                for definition in &columns {
                    cells.push(
                        row.attributes
                            .iter()
                            .find(|cell| cell.definition_id == definition.id)
                            .map_or_else(
                                || "—".to_owned(),
                                |cell| attributes::display_value(project, &cell.value),
                            ),
                    );
                }
                TableItem {
                    key: format!("{} / {}", row.key.document_id, row.key.xml_id),
                    cells,
                    tooltip: format!("{} / {}\n{}", row.key.document_id, row.key.xml_id, outgoing),
                    selected: if self.attributes.marked.is_empty() {
                        self.selected.as_ref() == Some(&row.key.xml_id)
                    } else {
                        self.attributes.marked.contains(&row.key)
                    },
                }
            })
            .collect();
        let table_height = (ui.available_height() * 0.5).max(150.0);
        let mut headings = vec!["NAME", "ROLE", "LANE", "SYSTEMS", "DOCUMENTS", "OUTGOING"];
        let mut widths = vec![190.0, 110.0, 110.0, 125.0, 125.0, 180.0];
        for definition in &columns {
            headings.push(definition.name.as_str());
            widths.push(140.0);
        }
        let editable = d.capability.allows_structural_editing();
        let mut edits = Vec::new();
        let mut invalid = None;
        let marked = &self.attributes.marked;
        let drafts = &mut self.attributes.cells;
        let clicked = data_table_editable(
            ui,
            "activity_table",
            &headings,
            &widths,
            &items,
            table_height,
            &mut |ui, index, column| {
                let Some(definition) = column.checked_sub(6).and_then(|i| columns.get(i)) else {
                    return false;
                };
                let row = &rows[index];
                let current = row
                    .attributes
                    .iter()
                    .find(|cell| cell.definition_id == definition.id)
                    .map(|cell| &cell.value);
                let outcome = attributes::value_editor(
                    ui,
                    drafts,
                    attributes::Field {
                        project,
                        key: &row.key,
                        definition,
                        current,
                        enabled: editable,
                        compact: true,
                        force: marked.len() > 1 && marked.contains(&row.key),
                        name: format!(
                            "{} for {}",
                            definition.name,
                            row.name.as_deref().unwrap_or("unnamed activity")
                        ),
                        label: None,
                    },
                );
                match outcome {
                    attributes::Edit::Commit(value) => {
                        edits.push((row.key.clone(), definition.id, value))
                    }
                    attributes::Edit::Invalid(message) => invalid = Some(message),
                    attributes::Edit::Idle => {}
                }
                true
            },
        );
        if let Some(message) = invalid {
            self.ctl.error = message;
        }
        for (key, definition, value) in edits {
            self.apply_table_edit(&key, definition, value);
        }
        if let Some((index, _)) = clicked {
            let key = rows[index].key.clone();
            if ui.input(|i| i.modifiers.command || i.modifiers.shift) {
                // Seed from the selected row only when it is a visible activity row.
                if self.attributes.marked.is_empty()
                    && let Some(current) = rows
                        .iter()
                        .find(|row| self.selected.as_ref() == Some(&row.key.xml_id))
                {
                    self.attributes.marked.push(current.key.clone());
                }
                if let Some(position) = self.attributes.marked.iter().position(|m| *m == key) {
                    self.attributes.marked.remove(position);
                } else {
                    self.attributes.marked.push(key);
                }
            } else {
                self.attributes.marked.clear();
                self.select(Some(key.xml_id));
            }
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
    /// Toggles one role in one A/C/I attribute of one activity: one batch, so
    /// one undo step, which also creates the fixed definition on first use.
    pub(super) fn toggle_raci(
        &mut self,
        key: &ElementKey,
        raci: processcraft_core::attributes::RaciAttribute,
        target: &DictionaryTarget,
    ) -> bool {
        let snap = self.ctl.session.snapshot();
        let mutations = raci_toggle_mutations(&snap.project, key, raci, target);
        self.ctl.mutate(mutations)
    }

    /// Third segment of the activity table: role columns, activity rows, R/A/C/I cells. The
    /// activity names stay fixed on the left while the role columns scroll.
    fn raci_ui(
        &mut self,
        ui: &mut egui::Ui,
        project: &ProjectData,
        document: DocumentId,
        process: &ElementSelector,
        editable: bool,
    ) {
        use processcraft_core::attributes::RaciAttribute;
        const ROW: f32 = 30.0;
        const NAME_WIDTH: f32 = 220.0;
        const COLUMN: f32 = 112.0;
        // The matrix walks the whole process; reuse it until the model changes.
        let key = (
            self.ctl.session.id(),
            self.ctl.session.snapshot().generation,
            document,
            process.clone(),
        );
        let slot = egui::Id::unique("raci_matrix_cache");
        type Cached = std::sync::Arc<(
            (String, u64, DocumentId, ElementSelector),
            Option<queries::RaciMatrix>,
        )>;
        let cached = ui
            .ctx()
            .data(|data| data.get_temp::<Cached>(slot))
            .filter(|cached| cached.0 == key);
        let cached = cached.unwrap_or_else(|| {
            let fresh: Cached = std::sync::Arc::new((
                key.clone(),
                queries::raci_matrix(project, document, process),
            ));
            ui.ctx()
                .data_mut(|data| data.insert_temp(slot, fresh.clone()));
            fresh
        });
        let Some(matrix) = &cached.1 else {
            return;
        };
        // Roles added to the grid before they hold a letter: view state.
        let extra_slot = egui::Id::unique("raci_extra_roles");
        let mut extra: Vec<DictionaryEntryId> = ui
            .ctx()
            .data(|data| data.get_temp(extra_slot))
            .unwrap_or_default();
        let mut columns = matrix.columns.clone();
        extra.retain(|id| {
            project
                .dictionary_entries
                .iter()
                .any(|e| e.id == *id && e.kind == DictionaryKind::Role)
        });
        for id in &extra {
            let target = DictionaryTarget::Entry(*id);
            if !columns.iter().any(|c| c.target == target)
                && let Some(entry) = project.dictionary_entries.iter().find(|e| e.id == *id)
            {
                columns.push(queries::RaciColumn {
                    target,
                    name: entry.name.clone(),
                });
            }
        }
        ui.add_space(12.0);
        let mut export = false;
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Add role column…", |ui| {
                let mut any = false;
                for entry in project
                    .dictionary_entries
                    .iter()
                    .filter(|e| e.kind == DictionaryKind::Role)
                    .filter(|e| {
                        !columns
                            .iter()
                            .any(|c| c.target == DictionaryTarget::Entry(e.id))
                    })
                {
                    any = true;
                    if ui.button(&entry.name).clicked() {
                        extra.push(entry.id);
                        ui.close();
                    }
                }
                if !any {
                    ui.label("Every role of the dictionary is already a column.");
                }
            });
            export = ui.button("Export CSV").clicked();
            ui.label(
                egui::RichText::new(
                    "R follows the role of the activity's lane. Click A, C or I to toggle; \
                     the CSV lists the columns that hold a letter.",
                )
                .small()
                .weak(),
            );
        });
        ui.ctx()
            .data_mut(|data| data.insert_temp(extra_slot, extra));
        if export {
            self.export_raci_csv(ui.ctx().clone(), matrix);
        }
        if matrix.rows.is_empty() {
            ui.label("No activities in this process.");
            return;
        }
        let muted = ui.visuals().weak_text_color();
        let mut toggles = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("raci_vertical")
            .max_height((ui.available_height() * 0.8).max(150.0))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                    ui.vertical(|ui| {
                        ui.set_width(NAME_WIDTH);
                        ui.add_sized(
                            [NAME_WIDTH, ROW],
                            egui::Label::new(
                                egui::RichText::new("ACTIVITY")
                                    .size(12.0)
                                    .strong()
                                    .color(muted),
                            ),
                        );
                        for row in &matrix.rows {
                            ui.add_sized(
                                [NAME_WIDTH, ROW],
                                egui::Label::new(row.name.as_deref().unwrap_or("Unnamed activity"))
                                    .truncate(),
                            )
                            .on_hover_text(format!("{} / {}", row.key.document_id, row.key.xml_id));
                        }
                    });
                    egui::ScrollArea::horizontal()
                        .id_salt("raci_horizontal")
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                for column in &columns {
                                    ui.vertical(|ui| {
                                        ui.set_width(COLUMN);
                                        ui.add_sized(
                                            [COLUMN, ROW],
                                            egui::Label::new(
                                                egui::RichText::new(&column.name)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(muted),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&column.name);
                                        for row in &matrix.rows {
                                            let cell = row
                                                .cells
                                                .iter()
                                                .find(|cell| cell.target == column.target);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(COLUMN, ROW),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    let responsible =
                                                        cell.is_some_and(|c| c.responsible);
                                                    let r = egui::RichText::new("R");
                                                    ui.label(if responsible {
                                                        r.strong()
                                                    } else {
                                                        r.weak()
                                                    })
                                                    .on_hover_text(
                                                        "Responsible is derived: it follows the \
                                                         role linked to the activity's lane. \
                                                         Change the lane's role to change it.",
                                                    );
                                                    for raci in RaciAttribute::ALL {
                                                        let on = cell.is_some_and(|c| match raci {
                                                            RaciAttribute::Accountable => {
                                                                c.accountable
                                                            }
                                                            RaciAttribute::Consulted => c.consulted,
                                                            RaciAttribute::Informed => c.informed,
                                                        });
                                                        let response = ui
                                                            .add_enabled_ui(editable, |ui| {
                                                                ui.selectable_label(
                                                                    on,
                                                                    raci.letter().to_string(),
                                                                )
                                                            })
                                                            .inner
                                                            .on_hover_text(format!(
                                                                "{} for {}: {}",
                                                                raci.name(),
                                                                row.name
                                                                    .as_deref()
                                                                    .unwrap_or("unnamed activity"),
                                                                column.name
                                                            ));
                                                        if response.clicked() {
                                                            toggles.push((
                                                                row.key.clone(),
                                                                raci,
                                                                column.target.clone(),
                                                            ));
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                    });
                                }
                            });
                        });
                });
            });
        for (key, raci, target) in toggles {
            self.toggle_raci(&key, raci, &target);
        }
    }

    /// Asks for a destination and writes the CSV through the same grant-checked path as the
    /// other exports.
    fn export_raci_csv(&mut self, ctx: egui::Context, matrix: &queries::RaciMatrix) {
        if self.ctl.is_saving() {
            self.ctl.error = "Wait for the project save to finish before exporting.".into();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name("raci.csv")
            .save_file()
        else {
            return;
        };
        let grant = match processcraft_io::FileGrant::new(&path, processcraft_io::Access::Write)
            .and_then(|grant| {
                if self
                    .ctl
                    .file
                    .as_ref()
                    .map(|file| file.same_target(&grant))
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(std::io::Error::other(
                        "Choose a destination other than the open project file.",
                    ));
                }
                Ok(grant)
            }) {
            Ok(grant) => grant,
            Err(error) => {
                self.ctl.error = format!("Cannot use export destination: {error}");
                return;
            }
        };
        self.effects.write(
            ctx,
            grant,
            queries::raci_csv(matrix).into_bytes(),
            self.ctl.session.id(),
        );
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
            ui.collapsing(format!("{} links", dictionary_kind_label(kind)), |ui| {
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
                    match queries::role_lane_choices(&snap.project, d.id, &p.id.selector(), role.id) {
                        Some(queries::RoleLaneChoices::CreateLane) => {
                            if ui.button("Create lane and assign").clicked() && self.commit_draft() {
                                let lane_id = controller::id("Lane");
                                let lane: ElementSelector = lane_id.clone().into();
                                let lane_bounds = Bounds { x: self.camera.0 + 30.0, y: self.camera.1 + 80.0 + p.lanes.len() as f64 * 160.0, width: 600.0, height: 150.0 };
                                let Some((bounds, moves)) = self.role_assignment_bounds(d, &element, &lane_bounds) else { return };
                                let mut mutations = vec![Mutation::CreateLane {  document_id: d.id, process_id: p.id.selector(), lane: Lane { base: None, child_lane_set_base: None, child_lane_set_name: None, partition_element: None, partition_element_ref: None,
    child_lane_set_id: None, id: lane_id.into(), name: Some(role.name.clone()), parent: None, members: vec![] } }, Mutation::SetDictionaryLinks { source: processcraft_core::ids::ElementKey { document_id: d.id, xml_id: lane.clone() }, kind: DictionaryKind::Role, targets: vec![DictionaryTarget::Entry(role.id)] }];
                                if let Some(diagram) = d.diagrams.iter().find(|diagram| self.diagram_id.as_ref().is_none_or(|id| &diagram.id == id)) {
                                    mutations.push(Mutation::CreateShape {  document_id: d.id, diagram_id: diagram.id.selector(), shape: Shape { di_metadata: None, is_message_visible: None, participant_band_kind: None, choreography_activity_shape: None, label_visibility: Some(DiLabelVisibility::Explicit), is_horizontal: None, is_marker_visible: None,
    is_expanded: None, id: controller::id("LaneShape").into(), element: lane.clone(), bounds: lane_bounds, label: Some(Label::default()), style: None } });
                                }
                                mutations.extend(moves);
                                mutations.push(Mutation::AssignLane { document_id: d.id, process_id: p.id.selector(), element_id: element.clone(), lane_id: Some(lane), bounds });
                                self.ctl.mutate(mutations); ui.close();
                            }
                        }
                        Some(choice) => {
                            let lanes = match choice { queries::RoleLaneChoices::Assign(id) => vec![id], queries::RoleLaneChoices::Choose(ids) => ids, _ => vec![] };
                            for lane in lanes {
                                let name = p.lanes.iter().find(|l| l.id == lane).map_or_else(|| lane.to_string(), |l| l.name.clone().unwrap_or_default());
                                if ui.button(name).clicked() && self.commit_draft() {
                                    let Some(lane_bounds) = d.diagrams.first().and_then(|diagram| diagram.shapes.iter().find(|shape| shape.element == lane)).map(|shape| &shape.bounds) else { self.ctl.error = "The lane must have a diagram shape before assigning a role.".into(); return };
                                    let Some((bounds, mut moves)) = self.role_assignment_bounds(d, &element, lane_bounds) else { return };
                                    moves.push(Mutation::AssignLane { document_id: d.id, process_id: p.id.selector(), element_id: element.clone(), lane_id: Some(lane), bounds });
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
            process_id: p.id.selector(),
            flow_id,
            node: Node {
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: node_id.clone().into(),
                name: Some("Task".into()),
                documentation: String::new().into(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            },
            new_flow: Flow {
                message_ref: None,
                flow_metadata: None,
                base_extras: None,
                association_direction: None,
                id: new_flow_id.clone().into(),
                kind: FlowKind::Sequence,
                source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                    node_id.clone().into(),
                )),
                target: flow.target.clone(),
                name: Some(String::new()),
                documentation: String::new().into(),
                condition: None,
            },
            diagram_id: diagram.id.selector(),
            shape: Shape {
                di_metadata: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                label_visibility: Some(DiLabelVisibility::Explicit),
                is_horizontal: None,
                is_marker_visible: None,
                is_expanded: None,
                id: controller::id("Shape").into(),
                element: node_id.clone().into(),
                bounds: Bounds {
                    x: x - 50.0,
                    y: y - 35.0,
                    width: 100.0,
                    height: 70.0,
                },
                label: Some(Label::default()),
                style: None,
            },
            old_waypoints: vec![a.clone(), enter],
            new_edge: Edge {
                di_metadata: None,
                message_visible_kind: None,
                label_visibility: Some(DiLabelVisibility::Explicit),
                source_element: None,
                target_element: None,
                id: controller::id("Edge").into(),
                element: new_flow_id.into(),
                waypoints: vec![exit, b.clone()],
                label: Some(Label::default()),
                style: None,
            },
        };
        if self.ctl.mutate(vec![mutation]) {
            self.select(Some(node_id.into()));
        }
    }

    fn role_assignment_bounds(
        &mut self,
        d: &BpmnDocument,
        element: &ElementSelector,
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
            diagram_id: diagram.id.selector(),
            shape_id: shape.id.selector(),
            bounds,
        }];
        match processcraft_core::geometry::prepare_move(d, &diagram.id.selector(), bounds.clone()) {
            Ok(prepared) => {
                if !prepared.warnings.is_empty() {
                    self.ctl.notify(prepared.warnings.join("; "));
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
    use processcraft_core::attributes::{
        AttributeDeleteResolution, AttributeOptionMapping, AttributeUpdateResolution,
        AttributeValue,
    };
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
                            app.analyst_views_panel(ui);
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
    /// An app with one task carrying `Risk = High` (options Low, High, applies to activities).
    fn app_with_risk() -> (
        ProcessCraft,
        processcraft_core::attributes::AttributeDefinition,
    ) {
        use processcraft_core::attributes::*;
        let mut project = controller::blank_project();
        let document = std::sync::Arc::make_mut(&mut project.documents[0]);
        let id = controller::id("Node");
        document.processes[0].nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            details: None,
            id: id.clone().into(),
            name: Some("Task".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        });
        let key = processcraft_core::ids::ElementKey {
            document_id: document.id,
            xml_id: id.into(),
        };
        let risk = AttributeDefinition {
            id: processcraft_core::ids::AttributeDefinitionId::new(),
            name: "Risk".into(),
            description: String::new(),
            value_type: AttributeType::Enum,
            unit: None,
            options: vec!["Low".into(), "High".into()],
            dictionary_kind: None,
            applies_to: vec![AttributeTarget::Activity],
            required: false,
        };
        let mut app = ProcessCraft::default();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        assert!(app.ctl.mutate(vec![
            Mutation::CreateAttributeDefinition {
                definition: risk.clone()
            },
            Mutation::SetAttributeValues {
                source: key,
                values: vec![AttributeValueInput {
                    definition_id: risk.id,
                    value: Some(AttributeValue::Enum("High".into())),
                }],
            },
        ]));
        let snap = app.ctl.session.snapshot();
        app.choose_definition(&snap.project, Some(risk.id));
        (app, risk)
    }
    #[test]
    fn toggling_a_raci_cell_is_one_undo_step_and_creates_the_definition_once() {
        use processcraft_core::attributes::{AttributeValue, RaciAttribute};
        let (mut app, _) = app_with_risk();
        let key = app.ctl.session.snapshot().project.attribute_values[0]
            .source
            .clone();
        let role = DictionaryEntry {
            id: processcraft_core::ids::DictionaryEntryId::new(),
            kind: DictionaryKind::Role,
            name: "Buyer".into(),
            description: String::new(),
        };
        let target = DictionaryTarget::Entry(role.id);
        assert!(
            app.ctl
                .mutate(vec![Mutation::CreateDictionaryEntry { entry: role }])
        );
        let before = app.ctl.session.snapshot().generation;
        assert!(app.toggle_raci(&key, RaciAttribute::Accountable, &target));
        let after = app.ctl.session.snapshot();
        assert_eq!(
            after.generation,
            before + 1,
            "definition and value in one batch"
        );
        assert!(
            after
                .project
                .attribute_definitions
                .iter()
                .any(|d| d.id == RaciAttribute::Accountable.id())
        );
        assert!(after.project.attribute_values.iter().any(|v| {
            v.definition == RaciAttribute::Accountable.id()
                && v.value == AttributeValue::DictionaryRefs(vec![target.clone()])
        }));
        // A second toggle only changes the value, then one undo restores the first state.
        assert!(app.toggle_raci(&key, RaciAttribute::Accountable, &target));
        assert!(
            !app.ctl
                .session
                .snapshot()
                .project
                .attribute_values
                .iter()
                .any(|v| v.definition == RaciAttribute::Accountable.id())
        );
        assert!(app.ctl.apply(Request::Undo));
        assert!(app.ctl.apply(Request::Undo));
        let undone = app.ctl.session.snapshot();
        assert!(
            !undone
                .project
                .attribute_definitions
                .iter()
                .any(|d| d.id == RaciAttribute::Accountable.id())
        );
        assert!(
            undone
                .project
                .attribute_values
                .iter()
                .all(|v| v.definition != RaciAttribute::Accountable.id())
        );
    }
    #[test]
    fn restricted_definition_update_opens_the_prompt_instead_of_submitting() {
        let (mut app, risk) = app_with_risk();
        let before = app.ctl.session.snapshot().generation;
        // Removing the option "High" that a value uses is restricted.
        app.attributes.draft.as_mut().unwrap().options = "Low".into();
        assert!(!app.save_definition_draft());
        assert!(app.prompt_open());
        assert_eq!(app.ctl.session.snapshot().generation, before);
        assert_eq!(
            app.ctl.session.snapshot().project.attribute_definitions[0]
                .options
                .len(),
            2
        );
        // Mapping the removed option onto a kept one resolves it in one step.
        assert!(
            app.resolve_definition_update(AttributeUpdateResolution::MapOptions(vec![
                AttributeOptionMapping {
                    from: "High".into(),
                    to: "Low".into()
                }
            ]))
        );
        assert!(!app.prompt_open());
        let snap = app.ctl.session.snapshot();
        assert_eq!(snap.project.attribute_definitions[0].options, vec!["Low"]);
        assert_eq!(
            snap.project.attribute_values[0].value,
            AttributeValue::Enum("Low".into())
        );
        // Narrowing applies_to is restricted too; an unrestricted rename is submitted directly.
        app.attributes.draft.as_mut().unwrap().applies = [false; 8];
        app.attributes.draft.as_mut().unwrap().applies[1] = true;
        let before = app.ctl.session.snapshot().generation;
        assert!(!app.save_definition_draft());
        assert!(app.prompt_open());
        assert_eq!(app.ctl.session.snapshot().generation, before);
        app.attributes.prompt = None;
        app.attributes.draft.as_mut().unwrap().applies = [false; 8];
        app.attributes.draft.as_mut().unwrap().applies[0] = true;
        app.attributes.draft.as_mut().unwrap().name = "Risk level".into();
        assert!(app.save_definition_draft());
        assert!(!app.prompt_open());
        assert_eq!(
            app.ctl.session.snapshot().project.attribute_definitions[0].name,
            "Risk level"
        );
        assert_eq!(
            risk.id,
            app.ctl.session.snapshot().project.attribute_definitions[0].id
        );
    }
    #[test]
    fn deleting_a_used_definition_opens_the_prompt_instead_of_submitting() {
        let (mut app, _) = app_with_risk();
        let before = app.ctl.session.snapshot().generation;
        app.request_definition_delete();
        assert!(app.prompt_open());
        assert_eq!(app.ctl.session.snapshot().generation, before);
        // Reject is refused by core while values use the definition.
        assert!(!app.resolve_definition_delete(AttributeDeleteResolution::Reject));
        assert_eq!(
            app.ctl
                .session
                .snapshot()
                .project
                .attribute_definitions
                .len(),
            1
        );
        assert!(app.resolve_definition_delete(AttributeDeleteResolution::Discard));
        let snap = app.ctl.session.snapshot();
        assert!(snap.project.attribute_definitions.is_empty());
        assert!(snap.project.attribute_values.is_empty());
        assert!(!app.prompt_open());
    }
    #[test]
    fn attribute_tab_and_table_columns_render() {
        let (mut app, risk) = app_with_risk();
        app.view = 2;
        app.attributes.tab = attributes::Tab::Attributes;
        let ctx = egui::Context::default();
        let frame = |app: &mut ProcessCraft| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| app.analyst_views_panel(ui),
            );
            output.textures_delta.clear();
        };
        frame(&mut app);
        app.view = 3;
        app.attributes.columns = vec![risk.id];
        frame(&mut app);
        assert_eq!(app.attributes.columns, vec![risk.id]);
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
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: id.clone().into(),
                name: Some("Task".into()),
                documentation: String::new().into(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            });
        }
        p.lanes.push(Lane {
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            base: None,
            child_lane_set_id: None,
            id: lane.clone().into(),
            name: Some("Role".into()),
            parent: None,
            members: vec![],
        });
        p.flows.push(Flow {
            message_ref: None,
            flow_metadata: None,
            base_extras: None,
            association_direction: None,
            id: flow.clone().into(),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                node.clone().into(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                target.clone().into(),
            )),
            name: Some(String::new()),
            documentation: String::new().into(),
            condition: None,
        });
        let diagram = &mut d.diagrams[0];
        for (id, x) in [(&node, 0.0), (&target, 300.0)] {
            diagram.shapes.push(Shape {
                di_metadata: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                label_visibility: None,
                is_horizontal: None,
                is_marker_visible: None,
                is_expanded: None,
                id: controller::id("Shape").into(),
                element: id.clone().into(),
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
            di_metadata: None,
            message_visible_kind: None,
            label_visibility: None,
            source_element: None,
            target_element: None,
            id: controller::id("Edge").into(),
            element: flow.into(),
            waypoints: vec![Point { x: 100.0, y: 35.0 }, Point { x: 300.0, y: 35.0 }],
            label: None,
            style: None,
        });
        let doc_id = d.id;
        let process_id = d.processes[0].id.selector();
        app.ctl.session = processcraft_core::commands::Session::new(project).unwrap();
        let snap = app.ctl.session.snapshot();
        let d = &snap.project.documents[0];
        let (bounds, mut moves) = app
            .role_assignment_bounds(
                d,
                &node.clone().into(),
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
            element_id: node.clone().into(),
            lane_id: Some(lane.into()),
            bounds,
        });
        assert!(app.ctl.mutate(moves));
        let changed = app.ctl.session.snapshot();
        let d = &changed.project.documents[0];
        assert_eq!(
            d.processes[0].lanes[0].members,
            vec![processcraft_core::ids::ElementSelector::from(node)]
        );
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
