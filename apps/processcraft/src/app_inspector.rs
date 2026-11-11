use super::{ProcessCraft, node_kind_label};
use crate::widgets;

#[derive(Clone, Copy)]
enum ExpressionDefault {
    Base,
    SequenceFlow,
}

pub(super) struct ExpressionDraft {
    slot: String,
    id: Option<String>,
    formal: bool,
    explicit_type: bool,
    language: Option<String>,
    type_reference: Option<(String, String)>,
    text: String,
    namespaces: Vec<(String, String)>,
    error: String,
}

impl ExpressionDraft {
    fn new(
        slot: &str,
        value: &processcraft_core::bpmn_model::Expression,
        default: ExpressionDefault,
    ) -> Self {
        use processcraft_core::bpmn_model::{Expression, ExpressionKind};
        let mut draft = Self {
            slot: slot.into(),
            id: None,
            formal: false,
            explicit_type: false,
            language: None,
            type_reference: None,
            text: value.text().into(),
            namespaces: Vec::new(),
            error: String::new(),
        };
        if matches!(value, Expression::Legacy(_)) {
            draft.namespaces.push((
                String::new(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            ));
            if matches!(default, ExpressionDefault::SequenceFlow) {
                draft.formal = true;
                draft.explicit_type = true;
                draft.namespaces.push((
                    "xsi".into(),
                    "http://www.w3.org/2001/XMLSchema-instance".into(),
                ));
            }
        }
        if let Expression::Structured(data) = value {
            draft.id = data.id.as_ref().map(ToString::to_string);
            draft.namespaces = data.namespaces.clone();
            match &data.kind {
                ExpressionKind::Base { explicit_type } => draft.explicit_type = *explicit_type,
                ExpressionKind::Formal {
                    explicit_type,
                    language,
                    evaluates_to_type_ref,
                } => {
                    draft.formal = true;
                    draft.explicit_type = *explicit_type;
                    draft.language = language.clone();
                    draft.type_reference = evaluates_to_type_ref
                        .as_ref()
                        .map(|q| (q.namespace.clone(), q.local.to_string()));
                }
            }
        }
        draft
    }
    fn build(&self) -> Result<processcraft_core::bpmn_model::ExpressionData, String> {
        use processcraft_core::bpmn_model::{ExpressionData, ExpressionKind, QNameRef};
        let id = self
            .id
            .as_ref()
            .map(|s| {
                s.parse()
                    .map_err(|_| "Expression ID must be a valid XML ID.".to_owned())
            })
            .transpose()?;
        let type_ref = if self.formal {
            self.type_reference
                .as_ref()
                .map(|(namespace, local)| {
                    Ok::<QNameRef, String>(QNameRef {
                        namespace: namespace.clone(),
                        local: local.parse().map_err(|_| {
                            "Type reference local name must be a valid XML ID.".to_owned()
                        })?,
                    })
                })
                .transpose()?
        } else {
            None
        };
        if self.formal && !self.explicit_type {
            return Err("Formal expressions require an explicit type in this context.".into());
        }
        let kind = if self.formal {
            ExpressionKind::Formal {
                explicit_type: self.explicit_type,
                language: self.language.clone(),
                evaluates_to_type_ref: type_ref,
            }
        } else {
            ExpressionKind::Base {
                explicit_type: self.explicit_type,
            }
        };
        Ok(ExpressionData {
            id,
            kind,
            text: self.text.clone(),
            namespaces: self.namespaces.clone(),
        })
    }
}

fn optional_metadata_text(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut present = value.is_some();
    let checkbox = ui.checkbox(&mut present, label);
    if present {
        ui.text_edit_singleline(value.get_or_insert_with(String::new))
            .labelled_by(checkbox.id);
    } else {
        *value = None;
    }
}

fn expression_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut processcraft_core::bpmn_model::Expression,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
    default: ExpressionDefault,
) -> bool {
    use processcraft_core::bpmn_model::Expression;
    let label_response = ui.label(label);
    let mut changed = false;
    ui.add_enabled_ui(draft.is_none(), |ui| {
        let response = if multiline {
            ui.text_edit_multiline(value.text_mut())
        } else {
            ui.text_edit_singleline(value.text_mut())
        };
        changed |= response.labelled_by(label_response.id).changed();
        if ui.button("Edit expression metadata…").clicked() {
            *draft = Some(ExpressionDraft::new(label, value, default));
        }
    });
    if draft.as_ref().is_some_and(|d| d.slot == label) {
        let current = draft.as_mut().expect("draft exists");
        optional_metadata_text(ui, "Expression ID", &mut current.id);
        let was_formal = current.formal;
        egui::ComboBox::from_label("Expression kind")
            .selected_text(if current.formal { "Formal" } else { "Base" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut current.formal, false, "Base");
                ui.selectable_value(&mut current.formal, true, "Formal");
            });
        if current.formal && !was_formal {
            current.explicit_type = true;
            if !current.namespaces.iter().any(|(prefix, uri)| {
                !prefix.is_empty() && uri == "http://www.w3.org/2001/XMLSchema-instance"
            }) {
                let mut prefix = "xsi".to_owned();
                while current
                    .namespaces
                    .iter()
                    .any(|(existing, _)| existing == &prefix)
                {
                    prefix.push('_');
                }
                current
                    .namespaces
                    .push((prefix, "http://www.w3.org/2001/XMLSchema-instance".into()));
            }
        }
        ui.checkbox(&mut current.explicit_type, "Explicit expression type");
        if current.formal {
            optional_metadata_text(ui, "Language", &mut current.language);
            let mut present = current.type_reference.is_some();
            ui.checkbox(&mut present, "Evaluates to type");
            if present {
                let (namespace, local) = current
                    .type_reference
                    .get_or_insert_with(|| (String::new(), String::new()));
                let label = ui.label("Type namespace");
                ui.text_edit_singleline(namespace).labelled_by(label.id);
                let label = ui.label("Type local name");
                ui.text_edit_singleline(local).labelled_by(label.id);
            } else {
                current.type_reference = None;
            }
        }
        ui.label("Namespace bindings");
        let mut remove = None;
        for (index, (prefix, namespace)) in current.namespaces.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                let label = ui.label("Prefix (empty for default)");
                ui.text_edit_singleline(prefix).labelled_by(label.id);
                let label = ui.label("Namespace URI");
                ui.text_edit_singleline(namespace).labelled_by(label.id);
                if ui.button("Remove namespace").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            current.namespaces.remove(index);
        }
        if ui
            .add_enabled(
                current.namespaces.len() < 256,
                egui::Button::new("Add namespace"),
            )
            .clicked()
        {
            current.namespaces.push((String::new(), String::new()));
        }
        if !current.error.is_empty() {
            ui.label(&current.error);
        }
        let mut clear = false;
        if ui.button("Apply expression metadata").clicked() {
            match current.build() {
                Ok(data) => {
                    *value = Expression::Structured(data);
                    changed = true;
                    clear = true;
                }
                Err(error) => current.error = error,
            }
        }
        if ui.button("Cancel expression metadata").clicked() {
            clear = true;
        }
        if clear {
            *draft = None;
        }
    }
    changed
}

