use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;

pub(super) struct ParticipantDraft {
    generation: u64,
    root: ElementSelector,
    value: Participant,
    is_new: bool,
    process: Option<ReferenceDraft>,
    interfaces: Vec<Option<ReferenceDraft>>,
    endpoints: Vec<Option<ReferenceDraft>>,
}
impl ParticipantDraft {
    fn new(root: &Collaboration, value: Participant, generation: u64, is_new: bool) -> Self {
        Self {
            generation,
            root: root.id.selector(),
            process: value.process.as_ref().map(ReferenceDraft::new),
            interfaces: value
                .interface_refs
                .iter()
                .map(|v| Some(ReferenceDraft::new(v)))
                .collect(),
            endpoints: value
                .endpoint_refs
                .iter()
                .map(|v| Some(ReferenceDraft::new(v)))
                .collect(),
            value,
            is_new,
        }
    }
    fn build(&self) -> Result<Participant, String> {
        let build_list = |values: &[Option<ReferenceDraft>]| {
            values
                .iter()
                .map(|v| {
                    v.as_ref()
                        .ok_or_else(|| "Select a target or remove the reference.".to_owned())?
                        .build()
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let mut value = self.value.clone();
        value.process = build_ref(&self.process)?;
        value.interface_refs = build_list(&self.interfaces)?;
        value.endpoint_refs = build_list(&self.endpoints)?;
        Ok(value)
    }
}
fn ref_list(
    ui: &mut egui::Ui,
    title: &str,
    values: &mut Vec<Option<ReferenceDraft>>,
    choices: &[(ElementSelector, String)],
) {
    ui.strong(title);
    let mut remove = None;
    for (index, value) in values.iter_mut().enumerate() {
        ui.push_id((title, index), |ui| {
            ref_ui(ui, &format!("{title} {}", index + 1), value, choices);
            if ui.button("Remove reference").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        values.remove(index);
    }
    if ui
        .add_enabled(
            values.len() < 256,
            egui::Button::new(format!("Add {title}")),
        )
        .clicked()
    {
        values.push(Some(ReferenceDraft::Local(None)));
    }
}
impl ProcessCraft {
    pub(super) fn collaboration_participants_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
    ) {
        ui.collapsing("Participants", |ui| {
            if self.collaboration.participant.is_none() {
                for (index, value) in root.participants.iter().enumerate() {
                    let name = super::event::local_choice_label(value.name.as_deref(), "Participant", &value.id, index + 1);
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {name} properties"))).clicked() && self.commit_draft() { self.select(Some(value.id.selector())); }
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Edit {name}…"))).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id);
                    self.collaboration.participant = Some(ParticipantDraft::new(root, value.clone(), generation, false)); }
                }
                if ui.add_enabled(!self.collaboration.has_draft() && root.participants.len() < 256, egui::Button::new("Add participant…")).clicked() && self.commit_draft() {
                    self.collaboration.document = Some(document.id);
                    self.collaboration.participant = Some(ParticipantDraft::new(root, Participant { id: processcraft_core::ids::ElementIdentity::anonymous(), base: None, name: None, process: None, interface_refs: vec![], endpoint_refs: vec![], multiplicity: None }, generation, true));
                }
                return;
            }
            let processes: Vec<_> = document.processes.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Process", &v.id, i + 1))).collect();
            let interfaces: Vec<_> = document.catalog.interfaces.iter().enumerate().filter_map(|(i,v)| v.id.as_ref().map(|id| (id.selector(), super::event::local_choice_label(Some(&v.name), "Interface", id, i + 1)))).collect();
            let endpoints: Vec<_> = document.endpoints.iter().enumerate().filter_map(|(i,v)| v.id.as_ref().map(|id| (id.selector(), super::event::local_choice_label(None, "Endpoint", id, i + 1)))).collect();
            let mut draft = self.collaboration.participant.take().expect("open");
            let mut close = false;
            super::catalog::optional_text(ui, "Name", &mut draft.value.name);
            ref_ui(ui, "Process", &mut draft.process, &processes);
            ref_list(ui, "Interface", &mut draft.interfaces, &interfaces);
            ref_list(ui, "Endpoint", &mut draft.endpoints, &endpoints);
            ui.horizontal(|ui| {
                if ui.button("Apply participant").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { match draft.build() { Ok(value) => { let mut updated = root.clone(); if draft.is_new { updated.participants.push(value); } else if let Some(current) = updated.participants.iter_mut().find(|v| v.id == value.id) { *current = value; } else { self.ctl.error = "The participant no longer exists.".into(); return; }
                    if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }, Err(error) => self.ctl.error = error } }
                }
                if !draft.is_new && ui.button("Delete participant").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { let mut updated = root.clone(); updated.participants.retain(|value| value.id != draft.value.id); if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }
                }
                if ui.button("Discard participant draft").clicked() { close = true; }
            });
            if !close { self.collaboration.participant = Some(draft); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_model::QNameRef;
    #[test]
    fn participant_draft_keeps_qualified_namespaces_and_invalid_text() {
        let root = Collaboration {
            kind: None,
            extras: None,
            correlation_keys: vec![],
            base: None,
            name: None,
            id: "Root".parse().unwrap(),
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        let value = Participant {
            id: "Participant".parse().unwrap(),
            name: Some(String::new()),
            base: None,
            process: Some(ModelReference::Qualified(QNameRef {
                namespace: "urn:remote".into(),
                local: "Process".parse().unwrap(),
            })),
            interface_refs: vec![ModelReference::Qualified(QNameRef {
                namespace: "urn:interface".into(),
                local: "I".parse().unwrap(),
            })],
            endpoint_refs: vec![],
            multiplicity: None,
        };
        let mut draft = ParticipantDraft::new(&root, value.clone(), 0, false);
        let built = draft.build().unwrap();
        assert_eq!(built.process, value.process);
        assert_eq!(built.interface_refs, value.interface_refs);
        assert_eq!(built.name, Some(String::new()));
        draft.process = Some(ReferenceDraft::Qualified(
            super::super::catalog::QNameDraft {
                namespace: "urn:remote".into(),
                local: "invalid name".into(),
            },
        ));
        assert!(draft.build().is_err());
        assert!(matches!(draft.process, Some(ReferenceDraft::Qualified(_))));
    }
}
