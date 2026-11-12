use super::catalog::{QNameDraft, optional_qname, optional_text, q_build};
use super::event::{ReferenceDraft, build_ref, local_choices, ref_ui};
use super::inspector::{ExpressionDefault, optional_expression_ui_with_default};
use super::*;
use processcraft_core::bpmn_activity::{CorrelationProperty, CorrelationRetrieval};

struct RetrievalDraft {
    value: CorrelationRetrieval,
    message: Option<ReferenceDraft>,
}
struct CorrelationDraft {
    value: CorrelationProperty,
    item_type: Option<QNameDraft>,
    retrievals: Vec<RetrievalDraft>,
}
pub(super) struct CorrelationCatalogDraft {
    values: Vec<CorrelationDraft>,
    error: String,
}
impl CorrelationCatalogDraft {
    fn new(values: &[CorrelationProperty]) -> Self {
        Self {
            values: values
                .iter()
                .map(|v| CorrelationDraft {
                    value: v.clone(),
                    item_type: v.item_type.as_ref().map(QNameDraft::from),
                    retrievals: v
                        .retrieval_expressions
                        .iter()
                        .map(|r| RetrievalDraft {
                            value: r.clone(),
                            message: r.message_ref.as_ref().map(ReferenceDraft::new),
                        })
                        .collect(),
                })
                .collect(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Vec<CorrelationProperty>, String> {
        self.values
            .iter()
            .map(|d| {
                let mut value = d.value.clone();
                value.item_type = q_build(&d.item_type)?;
                value.retrieval_expressions = d
                    .retrievals
                    .iter()
                    .map(|r| {
                        let mut value = r.value.clone();
                        value.message_ref = build_ref(&r.message)?;
                        Ok(value)
                    })
                    .collect::<Result<_, String>>()?;
                Ok(value)
            })
            .collect()
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        expression_draft: &mut Option<super::inspector::ExpressionDraft>,
    ) {
        let messages = local_choices(document, "Message", None);
        let mut remove = None;
        let mut up = None;
        for (i, d) in self.values.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.strong(format!("Correlation property {}", i + 1));
                optional_text(ui, "Name", &mut d.value.name);
                optional_qname(ui, "Item type", &mut d.item_type);
                let mut remove_retrieval = None;
                let mut up_retrieval = None;
                for (j, r) in d.retrievals.iter_mut().enumerate() {
                    ui.push_id(j, |ui| {
                        ui.label(format!("Message retrieval {}", j + 1));
                        ref_ui(ui, "Message", &mut r.message, &messages);
                        optional_expression_ui_with_default(
                            ui,
                            &format!("Message path {}.{}", i + 1, j + 1),
                            &mut r.value.message_path,
                            true,
                            expression_draft,
                            ExpressionDefault::ImplicitFormal,
                        );
                        ui.add_enabled_ui(expression_draft.is_none(), |ui| {
                            if ui
                                .add_enabled(j > 0, egui::Button::new("Move retrieval up"))
                                .clicked()
                            {
                                up_retrieval = Some(j);
                            }
                            if ui.button("Remove retrieval").clicked() {
                                remove_retrieval = Some(j);
                            }
                        });
                    });
                }
                if let Some(j) = remove_retrieval {
                    d.retrievals.remove(j);
                } else if let Some(j) = up_retrieval {
                    d.retrievals.swap(j, j - 1);
                }
                if ui
                    .add_enabled(
                        expression_draft.is_none() && d.retrievals.len() < 256,
                        egui::Button::new("Add message retrieval"),
                    )
                    .clicked()
                {
                    d.retrievals.push(RetrievalDraft {
                        value: CorrelationRetrieval {
                            id: None,
                            base: None,
                            message_ref: None,
                            message_path: None,
                        },
                        message: None,
                    });
                }
                ui.add_enabled_ui(expression_draft.is_none(), |ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move property up"))
                        .clicked()
                    {
                        up = Some(i);
                    }
                    if ui.button("Remove property").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            self.values.remove(i);
        } else if let Some(i) = up {
            self.values.swap(i, i - 1);
        }
        if ui
            .add_enabled(
                expression_draft.is_none() && self.values.len() < 256,
                egui::Button::new("Add correlation property"),
            )
            .clicked()
        {
            self.values.push(CorrelationDraft {
                value: CorrelationProperty {
                    id: None,
                    base: None,
                    name: None,
                    item_type: None,
                    retrieval_expressions: vec![],
                },
                item_type: None,
                retrievals: vec![],
            });
        }
    }
}
impl ProcessCraft {
    pub(super) fn correlation_catalog_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        if self.correlation_catalog_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit correlation properties…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.correlation_catalog_draft = Some(CorrelationCatalogDraft::new(
                    &document.correlation_properties,
                ));
            }
            return;
        }
        let draft = self.correlation_catalog_draft.as_mut().unwrap();
        ui.strong("Correlation properties");
        draft.ui(ui, document, &mut self.expression_draft);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Apply correlation properties"),
            )
            .clicked()
        {
            match draft.build() {
                Ok(values) => {
                    if self.ctl.mutate(vec![Mutation::SetCorrelationProperties {
                        document_id: document.id,
                        correlation_properties: values,
                    }]) {
                        self.correlation_catalog_draft = None;
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui
            .add_enabled(
                self.expression_draft.is_none(),
                egui::Button::new("Cancel correlation editing"),
            )
            .clicked()
        {
            self.correlation_catalog_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlation_draft_preserves_metadata_and_guards_navigation() {
        let value = CorrelationProperty {
            id: Some("Correlation".parse().unwrap()),
            base: Some(Default::default()),
            name: Some(String::new()),
            item_type: None,
            retrieval_expressions: vec![CorrelationRetrieval {
                id: None,
                base: Some(Default::default()),
                message_ref: None,
                message_path: Some("path".into()),
            }],
        };
        let draft = CorrelationCatalogDraft::new(std::slice::from_ref(&value));
        assert_eq!(draft.build().unwrap(), vec![value]);
        let mut app = ProcessCraft {
            correlation_catalog_draft: Some(draft),
            ..Default::default()
        };
        assert!(app.has_drafts());
        assert!(!app.commit_draft());
        app.select(Some("Other".parse().unwrap()));
        assert!(app.selected.is_none());
        assert!(app.correlation_catalog_draft.is_some());
    }
}
