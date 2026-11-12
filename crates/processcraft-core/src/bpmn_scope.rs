//! Read-only inventory of the pinned OMG BPMN 2.0.2 XSD declarations.
//! A declaration's presence does not imply implementation or conformance.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct Coverage {
    pub model: String,
    pub edit: String,
    pub render: String,
    pub import: String,
    pub export: String,
    pub validate: String,
}

#[derive(Debug, Deserialize)]
pub struct Declaration {
    pub schema: String,
    pub namespace: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub coverage: Coverage,
}

#[derive(Debug, Deserialize)]
pub struct Inventory {
    pub version: String,
    pub declarations: Vec<Declaration>,
}

/// The catalogue describes normative schema declarations, not supported features.
pub fn inventory() -> &'static Inventory {
    static INVENTORY: OnceLock<Inventory> = OnceLock::new();
    INVENTORY.get_or_init(|| {
        serde_json::from_str(include_str!("../../../docs/research/bpmn-2-coverage.json"))
            .expect("embedded BPMN inventory must be valid")
    })
}

/// Return every declaration matching a namespace, declaration kind and name.
/// Local declarations can occur in multiple contexts; callers must inspect `path`.
pub fn find<'a>(
    namespace: &'a str,
    kind: &'a str,
    name: &'a str,
) -> impl Iterator<Item = &'static Declaration> + 'a {
    inventory()
        .declarations
        .iter()
        .filter(move |item| item.namespace == namespace && item.kind == kind && item.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_process_collaboration_choreography_and_di() {
        let model = "http://www.omg.org/spec/BPMN/20100524/MODEL";
        for name in [
            "process",
            "collaboration",
            "choreography",
            "conversation",
            "task",
            "sequenceFlow",
        ] {
            assert!(find(model, "element", name).next().is_some(), "{name}");
        }
        assert!(
            find(
                "http://www.omg.org/spec/BPMN/20100524/DI",
                "element",
                "BPMNDiagram"
            )
            .next()
            .is_some()
        );
        assert!(inventory().declarations.iter().all(|d| matches!(
            d.coverage.model.as_str(),
            "unverified" | "partial" | "verified"
        )));
    }
}
