use super::*;
use processcraft_core::bpmn_data::{DataAssignment, DataAssociation, DataAssociationKind};
use processcraft_core::bpmn_model::{EventIo, NodeDetails};
use processcraft_core::queries::ChoreographyScope;

pub(super) struct GraphDataDraft {
    root: ElementSelector,
    scope: ElementSelector,
    generation: u64,
    original: Option<DataAssociation>,
    owner: Option<ElementSelector>,
    kind: Option<DataAssociationKind>,
    sources: Vec<ElementSelector>,
    target: Option<ElementSelector>,
    transformation: Option<processcraft_core::bpmn_model::Expression>,
    assignments: Vec<DataAssignment>,
    error: String,
}

type EventChoices = (
    ElementSelector,
    String,
    Vec<ElementSelector>,
    Vec<ElementSelector>,
    Vec<ElementSelector>,
);
impl GraphDataDraft {
    fn build(
        &self,
        valid_sources: &[ElementSelector],
        valid_targets: &[ElementSelector],
    ) -> Result<DataAssociation, String> {
        let (Some(owner), Some(kind), Some(target)) =
            (self.owner.clone(), self.kind, self.target.clone())
        else {
            return Err("Select an event, direction and target.".into());
        };
        if self.sources.is_empty()
            || !self
                .sources
                .iter()
                .all(|source| valid_sources.contains(source))
            || !valid_targets.contains(&target)
        {
            return Err("Select valid sources and a target for this event and direction.".into());
        }
        let mut association = self.original.clone().unwrap_or_else(|| DataAssociation {
            id: Some(processcraft_core::ids::ElementIdentity::anonymous()),
            base: None,
            owner: owner.clone(),
            kind,
            sources: Vec::new(),
            target: target.clone(),
            transformation: None,
            assignments: Vec::new(),
        });
        association.owner = owner;
        association.kind = kind;
        association.sources = self.sources.clone();
        association.target = target;
        association.transformation = self.transformation.clone();
        association.assignments = self.assignments.clone();
        Ok(association)
    }
}

fn owner_choices(scope: &ChoreographyScope<'_>) -> Vec<EventChoices> {
    scope
        .events
        .iter()
        .filter_map(|event| {
            let Some(NodeDetails::Event {
                io: Some(io),
                properties,
                ..
            }) = &event.details
            else {
                return None;
            };
            let (inputs, outputs) = match io {
                EventIo::Catch { outputs, .. } => (
                    Vec::new(),
                    outputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(|id| id.selector()))
                        .collect(),
                ),
                EventIo::Throw { inputs, .. } => (
                    inputs
                        .iter()
                        .filter_map(|port| port.id.as_ref().map(|id| id.selector()))
                        .collect(),
                    Vec::new(),
                ),
            };
            Some((
                event.id.selector(),
                super::event::local_choice_label(event.name.as_deref(), "Event", &event.id, 1),
                inputs,
                outputs,
                properties
                    .iter()
                    .filter_map(|property| property.id.as_ref().map(|id| id.selector()))
                    .collect(),
            ))
        })
        .collect()
}

/// Valid sources and targets of `draft` for its owning event and direction.
fn endpoints(
    document: &BpmnDocument,
    scope: &ChoreographyScope<'_>,
    draft: &GraphDataDraft,
) -> (Vec<ElementSelector>, Vec<ElementSelector>) {
    let owners = owner_choices(scope);
    let owner = owners
        .iter()
        .find(|owner| Some(&owner.0) == draft.owner.as_ref());
    let mut local = document
        .data_stores
        .iter()
        .filter_map(|store| store.id.as_ref().map(|id| id.selector()))
        .collect::<Vec<_>>();
    if let Some((_, _, _, _, properties)) = owner {
        local.extend(properties.iter().cloned());
    }
    let inputs = owner.map_or(Vec::new(), |owner| owner.2.clone());
    let outputs = owner.map_or(Vec::new(), |owner| owner.3.clone());
    match draft.kind {
        Some(DataAssociationKind::Input) => (local, inputs),
        Some(DataAssociationKind::Output) => (outputs, local),
        None => (Vec::new(), Vec::new()),
    }
}

fn label(document: &BpmnDocument, id: &ElementSelector) -> String {
    processcraft_core::queries::element_name(document, id)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            processcraft_core::queries::element_identity(document, id)
                .and_then(|identity| identity.xml_id())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "Unnamed item".into())
}

