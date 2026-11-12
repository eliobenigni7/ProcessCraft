use super::callable::reference_list_ui;
use super::event::{ReferenceDraft, local_choice_label, ref_ui};
use super::*;
use processcraft_core::bpmn_activity::FlowElementMetadata;
use processcraft_core::bpmn_collaboration::ChoreographyActivity;

fn route_new_sequence(
    diagram: &Diagram,
    source: &ElementSelector,
    target: &ElementSelector,
) -> Result<(ElementSelector, ElementSelector, Vec<Point>), String> {
    use processcraft_core::geometry::{route, valid_bounds};
    use processcraft_core::model::DiReference;
    if source == target {
        return Err("self-loop routing requires an explicit diagram edge".into());
    }
    let source_shapes = diagram
        .shapes
        .iter()
        .filter(|shape| shape.element == *source && shape.choreography_activity_shape.is_none())
        .collect::<Vec<_>>();
    let target_shapes = diagram
        .shapes
        .iter()
        .filter(|shape| shape.element == *target && shape.choreography_activity_shape.is_none())
        .collect::<Vec<_>>();
    let ([source_shape], [target_shape]) = (source_shapes.as_slice(), target_shapes.as_slice())
    else {
        return Err("source and target each need exactly one unambiguous shape".into());
    };
    let (a, b) = (&source_shape.bounds, &target_shape.bounds);
    if !valid_bounds(a)
        || !valid_bounds(b)
        || a.width <= 0.0
        || a.height <= 0.0
        || b.width <= 0.0
        || b.height <= 0.0
    {
        return Err("source or target shape has invalid bounds".into());
    }
    let (ax, ay, bx, by) = (
        a.x + a.width / 2.0,
        a.y + a.height / 2.0,
        b.x + b.width / 2.0,
        b.y + b.height / 2.0,
    );
    let horizontal = a.x + a.width < b.x || b.x + b.width < a.x;
    let vertical = a.y + a.height < b.y || b.y + b.height < a.y;
    if !horizontal && !vertical {
        return Err("source and target shapes overlap".into());
    }
    let use_horizontal = horizontal && (!vertical || (bx - ax).abs() >= (by - ay).abs());
    let (start, end, start_delta, end_delta) = if use_horizontal && bx > ax {
        (
            Point {
                x: a.x + a.width,
                y: ay,
            },
            Point { x: b.x, y: by },
            (12.0, 0.0),
            (-12.0, 0.0),
        )
    } else if use_horizontal {
        (
            Point { x: a.x, y: ay },
            Point {
                x: b.x + b.width,
                y: by,
            },
            (-12.0, 0.0),
            (12.0, 0.0),
        )
    } else if by > ay {
        (
            Point {
                x: ax,
                y: a.y + a.height,
            },
            Point { x: bx, y: b.y },
            (0.0, 12.0),
            (0.0, -12.0),
        )
    } else {
        (
            Point { x: ax, y: a.y },
            Point {
                x: bx,
                y: b.y + b.height,
            },
            (0.0, -12.0),
            (0.0, 12.0),
        )
    };
    let exit = Point {
        x: start.x + start_delta.0,
        y: start.y + start_delta.1,
    };
    let approach = Point {
        x: end.x + end_delta.0,
        y: end.y + end_delta.1,
    };
    let obstacles = diagram.shapes.iter().filter(|shape| {
        if shape.id == source_shape.id || shape.id == target_shape.id { return false; }
        !matches!(&shape.choreography_activity_shape, Some(DiReference::Local(owner)) if source_shape.id == *owner || target_shape.id == *owner)
    }).map(|shape| shape.bounds.clone()).collect::<Vec<_>>();
    let result =
        route(exit.clone(), approach.clone(), &obstacles, 8.0, 2048).map_err(str::to_owned)?;
    if let Some(warning) = result.warning {
        return Err(warning);
    }
    let mut points = vec![start];
    points.extend(result.waypoints.ok_or("no clear route found")?);
    points.push(end);
    let crosses = |p: &Point, q: &Point, r: &Bounds| {
        if p.x == q.x {
            p.x > r.x && p.x < r.x + r.width && p.y.max(q.y) > r.y && p.y.min(q.y) < r.y + r.height
        } else if p.y == q.y {
            p.y > r.y && p.y < r.y + r.height && p.x.max(q.x) > r.x && p.x.min(q.x) < r.x + r.width
        } else {
            true
        }
    };
    if points.windows(2).any(|pair| {
        (pair[0].x == pair[1].x && pair[0].y == pair[1].y)
            || crosses(&pair[0], &pair[1], a)
            || crosses(&pair[0], &pair[1], b)
            || obstacles.iter().any(|obstacle| {
                crosses(
                    &pair[0],
                    &pair[1],
                    &Bounds {
                        x: obstacle.x - 8.0,
                        y: obstacle.y - 8.0,
                        width: obstacle.width + 16.0,
                        height: obstacle.height + 16.0,
                    },
                )
            })
    }) {
        return Err("no safe route between the shape boundaries".into());
    }
    Ok((
        source_shape.id.selector(),
        target_shape.id.selector(),
        points,
    ))
}

