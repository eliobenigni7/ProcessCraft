use processcraft_core::{
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementKey, XmlId},
    model::*,
};
use std::sync::Arc;
fn id(s: &str) -> XmlId {
    s.parse().unwrap()
}
fn env(s: &Session) -> Envelope {
    Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: None,
    }
}
fn apply(
    s: &mut Session,
    r: Request,
) -> Result<processcraft_core::commands::ChangeSet, CommandError> {
    s.apply(env(s), r)
}
fn batch(m: Vec<Mutation>) -> Request {
    Request::Batch { mutations: m }
}
fn project() -> Session {
    Session::new(ProjectData::new("Project".into())).unwrap()
}
fn fixture(s: &mut Session) {
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(s, r).unwrap();
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn rename(name: &str) -> Mutation {
    Mutation::RenameProject { name: name.into() }
}
#[test]
fn failed_batch_preserves_state_and_history() {
    let mut s = project();
    fixture(&mut s);
    apply(&mut s, Request::Undo).unwrap();
    apply(&mut s, Request::Redo).unwrap();
    s.set_selection(vec![ElementKey {
        document_id: docid(),
        xml_id: id("review"),
    }]);
    let before = s.snapshot();
    let bytes = serde_json::to_vec(&before.project).unwrap();
    let invalid = Mutation::CreateFlow {
        document_id: docid(),
        process_id: Some(id("procurement")),
        flow: Flow {
            association_direction: None,
            id: id("bad"),
            kind: FlowKind::Sequence,
            source: id("review"),
            target: id("missing"),
            name: String::new(),
            documentation: String::new(),
            condition: None,
        },
    };
    assert!(apply(&mut s, batch(vec![rename("changed"), invalid])).is_err());
    let after = s.snapshot();
    assert_eq!(bytes, serde_json::to_vec(&after.project).unwrap());
    assert_eq!(before.content_id, after.content_id);
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.undo_count, after.undo_count);
    assert_eq!(before.redo_count, after.redo_count);
    assert_eq!(before.selection, after.selection);
}
#[test]
fn generation_conflict_changes_nothing() {
    let mut s = project();
    let before = s.snapshot();
    let mut e = env(&s);
    e.expected_generation = Some(9);
    assert_eq!(
        s.apply(e, batch(vec![rename("changed")])).unwrap_err(),
        CommandError::GenerationConflict
    );
    assert_eq!(s.snapshot().content_id, before.content_id);
    assert_eq!(s.snapshot().generation, 0);
}
#[test]
fn one_batch_is_one_undo() {
    let mut s = project();
    let initial = s.snapshot().content_id;
    fixture(&mut s);
    assert_eq!(s.snapshot().undo_count, 1);
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, initial);
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().project.documents.len(), 1);
}
#[test]
fn undo_to_saved_content_is_clean() {
    let mut s = project();
    fixture(&mut s);
    let saved = s.snapshot().content_id;
    s.mark_saved(saved);
    apply(&mut s, batch(vec![rename("changed")])).unwrap();
    assert!(s.snapshot().dirty);
    apply(&mut s, Request::Undo).unwrap();
    assert!(!s.snapshot().dirty);
}
#[test]
fn history_budget_drops_only_undo() {
    let mut s = project();
    for i in 0..205 {
        apply(&mut s, batch(vec![rename(&format!("p{i}"))])).unwrap();
    }
    assert_eq!(s.snapshot().undo_count, 200);
    assert_eq!(s.snapshot().project.name, "p204");
    for _ in 0..200 {
        apply(&mut s, Request::Undo).unwrap();
    }
    assert_eq!(s.snapshot().project.name, "p4");
    assert_eq!(s.snapshot().redo_count, 200);
}
#[test]
fn noop_and_stale_save_ack() {
    let mut s = project();
    let saved = s.snapshot().content_id;
    assert!(
        !apply(&mut s, batch(vec![rename("Project")]))
            .unwrap()
            .changed
    );
    assert_eq!(s.snapshot().generation, 0);
    apply(&mut s, batch(vec![rename("later")])).unwrap();
    s.mark_saved(saved);
    assert!(s.snapshot().dirty);
    assert_eq!(s.snapshot().undo_count, 1);
}
#[test]
fn copy_on_write_shares_unchanged_document() {
    let mut s = project();
    fixture(&mut s);
    let d = BpmnDocument::new("second".into(), id("defs2"));
    apply(
        &mut s,
        batch(vec![Mutation::CreateDocument { document: d }]),
    )
    .unwrap();
    let before = s.snapshot();
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: docid(),
            element_id: id("review"),
            name: "New".into(),
        }]),
    )
    .unwrap();
    let after = s.snapshot();
    assert!(!Arc::ptr_eq(
        &before.project.documents[0],
        &after.project.documents[0]
    ));
    assert!(Arc::ptr_eq(
        &before.project.documents[1],
        &after.project.documents[1]
    ));
}
#[test]
fn capability_restrictions_and_unknown_fields() {
    let mut p = ProjectData::new("p".into());
    let r = Resource::new(id("source"), "text/xml".into(), Arc::from(&b"xml"[..]));
    p.resources.push(r);
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    let did = d.id;
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::DocumentProperties {
                document_id: did,
                name: "x".into(),
                folder: None
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert!(
        serde_json::from_str::<Request>(r#"{"type":"batch","mutations":[],"extra":1}"#).is_err()
    );
}
#[test]
fn retained_extensions_allow_labels_but_reject_structural_changes() {
    let mut p = ProjectData::new("p".into());
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    let mut d = BpmnDocument::new("d".into(), id("defs"));
    d.capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
    d.processes.push(Process {
        lane_set_id: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("p"),
        name: "before".into(),
        documentation: String::new(),
        nodes: vec![],
        lanes: vec![],
        flows: vec![],
    });
    let did = d.id;
    p.documents.push(Arc::new(d));
    let mut s = Session::new(p).unwrap();
    apply(
        &mut s,
        batch(vec![Mutation::Rename {
            document_id: did,
            element_id: id("p"),
            name: "after".into(),
        }]),
    )
    .unwrap();
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::Delete {
                document_id: did,
                element_id: id("p")
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
}
#[test]
fn deletion_rejects_dangling_connections_atomically() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot();
    assert!(
        apply(
            &mut s,
            batch(vec![Mutation::Delete {
                document_id: docid(),
                element_id: id("review")
            }])
        )
        .is_err()
    );
    assert_eq!(s.snapshot().content_id, before.content_id);
    assert_eq!(s.snapshot().undo_count, before.undo_count);
}
#[test]
fn batch_cap_and_unknown_mutation_fields_are_enforced() {
    let mut s = project();
    assert!(apply(&mut s, batch((0..1001).map(|_| rename("x")).collect())).is_err());
    assert!(
        serde_json::from_str::<Request>(
            r#"{"type":"batch","mutations":[{"type":"renameProject","name":"x","extra":1}]}"#
        )
        .is_err()
    );
}
#[test]
fn protected_documents_cannot_be_deleted_or_moved_between_folders() {
    let mut p = ProjectData::new("p".into());
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    p.folders.push(Folder {
        id: id("folder"),
        name: "folder".into(),
        parent: None,
    });
    let mut retained = BpmnDocument::new("retained".into(), id("defs"));
    retained.capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
    let retained_id = retained.id;
    p.documents.push(Arc::new(retained));
    let mut preserved = BpmnDocument::new("preserved".into(), id("defs2"));
    preserved.capability = Capability::PreservedSource {
        source_resource_id: id("source"),
    };
    let preserved_id = preserved.id;
    p.documents.push(Arc::new(preserved));
    let mut s = Session::new(p).unwrap();
    let before = s.snapshot().content_id;
    for document_id in [retained_id, preserved_id] {
        assert_eq!(
            apply(
                &mut s,
                batch(vec![Mutation::DeleteDocument { document_id }])
            )
            .unwrap_err(),
            CommandError::CapabilityDenied
        );
    }
    assert_eq!(
        apply(
            &mut s,
            batch(vec![Mutation::DocumentProperties {
                document_id: retained_id,
                name: "renamed".into(),
                folder: Some(id("folder"))
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert_eq!(s.snapshot().content_id, before);
    apply(
        &mut s,
        batch(vec![Mutation::DocumentProperties {
            document_id: retained_id,
            name: "renamed".into(),
            folder: None,
        }]),
    )
    .unwrap();
}
#[test]
fn unsaved_state_remains_dirty_until_save_acknowledged() {
    let mut s = project();
    s.mark_unsaved();
    assert!(s.snapshot().dirty);
    apply(&mut s, batch(vec![rename("change")])).unwrap();
    apply(&mut s, Request::Undo).unwrap();
    assert!(s.snapshot().dirty);
    s.mark_saved(s.snapshot().content_id);
    assert!(!s.snapshot().dirty);
}
#[test]
fn undo_redo_restore_selection_context() {
    let mut s = project();
    fixture(&mut s);
    let key = ElementKey {
        document_id: docid(),
        xml_id: id("review"),
    };
    s.set_selection(vec![key.clone()]);
    apply(
        &mut s,
        batch(vec![Mutation::DeleteDocument {
            document_id: docid(),
        }]),
    )
    .unwrap();
    assert!(s.snapshot().selection.is_empty());
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().selection, vec![key]);
    apply(&mut s, Request::Redo).unwrap();
    assert!(s.snapshot().selection.is_empty());
}
#[test]
fn flow_condition_changes_identity_and_protected_document_rejects_it() {
    let mut s = project();
    fixture(&mut s);
    let before = s.snapshot().content_id;
    apply(
        &mut s,
        batch(vec![Mutation::SetFlowCondition {
            document_id: docid(),
            flow_id: id("flow_approved"),
            condition: Some("amount > 10".into()),
        }]),
    )
    .unwrap();
    assert_ne!(s.snapshot().content_id, before);
    let mut p = (*s.snapshot().project).clone();
    p.resources.push(Resource::new(
        id("source"),
        "text/xml".into(),
        Arc::from(&b"xml"[..]),
    ));
    Arc::make_mut(&mut p.documents[0]).capability = Capability::RetainedExtensions {
        source_resource_id: id("source"),
        extensions: vec![],
    };
    let mut protected = Session::new(p).unwrap();
    let old = protected.snapshot().content_id;
    assert_eq!(
        apply(
            &mut protected,
            batch(vec![Mutation::SetFlowCondition {
                document_id: docid(),
                flow_id: id("flow_approved"),
                condition: None
            }])
        )
        .unwrap_err(),
        CommandError::CapabilityDenied
    );
    assert_eq!(protected.snapshot().content_id, old);
}
#[test]
fn descriptors_match_protocol_tags() {
    let descriptors = processcraft_core::commands::command_descriptors();
    let expected = [
        "setActivityProperties",
        "createCategory",
        "updateCategory",
        "deleteCategory",
        "createGroup",
        "updateGroup",
        "deleteGroup",
        "setAssociationDirection",
        "setLaneSetId",
        "setChildLaneSetId",
        "setLabelBounds",
        "setNodeIoSpecification",
        "createDataStore",
        "updateDataStore",
        "deleteDataStore",
        "createDataObject",
        "updateDataObject",
        "deleteDataObject",
        "createDataAssociation",
        "updateDataAssociation",
        "deleteDataAssociation",
        "setDiagramPlane",
        "createChoreography",
        "updateChoreography",
        "deleteChoreography",
        "createChoreographyActivity",
        "updateChoreographyActivity",
        "deleteChoreographyActivity",
        "createConversationNode",
        "updateConversationNode",
        "deleteConversationNode",
        "createConversationLink",
        "updateConversationLink",
        "deleteConversationLink",
        "applyLayout",
        "createComment",
        "updateComment",
        "deleteComment",
        "createCheckpoint",
        "renameCheckpoint",
        "deleteCheckpoint",
        "setReferenceCheckpoint",
        "restoreCheckpoint",
        "setNodeLoop",
        "setNodeDetails",
        "addResource",
        "spliceActivity",
        "createFolder",
        "renameFolder",
        "moveFolder",
        "deleteFolder",
        "createDictionaryEntry",
        "updateDictionaryEntry",
        "deleteDictionaryEntry",
        "setDictionaryLinks",
        "batch",
        "undo",
        "redo",
        "createDocument",
        "createProcess",
        "createCollaboration",
        "createParticipant",
        "createLane",
        "createElement",
        "createFlow",
        "createDiagram",
        "createShape",
        "createEdge",
        "deleteDocument",
        "projectProperties",
        "renameProject",
        "documentProperties",
        "rename",
        "documentation",
        "delete",
        "moveShape",
        "reroute",
        "assignLane",
        "setFlowCondition",
        "setNodeOwnerContact",
    ];
    assert_eq!(
        descriptors.iter().map(|d| d.serde_tag).collect::<Vec<_>>(),
        expected
    );
    for d in descriptors {
        assert_eq!(d.id, d.serde_tag);
        assert!(!d.label.is_empty());
    }
    let request: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    let serialized = serde_json::to_value(request).unwrap();
    for m in serialized["mutations"].as_array().unwrap() {
        assert!(
            descriptors
                .iter()
                .any(|d| Some(d.serde_tag) == m["type"].as_str())
        );
    }
    let mutation = Mutation::SetFlowCondition {
        document_id: docid(),
        flow_id: id("flow_approved"),
        condition: None,
    };
    assert_eq!(
        serde_json::to_value(mutation).unwrap()["type"],
        "setFlowCondition"
    );
}
