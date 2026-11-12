use super::xml::{AttributeDraft, ContentDraft, bindings_ui};
use processcraft_core::bpmn_xml::{NamespaceBinding, XmlContent};
use processcraft_core::model::DiElementMetadata;
#[derive(Clone)]
pub(super) struct DiMetadataDraft {
    present: bool,
    namespaces: Vec<NamespaceBinding>,
    attributes: Vec<AttributeDraft>,
    extension: Option<ContentDraft>,
}
impl DiMetadataDraft {
    pub(super) fn new(value: &Option<DiElementMetadata>) -> Self {
        let present = value.is_some();
        let value = value.clone().unwrap_or_default();
        Self {
            present,
            namespaces: value.namespaces,
            attributes: value
                .foreign_attributes
                .iter()
                .map(AttributeDraft::new)
                .collect(),
            extension: value.extension.as_ref().map(ContentDraft::new),
        }
    }
    pub(super) fn build(&self) -> Result<Option<DiElementMetadata>, String> {
        if !self.present {
            return Ok(None);
        }
        Ok(Some(DiElementMetadata {
            namespaces: self.namespaces.clone(),
            foreign_attributes: self
                .attributes
                .iter()
                .map(AttributeDraft::build)
                .collect::<Result<_, _>>()?,
            extension: self
                .extension
                .as_ref()
                .map(ContentDraft::build)
                .transpose()?,
        }))
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.present, "Diagram metadata present");
        if !self.present {
            return;
        }
        bindings_ui(ui, &mut self.namespaces);
        ui.collapsing("Foreign attributes", |ui| {
            let mut remove = None;
            for (i, attribute) in self.attributes.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    attribute.ui(ui);
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
        let mut present = self.extension.is_some();
        ui.checkbox(&mut present, "Diagram extension present");
        if present {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_round_trips_and_invalid_attribute_stays_in_draft() {
        let original = Some(DiElementMetadata {
            namespaces: vec![NamespaceBinding {
                prefix: "ex".into(),
                uri: "urn:example".into(),
            }],
            foreign_attributes: vec![processcraft_core::bpmn_xml::XmlAttribute {
                namespace: "urn:example".into(),
                local: "flag".parse().unwrap(),
                prefix: Some("ex".parse().unwrap()),
                value: String::new(),
            }],
            extension: Some(XmlContent {
                namespaces: vec![],
                nodes: vec![],
            }),
        });
        let mut draft = DiMetadataDraft::new(&original);
        assert_eq!(draft.build().unwrap(), original);
        draft.attributes[0].local = "invalid name".into();
        assert!(draft.build().is_err());
        assert_eq!(draft.attributes[0].local, "invalid name");
    }
}
