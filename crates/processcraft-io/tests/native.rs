use processcraft_core::{
    ids::XmlId,
    model::{ProjectData, Resource},
};
use processcraft_io::native::{load_bytes, write_bytes};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn project() -> ProjectData {
    let mut p = ProjectData::new("Native test".into());
    p.resources.push(Resource::new(
        "asset".parse::<XmlId>().unwrap(),
        "text/plain".into(),
        Arc::from(&b"hello world"[..]),
    ));
    p
}
fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let n = f.name().to_owned();
            let mut b = vec![];
            f.read_to_end(&mut b).unwrap();
            (n, b)
        })
        .collect()
}
fn archive(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for (n, b) in entries {
        w.start_file(n, SimpleFileOptions::default()).unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}
fn edit_json(
    entries: &mut [(String, Vec<u8>)],
    name: &str,
    f: impl FnOnce(&mut serde_json::Value),
) {
    let b = &mut entries.iter_mut().find(|(n, _)| n == name).unwrap().1;
    let mut v = serde_json::from_slice(b).unwrap();
    f(&mut v);
    *b = serde_json::to_vec(&v).unwrap();
}
#[test]
fn native_round_trip_preserves_content() {
    let p = project();
    let bytes = write_bytes(&p).unwrap();
    let loaded = load_bytes(&bytes).unwrap();
    assert_eq!(
        p.content_identity().unwrap(),
        loaded.project.content_identity().unwrap()
    );
    assert!(!loaded.requires_save_as_or_backup);
    let list = entries(&bytes);
    let json = &list.iter().find(|(n, _)| n == "project.json").unwrap().1;
    assert!(!String::from_utf8_lossy(json).contains("bytes"));
}
#[test]
fn unknown_required_capability_refuses_write() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!(["future"])
    });
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn resource_digest_mismatch_fails() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.iter_mut()
        .find(|(n, _)| n.starts_with("resources/"))
        .unwrap()
        .1
        .push(1);
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn writer_cannot_exceed_reader_limits() {
    let mut p = project();
    p.resources[0] = Resource::new(
        "asset".parse().unwrap(),
        "x".into(),
        Arc::from(vec![0; 32 * 1024 * 1024 + 1]),
    );
    assert!(write_bytes(&p).is_err());
}
#[test]
fn unsafe_archive_is_rejected() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.push(("../escape".into(), vec![]));
    assert!(load_bytes(&archive(&e)).is_err());
    e.pop();
    e.push(("mimetypX".into(), vec![]));
    let mut bytes = archive(&e);
    for i in 0..bytes.len() - 8 {
        if &bytes[i..i + 8] == b"mimetypX" {
            bytes[i + 7] = b'e';
        }
    }
    assert!(load_bytes(&bytes).is_err());
}
#[test]
fn malformed_and_duplicate_json_fail() {
    let original = entries(&write_bytes(&project()).unwrap());
    for replacement in [b"{\"id\":1,\"id\":2}".to_vec(), vec![0xff], b"{".to_vec()] {
        let mut e = original.clone();
        e.iter_mut().find(|(n, _)| n == "project.json").unwrap().1 = replacement;
        assert!(load_bytes(&archive(&e)).is_err());
    }
    let mut e = original.clone();
    e.retain(|(n, _)| n != "project.json");
    assert!(load_bytes(&archive(&e)).is_err());
    let bytes = archive(&original);
    assert!(load_bytes(&bytes[..bytes.len() / 2]).is_err());
}
#[test]
fn synthetic_v0_migrates_without_mutating_input() {
    let p = project();
    let mut e = entries(&write_bytes(&p).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!([]);
        v["schema"] = 0.into();
        v["min_reader"] = 0.into();
        v["content_identity"] = serde_json::to_value(p.legacy_content_identity().unwrap()).unwrap();
    });
    edit_json(&mut e, "project.json", |v| {
        v.as_object_mut().unwrap().remove("description");
        v.as_object_mut().unwrap().remove("dictionary_entries");
        v.as_object_mut().unwrap().remove("dictionary_links");
    });
    let bytes = archive(&e);
    let before = bytes.clone();
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert_eq!(bytes, before);
    assert_eq!(
        loaded.project.content_identity().unwrap(),
        p.content_identity().unwrap()
    );
}
#[test]
fn expanded_entry_limit_is_enforced() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    e.iter_mut()
        .find(|(n, _)| n.starts_with("resources/"))
        .unwrap()
        .1 = vec![0; 32 * 1024 * 1024 + 1];
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn unknown_project_fields_and_future_schema_are_rejected() {
    let original = entries(&write_bytes(&project()).unwrap());
    let mut e = original.clone();
    edit_json(&mut e, "project.json", |v| {
        v["resources"][0]["future"] = true.into();
    });
    assert!(load_bytes(&archive(&e)).is_err());
    let mut e = original;
    edit_json(&mut e, "manifest.json", |v| {
        v["schema"] = 4.into();
    });
    assert!(load_bytes(&archive(&e)).is_err());
}
#[test]
fn shared_resources_reuse_hydrated_bytes() {
    let mut p = project();
    let mut alias = p.resources[0].clone();
    alias.id = "alias".parse().unwrap();
    p.resources.push(alias);
    let before = p.content_identity().unwrap();
    let loaded = load_bytes(&write_bytes(&p).unwrap()).unwrap().project;
    assert_eq!(before, loaded.content_identity().unwrap());
    assert!(Arc::ptr_eq(
        &loaded.resources[0].bytes,
        &loaded.resources[1].bytes
    ));
}
#[test]
fn logical_resource_size_budget_is_checked_before_hydration() {
    let mut e = entries(&write_bytes(&project()).unwrap());
    edit_json(&mut e, "project.json", |v| {
        let original = v["resources"][0].clone();
        let resources = v["resources"].as_array_mut().unwrap();
        for i in 0..9 {
            let mut alias = original.clone();
            alias["id"] = format!("alias{i}").into();
            alias["size"] = (32_u64 * 1024 * 1024).into();
            resources.push(alias);
        }
    });
    assert!(
        load_bytes(&archive(&e))
            .err()
            .unwrap()
            .to_string()
            .contains("logical resources exceed limit")
    );
}

