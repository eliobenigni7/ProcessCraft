use super::*;
use processcraft_core::bpmn_collaboration::CollaborationRootKind;

pub(super) struct RootDraft {
    generation: u64,
    value: Collaboration,
}
pub(super) struct RootOrderDraft {
    generation: u64,
    order: Vec<ElementSelector>,
}
impl ProcessCraft {
    pub(super) fn collaboration_root_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.collaboration.has_draft(),
                    egui::Button::new("Select collaboration identity"),
                )
                .on_hover_text("Select this root to assign or change its XML ID in Properties.")
                .clicked()
                && self.commit_draft()
            {
                self.select(Some(root.id.selector()));
            }
            if self.collaboration.root_details.is_none()
                && ui
                    .add_enabled(
                        !self.collaboration.has_draft(),
                        egui::Button::new("Edit collaboration…"),
                    )
                    .clicked()
                && self.commit_draft()
            {
                self.collaboration.document = Some(document.id);
                self.collaboration.root_details = Some(RootDraft {
                    generation,
                    value: root.clone(),
                });
            }
            if ui
                .add_enabled(
                    !self.collaboration.has_draft(),
                    egui::Button::new("Delete collaboration"),
                )
                .on_hover_text("Delete this collaboration and its diagrams.")
                .clicked()
                && self.commit_draft()
                && self.ctl.mutate(vec![if document
                    .choreographies
                    .iter()
                    .any(|value| value.id == root.id)
                {
                    Mutation::DeleteChoreography {
                        document_id: document.id,
                        choreography_id: root.id.selector(),
                    }
                } else {
                    Mutation::DeleteCollaborationRoot {
                        document_id: document.id,
                        collaboration_id: root.id.selector(),
                    }
                }])
            {
                self.collaboration.root = None;
            }
        });
        if let Some(mut draft) = self.collaboration.root_details.take() {
            let mut close = false;
            ui.group(|ui| {
                super::catalog::optional_text(ui, "Name", &mut draft.value.name);
                let mut global = draft.value.kind.is_some();
                if !document.choreographies.iter().any(|value| value.id == draft.value.id) && ui.checkbox(&mut global, "Global conversation").changed() { draft.value.kind = global.then_some(CollaborationRootKind::GlobalConversation); }
                ui.horizontal(|ui| {
                    if ui.button("Apply collaboration").clicked() {
                        if draft.generation != self.ctl.session.snapshot().generation || draft.value.id != root.id { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                        else if self.ctl.mutate(vec![super::collaboration::update_root(document, draft.value.clone())]) { if document.collaboration.as_ref().is_some_and(|first| first.id == draft.value.id) { self.collaboration_name = draft.value.name.clone(); } close = true; }
                    }
                    if ui.button("Discard collaboration draft").clicked() { close = true; }
                });
            });
            if !close {
                self.collaboration.root_details = Some(draft);
            }
        }
        ui.collapsing("Document root order", |ui| {
            if self.collaboration.root_order.is_none() {
                if let Some(order) = &document.root_order {
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Edit root order…")).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id);
                    self.collaboration.root_order = Some(RootOrderDraft { generation, order: order.clone() }); }
                } else {
                    ui.label("Order is not explicitly set.");
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Initialize root order")).on_hover_text("Records the current order and assigns internal identities to anonymous roots without assigning XML IDs.").clicked() && self.commit_draft()
                        && let Some(changes) = self.ctl.mutate_changes(vec![Mutation::InitializeDocumentRootOrder { document_id: document.id }]) { self.remap_context(&changes.selector_mappings); }
                }
                return;
            }
            let mut draft = self.collaboration.root_order.take().expect("open");
            let mut close = false;
            let mut swap = None;
            for (index, selector) in draft.order.iter().enumerate() {
                ui.push_id(index, |ui| ui.horizontal(|ui| {
                    let identity = processcraft_core::queries::element_identity(document, selector);
                    let name = processcraft_core::queries::element_name(document, selector).filter(|name| !name.is_empty()).map(str::to_owned).or_else(|| identity.and_then(|id| id.xml_id()).map(ToString::to_string)).unwrap_or_else(|| format!("Root {}", index + 1));
                    ui.label(name);
                    if ui.add_enabled(index > 0, egui::Button::new("Move up")).clicked() { swap = Some(index); }
                }));
            }
            if let Some(index) = swap { draft.order.swap(index, index - 1); }
            ui.horizontal(|ui| {
                if ui.button("Apply root order").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else if self.ctl.mutate(vec![Mutation::SetDocumentRootOrder { document_id: document.id, order: Some(draft.order.clone()) }]) { close = true; }
                }
                if ui.button("Discard root order draft").clicked() { close = true; }
            });
            if !close { self.collaboration.root_order = Some(draft); }
        });
    }
}
