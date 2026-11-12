use processcraft_core::{
    bpmn_data::*,
    bpmn_model::{Expression, ExpressionData, ExpressionKind, NodeDetails, QNameRef},
    commands::{Envelope, Mutation, Request, Session},
    ids::{ElementKey, ElementSelector},
    model::*,
};
use std::sync::Arc;
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn apply(
    s: &mut Session,
    m: Vec<Mutation>,
) -> Result<(), processcraft_core::commands::CommandError> {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Batch { mutations: m },
    )
    .map(|_| ())
}
fn fixture() -> (Session, ElementKey) {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            details: None,
            loop_characteristics: None,
            io_specification: None,
            activity: None,
            id: id("activity"),
            name: Some("A".into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            owner: String::new(),
            contact: String::new(),
        }],
        lanes: vec![Lane {
            base: None,
            child_lane_set_id: None,
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            id: id("lane"),
            name: Some("L".into()),
            parent: None,
            members: vec![id("activity")],
        }],
        flows: vec![],
    });
    let key = ElementKey {
        document_id: d.id,
        xml_id: id("activity"),
    };
    p.documents.push(Arc::new(d));
    (Session::new(p).unwrap(), key)
}

fn undo(s: &mut Session) {
    s.apply(
        Envelope {
            request_id: "undo".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Undo,
    )
    .unwrap();
}
fn q(local: &str) -> QNameRef {
    QNameRef {
        namespace: "urn:m3".into(),
        local: id(local),
    }
}
fn aware(local: &str) -> Option<ItemAwareMetadata> {
    Some(ItemAwareMetadata {
        item_subject_ref: Some(q(local)),
        data_state: None,
    })
}
fn doc(s: &Session) -> Arc<BpmnDocument> {
    s.snapshot().project.documents[0].clone()
}
fn sels(v: &[&str]) -> Vec<ElementSelector> {
    v.iter().map(|s| id(s)).collect()
}
fn node(name: &str, kind: NodeKind, details: Option<NodeDetails>) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        id: id(name),
        name: Some(name.into()),
        documentation: String::new().into(),
        kind,
        details,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        owner: String::new(),
        contact: String::new(),
    }
}
fn store(name: &str, capacity: &str, unlimited: bool, item: &str) -> DataStore {
    DataStore {
        base: None,
        id: Some(id("store")),
        name: Some(name.into()),
        capacity: Some(capacity.into()),
        is_unlimited: Some(unlimited),
        item_aware: aware(item),
    }
}
fn object(collection: bool, item: &str) -> DataObject {
    DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id("obj")),
        name: Some("Order".into()),
        is_collection: Some(collection),
        item_aware: aware(item),
    }
}
fn port(pid: &str, name: &str, collection: bool, item: &str) -> DataPort {
    DataPort {
        base: None,
        id: Some(id(pid)),
        name: Some(name.into()),
        item_aware: aware(item),
        is_collection: Some(collection),
    }
}
fn io(first: bool) -> IoSpecification {
    let (n, c, i) = if first {
        ("A", false, "t1")
    } else {
        ("B", true, "t2")
    };
    let pick = |a: &[&str], b: &[&str]| sels(if first { a } else { b });
    IoSpecification {
        base: None,
        id: None,
        inputs: vec![
            port("in1", n, c, i),
            port("in2", "Spare", false, "t1"),
            port("in3", "Live", false, "t1"),
        ],
        outputs: vec![
            port("out1", n, c, i),
            port("out2", "Spare", false, "t1"),
            port("out3", "Live", false, "t1"),
        ],
        input_sets: vec![IoSet {
            id: Some(id("iset")),
            name: Some(format!("in {n}")),
            data_refs: pick(&["in1"], &["in2"]),
            optional_refs: pick(&["in2"], &["in1"]),
            while_executing_refs: pick(&["in3"], &[]),
            counterpart_refs: pick(&["oset"], &[]),
            ..Default::default()
        }],
        output_sets: vec![IoSet {
            id: Some(id("oset")),
            name: Some(format!("out {n}")),
            data_refs: pick(&["out1"], &["out2"]),
            optional_refs: pick(&["out2"], &["out1"]),
            while_executing_refs: pick(&["out3"], &[]),
            counterpart_refs: pick(&["iset"], &[]),
            ..Default::default()
        }],
    }
}
fn association(
    aid: &str,
    kind: DataAssociationKind,
    sources: &[&str],
    target: &str,
) -> DataAssociation {
    DataAssociation {
        base: None,
        id: Some(id(aid)),
        owner: id("activity"),
        kind,
        sources: sels(sources),
        target: id(target),
        transformation: None,
        assignments: vec![],
    }
}
fn store_ref(name: &str) -> Option<NodeDetails> {
    Some(NodeDetails::DataStoreReference {
        data_store_ref: Some(DataStoreRef::Qualified(q(name))),
    })
}
fn object_ref() -> Option<NodeDetails> {
    Some(NodeDetails::DataObjectReference {
        data_object_ref: Some(id("obj")),
    })
}
/// Session with a store, a data object, two object references, a store reference and I/O ports on `activity`.
fn setup() -> (Session, ElementKey) {
    let (mut s, key) = fixture();
    let d = key.document_id;
    let create = |n| Mutation::CreateElement {
        document_id: d,
        process_id: id("process"),
        element: n,
    };
    apply(
        &mut s,
        vec![
            Mutation::CreateDataStore {
                document_id: d,
                data_store: store("S", "1", false, "t1"),
            },
            Mutation::CreateDataObject {
                document_id: d,
                scope_id: id("process"),
                data_object: object(false, "t1"),
            },
            create(node("ref1", NodeKind::DataObjectReference, object_ref())),
            create(node("ref2", NodeKind::DataObjectReference, object_ref())),
            create(node(
                "sref",
                NodeKind::DataStoreReference,
                store_ref("store"),
            )),
            Mutation::SetNodeIoSpecification {
                document_id: d,
                element_id: id("activity"),
                io_specification: Some(io(true)),
            },
        ],
    )
    .unwrap();
    (s, key)
}
fn formal(text: &str) -> Expression {
    Expression::Structured(ExpressionData {
        foreign_attributes: vec![],
        id: None,
        kind: ExpressionKind::Formal {
            explicit_type: false,
            language: None,
            evaluates_to_type_ref: None,
        },
        text: text.into(),
        namespaces: vec![(
            "bpmn".into(),
            "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
        )],
    })
}
fn find(d: &BpmnDocument, n: &str) -> Node {
    d.processes[0]
        .nodes
        .iter()
        .find(|x| x.id == id::<ElementSelector>(n))
        .unwrap()
        .clone()
}
fn activity_io(s: &Session) -> IoSpecification {
    find(&doc(s), "activity").io_specification.unwrap()
}
fn update_assoc(s: &mut Session, d: processcraft_core::ids::DocumentId, a: DataAssociation) {
    apply(
        s,
        vec![Mutation::UpdateDataAssociation {
            document_id: d,
            scope_id: id("process"),
            association: a,
        }],
    )
    .unwrap();
}

