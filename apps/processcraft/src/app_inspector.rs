use super::{ProcessCraft, node_kind_label};
use crate::widgets;

impl ProcessCraft {
    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui, show_heading: bool) {
        let snap = self.ctl.session.snapshot();
        let editable = snap
            .project
            .documents
            .get(self.doc)
            .is_some_and(|d| d.capability.allows_structural_editing());
        let process = snap.project.documents.get(self.doc).and_then(|d| {
            processcraft_core::queries::scopes(d)
                .into_iter()
                .find(|p| self.process_id.as_ref().is_none_or(|id| p.id == id))
        });
        let palette = self.theme.palette();
        if show_heading {
            ui.label(egui::RichText::new("Properties").size(17.0).strong());
            ui.add_space(4.0);
            ui.separator();
        }
        egui::ScrollArea::vertical()
            .id_salt("inspector_scroll")
            .show(ui, |side| {
                side.spacing_mut().text_edit_width = (side.available_width() - 8.0).max(40.0);
                if let Some(selected) = self.selected.as_ref() {
                    let kind = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_node(d, selected))
                        .map_or(
                            if self.is_flow {
                                "Connection"
                            } else {
                                "Selection"
                            },
                            |n| node_kind_label(n.kind),
                        );
                    widgets::section_label(side, kind);
                    side.label(
                        egui::RichText::new(if self.draft.is_empty() {
                            "Untitled"
                        } else {
                            &self.draft
                        })
                        .size(20.0),
                    );
                    side.label(
                        egui::RichText::new(format!("ID · {selected}"))
                            .small()
                            .color(palette.muted),
                    )
                    .on_hover_text(selected.to_string());
                    side.add_space(8.0);
                    let annotation = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_node(d, selected))
                        .is_some_and(|n| {
                            n.kind == processcraft_core::model::NodeKind::TextAnnotation
                        });
                    let label = side.label(
                        egui::RichText::new(if annotation {
                            "Annotation text"
                        } else {
                            "Name"
                        })
                        .small()
                        .color(palette.muted),
                    );
                    let response = if annotation {
                        side.add(egui::TextEdit::multiline(&mut self.draft).desired_rows(4))
                    } else {
                        side.text_edit_singleline(&mut self.draft)
                    };
                    if response.labelled_by(label.id).changed() {
                        self.draft_dirty = true;
                    }
                    if self.is_node || self.is_flow {
                        let label = side.label(
                            egui::RichText::new("Documentation")
                                .small()
                                .color(palette.muted),
                        );
                        if side
                            .add(
                                egui::TextEdit::multiline(&mut self.docs)
                                    .hint_text("Describe this activity and its outcome")
                                    .desired_rows(4),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    }
                    let selected_flow = snap
                        .project
                        .documents
                        .get(self.doc)
                        .and_then(|d| processcraft_core::queries::find_flow(d, selected));
                    if self.is_flow
                        && selected_flow
                            .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Sequence)
                    {
                        let label = side.label(
                            egui::RichText::new("Condition")
                                .small()
                                .color(palette.muted),
                        );
                        if side
                            .text_edit_multiline(&mut self.condition)
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    } else if self.is_node {
                        let label =
                            side.label(egui::RichText::new("Owner").small().color(palette.muted));
                        if side
                            .add(
                                egui::TextEdit::singleline(&mut self.owner)
                                    .hint_text("Optional owner"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                        let label =
                            side.label(egui::RichText::new("Contact").small().color(palette.muted));
                        if side
                            .add(
                                egui::TextEdit::singleline(&mut self.contact)
                                    .hint_text("Optional contact details"),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            self.draft_dirty = true;
                        }
                    }
                    if selected_flow
                        .is_some_and(|f| f.kind == processcraft_core::model::FlowKind::Association)
                    {
                        use processcraft_core::bpmn_model::AssociationDirection as A;
                        let before = self.association_direction;
                        egui::ComboBox::from_label("Association direction")
                            .selected_text(
                                self.association_direction
                                    .map_or("None (default)".into(), |v| format!("{v:?}")),
                            )
                            .show_ui(side, |ui| {
                                ui.selectable_value(
                                    &mut self.association_direction,
                                    None,
                                    "None (default)",
                                );
                                for direction in [A::None, A::One, A::Both] {
                                    ui.selectable_value(
                                        &mut self.association_direction,
                                        Some(direction),
                                        format!("{direction:?}"),
                                    );
                                }
                            });
                        self.draft_dirty |= before != self.association_direction;
                    }
                    if self.is_node {
                        self.bpmn_properties_ui(side, editable);
                        self.dictionary_links_ui(side);
                    }
                    side.add_space(8.0);
                    if side
                        .add_enabled_ui(self.draft_dirty, |ui| {
                            widgets::primary_button(ui, "Apply properties")
                        })
                        .inner
                        .clicked()
                    {
                        self.commit_draft();
                    }
                    if side.button("Element comments…").clicked() {
                        self.open_comments();
                    }
                    side.add_space(8.0);
                    egui::CollapsingHeader::new("Advanced").show(side, |side| {
                        if self.is_flow && side.button("Insert activity on selected flow").clicked()
                        {
                            self.splice_selected();
                        }
                        if side
                            .add_enabled(editable, egui::Button::new("Reroute selected connection"))
                            .clicked()
                        {
                            self.reroute_selected();
                        }
                        side.label("Move selected");
                        side.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(editable, egui::Button::new("Left"))
                                .clicked()
                            {
                                self.move_selected(-10.0, 0.0);
                            }
                            if ui.add_enabled(editable, egui::Button::new("Up")).clicked() {
                                self.move_selected(0.0, -10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Down"))
                                .clicked()
                            {
                                self.move_selected(0.0, 10.0);
                            }
                            if ui
                                .add_enabled(editable, egui::Button::new("Right"))
                                .clicked()
                            {
                                self.move_selected(10.0, 0.0);
                            }
                        });
                        if side
                            .add_enabled(editable, egui::Button::new("Delete selected"))
                            .clicked()
                        {
                            self.delete();
                        }
                    });
                } else {
                    widgets::section_label(side, "Project and document");
                    side.label(
                        egui::RichText::new(
                            "Select an element on the canvas to edit its properties.",
                        )
                        .small()
                        .color(palette.muted),
                    );
                    side.add_space(8.0);
                    let name = side.label(
                        egui::RichText::new("Project name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.project_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    let name = side.label(
                        egui::RichText::new("Document name")
                            .small()
                            .color(palette.muted),
                    );
                    if side
                        .text_edit_singleline(&mut self.document_name)
                        .labelled_by(name.id)
                        .changed()
                    {
                        self.draft_dirty = true;
                    }
                    if side
                        .add_enabled_ui(self.draft_dirty, |ui| {
                            widgets::primary_button(ui, "Apply names")
                        })
                        .inner
                        .clicked()
                    {
                        self.commit_draft();
                    }
                }
                if let Some(p) = process {
                    side.separator();
                    egui::CollapsingHeader::new("Connections and lanes").show(side, |side| {
                        let lane_names: Vec<_> = p.lanes.iter().map(|l| l.name.clone()).collect();
                        egui::ComboBox::from_label("Assign selected node to lane")
                            .selected_text(if self.lane == 0 {
                                "No lane"
                            } else {
                                lane_names
                                    .get(self.lane - 1)
                                    .map_or("No lane", String::as_str)
                            })
                            .show_ui(side, |ui| {
                                ui.selectable_value(&mut self.lane, 0, "No lane");
                                for (i, n) in lane_names.iter().enumerate() {
                                    ui.selectable_value(&mut self.lane, i + 1, n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Apply lane"))
                            .clicked()
                        {
                            self.assign_lane();
                        }
                        side.strong("Connect nodes");
                        egui::ComboBox::from_label("Connection type")
                            .selected_text(format!("{:?}", self.connection_kind))
                            .show_ui(side, |ui| {
                                ui.selectable_value(
                                    &mut self.connection_kind,
                                    processcraft_core::model::FlowKind::Sequence,
                                    "Sequence flow",
                                );
                                ui.selectable_value(
                                    &mut self.connection_kind,
                                    processcraft_core::model::FlowKind::Association,
                                    "Association",
                                );
                            });
                        let names: Vec<_> = p.nodes.iter().map(|n| n.name.clone()).collect();
                        egui::ComboBox::from_label("Source")
                            .selected_text(names.get(self.source).map_or("Choose", String::as_str))
                            .show_ui(side, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.source, i, n);
                                }
                            });
                        egui::ComboBox::from_label("Target")
                            .selected_text(names.get(self.target).map_or("Choose", String::as_str))
                            .show_ui(side, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut self.target, i, n);
                                }
                            });
                        if side
                            .add_enabled(editable, egui::Button::new("Create connection"))
                            .clicked()
                        {
                            self.connect();
                        }
                    });
                }
            });
    }
}

