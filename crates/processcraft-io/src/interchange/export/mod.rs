//! Export half: serialize a typed document to BPMN XML and verify it round-trips.
pub(super) mod choreography;
pub(super) mod collaboration;
pub(super) mod document;
pub(super) mod elements;
pub(super) mod owner_metadata;
pub(super) mod scope;
pub(super) mod xml;
use processcraft_core::ids::{ElementSelector, XmlId};
use processcraft_core::model::*;
/// Caller must additionally account for project-level dictionary and navigation links.
pub(super) struct ExportContext<'a> {
    pub(super) document: &'a BpmnDocument,
    pub(super) xml_ids: std::collections::HashMap<ElementSelector, &'a XmlId>,
}
impl<'a> ExportContext<'a> {
    pub(super) fn new(document: &'a BpmnDocument) -> Self {
        let xml_ids = document
            .element_identities()
            .into_iter()
            .filter_map(|identity| identity.xml_id().map(|id| (identity.selector(), id)))
            .collect();
        Self { document, xml_ids }
    }
}
impl std::ops::Deref for ExportContext<'_> {
    type Target = BpmnDocument;
    fn deref(&self) -> &Self::Target {
        self.document
    }
}
