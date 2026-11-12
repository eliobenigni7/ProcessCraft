use super::event::{ReferenceDraft, build_ref, ref_ui};
use super::*;
use processcraft_core::bpmn_collaboration::{CollaborationArtifact, CollaborationExtras};
use processcraft_core::bpmn_model::{Group, NodeDetails};

struct ArtifactDraft {
    value: CollaborationArtifact,
    source: Option<ReferenceDraft>,
    target: Option<ReferenceDraft>,
    structured_body: Option<super::xml::MixedDraft>,
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
            structured_body: match value {
                CollaborationArtifact::TextAnnotation(node) => match &node.details {
                    Some(NodeDetails::TextAnnotation {
                        text: Some(content @ processcraft_core::bpmn_xml::MixedXml::Structured(_)),
                        ..
                    }) => Some(super::xml::MixedDraft::new(content)),
                    _ => None,
                },
                _ => None,
            },
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
            CollaborationArtifact::TextAnnotation(node) => {
                if let (Some(body), Some(NodeDetails::TextAnnotation { text, .. })) =
                    (&self.structured_body, &mut node.details)
                {
                    *text = Some(body.build()?);
                }
            }
        }
        Ok(value)
    }
}
pub(super) struct ArtifactsDraft {
    generation: u64,
    root: ElementSelector,
    scope: Option<ElementSelector>,
    original: Vec<ElementSelector>,
    extras: Option<CollaborationExtras>,
    values: Vec<ArtifactDraft>,
}
impl ArtifactsDraft {
    fn new(root: &Collaboration, generation: u64) -> Self {
        Self {
            generation,
            root: root.id.selector(),
            scope: None,
            original: Vec::new(),
            extras: root.extras.clone(),
            values: root.extras.as_ref().map_or_else(Vec::new, |extras| {
                extras.artifacts.iter().map(ArtifactDraft::new).collect()
            }),
        }
    }
    fn scoped(
        root: &Collaboration,
        scope: &processcraft_core::queries::ChoreographyScope<'_>,
        generation: u64,
    ) -> Self {
        Self {
            generation,
            root: root.id.selector(),
            scope: Some(scope.owner.clone()),
            original: scope
                .artifacts
                .iter()
                .map(|artifact| artifact_id(artifact))
                .collect(),
            extras: None,
            values: scope
                .artifacts
                .iter()
                .map(|artifact| ArtifactDraft::new(artifact))
                .collect(),
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
fn artifact_id(artifact: &CollaborationArtifact) -> ElementSelector {
    match artifact {
        CollaborationArtifact::Group(value) => value.id.selector(),
        CollaborationArtifact::TextAnnotation(value) => value.id.selector(),
        CollaborationArtifact::Association(value) => value.id.selector(),
    }
}
fn scoped_artifact_mutations(
    document_id: processcraft_core::ids::DocumentId,
    root: &ElementSelector,
    scope: &ElementSelector,
    original: &[ElementSelector],
    desired: Vec<CollaborationArtifact>,
) -> Vec<Mutation> {
    let desired_ids = desired.iter().map(artifact_id).collect::<Vec<_>>();
    let mut current = original.to_vec();
    let mut mutations = Vec::new();
    for id in original.iter().rev().filter(|id| !desired_ids.contains(id)) {
        mutations.push(Mutation::DeleteChoreographyArtifact {
            document_id,
            choreography_id: root.clone(),
            scope_id: scope.clone(),
            artifact_id: id.clone(),
        });
        current.retain(|value| value != id);
    }
    for (index, artifact) in desired.into_iter().enumerate() {
        let id = artifact_id(&artifact);
        if let Some(position) = current.iter().position(|value| value == &id) {
            mutations.push(Mutation::UpdateChoreographyArtifact {
                document_id,
                choreography_id: root.clone(),
                scope_id: scope.clone(),
                artifact,
            });
            if position != index {
                mutations.push(Mutation::ReorderChoreographyArtifact {
                    document_id,
                    choreography_id: root.clone(),
                    scope_id: scope.clone(),
                    artifact_id: id.clone(),
                    index,
                });
                current.remove(position);
                current.insert(index, id);
            }
        } else {
            mutations.push(Mutation::CreateChoreographyArtifact {
                document_id,
                choreography_id: root.clone(),
                scope_id: scope.clone(),
                index,
                artifact,
            });
            current.insert(index, id);
        }
    }
    mutations
}
impl ProcessCraft {
    pub(super) fn collaboration_artifacts_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
        scope: Option<&processcraft_core::queries::ChoreographyScope<'_>>,
    ) {
        ui.collapsing("Collaboration artifacts", |ui| {
            if self.collaboration.artifacts.as_ref().is_some_and(|draft| draft.root != root.id.selector() || draft.scope.as_ref() != scope.map(|scope| &scope.owner)) {
                ui.label("An artifact draft is open in another scope. Apply or discard it there first.");
                return;
            }
            if self.collaboration.artifacts.is_none() {
                let artifacts: Vec<&CollaborationArtifact> = scope.map_or_else(|| root.extras.iter().flat_map(|extras| extras.artifacts.iter()).collect(), |scope| scope.artifacts.clone());
                for (index, artifact) in artifacts.iter().enumerate() {
                    let (identity, kind) = match artifact { CollaborationArtifact::Group(value) => (&value.id, "Group"), CollaborationArtifact::TextAnnotation(value) => (&value.id, "Annotation"), CollaborationArtifact::Association(value) => (&value.id, "Association") };
                    let label = super::event::local_choice_label(None, kind, identity, index + 1);
                    if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new(format!("Select {label} properties"))).clicked() && self.commit_draft() { self.select(Some(identity.selector())); }
                    if let Some(current) = scope {
                        ui.push_id(identity.selector(), |ui| { ui.menu_button(format!("Move {label} to scope"), |ui| {
                            for destination in processcraft_core::queries::choreography_scopes(document).into_iter().filter(|candidate| candidate.root.id == current.root.id && candidate.owner != current.owner) {
                                let name = if destination.parent.is_none() { current.root.name.clone().unwrap_or_else(|| "Root choreography".into()) } else { processcraft_core::queries::choreography_scopes(document).iter().flat_map(|scope| scope.activities.iter()).find(|activity| activity.id == destination.owner).and_then(|activity| activity.name.clone()).unwrap_or_else(|| "Unnamed sub-choreography".into()) };
                                if ui.button(name).clicked() && self.prepare_navigation() {
                                    if self.ctl.session.snapshot().generation != generation { self.ctl.error = "The model changed; reopen the scope before moving this artifact.".into(); }
                                    else if self.ctl.mutate_at_generation(generation, vec![Mutation::ReparentChoreographyArtifact { document_id: document.id, choreography_id: current.root.id.selector(), source_scope_id: current.owner.clone(), target_scope_id: destination.owner, artifact_id: identity.selector(), index: destination.artifacts.len() }]) { ui.close(); }
                                }
                            }
                        }); });
                    }
                }
                if ui.add_enabled(!self.collaboration.has_draft(), egui::Button::new("Edit artifacts…")).clicked() && self.commit_draft() { self.collaboration.document = Some(document.id); self.collaboration.artifacts = Some(scope.map_or_else(|| ArtifactsDraft::new(root, generation), |scope| ArtifactsDraft::scoped(root, scope, generation))); }
                return;
            }
            let choices: Vec<_> = root.participants.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1))).chain(root.conversation_nodes.iter().enumerate().map(|(i,v)| (v.id.selector(), super::event::local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1)))).chain(root.extras.iter().flat_map(|extras| extras.artifacts.iter()).enumerate().filter_map(|(i, value)| { let (identity, kind) = match value { CollaborationArtifact::Group(value) => (&value.id, "Group"), CollaborationArtifact::TextAnnotation(value) => (&value.id, "Annotation"), CollaborationArtifact::Association(_) => return None }; Some((identity.selector(), super::event::local_choice_label(None, kind, identity, i + 1))) })).chain(scope.into_iter().flat_map(|scope| scope.ordered.iter()).enumerate().filter_map(|(i, value)| { use processcraft_core::queries::ChoreographyElementRef; let (identity, name) = match value { ChoreographyElementRef::Activity(v) => (&v.id, v.name.as_deref()), ChoreographyElementRef::Event(v) | ChoreographyElementRef::Gateway(v) => (&v.id, v.name.as_deref()), ChoreographyElementRef::SequenceFlow(_) => return None }; Some((identity.selector(), super::event::local_choice_label(name, "Flow node", identity, i + 1))) })).collect();
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
                            match text { Some(content) if content.plain_text().is_none() => { if let Some(body) = &mut artifact.structured_body { ui.collapsing("Structured annotation content", |ui| body.ui(ui)); } }, _ => {
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
                if ui.button("Apply artifacts").clicked() { if draft.generation != self.ctl.session.snapshot().generation || draft.root != root.id.selector() { self.ctl.error = "The model changed while this draft was open. Discard it and reopen the editor.".into(); } else { match draft.build() { Ok(extras) => {
                    let mutations = if let Some(scope_id) = &draft.scope {
                        let desired = extras.map_or_else(Vec::new, |extras| extras.artifacts);
                        let mut mutations = scoped_artifact_mutations(document.id, &draft.root, scope_id, &draft.original, desired.clone());
                        if let Some(diagram) = document.diagrams.iter().find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id) && diagram.plane == *scope_id) {
                            for (index, artifact) in desired.iter().enumerate().filter(|(_, artifact)| !draft.original.contains(&artifact_id(artifact))) {
                                if matches!(artifact, CollaborationArtifact::Group(_) | CollaborationArtifact::TextAnnotation(_)) {
                                    mutations.push(Mutation::CreateShape { document_id: document.id, diagram_id: diagram.id.selector(), shape: Shape { id: controller::id("ArtifactShape").into(), element: artifact_id(artifact), bounds: Bounds { x: self.camera.0 + 100.0, y: self.camera.1 + 100.0 + index as f64 * 90.0, width: 140.0, height: 70.0 }, di_metadata: None, is_message_visible: None, participant_band_kind: None, choreography_activity_shape: None, is_expanded: None, is_horizontal: None, is_marker_visible: None, label: None, label_visibility: None, style: None } });
                                } else if let CollaborationArtifact::Association(flow) = artifact
                                    && let (Some(processcraft_core::bpmn_activity::ModelReference::Local(source)), Some(processcraft_core::bpmn_activity::ModelReference::Local(target))) = (&flow.source, &flow.target)
                                {
                                    let sources = diagram.shapes.iter().filter(|shape| shape.element == *source && shape.choreography_activity_shape.is_none()).collect::<Vec<_>>();
                                    let targets = diagram.shapes.iter().filter(|shape| shape.element == *target && shape.choreography_activity_shape.is_none()).collect::<Vec<_>>();
                                    if let ([source], [target]) = (sources.as_slice(), targets.as_slice()) {
                                        let center = |bounds: &Bounds| Point { x: bounds.x + bounds.width / 2.0, y: bounds.y + bounds.height / 2.0 };
                                        mutations.push(Mutation::CreateEdge { document_id: document.id, diagram_id: diagram.id.selector(), edge: Edge { id: controller::id("AssociationEdge").into(), element: flow.id.selector(), source_element: Some(source.id.selector()), target_element: Some(target.id.selector()), waypoints: vec![center(&source.bounds), center(&target.bounds)], label: None, label_visibility: None, style: None, di_metadata: None, message_visible_kind: None } });
                                    }
                                }
                            }
                        }
                        mutations
                    } else { let mut updated = root.clone(); updated.extras = extras; vec![super::collaboration::update_root(document, updated)] };
                    if mutations.is_empty() || self.ctl.mutate_at_generation(draft.generation, mutations) { close = true; }
                }, Err(error) => self.ctl.error = error } } }
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
    fn artifact_draft_preserves_structured_annotation_content() {
        let body = processcraft_core::bpmn_xml::MixedXml::Structured(
            processcraft_core::bpmn_xml::XmlContent {
                namespaces: vec![],
                nodes: vec![
                    processcraft_core::bpmn_xml::XmlNode::Text {
                        text: "Keep".into(),
                    },
                    processcraft_core::bpmn_xml::XmlNode::Comment {
                        text: "context".into(),
                    },
                ],
            },
        );
        let node = Node {
            id: "Annotation".parse().unwrap(),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            kind: NodeKind::TextAnnotation,
            details: Some(NodeDetails::TextAnnotation {
                text_format: None,
                text: Some(body.clone()),
            }),
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            owner: String::new(),
            contact: String::new(),
            flow_metadata: None,
            item_aware: None,
        };
        let draft = ArtifactDraft::new(&CollaborationArtifact::TextAnnotation(Box::new(node)));
        let CollaborationArtifact::TextAnnotation(result) = draft.build().unwrap() else {
            panic!("annotation")
        };
        assert!(
            matches!(&result.details, Some(NodeDetails::TextAnnotation { text: Some(value), .. }) if value == &body)
        );
    }
    #[test]
    fn scoped_artifact_draft_emits_ordered_changes_without_replacing_owner() {
        let first = CollaborationArtifact::Group(Group {
            id: "First".parse().unwrap(),
            base: None,
            category_value_ref: None,
        });
        let second = CollaborationArtifact::Group(Group {
            id: "Second".parse().unwrap(),
            base: None,
            category_value_ref: None,
        });
        let third = CollaborationArtifact::Group(Group {
            id: "Third".parse().unwrap(),
            base: None,
            category_value_ref: None,
        });
        let doc = ProcessCraft::default()
            .ctl
            .session
            .snapshot()
            .project
            .documents[0]
            .id;
        let mutations = scoped_artifact_mutations(
            doc,
            &"Root".parse().unwrap(),
            &"Sub".parse().unwrap(),
            &[artifact_id(&first), artifact_id(&second)],
            vec![second, third],
        );
        assert!(
            matches!(&mutations[0], Mutation::DeleteChoreographyArtifact { artifact_id, .. } if *artifact_id == "First".parse::<ElementSelector>().unwrap())
        );
        assert!(matches!(
            &mutations[1],
            Mutation::UpdateChoreographyArtifact { .. }
        ));
        assert!(matches!(
            &mutations[2],
            Mutation::CreateChoreographyArtifact { index: 1, .. }
        ));
    }
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
