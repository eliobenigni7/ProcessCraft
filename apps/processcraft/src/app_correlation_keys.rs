use super::callable::reference_list_ui;
use super::catalog::optional_text;
use super::event::{ReferenceDraft, local_choice_label};
use super::*;
use processcraft_core::bpmn_activity::CorrelationKey;
use processcraft_core::bpmn_collaboration::{ChoreographyActivity, ConversationNode};
#[derive(Clone)]
enum KeyOwner {
    Collaboration(Box<Collaboration>),
    Conversation(Box<Collaboration>, Box<ConversationNode>),
    Choreography(ElementSelector, Box<ChoreographyActivity>),
    Graph(ElementSelector, ElementSelector, Box<ChoreographyActivity>),
}
struct KeyDraft {
    value: CorrelationKey,
    properties: Vec<ReferenceDraft>,
}
pub(super) struct CorrelationKeysDraft {
    owner: KeyOwner,
    keys: Vec<KeyDraft>,
    error: String,
    generation: u64,
}
impl CorrelationKeysDraft {
    fn new(owner: KeyOwner, values: &[CorrelationKey], generation: u64) -> Self {
        Self {
            owner,
            generation,
            keys: values
                .iter()
                .map(|k| KeyDraft {
                    value: k.clone(),
                    properties: k.property_refs.iter().map(ReferenceDraft::new).collect(),
                })
                .collect(),
            error: String::new(),
        }
    }
    fn build(&self) -> Result<Vec<CorrelationKey>, String> {
        self.keys
            .iter()
            .map(|k| {
                let mut v = k.value.clone();
                v.property_refs = k
                    .properties
                    .iter()
                    .map(ReferenceDraft::build)
                    .collect::<Result<_, _>>()?;
                Ok(v)
            })
            .collect()
    }
}
impl ProcessCraft {
    pub(super) fn correlation_keys_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.correlation_keys_draft.is_none() {
            ui.menu_button("Edit correlation keys…", |ui| {
                for scope in processcraft_core::queries::choreography_scopes(document)
                    .into_iter()
                    .filter(|scope| scope.root.graph.is_some())
                {
                    for (i, activity) in scope.activities.iter().enumerate() {
                        let label = local_choice_label(
                            activity.name.as_deref(),
                            "Choreography activity",
                            &activity.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(
                                editable && !self.draft_dirty,
                                egui::Button::new(format!("{label} keys")),
                            )
                            .clicked()
                            && self.commit_draft()
                        {
                            self.correlation_keys_draft = Some(CorrelationKeysDraft::new(
                                KeyOwner::Graph(
                                    scope.root.id.selector(),
                                    scope.owner.clone(),
                                    Box::new((*activity).clone()),
                                ),
                                &activity.correlation_keys,
                                self.ctl.session.snapshot().generation,
                            ));
                            ui.close();
                        }
                    }
                }
                let roots = document
                    .collaboration
                    .iter()
                    .chain(document.additional_collaborations.iter())
                    .cloned()
                    .chain(
                        document
                            .choreographies
                            .iter()
                            .map(super::collaboration::choreography_view),
                    );
                for (index, root) in roots.enumerate() {
                    let label = local_choice_label(
                        root.name.as_deref(),
                        "Collaboration",
                        &root.id,
                        index + 1,
                    );
                    if ui
                        .add_enabled(
                            editable && !self.draft_dirty,
                            egui::Button::new(format!("{label} keys")),
                        )
                        .clicked()
                        && self.commit_draft()
                    {
                        self.correlation_keys_draft = Some(CorrelationKeysDraft::new(
                            KeyOwner::Collaboration(Box::new(root.clone())),
                            &root.correlation_keys,
                            self.ctl.session.snapshot().generation,
                        ));
                        ui.close();
                    }
                    for (i, node) in root.conversation_nodes.iter().enumerate() {
                        let label = local_choice_label(
                            node.name.as_deref(),
                            "Conversation",
                            &node.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(
                                editable && !self.draft_dirty,
                                egui::Button::new(format!("{label} keys")),
                            )
                            .clicked()
                            && self.commit_draft()
                        {
                            self.correlation_keys_draft = Some(CorrelationKeysDraft::new(
                                KeyOwner::Conversation(
                                    Box::new(root.clone()),
                                    Box::new(node.clone()),
                                ),
                                &node.correlation_keys,
                                self.ctl.session.snapshot().generation,
                            ));
                            ui.close();
                        }
                    }
                }
                for choreography in &document.choreographies {
                    for (i, activity) in choreography.activities.iter().enumerate() {
                        let label = local_choice_label(
                            activity.name.as_deref(),
                            "Choreography activity",
                            &activity.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            self.correlation_keys_draft = Some(CorrelationKeysDraft::new(
                                KeyOwner::Choreography(
                                    choreography.id.selector(),
                                    Box::new(activity.clone()),
                                ),
                                &activity.correlation_keys,
                                self.ctl.session.snapshot().generation,
                            ));
                            ui.close();
                        }
                    }
                }
            });
            return;
        }
        let draft = self.correlation_keys_draft.as_mut().unwrap();
        ui.strong("Correlation keys");
        let choices = super::process_activity::correlation_properties(document);
        let mut remove = None;
        let mut up = None;
        for (i, k) in draft.keys.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.label(format!("Key {}", i + 1));
                optional_text(ui, "Name", &mut k.value.name);
                reference_list_ui(ui, "Correlation property", &mut k.properties, &choices);
                if ui
                    .add_enabled(i > 0, egui::Button::new("Move key up"))
                    .clicked()
                {
                    up = Some(i);
                }
                if ui.button("Remove key").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            draft.keys.remove(i);
        } else if let Some(i) = up {
            draft.keys.swap(i, i - 1);
        }
        if ui
            .add_enabled(draft.keys.len() < 256, egui::Button::new("Add key"))
            .clicked()
        {
            draft.keys.push(KeyDraft {
                value: CorrelationKey {
                    id: None,
                    base: None,
                    name: None,
                    property_refs: vec![],
                },
                properties: vec![],
            });
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply correlation keys").clicked() {
            match draft.build() {
                Ok(keys) => {
                    if draft.generation != self.ctl.session.snapshot().generation {
                        draft.error = "The model changed while this draft was open. Cancel it and reopen the editor.".into();
                        return;
                    }
                    let mutation = match &draft.owner {
                        KeyOwner::Collaboration(root) => {
                            let mut updated = root.as_ref().clone();
                            updated.correlation_keys = keys;
                            super::collaboration::update_root(document, updated)
                        }
                        KeyOwner::Conversation(root, node) => {
                            let mut updated = root.as_ref().clone();
                            if let Some(current) = updated
                                .conversation_nodes
                                .iter_mut()
                                .find(|value| value.id == node.id)
                            {
                                current.correlation_keys = keys;
                            } else {
                                draft.error = "The conversation no longer exists.".into();
                                return;
                            }
                            super::collaboration::update_root(document, updated)
                        }
                        KeyOwner::Choreography(id, activity) => {
                            let mut activity = activity.as_ref().clone();
                            activity.correlation_keys = keys;
                            Mutation::UpdateChoreographyActivity {
                                document_id: document.id,
                                choreography_id: id.clone(),
                                activity,
                            }
                        }
                        KeyOwner::Graph(root, scope, activity) => {
                            let mut activity = activity.as_ref().clone();
                            activity.correlation_keys = keys;
                            Mutation::UpdateChoreographyFlowElement { document_id: document.id, choreography_id: root.clone(), scope_id: scope.clone(), element: processcraft_core::bpmn_collaboration::ChoreographyFlowElement::Activity(Box::new(activity)) }
                        }
                    };
                    if self
                        .ctl
                        .mutate_at_generation(draft.generation, vec![mutation])
                    {
                        self.correlation_keys_draft = None;
                        self.restore_selection();
                    }
                }
                Err(error) => draft.error = error,
            }
        }
        if ui.button("Cancel correlation keys").clicked() {
            self.correlation_keys_draft = None;
        }
    }
}