fn optional_expression_ui(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<processcraft_core::bpmn_model::Expression>,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
) -> bool {
    optional_expression_ui_with_default(ui, label, value, multiline, draft, ExpressionDefault::Base)
}

fn optional_expression_ui_with_default(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<processcraft_core::bpmn_model::Expression>,
    multiline: bool,
    draft: &mut Option<ExpressionDraft>,
    default: ExpressionDefault,
) -> bool {
    let mut present = value.is_some();
    let checkbox = ui.add_enabled(draft.is_none(), egui::Checkbox::new(&mut present, label));
    let mut changed = checkbox.changed();
    if present {
        let expression = value.get_or_insert_with(|| String::new().into());
        changed |= expression_ui(ui, label, expression, multiline, draft, default);
    } else if value.take().is_some() {
        changed = true;
    }
    changed
}

impl ProcessCraft {
    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui, show_heading: bool) {
        let snap = self.ctl.session.snapshot();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        let process = snap.project.documents.get(self.doc).and_then(|d| {
            processcraft_core::queries::scopes(d)
                .into_iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        });
        let palette = self.theme.palette();
        if show_heading {
            ui.label(egui::RichText::new("Properties").size(17.0).strong());
            ui.add_space(4.0);
            ui.separator();
        }
        egui::ScrollArea::vertical()
            .id_salt("inspector_scroll")
            .show(ui, |side| {
                side.spacing_mut().text_edit_width = (side.available_width() - 8.0).max(40.0);
                if let Some(selected) = self.selected.as_ref() {
                    let kind = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_node(d, selected))
                        .map_or(
                            if self.is_flow {
                                "Connection"
                            } else {
                                "Selection"
                            },
                            |n| node_kind_label(n.kind),
                        );
                    widgets::section_label(side, kind);
                    side.label(
                        egui::RichText::new(if self.draft.is_empty() {
                            "Untitled"
                        } else {
                            &self.draft
                        })
                        .size(20.0),
                    );
                    side.label(
                        egui::RichText::new(format!("ID · {selected}"))
                            .small()
                            .color(palette.muted),
                    )
                    .on_hover_text(selected.to_string());
                    side.add_space(8.0);
                    let annotation = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_node(d, selected))
                        .is_some_and(|n| {
                            n.kind == processcraft_core::model::NodeKind::TextAnnotation
                        });
                    let label = side.label(
                        egui::RichText::new(if annotation {
                            "Annotation text"
                        } else {
                            "Name"
                        })
                        .small()
                        .color(palette.muted),
                    );
                    let response = if annotation {
                        side.add(egui::TextEdit::multiline(&mut self.draft).desired_rows(4))
                    } else {
                        side.text_edit_singleline(&mut self.draft)
                    };
                    if response.labelled_by(label.id).changed() {
                        self.draft_dirty = true;
                    }
                    if self.is_node || self.is_flow {
                        let label = side.label(
                            egui::RichText::new("Documentation")
                                .small()
                                .color(palette.muted),
                        );
                        if side
                            .add(
                                egui::TextEdit::multiline(&mut self.docs)
                                    .hint_text("Describe this activity and its outcome")
                                    .desired_rows(4),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    }
                    let selected_flow = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_flow(d, selected));
                    if self.is_flow
                        && selected_flow
                            .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Sequence)
                    {
                        self.draft_dirty |= optional_expression_ui_with_default(side, "Condition", &mut self.condition, true, &mut self.expression_draft, ExpressionDefault::SequenceFlow);
                    } else if self.is_node {
                        let label =
                            side.label(egui::RichText::new("Owner").small().color(palette.muted));
                        if side
                            .add(
                                egui::TextEdit::singleline(&mut self.owner)
                                    .hint_text("Optional owner"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                        let label =
                            side.label(egui::RichText::new("Contact").small().color(palette.muted));
                        if side
                            .add(
                                egui::TextEdit::singleline(&mut self.contact)
                                    .hint_text("Optional contact details"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    }
                    if selected_flow
                        .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Association)
                    {
                        use processcraft_core::bpmn_model::AssociationDirection as A;
                        let before = self.association_direction;
                        egui::ComboBox::from_label("Association direction")
                            .selected_text(
                                self.association_direction
                                    .map_or("None (default)".into(), |v| format!("{v:?}")),
                            )
                            .show_ui(side, |ui| {
                                ui.selectable_value(
                                    &mut self.association_direction,
                                    None,
                                    "None (default)",
                                );
                                for direction in [A::None, A::One, A::Both] {
                                    ui.selectable_value(
                                        &mut self.association_direction,
                                        Some(direction),
                                        format!("{direction:?}"),
                                    );
                                }
                            });
                        self.draft_dirty |= before != self.association_direction;
                    }
                    if self.is_node {
                        self.bpmn_properties_ui(side, editable);
                        self.dictionary_links_ui(side);
                    }
                    side.add_space(8.0);
                    if side
                        .add_enabled_ui(self.draft_dirty, |ui| {
                            widgets::primary_button(ui, "Apply properties")
                        })
                        .inner
                        .clicked()
                    {
                        self.commit_draft();
                    }
                    if side.button("Element comments…").clicked() {
                        self.open_comments();
                    }
                    side.add_space(8.0);
                    egui::CollapsingHeader::new("Advanced").show(side, |side| {
                        if self.is_flow && side.button("Insert activity on selected flow").clicked()
                        {
                            self.splice_selected();
                        }
                        if side
                            .add_enabled(editable, egui::Button::new("Reroute selected connection"))
                            .clicked()
                        {
                            self.reroute_selected();
                        }
                        side.label("Move selected");
                        side.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(editable, egui::Button::new("Left"))
                                .clicked()
                            {
                                self.move_selected(-10.0, 0.0);
                            }
                            if ui.add_enabled(editable, egui::Button::new("Up")).clicked() {
                                self.move_selected(0.0, -10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Down"))
                                .clicked()
                            {
                                self.move_selected(0.0, 10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Right"))
                                .clicked()
                            {
                                self.move_selected(10.0, 0.0);
                            }
                        });
                        if side
                            .add_enabled(editable, egui::Button::new("Delete selected"))
                            .clicked()
                        {
                            self.delete();
                        }
                    });
                } else {
                    widgets::section_label(side, "Project and document");
                    side.label(
                        egui::RichText::new(
                            "Select an element on the canvas to edit its properties.",
                        )
                        .small()
                        .color(palette.muted),
                    );
                    side.add_space(8.0);
                    let name = side.label(
                        egui::RichText::new("Project name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.project_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    let name = side.label(
                        egui::RichText::new("Document name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.document_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    if let Some(document) = snap.project.documents.get(self.doc) {
                        side.separator();
                        widgets::section_label(side, "BPMN definitions");
                        side.label(format!("ID · {}", document.definitions_id));
                        let legacy_import = document.definitions.is_none() && document.provenance.is_some();
                        if legacy_import {
                            side.label("Metadata editing is unavailable until the original target namespace is recovered.");
                        } else if document.definitions.is_none() {
                            side.label("The generated target namespace is used until changed.");
                        }
                        side.add_enabled_ui(editable && !legacy_import, |side| {
                            let label = side.label("Target namespace");
                            if side.text_edit_singleline(&mut self.definitions_draft.target_namespace)
                                .labelled_by(label.id).changed() {
                                self.definitions_dirty = true;
                            }
                            fn optional_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
                                let mut present = value.is_some();
                                let checkbox = ui.checkbox(&mut present, label);
                                let mut changed = checkbox.changed();
                                if present {
                                    let text = value.get_or_insert_with(String::new);
                                    changed |= ui.text_edit_singleline(text).labelled_by(checkbox.id).changed();
                                } else if value.take().is_some() {
                                    changed = true;
                                }
                                changed
                            }
                            self.definitions_dirty |= optional_field(side, "Name", &mut self.definitions_draft.name);
                            self.definitions_dirty |= optional_field(side, "Expression language", &mut self.definitions_draft.expression_language);
                            self.definitions_dirty |= optional_field(side, "Type language", &mut self.definitions_draft.type_language);
                            self.definitions_dirty |= optional_field(side, "Exporter", &mut self.definitions_draft.exporter);
                            self.definitions_dirty |= optional_field(side, "Exporter version", &mut self.definitions_draft.exporter_version);
                            egui::CollapsingHeader::new("Imports").show(side, |side| {
                                side.label("Import declarations are stored as model data.");
                                let mut remove = None;
                                for (index, import) in self.definitions_draft.imports.iter_mut().enumerate() {
                                    side.push_id(index, |side| {
                                        side.separator();
                                        side.label(format!("Import {}", index + 1));
                                        let label = side.label("Namespace");
                                        self.definitions_dirty |= side.text_edit_singleline(&mut import.namespace).labelled_by(label.id).changed();
                                        let label = side.label("Location");
                                        self.definitions_dirty |= side.text_edit_singleline(&mut import.location).labelled_by(label.id).changed();
                                        let label = side.label("Import type");
                                        self.definitions_dirty |= side.text_edit_singleline(&mut import.import_type).labelled_by(label.id).changed();
                                        if side.button("Remove import").clicked() { remove = Some(index); }
                                    });
                                }
                                if let Some(index) = remove {
                                    self.definitions_draft.imports.remove(index);
                                    self.definitions_dirty = true;
                                }
                                if side.add_enabled(self.definitions_draft.imports.len() < 256, egui::Button::new("Add import")).clicked() {
                                    self.definitions_draft.imports.push(processcraft_core::model::DefinitionsImport {
                                        namespace: String::new(), location: String::new(), import_type: String::new(),
                                    });
                                    self.definitions_dirty = true;
                                }
                            });
                        });
                        self.draft_dirty |= self.definitions_dirty;
                    }
                    if side
                        .add_enabled_ui(self.draft_dirty, |ui| {
                            widgets::primary_button(ui, "Apply properties")
                        })
                        .inner
                        .clicked()
                    {
                        self.commit_draft();
                    }
                }
                if let Some(p) = process {
                    side.separator();
                    egui::CollapsingHeader::new("Connections and lanes").show(side, |side| {
                        let lane_names: Vec<_> = p.lanes.iter().map(|l| l.name.clone()).collect();
                        egui::ComboBox::from_label("Assign selected node to lane")
                            .selected_text(if self.lane == 0 {
                                "No lane"
                            } else {
                                lane_names
                                    .get(self.lane - 1)
                                    .map_or("No lane", String::as_str)
                            })
                            .show_ui(side, |ui| {
                                ui.selectable_value(&mut self.lane, 0, "No lane");
                                for (i, n) in lane_names.iter().enumerate() {
                                    ui.selectable_value(&mut self.lane, i + 1, n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Apply lane"))
                            .clicked()
                        {
                            self.assign_lane();
                        }
                        side.strong("Connect nodes");
                        egui::ComboBox::from_label("Connection type")
                            .selected_text(format!("{:?}", self.connection_kind))
                            .show_ui(side, |ui| {
                                ui.selectable_value(
                                    &mut self.connection_kind,
                                    processcraft_core::model::FlowKind::Sequence,
                                    "Sequence flow",
                                );
                                ui.selectable_value(
                                    &mut self.connection_kind,
                                    processcraft_core::model::FlowKind::Association,
                                    "Association",
                                );
                            });
                        let names: Vec<_> = p.nodes.iter().map(|n| n.name.clone()).collect();
                        egui::ComboBox::from_label("Source")
                            .selected_text(names.get(self.source).map_or("Choose", String::as_str))
                            .show_ui(side, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.source, i, n);
                                }
                            });
                        egui::ComboBox::from_label("Target")
                            .selected_text(names.get(self.target).map_or("Choose", String::as_str))
                            .show_ui(side, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.target, i, n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Create connection"))
                            .clicked()
                        {
                            self.connect();
                        }
                    });
                }
            });
        self.draft_dirty |= self.expression_draft.is_some();
    }
}

impl ProcessCraft {
    fn bpmn_properties_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        use processcraft_core::bpmn_model::{EventDefinition as E, NodeDetails as D, TimerKind};
        use processcraft_core::{model::NodeKind as K, queries};
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let Some(node) = self
            .selected
            .as_ref()
            .and_then(|id| queries::find_node(document, id))
        else {
            return;
        };
        if let Some(D::SubProcess { .. }) = &node.details {
            ui.separator();
            if ui.button("Work inside subprocess").clicked() && self.prepare_navigation() {
                self.process_id = Some(node.id.clone());
                self.select(None);
            }
        }
        let event = matches!(
            node.kind,
            K::StartEvent
                | K::EndEvent
                | K::IntermediateCatchEvent
                | K::IntermediateThrowEvent
                | K::BoundaryEvent
        );
        if node.kind.is_activity() {
            self.activity_properties_ui(ui, node, document, editable);
            ui.add_enabled_ui(editable, |ui| {
                ui.collapsing("Loop characteristics", |ui| {
                    use processcraft_core::bpmn_model::LoopCharacteristics as L;
                    let mut value = self.bpmn_loop.clone();
                    let mut tag = match &value {
                        None => "None",
                        Some(L::Standard { .. }) => "Standard loop",
                        Some(L::MultiInstance {
                            is_sequential: true,
                            ..
                        }) => "Sequential instances",
                        Some(L::MultiInstance { .. }) => "Parallel instances",
                    };
                    let old_tag = tag;
                    ui.add_enabled_ui(self.expression_draft.is_none(), |ui| {
                        egui::ComboBox::from_label("Repetition")
                            .selected_text(tag)
                            .show_ui(ui, |ui| {
                                for option in [
                                    "None",
                                    "Standard loop",
                                    "Sequential instances",
                                    "Parallel instances",
                                ] {
                                    ui.selectable_value(&mut tag, option, option);
                                }
                            });
                    });
                    if tag != old_tag {
                        value = match tag {
                            "None" => None,
                            "Standard loop" => Some(L::Standard {
                                test_before: false,
                                condition: None,
                                maximum: None,
                            }),
                            _ => Some(L::MultiInstance {
                                is_sequential: tag == "Sequential instances",
                                cardinality: None,
                                completion_condition: None,
                            }),
                        };
                    }
                    let text = |ui: &mut egui::Ui, label: &str, value: &mut Option<String>| {
                        let mut current = value.clone().unwrap_or_default();
                        let label = ui.label(label);
                        if ui
                            .text_edit_singleline(&mut current)
                            .labelled_by(label.id)
                            .changed()
                        {
                            *value = (!current.is_empty()).then_some(current);
                        }
                    };
                    match &mut value {
                        Some(L::Standard {
                            test_before,
                            condition,
                            maximum,
                        }) => {
                            ui.checkbox(test_before, "Test before first iteration");
                            optional_expression_ui(
                                ui,
                                "Loop condition",
                                condition,
                                false,
                                &mut self.expression_draft,
                            );
                            text(ui, "Maximum iterations", maximum);
                        }
                        Some(L::MultiInstance {
                            cardinality,
                            completion_condition,
                            ..
                        }) => {
                            optional_expression_ui(
                                ui,
                                "Cardinality expression",
                                cardinality,
                                false,
                                &mut self.expression_draft,
                            );
                            optional_expression_ui(
                                ui,
                                "Completion condition",
                                completion_condition,
                                false,
                                &mut self.expression_draft,
                            );
                        }
                        None => {}
                    }
                    if value != self.bpmn_loop {
                        self.bpmn_loop = value;
                        self.draft_dirty = true;
                    }
                });
            });
        }
        if matches!(
            node.kind,
            K::ExclusiveGateway
                | K::InclusiveGateway
                | K::ParallelGateway
                | K::ComplexGateway
                | K::EventBasedGateway
        ) {
            ui.add_enabled_ui(editable, |ui| {
                use processcraft_core::bpmn_model::{EventGatewayType as T, GatewayDirection as G};
                let mut value = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if let D::Gateway {
                    default_flow,
                    direction,
                    instantiate,
                    event_gateway_type,
                    activation_condition,
                } = &mut value
                {
                    ui.separator();
                    ui.strong("Gateway behavior");
                    egui::ComboBox::from_label("Direction")
                        .selected_text(direction.map_or("Unspecified".into(), |v| format!("{v:?}")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(direction, None, "Unspecified (omitted)");
                            for v in [G::Unspecified, G::Converging, G::Diverging, G::Mixed] {
                                ui.selectable_value(direction, Some(v), format!("{v:?}"));
                            }
                        });
                    if matches!(
                        node.kind,
                        K::ExclusiveGateway | K::InclusiveGateway | K::ComplexGateway
                    ) {
                        let outgoing: Vec<_> = queries::scopes(document)
                            .into_iter()
                            .flat_map(|s| s.flows)
                            .filter(|f| {
                                f.source == node.id
                                    && f.kind == processcraft_core::model::FlowKind::Sequence
                            })
                            .collect();
                        egui::ComboBox::from_label("Default outgoing flow")
                            .selected_text(default_flow.as_ref().map_or("None".into(), |id| {
                                queries::element_name(document, id)
                                    .filter(|s| !s.is_empty())
                                    .map_or_else(|| id.to_string(), str::to_owned)
                            }))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(default_flow, None, "None");
                                for flow in outgoing {
                                    ui.selectable_value(
                                        default_flow,
                                        Some(flow.id.clone()),
                                        if flow.name.is_empty() {
                                            flow.id.to_string()
                                        } else {
                                            flow.name.clone()
                                        },
                                    );
                                }
                            });
                    }
                    if node.kind == K::EventBasedGateway {
                        let mut starts = instantiate.unwrap_or(false);
                        if ui.checkbox(&mut starts, "Instantiate process").changed() {
                            *instantiate = Some(starts);
                        }
                        egui::ComboBox::from_label("Event gateway type")
                            .selected_text(
                                event_gateway_type
                                    .map_or("Exclusive (default)".into(), |v| format!("{v:?}")),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    event_gateway_type,
                                    None,
                                    "Exclusive (default)",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Exclusive),
                                    "Exclusive",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Parallel),
                                    "Parallel",
                                );
                            });
                    }
                    if node.kind == K::ComplexGateway {
                        optional_expression_ui(
                            ui,
                            "Activation condition",
                            activation_condition,
                            true,
                            &mut self.expression_draft,
                        );
                    }
                }
                let original = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if value != original {
                    self.bpmn_details = Some(value);
                    self.draft_dirty = true;
                }
            });
        }
        if !event {
            return;
        }
        ui.separator();
        ui.strong("Event definition");
        ui.add_enabled_ui(editable, |ui| {
            // Multiple and referenced definitions stay intact until an explicit multi-definition editor exists.
            if matches!(&self.bpmn_details, Some(D::Event { definitions, .. }) if definitions.len() > 1) {
                ui.label("Multiple definitions retained. Single-definition editing is unavailable.");
                return;
            }
            let (definitions, attached_to, cancel_activity) = match &self.bpmn_details {
                Some(D::Event { definitions, attached_to, cancel_activity }) => (definitions.clone(), attached_to.clone(), *cancel_activity),
                Some(D::BoundaryEvent { attached_to, cancel_activity }) => (vec![], Some(attached_to.clone()), Some(*cancel_activity)),
                _ => (vec![], None, None),
            };
            let mut tag = match definitions.first() {
                None => "None", Some(E::Timer{..}) => "Timer", Some(E::Conditional{..}) => "Conditional",
                Some(E::Terminate{..}) => "Terminate", Some(E::Message{..}) => "Message", Some(E::Signal{..}) => "Signal",
                Some(E::Error{..}) => "Error", Some(E::Escalation{..}) => "Escalation",
                Some(E::Link{..}) => "Link", Some(E::Cancel{..}) => "Cancel", Some(E::Compensate{..}) => "Compensate",
            };
            let original_tag = tag;
            let scope = queries::scopes(document).into_iter().find(|scope| scope.nodes.iter().any(|candidate| candidate.id == node.id));
            let parent = scope.as_ref().and_then(|scope| queries::find_node(document, scope.id));
            let transaction_scope = parent.is_some_and(|parent| parent.kind == K::Transaction);
            let event_subprocess = parent.is_some_and(|parent| parent.kind == K::SubProcess && parent.activity.as_ref().is_some_and(|activity| activity.triggered_by_event == Some(true)));
            let transaction_attachment = attached_to.as_ref().and_then(|id| queries::find_node(document, id)).is_some_and(|target| target.kind == K::Transaction);
            ui.add_enabled_ui(self.expression_draft.is_none(), |ui| { egui::ComboBox::from_label("Definition type").selected_text(tag).show_ui(ui, |ui| {
                for option in ["None", "Timer", "Conditional", "Terminate", "Message", "Signal", "Error", "Escalation", "Link", "Cancel", "Compensate"] {
                    let allowed = match option {
                        "Timer" | "Conditional" => matches!(node.kind,K::StartEvent | K::IntermediateCatchEvent | K::BoundaryEvent),
                        "Terminate" => node.kind == K::EndEvent,
                        "Error" => matches!(node.kind,K::EndEvent | K::BoundaryEvent),
                        "Link" => matches!(node.kind, K::IntermediateCatchEvent | K::IntermediateThrowEvent),
                        "Cancel" => (node.kind == K::BoundaryEvent && transaction_attachment && cancel_activity.unwrap_or(true)) || (node.kind == K::EndEvent && transaction_scope),
                        "Compensate" => matches!(node.kind, K::BoundaryEvent | K::IntermediateThrowEvent | K::EndEvent) || (node.kind == K::StartEvent && event_subprocess),
                        "None" => node.kind != K::BoundaryEvent && node.kind != K::IntermediateCatchEvent,
                        _ => true,
                    };
                    if allowed { ui.selectable_value(&mut tag, option, option); }
                }
            }); });
            let mut definition = if tag == original_tag { definitions.first().cloned() } else {
                match tag {
                    "Timer" => Some(E::Timer { id: None, kind: TimerKind::Duration, expression: "PT1M".into() }),
                    "Conditional" => Some(E::Conditional { id: None, expression: String::new().into() }),
                    "Terminate" => Some(E::Terminate { id: None }),
                    "Message" => Some(E::Message { id: None, message_ref: None, operation_ref: None }),
                    "Signal" => Some(E::Signal { id: None, signal_ref: None }),
                    "Error" => Some(E::Error { id: None, error_ref: None }),
                    "Escalation" => Some(E::Escalation { id: None, escalation_ref: None }),
                    "Link" => Some(E::Link { id: Some(crate::controller::id("LinkDefinition")), name: String::new(), sources: Vec::new(), target: None }),
                    "Cancel" => Some(E::Cancel { id: None }),
                    "Compensate" => Some(E::Compensate { id: None, wait_for_completion: None, activity_ref: None }), _ => None,
                }
            };
            let mut changed = tag != original_tag;
            match &mut definition {
                Some(E::Timer { kind, expression, .. }) => {
                    let old = *kind;
                    egui::ComboBox::from_label("Timer expression").selected_text(format!("{kind:?}")).show_ui(ui, |ui| {
                        ui.selectable_value(kind, TimerKind::Date, "Date");
                        ui.selectable_value(kind, TimerKind::Duration, "Duration");
                        ui.selectable_value(kind, TimerKind::Cycle, "Cycle");
                    });
                    changed |= old != *kind;
                    changed |= expression_ui(ui, "Expression", expression, false, &mut self.expression_draft, ExpressionDefault::Base);
                }
                Some(E::Conditional { expression, .. }) => {
                    changed |= expression_ui(ui, "Condition expression", expression, true, &mut self.expression_draft, ExpressionDefault::Base);
                }
                Some(E::Link { id, name, sources, target }) => {
                    if let Some(id) = id { ui.label(format!("Definition ID: {id}")); }
                    let label = ui.label("Link name");
                    changed |= ui.text_edit_singleline(name).labelled_by(label.id).changed();
                    let mut links: Vec<_> = scope.as_ref().into_iter().flat_map(|scope| scope.nodes.iter()).filter(|other| {
                        other.id != node.id && if node.kind == K::IntermediateCatchEvent { other.kind == K::IntermediateThrowEvent } else { other.kind == K::IntermediateCatchEvent }
                    }).flat_map(|other| match &other.details {
                        Some(D::Event { definitions, .. }) => definitions.as_slice(), _ => &[],
                    }).filter_map(|event| match event {
                        E::Link { id: Some(id), name: other_name, .. } if other_name == name => Some(id.clone()), _ => None,
                    }).collect();
                    if node.kind == K::IntermediateCatchEvent {
                        ui.label("Source link definitions");
                        for existing in sources.iter() { if !links.contains(existing) { links.push(existing.clone()); } }
                        for candidate in links {
                            let mut present = sources.contains(&candidate);
                            if ui.checkbox(&mut present, candidate.to_string()).changed() {
                                if present { sources.push(candidate); } else { sources.retain(|id| id != &candidate); }
                                changed = true;
                            }
                        }
                    } else {
                        let previous = target.clone();
                        egui::ComboBox::from_label("Target link definition").selected_text(target.as_ref().map_or("None".into(), ToString::to_string)).show_ui(ui, |ui| {
                            ui.selectable_value(target, None, "None");
                            for candidate in links { ui.selectable_value(target, Some(candidate.clone()), candidate.to_string()); }
                        });
                        changed |= *target != previous;
                    }
                }
                Some(E::Compensate { wait_for_completion, activity_ref, .. }) => {
                    let previous = *wait_for_completion;
                    egui::ComboBox::from_label("Wait for completion").selected_text(match wait_for_completion { None => "Default", Some(true) => "Yes", Some(false) => "No" }).show_ui(ui, |ui| {
                        ui.selectable_value(wait_for_completion, None, "Default");
                        ui.selectable_value(wait_for_completion, Some(true), "Yes");
                        ui.selectable_value(wait_for_completion, Some(false), "No");
                    });
                    changed |= *wait_for_completion != previous;
                    let previous = activity_ref.clone();
                    egui::ComboBox::from_label("Activity reference").selected_text(activity_ref.as_ref().map_or("None".into(), ToString::to_string)).show_ui(ui, |ui| {
                        ui.selectable_value(activity_ref, None, "None");
                        for activity in scope.as_ref().into_iter().flat_map(|scope| scope.nodes.iter()).filter(|candidate| candidate.kind.is_activity()) {
                            ui.selectable_value(activity_ref, Some(activity.id.clone()), format!("{} · {}", activity.name, activity.id));
                        }
                    });
                    changed |= *activity_ref != previous;
                }
                _ => {}
            }
            let mut cancel_activity = cancel_activity;
            if node.kind == K::BoundaryEvent {
                if let Some(target) = &attached_to { ui.label(format!("Attached to: {}", queries::element_name(document, target).unwrap_or("Unnamed activity"))); }
                let mut interrupt = cancel_activity.unwrap_or(true);
                if ui.add_enabled(!matches!(definition, Some(E::Cancel { .. })), egui::Checkbox::new(&mut interrupt, "Interrupt activity")).changed() { cancel_activity = Some(interrupt); changed = true; }
            }
            if changed {
                self.bpmn_details = if definition.is_none() && attached_to.is_none() { None } else {
                    Some(D::Event { definitions: definition.into_iter().collect(), attached_to, cancel_activity })
                };
                self.draft_dirty = true;
            }
        });
    }
}

