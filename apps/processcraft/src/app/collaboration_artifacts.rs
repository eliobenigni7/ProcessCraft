//! Editor for the groups, text annotations and associations of a collaboration or choreography scope.
use super::event::{ReferenceDraft, build_ref, local_choice_label, ref_ui};
use super::*;
use processcraft_core::bpmn_activity::ModelReference;
use processcraft_core::bpmn_collaboration::{CollaborationArtifact, CollaborationExtras};
use processcraft_core::bpmn_model::{Group, NodeDetails};
use processcraft_core::bpmn_xml::MixedXml;
use processcraft_core::ids::{DocumentId, ElementIdentity};
use processcraft_core::queries::{self, ChoreographyElementRef, ChoreographyScope};

/// A selectable reference target and its label.
type Choice = (ElementSelector, String);

/// Upper bound on the artifacts a single draft can hold.
const MAX_ARTIFACTS: usize = 256;

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
                        text: Some(content @ MixedXml::Structured(_)),
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
    fn scoped(root: &Collaboration, scope: &ChoreographyScope<'_>, generation: u64) -> Self {
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
/// Identity of an artifact plus the kind word used in choice labels.
fn identity_and_kind(artifact: &CollaborationArtifact) -> (&ElementIdentity, &'static str) {
    match artifact {
        CollaborationArtifact::Group(value) => (&value.id, "Group"),
        CollaborationArtifact::TextAnnotation(value) => (&value.id, "Annotation"),
        CollaborationArtifact::Association(value) => (&value.id, "Association"),
    }
}
fn scoped_artifact_mutations(
    document_id: DocumentId,
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

fn new_group() -> CollaborationArtifact {
    CollaborationArtifact::Group(Group {
        id: ElementIdentity::anonymous(),
        base: None,
        category_value_ref: None,
    })
}
fn new_annotation() -> CollaborationArtifact {
    CollaborationArtifact::TextAnnotation(Box::new(Node {
        id: ElementIdentity::anonymous(),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        kind: NodeKind::TextAnnotation,
        details: Some(NodeDetails::TextAnnotation {
            text_format: None,
            text: None,
        }),
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
        flow_metadata: None,
        item_aware: None,
    }))
}
fn new_association() -> CollaborationArtifact {
    CollaborationArtifact::Association(Box::new(Flow {
        id: ElementIdentity::anonymous(),
        kind: FlowKind::Association,
        source: None,
        target: None,
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
        association_direction: None,
        flow_metadata: None,
        message_ref: None,
    }))
}

/// Source/target choices for artifact associations in `root` (and the drafted `scope`).
fn artifact_reference_choices(
    root: &Collaboration,
    scope: Option<&ChoreographyScope<'_>>,
) -> Vec<(ElementSelector, String)> {
    let participants = root.participants.iter().enumerate().map(|(i, v)| {
        let label = local_choice_label(v.name.as_deref(), "Participant", &v.id, i + 1);
        (v.id.selector(), label)
    });
    let conversations = root.conversation_nodes.iter().enumerate().map(|(i, v)| {
        let label = local_choice_label(v.name.as_deref(), "Conversation", &v.id, i + 1);
        (v.id.selector(), label)
    });
    let artifacts = root
        .extras
        .iter()
        .flat_map(|extras| extras.artifacts.iter())
        .enumerate()
        .filter_map(|(i, value)| {
            if matches!(value, CollaborationArtifact::Association(_)) {
                return None;
            }
            let (identity, kind) = identity_and_kind(value);
            Some((
                identity.selector(),
                local_choice_label(None, kind, identity, i + 1),
            ))
        });
    let flow_nodes = scope
        .into_iter()
        .flat_map(|scope| scope.ordered.iter())
        .enumerate()
        .filter_map(|(i, value)| {
            let (identity, name) = match value {
                ChoreographyElementRef::Activity(v) => (&v.id, v.name.as_deref()),
                ChoreographyElementRef::Event(v) | ChoreographyElementRef::Gateway(v) => {
                    (&v.id, v.name.as_deref())
                }
                ChoreographyElementRef::SequenceFlow(_) => return None,
            };
            Some((
                identity.selector(),
                local_choice_label(name, "Flow node", identity, i + 1),
            ))
        });
    participants
        .chain(conversations)
        .chain(artifacts)
        .chain(flow_nodes)
        .collect()
}

/// Editor for the body of a text annotation.
fn annotation_ui(
    ui: &mut egui::Ui,
    node: &mut Node,
    structured_body: &mut Option<super::xml::MixedDraft>,
) {
    let Some(NodeDetails::TextAnnotation { text_format, text }) = node.details.as_mut() else {
        ui.label(
            "This annotation uses a legacy body; select it on the diagram to edit the body safely.",
        );
        return;
    };
    super::catalog::optional_text(ui, "Text format", text_format);
    if matches!(text, Some(content) if content.plain_text().is_none()) {
        if let Some(body) = structured_body {
            ui.collapsing("Structured annotation content", |ui| body.ui(ui));
        }
        return;
    }
    let mut present = text.is_some();
    let label = ui.checkbox(&mut present, "Text");
    if !present {
        *text = None;
        return;
    }
    let content = text.get_or_insert_with(|| String::new().into());
    if let MixedXml::Text(value) = content {
        ui.text_edit_multiline(value).labelled_by(label.id);
    }
}

/// Editor for one drafted artifact; records a requested move-up or removal.
fn artifact_editor_ui(
    ui: &mut egui::Ui,
    index: usize,
    artifact: &mut ArtifactDraft,
    (choices, categories): (&[Choice], &[Choice]),
    (up, remove): (&mut Option<usize>, &mut Option<usize>),
) {
    ui.push_id(index, |ui| {
        ui.separator();
        match &mut artifact.value {
            CollaborationArtifact::Group(_) => {
                ui.strong(format!("Group {}", index + 1));
                ref_ui(ui, "Category value", &mut artifact.source, categories);
            }
            CollaborationArtifact::TextAnnotation(node) => {
                ui.strong(format!("Text annotation {}", index + 1));
                annotation_ui(ui, node, &mut artifact.structured_body);
            }
            CollaborationArtifact::Association(_) => {
                ui.strong(format!("Association {}", index + 1));
                ref_ui(ui, "Source", &mut artifact.source, choices);
                ref_ui(ui, "Target", &mut artifact.target, choices);
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(index > 0, egui::Button::new("Move artifact up"))
                .clicked()
            {
                *up = Some(index);
            }
            if ui.button("Remove artifact").clicked() {
                *remove = Some(index);
            }
        });
    });
}

/// An "Add ..." button that appends `make()` while the draft has room.
fn add_artifact_button(
    ui: &mut egui::Ui,
    draft: &mut ArtifactsDraft,
    label: &str,
    make: fn() -> CollaborationArtifact,
) {
    let has_room = draft.values.len() < MAX_ARTIFACTS;
    if ui.add_enabled(has_room, egui::Button::new(label)).clicked() {
        draft.values.push(ArtifactDraft::new(&make()));
    }
}

impl ProcessCraft {
    /// "Move to scope" menu of one artifact inside a choreography scope.
    fn artifact_move_menu(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        current: &ChoreographyScope<'_>,
        (identity, label): (&ElementIdentity, &str),
        generation: u64,
    ) {
        ui.push_id(identity.selector(), |ui| {
            ui.menu_button(format!("Move {label} to scope"), |ui| {
                let scopes = queries::choreography_scopes(document);
                let destinations = scopes.iter().filter(|candidate| {
                    candidate.root.id == current.root.id && candidate.owner != current.owner
                });
                for destination in destinations {
                    let name = if destination.parent.is_none() {
                        (current.root.name.clone()).unwrap_or_else(|| "Root choreography".into())
                    } else {
                        scopes
                            .iter()
                            .flat_map(|scope| scope.activities.iter())
                            .find(|activity| activity.id == destination.owner)
                            .and_then(|activity| activity.name.clone())
                            .unwrap_or_else(|| "Unnamed sub-choreography".into())
                    };
                    if !(ui.button(name).clicked() && self.prepare_navigation()) {
                        continue;
                    }
                    if self.ctl.session.snapshot().generation != generation {
                        self.ctl.error =
                            "The model changed; reopen the scope before moving this artifact."
                                .into();
                        continue;
                    }
                    let reparent = Mutation::ReparentChoreographyArtifact {
                        document_id: document.id,
                        choreography_id: current.root.id.selector(),
                        source_scope_id: current.owner.clone(),
                        target_scope_id: destination.owner.clone(),
                        artifact_id: identity.selector(),
                        index: destination.artifacts.len(),
                    };
                    if self.ctl.mutate_at_generation(generation, vec![reparent]) {
                        ui.close();
                    }
                }
            });
        });
    }

    /// Diagram shapes and edges for artifacts that were added to a scoped draft.
    fn new_artifact_diagram_mutations(
        &self,
        document: &BpmnDocument,
        diagram: &Diagram,
        desired: &[CollaborationArtifact],
        original: &[ElementSelector],
    ) -> Vec<Mutation> {
        let center = |bounds: &Bounds| Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        };
        let shapes_of = |element: &ElementSelector| {
            diagram
                .shapes
                .iter()
                .filter(|shape| {
                    shape.element == *element && shape.choreography_activity_shape.is_none()
                })
                .collect::<Vec<_>>()
        };
        let mut mutations = Vec::new();
        let added = desired
            .iter()
            .enumerate()
            .filter(|(_, artifact)| !original.contains(&artifact_id(artifact)));
        for (index, artifact) in added {
            match artifact {
                CollaborationArtifact::Group(_) | CollaborationArtifact::TextAnnotation(_) => {
                    let bounds = Bounds {
                        x: self.camera.0 + 100.0,
                        y: self.camera.1 + 100.0 + index as f64 * 90.0,
                        width: 140.0,
                        height: 70.0,
                    };
                    mutations.push(Mutation::CreateShape {
                        document_id: document.id,
                        diagram_id: diagram.id.selector(),
                        shape: Shape {
                            id: controller::id("ArtifactShape").into(),
                            element: artifact_id(artifact),
                            bounds,
                            di_metadata: None,
                            is_message_visible: None,
                            participant_band_kind: None,
                            choreography_activity_shape: None,
                            is_expanded: None,
                            is_horizontal: None,
                            is_marker_visible: None,
                            label: None,
                            label_visibility: None,
                            style: None,
                        },
                    });
                }
                CollaborationArtifact::Association(flow) => {
                    let (Some(ModelReference::Local(source)), Some(ModelReference::Local(target))) =
                        (&flow.source, &flow.target)
                    else {
                        continue;
                    };
                    let (sources, targets) = (shapes_of(source), shapes_of(target));
                    let ([source], [target]) = (sources.as_slice(), targets.as_slice()) else {
                        continue;
                    };
                    mutations.push(Mutation::CreateEdge {
                        document_id: document.id,
                        diagram_id: diagram.id.selector(),
                        edge: Edge {
                            id: controller::id("AssociationEdge").into(),
                            element: flow.id.selector(),
                            source_element: Some(source.id.selector()),
                            target_element: Some(target.id.selector()),
                            waypoints: vec![center(&source.bounds), center(&target.bounds)],
                            label: None,
                            label_visibility: None,
                            style: None,
                            di_metadata: None,
                            message_visible_kind: None,
                        },
                    });
                }
            }
        }
        mutations
    }

    /// Applies the draft. Returns whether the editor should close.
    fn apply_artifacts(
        &mut self,
        document: &BpmnDocument,
        root: &Collaboration,
        draft: &ArtifactsDraft,
    ) -> bool {
        if draft.generation != self.ctl.session.snapshot().generation
            || draft.root != root.id.selector()
        {
            self.ctl.error =
                "The model changed while this draft was open. Discard it and reopen the editor."
                    .into();
            return false;
        }
        let extras = match draft.build() {
            Ok(extras) => extras,
            Err(error) => {
                self.ctl.error = error;
                return false;
            }
        };
        let mutations = if let Some(scope_id) = &draft.scope {
            let desired = extras.map_or_else(Vec::new, |extras| extras.artifacts);
            let mut mutations = scoped_artifact_mutations(
                document.id,
                &draft.root,
                scope_id,
                &draft.original,
                desired.clone(),
            );
            let diagram = document.diagrams.iter().find(|diagram| {
                self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id)
                    && diagram.plane == *scope_id
            });
            if let Some(diagram) = diagram {
                mutations.extend(self.new_artifact_diagram_mutations(
                    document,
                    diagram,
                    &desired,
                    &draft.original,
                ));
            }
            mutations
        } else {
            let mut updated = root.clone();
            updated.extras = extras;
            vec![super::collaboration::update_root(document, updated)]
        };
        mutations.is_empty() || self.ctl.mutate_at_generation(draft.generation, mutations)
    }

    pub(super) fn collaboration_artifacts_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        root: &Collaboration,
        generation: u64,
        scope: Option<&ChoreographyScope<'_>>,
    ) {
        ui.collapsing("Collaboration artifacts", |ui| {
            let in_other_scope = self.collaboration.artifacts.as_ref().is_some_and(|draft| {
                draft.root != root.id.selector()
                    || draft.scope.as_ref() != scope.map(|scope| &scope.owner)
            });
            if in_other_scope {
                ui.label(
                    "An artifact draft is open in another scope. Apply or discard it there first.",
                );
                return;
            }
            if self.collaboration.artifacts.is_none() {
                let artifacts: Vec<&CollaborationArtifact> = match scope {
                    Some(scope) => scope.artifacts.clone(),
                    None => root
                        .extras
                        .iter()
                        .flat_map(|extras| extras.artifacts.iter())
                        .collect(),
                };
                for (index, artifact) in artifacts.iter().enumerate() {
                    let (identity, kind) = identity_and_kind(artifact);
                    let label = local_choice_label(None, kind, identity, index + 1);
                    let select = egui::Button::new(format!("Select {label} properties"));
                    let idle = !self.collaboration.has_draft();
                    if ui.add_enabled(idle, select).clicked() && self.commit_draft() {
                        self.select(Some(identity.selector()));
                    }
                    if let Some(current) = scope {
                        self.artifact_move_menu(
                            ui,
                            document,
                            current,
                            (identity, &label),
                            generation,
                        );
                    }
                }
                let edit = egui::Button::new("Edit artifacts…");
                if ui
                    .add_enabled(!self.collaboration.has_draft(), edit)
                    .clicked()
                    && self.commit_draft()
                {
                    self.collaboration.document = Some(document.id);
                    self.collaboration.artifacts = Some(match scope {
                        Some(scope) => ArtifactsDraft::scoped(root, scope, generation),
                        None => ArtifactsDraft::new(root, generation),
                    });
                }
                return;
            }
            let choices = artifact_reference_choices(root, scope);
            let categories: Vec<_> = document
                .categories
                .iter()
                .flat_map(|category| category.values.iter())
                .enumerate()
                .map(|(i, value)| {
                    let label = local_choice_label(
                        value.value.as_deref(),
                        "Category value",
                        &value.id,
                        i + 1,
                    );
                    (value.id.selector(), label)
                })
                .collect();
            let mut draft = self.collaboration.artifacts.take().expect("open");
            let mut close = false;
            let mut remove = None;
            let mut up = None;
            for (index, artifact) in draft.values.iter_mut().enumerate() {
                artifact_editor_ui(
                    ui,
                    index,
                    artifact,
                    (&choices, &categories),
                    (&mut up, &mut remove),
                );
            }
            if let Some(index) = remove {
                draft.values.remove(index);
            } else if let Some(index) = up {
                draft.values.swap(index, index - 1);
            }
            ui.horizontal_wrapped(|ui| {
                add_artifact_button(ui, &mut draft, "Add group", new_group);
                add_artifact_button(ui, &mut draft, "Add annotation", new_annotation);
                add_artifact_button(ui, &mut draft, "Add association", new_association);
            });
            ui.horizontal(|ui| {
                if ui.button("Apply artifacts").clicked()
                    && self.apply_artifacts(document, root, &draft)
                {
                    close = true;
                }
                if ui.button("Discard artifacts draft").clicked() {
                    close = true;
                }
            });
            if !close {
                self.collaboration.artifacts = Some(draft);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let Mutation::DeleteChoreographyArtifact { artifact_id, .. } = &mutations[0] else {
            panic!("delete expected")
        };
        assert_eq!(*artifact_id, "First".parse::<ElementSelector>().unwrap());
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
