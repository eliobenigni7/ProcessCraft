use super::xml::MixedDraft;
use super::*;
use processcraft_core::bpmn_xml::MixedXml;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum BodyOwner {
    Annotation(ElementSelector),
    Script(ElementSelector),
}
pub(super) struct BodyDraft {
    owner: BodyOwner,
    value: Option<MixedDraft>,
    error: String,
}
impl ProcessCraft {
    pub(super) fn mixed_body_ui(
        &mut self,
        ui: &mut egui::Ui,
        owner: BodyOwner,
        value: Option<MixedXml>,
        editable: bool,
    ) {
        if self.body_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit structured content…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.body_draft = Some(BodyDraft {
                    owner,
                    value: value.as_ref().map(MixedDraft::new),
                    error: String::new(),
                });
            }
            return;
        }
        let draft = self.body_draft.as_mut().unwrap();
        if draft.owner != owner {
            return;
        }
        let mut present = draft.value.is_some();
        ui.checkbox(&mut present, "Content present");
        if present {
            draft
                .value
                .get_or_insert_with(|| MixedDraft::new(&MixedXml::Text(String::new())))
                .ui(ui);
        } else {
            draft.value = None;
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply content").clicked() && self.apply_body_draft().is_ok() {
            self.restore_selection();
        }
        if ui.button("Cancel content editing").clicked() {
            self.body_draft = None;
        }
    }
    pub(super) fn apply_body_draft(&mut self) -> Result<(), String> {
        self.apply_typed_draft(
            |app| &mut app.body_draft,
            |draft, document| {
                let body = draft
                    .value
                    .as_ref()
                    .map(MixedDraft::build)
                    .transpose()
                    .inspect_err(|e| draft.error = e.clone())?;
                let document_id = document.id;
                Ok(vec![match &draft.owner {
                    BodyOwner::Annotation(element_id) => Mutation::SetAnnotationBody {
                        document_id,
                        element_id: element_id.clone(),
                        body,
                    },
                    BodyOwner::Script(element_id) => Mutation::SetScriptBody {
                        document_id,
                        element_id: element_id.clone(),
                        body,
                    },
                }])
            },
        )
    }
}