#[test]
fn data_store_attributes_edit_and_undo() {
    let (mut s, key) = setup();
    let d = key.document_id;
    assert_eq!(doc(&s).data_stores[0], store("S", "1", false, "t1"));
    apply(
        &mut s,
        vec![Mutation::UpdateDataStore {
            document_id: d,
            data_store: store("S2", "99", true, "t2"),
        }],
    )
    .unwrap();
    assert_eq!(doc(&s).data_stores[0], store("S2", "99", true, "t2"));
    undo(&mut s);
    assert_eq!(doc(&s).data_stores[0], store("S", "1", false, "t1"));
    apply(
        &mut s,
        vec![Mutation::SetDataStores {
            document_id: d,
            data_stores: vec![store("S3", "7", true, "t2")],
        }],
    )
    .unwrap();
    let got = doc(&s).data_stores[0].clone();
    assert_eq!(
        (
            got.name.as_deref(),
            got.capacity.as_deref(),
            got.is_unlimited
        ),
        (Some("S3"), Some("7"), Some(true))
    );
    assert_eq!(got.item_aware.unwrap().item_subject_ref, Some(q("t2")));
    undo(&mut s);
    assert_eq!(doc(&s).data_stores[0], store("S", "1", false, "t1"));
}

#[test]
fn data_object_collection_and_item_edit_and_undo() {
    let (mut s, key) = setup();
    let d = key.document_id;
    apply(
        &mut s,
        vec![Mutation::UpdateDataObject {
            document_id: d,
            scope_id: id("process"),
            data_object: object(true, "t2"),
        }],
    )
    .unwrap();
    let o = doc(&s).processes[0].data_objects[0].clone();
    assert_eq!(o.is_collection, Some(true));
    assert_eq!(o.item_aware.unwrap().item_subject_ref, Some(q("t2")));
    undo(&mut s);
    assert_eq!(doc(&s).processes[0].data_objects[0], object(false, "t1"));
    apply(
        &mut s,
        vec![Mutation::SetScopeDataObjects {
            document_id: d,
            scope_id: id("process"),
            data_objects: vec![object(true, "t2")],
        }],
    )
    .unwrap();
    assert_eq!(
        doc(&s).processes[0].data_objects[0].is_collection,
        Some(true)
    );
    undo(&mut s);
    assert_eq!(
        doc(&s).processes[0].data_objects[0].is_collection,
        Some(false)
    );
}

#[test]
fn data_store_reference_target_and_item_edit_and_undo() {
    let (mut s, key) = setup();
    let d = key.document_id;
    assert_eq!(find(&doc(&s), "sref").details, store_ref("store"));
    apply(
        &mut s,
        vec![
            Mutation::SetNodeDetails {
                document_id: d,
                element_id: id("sref"),
                details: store_ref("other"),
            },
            Mutation::SetNodeItemAware {
                document_id: d,
                element_id: id("sref"),
                item_aware: aware("t2"),
            },
        ],
    )
    .unwrap();
    let r = find(&doc(&s), "sref");
    assert_eq!(r.details, store_ref("other"));
    assert_eq!(r.item_aware.unwrap().item_subject_ref, Some(q("t2")));
    undo(&mut s);
    let r = find(&doc(&s), "sref");
    assert_eq!(r.details, store_ref("store"));
    assert_eq!(r.item_aware, None);
}

