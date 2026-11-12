use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{CollaborationArtifact, CollaborationExtras};
use processcraft_core::bpmn_model::{Group, NodeDetails};

struct ArtifactDraft {
    value: CollaborationArtifact,
    source: Option<ReferenceDraft>,
    target: Option<ReferenceDraft>,
}
impl ArtifactDraft {
    fn new(value: &CollaborationArtifact) -> Self {
        let (source, target) = match value {
            CollaborationArtifact::Group(value) => (
                value.category_value_ref.as_ref().map(ReferenceDraft::new),
                None,
            ),
            CollaborationArtifact::Association(value) => (
                value.source.as_ref().map(ReferenceDraft::new),
                value.target.as_ref().map(ReferenceDraft::new),
            ),
            _ => (None, None),
        };
        Self {
            value: value.clone(),
            source,
            target,
        }
    }
    fn build(&self) -> Result<CollaborationArtifact, String> {
        let mut value = self.value.clone();
        match &mut value {
            CollaborationArtifact::Association(flow) => {
                flow.source = build_ref(&self.source)?;
                flow.target = build_ref(&self.target)?;
            }
            CollaborationArtifact::Group(group) => {
                group.category_value_ref = build_ref(&self.source)?;
            }
            _ => {}
        }
        Ok(value)
    }
}
pub(super) struct ArtifactsDraft {
    generation: u64,
    root: ElementSelector,
    extras: Option<CollaborationExtras>,
    values: Vec<ArtifactDraft>,
}
impl ArtifactsDraft {
    fn new(root: &Collaboration, generation: u64) -> Self {
        Self {
            generation,
            root: root.id.selector(),
            extras: root.extras.clone(),
            values: root.extras.as_ref().map_or_else(Vec::new, |extras| {
                extras.artifacts.iter().map(ArtifactDraft::new).collect()
            }),
        }
    }
    fn build(&self) -> Result<Option<CollaborationExtras>, String> {
        let values = self
            .values
            .iter()
            .map(ArtifactDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        if self.extras.is_none() && values.is_empty() {
            return Ok(None);
        }
        let mut extras = self.extras.clone().unwrap_or_default();
        extras.artifacts = values;
        Ok(Some(extras))
    }
}
impl ProcessCraft {
    pub(super) fn collaboration_artifacts_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
    ) {
        ui.collapsing("Collaboration artifacts", |ui| {
            if self.collaboration.artifacts.is_none() {
                if let Some(extras) = &root.extras { for (index, artifact) in extras.artifacts.iter().enumerate() {
                    let (identity, kind) = match artifact { CollaborationArtifact::Group(value) => (&value.id, "Group"), CollaborationArtifact::TextAnnotation(value) => (&value.id, "Annotation"), CollaborationArtifact::Association(value) => (&value.id, "Association") };
                    let label = super::event::local_choice_label(None, kind, identity, index + 1);
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {label} properties"))).clicked() && self.commit_draft() { self.select(Some(identity.selector())); }
                } }
                if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Edit artifacts…")).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id); self.collaboration.artifacts = Some(ArtifactsDraft::new(root, generation)); }
                return;
            }
            let choices: Vec<_> = root.participants.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1))).chain(root.conversation_nodes.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1)))).chain(root.extras.iter().flat_map(|extras| extras.artifacts.iter()).enumerate().filter_map(|(i, value)| { let (identity, kind) = match value { CollaborationArtifact::Group(value) => (&value.id, "Group"), CollaborationArtifact::TextAnnotation(value) => (&value.id, "Annotation"), CollaborationArtifact::Association(_) => return None }; Some((identity.selector(), super::event::local_choice_label(None, kind, identity, i + 1))) })).collect();
            let categories: Vec<_> = document.categories.iter().flat_map(|category| category.values.iter()).enumerate().map(|(i, value)| (value.id.selector(), super::event::local_choice_label(value.value.as_deref(), "Category value", &value.id, i + 1))).collect();
            let mut draft = self.collaboration.artifacts.take().expect("open"); let mut close = false; let mut remove = None; let mut up = None;
            for (index, artifact) in draft.values.iter_mut().enumerate() {
                ui.push_id(index, |ui| { ui.separator(); match &mut artifact.value {
                    CollaborationArtifact::Group(_) => {
                        ui.strong(format!("Group {}", index + 1));
                        ref_ui(ui, "Category value", &mut artifact.source, &categories);
                    },
                    CollaborationArtifact::TextAnnotation(node) => {
                        ui.strong(format!("Text annotation {}", index + 1));
                        if let Some(NodeDetails::TextAnnotation { text_format, text }) = node.details.as_mut() {
                            super::catalog::optional_text(ui, "Text format", text_format);
                            match text { Some(content) if content.plain_text().is_none() => { ui.label("This annotation has structured content. Use the advanced content editor to preserve it."); }, _ => {
                                let mut present = text.is_some(); let label = ui.checkbox(&mut present, "Text"); if present { let content = text.get_or_insert_with(|| String::new().into()); if let processcraft_core::bpmn_xml::MixedXml::Text(value) = content { ui.text_edit_multiline(value).labelled_by(label.id); } } else { *text = None; }
                            } }
                        } else { ui.label("This annotation uses a legacy body; select it on the diagram to edit the body safely."); }
                    },
                    CollaborationArtifact::Association(_) => { ui.strong(format!("Association {}", index + 1)); ref_ui(ui, "Source", &mut artifact.source, &choices); ref_ui(ui, "Target", &mut artifact.target, &choices); },
                }
                ui.horizontal(|ui| { if ui.add_enabled(index > 0, egui::Button::new("Move artifact up")).clicked() { up = Some(index); }
                if ui.button("Remove artifact").clicked() { remove = Some(index); } }); });
            }
            if let Some(index) = remove { draft.values.remove(index); } else if let Some(index) = up { draft.values.swap(index, index - 1); }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(draft.values.len() < 256, egui::Button::new("Add group")).clicked() { draft.values.push(ArtifactDraft::new(&CollaborationArtifact::Group(Group { id: processcraft_core::ids::ElementIdentity::anonymous(), base: None, category_value_ref: None }))); }
                if ui.add_enabled(draft.values.len() < 256, egui::Button::new("Add annotation")).clicked() { draft.values.push(ArtifactDraft::new(&CollaborationArtifact::TextAnnotation(Box::new(Node { id: processcraft_core::ids::ElementIdentity::anonymous(), name: None, documentation: String::new().into(), base_extras: None, kind: NodeKind::TextAnnotation, details: Some(NodeDetails::TextAnnotation { text_format: None, text: None }), loop_characteristics: None, io_specification: None, activity: None, owner: String::new(), contact: String::new(), flow_metadata: None, item_aware: None })))); }
                if ui.add_enabled(draft.values.len() < 256, egui::Button::new("Add association")).clicked() { draft.values.push(ArtifactDraft::new(&CollaborationArtifact::Association(Box::new(Flow { id: processcraft_core::ids::ElementIdentity::anonymous(), kind: FlowKind::Association, source: None, target: None, name: None, documentation: String::new().into(), base_extras: None, condition: None, association_direction: None, flow_metadata: None, message_ref: None })))); }
            });
            ui.horizontal(|ui| {
                if ui.button("Apply artifacts").clicked() { if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); } else { match draft.build() { Ok(extras) => { let mut updated = root.clone(); updated.extras = extras; if self.ctl.mutate(vec![super::collaboration::update_root(document, updated)]) { close = true; } }, Err(error) => self.ctl.error = error } } }
                if ui.button("Discard artifacts draft").clicked() { close = true; }
            });
            if !close { self.collaboration.artifacts = Some(draft); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::bpmn_activity::ModelReference;
    use processcraft_core::bpmn_model::QNameRef;
    #[test]
    fn artifact_draft_preserves_external_category_and_other_settings() {
        let category = ModelReference::Qualified(QNameRef {
            namespace: "urn:catalog".into(),
            local: "Category".parse().unwrap(),
        });
        let root = Collaboration {
            kind: None,
            extras: Some(CollaborationExtras {
                is_closed: Some(false),
                artifacts: vec![CollaborationArtifact::Group(Group {
                    id: "Group".parse().unwrap(),
                    base: None,
                    category_value_ref: Some(category.clone()),
                })],
                ..Default::default()
            }),
            correlation_keys: vec![],
            base: None,
            name: None,
            id: "Root".parse().unwrap(),
            participants: vec![],
            flows: vec![],
            conversation_nodes: vec![],
            conversation_links: vec![],
        };
        let built = ArtifactsDraft::new(&root, 1).build().unwrap().unwrap();
        assert_eq!(built.is_closed, Some(false));
        let CollaborationArtifact::Group(group) = &built.artifacts[0] else {
            panic!("expected group")
        };
        assert_eq!(group.category_value_ref.as_ref(), Some(&category));
    }
}