fn association_identity_path(
    document: &BpmnDocument,
    root: &ElementSelector,
    scope: &ElementSelector,
    index: usize,
) -> Option<String> {
    use processcraft_core::bpmn_collaboration::{
        ChoreographyActivityDetails, ChoreographyFlowElement, ChoreographyGraph,
    };
    fn visit(
        graph: &ChoreographyGraph,
        owner: &ElementSelector,
        target: &ElementSelector,
        index: usize,
        path: String,
        depth: usize,
    ) -> Option<String> {
        if depth > 256 {
            return None;
        }
        if owner == target {
            return (index < graph.data_associations.len())
                .then(|| format!("{path}/data_associations/{index}/id"));
        }
        for (position, element) in graph.flow_elements.iter().enumerate() {
            if let ChoreographyFlowElement::Activity(activity) = element
                && let Some(ChoreographyActivityDetails::Sub { graph, .. }) = &activity.details
            {
                let nested = format!("{path}/flow_elements/{position}/value/details/value/graph");
                if let Some(found) = visit(
                    graph,
                    &activity.id.selector(),
                    target,
                    index,
                    nested,
                    depth + 1,
                ) {
                    return Some(found);
                }
            }
        }
        None
    }
    let (root_index, choreography) = document
        .choreographies
        .iter()
        .enumerate()
        .find(|(_, choreography)| choreography.id == *root)?;
    visit(
        choreography.graph.as_ref()?,
        root,
        scope,
        index,
        format!("/choreographies/{root_index}/graph"),
        0,
    )
}

