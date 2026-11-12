use processcraft_core::{
    commands::{Envelope, Mutation, Request, Session},
    geometry::hit_shape,
    ids::ElementKey,
    model::*,
    queries,
};
use processcraft_io::{Access, FileGrant, FileSession, ProjectSaveOutcome, native};
use processcraft_render::{Appearance, BpmnScene, TextEngine};
use serde_json::{Value, json};
use sha2::Digest;
use std::{error::Error, fs, hint::black_box, path::PathBuf, sync::Arc, time::Instant};
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn fixture(docs: usize, nodes: usize, active: usize, resource: usize) -> ProjectData {
    let mut p = ProjectData::new("Qualification".into());
    p.id = "10000000-0000-4000-8000-000000000001".parse().unwrap();
    if resource > 0 {
        let mut state = 0x123456789abcdefu64;
        let mut b = vec![0u8; resource];
        for chunk in b.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let v = state.to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        p.resources.push(Resource::new(
            id("benchmark_resource"),
            "application/octet-stream".into(),
            Arc::from(b),
        ));
    }
    for j in 0..docs {
        let mut d = BpmnDocument::new(format!("Document {j}"), id("definitions"));
        d.id = format!("20000000-0000-4000-8000-{j:012x}").parse().unwrap();
        let count = if docs > 1 && j == 0 {
            active
        } else if docs > 1 {
            (nodes - active) / (docs - 1) + usize::from(j - 1 < (nodes - active) % (docs - 1))
        } else {
            nodes
        };
        let mut ns = Vec::new();
        let mut shapes = Vec::new();
        for i in 0..count {
            let key = format!("node_{i}");
            ns.push(Node {
                flow_metadata: None,
                base_extras: None,
                item_aware: None,
                activity: None,
                io_specification: None,
                loop_characteristics: None,
                details: None,
                id: id(&key),
                name: Some(format!("Task {i}")),
                documentation: String::new().into(),
                kind: NodeKind::Task,
                owner: String::new(),
                contact: String::new(),
            });
            if j == 0 && i < active {
                shapes.push(Shape {
                    di_metadata: None,
                    is_message_visible: None,
                    participant_band_kind: None,
                    choreography_activity_shape: None,
                    label_visibility: None,
                    is_horizontal: None,
                    is_marker_visible: None,
                    is_expanded: None,
                    id: id(&format!("shape_{i}")),
                    element: id(&key),
                    bounds: Bounds {
                        x: (i % 25) as f64 * 80.,
                        y: (i / 25) as f64 * 50.,
                        width: 70.,
                        height: 40.,
                    },
                    label: None,
                    style: None,
                });
            }
        }
        d.processes.push(Process {
            activity_metadata: None,
            base_extras: None,
            is_executable: Some(false),
            io_specification: None,
            lane_set_id: None,
            lane_set_base: None,
            groups: vec![],
            data_objects: vec![],
            data_associations: vec![],
            id: id("process"),
            name: Some("Process".into()),
            documentation: String::new().into(),
            nodes: ns,
            lanes: vec![],
            flows: vec![],
        });
        d.diagrams.push(Diagram {
            plane_metadata: None,
            plane_order: None,
            documentation: None,
            resolution: None,
            label_styles: vec![],
            name: None,
            id: id("diagram"),
            plane_id: id("plane"),
            plane: id("process"),
            shapes,
            edges: vec![],
        });
        p.documents.push(Arc::new(d));
    }
    if docs == 20 {
        let kinds = [
            DictionaryKind::Role,
            DictionaryKind::System,
            DictionaryKind::Document,
        ];
        for (k, kind) in kinds.iter().enumerate() {
            for i in 0..10 {
                p.dictionary_entries.push(DictionaryEntry {
                    id: format!("30000000-0000-4000-8000-{:012x}", k * 10 + i + 1)
                        .parse()
                        .unwrap(),
                    kind: *kind,
                    name: format!("{:?} {i}", kind),
                    description: format!("Qualification {i}"),
                });
            }
        }
        for d in &mut p.documents {
            let d = Arc::make_mut(d);
            let process = &mut d.processes[0];
            for lane in 0..5 {
                let lane_id: processcraft_core::ids::ElementIdentity = id(&format!("lane_{lane}"));
                process.lanes.push(Lane {
                    base: None,
                    child_lane_set_id: None,
                    child_lane_set_base: None,
                    id: lane_id.clone(),
                    name: Some(format!("Lane {lane}")),
                    parent: None,
                    members: process
                        .nodes
                        .iter()
                        .skip(lane)
                        .step_by(5)
                        .map(|n| n.id.selector())
                        .collect(),
                });
                p.dictionary_links.push(DictionaryLink {
                    source: ElementKey {
                        document_id: d.id,
                        xml_id: lane_id.selector(),
                    },
                    kind: DictionaryKind::Role,
                    target: DictionaryTarget::Entry(p.dictionary_entries[lane].id),
                });
            }
            for (i, n) in process.nodes.iter().enumerate() {
                for (k, kind) in kinds.iter().enumerate().skip(1) {
                    p.dictionary_links.push(DictionaryLink {
                        source: ElementKey {
                            document_id: d.id,
                            xml_id: n.id.selector(),
                        },
                        kind: *kind,
                        target: DictionaryTarget::Entry(p.dictionary_entries[k * 10 + i % 10].id),
                    });
                }
                if i + 1 < process.nodes.len() {
                    process.flows.push(Flow {
                        message_ref: None,
                        flow_metadata: None,
                        base_extras: None,
                        association_direction: None,
                        id: id(&format!("flow_{i}")),
                        kind: FlowKind::Sequence,
                        source: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                            n.id.selector(),
                        )),
                        target: Some(processcraft_core::bpmn_activity::ModelReference::Local(
                            process.nodes[i + 1].id.selector(),
                        )),
                        name: Some(format!("Branch {i}")),
                        documentation: "Qualification flow".into(),
                        condition: Some(format!("amount > {i}").into()),
                    });
                }
            }
        }
    }
    p
}
fn measure(mut f: impl FnMut() -> Result<(), String>) -> Result<Value, String> {
    for _ in 0..5 {
        f()?;
    }
    let mut v = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        f()?;
        v.push(t.elapsed().as_secs_f64() * 1000.);
    }
    v.sort_by(f64::total_cmp);
    Ok(json!({"warmups":5,"samples":30,"p95_ms":v[28],"min_ms":v[0],"max_ms":v[29],"samples_ms":v}))
}
fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--out")) {
        return Err("usage: qualify --out DIRECTORY".into());
    }
    let out = PathBuf::from(args.next().ok_or("missing output")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    fs::create_dir_all(&out)?;
    let out = if out.is_absolute() {
        out
    } else {
        std::env::current_dir()?.join(out)
    };
    let small = fixture(1, 10, 10, 0);
    let medium = fixture(20, 5000, 500, 20 * 1024 * 1024);
    if std::env::var_os("M2_PROBE").is_some() {
        let mut without = medium.clone();
        without.resources.clear();
        let did = without.documents[0].id;
        let mut session = Session::new(without.clone()).map_err(|e| format!("{e:?}"))?;
        let mut seq = 0;
        let rename = measure(|| {
            seq += 1;
            session
                .apply(
                    Envelope {
                        request_id: "probe".into(),
                        session_id: session.id(),
                        expected_generation: None,
                    },
                    Request::Batch {
                        mutations: vec![Mutation::Rename {
                            document_id: did,
                            element_id: id("node_0"),
                            name: Some(format!("Renamed {seq}")),
                        }],
                    },
                )
                .map_err(|e| format!("{e:?}"))?;
            Ok(())
        })?;
        let hash = measure(|| {
            black_box(sha2::Sha256::digest(&medium.resources[0].bytes));
            Ok(())
        })?;
        let save = measure(|| {
            black_box(native::write_bytes(&without).map_err(|e| e.to_string())?);
            Ok(())
        })?;
        fs::write(
            out.join("resource-probe.json"),
            serde_json::to_vec_pretty(
                &json!({"rename_without_resource":rename,"sha256_resource_20_mib":hash,"native_save_without_resource":save,"note":"same M2 fixture, resource removed only; warm measurements"}),
            )?,
        )?;
        return Ok(());
    }
    let near = fixture(1, 2000, 2000, 0);
    for (name, p) in [
        ("small", &small),
        ("medium", &medium),
        ("near_limit", &near),
    ] {
        p.validate().map_err(|e| format!("{name}: {e}"))?;
        let b = native::write_bytes(p)?;
        native::load_bytes(&b)?;
        fs::write(out.join(format!("{name}.processcraft")), b)?;
    }
    let bytes = fs::read(out.join("medium.processcraft"))?;
    let mut results = serde_json::Map::new();
    let mut errors = Vec::<String>::new();
    macro_rules! record {
        ($name:expr,$body:expr) => {
            match measure($body) {
                Ok(v) => {
                    results.insert($name.into(), v);
                }
                Err(e) => errors.push(format!("{}: {}", $name, e)),
            }
        };
    }
    let mut session = Session::new(medium.clone()).map_err(|e| format!("{e:?}"))?;
    let did = medium.documents[0].id;
    let mut seq = 0;
    record!("rename", || {
        seq += 1;
        session
            .apply(
                Envelope {
                    request_id: "qualification".into(),
                    session_id: session.id(),
                    expected_generation: None,
                },
                Request::Batch {
                    mutations: vec![Mutation::Rename {
                        document_id: did,
                        element_id: id("node_0"),
                        name: Some(format!("Renamed {seq}")),
                    }],
                },
            )
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    });
    let rows = queries::activity_table(&medium, did, &id("process")).ok_or("table unavailable")?;
    if rows.len() != 500
        || rows[0].role.is_none()
        || rows[0].systems.len() != 1
        || rows[0].documents.len() != 1
        || rows[0].outgoing[0]
            .condition
            .as_ref()
            .and_then(|expression| expression.plain_text())
            != Some("amount > 0")
    {
        return Err("activity table content mismatch".into());
    }
    let table_digest = format!("{:x}", sha2::Sha256::digest(serde_json::to_vec(&rows)?));
    record!("activity_table", || {
        black_box(
            queries::activity_table(&medium, did, &id("process")).ok_or("table unavailable")?,
        );
        Ok(())
    });
    record!("library_search", || {
        black_box(queries::library_search(&medium, "Document"));
        Ok(())
    });
    record!("dictionary_usages", || {
        black_box(queries::dictionary_usages(
            &medium,
            medium.dictionary_entries[10].id,
        ));
        Ok(())
    });
    let shapes = &medium.documents[0].diagrams[0].shapes;
    record!("hit_test_500", || {
        for s in shapes {
            black_box(
                hit_shape(NodeKind::Task, &s.bounds, &Point { x: 35., y: 20. }, 3., 1.)
                    .map_err(str::to_string)?,
            );
        }
        Ok(())
    });
    record!("search_in_memory", || {
        let found: usize = medium
            .documents
            .iter()
            .flat_map(|d| d.processes.iter())
            .flat_map(|p| p.nodes.iter())
            .filter(|n| {
                n.name
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("task 2")
            })
            .count();
        black_box(found);
        Ok(())
    });
    let scene = BpmnScene::build(
        &medium.documents[0],
        &id("diagram"),
        Appearance::Light,
        &mut TextEngine::new(),
        "Noto Sans",
    )?;
    let w = scene.scene.width();
    let h = scene.scene.height();
    record!("full_scene_raster_100_percent", || {
        black_box(scene.scene.rasterize(1.).map_err(|e| e.to_string())?);
        Ok(())
    });
    record!("full_scene_raster_200_percent", || {
        black_box(scene.scene.rasterize(2.).map_err(|e| e.to_string())?);
        Ok(())
    });
    record!("native_open_memory", || {
        black_box(native::load_bytes(&bytes).map_err(|e| e.to_string())?);
        Ok(())
    });
    record!("native_save_memory", || {
        black_box(native::write_bytes(&medium).map_err(|e| e.to_string())?);
        Ok(())
    });
    let path = out.join("commit.processcraft");
    fs::write(&path, &bytes)?;
    let (mut file, _) = FileSession::open("qualification", FileGrant::new(&path, Access::Write)?)?;
    record!("file_session_save_including_serialization_and_disk", || {
        match file.save(&medium) {
            ProjectSaveOutcome::Committed { warning: None, .. } => Ok(()),
            other => Err(format!("{other:?}")),
        }
    });
    for _ in 0..165 {
        seq += 1;
        session
            .apply(
                Envelope {
                    request_id: "history".into(),
                    session_id: session.id(),
                    expected_generation: None,
                },
                Request::Batch {
                    mutations: vec![Mutation::Rename {
                        document_id: did,
                        element_id: id("node_0"),
                        name: Some(format!("Renamed {seq}")),
                    }],
                },
            )
            .map_err(|e| format!("{e:?}"))?;
    }
    let snap = session.snapshot();
    let mut over = fixture(1, 10001, 0, 0);
    let rejection = over.validate().err();
    over.documents.clear();
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let os = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let report = json!({"fixture":{"small":{"documents":1,"nodes":10,"shapes":10},"medium":{"documents":20,"nodes":5000,"shapes":500,"resource_bytes":20*1024*1024,"native_bytes":bytes.len()},"near_limit":{"documents":1,"nodes":2000,"shapes":2000,"note":"bounded workload, not a claim of near-100000 interactive capacity"}},"m2_fixture":{"dictionary_entries":medium.dictionary_entries.len(),"dictionary_links":medium.dictionary_links.len(),"lanes":medium.documents.iter().map(|d| d.processes[0].lanes.len()).sum::<usize>(),"flows":medium.documents.iter().map(|d| d.processes[0].flows.len()).sum::<usize>(),"table_rows":rows.len(),"table_sha256":table_digest,"usage_count":queries::dictionary_usages(&medium,medium.dictionary_entries[10].id).len()},"scene":{"width":w,"height":h,"labels":scene.scene.labels().len()},"measurements":results,"history":{"renames":200,"undo_count":snap.undo_count,"memory":"not measured"},"over_document_limit_rejection":rejection,"environment":{"os":os,"rustc":rustc,"arch":std::env::consts::ARCH,"memory_method":"not measured","disk":"output path; local SSD not independently verified"},"errors":errors,"method":"Instant wall time; warm memory operations; FileSession includes serialization and disk commit; resource SHA validation included; no cold-open measurement"});
    fs::write(
        out.join("measurements.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("qualification error: {e}");
        std::process::exit(1)
    }
}