#[test]
fn io_ports_and_sets_edit_and_undo() {
    let (mut s, key) = setup();
    assert_eq!(activity_io(&s), io(true));
    let before = activity_io(&s);
    assert_eq!(before.inputs[0].name.as_deref(), Some("A"));
    assert_eq!(before.outputs[0].is_collection, Some(false));
    assert_eq!(before.input_sets[0].while_executing_refs, sels(&["in3"]));
    assert_eq!(before.input_sets[0].counterpart_refs, sels(&["oset"]));
    assert_eq!(before.output_sets[0].while_executing_refs, sels(&["out3"]));
    assert_eq!(before.output_sets[0].counterpart_refs, sels(&["iset"]));
    apply(
        &mut s,
        vec![Mutation::SetNodeIoSpecification {
            document_id: key.document_id,
            element_id: id("activity"),
            io_specification: Some(io(false)),
        }],
    )
    .unwrap();
    let after = activity_io(&s);
    assert_eq!(
        (
            after.inputs[0].name.as_deref(),
            after.inputs[0].is_collection
        ),
        (Some("B"), Some(true))
    );
    assert_eq!(after.outputs[0].name.as_deref(), Some("B"));
    assert_eq!(after.outputs[0].is_collection, Some(true));
    assert_eq!(
        after.outputs[0]
            .item_aware
            .as_ref()
            .unwrap()
            .item_subject_ref,
        Some(q("t2"))
    );
    assert_eq!(after.input_sets[0].name.as_deref(), Some("in B"));
    assert_eq!(after.input_sets[0].data_refs, sels(&["in2"]));
    assert_eq!(after.input_sets[0].optional_refs, sels(&["in1"]));
    assert!(after.input_sets[0].while_executing_refs.is_empty());
    assert!(after.input_sets[0].counterpart_refs.is_empty());
    assert_eq!(after.output_sets[0].name.as_deref(), Some("out B"));
    assert_eq!(after.output_sets[0].data_refs, sels(&["out2"]));
    assert_eq!(after.output_sets[0].optional_refs, sels(&["out1"]));
    undo(&mut s);
    assert_eq!(activity_io(&s), before);
}

#[test]
fn data_association_sources_target_transformation_edit_and_undo() {
    let (mut s, key) = setup();
    let d = key.document_id;
    let input = association("a_in", DataAssociationKind::Input, &["ref1"], "in1");
    let output = association("a_out", DataAssociationKind::Output, &["out1"], "sref");
    apply(
        &mut s,
        vec![
            Mutation::CreateDataAssociation {
                document_id: d,
                scope_id: id("process"),
                association: input.clone(),
            },
            Mutation::CreateDataAssociation {
                document_id: d,
                scope_id: id("process"),
                association: output.clone(),
            },
        ],
    )
    .unwrap();
    let list = doc(&s).processes[0].data_associations.clone();
    assert_eq!(list, vec![input.clone(), output.clone()]);
    assert_eq!(list[1].kind, DataAssociationKind::Output);
    assert_eq!(list[1].sources, sels(&["out1"]));
    assert_eq!(list[1].target, id::<ElementSelector>("sref"));
    // Output association edit: retarget and add a transformation, then undo.
    let mut edited = output.clone();
    edited.target = id("ref2");
    edited.transformation = Some(formal("x + 1"));
    update_assoc(&mut s, d, edited.clone());
    assert_eq!(doc(&s).processes[0].data_associations[1], edited);
    undo(&mut s);
    assert_eq!(doc(&s).processes[0].data_associations[1], output);
    // Input association transformation set/undo.
    let mut t = input.clone();
    t.transformation = Some(formal("f(x)"));
    update_assoc(&mut s, d, t);
    assert_eq!(
        doc(&s).processes[0].data_associations[0].transformation,
        Some(formal("f(x)"))
    );
    undo(&mut s);
    assert_eq!(
        doc(&s).processes[0].data_associations[0].transformation,
        None
    );
}

#[test]
fn data_association_source_count_diagnostic_is_negative_tested() {
    let (mut s, key) = setup();
    let d = key.document_id;
    let count = |s: &Session| {
        s.snapshot()
            .project
            .diagnostics()
            .iter()
            .filter(|x| x.code == "dataAssociationSourceCount")
            .count()
    };
    apply(
        &mut s,
        vec![Mutation::CreateDataAssociation {
            document_id: d,
            scope_id: id("process"),
            association: association("a_in", DataAssociationKind::Input, &["ref1"], "in1"),
        }],
    )
    .unwrap();
    assert_eq!(count(&s), 0);
    let two = association("a_in", DataAssociationKind::Input, &["ref1", "ref2"], "in1");
    update_assoc(&mut s, d, two.clone());
    assert_eq!(count(&s), 1);
    let mut merged = two;
    merged.transformation = Some(formal("merge"));
    update_assoc(&mut s, d, merged);
    assert_eq!(count(&s), 0);
}