impl ProcessCraft {
    fn bpmn_properties_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        use processcraft_core::bpmn_model::{EventDefinition as E, NodeDetails as D, TimerKind};
        use processcraft_core::{model::NodeKind as K, queries};
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let Some(node) = self
            .selected
            .as_ref()
            .and_then(|id| queries::find_node(document, id))
        else {
            return;
        };
        if let Some(D::SubProcess { .. }) = &node.details {
            ui.separator();
            if ui.button("Work inside subprocess").clicked() && self.prepare_navigation() {
                self.process_id = Some(node.id.clone());
                self.select(None);
            }
        }
        let event = matches!(
            node.kind,
            K::StartEvent
                | K::EndEvent
                | K::IntermediateCatchEvent
                | K::IntermediateThrowEvent
                | K::BoundaryEvent
        );
        if node.kind.is_activity() {
            self.activity_properties_ui(ui, node, document, editable);
            ui.add_enabled_ui(editable, |ui| {
                ui.collapsing("Loop characteristics", |ui| {
                    use processcraft_core::bpmn_model::LoopCharacteristics as L;
                    let mut value = self.bpmn_loop.clone();
                    let mut tag = match &value {
                        None => "None",
                        Some(L::Standard { .. }) => "Standard loop",
                        Some(L::MultiInstance {
                            is_sequential: true,
                            ..
                        }) => "Sequential instances",
                        Some(L::MultiInstance { .. }) => "Parallel instances",
                    };
                    let old_tag = tag;
                    egui::ComboBox::from_label("Repetition")
                        .selected_text(tag)
                        .show_ui(ui, |ui| {
                            for option in [
                                "None",
                                "Standard loop",
                                "Sequential instances",
                                "Parallel instances",
                            ] {
                                ui.selectable_value(&mut tag, option, option);
                            }
                        });
                    if tag != old_tag {
                        value = match tag {
                            "None" => None,
                            "Standard loop" => Some(L::Standard {
                                test_before: false,
                                condition: None,
                                maximum: None,
                            }),
                            _ => Some(L::MultiInstance {
                                is_sequential: tag == "Sequential instances",
                                cardinality: None,
                                completion_condition: None,
                            }),
                        };
                    }
                    let text = |ui: &mut egui::Ui, label: &str, value: &mut Option<String>| {
                        let mut current = value.clone().unwrap_or_default();
                        let label = ui.label(label);
                        if ui
                            .text_edit_singleline(&mut current)
                            .labelled_by(label.id)
                            .changed()
                        {
                            *value = (!current.is_empty()).then_some(current);
                        }
                    };
                    match &mut value {
                        Some(L::Standard {
                            test_before,
                            condition,
                            maximum,
                        }) => {
                            ui.checkbox(test_before, "Test before first iteration");
                            text(ui, "Loop condition", condition);
                            text(ui, "Maximum iterations", maximum);
                        }
                        Some(L::MultiInstance {
                            cardinality,
                            completion_condition,
                            ..
                        }) => {
                            text(ui, "Cardinality expression", cardinality);
                            text(ui, "Completion condition", completion_condition);
                        }
                        None => {}
                    }
                    if value != self.bpmn_loop {
                        self.bpmn_loop = value;
                        self.draft_dirty = true;
                    }
                });
            });
        }
        if matches!(
            node.kind,
            K::ExclusiveGateway
                | K::InclusiveGateway
                | K::ParallelGateway
                | K::ComplexGateway
                | K::EventBasedGateway
        ) {
            ui.add_enabled_ui(editable, |ui| {
                use processcraft_core::bpmn_model::{EventGatewayType as T, GatewayDirection as G};
                let mut value = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if let D::Gateway {
                    default_flow,
                    direction,
                    instantiate,
                    event_gateway_type,
                    activation_condition,
                } = &mut value
                {
                    ui.separator();
                    ui.strong("Gateway behavior");
                    egui::ComboBox::from_label("Direction")
                        .selected_text(direction.map_or("Unspecified".into(), |v| format!("{v:?}")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(direction, None, "Unspecified (omitted)");
                            for v in [G::Unspecified, G::Converging, G::Diverging, G::Mixed] {
                                ui.selectable_value(direction, Some(v), format!("{v:?}"));
                            }
                        });
                    if matches!(
                        node.kind,
                        K::ExclusiveGateway | K::InclusiveGateway | K::ComplexGateway
                    ) {
                        let outgoing: Vec<_> = queries::scopes(document)
                            .into_iter()
                            .flat_map(|s| s.flows)
                            .filter(|f| {
                                f.source == node.id
                                    && f.kind == processcraft_core::model::FlowKind::Sequence
                            })
                            .collect();
                        egui::ComboBox::from_label("Default outgoing flow")
                            .selected_text(default_flow.as_ref().map_or("None".into(), |id| {
                                queries::element_name(document, id)
                                    .filter(|s| !s.is_empty())
                                    .map_or_else(|| id.to_string(), str::to_owned)
                            }))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(default_flow, None, "None");
                                for flow in outgoing {
                                    ui.selectable_value(
                                        default_flow,
                                        Some(flow.id.clone()),
                                        if flow.name.is_empty() {
                                            flow.id.to_string()
                                        } else {
                                            flow.name.clone()
                                        },
                                    );
                                }
                            });
                    }
                    if node.kind == K::EventBasedGateway {
                        let mut starts = instantiate.unwrap_or(false);
                        if ui.checkbox(&mut starts, "Instantiate process").changed() {
                            *instantiate = Some(starts);
                        }
                        egui::ComboBox::from_label("Event gateway type")
                            .selected_text(
                                event_gateway_type
                                    .map_or("Exclusive (default)".into(), |v| format!("{v:?}")),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    event_gateway_type,
                                    None,
                                    "Exclusive (default)",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Exclusive),
                                    "Exclusive",
                                );
                                ui.selectable_value(
                                    event_gateway_type,
                                    Some(T::Parallel),
                                    "Parallel",
                                );
                            });
                    }
                    if node.kind == K::ComplexGateway {
                        let mut expression = activation_condition.clone().unwrap_or_default();
                        let label = ui.label("Activation condition");
                        if ui
                            .text_edit_multiline(&mut expression)
                            .labelled_by(label.id)
                            .changed()
                        {
                            *activation_condition = (!expression.is_empty()).then_some(expression);
                        }
                    }
                }
                let original = self.bpmn_details.clone().unwrap_or(D::Gateway {
                    default_flow: None,
                    direction: None,
                    instantiate: None,
                    event_gateway_type: None,
                    activation_condition: None,
                });
                if value != original {
                    self.bpmn_details = Some(value);
                    self.draft_dirty = true;
                }
            });
        }
        if !event {
            return;
        }
        ui.separator();
        ui.strong("Event definition");
        ui.add_enabled_ui(editable, |ui| {
            // Multiple and referenced definitions stay intact until an explicit multi-definition editor exists.
            if matches!(&self.bpmn_details, Some(D::Event { definitions, .. }) if definitions.len() > 1) {
                ui.label("Multiple definitions retained. Single-definition editing is unavailable.");
                return;
            }
            let (definitions, attached_to, cancel_activity) = match &self.bpmn_details {
                Some(D::Event { definitions, attached_to, cancel_activity }) => (definitions.clone(), attached_to.clone(), *cancel_activity),
                Some(D::BoundaryEvent { attached_to, cancel_activity }) => (vec![], Some(attached_to.clone()), Some(*cancel_activity)),
                _ => (vec![], None, None),
            };
            let mut tag = match definitions.first() {
                None => "None", Some(E::Timer{..}) => "Timer", Some(E::Conditional{..}) => "Conditional",
                Some(E::Terminate{..}) => "Terminate", Some(E::Message{..}) => "Message", Some(E::Signal{..}) => "Signal",
                Some(E::Error{..}) => "Error", Some(E::Escalation{..}) => "Escalation", _ => "Retained definition",
            };
            let original_tag = tag;
            egui::ComboBox::from_label("Definition type").selected_text(tag).show_ui(ui, |ui| {
                for option in ["None", "Timer", "Conditional", "Terminate", "Message", "Signal", "Error", "Escalation"] {
                    let allowed = match option {
                        "Timer" | "Conditional" => matches!(node.kind,K::StartEvent | K::IntermediateCatchEvent | K::BoundaryEvent),
                        "Terminate" => node.kind == K::EndEvent,
                        "Error" => matches!(node.kind,K::EndEvent | K::BoundaryEvent),
                        "None" => node.kind != K::BoundaryEvent && node.kind != K::IntermediateCatchEvent,
                        _ => true,
                    };
                    if allowed { ui.selectable_value(&mut tag, option, option); }
                }
            });
            let mut definition = if tag == original_tag { definitions.first().cloned() } else {
                match tag {
                    "Timer" => Some(E::Timer { id: None, kind: TimerKind::Duration, expression: "PT1M".into() }),
                    "Conditional" => Some(E::Conditional { id: None, expression: String::new() }),
                    "Terminate" => Some(E::Terminate { id: None }),
                    "Message" => Some(E::Message { id: None, message_ref: None, operation_ref: None }),
                    "Signal" => Some(E::Signal { id: None, signal_ref: None }),
                    "Error" => Some(E::Error { id: None, error_ref: None }),
                    "Escalation" => Some(E::Escalation { id: None, escalation_ref: None }), _ => None,
                }
            };
            let mut changed = tag != original_tag;
            match &mut definition {
                Some(E::Timer { kind, expression, .. }) => {
                    let old = *kind;
                    egui::ComboBox::from_label("Timer expression").selected_text(format!("{kind:?}")).show_ui(ui, |ui| {
                        ui.selectable_value(kind, TimerKind::Date, "Date");
                        ui.selectable_value(kind, TimerKind::Duration, "Duration");
                        ui.selectable_value(kind, TimerKind::Cycle, "Cycle");
                    });
                    changed |= old != *kind;
                    let label = ui.label("Expression");
                    changed |= ui.text_edit_singleline(expression).labelled_by(label.id).changed();
                }
                Some(E::Conditional { expression, .. }) => {
                    let label = ui.label("Condition expression");
                    changed |= ui.text_edit_multiline(expression).labelled_by(label.id).changed();
                }
                _ => {}
            }
            let mut cancel_activity = cancel_activity;
            if node.kind == K::BoundaryEvent {
                if let Some(target) = &attached_to { ui.label(format!("Attached to: {}", queries::element_name(document, target).unwrap_or("Unnamed activity"))); }
                let mut interrupt = cancel_activity.unwrap_or(true);
                if ui.checkbox(&mut interrupt, "Interrupt activity").changed() { cancel_activity = Some(interrupt); changed = true; }
            }
            if changed {
                self.bpmn_details = if definition.is_none() && attached_to.is_none() { None } else {
                    Some(D::Event { definitions: definition.into_iter().collect(), attached_to, cancel_activity })
                };
                self.draft_dirty = true;
            }
        });
    }
}

