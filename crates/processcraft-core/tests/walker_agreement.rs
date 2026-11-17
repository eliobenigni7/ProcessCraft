//! Cross-checks the hand-written whole-document traversals so that adding an element kind to one
//! of them without the others fails here instead of drifting silently. The assertions live in
//! `BpmnDocument::check_walker_agreement`; `processcraft-io/tests/walker_agreement.rs` runs the
//! same check on every importable BPMN fixture.
//!
//! Must agree: `element_identities` == `document_ids`. Must be subsets of `element_identities`:
//! `document_selectors` references, `activity_read`/`collaboration_read` ids, and the
//! documentation ids seen by `metadata_visit`. Read-side qnames must be a subset of
//! `document_qnames`. Legitimately differ: references vs identities (many-to-one) and the
//! qname superset; `metadata_visit` skips definition extensions and expression documentation.
use processcraft_core::{
    commands::{Envelope, Request, Session},
    model::{BpmnDocument, ProjectData},
};

const FIXTURES: &[(&str, &str)] = &[
    (
        "procurement",
        include_str!("../../../tests/fixtures/commands/procurement.json"),
    ),
    (
        "attributes",
        include_str!("../../../tests/fixtures/commands/attributes.json"),
    ),
    (
        "library-dictionary",
        include_str!("../../../tests/fixtures/commands/library-dictionary.json"),
    ),
    (
        "raci",
        include_str!("../../../tests/fixtures/commands/raci.json"),
    ),
];

fn documents() -> Vec<(String, BpmnDocument)> {
    let mut out = Vec::new();
    for (name, json) in FIXTURES {
        let mut session = Session::new(ProjectData::new((*name).into())).unwrap();
        // library-dictionary.json has no createDocument; it extends the procurement project.
        let batches = if *name == "library-dictionary" {
            vec![FIXTURES[0].1, json]
        } else {
            vec![*json]
        };
        for batch in batches {
            let request: Request = serde_json::from_str(batch).unwrap();
            let envelope = Envelope {
                request_id: "t".into(),
                session_id: session.id(),
                expected_generation: None,
            };
            session.apply(envelope, request).unwrap();
        }
        for (index, document) in session.snapshot().project.documents.iter().enumerate() {
            out.push((format!("{name}#{index}"), (**document).clone()));
        }
    }
    assert!(!out.is_empty());
    out
}

#[test]
fn command_fixture_walkers_agree() {
    for (name, d) in documents() {
        d.check_walker_agreement()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}
