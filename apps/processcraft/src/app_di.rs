use super::*;

pub(super) enum DiDraft {
    Shape {
        diagram: ElementSelector,
        shape: ElementSelector,
        horizontal: Option<bool>,
        marker: Option<bool>,
        horizontal_allowed: bool,
        marker_allowed: bool,
        label: LabelDraft,
    },
    Edge {
        diagram: ElementSelector,
        edge: ElementSelector,
        source: Option<ElementSelector>,
        target: Option<ElementSelector>,
        label: LabelDraft,
    },
}

pub(super) struct LabelDraft {
    value: Option<processcraft_core::model::Label>,
    explicit: bool,
    kind: u8,
    qualified: Option<super::catalog::QNameDraft>,
}
impl LabelDraft {
    fn new(
        value: Option<processcraft_core::model::Label>,
        visibility: Option<processcraft_core::model::DiLabelVisibility>,
    ) -> Self {
        use processcraft_core::model::DiStyleRef;
        let qualified = value.as_ref().and_then(|l| match &l.style_ref {
            Some(DiStyleRef::Qualified(q)) => Some(super::catalog::QNameDraft::from(q)),
            _ => None,
        });
        let kind = match value.as_ref().and_then(|label| label.style_ref.as_ref()) {
            None => 0,
            Some(DiStyleRef::Local(_)) => 1,
            Some(DiStyleRef::Qualified(_)) => 2,
        };
        Self {
            value,
            explicit: visibility.is_some(),
            kind,
            qualified,
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, diagram: &Diagram) -> Result<(), String> {
        use processcraft_core::model::{DiStyleRef, Label};
        ui.separator();
        ui.strong("Diagram label");
        ui.checkbox(&mut self.explicit, "Use explicit label visibility");
        let mut present = self.value.is_some();
        ui.checkbox(&mut present, "Label present");
        if !present {
            self.value = None;
            return Ok(());
        }
        let label = self.value.get_or_insert(Label {
            id: None,
            bounds: None,
            style: None,
            style_ref: None,
        });
        let mut bounds = label.bounds.is_some();
        ui.checkbox(&mut bounds, "Label bounds present");
        if bounds {
            let bounds = label.bounds.get_or_insert(Bounds {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 30.,
            });
            for (name, value) in [
                ("X", &mut bounds.x),
                ("Y", &mut bounds.y),
                ("Width", &mut bounds.width),
                ("Height", &mut bounds.height),
            ] {
                ui.horizontal(|ui| {
                    ui.label(name);
                    ui.add(egui::DragValue::new(value));
                });
            }
        } else {
            label.bounds = None;
        }
        let mut kind = self.kind;
        egui::ComboBox::from_label("Label style")
            .selected_text(match kind {
                1 => "Diagram style",
                2 => "Qualified reference",
                _ => "Unspecified",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut kind, 0, "Unspecified");
                ui.selectable_value(&mut kind, 1, "Diagram style");
                ui.selectable_value(&mut kind, 2, "Qualified reference");
            });
        self.kind = kind;
        match kind {
            0 => label.style_ref = None,
            1 => {
                let mut selected = match &label.style_ref {
                    Some(DiStyleRef::Local(v)) => Some(v.clone()),
                    _ => None,
                };
                egui::ComboBox::from_label("Style")
                    .selected_text(
                        selected
                            .as_ref()
                            .and_then(|id| {
                                diagram
                                    .label_styles
                                    .iter()
                                    .position(|s| s.id.as_ref().is_some_and(|i| i == id))
                            })
                            .map_or("Choose a style".to_owned(), |i| format!("Style {}", i + 1)),
                    )
                    .show_ui(ui, |ui| {
                        for (i, style) in diagram.label_styles.iter().enumerate() {
                            if let Some(id) = &style.id {
                                ui.selectable_value(
                                    &mut selected,
                                    Some(id.selector()),
                                    format!("Style {}", i + 1),
                                );
                            }
                        }
                    });
                label.style_ref = Some(DiStyleRef::Local(
                    selected.ok_or("Choose a diagram style.")?,
                ));
            }
            _ => {
                self.qualified.get_or_insert_with(Default::default);
                super::catalog::optional_qname(ui, "Style reference", &mut self.qualified);
                label.style_ref =
                    super::catalog::q_build(&self.qualified)?.map(DiStyleRef::Qualified);
            }
        }
        Ok(())
    }
    fn visibility(&self) -> Option<processcraft_core::model::DiLabelVisibility> {
        self.explicit
            .then_some(processcraft_core::model::DiLabelVisibility::Explicit)
    }
}

