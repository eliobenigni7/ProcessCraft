//! Typed BaseElement identity classification, separate from legacy navigation anchors.
use crate::{
    bpmn_metadata::{Documentation, DocumentationSet},
    bpmn_model::{Expression, ExpressionBody, ExpressionPart},
    ids::ElementSelector,
    model::BpmnDocument,
};
use std::collections::HashSet;
impl BpmnDocument {
    /// Semantic BPMN BaseElement identities, excluding Definitions, DI and documentation.
    /// Retains the historical `semantic_ids` behavior separately for archive navigation.
    pub fn base_element_selectors(&self) -> HashSet<ElementSelector> {
        let mut ids: HashSet<_> = self
            .element_identities()
            .into_iter()
            .map(|id| id.selector())
            .collect();
        ids.remove(&self.definitions_id.selector());
        for diagram in &self.diagrams {
            ids.remove(&diagram.id.selector());
            ids.remove(&diagram.plane_id.selector());
            for style in &diagram.label_styles {
                if let Some(id) = &style.id {
                    ids.remove(&id.selector());
                }
            }
            for shape in &diagram.shapes {
                ids.remove(&shape.id.selector());
                if let Some(id) = shape.label.as_ref().and_then(|l| l.id.as_ref()) {
                    ids.remove(&id.selector());
                }
            }
            for edge in &diagram.edges {
                ids.remove(&edge.id.selector());
                if let Some(id) = edge.label.as_ref().and_then(|l| l.id.as_ref()) {
                    ids.remove(&id.selector());
                }
            }
        }
        fn remove_doc(ids: &mut HashSet<ElementSelector>, doc: &Documentation) {
            if let Some(id) = &doc.id {
                ids.remove(&id.selector());
            }
        }
        fn remove_set(ids: &mut HashSet<ElementSelector>, set: &DocumentationSet) {
            if let DocumentationSet::Structured(docs) = set {
                for doc in docs {
                    remove_doc(ids, doc)
                }
            }
        }
        crate::metadata_read::visit_base_metadata(self, |_, base| {
            if let Some(base) = base {
                for doc in &base.documentation {
                    remove_doc(&mut ids, doc)
                }
            }
        });
        crate::metadata_read::visit_legacy_metadata(self, |_, docs, _| remove_set(&mut ids, docs));
        let activity = crate::activity_read::activity_read(self);
        for base in activity.bases {
            for doc in &base.documentation {
                remove_doc(&mut ids, doc)
            }
        }
        let collaboration = crate::collaboration_read::collaboration_read(self);
        for base in collaboration.bases {
            for doc in &base.documentation {
                remove_doc(&mut ids, doc)
            }
        }
        for docs in collaboration.docs {
            remove_set(&mut ids, docs)
        }
        for extension in &self.extensions {
            for doc in &extension.documentation {
                remove_doc(&mut ids, doc)
            }
        }
        for expression in self.expressions() {
            if let Expression::Structured(data) = expression
                && let ExpressionBody::Mixed { parts } = &data.text
            {
                for part in parts {
                    if let ExpressionPart::Documentation { documentation } = part {
                        remove_doc(&mut ids, documentation)
                    }
                }
            }
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bpmn_metadata::Documentation,
        bpmn_model::{EventDefinition, ExpressionData, ExpressionKind},
        bpmn_xml::MixedXml,
        model::{Bounds, DiLabelVisibility, Diagram, Label, LabelStyle, Shape},
    };
    #[test]
    fn base_element_classifier_excludes_di_definitions_and_documentation() {
        let mut doc = BpmnDocument::new("m".into(), "defs".parse().unwrap());
        doc.extensions.push(crate::model::DefinitionsExtension {
            definition: None,
            must_understand: None,
            documentation: vec![Documentation {
                id: Some("doc".parse().unwrap()),
                text_format: None,
                content: MixedXml::Text("note".into()),
            }],
        });
        doc.root_event_definitions
            .push(EventDefinition::Conditional {
                id: Some("event".parse().unwrap()),
                base: None,
                expression: Expression::Structured(ExpressionData {
                    id: Some("expression".parse().unwrap()),
                    kind: ExpressionKind::Base {
                        explicit_type: false,
                    },
                    text: "x".into(),
                    namespaces: vec![],
                    foreign_attributes: vec![],
                }),
            });
        doc.diagrams.push(Diagram {
            id: "diagram".parse().unwrap(),
            plane_id: "plane".parse().unwrap(),
            plane: "event".parse().unwrap(),
            name: None,
            documentation: None,
            resolution: None,
            plane_order: None,
            plane_metadata: None,
            label_styles: vec![LabelStyle {
                id: Some("style".parse().unwrap()),
                font: None,
            }],
            shapes: vec![Shape {
                id: "shape".parse().unwrap(),
                element: "event".parse().unwrap(),
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                is_expanded: None,
                is_horizontal: None,
                is_marker_visible: None,
                is_message_visible: None,
                participant_band_kind: None,
                choreography_activity_shape: None,
                di_metadata: None,
                label: Some(Label {
                    id: Some("label".parse().unwrap()),
                    ..Default::default()
                }),
                label_visibility: Some(DiLabelVisibility::Explicit),
                style: None,
            }],
            edges: vec![],
        });
        let selectors = doc.base_element_selectors();
        for value in ["event", "expression"] {
            assert!(selectors.contains(&value.parse().unwrap()));
        }
        for value in ["defs", "doc", "diagram", "plane", "style", "shape", "label"] {
            assert!(!selectors.contains(&value.parse().unwrap()));
        }
    }
}