impl ProcessCraft {
    fn activity_properties_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &processcraft_core::model::BpmnDocument,
        editable: bool,
    ) {
        use processcraft_core::{
            bpmn_activity::AdHocOrdering,
            model::{FlowKind, NodeKind as K},
            queries,
        };
        ui.add_enabled_ui(editable, |ui| {
            ui.collapsing("Activity behavior", |ui| {
                let mut value = self.bpmn_activity.clone().unwrap_or_default();
                let checkbox = |ui: &mut egui::Ui, label: &str, value: &mut Option<bool>, default: bool| {
                    let mut checked = value.unwrap_or(default);
                    if ui.checkbox(&mut checked, label).changed() { *value = Some(checked); }
                };
                let text = |ui: &mut egui::Ui, label: &str, value: &mut Option<String>, multiline: bool| {
                    let mut draft = value.clone().unwrap_or_default();
                    let label = ui.label(label);
                    let response = if multiline { ui.add(egui::TextEdit::multiline(&mut draft).desired_rows(4)) } else { ui.text_edit_singleline(&mut draft) };
                    if response.labelled_by(label.id).changed() { *value = (!draft.is_empty()).then_some(draft); }
                };
                checkbox(ui, "For compensation", &mut value.is_for_compensation, false);
                text(ui, "Start quantity (default 1)", &mut value.start_quantity, false);
                text(ui, "Completion quantity (default 1)", &mut value.completion_quantity, false);
                let outgoing: Vec<_> = queries::scopes(document).into_iter().flat_map(|s|s.flows).filter(|f|f.kind == FlowKind::Sequence && f.source == node.id).collect();
                egui::ComboBox::from_label("Activity default flow").selected_text(value.default_flow.as_ref().map_or("None".into(), |id| queries::element_name(document,id).filter(|s|!s.is_empty()).map_or_else(||id.to_string(),str::to_owned))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut value.default_flow,None,"None");
                    for flow in outgoing { ui.selectable_value(&mut value.default_flow,Some(flow.id.clone()), if flow.name.is_empty() {flow.id.to_string()} else {flow.name.clone()}); }
                });
                if node.kind == K::SubProcess { checkbox(ui,"Triggered by event",&mut value.triggered_by_event,false); }
                if node.kind == K::AdHocSubProcess {
                    egui::ComboBox::from_label("Ad-hoc ordering").selected_text(value.ad_hoc_ordering.map_or("Parallel (default)".into(), |v|format!("{v:?}"))).show_ui(ui, |ui| {
                        ui.selectable_value(&mut value.ad_hoc_ordering,None,"Parallel (default)");
                        ui.selectable_value(&mut value.ad_hoc_ordering,Some(AdHocOrdering::Parallel),"Parallel");
                        ui.selectable_value(&mut value.ad_hoc_ordering,Some(AdHocOrdering::Sequential),"Sequential");
                    });
                    checkbox(ui,"Cancel remaining instances",&mut value.cancel_remaining_instances,true);
                    optional_expression_ui(ui,"Ad-hoc completion condition",&mut value.completion_condition,true, &mut self.expression_draft);
                }
                if node.kind == K::Transaction { text(ui,"Transaction method (default ##Compensate)",&mut value.transaction_method,false); }
                if matches!(node.kind,K::ServiceTask|K::SendTask|K::ReceiveTask|K::UserTask|K::BusinessRuleTask) { text(ui,"Implementation",&mut value.implementation,false); }
                if node.kind == K::ReceiveTask { checkbox(ui,"Instantiate process",&mut value.instantiate,false); }
                if node.kind == K::ScriptTask {
                    ui.label("Script source is stored as model data and is never executed by ProcessCraft.");
                    text(ui,"Script format",&mut value.script_format,false);
                    text(ui,"Script source",&mut value.script,true);
                }
                let original = self.bpmn_activity.clone().unwrap_or_default();
                if value != original { self.bpmn_activity = Some(value); self.draft_dirty = true; }
            });
        });
    }
}

