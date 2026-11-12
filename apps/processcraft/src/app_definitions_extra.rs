use super::catalog::{QNameDraft, optional_bool, optional_qname, q_build, text};
use super::documentation::EntryDraft;
use super::xml::{AttributeDraft, bindings_ui};
use super::*;
use processcraft_core::model::{
    DefinitionsExtension, DefinitionsRelationship, RelationshipDirection,
};
struct ExtensionDraft {
    documentation: Vec<EntryDraft>,
    definition: Option<QNameDraft>,
    must_understand: Option<bool>,
}
struct RelationshipDraft {
    value: DefinitionsRelationship,
    sources: Vec<QNameDraft>,
    targets: Vec<QNameDraft>,
}
pub(super) struct DefinitionsExtraDraft {
    extensions: Vec<ExtensionDraft>,
    relationships: Vec<RelationshipDraft>,
    namespaces: Vec<processcraft_core::bpmn_xml::NamespaceBinding>,
    attributes: Vec<AttributeDraft>,
    metadata: Option<processcraft_core::model::DefinitionsMetadata>,
    error: String,
}
impl DefinitionsExtraDraft {
    fn new(d: &BpmnDocument) -> Self {
        Self {
            extensions: d
                .extensions
                .iter()
                .map(|e| ExtensionDraft {
                    documentation: e.documentation.iter().map(EntryDraft::new).collect(),
                    definition: e.definition.as_ref().map(QNameDraft::from),
                    must_understand: e.must_understand,
                })
                .collect(),
            relationships: d
                .relationships
                .iter()
                .map(|r| RelationshipDraft {
                    value: r.clone(),
                    sources: r.sources.iter().map(QNameDraft::from).collect(),
                    targets: r.targets.iter().map(QNameDraft::from).collect(),
                })
                .collect(),
            namespaces: d
                .definitions
                .as_ref()
                .map_or_else(Vec::new, |m| m.namespaces.clone()),
            attributes: d.definitions.as_ref().map_or_else(Vec::new, |m| {
                m.foreign_attributes
                    .iter()
                    .map(AttributeDraft::new)
                    .collect()
            }),
            metadata: d.definitions.clone(),
            error: String::new(),
        }
    }
    fn changes(&self, id: processcraft_core::ids::DocumentId) -> Result<Vec<Mutation>, String> {
        let extensions = self
            .extensions
            .iter()
            .map(|e| {
                Ok(DefinitionsExtension {
                    documentation: e
                        .documentation
                        .iter()
                        .map(EntryDraft::build)
                        .collect::<Result<_, _>>()?,
                    definition: q_build(&e.definition)?,
                    must_understand: e.must_understand,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let relationships = self
            .relationships
            .iter()
            .map(|r| {
                let mut v = r.value.clone();
                v.sources = r
                    .sources
                    .iter()
                    .map(QNameDraft::build)
                    .collect::<Result<_, _>>()?;
                v.targets = r
                    .targets
                    .iter()
                    .map(QNameDraft::build)
                    .collect::<Result<_, _>>()?;
                Ok(v)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut result = vec![
            Mutation::SetDefinitionsExtensions {
                document_id: id,
                extensions,
            },
            Mutation::SetDefinitionsRelationships {
                document_id: id,
                relationships,
            },
        ];
        if let Some(mut metadata) = self.metadata.clone() {
            metadata.namespaces = self.namespaces.clone();
            metadata.foreign_attributes = self
                .attributes
                .iter()
                .map(AttributeDraft::build)
                .collect::<Result<_, _>>()?;
            result.push(Mutation::SetDefinitionsMetadata {
                document_id: id,
                metadata,
            });
        }
        Ok(result)
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        if self.metadata.is_some() {
            ui.collapsing("Definitions namespaces and attributes", |ui| {
                bindings_ui(ui, &mut self.namespaces);
                let mut remove = None;
                for (i, a) in self.attributes.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        a.ui(ui);
                        if ui.button("Remove attribute").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.attributes.remove(i);
                }
                if ui
                    .add_enabled(
                        self.attributes.len() < 256,
                        egui::Button::new("Add attribute"),
                    )
                    .clicked()
                {
                    self.attributes.push(AttributeDraft {
                        namespace: String::new(),
                        local: String::new(),
                        prefix: None,
                        value: String::new(),
                    });
                }
            });
        }
        ui.collapsing("Definitions extensions", |ui| {
            let mut remove = None;
            for (i, e) in self.extensions.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.label(format!("Extension {}", i + 1));
                    optional_qname(ui, "Definition reference", &mut e.definition);
                    optional_bool(ui, "Must understand", &mut e.must_understand);
                    let mut rm = None;
                    for (j, d) in e.documentation.iter_mut().enumerate() {
                        ui.push_id(j, |ui| {
                            d.ui(ui);
                            if ui.button("Remove documentation").clicked() {
                                rm = Some(j);
                            }
                        });
                    }
                    if let Some(j) = rm {
                        e.documentation.remove(j);
                    }
                    if ui
                        .add_enabled(
                            e.documentation.len() < 256,
                            egui::Button::new("Add documentation"),
                        )
                        .clicked()
                    {
                        e.documentation.push(EntryDraft::empty());
                    }
                    if ui.button("Remove extension").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.extensions.remove(i);
            }
            if ui
                .add_enabled(
                    self.extensions.len() < 256,
                    egui::Button::new("Add extension"),
                )
                .clicked()
            {
                self.extensions.push(ExtensionDraft {
                    documentation: vec![],
                    definition: None,
                    must_understand: None,
                });
            }
        });
        ui.collapsing("Relationships", |ui| {
            let mut remove = None;
            for (i, r) in self.relationships.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.label(format!("Relationship {}", i + 1));
                    text(ui, "Relationship type", &mut r.value.relationship_type);
                    egui::ComboBox::from_label("Direction")
                        .selected_text(match r.value.direction {
                            None => "Unspecified",
                            Some(RelationshipDirection::None) => "None",
                            Some(RelationshipDirection::Forward) => "Forward",
                            Some(RelationshipDirection::Backward) => "Backward",
                            Some(RelationshipDirection::Both) => "Both",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut r.value.direction, None, "Unspecified");
                            for (dir, label) in [
                                (RelationshipDirection::None, "None"),
                                (RelationshipDirection::Forward, "Forward"),
                                (RelationshipDirection::Backward, "Backward"),
                                (RelationshipDirection::Both, "Both"),
                            ] {
                                ui.selectable_value(&mut r.value.direction, Some(dir), label);
                            }
                        });
                    for (label, refs) in [("Source", &mut r.sources), ("Target", &mut r.targets)] {
                        let mut rm = None;
                        for (j, q) in refs.iter_mut().enumerate() {
                            ui.push_id((label, j), |ui| {
                                text(ui, "Namespace", &mut q.namespace);
                                text(ui, "Local name", &mut q.local);
                                if ui.button(format!("Remove {label}")).clicked() {
                                    rm = Some(j);
                                }
                            });
                        }
                        if let Some(j) = rm {
                            refs.remove(j);
                        }
                        if ui
                            .add_enabled(
                                refs.len() < 256,
                                egui::Button::new(format!("Add {label}")),
                            )
                            .clicked()
                        {
                            refs.push(QNameDraft::default());
                        }
                    }
                    if ui.button("Remove relationship").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.relationships.remove(i);
            }
            if ui
                .add_enabled(
                    self.relationships.len() < 256,
                    egui::Button::new("Add relationship"),
                )
                .clicked()
            {
                self.relationships.push(RelationshipDraft {
                    value: DefinitionsRelationship {
                        id: None,
                        base: None,
                        relationship_type: String::new(),
                        direction: None,
                        sources: vec![],
                        targets: vec![],
                    },
                    sources: vec![],
                    targets: vec![],
                });
            }
        });
    }
}
impl ProcessCraft {
    pub(super) fn definitions_extra_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.definitions_extra_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit definitions extensions and relationships…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.definitions_extra_draft = Some(DefinitionsExtraDraft::new(document));
            }
            return;
        }
        let draft = self.definitions_extra_draft.as_mut().unwrap();
        draft.ui(ui);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply definitions details").clicked() {
            match draft.changes(document.id) {
                Ok(changes) => {
                    if self.ctl.mutate(changes) {
                        self.definitions_extra_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel definitions details").clicked() {
            self.definitions_extra_draft = None;
        }
    }
}