#[test]
fn schema_one_is_verified_then_migrated_without_touching_source() {
    let p = project();
    let mut e = entries(&write_bytes(&p).unwrap());
    edit_json(&mut e, "manifest.json", |v| {
        v["required_capabilities"] = serde_json::json!([]);
        v["schema"] = 1.into();
        v["min_reader"] = 1.into();
        v["content_identity"] = serde_json::to_value(p.legacy_content_identity().unwrap()).unwrap();
    });
    edit_json(&mut e, "project.json", |v| {
        v.as_object_mut().unwrap().remove("dictionary_entries");
        v.as_object_mut().unwrap().remove("dictionary_links");
    });
    let bytes = archive(&e);
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert!(loaded.project.dictionary_entries.is_empty());
    edit_json(&mut e, "project.json", |v| v["name"] = "tampered".into());
    assert!(load_bytes(&archive(&e)).is_err());
    assert!(
        !load_bytes(&write_bytes(&loaded.project).unwrap())
            .unwrap()
            .requires_save_as_or_backup
    );
}

#[test]
fn schema_two_round_trips_dictionary_entries() {
    use processcraft_core::{
        ids::{DictionaryEntryId, ElementKey},
        model::{
            BpmnDocument, DictionaryEntry, DictionaryKind, DictionaryLink, DictionaryTarget, Lane,
            Process,
        },
    };
    let mut p = project();
    p.dictionary_entries.push(DictionaryEntry {
        id: DictionaryEntryId::new(),
        kind: DictionaryKind::Role,
        name: "Manager".into(),
        description: "Approves".into(),
    });
    let mut doc = BpmnDocument::new("document".into(), "defs".parse().unwrap());
    doc.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: "process".parse().unwrap(),
        name: String::new(),
        documentation: String::new(),
        nodes: vec![],
        lanes: vec![Lane {
            child_lane_set_id: None,
            id: "lane".parse().unwrap(),
            name: String::new(),
            parent: None,
            members: vec![],
        }],
        flows: vec![],
    });
    p.dictionary_links.push(DictionaryLink {
        source: ElementKey {
            document_id: doc.id,
            xml_id: "lane".parse().unwrap(),
        },
        kind: DictionaryKind::Role,
        target: DictionaryTarget::Entry(p.dictionary_entries[0].id),
    });
    p.documents.push(Arc::new(doc));
    let data = write_bytes(&p).unwrap();
    let loaded = load_bytes(&data).unwrap();
    assert_eq!(
        loaded.project.dictionary_links[0].target,
        DictionaryTarget::Entry(p.dictionary_entries[0].id)
    );
    assert!(!loaded.requires_save_as_or_backup);
    assert_eq!(
        loaded.project.content_identity().unwrap(),
        p.content_identity().unwrap()
    );
    assert_eq!(loaded.project.dictionary_entries[0].name, "Manager");
}

