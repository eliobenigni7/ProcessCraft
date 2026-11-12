use super::event::{ReferenceDraft, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{EndPoint, PartnerEntity, PartnerRole};

struct PartnerDraft<T> {
    value: T,
    references: Vec<Option<ReferenceDraft>>,
}
pub(super) struct PartnerCatalogDraft {
    generation: u64,
    entities: Vec<PartnerDraft<PartnerEntity>>,
    roles: Vec<PartnerDraft<PartnerRole>>,
    endpoints: Vec<EndPoint>,
}
impl PartnerCatalogDraft {
    fn new(doc: &BpmnDocument, generation: u64) -> Self {
        Self {
            generation,
            entities: doc
                .partner_entities
                .iter()
                .map(|value| PartnerDraft {
                    value: value.clone(),
                    references: value
                        .participant_refs
                        .iter()
                        .map(|v| Some(ReferenceDraft::new(v)))
                        .collect(),
                })
                .collect(),
            roles: doc
                .partner_roles
                .iter()
                .map(|value| PartnerDraft {
                    value: value.clone(),
                    references: value
                        .participant_refs
                        .iter()
                        .map(|v| Some(ReferenceDraft::new(v)))
                        .collect(),
                })
                .collect(),
            endpoints: doc.endpoints.clone(),
        }
    }
    fn build(&self, document_id: processcraft_core::ids::DocumentId) -> Result<Mutation, String> {
        let refs = |values: &[Option<ReferenceDraft>]| {
            values
                .iter()
                .map(|v| {
                    v.as_ref()
                        .ok_or_else(|| "Select a participant or remove the reference.".to_owned())?
                        .build()
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let entities = self
            .entities
            .iter()
            .map(|draft| {
                let mut value = draft.value.clone();
                value.participant_refs = refs(&draft.references)?;
                Ok(value)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let roles = self
            .roles
            .iter()
            .map(|draft| {
                let mut value = draft.value.clone();
                value.participant_refs = refs(&draft.references)?;
                Ok(value)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Mutation::SetPartnerCatalog {
            document_id,
            partner_entities: entities,
            partner_roles: roles,
            endpoints: self.endpoints.clone(),
        })
    }
}
fn references_ui(
    ui: &mut egui::Ui,
    references: &mut Vec<Option<ReferenceDraft>>,
    choices: &[(ElementSelector, String)],
) {
    let mut remove = None;
    for (index, reference) in references.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ref_ui(
                ui,
                &format!("Participant {}", index + 1),
                reference,
                choices,
            );
            if ui.button("Remove participant reference").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        references.remove(index);
    }
    if ui
        .add_enabled(
            references.len() < 256,
            egui::Button::new("Add participant reference"),
        )
        .clicked()
    {
        references.push(Some(ReferenceDraft::Local(None)));
    }
}
impl ProcessCraft {
    pub(super) fn partner_catalog_ui(
        &mut self,
        ui: &mut egui::Ui,
        doc: &BpmnDocument,
        generation: u64,
    ) {
        ui.collapsing("Partners and endpoints", |ui| {
            if self.collaboration.partner_catalog.is_none() {
                for (kind, name, identity) in doc.partner_entities.iter().map(|value| ("Partner entity", value.name.as_deref(), value.id.as_ref())).chain(doc.partner_roles.iter().map(|value| ("Partner role", value.name.as_deref(), value.id.as_ref()))).chain(doc.endpoints.iter().map(|value| ("Endpoint", None, value.id.as_ref()))) {
                    if let Some(identity) = identity { let label = super::event::local_choice_label(name, kind, identity, 1); if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {label} properties"))).clicked() && self.commit_draft() { self.select(Some(identity.selector())); } }
                    else { ui.label(format!("{kind}: select and promote its identity in document Properties to edit its metadata or reference it.")); }
                }
                if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Edit partners and endpoints…")).clicked() && self.commit_draft() {
                    self.collaboration.document = Some(doc.id);
                    self.collaboration.partner_catalog = Some(PartnerCatalogDraft::new(doc, generation));
                }
                return;
            }
            let choices: Vec<_> = doc.collaboration.iter().chain(doc.additional_collaborations.iter()).flat_map(|root| root.participants.iter()).chain(doc.choreographies.iter().flat_map(|root| root.participants.iter())).enumerate().map(|(index, participant)| (participant.id.selector(), super::event::local_choice_label(participant.name.as_deref(), "Participant", &participant.id, index + 1))).collect();
            let mut draft = self.collaboration.partner_catalog.take().expect("open");
            let mut close = false;
            ui.strong("Partner entities");
            let mut remove = None;
            for (index, partner) in draft.entities.iter_mut().enumerate() {
                ui.push_id(("entity", index), |ui| { ui.collapsing(format!("Entity {}", index + 1), |ui| {
                    super::catalog::optional_text(ui, "Name", &mut partner.value.name);
                    references_ui(ui, &mut partner.references, &choices);
                    if ui.button("Remove entity").clicked() { remove = Some(index); }
                }); });
            }
            if let Some(index) = remove { draft.entities.remove(index); }
            if ui.add_enabled(draft.entities.len() < 256, egui::Button::new("Add entity")).clicked() { draft.entities.push(PartnerDraft { value: PartnerEntity { id: Some(processcraft_core::ids::ElementIdentity::anonymous()), base: None, name: None, participant_refs: vec![] }, references: vec![] }); }
            ui.strong("Partner roles");
            let mut remove = None;
            for (index, partner) in draft.roles.iter_mut().enumerate() {
                ui.push_id(("role", index), |ui| { ui.collapsing(format!("Role {}", index + 1), |ui| {
                    super::catalog::optional_text(ui, "Name", &mut partner.value.name);
                    references_ui(ui, &mut partner.references, &choices);
                    if ui.button("Remove role").clicked() { remove = Some(index); }
                }); });
            }
            if let Some(index) = remove { draft.roles.remove(index); }
            if ui.add_enabled(draft.roles.len() < 256, egui::Button::new("Add role")).clicked() { draft.roles.push(PartnerDraft { value: PartnerRole { id: Some(processcraft_core::ids::ElementIdentity::anonymous()), base: None, name: None, participant_refs: vec![] }, references: vec![] }); }
            ui.strong("Endpoints");
            let mut remove = None;
            for (index, endpoint) in draft.endpoints.iter().enumerate() {
                ui.push_id(("endpoint", index), |ui| ui.horizontal(|ui| {
                    ui.label(endpoint.id.as_ref().and_then(|id| id.xml_id()).map_or_else(|| format!("Endpoint {}", index + 1), ToString::to_string));
                    if ui.button("Remove endpoint").clicked() { remove = Some(index); }
                }));
            }
            if let Some(index) = remove { draft.endpoints.remove(index); }
            if ui.add_enabled(draft.endpoints.len() < 256, egui::Button::new("Add endpoint")).clicked() { draft.endpoints.push(EndPoint { id: Some(processcraft_core::ids::ElementIdentity::anonymous()), base: None }); }
            ui.horizontal(|ui| {
                if ui.button("Apply partners and endpoints").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); }
                    else { match draft.build(doc.id) { Ok(mutation) => { if self.ctl.mutate(vec![mutation]) { close = true; } }, Err(error) => self.ctl.error = error } }
                }
                if ui.button("Discard partner draft").clicked() { close = true; }
            });
            if !close { self.collaboration.partner_catalog = Some(draft); }
        });
    }
}
