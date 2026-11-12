//! Round-2 validate evidence for data owners (BPMN 2.0.2 §10.4.1, Tables 10.51-10.63).
use processcraft_core::{
    bpmn_catalog::ItemDefinition,
    bpmn_data::*,
    bpmn_model::{NodeDetails, QNameRef},
    ids::ElementSelector,
    model::*,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn q(local: &str) -> QNameRef {
    QNameRef {
        namespace: GENERATED_TARGET_NAMESPACE.into(),
        local: id(local),
    }
}
fn aware(local: &str) -> Option<ItemAwareMetadata> {
    Some(ItemAwareMetadata {
        item_subject_ref: Some(q(local)),
        data_state: None,
    })
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
fn port(pid: &str, collection: bool, item: &str) -> DataPort {
    DataPort {
        base: None,
        id: Some(id(pid)),
        name: Some(pid.into()),
        item_aware: aware(item),
        is_collection: Some(collection),
    }
}
fn set(sid: &str, data: &[&str], optional: &[&str], counterpart: &[&str]) -> IoSet {
    IoSet {
        id: Some(id(sid)),
        data_refs: sels(data),
        optional_refs: sels(optional),
        counterpart_refs: sels(counterpart),
        ..Default::default()
    }
}
fn io(
    inputs: Vec<DataPort>,
    outputs: Vec<DataPort>,
    i: Vec<IoSet>,
    o: Vec<IoSet>,
) -> IoSpecification {
    IoSpecification {
        inputs,
        outputs,
        input_sets: i,
        output_sets: o,
        ..Default::default()
    }
}
fn assoc(aid: &str, owner: &str, sources: &[&str], target: &str) -> DataAssociation {
    DataAssociation {
        base: None,
        id: Some(id(aid)),
        owner: id(owner),
        kind: DataAssociationKind::Input,
        sources: sels(sources),
        target: id(target),
        transformation: None,
        assignments: vec![],
    }
}
fn project(edit: impl FnOnce(&mut BpmnDocument, &mut Process)) -> ProjectData {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    for (item, collection) in [("single", false), ("many", true)] {
        d.catalog.item_definitions.push(ItemDefinition {
            base: None,
            id: Some(id(item)),
            structure_ref: None,
            is_collection: Some(collection),
            item_kind: None,
        });
    }
    let mut process = Process {
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
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes: vec![node("activity", NodeKind::Task, None)],
        lanes: vec![],
        flows: vec![],
    };
    edit(&mut d, &mut process);
    d.processes.push(process);
    p.documents.push(Arc::new(d));
    p
}
fn codes(p: &ProjectData) -> Vec<(&'static str, Option<ElementSelector>)> {
    p.diagnostics()
        .into_iter()
        .map(|d| (d.code, d.element_id))
        .collect()
}
fn has(p: &ProjectData, code: &str, owner: &str) -> bool {
    codes(p)
        .iter()
        .any(|(c, o)| *c == code && o.as_ref() == Some(&id::<ElementSelector>(owner)))
}
fn count(p: &ProjectData, code: &str) -> usize {
    codes(p).iter().filter(|(c, _)| *c == code).count()
}
fn object(oid: &str, collection: bool, item: &str) -> DataObject {
    DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id(oid)),
        name: Some(oid.into()),
        is_collection: Some(collection),
        item_aware: aware(item),
    }
}
fn store(sid: &str, capacity: &str, item: &str) -> DataStore {
    DataStore {
        base: None,
        id: Some(id(sid)),
        name: Some(sid.into()),
        capacity: Some(capacity.into()),
        is_unlimited: Some(false),
        item_aware: aware(item),
    }
}

/// Tables 10.51/10.52: itemSubjectRef must resolve and isCollection must match the ItemDefinition.
#[test]
fn data_object_item_subject_and_collection_are_validated() {
    let ok = project(|_, p| {
        p.data_objects = vec![
            object("good", true, "many"),
            object("plain", false, "single"),
        ]
    });
    assert_eq!(count(&ok, "dataItemSubjectUnresolved"), 0);
    assert_eq!(count(&ok, "dataCollectionMismatch"), 0);
    let bad = project(|_, p| {
        p.data_objects = vec![
            object("mismatch", false, "many"),
            object("mismatch2", true, "single"),
            object("dangling", false, "missing"),
        ]
    });
    assert!(has(&bad, "dataCollectionMismatch", "mismatch"));
    assert!(has(&bad, "dataCollectionMismatch", "mismatch2"));
    assert!(has(&bad, "dataItemSubjectUnresolved", "dangling"));
    assert_eq!(count(&bad, "dataCollectionMismatch"), 2);
    let message = bad
        .diagnostics()
        .into_iter()
        .find(|d| d.code == "dataCollectionMismatch")
        .unwrap()
        .message;
    assert!(message.contains("Table 10.52"), "{message}");
}