fn graph_flow_choices(
    scope: &processcraft_core::queries::ChoreographyScope<'_>,
) -> Vec<(ElementSelector, String)> {
    use processcraft_core::bpmn_collaboration::ChoreographyActivityDetails;
    use processcraft_core::queries::ChoreographyElementRef;
    scope
        .ordered
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            let (id, name, kind) = match element {
                ChoreographyElementRef::Activity(value) => (
                    &value.id,
                    value.name.as_deref(),
                    match &value.details {
                        Some(ChoreographyActivityDetails::Sub { .. }) => "Sub-choreography",
                        Some(ChoreographyActivityDetails::Call { .. }) => "Call choreography",
                        None => "Choreography task",
                    },
                ),
                ChoreographyElementRef::Event(value) | ChoreographyElementRef::Gateway(value) => (
                    &value.id,
                    value.name.as_deref(),
                    node_kind_label(value.kind),
                ),
                ChoreographyElementRef::SequenceFlow(_) => return None,
            };
            let description = name
                .filter(|name| !name.trim().is_empty())
                .map_or_else(|| kind.to_owned(), |name| format!("{kind} · {name}"));
            Some((id.selector(), format!("{description} · {}", index + 1)))
        })
        .collect()
}
pub(super) struct ChoreographyFlowDraft {
    choreography: ElementSelector,
    activity: Option<ChoreographyActivity>,
    sequence: Option<ElementSelector>,
    sequence_flow: Option<Flow>,
    source: Option<ReferenceDraft>,
    target: Option<ReferenceDraft>,
    present: bool,
    value: FlowElementMetadata,
    categories: Vec<ReferenceDraft>,
    incoming: Vec<ReferenceDraft>,
    outgoing: Vec<ReferenceDraft>,
    error: String,
    scope: Option<ElementSelector>,
    generation: u64,
    participants: Vec<ReferenceDraft>,
    initiating: Option<ReferenceDraft>,
    messages: Vec<ReferenceDraft>,
    called: Option<ReferenceDraft>,
    mappings: Vec<(
        processcraft_core::bpmn_collaboration::ParticipantAssociation,
        Option<ReferenceDraft>,
        Option<ReferenceDraft>,
    )>,
}
impl ChoreographyFlowDraft {
    fn new(choreography: ElementSelector, activity: ChoreographyActivity) -> Self {
        let present = activity.flow_metadata.is_some();
        let value = activity.flow_metadata.clone().unwrap_or_default();
        Self {
            choreography,
            activity: Some(activity.clone()),
            sequence: None,
            sequence_flow: None,
            source: None,
            target: None,
            categories: value
                .category_value_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            incoming: value.incoming.iter().map(ReferenceDraft::new).collect(),
            outgoing: value.outgoing.iter().map(ReferenceDraft::new).collect(),
            present,
            value,
            error: String::new(),
            scope: None,
            generation: 0,
            participants: activity
                .participant_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            initiating: activity
                .initiating_participant
                .as_ref()
                .map(ReferenceDraft::new),
            messages: activity
                .message_flow_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            called: match &activity.details {
                Some(
                    processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Call {
                        called_choreography_ref,
                        ..
                    },
                ) => called_choreography_ref.as_ref().map(ReferenceDraft::new),
                _ => None,
            },
            mappings: match &activity.details {
                Some(
                    processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Call {
                        participant_associations,
                        ..
                    },
                ) => participant_associations
                    .iter()
                    .map(|m| {
                        (
                            m.clone(),
                            m.inner.as_ref().map(ReferenceDraft::new),
                            m.outer.as_ref().map(ReferenceDraft::new),
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            },
        }
    }
    fn new_sequence(choreography: ElementSelector, flow: &Flow) -> Self {
        let value = flow.flow_metadata.clone().unwrap_or_default();
        Self {
            choreography,
            activity: None,
            sequence: Some(flow.id.selector()),
            sequence_flow: Some(flow.clone()),
            source: flow.source.as_ref().map(ReferenceDraft::new),
            target: flow.target.as_ref().map(ReferenceDraft::new),
            present: flow.flow_metadata.is_some(),
            categories: value
                .category_value_refs
                .iter()
                .map(ReferenceDraft::new)
                .collect(),
            incoming: value.incoming.iter().map(ReferenceDraft::new).collect(),
            outgoing: value.outgoing.iter().map(ReferenceDraft::new).collect(),
            value,
            error: String::new(),
            scope: None,
            generation: 0,
            participants: Vec::new(),
            initiating: None,
            messages: Vec::new(),
            called: None,
            mappings: Vec::new(),
        }
    }
    fn build(&self) -> Result<Option<FlowElementMetadata>, String> {
        if !self.present {
            return Ok(None);
        }
        let mut value = self.value.clone();
        value.category_value_refs = self
            .categories
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        value.incoming = self
            .incoming
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        value.outgoing = self
            .outgoing
            .iter()
            .map(ReferenceDraft::build)
            .collect::<Result<_, _>>()?;
        Ok(Some(value))
    }
}
impl ProcessCraft {
    pub(super) fn choreography_flow_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        editable: bool,
    ) {
        if self.choreography_flow_draft.is_none() {
            ui.menu_button("Edit choreography flow details…", |ui| {
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
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            let mut draft = ChoreographyFlowDraft::new(
                                scope.root.id.selector(),
                                (*activity).clone(),
                            );
                            draft.scope = Some(scope.owner.clone());
                            draft.generation = self.ctl.session.snapshot().generation;
                            self.choreography_flow_draft = Some(draft);
                            ui.close();
                        }
                    }
                    for (i, flow) in scope.flows.iter().enumerate() {
                        let label = local_choice_label(
                            flow.name.as_deref(),
                            "Sequence flow",
                            &flow.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            let mut draft =
                                ChoreographyFlowDraft::new_sequence(scope.root.id.selector(), flow);
                            draft.scope = Some(scope.owner.clone());
                            draft.generation = self.ctl.session.snapshot().generation;
                            self.choreography_flow_draft = Some(draft);
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
                            self.choreography_flow_draft = Some(ChoreographyFlowDraft::new(
                                choreography.id.selector(),
                                activity.clone(),
                            ));
                            ui.close();
                        }
                    }
                    for (i, flow) in choreography.sequence_flows.iter().enumerate() {
                        let label = local_choice_label(
                            flow.name.as_deref(),
                            "Sequence flow",
                            &flow.id,
                            i + 1,
                        );
                        if ui
                            .add_enabled(editable && !self.draft_dirty, egui::Button::new(label))
                            .clicked()
                            && self.commit_draft()
                        {
                            self.choreography_flow_draft =
                                Some(ChoreographyFlowDraft::new_sequence(
                                    choreography.id.selector(),
                                    flow,
                                ));
                            ui.close();
                        }
                    }
                }
            });
            return;
        }
        let draft = self.choreography_flow_draft.as_mut().unwrap();
        ui.strong("Choreography flow details");
        if let (Some(scope_id), Some(flow)) = (&draft.scope, &mut draft.sequence_flow) {
            let mut named = flow.name.is_some();
            ui.checkbox(&mut named, "Name present");
            if named {
                let label = ui.label("Sequence flow name");
                ui.text_edit_singleline(flow.name.get_or_insert_with(String::new))
                    .labelled_by(label.id);
            } else {
                flow.name = None;
            }
            if let Some(scope) = processcraft_core::queries::choreography_scopes(document)
                .into_iter()
                .find(|s| &s.owner == scope_id && s.root.id == draft.choreography)
            {
                let choices = graph_flow_choices(&scope);
                ref_ui(ui, "Source node", &mut draft.source, &choices);
                ref_ui(ui, "Target node", &mut draft.target, &choices);
            }
        }
        if draft.scope.is_some()
            && let Some(activity) = &mut draft.activity
        {
            let mut named = activity.name.is_some();
            ui.checkbox(&mut named, "Name present");
            if named {
                let label = ui.label("Activity name");
                ui.text_edit_singleline(activity.name.get_or_insert_with(String::new))
                    .labelled_by(label.id);
            } else {
                activity.name = None;
            }
            if let Some(root) = document
                .choreographies
                .iter()
                .find(|root| root.id == draft.choreography)
            {
                let participants = root
                    .participants
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        (
                            p.id.selector(),
                            local_choice_label(p.name.as_deref(), "Participant", &p.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                let messages = root
                    .message_flows
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        (
                            p.id.selector(),
                            local_choice_label(p.name.as_deref(), "Message flow", &p.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                reference_list_ui(ui, "Participant", &mut draft.participants, &participants);
                ref_ui(
                    ui,
                    "Initiating participant",
                    &mut draft.initiating,
                    &participants,
                );
                if activity.details.is_none() {
                    reference_list_ui(ui, "Message flow", &mut draft.messages, &messages);
                }
                if matches!(
                    activity.details,
                    Some(
                        processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Call { .. }
                    )
                ) {
                    let roots = document
                        .choreographies
                        .iter()
                        .enumerate()
                        .map(|(i, root)| {
                            (
                                root.id.selector(),
                                local_choice_label(
                                    root.name.as_deref(),
                                    "Choreography",
                                    &root.id,
                                    i + 1,
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    let all_participants = document
                        .choreographies
                        .iter()
                        .flat_map(|root| &root.participants)
                        .enumerate()
                        .map(|(i, p)| {
                            (
                                p.id.selector(),
                                local_choice_label(p.name.as_deref(), "Participant", &p.id, i + 1),
                            )
                        })
                        .collect::<Vec<_>>();
                    ref_ui(ui, "Called choreography", &mut draft.called, &roots);
                    let mut remove = None;
                    for (index, (_, inner, outer)) in draft.mappings.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.label(format!("Participant mapping {}", index + 1));
                            ref_ui(ui, "Inner participant", inner, &all_participants);
                            ref_ui(ui, "Outer participant", outer, &participants);
                            if ui.button("Remove mapping").clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        draft.mappings.remove(index);
                    }
                    if ui.button("Add participant mapping").clicked() {
                        draft.mappings.push((
                            processcraft_core::bpmn_collaboration::ParticipantAssociation {
                                id: None,
                                base: None,
                                inner: None,
                                outer: None,
                            },
                            None,
                            None,
                        ));
                    }
                }
            }
            use processcraft_core::bpmn_collaboration::ChoreographyLoopType as L;
            let label = |value: Option<L>| match value {
                None => "Unspecified",
                Some(L::None) => "No loop",
                Some(L::Standard) => "Standard loop",
                Some(L::MultiInstanceSequential) => "Multi-instance, sequential",
                Some(L::MultiInstanceParallel) => "Multi-instance, parallel",
            };
            egui::ComboBox::from_label("Loop behavior")
                .selected_text(label(activity.loop_type))
                .show_ui(ui, |ui| {
                    for value in [
                        None,
                        Some(L::None),
                        Some(L::Standard),
                        Some(L::MultiInstanceSequential),
                        Some(L::MultiInstanceParallel),
                    ] {
                        ui.selectable_value(&mut activity.loop_type, value, label(value));
                    }
                });
        }
        ui.checkbox(&mut draft.present, "Flow details present");
        if draft.present {
            for (label, value) in [
                ("Auditing", &mut draft.value.auditing),
                ("Monitoring", &mut draft.value.monitoring),
            ] {
                let mut present = value.is_some();
                ui.checkbox(&mut present, label);
                if present {
                    value.get_or_insert_with(Default::default);
                } else {
                    *value = None;
                }
            }
            let categories = document
                .categories
                .iter()
                .flat_map(|c| &c.values)
                .enumerate()
                .map(|(i, v)| {
                    (
                        v.id.selector(),
                        local_choice_label(v.value.as_deref(), "Category value", &v.id, i + 1),
                    )
                })
                .collect::<Vec<_>>();
            reference_list_ui(ui, "Category value", &mut draft.categories, &categories);
            if draft.sequence.is_some() {
                super::catalog::optional_bool(ui, "Immediate", &mut draft.value.is_immediate);
            }
            if let Some(activity) = &draft.activity
                && let Some(choreography) = document
                    .choreographies
                    .iter()
                    .find(|c| c.id == draft.choreography)
            {
                let incoming = choreography
                    .sequence_flows
                    .iter()
                    .filter(|f| matches!(&f.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if activity.id == *id))
                    .enumerate()
                    .map(|(i, f)| {
                        (
                            f.id.selector(),
                            local_choice_label(f.name.as_deref(), "Sequence flow", &f.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                let outgoing = choreography
                    .sequence_flows
                    .iter()
                    .filter(|f| matches!(&f.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if activity.id == *id))
                    .enumerate()
                    .map(|(i, f)| {
                        (
                            f.id.selector(),
                            local_choice_label(f.name.as_deref(), "Sequence flow", &f.id, i + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                ui.collapsing("Declared incoming flows", |ui| {
                    reference_list_ui(ui, "Incoming flow", &mut draft.incoming, &incoming)
                });
                ui.collapsing("Declared outgoing flows", |ui| {
                    reference_list_ui(ui, "Outgoing flow", &mut draft.outgoing, &outgoing)
                });
            }
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Apply choreography details").clicked()
            && self.apply_choreography_flow_draft().is_ok()
        {
            self.restore_selection();
        }
        if ui.button("Cancel choreography details").clicked() {
            self.choreography_flow_draft = None;
        }
    }
    pub(super) fn apply_choreography_flow_draft(&mut self) -> Result<(), String> {
        let snap = self.ctl.session.snapshot();
        let Some(document) = snap.project.documents.get(self.doc) else {
            return Err("no model is open".into());
        };
        let Some(draft) = self.choreography_flow_draft.as_mut() else {
            return Ok(());
        };
        let metadata = draft.build().inspect_err(|e| draft.error = e.clone())?;
        let mutation = if let Some(sequence) = &draft.sequence {
            if let Some(scope) = &draft.scope {
                let built = (|| -> Result<_, String> {
                    Ok((
                        draft
                            .source
                            .as_ref()
                            .map(ReferenceDraft::build)
                            .transpose()?,
                        draft
                            .target
                            .as_ref()
                            .map(ReferenceDraft::build)
                            .transpose()?,
                    ))
                })();
                let Ok((source, target)) = built else {
                    draft.error = built.unwrap_err();
                    return Err(draft.error.clone());
                };
                let mut flow = draft.sequence_flow.clone().expect("sequence owner");
                flow.source = source;
                flow.target = target;
                flow.flow_metadata = metadata;
                if self.ctl.session.snapshot().generation != draft.generation {
                    draft.error = "The model changed; cancel and reopen this draft.".into();
                    return Err(draft.error.clone());
                }
                Mutation::UpdateChoreographyFlowElement {
                    document_id: document.id,
                    choreography_id: draft.choreography.clone(),
                    scope_id: scope.clone(),
                    element:
                        processcraft_core::bpmn_collaboration::ChoreographyFlowElement::SequenceFlow(
                            Box::new(flow),
                        ),
                }
            } else {
                Mutation::SetFlowMetadata {
                    document_id: document.id,
                    element_id: sequence.clone(),
                    metadata,
                }
            }
        } else {
            let mut activity = draft.activity.clone().expect("activity owner");
            activity.flow_metadata = metadata;
            if let Some(scope) = &draft.scope {
                let references = (|| -> Result<_, String> {
                    Ok((
                        draft
                            .participants
                            .iter()
                            .map(ReferenceDraft::build)
                            .collect::<Result<Vec<_>, _>>()?,
                        draft
                            .initiating
                            .as_ref()
                            .map(ReferenceDraft::build)
                            .transpose()?,
                        draft
                            .messages
                            .iter()
                            .map(ReferenceDraft::build)
                            .collect::<Result<Vec<_>, _>>()?,
                    ))
                })();
                let Ok((participants, initiating, messages)) = references else {
                    draft.error = references.unwrap_err();
                    return Err(draft.error.clone());
                };
                activity.participant_refs = participants;
                activity.initiating_participant = initiating;
                activity.message_flow_refs = messages;
                if let Some(
                    processcraft_core::bpmn_collaboration::ChoreographyActivityDetails::Call {
                        participant_associations,
                        called_choreography_ref,
                    },
                ) = &mut activity.details
                {
                    let built = (|| -> Result<_, String> {
                        let called = draft
                            .called
                            .as_ref()
                            .map(ReferenceDraft::build)
                            .transpose()?;
                        let mappings = draft
                            .mappings
                            .iter()
                            .map(|(original, inner, outer)| {
                                let mut value = original.clone();
                                value.inner =
                                    inner.as_ref().map(ReferenceDraft::build).transpose()?;
                                value.outer =
                                    outer.as_ref().map(ReferenceDraft::build).transpose()?;
                                Ok(value)
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        Ok((called, mappings))
                    })();
                    match built {
                        Ok((called, mappings)) => {
                            *called_choreography_ref = called;
                            *participant_associations = mappings;
                        }
                        Err(error) => {
                            draft.error = error;
                            return Err(draft.error.clone());
                        }
                    }
                }
                if self.ctl.session.snapshot().generation != draft.generation {
                    draft.error = "The model changed; cancel and reopen this draft.".into();
                    return Err(draft.error.clone());
                }
                Mutation::UpdateChoreographyFlowElement {
                    document_id: document.id,
                    choreography_id: draft.choreography.clone(),
                    scope_id: scope.clone(),
                    element:
                        processcraft_core::bpmn_collaboration::ChoreographyFlowElement::Activity(
                            Box::new(activity),
                        ),
                }
            } else {
                Mutation::UpdateChoreographyActivity {
                    document_id: document.id,
                    choreography_id: draft.choreography.clone(),
                    activity,
                }
            }
        };
        let mut mutations = vec![mutation];
        let mut route_warning = None;
        if let (Some(scope), Some(sequence)) = (&draft.scope, &draft.sequence) {
            let diagram = document.diagrams.iter().find(|diagram| {
                self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id)
                    && diagram.plane == *scope
            });
            match diagram {
                None => route_warning = Some("select a diagram for this scope".to_owned()),
                Some(diagram) if !diagram.edges.iter().any(|edge| edge.element == *sequence) => {
                    let result = match (&draft.source, &draft.target) {
                        (
                            Some(ReferenceDraft::Local(Some(source))),
                            Some(ReferenceDraft::Local(Some(target))),
                        ) => route_new_sequence(diagram, source, target),
                        _ => Err("select local source and target nodes".into()),
                    };
                    match result {
                        Ok((source, target, waypoints)) => mutations.push(Mutation::CreateEdge {
                            document_id: document.id,
                            diagram_id: diagram.id.selector(),
                            edge: Edge {
                                id: controller::id("SequenceEdge").into(),
                                element: sequence.clone(),
                                source_element: Some(source),
                                target_element: Some(target),
                                waypoints,
                                label: None,
                                label_visibility: None,
                                style: None,
                                di_metadata: None,
                                message_visible_kind: None,
                            },
                        }),
                        Err(reason) => route_warning = Some(reason),
                    }
                }
                Some(_) => {}
            }
        }
        if self.ctl.mutate_at_generation(
            if draft.scope.is_some() {
                draft.generation
            } else {
                self.ctl.session.snapshot().generation
            },
            mutations,
        ) {
            self.choreography_flow_draft = None;
            if let Some(reason) = route_warning {
                self.ctl.error =
                    format!("Sequence flow saved; no diagram edge was created: {reason}.");
            }
            Ok(())
        } else {
            Err(self.ctl.error.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_sequence_routes_between_boundaries_around_obstacles() {
        let shape = |id: &str, element: &str, bounds: Bounds| Shape {
            id: id.parse().unwrap(),
            element: element.parse().unwrap(),
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
        };
        let source = shape(
            "SourceShape",
            "Start",
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 36.0,
                height: 36.0,
            },
        );
        let target = shape(
            "TargetShape",
            "Gateway",
            Bounds {
                x: 240.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            },
        );
        let obstacle = shape(
            "ObstacleShape",
            "Other",
            Bounds {
                x: 100.0,
                y: -10.0,
                width: 40.0,
                height: 80.0,
            },
        );
        let mut diagram = Diagram {
            id: "Diagram".parse().unwrap(),
            plane_id: "Plane".parse().unwrap(),
            plane: "Root".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: vec![source, target, obstacle],
            edges: vec![],
        };
        let (source_id, target_id, points) = route_new_sequence(
            &diagram,
            &"Start".parse().unwrap(),
            &"Gateway".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(source_id, "SourceShape".parse::<ElementSelector>().unwrap());
        assert_eq!(target_id, "TargetShape".parse::<ElementSelector>().unwrap());
        assert_eq!(points.first(), Some(&Point { x: 36.0, y: 18.0 }));
        assert_eq!(points.last(), Some(&Point { x: 240.0, y: 25.0 }));
        assert!(
            points
                .windows(2)
                .all(|pair| (pair[0].x == pair[1].x || pair[0].y == pair[1].y)
                    && !(pair[0].x > 100.0
                        && pair[0].x < 140.0
                        && pair[1].x == pair[0].x
                        && pair[0].y.max(pair[1].y) > -10.0
                        && pair[0].y.min(pair[1].y) < 70.0)
                    && !(pair[0].y > -10.0
                        && pair[0].y < 70.0
                        && pair[1].y == pair[0].y
                        && pair[0].x.max(pair[1].x) > 100.0
                        && pair[0].x.min(pair[1].x) < 140.0))
        );
        assert!(
            route_new_sequence(
                &diagram,
                &"Start".parse().unwrap(),
                &"Start".parse().unwrap()
            )
            .is_err()
        );
        diagram.shapes.push(shape(
            "Duplicate",
            "Gateway",
            Bounds {
                x: 300.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            },
        ));
        assert!(
            route_new_sequence(
                &diagram,
                &"Start".parse().unwrap(),
                &"Gateway".parse().unwrap()
            )
            .is_err()
        );
    }
    #[test]
    fn graph_flow_choices_use_node_kinds_and_names_not_generated_ids() {
        use processcraft_core::bpmn_collaboration::ChoreographyFlowElement;
        let mut document = crate::controller::blank_document();
        let mut root = super::super::collaboration::fresh_choreography(
            [
                "Root".parse().unwrap(),
                "TaskRandomId".parse().unwrap(),
                "Buyer".parse().unwrap(),
                "Seller".parse().unwrap(),
            ],
            [
                "Orders".into(),
                "Submit".into(),
                "Buyer".into(),
                "Seller".into(),
            ],
        );
        let node = |id: &str, kind, name| Node {
            id: id.parse().unwrap(),
            name,
            documentation: String::new().into(),
            base_extras: None,
            kind,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            flow_metadata: None,
            item_aware: None,
            owner: String::new(),
            contact: String::new(),
        };
        root.graph
            .as_mut()
            .unwrap()
            .flow_elements
            .push(ChoreographyFlowElement::Event(Box::new(node(
                "GeneratedStartUuid",
                NodeKind::StartEvent,
                None,
            ))));
        root.graph
            .as_mut()
            .unwrap()
            .flow_elements
            .push(ChoreographyFlowElement::Gateway(Box::new(node(
                "GeneratedGatewayUuid",
                NodeKind::ExclusiveGateway,
                Some("Review".into()),
            ))));
        document.choreographies.push(root);
        let scope = processcraft_core::queries::choreography_scopes(&document)
            .into_iter()
            .find(|scope| scope.owner == "Root".parse::<ElementSelector>().unwrap())
            .unwrap();
        let choices = graph_flow_choices(&scope);
        assert_eq!(
            choices
                .iter()
                .map(|(_, label)| label.as_str())
                .collect::<Vec<_>>(),
            [
                "Choreography task · Submit · 1",
                "Start event · 2",
                "Exclusive gateway · Review · 3"
            ]
        );
        assert_eq!(
            choices[1].0,
            "GeneratedStartUuid".parse::<ElementSelector>().unwrap()
        );
    }
    #[test]
    fn choreography_sequence_flow_draft_retains_metadata_and_optional_false() {
        let metadata = FlowElementMetadata {
            is_immediate: Some(false),
            auditing: Some(Default::default()),
            ..Default::default()
        };
        let flow = Flow {
            message_ref: None,
            id: "Flow".parse().unwrap(),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Source".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Target".parse().unwrap(),
            )),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
            association_direction: None,
            flow_metadata: Some(metadata.clone()),
        };
        let mut draft = ChoreographyFlowDraft::new_sequence("Choreography".parse().unwrap(), &flow);
        assert_eq!(draft.sequence, Some(flow.id.selector()));
        assert_eq!(draft.build().unwrap(), Some(metadata.clone()));
        draft.value.is_immediate = None;
        let result = draft.build().unwrap().unwrap();
        assert_eq!(result.is_immediate, None);
        assert_eq!(result.auditing, metadata.auditing);
    }
}
