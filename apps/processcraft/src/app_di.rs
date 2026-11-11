use super::*;

pub(super) enum DiDraft {
    Shape {
        diagram: ElementSelector,
        shape: ElementSelector,
        horizontal: Option<bool>,
        marker: Option<bool>,
        horizontal_allowed: bool,
        marker_allowed: bool,
    },
    Edge {
        diagram: ElementSelector,
        edge: ElementSelector,
        source: Option<ElementSelector>,
        target: Option<ElementSelector>,
    },
}
fn representation_label(
    document: &BpmnDocument,
    element: &ElementSelector,
    identity: &processcraft_core::ids::ElementIdentity,
    kind: &str,
    index: usize,
) -> String {
    let mut label = format!("{kind} {}", index + 1);
    if let Some(name) =
        processcraft_core::queries::element_name(document, element).filter(|name| !name.is_empty())
    {
        label.push_str(" · ");
        label.push_str(name);
    }
    if let Some(id) = identity.xml_id() {
        label.push_str(" · ");
        label.push_str(&id.to_string());
    }
    label
}

fn optional_flag(ui: &mut egui::Ui, label: &str, value: &mut Option<bool>, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_label(label)
            .selected_text(match value {
                None => "Unspecified",
                Some(false) => "No",
                Some(true) => "Yes",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(value, None, "Unspecified");
                ui.selectable_value(value, Some(false), "No");
                ui.selectable_value(value, Some(true), "Yes");
            });
    });
}
impl ProcessCraft {
    pub(super) fn di_ui(&mut self, ui: &mut egui::Ui, editable: bool) {
        let snapshot = self.ctl.session.snapshot();
        let Some(document) = snapshot.project.documents.get(self.doc) else {
            return;
        };
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let Some(diagram) = document
            .diagrams
            .iter()
            .find(|diagram| self.diagram_id.as_ref().is_none_or(|id| diagram.id == *id))
        else {
            return;
        };
        if self.di_draft.is_none() {
            ui.collapsing("Diagram representation", |ui| {
                for (index, shape) in diagram
                    .shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, shape)| shape.element == selected)
                {
                    if ui
                        .add_enabled(
                            editable
                                && self.expression_draft.is_none()
                                && self.catalog_draft.is_none()
                                && self.binding_draft.is_none()
                                && self.data_draft.is_none()
                                && self.xml_id_draft.is_none(),
                            egui::Button::new(format!(
                                "Edit {}",
                                representation_label(
                                    document,
                                    &shape.element,
                                    &shape.id,
                                    "shape",
                                    index
                                )
                            )),
                        )
                        .clicked()
                        && self.commit_draft()
                    {
                        let horizontal_allowed = document
                            .collaboration
                            .as_ref()
                            .is_some_and(|c| c.participants.iter().any(|p| p.id == selected))
                            || processcraft_core::queries::scopes(document)
                                .iter()
                                .any(|scope| scope.lanes.iter().any(|lane| lane.id == selected));
                        let marker_allowed =
                            processcraft_core::queries::find_node(document, &selected)
                                .is_some_and(|node| node.kind == NodeKind::ExclusiveGateway);
                        self.di_draft = Some(DiDraft::Shape {
                            diagram: diagram.id.selector(),
                            shape: shape.id.selector(),
                            horizontal: shape.is_horizontal,
                            marker: shape.is_marker_visible,
                            horizontal_allowed,
                            marker_allowed,
                        });
                    }
                }
                for (index, edge) in diagram
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, edge)| edge.element == selected)
                {
                    if ui
                        .add_enabled(
                            editable
                                && self.expression_draft.is_none()
                                && self.catalog_draft.is_none()
                                && self.binding_draft.is_none()
                                && self.data_draft.is_none()
                                && self.xml_id_draft.is_none(),
                            egui::Button::new(format!(
                                "Edit {}",
                                representation_label(
                                    document,
                                    &edge.element,
                                    &edge.id,
                                    "edge",
                                    index
                                )
                            )),
                        )
                        .clicked()
                        && self.commit_draft()
                    {
                        self.di_draft = Some(DiDraft::Edge {
                            diagram: diagram.id.selector(),
                            edge: edge.id.selector(),
                            source: edge.source_element.clone(),
                            target: edge.target_element.clone(),
                        });
                    }
                }
            });
            return;
        }
        let draft = self.di_draft.as_mut().unwrap();
        ui.strong("Diagram representation");
        let mut change = None;
        match draft {
            DiDraft::Shape {
                diagram,
                shape,
                horizontal,
                marker,
                horizontal_allowed,
                marker_allowed,
            } => {
                optional_flag(
                    ui,
                    "Horizontal orientation",
                    horizontal,
                    *horizontal_allowed,
                );
                optional_flag(ui, "Gateway marker visible", marker, *marker_allowed);
                if !*horizontal_allowed && horizontal.is_some() {
                    ui.label("Imported orientation retained for this shape.");
                }
                if !*marker_allowed && marker.is_some() {
                    ui.label("Imported marker visibility retained for this shape.");
                }
                if ui.button("Apply diagram properties").clicked() {
                    change = Some(Mutation::SetShapeDiAttributes {
                        document_id: document.id,
                        diagram_id: diagram.clone(),
                        shape_id: shape.clone(),
                        is_horizontal: *horizontal,
                        is_marker_visible: *marker,
                    });
                }
            }
            DiDraft::Edge {
                diagram: diagram_id,
                edge,
                source,
                target,
            } => {
                let candidates = diagram
                    .shapes
                    .iter()
                    .enumerate()
                    .map(|(index, shape)| {
                        (
                            shape.id.selector(),
                            representation_label(
                                document,
                                &shape.element,
                                &shape.id,
                                "Shape",
                                index,
                            ),
                        )
                    })
                    .chain(diagram.edges.iter().enumerate().map(|(index, edge)| {
                        (
                            edge.id.selector(),
                            representation_label(document, &edge.element, &edge.id, "Edge", index),
                        )
                    }))
                    .collect::<Vec<_>>();
                for (label, value) in [
                    ("Source representation", &mut *source),
                    ("Target representation", &mut *target),
                ] {
                    egui::ComboBox::from_label(label)
                        .selected_text(value.as_ref().map_or_else(
                            || "Unspecified".to_owned(),
                            |selected| {
                                candidates
                                    .iter()
                                    .find(|(candidate, _)| candidate == selected)
                                    .map_or_else(
                                        || "Unavailable representation".to_owned(),
                                        |(_, name)| name.clone(),
                                    )
                            },
                        ))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(value, None, "Unspecified");
                            for (candidate, name) in &candidates {
                                ui.selectable_value(value, Some(candidate.clone()), name);
                            }
                        });
                }
                if ui.button("Apply diagram properties").clicked() {
                    change = Some(Mutation::SetEdgeDiEndpoints {
                        document_id: document.id,
                        diagram_id: diagram_id.clone(),
                        edge_id: edge.clone(),
                        source_element: source.clone(),
                        target_element: target.clone(),
                    });
                }
            }
        }
        if let Some(change) = change
            && self.ctl.mutate(vec![change])
        {
            self.di_draft = None;
        }
        if ui.button("Cancel diagram editing").clicked() {
            self.di_draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagram_draft_blocks_navigation_and_retains_optional_flags() {
        let mut app = ProcessCraft::default();
        app.add(NodeKind::ExclusiveGateway);
        let selected = app.selected.clone();
        let snapshot = app.ctl.session.snapshot();
        let document = &snapshot.project.documents[0];
        let diagram = &document.diagrams[0];
        let shape = &diagram.shapes[0];
        app.di_draft = Some(DiDraft::Shape {
            diagram: diagram.id.selector(),
            shape: shape.id.selector(),
            horizontal: None,
            marker: Some(false),
            horizontal_allowed: false,
            marker_allowed: true,
        });
        assert!(!app.commit_draft());
        app.select(None);
        app.history(Request::Undo);
        assert_eq!(app.selected, selected);
        assert!(matches!(
            app.di_draft,
            Some(DiDraft::Shape {
                horizontal: None,
                marker: Some(false),
                ..
            })
        ));
        let document_id = document.id;
        let diagram_id = diagram.id.selector();
        let shape_id = shape.id.selector();
        app.di_draft = None;
        assert!(app.ctl.mutate(vec![Mutation::SetShapeDiAttributes {
            document_id,
            diagram_id,
            shape_id,
            is_horizontal: None,
            is_marker_visible: Some(false)
        }]));
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0].is_marker_visible,
            Some(false)
        );
        app.history(Request::Undo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0].is_marker_visible,
            None
        );
        app.history(Request::Redo);
        assert_eq!(
            app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0].is_marker_visible,
            Some(false)
        );
    }
}