/// Tables 10.55/10.56: capacity is an integer; dataStoreRef must name a DataStore; itemSubjectRef resolves.
#[test]
fn data_store_and_reference_are_validated() {
    let ok = project(|d, p| {
        d.data_stores = vec![store("s1", "12", "single"), store("s2", "+5", "single")];
        let mut by_qname = node(
            "qref",
            NodeKind::DataStoreReference,
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(DataStoreRef::Qualified(q("s1"))),
            }),
        );
        by_qname.item_aware = aware("single");
        p.nodes.push(by_qname);
        p.nodes.push(node(
            "lref",
            NodeKind::DataStoreReference,
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(DataStoreRef::Legacy(id("s2"))),
            }),
        ));
    });
    for code in [
        "dataStoreCapacityInvalid",
        "dataStoreRefUnresolved",
        "dataItemSubjectUnresolved",
    ] {
        assert_eq!(count(&ok, code), 0, "{code}");
    }
    let bad = project(|d, p| {
        d.data_stores = vec![store("s1", "many", "missing")];
        let mut dangling = node(
            "qref",
            NodeKind::DataStoreReference,
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(DataStoreRef::Qualified(q("ghost"))),
            }),
        );
        dangling.item_aware = aware("missing");
        p.nodes.push(dangling);
        p.nodes.push(node(
            "lref",
            NodeKind::DataStoreReference,
            Some(NodeDetails::DataStoreReference {
                data_store_ref: Some(DataStoreRef::Legacy(id("ghost"))),
            }),
        ));
    });
    assert!(has(&bad, "dataStoreCapacityInvalid", "s1"));
    assert!(has(&bad, "dataItemSubjectUnresolved", "s1"));
    assert!(has(&bad, "dataStoreRefUnresolved", "qref"));
    assert!(has(&bad, "dataStoreRefUnresolved", "lref"));
    assert!(has(&bad, "dataItemSubjectUnresolved", "qref"));
}

/// Tables 10.59/10.60: DataInput/DataOutput itemSubjectRef/isCollection and membership in at least one set.
#[test]
fn data_ports_are_validated() {
    let ok = project(|_, p| {
        p.nodes[0].io_specification = Some(io(
            vec![port("in1", false, "single")],
            vec![port("out1", true, "many")],
            vec![set("is", &["in1"], &[], &["os"])],
            vec![set("os", &["out1"], &[], &["is"])],
        ));
    });
    for code in [
        "dataItemSubjectUnresolved",
        "dataCollectionMismatch",
        "dataPortWithoutSet",
    ] {
        assert_eq!(count(&ok, code), 0, "{code}");
    }
    let bad = project(|_, p| {
        p.nodes[0].io_specification = Some(io(
            vec![
                port("in1", true, "single"),
                port("in2", false, "missing"),
                port("orphanIn", false, "single"),
            ],
            vec![
                port("out1", false, "many"),
                port("orphanOut", false, "single"),
            ],
            vec![set("is", &["in1", "in2"], &[], &[])],
            vec![set("os", &["out1"], &[], &[])],
        ));
    });
    assert!(has(&bad, "dataCollectionMismatch", "in1"));
    assert!(has(&bad, "dataCollectionMismatch", "out1"));
    assert!(has(&bad, "dataItemSubjectUnresolved", "in2"));
    assert!(has(&bad, "dataPortWithoutSet", "orphanIn"));
    assert!(has(&bad, "dataPortWithoutSet", "orphanOut"));
    assert_eq!(count(&bad, "dataPortWithoutSet"), 2);
}

