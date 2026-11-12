//! Release-mode timing probes for large projects. Run with
//! `cargo test --release -p processcraft-core --test perf -- --ignored --nocapture`.
use processcraft_core::{
    commands::{Envelope, Mutation, Request, Session},
    ids::DocumentId,
    model::{ProjectData, Resource},
};
use serde_json::json;
use std::{sync::Arc, time::Instant};

const DOC: &str = "11111111-1111-4111-8111-111111111111";

fn apply(s: &mut Session, request: Request) {
    let envelope = Envelope {
        request_id: "perf".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request).unwrap();
}

fn chain(tasks: usize) -> Session {
    let mut mutations = vec![
        json!({"type":"createDocument","document":{"id":DOC,"name":"Chain","folder":null,
            "definitions_id":"defs","processes":[],"collaboration":null,"diagrams":[],
            "provenance":null,"capability":"Editable"}}),
        json!({"type":"createProcess","document_id":DOC,"process":{"id":"p","name":"P",
            "documentation":"","nodes":[],"lanes":[],"flows":[]}}),
    ];
    let ids: Vec<String> = std::iter::once("start".to_owned())
        .chain((0..tasks).map(|i| format!("t{i}")))
        .chain(std::iter::once("end".to_owned()))
        .collect();
    let mut shapes = vec![];
    let mut edges = vec![];
    for (i, id) in ids.iter().enumerate() {
        let kind = match i {
            0 => "StartEvent",
            _ if i == ids.len() - 1 => "EndEvent",
            _ => "Task",
        };
        mutations.push(
            json!({"type":"createElement","document_id":DOC,"process_id":"p",
            "element":{"id":id,"name":id,"documentation":"","kind":kind,"owner":"","contact":""}}),
        );
        shapes.push(json!({"id":format!("s_{id}"),"element":id,
            "bounds":{"x":(i * 160) as f64,"y":100.0,"width":100.0,"height":80.0},
            "label":null,"style":null}));
    }
    for (i, pair) in ids.windows(2).enumerate() {
        mutations.push(
            json!({"type":"createFlow","document_id":DOC,"process_id":"p",
            "flow":{"id":format!("f{i}"),"kind":"Sequence","source":pair[0],"target":pair[1],
                "name":"","documentation":"","condition":null}}),
        );
        edges.push(json!({"id":format!("e{i}"),"element":format!("f{i}"),
            "waypoints":[{"x":(i * 160 + 100) as f64,"y":140.0},{"x":((i + 1) * 160) as f64,"y":140.0}],
            "label":null,"style":null}));
    }
    // The document element limit admits DI for 2000 but not 4000 tasks.
    if tasks <= 2000 {
        mutations.push(json!({"type":"createDiagram","document_id":DOC,"diagram":{
        "id":"d","plane_id":"plane","plane":"p","shapes":shapes,"edges":edges}}));
    }
    for id in ids.iter().skip(1).take(tasks) {
        mutations.push(json!({"type":"setDictionaryLinks",
            "source":{"document_id":DOC,"xml_id":id},"kind":"System",
            "targets":[{"Literal":"ERP"}]}));
    }
    let mut session = Session::new(ProjectData::new("Perf".into())).unwrap();
    for chunk in mutations.chunks(1000) {
        let request: Request =
            serde_json::from_value(json!({"type":"batch","mutations":chunk})).unwrap();
        apply(&mut session, request);
    }
    session
}

fn rename(name: &str) -> Request {
    serde_json::from_value(json!({"type":"batch","mutations":[{"type":"rename",
        "document_id":DOC,"element_id":"t1","name":name}]}))
    .unwrap()
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "release timing probe"]
fn large_chain_timings() {
    for tasks in [2000, 4000] {
        let started = Instant::now();
        let mut session = chain(tasks);
        let build = ms(started);
        // Median of five renames; the first follows the build batches.
        let mut samples: Vec<f64> = (0..5)
            .map(|i| {
                let started = Instant::now();
                apply(&mut session, rename(&format!("renamed {i}")));
                ms(started)
            })
            .collect();
        samples.sort_by(f64::total_cmp);
        let rename_ms = samples[2];
        let started = Instant::now();
        apply(&mut session, Request::Undo);
        let undo_ms = ms(started);
        let project = session.snapshot().project.clone();
        let started = Instant::now();
        let diagnostics = project.diagnostics();
        let diagnostics_ms = ms(started);
        let started = Instant::now();
        let rows = processcraft_core::queries::activity_table(
            &project,
            DOC.parse::<DocumentId>().unwrap(),
            &"p".parse().unwrap(),
        )
        .unwrap();
        let table_ms = ms(started);
        assert_eq!(rows.len(), tasks);
        println!(
            "{tasks} tasks: build {build:.0} ms, rename {rename_ms:.1} ms, undo {undo_ms:.1} ms, diagnostics {diagnostics_ms:.1} ms ({} items), activity table {table_ms:.1} ms",
            diagnostics.len()
        );
    }
}

#[test]
#[ignore = "release timing probe"]
fn checkpoints_and_resource_commit_timing() {
    let mut session = chain(200);
    let bytes: Arc<[u8]> = vec![7u8; 30 * 1024 * 1024].into();
    apply(
        &mut session,
        Request::Batch {
            mutations: vec![Mutation::AddResource {
                resource: Resource::new("blob".parse().unwrap(), "image/png".into(), bytes),
            }],
        },
    );
    for i in 0..20 {
        apply(
            &mut session,
            serde_json::from_value(
                json!({"type":"batch","mutations":[{"type":"createCheckpoint",
                "id":format!("00000000-0000-4000-8000-{i:012}"),"name":format!("c{i}"),
                "created_unix_ms":i}]}),
            )
            .unwrap(),
        );
    }
    let started = Instant::now();
    apply(&mut session, rename("renamed"));
    println!(
        "20 checkpoints + 30 MiB resource: rename {:.1} ms",
        ms(started)
    );
}

#[test]
#[ignore = "release timing probe"]
fn identity_breakdown() {
    let session = chain(2000);
    let project = session.snapshot().project.clone();
    let started = Instant::now();
    project.validate().unwrap();
    let validate = ms(started);
    let started = Instant::now();
    let json = serde_json::to_value(&*project).unwrap();
    let to_value = ms(started);
    let started = Instant::now();
    let bytes = serde_json::to_vec(&json).unwrap();
    let to_vec = ms(started);
    let started = Instant::now();
    project.content_identity().unwrap();
    println!(
        "validate {validate:.1} to_value {to_value:.1} to_vec {to_vec:.1} ({} bytes) identity {:.1}",
        bytes.len(),
        ms(started)
    );
}
