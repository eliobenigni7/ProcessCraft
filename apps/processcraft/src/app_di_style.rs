use super::catalog::{optional_bool, optional_text};
use super::*;
use processcraft_core::model::{DiFont, Diagram, LabelStyle};
pub(super) struct StyleDraft {
    diagram: ElementSelector,
    documentation: Option<String>,
    resolution: Option<String>,
    styles: Vec<StyleRow>,
    plane_metadata: super::di_metadata::DiMetadataDraft,
    plane_order: Option<Vec<ElementSelector>>,
    error: String,
}
struct StyleRow {
    value: LabelStyle,
    size: Option<String>,
}
impl StyleDraft {
    fn new(diagram: &Diagram) -> Self {
        Self {
            diagram: diagram.id.selector(),
            documentation: diagram.documentation.clone(),
            resolution: diagram.resolution.map(|v| v.to_string()),
            plane_metadata: super::di_metadata::DiMetadataDraft::new(&diagram.plane_metadata),
            plane_order: diagram.plane_order.clone(),
            styles: diagram
                .label_styles
                .iter()
                .map(|s| StyleRow {
                    value: s.clone(),
                    size: s.font.as_ref().and_then(|f| f.size.map(|v| v.to_string())),
                })
                .collect(),
            error: String::new(),
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, diagram: &Diagram, document: &BpmnDocument) {
        ui.collapsing("Plane diagram metadata", |ui| self.plane_metadata.ui(ui));
        ui.collapsing("Representation order", |ui| {
            let mut explicit = self.plane_order.is_some();
            ui.checkbox(&mut explicit, "Explicit order");
            if explicit {
                let order = self.plane_order.get_or_insert_with(|| {
                    diagram
                        .shapes
                        .iter()
                        .map(|s| s.id.selector())
                        .chain(diagram.edges.iter().map(|e| e.id.selector()))
                        .collect()
                });
                let mut up = None;
                for (i, id) in order.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        let label = diagram
                            .shapes
                            .iter()
                            .enumerate()
                            .find(|(_, s)| s.id == *id)
                            .map(|(j, s)| {
                                super::di::representation_label(document, &s.element, "Shape", j)
                            })
                            .or_else(|| {
                                diagram
                                    .edges
                                    .iter()
                                    .enumerate()
                                    .find(|(_, e)| e.id == *id)
                                    .map(|(j, e)| {
                                        super::di::representation_label(
                                            document, &e.element, "Edge", j,
                                        )
                                    })
                            })
                            .unwrap_or_else(|| "Unavailable representation".into());
                        ui.horizontal(|ui| {
                            ui.label(label);
                            if ui
                                .add_enabled(i > 0, egui::Button::new("Move up"))
                                .clicked()
                            {
                                up = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = up {
                    order.swap(i, i - 1);
                }
            } else {
                self.plane_order = None;
            }
        });
        optional_text(ui, "Diagram documentation", &mut self.documentation);
        optional_text(ui, "Resolution", &mut self.resolution);
        let mut remove = None;
        let mut up = None;
        for (i, row) in self.styles.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.strong(format!("Label style {}", i + 1));
                let mut present = row.value.font.is_some();
                ui.checkbox(&mut present, "Font present");
                if present {
                    let font = row.value.font.get_or_insert_with(DiFont::default);
                    optional_text(ui, "Font name", &mut font.name);
                    optional_text(ui, "Font size", &mut row.size);
                    optional_bool(ui, "Bold", &mut font.is_bold);
                    optional_bool(ui, "Italic", &mut font.is_italic);
                    optional_bool(ui, "Underline", &mut font.is_underline);
                    optional_bool(ui, "Strike through", &mut font.is_strike_through);
                } else {
                    row.value.font = None;
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("Move style up"))
                    .clicked()
                {
                    up = Some(i);
                }
                if ui.button("Remove style").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.styles.remove(i);
        } else if let Some(i) = up {
            self.styles.swap(i, i - 1);
        }
        if ui
            .add_enabled(
                self.styles.len() < 256,
                egui::Button::new("Add label style"),
            )
            .clicked()
        {
            self.styles.push(StyleRow {
                value: LabelStyle {
                    id: None,
                    font: None,
                },
                size: None,
            });
        }
    }
    fn changes(
        &self,
        document_id: processcraft_core::ids::DocumentId,
    ) -> Result<Vec<Mutation>, String> {
        fn number(
            value: &Option<String>,
            label: &str,
            allow_zero: bool,
        ) -> Result<Option<f64>, String> {
            value
                .as_ref()
                .map(|s| {
                    s.parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite() && (*n > 0. || (allow_zero && *n == 0.)))
                        .ok_or_else(|| {
                            format!(
                                "{label} must be {} number.",
                                if allow_zero {
                                    "a non-negative"
                                } else {
                                    "a positive"
                                }
                            )
                        })
                })
                .transpose()
        }
        let mut styles = Vec::new();
        for row in &self.styles {
            let mut style = row.value.clone();
            if let Some(font) = &mut style.font {
                font.size = number(&row.size, "Font size", true)?;
            }
            styles.push(style);
        }
        Ok(vec![
            Mutation::SetDiagramMetadata {
                document_id,
                diagram_id: self.diagram.clone(),
                documentation: self.documentation.clone(),
                resolution: number(&self.resolution, "Resolution", false)?,
            },
            Mutation::SetDiagramLabelStyles {
                document_id,
                diagram_id: self.diagram.clone(),
                label_styles: styles,
            },
            Mutation::SetDiagramPlaneMetadata {
                document_id,
                diagram_id: self.diagram.clone(),
                metadata: self.plane_metadata.build()?,
            },
            Mutation::SetDiagramPlaneOrder {
                document_id,
                diagram_id: self.diagram.clone(),
                order: self.plane_order.clone(),
            },
        ])
    }
}
impl ProcessCraft {
    pub(super) fn diagram_styles_ui(
        &mut self,
        ui: &mut egui::Ui,
        diagram: &Diagram,
        editable: bool,
    ) {
        if self.style_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit diagram fonts and details…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.style_draft = Some(StyleDraft::new(diagram));
            }
            return;
        }
        let draft = self.style_draft.as_mut().unwrap();
        if draft.diagram != diagram.id.selector() {
            return;
        }
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        draft.ui(ui, diagram, document);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply diagram fonts and details").clicked() {
            let _ = self.apply_style_draft();
        }
        if ui.button("Cancel diagram fonts and details").clicked() {
            self.style_draft = None;
        }
    }
    pub(super) fn apply_style_draft(&mut self) -> Result<(), String> {
        self.apply_typed_draft(
            |app| &mut app.style_draft,
            |draft, document| {
                draft
                    .changes(document.id)
                    .inspect_err(|e| draft.error = e.clone())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plane_order_draft_applies_and_undo_redo_keeps_representation_ids() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::Task);
        app.select(None);
        app.add(NodeKind::Task);
        let snap = app.ctl.session.snapshot();
        let document = &snap.project.documents[0];
        let diagram = &document.diagrams[0];
        let document_id = document.id;
        let mut draft = StyleDraft::new(diagram);
        let order = diagram
            .shapes
            .iter()
            .rev()
            .map(|shape| shape.id.selector())
            .collect::<Vec<_>>();
        draft.plane_order = Some(order.clone());
        assert!(app.ctl.mutate(draft.changes(document_id).unwrap()));
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].plane_order,
            Some(order.clone())
        );
        app.history(Request::Undo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].plane_order,
            None
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].plane_order,
            Some(order)
        );
    }
    #[test]
    fn font_size_zero_is_preserved() {
        let app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let mut draft = StyleDraft::new(&snapshot.project.documents[0].diagrams[0]);
        draft.styles.push(StyleRow {
            value: LabelStyle {
                id: None,
                font: Some(DiFont::default()),
            },
            size: Some("0".into()),
        });
        let changes = draft.changes(snapshot.project.documents[0].id).unwrap();
        assert!(
            matches!(&changes[1],Mutation::SetDiagramLabelStyles{label_styles,..} if label_styles[0].font.as_ref().unwrap().size==Some(0.0))
        );
    }
}