/// Tables 10.61/10.62: optional / whileExecuting members must be in dataRefs; references must resolve.
#[test]
fn input_and_output_set_references_are_validated() {
    let ok = project(|_, p| {
        let mut input_set = set("is", &["in1", "in2"], &["in2"], &["os"]);
        input_set.while_executing_refs = sels(&["in1"]);
        p.nodes[0].io_specification = Some(io(
            vec![port("in1", false, "single"), port("in2", false, "single")],
            vec![port("out1", false, "single")],
            vec![input_set],
            vec![set("os", &["out1"], &["out1"], &["is"])],
        ));
    });
    for code in [
        "ioSetMemberNotInDataRefs",
        "ioSetPortRefUnresolved",
        "ioSetCounterpartUnresolved",
    ] {
        assert_eq!(count(&ok, code), 0, "{code}");
    }
    let bad = project(|_, p| {
        let mut input_set = set("is", &["in1"], &["in2"], &["ghostSet"]);
        input_set.while_executing_refs = sels(&["in2"]);
        p.nodes[0].io_specification = Some(io(
            vec![port("in1", false, "single"), port("in2", false, "single")],
            vec![port("out1", false, "single")],
            vec![input_set],
            vec![set("os", &["ghostPort"], &["out1"], &[])],
        ));
    });
    // is: optional + whileExecuting members outside dataRefs; os: optional member outside dataRefs.
    assert_eq!(count(&bad, "ioSetMemberNotInDataRefs"), 3);
    assert!(has(&bad, "ioSetMemberNotInDataRefs", "is"));
    assert!(has(&bad, "ioSetMemberNotInDataRefs", "os"));
    assert!(has(&bad, "ioSetPortRefUnresolved", "os"));
    assert!(has(&bad, "ioSetCounterpartUnresolved", "is"));
}

/// Table 10.63 and §10.4.1/§10.4.2: endpoints are ItemAwareElements; Process ports are not ordinary endpoints.
#[test]
fn data_association_endpoints_are_validated() {
    let ok = project(|_, p| {
        p.data_objects = vec![object("obj", false, "single")];
        p.nodes[0].io_specification = Some(io(
            vec![port("in1", false, "single")],
            vec![port("out1", false, "single")],
            vec![set("is", &["in1"], &[], &[])],
            vec![set("os", &["out1"], &[], &[])],
        ));
        p.data_associations = vec![
            assoc("a1", "activity", &["obj"], "in1"),
            assoc("a2", "activity", &["out1"], "obj"),
        ];
    });
    assert_eq!(count(&ok, "dataAssociationEndpointUnresolved"), 0);
    let bad = project(|_, p| {
        p.data_objects = vec![object("obj", false, "single")];
        p.data_associations = vec![
            assoc("a1", "activity", &["ghost"], "obj"),
            assoc("a2", "activity", &["obj"], "ghostTarget"),
        ];
    });
    assert!(has(&bad, "dataAssociationEndpointUnresolved", "a1"));
    assert!(has(&bad, "dataAssociationEndpointUnresolved", "a2"));
    assert_eq!(count(&bad, "dataAssociationEndpointUnresolved"), 2);

    let process_ports = |owner: &'static str| {
        project(move |_, p| {
            p.io_specification = Some(io(
                vec![port("pin", false, "single")],
                vec![port("pout", false, "single")],
                vec![set("pis", &["pin"], &[], &[])],
                vec![set("pos", &["pout"], &[], &[])],
            ));
            p.data_objects = vec![object("obj", false, "single")];
            p.nodes.push(node("start", NodeKind::StartEvent, None));
            p.nodes.push(node("end", NodeKind::EndEvent, None));
            p.data_associations = vec![
                assoc("toIn", owner, &["obj"], "pin"),
                assoc("fromOut", owner, &["pout"], "obj"),
            ];
        })
    };
    let from_task = process_ports("activity");
    assert!(has(&from_task, "dataAssociationProcessInputTarget", "toIn"));
    assert!(has(
        &from_task,
        "dataAssociationProcessOutputSource",
        "fromOut"
    ));
    // Start Event may fill the Process Data Inputs and End Event may read the Process Data Outputs (§10.4.2).
    assert_eq!(
        count(&process_ports("start"), "dataAssociationProcessInputTarget"),
        0
    );
    assert_eq!(
        count(&process_ports("end"), "dataAssociationProcessOutputSource"),
        0
    );
    assert_eq!(
        count(
            &process_ports("start"),
            "dataAssociationProcessOutputSource"
        ),
        1
    );
}