#[test]
fn checkpoint_payload_round_trips_with_shared_resources_and_detects_tampering() {
    use processcraft_core::{
        ids::CheckpointId,
        model::{Checkpoint, FrozenProject},
    };
    let mut p = project();
    let id = CheckpointId::new();
    p.checkpoints.push(Checkpoint {
        id,
        name: "Reference".into(),
        created_unix_ms: 42,
        content: FrozenProject::capture(&p),
    });
    p.reference_checkpoint = Some(id);
    p.name = "Current".into();
    let data = write_bytes(&p).unwrap();
    let mut archive_entries = entries(&data);
    let path = format!("checkpoints/{id}.json");
    assert!(archive_entries.iter().any(|(n, _)| n == &path));
    assert_eq!(
        archive_entries
            .iter()
            .filter(|(n, _)| n.starts_with("resources/"))
            .count(),
        1
    );
    let loaded = load_bytes(&data).unwrap().project;
    assert_eq!(loaded.checkpoints[0].content.name, "Native test");
    assert_eq!(loaded.reference_checkpoint, Some(id));
    assert!(Arc::ptr_eq(
        &loaded.resources[0].bytes,
        &loaded.checkpoints[0].content.resources[0].bytes
    ));
    edit_json(&mut archive_entries, &path, |v| {
        v["name"] = "tampered".into()
    });
    assert!(load_bytes(&archive(&archive_entries)).is_err());
}

#[test]
fn native_comments_round_trip_including_orphan() {
    use processcraft_core::{
        ids::{CommentId, DocumentId, ElementKey},
        model::Comment,
        queries::comment_is_orphan,
    };
    let mut p = project();
    p.comments.push(Comment {
        id: CommentId::new(),
        target: ElementKey {
            document_id: DocumentId::new(),
            xml_id: "deleted".parse().unwrap(),
        },
        author: "Analyst".into(),
        text: "Keep this context".into(),
    });
    let loaded = load_bytes(&write_bytes(&p).unwrap()).unwrap().project;
    assert_eq!(loaded.comments[0].text, "Keep this context");
    assert!(comment_is_orphan(&loaded, &loaded.comments[0]));
}

#[test]
fn schema_two_digest_verified_before_migration_to_three() {
    let p = project();
    let mut old = entries(&write_bytes(&p).unwrap());
    edit_json(&mut old, "manifest.json", |v| {
        v["schema"] = 2.into();
        v["min_reader"] = 2.into();
        v["required_capabilities"] = serde_json::json!([]);
    });
    let bytes = archive(&old);
    let loaded = load_bytes(&bytes).unwrap();
    assert!(loaded.requires_save_as_or_backup);
    assert_eq!(
        loaded.source_content_identity,
        p.content_identity().unwrap()
    );
    edit_json(&mut old, "project.json", |v| v["name"] = "tampered".into());
    assert!(load_bytes(&archive(&old)).is_err());
}

#[test]
fn twenty_checkpoints_share_large_resource_and_round_trip_with_bounded_archive() {
    use processcraft_core::{
        ids::CheckpointId,
        model::{Checkpoint, FrozenProject},
    };
    let mut p = ProjectData::new("Bounded history".into());
    p.resources.push(Resource::new(
        "shared".parse().unwrap(),
        "application/octet-stream".into(),
        Arc::from(vec![42u8; 1024 * 1024]),
    ));
    for index in 0..20 {
        p.checkpoints.push(Checkpoint {
            id: CheckpointId::new(),
            name: format!("Checkpoint {index}"),
            created_unix_ms: index,
            content: FrozenProject::capture(&p),
        });
    }
    let bytes = write_bytes(&p).unwrap();
    let archived = entries(&bytes);
    assert_eq!(
        archived
            .iter()
            .filter(|(name, _)| name.starts_with("checkpoints/"))
            .count(),
        20
    );
    assert_eq!(
        archived
            .iter()
            .filter(|(name, _)| name.starts_with("resources/"))
            .count(),
        1
    );
    let loaded = load_bytes(&bytes).unwrap().project;
    assert_eq!(loaded.checkpoints.len(), 20);
    for checkpoint in &loaded.checkpoints {
        assert!(Arc::ptr_eq(
            &loaded.resources[0].bytes,
            &checkpoint.content.resources[0].bytes
        ));
    }
    let extra = Checkpoint {
        id: CheckpointId::new(),
        name: "Over limit".into(),
        created_unix_ms: 21,
        content: FrozenProject::capture(&p),
    };
    p.checkpoints.push(extra);
    assert!(write_bytes(&p).is_err());
}