impl ProcessCraft {
    pub(super) fn graph_data_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        scope: &ChoreographyScope<'_>,
        editable: bool,
        generation: u64,
    ) {
        ui.collapsing("Event data associations", |ui| {
            if self.graph_data_draft.is_none() {
                for (index, association) in scope.data_associations.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "Association {} · {} · {}",
                                index + 1,
                                label(document, &association.owner),
                                super::data::data_kind_label(association.kind)
                            ));
                            if let Some(id) = &association.id {
                                if ui
                                    .add_enabled(
                                        !self.has_drafts(),
                                        egui::Button::new("Select association properties"),
                                    )
                                    .clicked()
                                    && self.prepare_navigation()
                                {
                                    self.select(Some(id.selector()));
                                }
                                if ui
                                    .add_enabled(
                                        editable && !self.has_drafts(),
                                        egui::Button::new("Edit association…"),
                                    )
                                    .clicked()
                                    && self.commit_draft()
                                {
                                    if self.ctl.session.snapshot().generation != generation {
                                        self.ctl.error =
                                            "The model changed; reopen this scope.".into();
                                        return;
                                    }
                                    self.graph_data_draft = Some(GraphDataDraft {
                                        root: scope.root.id.selector(),
                                        scope: scope.owner.clone(),
                                        generation,
                                        original: Some((*association).clone()),
                                        owner: Some(association.owner.clone()),
                                        kind: Some(association.kind),
                                        sources: association.sources.clone(),
                                        target: Some(association.target.clone()),
                                        transformation: association.transformation.clone(),
                                        assignments: association.assignments.clone(),
                                        error: String::new(),
                                    });
                                }
                                let selector = id.selector();
                                if ui
                                    .add_enabled(
                                        editable && !self.has_drafts(),
                                        egui::Button::new("Delete association"),
                                    )
                                    .clicked()
                                    && self.prepare_navigation()
                                {
                                    if self.ctl.session.snapshot().generation != generation {
                                        self.ctl.error =
                                            "The model changed; reopen this scope.".into();
                                        return;
                                    }
                                    if self.ctl.mutate_at_generation(
                                        generation,
                                        vec![Mutation::DeleteChoreographyDataAssociation {
                                            document_id: document.id,
                                            choreography_id: scope.root.id.selector(),
                                            scope_id: scope.owner.clone(),
                                            association_id: selector.clone(),
                                        }],
                                    ) {
                                        if self.selected.as_ref() == Some(&selector) {
                                            self.select(None);
                                        } else {
                                            self.restore_selection();
                                        }
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        editable && !self.has_drafts() && index > 0,
                                        egui::Button::new("Move earlier"),
                                    )
                                    .clicked()
                                    && self.prepare_navigation()
                                {
                                    if self.ctl.session.snapshot().generation != generation {
                                        self.ctl.error =
                                            "The model changed; reopen this scope.".into();
                                        return;
                                    }
                                    if self.ctl.mutate_at_generation(
                                        generation,
                                        vec![Mutation::ReorderChoreographyDataAssociation {
                                            document_id: document.id,
                                            choreography_id: scope.root.id.selector(),
                                            scope_id: scope.owner.clone(),
                                            association_id: selector,
                                            index: index - 1,
                                        }],
                                    ) {
                                        self.restore_selection();
                                    }
                                }
                            } else {
                                ui.label("No editing identity assigned.");
                                if ui
                                    .add_enabled(
                                        editable && !self.has_drafts(),
                                        egui::Button::new("Assign editing identity"),
                                    )
                                    .clicked()
                                    && self.prepare_navigation()
                                {
                                    if self.ctl.session.snapshot().generation != generation {
                                        self.ctl.error =
                                            "The model changed; reopen this scope.".into();
                                    } else if let Some(model_path) = association_identity_path(
                                        document,
                                        &scope.root.id.selector(),
                                        &scope.owner,
                                        index,
                                    ) {
                                        if self.ctl.mutate_at_generation(
                                            generation,
                                            vec![Mutation::PromoteElementIdentity {
                                                document_id: document.id,
                                                model_path,
                                                xml_id: None,
                                            }],
                                        ) {
                                            self.restore_selection();
                                        }
                                    } else {
                                        self.ctl.error =
                                            "The association is no longer available in this scope."
                                                .into();
                                    }
                                }
                            }
                        });
                    });
                }
                if ui
                    .add_enabled(
                        editable && !self.has_drafts(),
                        egui::Button::new("Add event data association…"),
                    )
                    .clicked()
                    && self.commit_draft()
                {
                    if self.ctl.session.snapshot().generation != generation {
                        self.ctl.error = "The model changed; reopen this scope.".into();
                        return;
                    }
                    let owners = owner_choices(scope);
                    let selected_owner = self
                        .selected
                        .as_ref()
                        .filter(|id| owners.iter().any(|owner| &owner.0 == *id))
                        .cloned();
                    self.graph_data_draft = Some(GraphDataDraft {
                        root: scope.root.id.selector(),
                        scope: scope.owner.clone(),
                        generation,
                        original: None,
                        owner: selected_owner,
                        kind: None,
                        sources: Vec::new(),
                        target: None,
                        transformation: None,
                        assignments: Vec::new(),
                        error: String::new(),
                    });
                }
                return;
            }
            let draft = self.graph_data_draft.as_mut().expect("open draft");
            if draft.root != scope.root.id.selector() || draft.scope != scope.owner {
                ui.label("An association draft is open in another scope.");
                return;
            }
            ui.strong("Event data association");
            let owners = owner_choices(scope);
            egui::ComboBox::from_label("Owning event")
                .selected_text(
                    draft
                        .owner
                        .as_ref()
                        .map_or_else(|| "Select event".into(), |id| label(document, id)),
                )
                .show_ui(ui, |ui| {
                    for (id, name, ..) in &owners {
                        ui.selectable_value(&mut draft.owner, Some(id.clone()), name);
                    }
                });
            egui::ComboBox::from_label("Direction")
                .selected_text(match draft.kind {
                    Some(DataAssociationKind::Input) => "Input",
                    Some(DataAssociationKind::Output) => "Output",
                    None => "Select direction",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.kind, Some(DataAssociationKind::Input), "Input");
                    ui.selectable_value(
                        &mut draft.kind,
                        Some(DataAssociationKind::Output),
                        "Output",
                    );
                });
            let (sources, targets) = endpoints(document, scope, draft);
            let (sources, targets) = (sources.as_slice(), targets.as_slice());
            ui.label("Sources");
            let mut all_sources = draft.sources.clone();
            for id in sources {
                if !all_sources.contains(id) {
                    all_sources.push(id.clone());
                }
            }
            let mut up = None;
            for (index, id) in all_sources.iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        let mut selected = draft.sources.contains(id);
                        if ui.checkbox(&mut selected, label(document, id)).changed() {
                            if selected {
                                draft.sources.push(id.clone());
                            } else {
                                draft.sources.retain(|source| source != id);
                            }
                        }
                        if let Some(position) = draft.sources.iter().position(|source| source == id)
                            && ui
                                .add_enabled(position > 0, egui::Button::new("Move up"))
                                .clicked()
                        {
                            up = Some(position);
                        }
                    });
                });
            }
            if let Some(position) = up {
                draft.sources.swap(position, position - 1);
            }
            egui::ComboBox::from_label("Target")
                .selected_text(
                    draft
                        .target
                        .as_ref()
                        .map_or_else(|| "Select target".into(), |id| label(document, id)),
                )
                .show_ui(ui, |ui| {
                    for id in targets {
                        ui.selectable_value(
                            &mut draft.target,
                            Some(id.clone()),
                            label(document, id),
                        );
                    }
                });
            super::inspector::optional_expression_ui_with_default(
                ui,
                "Transformation",
                &mut draft.transformation,
                true,
                &mut self.expression_draft,
                super::inspector::ExpressionDefault::ImplicitFormal,
            );
            let mut remove = None;
            for (index, assignment) in draft.assignments.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.label(format!("Assignment {}", index + 1));
                    super::inspector::expression_ui(
                        ui,
                        &format!("From {index}"),
                        &mut assignment.from,
                        true,
                        &mut self.expression_draft,
                        super::inspector::ExpressionDefault::Base,
                    );
                    super::inspector::expression_ui(
                        ui,
                        &format!("To {index}"),
                        &mut assignment.to,
                        true,
                        &mut self.expression_draft,
                        super::inspector::ExpressionDefault::Base,
                    );
                    if ui
                        .add_enabled(
                            self.expression_draft.is_none(),
                            egui::Button::new("Remove assignment"),
                        )
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                draft.assignments.remove(index);
            }
            if ui
                .add_enabled(
                    self.expression_draft.is_none() && draft.assignments.len() < 256,
                    egui::Button::new("Add assignment"),
                )
                .clicked()
            {
                draft.assignments.push(DataAssignment {
                    id: None,
                    base: None,
                    from: String::new().into(),
                    to: String::new().into(),
                });
            }
            if !draft.error.is_empty() {
                ui.label(&draft.error);
            }
            let mut close = false;
            let mut apply = false;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.expression_draft.is_none(),
                        egui::Button::new("Apply association"),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .add_enabled(
                        self.expression_draft.is_none(),
                        egui::Button::new("Cancel association"),
                    )
                    .clicked()
                {
                    close = true;
                }
            });
            if close {
                self.graph_data_draft = None;
            } else if apply && self.apply_graph_data_draft().is_ok() {
                self.restore_selection();
            }
        });
    }
    /// A new association where nothing was chosen yet.
    pub(super) fn graph_data_draft_untouched(&self) -> bool {
        self.graph_data_draft.as_ref().is_some_and(|d| {
            d.original.is_none() && d.kind.is_none() && d.sources.is_empty() && d.target.is_none()
        })
    }
    pub(super) fn apply_graph_data_draft(&mut self) -> Result<(), String> {
        let generation = self.ctl.session.snapshot().generation;
        self.apply_typed_draft(
            |app| &mut app.graph_data_draft,
            |draft, document| {
                if generation != draft.generation {
                    draft.error = "The model changed; cancel and reopen this draft.".into();
                    return Err(draft.error.clone());
                }
                let Some(scope) = processcraft_core::queries::choreography_scopes(document)
                    .into_iter()
                    .find(|scope| scope.root.id == draft.root && scope.owner == draft.scope)
                else {
                    draft.error = "The choreography scope is no longer available.".into();
                    return Err(draft.error.clone());
                };
                let (sources, targets) = endpoints(document, &scope, draft);
                let association = draft
                    .build(&sources, &targets)
                    .inspect_err(|e| draft.error = e.clone())?;
                Ok(vec![if draft.original.is_some() {
                    Mutation::UpdateChoreographyDataAssociation {
                        document_id: document.id,
                        choreography_id: draft.root.clone(),
                        scope_id: draft.scope.clone(),
                        association,
                    }
                } else {
                    Mutation::CreateChoreographyDataAssociation {
                        document_id: document.id,
                        choreography_id: draft.root.clone(),
                        scope_id: draft.scope.clone(),
                        index: scope.data_associations.len(),
                        association,
                    }
                }])
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn association_draft_preserves_structured_expressions_and_identity() {
        use processcraft_core::bpmn_model::{
            Expression, ExpressionBody, ExpressionData, ExpressionKind, ExpressionPart,
        };
        let mixed = Expression::Structured(ExpressionData {
            id: None,
            kind: ExpressionKind::Base {
                explicit_type: false,
            },
            text: ExpressionBody::Mixed {
                parts: vec![
                    ExpressionPart::Text {
                        text: "expression".into(),
                    },
                    ExpressionPart::Comment {
                        text: "keep".into(),
                    },
                ],
            },
            namespaces: vec![("p".into(), "urn:test".into())],
            foreign_attributes: vec![],
        });
        let original = DataAssociation {
            id: Some("Assoc".parse().unwrap()),
            base: Some(Default::default()),
            owner: "Event".parse().unwrap(),
            kind: DataAssociationKind::Output,
            sources: vec!["Output".parse().unwrap()],
            target: "Property".parse().unwrap(),
            transformation: Some(mixed.clone()),
            assignments: vec![DataAssignment {
                id: None,
                base: Some(Default::default()),
                from: mixed.clone(),
                to: "target".into(),
            }],
        };
        let draft = GraphDataDraft {
            root: "Root".parse().unwrap(),
            scope: "Sub".parse().unwrap(),
            generation: 4,
            original: Some(original.clone()),
            owner: Some(original.owner.clone()),
            kind: Some(original.kind),
            sources: original.sources.clone(),
            target: Some(original.target.clone()),
            transformation: original.transformation.clone(),
            assignments: original.assignments.clone(),
            error: String::new(),
        };
        assert_eq!(
            draft
                .build(&original.sources, std::slice::from_ref(&original.target))
                .unwrap(),
            original
        );
        assert!(
            draft
                .build(&[], std::slice::from_ref(&original.target))
                .is_err()
        );
    }
    #[test]
    fn invalid_association_draft_blocks_navigation_and_history_until_cancelled() {
        let draft = |kind| GraphDataDraft {
            root: "Root".parse().unwrap(),
            scope: "Sub".parse().unwrap(),
            generation: 0,
            original: None,
            owner: None,
            kind,
            sources: vec![],
            target: None,
            transformation: None,
            assignments: vec![],
            error: String::new(),
        };
        // A new association where nothing was chosen yet just closes.
        let mut app = ProcessCraft {
            graph_data_draft: Some(draft(None)),
            ..Default::default()
        };
        assert!(app.prepare_navigation(), "{}", app.ctl.error);
        assert!(app.graph_data_draft.is_none());
        // A started but invalid one is refused with its reason and stays open.
        app.graph_data_draft = Some(draft(Some(DataAssociationKind::Input)));
        assert!(app.has_drafts());
        assert!(!app.prepare_navigation());
        assert!(
            app.ctl
                .error
                .contains("choreography data association draft"),
            "{}",
            app.ctl.error
        );
        let generation = app.ctl.session.snapshot().generation;
        app.history(Request::Undo);
        assert_eq!(app.ctl.session.snapshot().generation, generation);
        assert!(app.graph_data_draft.is_some());
        app.graph_data_draft = None;
        assert!(app.prepare_navigation());
    }
    use processcraft_core::bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyFlowElement,
        ChoreographyGraph,
    };

    #[test]
    fn anonymous_association_path_tracks_its_real_nested_scope() {
        let mut document = crate::controller::blank_document();
        let association = DataAssociation {
            id: None,
            base: None,
            owner: "Event".parse().unwrap(),
            kind: DataAssociationKind::Output,
            sources: vec!["Output".parse().unwrap()],
            target: "Property".parse().unwrap(),
            transformation: None,
            assignments: vec![],
        };
        let sub = ChoreographyActivity {
            id: "Sub".parse().unwrap(),
            name: None,
            base: None,
            flow_metadata: None,
            correlation_keys: vec![],
            participant_refs: vec![],
            initiating_participant: None,
            message_flow_refs: vec![],
            loop_type: None,
            details: Some(ChoreographyActivityDetails::Sub {
                graph: Box::new(ChoreographyGraph {
                    data_associations: vec![association],
                    ..Default::default()
                }),
                artifacts: vec![],
            }),
        };
        document.choreographies.push(Choreography {
            id: "Root".parse().unwrap(),
            name: None,
            kind: None,
            graph: Some(ChoreographyGraph {
                flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(sub))],
                ..Default::default()
            }),
            extras: None,
            base: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            participants: vec![],
            message_flows: vec![],
            activities: vec![],
            sequence_flows: vec![],
        });
        let path = association_identity_path(
            &document,
            &"Root".parse().unwrap(),
            &"Sub".parse().unwrap(),
            0,
        )
        .unwrap();
        let parent = path.strip_suffix("/id").unwrap();
        let json = serde_json::to_value(&document).unwrap();
        assert_eq!(
            json.pointer(parent)
                .and_then(|value| value.get("owner"))
                .and_then(serde_json::Value::as_str),
            Some("Event")
        );
        assert!(
            association_identity_path(
                &document,
                &"Root".parse().unwrap(),
                &"Other".parse().unwrap(),
                0
            )
            .is_none()
        );
    }
}
