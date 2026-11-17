//! Editor for the partner entities, partner roles and endpoints of a document.
use super::event::{ReferenceDraft, local_choice_label, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{EndPoint, PartnerEntity, PartnerRole};
use processcraft_core::ids::ElementIdentity;

/// Upper bound on the entries of each partner list in a draft.
const MAX_ENTRIES: usize = 256;
const STALE_DRAFT: &str =
    "The model changed while this draft was open. Discard it and reopen the editor.";

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
            references.len() < MAX_ENTRIES,
            egui::Button::new("Add participant reference"),
        )
        .clicked()
    {
        references.push(Some(ReferenceDraft::Local(None)));
    }
}

/// One collapsible entity/role editor; returns true when "Remove" was clicked.
fn partner_ui<T>(
    ui: &mut egui::Ui,
    (id_salt, title, remove_label): (&str, String, &str),
    index: usize,
    partner: &mut PartnerDraft<T>,
    name: impl FnOnce(&mut T) -> &mut Option<String>,
    choices: &[(ElementSelector, String)],
) -> bool {
    let mut remove = false;
    ui.push_id((id_salt, index), |ui| {
        ui.collapsing(title, |ui| {
            super::catalog::optional_text(ui, "Name", name(&mut partner.value));
            references_ui(ui, &mut partner.references, choices);
            if ui.button(remove_label).clicked() {
                remove = true;
            }
        });
    });
    remove
}

/// An "Add ..." button that is disabled once a list reaches its bound.
fn add_button(ui: &mut egui::Ui, len: usize, label: &str) -> bool {
    ui.add_enabled(len < MAX_ENTRIES, egui::Button::new(label))
        .clicked()
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
                let entities = doc
                    .partner_entities
                    .iter()
                    .map(|value| ("Partner entity", value.name.as_deref(), value.id.as_ref()));
                let roles = doc
                    .partner_roles
                    .iter()
                    .map(|value| ("Partner role", value.name.as_deref(), value.id.as_ref()));
                let endpoints = doc
                    .endpoints
                    .iter()
                    .map(|value| ("Endpoint", None, value.id.as_ref()));
                for (kind, name, identity) in entities.chain(roles).chain(endpoints) {
                    let Some(identity) = identity else {
                        ui.label(format!(
                            "{kind}: select and promote its identity in document Properties \
                             to edit its metadata or reference it."
                        ));
                        continue;
                    };
                    let label = local_choice_label(name, kind, identity, 1);
                    let select = egui::Button::new(format!("Select {label} properties"));
                    let idle = !self.collaboration.has_draft();
                    if ui.add_enabled(idle, select).clicked() && self.commit_draft() {
                        self.select(Some(identity.selector()));
                    }
                }
                let edit = egui::Button::new("Edit partners and endpoints…");
                if ui
                    .add_enabled(!self.collaboration.has_draft(), edit)
                    .clicked()
                    && self.commit_draft()
                {
                    self.collaboration.document = Some(doc.id);
                    self.collaboration.partner_catalog =
                        Some(PartnerCatalogDraft::new(doc, generation));
                }
                return;
            }
            let participants = doc
                .collaboration
                .iter()
                .chain(doc.additional_collaborations.iter())
                .flat_map(|root| root.participants.iter())
                .chain(
                    doc.choreographies
                        .iter()
                        .flat_map(|root| root.participants.iter()),
                );
            let choices: Vec<_> = participants
                .enumerate()
                .map(|(index, participant)| {
                    let label = local_choice_label(
                        participant.name.as_deref(),
                        "Participant",
                        &participant.id,
                        index + 1,
                    );
                    (participant.id.selector(), label)
                })
                .collect();
            let mut draft = self.collaboration.partner_catalog.take().expect("open");
            let mut close = false;
            ui.strong("Partner entities");
            let mut remove = None;
            for (index, partner) in draft.entities.iter_mut().enumerate() {
                let title = format!("Entity {}", index + 1);
                let labels = ("entity", title, "Remove entity");
                if partner_ui(ui, labels, index, partner, |v| &mut v.name, &choices) {
                    remove = Some(index);
                }
            }
            if let Some(index) = remove {
                draft.entities.remove(index);
            }
            if add_button(ui, draft.entities.len(), "Add entity") {
                draft.entities.push(PartnerDraft {
                    value: PartnerEntity {
                        id: Some(ElementIdentity::anonymous()),
                        base: None,
                        name: None,
                        participant_refs: vec![],
                    },
                    references: vec![],
                });
            }
            ui.strong("Partner roles");
            let mut remove = None;
            for (index, partner) in draft.roles.iter_mut().enumerate() {
                let title = format!("Role {}", index + 1);
                let labels = ("role", title, "Remove role");
                if partner_ui(ui, labels, index, partner, |v| &mut v.name, &choices) {
                    remove = Some(index);
                }
            }
            if let Some(index) = remove {
                draft.roles.remove(index);
            }
            if add_button(ui, draft.roles.len(), "Add role") {
                draft.roles.push(PartnerDraft {
                    value: PartnerRole {
                        id: Some(ElementIdentity::anonymous()),
                        base: None,
                        name: None,
                        participant_refs: vec![],
                    },
                    references: vec![],
                });
            }
            ui.strong("Endpoints");
            let mut remove = None;
            for (index, endpoint) in draft.endpoints.iter().enumerate() {
                ui.push_id(("endpoint", index), |ui| {
                    ui.horizontal(|ui| {
                        let label = endpoint
                            .id
                            .as_ref()
                            .and_then(|id| id.xml_id())
                            .map_or_else(|| format!("Endpoint {}", index + 1), ToString::to_string);
                        ui.label(label);
                        if ui.button("Remove endpoint").clicked() {
                            remove = Some(index);
                        }
                    })
                });
            }
            if let Some(index) = remove {
                draft.endpoints.remove(index);
            }
            if add_button(ui, draft.endpoints.len(), "Add endpoint") {
                draft.endpoints.push(EndPoint {
                    id: Some(ElementIdentity::anonymous()),
                    base: None,
                });
            }
            ui.horizontal(|ui| {
                if ui.button("Apply partners and endpoints").clicked() {
                    if draft.generation != self.ctl.session.snapshot().generation {
                        self.ctl.error = STALE_DRAFT.into();
                    } else {
                        match draft.build(doc.id) {
                            Ok(mutation) => close |= self.ctl.mutate(vec![mutation]),
                            Err(error) => self.ctl.error = error,
                        }
                    }
                }
                if ui.button("Discard partner draft").clicked() {
                    close = true;
                }
            });
            if !close {
                self.collaboration.partner_catalog = Some(draft);
            }
        });
    }
}
