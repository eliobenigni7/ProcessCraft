use super::documentation::EntryDraft;
use super::xml::{AttributeDraft, ContentDraft, bindings_ui};
use super::*;
use processcraft_core::{
    bpmn_metadata::{BaseExtras, BaseMetadata, ExtensionElements},
    bpmn_xml::{NamespaceBinding, XmlContent},
    metadata_read::OwnerMetadata,
};

pub(super) struct MetadataDraft {
    target: ElementSelector,
    legacy: bool,
    present: bool,
    namespaces: Vec<NamespaceBinding>,
    attributes: Vec<AttributeDraft>,
    extension: Option<ContentDraft>,
    docs: Vec<EntryDraft>,
    error: String,
}
impl MetadataDraft {
    fn new(target: ElementSelector, value: OwnerMetadata) -> Self {
        let (legacy, present, namespaces, attributes, extension, docs) = match value {
            OwnerMetadata::Legacy { extras, .. } => {
                let present = extras.is_some();
                let x = extras.unwrap_or_default();
                (
                    true,
                    present,
                    x.namespaces,
                    x.foreign_attributes,
                    x.extension_elements,
                    vec![],
                )
            }
            OwnerMetadata::Base(base) => {
                let present = base.is_some();
                let x = base.unwrap_or_default();
                (
                    false,
                    present,
                    x.namespaces,
                    x.foreign_attributes,
                    x.extension_elements,
                    x.documentation,
                )
            }
        };
        Self {
            target,
            legacy,
            present,
            namespaces,
            attributes: attributes.iter().map(AttributeDraft::new).collect(),
            extension: extension.map(|x| ContentDraft::new(&x.content)),
            docs: docs.iter().map(EntryDraft::new).collect(),
            error: String::new(),
        }
    }
    fn mutation(
        &self,
        document_id: processcraft_core::ids::DocumentId,
    ) -> Result<Mutation, String> {
        let attrs = self
            .attributes
            .iter()
            .map(AttributeDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        let extension_elements = self
            .extension
            .as_ref()
            .map(|x| {
                Ok::<_, String>(ExtensionElements {
                    content: x.build()?,
                })
            })
            .transpose()?;
        if self.legacy {
            Ok(Mutation::SetBaseExtras {
                document_id,
                element_id: self.target.clone(),
                extras: self.present.then(|| BaseExtras {
                    namespaces: self.namespaces.clone(),
                    foreign_attributes: attrs,
                    extension_elements,
                }),
            })
        } else {
            let documentation = self
                .docs
                .iter()
                .map(EntryDraft::build)
                .collect::<Result<_, _>>()?;
            Ok(Mutation::SetBaseMetadata {
                document_id,
                element_id: self.target.clone(),
                metadata: self.present.then(|| BaseMetadata {
                    namespaces: self.namespaces.clone(),
                    foreign_attributes: attrs,
                    extension_elements,
                    documentation,
                }),
            })
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.present, "Advanced metadata present");
        if !self.present {
            return;
        }
        if !self.legacy {
            ui.collapsing("Documentation entries", |ui| {
                let mut remove = None;
                let mut up = None;
                for (i, doc) in self.docs.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.label(format!("Entry {}", i + 1));
                        doc.ui(ui);
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move up"))
                            .clicked()
                        {
                            up = Some(i);
                        }
                        if ui.button("Remove entry").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.docs.remove(i);
                } else if let Some(i) = up {
                    self.docs.swap(i, i - 1);
                }
                if ui
                    .add_enabled(
                        self.docs.len() < 256,
                        egui::Button::new("Add documentation"),
                    )
                    .clicked()
                {
                    self.docs.push(EntryDraft::empty());
                }
            });
        }
        bindings_ui(ui, &mut self.namespaces);
        ui.collapsing("Foreign attributes", |ui| {
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
        let mut extension = self.extension.is_some();
        ui.checkbox(&mut extension, "Extension elements present");
        if extension {
            self.extension
                .get_or_insert_with(|| {
                    ContentDraft::new(&XmlContent {
                        namespaces: vec![],
                        nodes: vec![],
                    })
                })
                .ui(ui, 0);
        } else {
            self.extension = None;
        }
    }
}
impl ProcessCraft {
    pub(super) fn base_metadata_ui(
        &mut self,
        ui: &mut egui::Ui,
        target: ElementSelector,
        value: OwnerMetadata,
        editable: bool,
    ) {
        if self.metadata_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Edit advanced metadata…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.metadata_draft = Some(MetadataDraft::new(target, value));
            }
            return;
        }
        let draft = self.metadata_draft.as_mut().unwrap();
        if draft.target != target {
            return;
        }
        draft.ui(ui);
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply advanced metadata").clicked() && self.apply_metadata_draft().is_ok() {
            self.restore_selection();
        }
        if ui.button("Cancel advanced metadata").clicked() {
            self.metadata_draft = None;
        }
    }
    pub(super) fn apply_metadata_draft(&mut self) -> Result<(), String> {
        self.apply_typed_draft(
            |app| &mut app.metadata_draft,
            |draft, document| {
                draft
                    .mutation(document.id)
                    .map(|m| vec![m])
                    .inspect_err(|e| draft.error = e.clone())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_metadata::Documentation;
    use processcraft_core::bpmn_xml::MixedXml;
    #[test]
    fn base_draft_retains_ordered_documentation_and_extension_presence() {
        let base = BaseMetadata {
            documentation: vec![
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
            ],
            extension_elements: Some(ExtensionElements {
                content: XmlContent {
                    namespaces: vec![],
                    nodes: vec![],
                },
            }),
            ..Default::default()
        };
        let target = "Activity".parse().unwrap();
        let draft = MetadataDraft::new(target, OwnerMetadata::Base(Some(base.clone())));
        let id = ProcessCraft::default()
            .ctl
            .session
            .snapshot()
            .project
            .documents[0]
            .id;
        let Mutation::SetBaseMetadata { metadata, .. } = draft.mutation(id).unwrap() else {
            panic!("wrong mutation")
        };
        assert_eq!(metadata, Some(base));
    }
}
