//! Runs `BpmnDocument::check_walker_agreement` (the core walker cross-checks) on every
//! fixture under `tests/fixtures/bpmn` that `prepare_import` accepts as editable.
use processcraft_io::interchange::prepare_import;
use std::path::{Path, PathBuf};

fn bpmn_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            bpmn_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "bpmn") {
            out.push(path);
        }
    }
}

#[test]
fn walkers_agree_on_every_editable_bpmn_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bpmn");
    let mut files = Vec::new();
    bpmn_files(&root, &mut files);
    files.sort();
    let mut checked = 0;
    let mut failures = Vec::new();
    for file in &files {
        let bytes = std::fs::read(file).unwrap();
        let Ok(prepared) = prepare_import(&bytes, "fixture") else {
            continue;
        };
        if !prepared.report.editable {
            continue;
        }
        checked += 1;
        if let Err(e) = prepared.document.check_walker_agreement() {
            failures.push(format!("{}: {e}", file.display()));
        }
    }
    eprintln!(
        "walker agreement checked {checked} of {} fixtures",
        files.len()
    );
    assert!(checked > 20, "only {checked} editable fixtures");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Lane `partitionElement` ids are ordinary identities: an XML-ID edit reaches them and the
/// new id round-trips through BPMN export.
#[test]
fn lane_partition_element_id_is_editable_and_round_trips() {
    use processcraft_core::{
        commands::{Envelope, Mutation, Request, Session},
        model::ProjectData,
    };
    use std::sync::Arc;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/bpmn/edit-model-io/lane-partitions.bpmn");
    let prepared = prepare_import(&std::fs::read(path).unwrap(), "fixture").unwrap();
    let document_id = prepared.document.id;
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(prepared.document));
    project.resources = prepared.resources;
    let mut session = Session::new(project).unwrap();
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session
        .apply(
            envelope,
            Request::Batch {
                mutations: vec![Mutation::SetElementXmlId {
                    document_id,
                    element_id: "Part".parse().unwrap(),
                    xml_id: Some("PartRenamed".parse().unwrap()),
                }],
            },
        )
        .unwrap();
    let project = session.snapshot().project.clone();
    let xml = String::from_utf8(
        processcraft_io::interchange::export_bpmn(&project, document_id, false)
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    assert!(xml.contains("id=\"PartRenamed\""), "{xml}");
    assert!(!xml.contains("id=\"Part\""));
    let again = prepare_import(xml.as_bytes(), "again").unwrap();
    again.document.check_walker_agreement().unwrap();
}
