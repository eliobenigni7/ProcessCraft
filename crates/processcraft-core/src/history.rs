use crate::{
    ids::{ElementIdentity, ElementKey, ElementSelector, XmlId},
    model::*,
};
use std::{collections::HashSet, mem::size_of, sync::Arc};

pub(crate) const MAX_GESTURES: usize = 200;
pub(crate) const MAX_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone)]
pub(crate) struct HistoryEntry {
    pub project: Arc<ProjectData>,
    pub selection: Vec<ElementKey>,
    pub selector_mappings: Vec<crate::commands::SelectorMapping>,
}
#[derive(Clone, Default)]
pub(crate) struct History {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
}
impl History {
    pub fn push(&mut self, state: HistoryEntry) {
        self.push_with_budget(state, MAX_GESTURES, MAX_BYTES);
    }
    pub fn fits_single(state: HistoryEntry) -> bool {
        Self::fits_single_with_budget(state, MAX_GESTURES, MAX_BYTES)
    }
    fn fits_single_with_budget(state: HistoryEntry, count: usize, bytes: usize) -> bool {
        let mut history = Self::default();
        history.push_with_budget(state, count, bytes);
        !history.undo.is_empty()
    }
    pub fn push_required(&mut self, state: HistoryEntry) -> bool {
        self.push_required_with_budget(state, MAX_GESTURES, MAX_BYTES)
    }
    fn push_required_with_budget(
        &mut self,
        state: HistoryEntry,
        count: usize,
        bytes: usize,
    ) -> bool {
        let mut candidate = self.clone();
        candidate.push_with_budget(state, count, bytes);
        if candidate.undo.is_empty() {
            return false;
        }
        *self = candidate;
        true
    }
    fn push_with_budget(&mut self, state: HistoryEntry, count: usize, bytes: usize) {
        self.redo.clear();
        self.undo.push(state);
        self.trim_with_budget(count, bytes);
    }
    pub fn trim(&mut self) {
        self.trim_with_budget(MAX_GESTURES, MAX_BYTES);
    }
    fn trim_with_budget(&mut self, count: usize, bytes: usize) {
        while self.undo.len() + self.redo.len() > count || self.estimated_bytes() > bytes {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
            self.undo.shrink_to_fit();
            self.redo.shrink_to_fit();
        }
    }
    // Counts allocated capacities and shared data once, plus generous per-allocation
    // overhead. The history stack itself is charged even if it has spare capacity.
    fn estimated_bytes(&self) -> usize {
        let mut docs = HashSet::new();
        let mut resources = HashSet::new();
        let mut projects = HashSet::new();
        let mut total = vec_cost(&self.undo) + vec_cost(&self.redo);
        for entry in self.undo.iter().chain(&self.redo) {
            total += vec_cost(&entry.selection);
            total += vec_cost(&entry.selector_mappings);
            for mapping in &entry.selector_mappings {
                total += xml(&mapping.from) + xml(&mapping.to);
            }
            for key in &entry.selection {
                total += xml(&key.xml_id);
            }
            let p = &entry.project;
            if !projects.insert(Arc::as_ptr(p) as usize) {
                continue;
            }
            total += 64
                + size_of::<ProjectData>()
                + string(&p.name)
                + string(&p.description)
                + vec_cost(&p.folders)
                + vec_cost(&p.documents)
                + vec_cost(&p.resources)
                + vec_cost(&p.links)
                + vec_cost(&p.dictionary_entries)
                + vec_cost(&p.dictionary_links)
                + vec_cost(&p.comments)
                + vec_cost(&p.checkpoints);
            for c in &p.comments {
                total += xml(&c.target.xml_id) + string(&c.author) + string(&c.text);
            }
            for checkpoint in &p.checkpoints {
                let c = &checkpoint.content;
                total += string(&checkpoint.name)
                    + string(&c.name)
                    + string(&c.description)
                    + vec_cost(&c.folders)
                    + vec_cost(&c.documents)
                    + vec_cost(&c.resources)
                    + vec_cost(&c.links)
                    + vec_cost(&c.dictionary_entries)
                    + vec_cost(&c.dictionary_links)
                    + vec_cost(&c.comments);
                for e in &c.dictionary_entries {
                    total += string(&e.name) + string(&e.description);
                }
                for l in &c.dictionary_links {
                    total += xml(&l.source.xml_id);
                    if let DictionaryTarget::Literal(s) = &l.target {
                        total += string(s);
                    }
                }
                for f in &c.folders {
                    total += xml(&f.id) + string(&f.name) + opt_xml(&f.parent);
                }
                for l in &c.links {
                    total += xml(&l.source.xml_id) + string(&l.label);
                }
                for comment in &c.comments {
                    total += xml(&comment.target.xml_id)
                        + string(&comment.author)
                        + string(&comment.text);
                }
                for r in &c.resources {
                    total += xml(&r.id) + string(&r.media_type);
                    let ptr = Arc::as_ptr(&r.bytes) as *const u8 as usize;
                    if resources.insert(ptr) {
                        total += 64 + r.bytes.len();
                    }
                }
                for d in &c.documents {
                    if docs.insert(Arc::as_ptr(d) as usize) {
                        total += document(d);
                    }
                }
            }
            for e in &p.dictionary_entries {
                total += string(&e.name) + string(&e.description);
            }
            for l in &p.dictionary_links {
                total += xml(&l.source.xml_id);
                if let DictionaryTarget::Literal(s) = &l.target {
                    total += string(s);
                }
            }
            for f in &p.folders {
                total += xml(&f.id) + string(&f.name) + opt_xml(&f.parent);
            }
            for l in &p.links {
                total += xml(&l.source.xml_id) + string(&l.label);
            }
            for r in &p.resources {
                total += xml(&r.id) + string(&r.media_type);
                let ptr = Arc::as_ptr(&r.bytes) as *const u8 as usize;
                if resources.insert(ptr) {
                    total += 64 + r.bytes.len();
                }
            }
            for d in &p.documents {
                if docs.insert(Arc::as_ptr(d) as usize) {
                    total += document(d);
                }
            }
        }
        total
    }
}
fn vec_cost<T>(v: &Vec<T>) -> usize {
    if v.capacity() == 0 {
        0
    } else {
        64 + v.capacity() * size_of::<T>()
    }
}
trait StringCost {
    fn string_cost(&self) -> usize;
}
impl StringCost for String {
    fn string_cost(&self) -> usize {
        if self.capacity() == 0 {
            0
        } else {
            64 + self.capacity()
        }
    }
}
impl StringCost for Option<String> {
    fn string_cost(&self) -> usize {
        self.as_ref().map_or(0, string)
    }
}
impl StringCost for crate::bpmn_metadata::DocumentationSet {
    fn string_cost(&self) -> usize {
        match self {
            Self::Legacy(value) => string(value),
            Self::Structured(_) => 0,
        }
    }
}
fn string(s: &impl StringCost) -> usize {
    s.string_cost()
}
trait IdCost {
    fn id_cost(&self) -> usize;
}
impl IdCost for XmlId {
    fn id_cost(&self) -> usize {
        64 + self.allocated_bytes()
    }
}
impl IdCost for ElementSelector {
    fn id_cost(&self) -> usize {
        match self {
            Self::Xml(id) => xml(id),
            Self::Native { .. } => 0,
        }
    }
}
impl IdCost for ElementIdentity {
    fn id_cost(&self) -> usize {
        self.xml_id().map_or(0, xml)
    }
}
fn xml(id: &impl IdCost) -> usize {
    id.id_cost()
}
fn opt_xml<T: IdCost>(id: &Option<T>) -> usize {
    id.as_ref().map_or(0, xml)
}
fn style(s: &Option<Style>) -> usize {
    s.as_ref().map_or(0, |s| {
        s.fill.as_ref().map_or(0, string)
            + s.stroke.as_ref().map_or(0, string)
            + s.font.as_ref().map_or(0, string)
    })
}
fn label(l: &Option<Label>) -> usize {
    l.as_ref().map_or(0, |l| style(&l.style))
}
fn flow(f: &Flow) -> usize {
    xml(&f.id)
        + xml(&f.source)
        + xml(&f.target)
        + string(&f.name)
        + string(&f.documentation)
        + f.condition
            .as_ref()
            .and_then(|value| value.plain_text())
            .map_or(0, str::len)
}
fn document(d: &BpmnDocument) -> usize {
    // Charge the complete typed payload, including nested/new BPMN families. Streaming avoids
    // allocating another document-sized buffer. Four times JSON size is deliberately conservative
    // for strings, vector growth and allocation overhead; fixed capacities are also charged below.
    struct Meter(usize);
    impl std::io::Write for Meter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut meter = Meter(0);
    if serde_json::to_writer(&mut meter, d).is_err() {
        return MAX_BYTES.saturating_add(1);
    }
    let typed_payload = meter.0.saturating_mul(4);

    let mut n = 64
        + size_of::<BpmnDocument>()
        + string(&d.name)
        + opt_xml(&d.folder)
        + xml(&d.definitions_id)
        + vec_cost(&d.processes)
        + vec_cost(&d.diagrams);
    if let Some(p) = &d.provenance {
        n += xml(&p.source_resource_id) + string(&p.source_name);
    }
    match &d.capability {
        Capability::Editable => {}
        Capability::PreservedSource { source_resource_id } => n += xml(source_resource_id),
        Capability::RetainedExtensions {
            source_resource_id,
            extensions,
        } => {
            n += xml(source_resource_id) + vec_cost(extensions);
            for e in extensions {
                n += xml(&e.owner) + vec_cost(&e.namespaces) + string(&e.content);
                for (a, b) in &e.namespaces {
                    n += string(a) + string(b);
                }
            }
        }
    }
    for p in &d.processes {
        n += xml(&p.id)
            + string(&p.name)
            + string(&p.documentation)
            + vec_cost(&p.nodes)
            + vec_cost(&p.lanes)
            + vec_cost(&p.flows);
        for x in &p.nodes {
            n += xml(&x.id)
                + string(&x.name)
                + string(&x.documentation)
                + string(&x.owner)
                + string(&x.contact);
        }
        for l in &p.lanes {
            n += xml(&l.id) + string(&l.name) + opt_xml(&l.parent) + vec_cost(&l.members);
            for id in &l.members {
                n += xml(id);
            }
        }
        for f in &p.flows {
            n += flow(f);
        }
    }
    if let Some(c) = &d.collaboration {
        n += xml(&c.id) + vec_cost(&c.participants) + vec_cost(&c.flows);
        for p in &c.participants {
            n += xml(&p.id) + string(&p.name) + opt_xml(&p.process);
        }
        for f in &c.flows {
            n += flow(f);
        }
    }
    for dg in &d.diagrams {
        n += xml(&dg.id)
            + xml(&dg.plane_id)
            + xml(&dg.plane)
            + vec_cost(&dg.shapes)
            + vec_cost(&dg.edges);
        for s in &dg.shapes {
            n += xml(&s.id) + xml(&s.element) + label(&s.label) + style(&s.style);
        }
        for e in &dg.edges {
            n += xml(&e.id)
                + xml(&e.element)
                + vec_cost(&e.waypoints)
                + label(&e.label)
                + style(&e.style);
        }
    }
    n.saturating_add(typed_payload)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_typed_metadata_is_charged_without_copying_the_document() {
        let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
        document
            .event_resources
            .push(crate::bpmn_model::EventResource::Message {
                base: None,
                item_ref: None,
                id: "Message".parse().unwrap(),
                name: Some("x".repeat(100_000)),
            });
        let bytes = super::document(&document);
        assert!(bytes >= 400_000, "typed root metadata was omitted: {bytes}");
    }
    #[test]
    fn structured_documentation_is_charged_to_history_budget() {
        use crate::{
            bpmn_metadata::{Documentation, DocumentationSet},
            bpmn_xml::MixedXml,
        };
        let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
        document.processes.push(Process {
            id: "process".parse().unwrap(),
            name: Some("Process".into()),
            documentation: DocumentationSet::Structured(vec![Documentation {
                id: None,
                text_format: None,
                content: MixedXml::Text("x".repeat(100_000)),
            }]),
            base_extras: None,
            is_executable: Some(false),
            io_specification: None,
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            nodes: vec![],
            lanes: vec![],
            flows: vec![],
        });
        assert!(super::document(&document) >= 400_000);
    }
    #[test]
    fn required_undo_refusal_preserves_existing_history() {
        let project = Arc::new(ProjectData::new("existing".into()));
        let mut history = History::default();
        history.undo.push(HistoryEntry {
            project: project.clone(),
            selection: vec![],
            selector_mappings: vec![],
        });
        history.redo.push(HistoryEntry {
            project: project.clone(),
            selection: vec![],
            selector_mappings: vec![],
        });
        let before_undo = history.undo.len();
        let before_redo = history.redo.len();
        assert!(!History::fits_single_with_budget(
            HistoryEntry {
                project: project.clone(),
                selection: vec![],
                selector_mappings: vec![]
            },
            200,
            1
        ));
        assert!(!history.push_required_with_budget(
            HistoryEntry {
                project,
                selection: vec![],
                selector_mappings: vec![]
            },
            200,
            1
        ));
        assert_eq!(history.undo.len(), before_undo);
        assert_eq!(history.redo.len(), before_redo);
    }
    #[test]
    fn budget_evicts_oldest_and_counts_long_owned_labels() {
        let mut first = ProjectData::new("one".into());
        first.folders.push(Folder {
            id: "folder".parse().unwrap(),
            name: "x".repeat(16_384),
            parent: None,
        });
        let mut second = first.clone();
        second.name = "two".into();
        let first = Arc::new(first);
        let second = Arc::new(second);
        let mut h = History::default();
        h.undo.push(HistoryEntry {
            project: first.clone(),
            selection: vec![],
            selector_mappings: vec![],
        });
        let one = h.estimated_bytes();
        assert!(one >= 16_384);
        h.push_with_budget(
            HistoryEntry {
                project: second.clone(),
                selection: vec![],
                selector_mappings: vec![],
            },
            200,
            one + 1024,
        );
        assert_eq!(h.undo.len(), 1);
        assert!(Arc::ptr_eq(&h.undo[0].project, &second));
    }
    #[test]
    fn large_current_moved_to_redo_is_trimmed() {
        let small = Arc::new(ProjectData::new("small".into()));
        let large = Arc::new(ProjectData::new("x".repeat(16_384)));
        let mut h = History::default();
        h.undo.push(HistoryEntry {
            project: small,
            selection: vec![],
            selector_mappings: vec![],
        });
        let budget = h.estimated_bytes() + 2048;
        h.undo.pop();
        h.redo.push(HistoryEntry {
            project: large,
            selection: vec![],
            selector_mappings: vec![],
        });
        h.trim_with_budget(200, budget);
        assert!(h.redo.is_empty());
        assert!(h.estimated_bytes() <= budget);
    }
    #[test]
    fn shared_document_and_resource_are_charged_once() {
        let mut p = ProjectData::new("p".into());
        p.documents.push(Arc::new(BpmnDocument::new(
            "d".repeat(1000),
            "defs".parse().unwrap(),
        )));
        p.resources.push(Resource::new(
            "r".parse().unwrap(),
            "binary".into(),
            Arc::from(vec![0; 4096]),
        ));
        let p = Arc::new(p);
        let mut h = History::default();
        h.undo.push(HistoryEntry {
            project: p.clone(),
            selection: vec![],
            selector_mappings: vec![],
        });
        let one = h.estimated_bytes();
        let mut q = (*p).clone();
        q.name = "q".into();
        h.redo.push(HistoryEntry {
            project: Arc::new(q),
            selection: vec![],
            selector_mappings: vec![],
        });
        let two = h.estimated_bytes();
        assert!(
            two - one < 4096,
            "shared data charged again: {one} -> {two}"
        );
    }
}
