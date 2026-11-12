use super::event::local_choice_label;
use super::*;
pub(super) struct BandDraft {
    diagram: ElementSelector,
    id: processcraft_core::ids::ElementIdentity,
    owner: Option<ElementSelector>,
    participant: Option<ElementSelector>,
    kind: Option<ParticipantBandKind>,
    visible: Option<bool>,
    bounds: Bounds,
    error: String,
}
impl BandDraft {
    fn build(&self) -> Result<Shape, String> {
        let owner = self.owner.clone().ok_or("Select an activity shape.")?;
        let participant = self.participant.clone().ok_or("Select a participant.")?;
        let kind = self.kind.ok_or("Select a band position.")?;
        Ok(Shape {
            id: self.id.clone(),
            element: participant,
            bounds: self.bounds.clone(),
            di_metadata: None,
            is_message_visible: self.visible,
            participant_band_kind: Some(kind),
            choreography_activity_shape: Some(DiReference::Local(owner)),
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            label: Some(Label::default()),
            label_visibility: Some(DiLabelVisibility::Explicit),
            style: None,
        })
    }
}
impl ProcessCraft {
    pub(super) fn band_create_ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &BpmnDocument,
        diagram: &Diagram,
        editable: bool,
    ) {
        if self.band_draft.is_none() {
            if ui
                .add_enabled(
                    editable && !self.draft_dirty,
                    egui::Button::new("Add participant band…"),
                )
                .clicked()
                && self.commit_draft()
            {
                self.band_draft = Some(BandDraft {
                    diagram: diagram.id.selector(),
                    id: processcraft_core::ids::ElementIdentity::anonymous(),
                    owner: None,
                    participant: None,
                    kind: None,
                    visible: None,
                    bounds: Bounds {
                        x: 0.,
                        y: 0.,
                        width: 160.,
                        height: 30.,
                    },
                    error: String::new(),
                });
            }
            return;
        }
        let draft = self.band_draft.as_mut().unwrap();
        if draft.diagram != diagram.id.selector() {
            return;
        }
        let owners = diagram
            .shapes
            .iter()
            .enumerate()
            .filter_map(|(i, shape)| {
                document.choreographies.iter().find_map(|c| {
                    c.activities
                        .iter()
                        .find(|a| a.id == shape.element)
                        .map(|a| (shape, c, a, i))
                })
            })
            .collect::<Vec<_>>();
        let old = draft.owner.clone();
        egui::ComboBox::from_label("Activity representation")
            .selected_text(
                draft
                    .owner
                    .as_ref()
                    .and_then(|id| owners.iter().find(|(shape, _, _, _)| shape.id == *id))
                    .map_or("Select activity shape".into(), |(shape, _, _, i)| {
                        super::di::representation_label(
                            document,
                            &shape.element,
                            "Activity shape",
                            *i,
                        )
                    }),
            )
            .show_ui(ui, |ui| {
                for (shape, _, _, i) in &owners {
                    ui.selectable_value(
                        &mut draft.owner,
                        Some(shape.id.selector()),
                        super::di::representation_label(
                            document,
                            &shape.element,
                            "Activity shape",
                            *i,
                        ),
                    );
                }
            });
        if draft.owner != old {
            draft.participant = None;
            draft.kind = None;
            if let Some((shape, _, _, _)) = owners
                .iter()
                .find(|(shape, _, _, _)| Some(shape.id.selector()) == draft.owner)
            {
                draft.bounds.x = shape.bounds.x;
                draft.bounds.y = shape.bounds.y;
                draft.bounds.width = shape.bounds.width;
            }
        }
        let participants = draft
            .owner
            .as_ref()
            .and_then(|id| owners.iter().find(|(shape, _, _, _)| shape.id == *id))
            .map_or_else(Vec::new, |(_, c, a, _)| {
                c.participants
                    .iter()
                    .filter(|p| {
                        a.participant_refs.contains(
                            &processcraft_core::bpmn_activity::ModelReference::Local(
                                p.id.selector(),
                            ),
                        )
                    })
                    .collect::<Vec<_>>()
            });
        let old_participant = draft.participant.clone();
        egui::ComboBox::from_label("Participant")
            .selected_text(
                draft
                    .participant
                    .as_ref()
                    .and_then(|id| {
                        participants.iter().position(|p| p.id == *id).map(|i| {
                            local_choice_label(
                                participants[i].name.as_deref(),
                                "Participant",
                                &participants[i].id,
                                i + 1,
                            )
                        })
                    })
                    .unwrap_or_else(|| "Select participant".into()),
            )
            .show_ui(ui, |ui| {
                for (i, p) in participants.iter().enumerate() {
                    ui.selectable_value(
                        &mut draft.participant,
                        Some(p.id.selector()),
                        local_choice_label(p.name.as_deref(), "Participant", &p.id, i + 1),
                    );
                }
            });
        if draft.participant != old_participant {
            draft.kind = None;
        }
        let initiating = draft
            .owner
            .as_ref()
            .and_then(|id| owners.iter().find(|(shape, _, _, _)| shape.id == *id))
            .and_then(|(_, _, activity, _)| {
                draft.participant.as_ref().map(|participant| {
                    activity.initiating_participant
                        == Some(processcraft_core::bpmn_activity::ModelReference::Local(
                            participant.clone(),
                        ))
                })
            });
        egui::ComboBox::from_label("Band position and role")
            .selected_text(match draft.kind {
                None => "Select band",
                Some(ParticipantBandKind::TopInitiating) => "Top · initiating",
                Some(ParticipantBandKind::MiddleInitiating) => "Middle · initiating",
                Some(ParticipantBandKind::BottomInitiating) => "Bottom · initiating",
                Some(ParticipantBandKind::TopNonInitiating) => "Top · non-initiating",
                Some(ParticipantBandKind::MiddleNonInitiating) => "Middle · non-initiating",
                Some(ParticipantBandKind::BottomNonInitiating) => "Bottom · non-initiating",
            })
            .show_ui(ui, |ui| {
                for (value, label) in [
                    (ParticipantBandKind::TopInitiating, "Top · initiating"),
                    (ParticipantBandKind::MiddleInitiating, "Middle · initiating"),
                    (ParticipantBandKind::BottomInitiating, "Bottom · initiating"),
                    (
                        ParticipantBandKind::TopNonInitiating,
                        "Top · non-initiating",
                    ),
                    (
                        ParticipantBandKind::MiddleNonInitiating,
                        "Middle · non-initiating",
                    ),
                    (
                        ParticipantBandKind::BottomNonInitiating,
                        "Bottom · non-initiating",
                    ),
                ] {
                    let value_initiates = matches!(
                        value,
                        ParticipantBandKind::TopInitiating
                            | ParticipantBandKind::MiddleInitiating
                            | ParticipantBandKind::BottomInitiating
                    );
                    if initiating == Some(value_initiates) {
                        ui.selectable_value(&mut draft.kind, Some(value), label);
                    }
                }
            });
        super::catalog::optional_bool(ui, "Message visible", &mut draft.visible);
        for (label, value) in [
            ("X", &mut draft.bounds.x),
            ("Y", &mut draft.bounds.y),
            ("Width", &mut draft.bounds.width),
            ("Height", &mut draft.bounds.height),
        ] {
            ui.horizontal(|ui| {
                let label = ui.label(label);
                ui.add(egui::DragValue::new(value)).labelled_by(label.id);
            });
        }
        if !draft.error.is_empty() {
            ui.label(&draft.error);
        }
        if ui.button("Create participant band").clicked() {
            let _ = self.apply_band_draft();
        }
        if ui.button("Cancel participant band").clicked() {
            self.band_draft = None;
        }
    }
    /// A band draft where nothing was chosen yet.
    pub(super) fn band_draft_untouched(&self) -> bool {
        self.band_draft
            .as_ref()
            .is_some_and(|d| d.owner.is_none() && d.participant.is_none() && d.kind.is_none())
    }
    pub(super) fn apply_band_draft(&mut self) -> Result<(), String> {
        self.apply_typed_draft(
            |app| &mut app.band_draft,
            |draft, document| {
                let shape = draft.build().inspect_err(|e| draft.error = e.clone())?;
                Ok(vec![Mutation::CreateShape {
                    document_id: document.id,
                    diagram_id: draft.diagram.clone(),
                    shape,
                }])
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_band_uses_shared_history_and_exports() {
        use processcraft_core::bpmn_collaboration::{Choreography, ChoreographyActivity};
        let mut app = ProcessCraft::default();
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        let participant = |id: &str| Participant {
            interface_refs: vec![],
            endpoint_refs: vec![],
            id: id.parse().unwrap(),
            name: Some(id.into()),
            base: None,
            process: None,
            multiplicity: None,
        };
        let message = Flow {
            id: "MessageFlow".parse().unwrap(),
            kind: FlowKind::Message,
            source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Buyer".parse().unwrap(),
            )),
            target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                "Seller".parse().unwrap(),
            )),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            flow_metadata: None,
            condition: None,
            association_direction: None,
            message_ref: None,
        };
        let choreography = Choreography {
            kind: None,
            graph: None,
            extras: None,
            conversation_nodes: vec![],
            conversation_links: vec![],
            correlation_keys: vec![],
            id: "Choreography".parse().unwrap(),
            name: Some("Orders".into()),
            base: None,
            participants: vec![participant("Buyer"), participant("Seller")],
            message_flows: vec![message],
            activities: vec![ChoreographyActivity {
                loop_type: None,
                details: None,
                id: "Task".parse().unwrap(),
                name: Some("Order".into()),
                base: None,
                flow_metadata: None,
                correlation_keys: vec![],
                participant_refs: vec![
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Buyer".parse().unwrap(),
                    ),
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Seller".parse().unwrap(),
                    ),
                ],
                initiating_participant: Some(
                    processcraft_core::bpmn_activity::ModelReference::Local(
                        "Buyer".parse().unwrap(),
                    ),
                ),
                message_flow_refs: vec![processcraft_core::bpmn_activity::ModelReference::Local(
                    "MessageFlow".parse().unwrap(),
                )],
            }],
            sequence_flows: vec![],
        };
        let shape = Shape {
            id: "ActivityShape".parse().unwrap(),
            element: "Task".parse().unwrap(),
            bounds: Bounds {
                x: 0.,
                y: 0.,
                width: 160.,
                height: 100.,
            },
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            label: Some(Label::default()),
            label_visibility: Some(DiLabelVisibility::Explicit),
            style: None,
        };
        let diagram = Diagram {
            id: "ChoreographyDiagram".parse().unwrap(),
            plane_id: "ChoreographyPlane".parse().unwrap(),
            plane: "Choreography".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            plane_metadata: None,
            plane_order: None,
            shapes: vec![shape],
            edges: vec![],
        };
        assert!(app.ctl.mutate(vec![
            Mutation::CreateChoreography {
                document_id,
                choreography
            },
            Mutation::CreateDiagram {
                document_id,
                diagram
            }
        ]));
        let draft = BandDraft {
            diagram: "ChoreographyDiagram".parse().unwrap(),
            id: processcraft_core::ids::ElementIdentity::anonymous(),
            owner: Some("ActivityShape".parse().unwrap()),
            participant: Some("Buyer".parse().unwrap()),
            kind: Some(ParticipantBandKind::TopInitiating),
            visible: Some(false),
            bounds: Bounds {
                x: 0.,
                y: 0.,
                width: 160.,
                height: 30.,
            },
            error: String::new(),
        };
        assert!(app.ctl.mutate(vec![Mutation::CreateShape {
            document_id,
            diagram_id: draft.diagram.clone(),
            shape: draft.build().unwrap()
        }]));
        let snapshot = app.ctl.session.snapshot();
        let exported =
            processcraft_io::interchange::export_bpmn(&snapshot.project, document_id, true)
                .expect("authored band exports");
        let xml = String::from_utf8(exported.bytes).unwrap();
        assert!(xml.contains("participantBandKind=\"top_initiating\""));
        assert!(xml.contains("isMessageVisible=\"false\""));
        let imported =
            processcraft_io::interchange::prepare_import(xml.as_bytes(), "authored band")
                .expect("authored band imports");
        let band = imported
            .document
            .diagrams
            .iter()
            .flat_map(|diagram| &diagram.shapes)
            .find(|shape| shape.participant_band_kind.is_some())
            .expect("band retained");
        assert_eq!(band.is_message_visible, Some(false));
        assert_eq!(
            band.participant_band_kind,
            Some(ParticipantBandKind::TopInitiating)
        );
        app.history(Request::Undo);
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(
            snapshot.project.documents[0]
                .diagrams
                .iter()
                .find(|d| d.id == draft.diagram)
                .unwrap()
                .shapes
                .len(),
            1
        );
        app.history(Request::Redo);
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(
            snapshot.project.documents[0]
                .diagrams
                .iter()
                .find(|d| d.id == draft.diagram)
                .unwrap()
                .shapes
                .len(),
            2
        );
    }
    #[test]
    fn new_band_requires_explicit_choices_and_preserves_optional_visibility() {
        let mut draft = BandDraft {
            diagram: "Diagram".parse().unwrap(),
            id: "Band".parse().unwrap(),
            owner: None,
            participant: None,
            kind: None,
            visible: Some(false),
            bounds: Bounds {
                x: 1.,
                y: 2.,
                width: 160.,
                height: 30.,
            },
            error: String::new(),
        };
        assert!(draft.build().is_err());
        draft.owner = Some("ActivityShape".parse().unwrap());
        draft.participant = Some("Participant".parse().unwrap());
        draft.kind = Some(ParticipantBandKind::MiddleNonInitiating);
        let shape = draft.build().unwrap();
        assert_eq!(shape.is_message_visible, Some(false));
        assert_eq!(
            shape.participant_band_kind,
            Some(ParticipantBandKind::MiddleNonInitiating)
        );
        assert_eq!(
            shape.choreography_activity_shape,
            Some(DiReference::Local("ActivityShape".parse().unwrap()))
        );
        assert!(matches!(
            shape.label_visibility,
            Some(DiLabelVisibility::Explicit)
        ));
    }
}