#[cfg(test)]
mod expression_draft_tests {
    use super::*;
    use processcraft_core::model::NodeKind;

    #[test]
    fn legacy_promotion_preserves_flow_and_event_expression_defaults() {
        use processcraft_core::bpmn_model::ExpressionKind;
        let flow = ExpressionDraft::new("Condition", &"x".into(), ExpressionDefault::SequenceFlow)
            .build()
            .unwrap();
        assert!(matches!(
            flow.kind,
            ExpressionKind::Formal {
                explicit_type: true,
                ..
            }
        ));
        assert!(
            flow.namespaces
                .iter()
                .any(|(p, n)| p.is_empty() && n == "http://www.omg.org/spec/BPMN/20100524/MODEL")
        );
        assert!(
            flow.namespaces
                .iter()
                .any(|(p, n)| p == "xsi" && n == "http://www.w3.org/2001/XMLSchema-instance")
        );
        let event =
            ExpressionDraft::new("Condition expression", &"x".into(), ExpressionDefault::Base)
                .build()
                .unwrap();
        assert!(matches!(
            event.kind,
            ExpressionKind::Base {
                explicit_type: false
            }
        ));
    }

    #[test]
    fn expression_metadata_draft_round_trips_and_validates() {
        use processcraft_core::bpmn_model::{Expression, ExpressionData, ExpressionKind, QNameRef};
        let data = ExpressionData {
            id: Some("ExpressionId".parse().unwrap()),
            kind: ExpressionKind::Formal {
                explicit_type: true,
                language: Some(String::new()),
                evaluates_to_type_ref: Some(QNameRef {
                    namespace: "urn:type".into(),
                    local: "Thing".parse().unwrap(),
                }),
            },
            text: "body".into(),
            namespaces: vec![("t".into(), "urn:type".into())],
        };
        let mut draft = ExpressionDraft::new(
            "Condition",
            &Expression::Structured(data.clone()),
            ExpressionDefault::Base,
        );
        assert_eq!(draft.build().unwrap(), data);
        draft.type_reference.as_mut().unwrap().1 = "invalid name".into();
        assert!(draft.build().is_err());
        draft.formal = false;
        assert!(draft.build().is_ok());
    }

    #[test]
    fn invalid_expression_metadata_blocks_navigation_and_history() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::IntermediateCatchEvent);
        let selected = app.selected.clone();
        let mut metadata =
            ExpressionDraft::new("Expression", &"PT1M".into(), ExpressionDefault::Base);
        metadata.id = Some("invalid id".into());
        assert!(metadata.build().is_err());
        app.expression_draft = Some(metadata);
        app.draft_dirty = true;
        assert!(!app.commit_draft());
        app.select(None);
        app.history(processcraft_core::commands::Request::Undo);
        assert_eq!(app.selected, selected);
        assert_eq!(
            app.expression_draft.as_ref().unwrap().id.as_deref(),
            Some("invalid id")
        );
        assert!(app.has_drafts());
    }
}