fn representation_label(
    document: &BpmnDocument,
    element: &ElementSelector,
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
                                representation_label(document, &shape.element, "shape", index)
                            )),
                        )
                        .on_hover_text(shape.id.xml_id().map_or_else(
                            || "XML ID not assigned".to_owned(),
                            |id| format!("XML ID: {id}"),
                        ))
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
                            label: LabelDraft::new(shape.label.clone(), shape.label_visibility),
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
                                representation_label(document, &edge.element, "edge", index)
                            )),
                        )
                        .on_hover_text(edge.id.xml_id().map_or_else(
                            || "XML ID not assigned".to_owned(),
                            |id| format!("XML ID: {id}"),
                        ))
                        .clicked()
                        && self.commit_draft()
                    {
                        self.di_draft = Some(DiDraft::Edge {
                            diagram: diagram.id.selector(),
                            edge: edge.id.selector(),
                            source: edge.source_element.clone(),
                            target: edge.target_element.clone(),
                            label: LabelDraft::new(edge.label.clone(), edge.label_visibility),
                        });
                    }
                }
            });
            return;
        }
        let draft = self.di_draft.as_mut().unwrap();
        ui.strong("Diagram representation");
        let mut change = None;
        let mut label_change = None;
        match draft {
            DiDraft::Shape {
                diagram: diagram_id,
                shape,
                horizontal,
                marker,
                horizontal_allowed,
                marker_allowed,
                label,
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
                let label_error = label.ui(ui, diagram).err();
                if let Some(error) = &label_error {
                    ui.label(error);
                }
                if ui
                    .add_enabled(
                        label_error.is_none(),
                        egui::Button::new("Apply diagram properties"),
                    )
                    .clicked()
                {
                    label_change = Some(Mutation::SetShapeLabel {
                        document_id: document.id,
                        diagram_id: diagram_id.clone(),
                        shape_id: shape.clone(),
                        label: label.value.clone(),
                        label_visibility: label.visibility(),
                    });
                    change = Some(Mutation::SetShapeDiAttributes {
                        document_id: document.id,
                        diagram_id: diagram_id.clone(),
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
                label,
            } => {
                let candidates = diagram
                    .shapes
                    .iter()
                    .enumerate()
                    .map(|(index, shape)| {
                        (
                            shape.id.selector(),
                            representation_label(document, &shape.element, "Shape", index),
                        )
                    })
                    .chain(diagram.edges.iter().enumerate().map(|(index, edge)| {
                        (
                            edge.id.selector(),
                            representation_label(document, &edge.element, "Edge", index),
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
                let label_error = label.ui(ui, diagram).err();
                if let Some(error) = &label_error {
                    ui.label(error);
                }
                if ui
                    .add_enabled(
                        label_error.is_none(),
                        egui::Button::new("Apply diagram properties"),
                    )
                    .clicked()
                {
                    label_change = Some(Mutation::SetEdgeLabel {
                        document_id: document.id,
                        diagram_id: diagram_id.clone(),
                        edge_id: edge.clone(),
                        label: label.value.clone(),
                        label_visibility: label.visibility(),
                    });
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
            && self.ctl.mutate(vec![
                change,
                label_change.expect("label change accompanies diagram change"),
            ])
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
            label: LabelDraft::new(None, None),
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
