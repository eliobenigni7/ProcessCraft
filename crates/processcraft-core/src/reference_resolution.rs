//! Namespace-safe reference resolution; never match a foreign QName by local part.
use crate::{
    bpmn_activity::ModelReference,
    ids::{ElementSelector, XmlId},
    model::{BpmnDocument, GENERATED_TARGET_NAMESPACE},
};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceResolution {
    Resolved(ElementSelector),
    MissingLocal,
    External,
    NamespaceUnknown,
}
/// Build once for multiple lookups; caller checks the resolved element's kind and scope.
pub struct ReferenceIndex {
    selectors: HashSet<ElementSelector>,
    xml_ids: HashMap<XmlId, ElementSelector>,
    namespace: Option<String>,
}
impl ReferenceIndex {
    pub fn new(document: &BpmnDocument) -> Self {
        let identities = document.element_identities();
        Self {
            selectors: identities.iter().map(|id| id.selector()).collect(),
            xml_ids: identities
                .iter()
                .filter_map(|id| id.xml_id().map(|xml| (xml.clone(), id.selector())))
                .collect(),
            namespace: document
                .definitions
                .as_ref()
                .map(|d| d.target_namespace.clone())
                .or_else(|| {
                    document
                        .provenance
                        .is_none()
                        .then(|| GENERATED_TARGET_NAMESPACE.to_owned())
                }),
        }
    }
    pub fn resolve(&self, reference: &ModelReference) -> ReferenceResolution {
        match reference {
            ModelReference::Local(id) => {
                if self.selectors.contains(id) {
                    ReferenceResolution::Resolved(id.clone())
                } else {
                    ReferenceResolution::MissingLocal
                }
            }
            ModelReference::Qualified(name) => match &self.namespace {
                None => ReferenceResolution::NamespaceUnknown,
                Some(namespace) if namespace != &name.namespace => ReferenceResolution::External,
                Some(_) => self.xml_ids.get(&name.local).cloned().map_or(
                    ReferenceResolution::MissingLocal,
                    ReferenceResolution::Resolved,
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bpmn_model::QNameRef, model::DefinitionsMetadata};
    #[test]
    fn qualified_reference_never_aliases_external_or_missing_namespace() {
        let mut doc = BpmnDocument::new("d".into(), "defs".parse().unwrap());
        doc.definitions = Some(DefinitionsMetadata {
            target_namespace: "urn:local".into(),
            name: None,
            expression_language: None,
            type_language: None,
            exporter: None,
            exporter_version: None,
            imports: vec![],
            namespaces: vec![],
            foreign_attributes: vec![],
        });
        let index = ReferenceIndex::new(&doc);
        let local = ModelReference::Qualified(QNameRef {
            namespace: "urn:local".into(),
            local: "defs".parse().unwrap(),
        });
        let external = ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "defs".parse().unwrap(),
        });
        assert!(matches!(
            index.resolve(&local),
            ReferenceResolution::Resolved(_)
        ));
        assert_eq!(index.resolve(&external), ReferenceResolution::External);
        doc.definitions = None;
        doc.provenance = Some(crate::model::Provenance {
            source_resource_id: "source".parse().unwrap(),
            source_name: "p".into(),
        });
        assert_eq!(
            ReferenceIndex::new(&doc).resolve(&local),
            ReferenceResolution::NamespaceUnknown
        );
    }
}
