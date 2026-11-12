use super::catalog::{IdentityDraft, id_build, id_string, optional_text};
use super::xml::MixedDraft;
use super::*;
use processcraft_core::bpmn_metadata::{Documentation, DocumentationSet};
use processcraft_core::bpmn_xml::MixedXml;

pub(super) struct EntryDraft {
    id: IdentityDraft,
    format: Option<String>,
    content: MixedDraft,
}
impl EntryDraft {
    pub(super) fn new(value: &Documentation) -> Self {
        Self {
            id: id_string(&value.id),
            format: value.text_format.clone(),
            content: MixedDraft::new(&value.content),
        }
    }
    pub(super) fn empty() -> Self {
        Self {
            id: Default::default(),
            format: None,
            content: MixedDraft::new(&MixedXml::Text(String::new())),
        }
    }
    pub(super) fn build(&self) -> Result<Documentation, String> {
        Ok(Documentation {
            id: id_build(&self.id)?,
            text_format: self.format.clone(),
            content: self.content.build()?,
        })
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        optional_text(ui, "Documentation ID", &mut self.id.value);
        optional_text(ui, "Text format", &mut self.format);
        self.content.ui(ui);
    }
}
enum BodyDraft {
    Legacy(String),
    Structured(Vec<EntryDraft>),
}
pub(super) struct DocumentationDraft {
    target: ElementSelector,
    body: BodyDraft,
    error: String,
}
impl DocumentationDraft {
    fn new(target: ElementSelector, value: &DocumentationSet) -> Self {
        Self {
            target,
            body: match value {
                DocumentationSet::Legacy(text) => BodyDraft::Legacy(text.clone()),
                DocumentationSet::Structured(entries) => {
                    BodyDraft::Structured(entries.iter().map(EntryDraft::new).collect())
                }
            },
            error: String::new(),
        }
    }
    fn build(&self) -> Result<DocumentationSet, String> {
        Ok(match &self.body {
            BodyDraft::Legacy(text) => DocumentationSet::Legacy(text.clone()),
            BodyDraft::Structured(entries) => DocumentationSet::Structured(
                entries
                    .iter()
                    .map(EntryDraft::build)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        if let BodyDraft::Legacy(text) = &mut self.body {
            let label = ui.label("Documentation");
            ui.text_edit_multiline(text).labelled_by(label.id);
            if ui.button("Add another entry").clicked() {
                let mut entries = vec![EntryDraft {
                    id: Default::default(),
                    format: None,
                    content: MixedDraft::new(&MixedXml::Text(text.clone())),
                }];
                entries.push(EntryDraft::empty());
                self.body = BodyDraft::Structured(entries);
            }
        } else if let BodyDraft::Structured(entries) = &mut self.body {
            let mut remove = None;
            let mut swap = None;
            for (i, entry) in entries.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.strong(format!("Documentation {}", i + 1));
                    optional_text(ui, "Documentation ID", &mut entry.id.value);
                    optional_text(ui, "Text format", &mut entry.format);
                    ui.label(if entry.format.is_none() {
                        "Unspecified (text/plain)"
                    } else {
                        "Explicit text format"
                    });
                    ui.collapsing("Content", |ui| entry.content.ui(ui));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move up"))
                            .clicked()
                        {
                            swap = Some((i, i - 1));
                        }
                        if ui.button("Remove entry").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                entries.remove(i);
            } else if let Some((a, b)) = swap {
                entries.swap(a, b);
            }
            if ui
                .add_enabled(entries.len() < 256, egui::Button::new("Add entry"))
                .clicked()
            {
                entries.push(EntryDraft::empty());
            }
        }
    }
}
impl ProcessCraft {
    pub(super) fn documentation_ui(
        &mut self,
        ui: &mut egui::Ui,
        target: ElementSelector,
        value: &DocumentationSet,
        editable: bool,
    ) {
        if self.documentation_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit documentation…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.documentation_draft = Some(DocumentationDraft::new(target, value));
            }
            return;
        }
        if self
            .documentation_draft
            .as_ref()
            .is_none_or(|draft| draft.target != target)
        {
            return;
        }
        let draft = self.documentation_draft.as_mut().unwrap();
        ui.strong("Documentation entries");
        draft.ui(ui);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply documentation").clicked() {
            match draft.build() {
                Ok(documentation) => {
                    let document_id = self.ctl.session.snapshot().project.documents[self.doc].id;
                    if self.ctl.mutate(vec![Mutation::ReplaceDocumentation {
                        document_id,
                        element_id: draft.target.clone(),
                        documentation,
                    }]) {
                        self.documentation_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel documentation editing").clicked() {
            self.documentation_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documentation_apply_undo_redo_preserves_mixed_entries() {
        use processcraft_core::commands::Request;
        let mut app = ProcessCraft::default();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let document_id = document.id;
        let target = document.processes[0].id.selector();
        let docs = DocumentationSet::Structured(vec![
            Documentation {
                id: None,
                text_format: Some("text/plain".into()),
                content: MixedXml::Text("first".into()),
            },
            Documentation {
                id: None,
                text_format: Some("text/html".into()),
                content: MixedXml::Text("second".into()),
            },
        ]);
        assert!(app.ctl.mutate(vec![Mutation::ReplaceDocumentation {
            document_id,
            element_id: target.clone(),
            documentation: docs.clone()
        }]));
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].processes[0].documentation,
            docs
        );
        app.history(Request::Undo);
        assert_ne!(
            app.ctl.session.snapshot().project.documents[0].processes[0].documentation,
            docs
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].processes[0].documentation,
            docs
        );
    }
    #[test]
    fn structured_entries_keep_order_and_siblings() {
        let original = DocumentationSet::Structured(vec![
            Documentation {
                id: None,
                text_format: Some("text/plain".into()),
                content: MixedXml::Text("first".into()),
            },
            Documentation {
                id: None,
                text_format: Some(String::new()),
                content: MixedXml::Text("second".into()),
            },
        ]);
        let mut draft = DocumentationDraft::new("Activity".parse().unwrap(), &original);
        assert_eq!(draft.build().unwrap(), original);
        if let BodyDraft::Structured(entries) = &mut draft.body {
            entries[0].content = MixedDraft::new(&MixedXml::Text("edited".into()));
        }
        let DocumentationSet::Structured(entries) = draft.build().unwrap() else {
            panic!("entries lost");
        };
        assert_eq!(entries[0].content, MixedXml::Text("edited".into()));
        assert_eq!(entries[1].content, MixedXml::Text("second".into()));
        assert_eq!(entries[1].text_format.as_deref(), Some(""));
    }
}
