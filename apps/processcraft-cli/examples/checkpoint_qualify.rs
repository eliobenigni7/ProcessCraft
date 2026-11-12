//! Synthetic checkpoint/native size qualification. `--large` uses up to ~256 MiB of resource data.
use processcraft_core::{
    commands::{Envelope, Mutation, Request, Session},
    ids::CheckpointId,
    model::{ProjectData, Resource},
};
use processcraft_io::native::{load_bytes, write_bytes};
use std::{fs, sync::Arc, time::Instant};
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().expect("valid fixture id")
}
fn apply(session: &mut Session, mutation: Mutation) -> Result<(), String> {
    let env = Envelope {
        request_id: uuid::Uuid::new_v4().to_string(),
        session_id: session.id(),
        expected_generation: Some(session.snapshot().generation),
    };
    session
        .apply(
            env,
            Request::Batch {
                mutations: vec![mutation],
            },
        )
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = std::env::args().nth(1).filter(|arg| arg != "--large") {
        let bytes = fs::read(path)?;
        let loaded = load_bytes(&bytes)?;
        let doc = &loaded.project.documents[0];
        let diagram = doc.diagrams[0].id.selector();
        let result = processcraft_core::layout::propose_layout(
            doc,
            &diagram,
            "qualification",
            0,
            &std::sync::atomic::AtomicBool::new(false),
        );
        match result {
            Ok(proposal) => println!("{}", serde_json::to_string_pretty(&proposal)?),
            Err(error) => println!("LAYOUT_REFUSED {}", error),
        }
        return Ok(());
    }
    let large = std::env::args().any(|a| a == "--large");
    let out = "target/m3-qualification/checkpoint";
    fs::create_dir_all(out)?;
    let start = Instant::now();
    let mut report = serde_json::Map::new();
    let max = 32 * 1024 * 1024;
    let mut exact = ProjectData::new("resource ceiling".into());
    exact.resources.push(Resource::new(
        id("max_resource"),
        "application/octet-stream".into(),
        Arc::from(vec![7u8; max]),
    ));
    exact.validate()?;
    report.insert("resource_exact_32m_valid".into(), true.into());
    let mut over = exact.clone();
    over.resources[0] = Resource::new(
        id("max_resource"),
        "application/octet-stream".into(),
        Arc::from(vec![7u8; max + 1]),
    );
    let err = over.validate().expect_err("32 MiB + 1 must fail");
    report.insert("resource_over_32m_error".into(), err.into());
    drop(over);
    let mut session = Session::new(exact).map_err(|e| format!("{e:?}"))?;
    let checkpoint = CheckpointId::new();
    apply(
        &mut session,
        Mutation::CreateCheckpoint {
            id: checkpoint,
            name: "Frozen first".into(),
            created_unix_ms: 1,
        },
    )?;
    apply(
        &mut session,
        Mutation::SetReferenceCheckpoint {
            id: Some(checkpoint),
        },
    )?;
    let snapshot = session.snapshot();
    let identity = snapshot.content_id;
    let t = Instant::now();
    let archive = write_bytes(&snapshot.project)?;
    let write_ms = t.elapsed().as_secs_f64() * 1000.0;
    fs::write(format!("{out}/dedup.processcraft"), &archive)?;
    let t = Instant::now();
    let reopened = load_bytes(&archive)?;
    let read_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(reopened.project.reference_checkpoint, Some(checkpoint));
    assert_eq!(
        reopened.project.checkpoints[0].content.resources[0].content_hash,
        reopened.project.resources[0].content_hash
    );
    assert_eq!(reopened.project.content_identity()?, identity);
    report.insert("roundtrip".into(),serde_json::json!({"archive_bytes":archive.len(),"resource_bytes":max,"write_ms":write_ms,"read_ms":read_ms,"frozen_reference_preserved":true,"undo_count":snapshot.undo_count}));
    drop(reopened);
    drop(archive);
    drop(snapshot);
    drop(session);
    let mut capped =
        Session::new(ProjectData::new("checkpoint count".into())).map_err(|e| format!("{e:?}"))?;
    for i in 0..20 {
        apply(
            &mut capped,
            Mutation::CreateCheckpoint {
                id: CheckpointId::new(),
                name: format!("Checkpoint {i}"),
                created_unix_ms: i,
            },
        )?;
    }
    let before = capped.snapshot();
    let error = apply(
        &mut capped,
        Mutation::CreateCheckpoint {
            id: CheckpointId::new(),
            name: "Over limit".into(),
            created_unix_ms: 21,
        },
    )
    .expect_err("21st checkpoint must fail");
    assert_eq!(capped.snapshot().content_id, before.content_id);
    let count_archive = write_bytes(&before.project)?;
    let count_loaded = load_bytes(&count_archive)?;
    assert_eq!(count_loaded.project.checkpoints.len(), 20);
    report.insert("checkpoint_count".into(),serde_json::json!({"accepted":20,"over_limit_error":error,"reopened":count_loaded.project.checkpoints.len(),"undo_count":before.undo_count}));
    if large {
        let t = Instant::now();
        let mut near = ProjectData::new("near expanded limit".into());
        for i in 0..8 {
            let size = if i == 7 { max - 65536 } else { max };
            let bytes: Arc<[u8]> = Arc::from(vec![(i + 1) as u8; size]);
            near.resources.push(Resource::new(
                id(&format!("resource_{i}")),
                "application/octet-stream".into(),
                bytes,
            ));
        }
        let resource_bytes: usize = near.resources.iter().map(|r| r.bytes.len()).sum();
        let checkpoint = CheckpointId::new();
        let mut session = Session::new(near).map_err(|e| format!("{e:?}"))?;
        apply(
            &mut session,
            Mutation::CreateCheckpoint {
                id: checkpoint,
                name: "Near expanded".into(),
                created_unix_ms: 2,
            },
        )?;
        apply(
            &mut session,
            Mutation::SetReferenceCheckpoint {
                id: Some(checkpoint),
            },
        )?;
        apply(
            &mut session,
            Mutation::RenameProject {
                name: "changed after frozen checkpoint".into(),
            },
        )?;
        let prior_restore = session.snapshot();
        let restore_error = apply(&mut session, Mutation::RestoreCheckpoint { id: checkpoint })
            .expect_err("restore must refuse when undo cannot be retained");
        assert!(restore_error.contains("history budget"));
        assert_eq!(session.snapshot().content_id, prior_restore.content_id);
        assert_eq!(session.snapshot().generation, prior_restore.generation);
        let snap = session.snapshot();
        let undo = snap.undo_count;
        let write_start = Instant::now();
        let bytes = write_bytes(&snap.project)?;
        let write_ms = write_start.elapsed().as_secs_f64() * 1000.0;
        fs::write(format!("{out}/near-expanded.processcraft"), &bytes)?;
        let read_start = Instant::now();
        let loaded = load_bytes(&bytes)?;
        let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(loaded.project.reference_checkpoint, Some(checkpoint));
        assert_eq!(loaded.project.checkpoints[0].content.resources.len(), 8);
        assert_eq!(loaded.project.content_identity()?, snap.content_id);
        report.insert("near_expanded".into(),serde_json::json!({"unique_resource_bytes":resource_bytes,"gap_from_256m_bytes":65536,"archive_bytes":bytes.len(),"write_ms":write_ms,"read_ms":read_ms,"elapsed_ms":t.elapsed().as_secs_f64()*1000.0,"history_undo_count":undo,"restore_refused_error":restore_error,"restore_generation_unchanged":true,"frozen_reference_preserved":true}));
        drop(loaded);
        drop(bytes);
        drop(snap);
        drop(session);
        let mut beyond = ProjectData::new("expanded over".into());
        for i in 0..8 {
            beyond.resources.push(Resource::new(
                id(&format!("beyond_{i}")),
                "application/octet-stream".into(),
                Arc::from(vec![(i + 11) as u8; max]),
            ));
        }
        let err = write_bytes(&beyond)
            .expect_err("256 MiB blobs plus metadata must exceed expanded limit");
        report.insert(
            "expanded_256m_blobs_plus_metadata_error".into(),
            err.to_string().into(),
        );
    }
    report.insert(
        "total_ms".into(),
        serde_json::json!(start.elapsed().as_secs_f64() * 1000.0),
    );
    report.insert("large_mode".into(), large.into());
    fs::write(
        format!("{out}/measurements.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