impl ProcessCraft {
    fn activity_properties_ui(
        &mut self,
        ui: &mut egui::Ui,
        node: &processcraft_core::model::Node,
        document: &processcraft_core::model::BpmnDocument,
        editable: bool,
    ) {
        use processcraft_core::{
            bpmn_activity::AdHocOrdering,
            model::{FlowKind, NodeKind as K},
            queries,
        };
        ui.add_enabled_ui(editable, |ui| {
            ui.collapsing("Activity behavior", |ui| {
                let mut value = self.bpmn_activity.clone().unwrap_or_default();
                let checkbox = |ui: &mut egui::Ui, label: &str, value: &mut Option<bool>, default: bool| {
                    let mut checked = value.unwrap_or(default);
                    if ui.checkbox(&mut checked, label).changed() { *value = Some(checked); }
                };
                let text = |ui: &mut egui::Ui, label: &str, value: &mut Option<String>, multiline: bool| {
                    let mut draft = value.clone().unwrap_or_default();
                    let label = ui.label(label);
                    let response = if multiline { ui.add(egui::TextEdit::multiline(&mut draft).desired_rows(4)) } else { ui.text_edit_singleline(&mut draft) };
                    if response.labelled_by(label.id).changed() { *value = (!draft.is_empty()).then_some(draft); }
                };
                checkbox(ui, "For compensation", &mut value.is_for_compensation, false);
                text(ui, "Start quantity (default 1)", &mut value.start_quantity, false);
                text(ui, "Completion quantity (default 1)", &mut value.completion_quantity, false);
                let outgoing: Vec<_> = queries::scopes(document).into_iter().flat_map(|s|s.flows).filter(|f|f.kind == FlowKind::Sequence && f.source == node.id).collect();
                egui::ComboBox::from_label("Activity default flow").selected_text(value.default_flow.as_ref().map_or("None".into(), |id| queries::element_name(document,id).filter(|s|!s.is_empty()).map_or_else(||id.to_string(),str::to_owned))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut value.default_flow,None,"None");
                    for flow in outgoing { ui.selectable_value(&mut value.default_flow,Some(flow.id.clone()), if flow.name.is_empty() {flow.id.to_string()} else {flow.name.clone()}); }
                });
                if node.kind == K::SubProcess { checkbox(ui,"Triggered by event",&mut value.triggered_by_event,false); }
                if node.kind == K::AdHocSubProcess {
                    egui::ComboBox::from_label("Ad-hoc ordering").selected_text(value.ad_hoc_ordering.map_or("Parallel (default)".into(), |v|format!("{v:?}"))).show_ui(ui, |ui| {
                        ui.selectable_value(&mut value.ad_hoc_ordering,None,"Parallel (default)");
                        ui.selectable_value(&mut value.ad_hoc_ordering,Some(AdHocOrdering::Parallel),"Parallel");
                        ui.selectable_value(&mut value.ad_hoc_ordering,Some(AdHocOrdering::Sequential),"Sequential");
                    });
                    checkbox(ui,"Cancel remaining instances",&mut value.cancel_remaining_instances,true);
                    text(ui,"Ad-hoc completion condition",&mut value.completion_condition,true);
                }
                if node.kind == K::Transaction { text(ui,"Transaction method (default ##Compensate)",&mut value.transaction_method,false); }
                if matches!(node.kind,K::ServiceTask|K::SendTask|K::ReceiveTask|K::UserTask|K::BusinessRuleTask) { text(ui,"Implementation",&mut value.implementation,false); }
                if node.kind == K::ReceiveTask { checkbox(ui,"Instantiate process",&mut value.instantiate,false); }
                if node.kind == K::ScriptTask {
                    ui.label("Script source is stored as model data and is never executed by ProcessCraft.");
                    text(ui,"Script format",&mut value.script_format,false);
                    text(ui,"Script source",&mut value.script,true);
                }
                let original = self.bpmn_activity.clone().unwrap_or_default();
                if value != original { self.bpmn_activity = Some(value); self.draft_dirty = true; }
            });
        });
    }
}
