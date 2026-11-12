//! Repair evidence for the data family (BPMN 2.0.2 section 10.4.1, Tables 10.51-10.63).
use processcraft_core::{
    bpmn_catalog::ItemDefinition,
    bpmn_data::*,
    bpmn_model::{Expression, ExpressionData, ExpressionKind, NodeDetails, QNameRef},
    commands::{Envelope, Mutation, Request, Session},
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
fn port(pid: &str, item: &str) -> DataPort {
    DataPort {
        base: None,
        id: Some(id(pid)),
        name: Some(pid.into()),
        item_aware: aware(item),
        is_collection: Some(false),
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
fn assoc(aid: &str, kind: DataAssociationKind, sources: &[&str], target: &str) -> DataAssociation {
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
fn object(oid: &str) -> DataObject {
    DataObject {
        flow_metadata: None,
        base: None,
        id: Some(id(oid)),
        name: Some(oid.into()),
        is_collection: Some(false),
        item_aware: aware("single"),
    }
}
fn empty_process() -> Process {
    Process {
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
        nodes: vec![node("activity", NodeKind::Task, None)],
        lanes: vec![],
        flows: vec![],
    }
}
fn project(edit: impl FnOnce(&mut Process)) -> ProjectData {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    for item in ["single", "t1", "t2"] {
        d.catalog.item_definitions.push(ItemDefinition {
            base: None,
            id: Some(id(item)),
            structure_ref: None,
            is_collection: Some(false),
            item_kind: None,
        });
    }
    let mut process = empty_process();
    edit(&mut process);
    d.processes.push(process);
    p.documents.push(Arc::new(d));
    p
}
fn has(p: &ProjectData, code: &str, owner: &str) -> bool {
    p.diagnostics()
        .iter()
        .any(|d| d.code == code && d.element_id.as_ref() == Some(&id::<ElementSelector>(owner)))
}
fn count(p: &ProjectData, code: &str) -> usize {
    p.diagnostics().iter().filter(|d| d.code == code).count()
}
fn apply(s: &mut Session, m: Vec<Mutation>) {
    s.apply(
        Envelope {
            request_id: "x".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Batch { mutations: m },
    )
    .unwrap();
}
fn undo(s: &mut Session) {
    s.apply(
        Envelope {
            request_id: "u".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Undo,
    )
    .unwrap();
}
fn doc(s: &Session) -> Arc<BpmnDocument> {
    s.snapshot().project.documents[0].clone()
}
fn session(edit: impl FnOnce(&mut Process)) -> (Session, processcraft_core::ids::DocumentId) {
    let s = Session::new(project(edit)).unwrap();
    let d = doc(&s).id;
    (s, d)
}
fn activity_io(s: &Session) -> IoSpecification {
    doc(s).processes[0].nodes[0]
        .io_specification
        .clone()
        .unwrap()
}
fn full_io(item: &str, os_counterpart: &[&str], os_while: &[&str]) -> IoSpecification {
    let mut os = set("os", &["out1", "out2"], &[], os_counterpart);
    os.while_executing_refs = sels(os_while);
    IoSpecification {
        inputs: vec![port("in1", item)],
        outputs: vec![port("out1", "single"), port("out2", "single")],
        input_sets: vec![set(
            "is",
            &["in1"],
            &[],
            if os_counterpart.is_empty() {
                &[]
            } else {
                &["os"]
            },
        )],
        output_sets: vec![os],
        ..Default::default()
    }
}
fn set_io(s: &mut Session, d: processcraft_core::ids::DocumentId, io: IoSpecification) {
    apply(
        s,
        vec![Mutation::SetNodeIoSpecification {
            document_id: d,
            element_id: id("activity"),
            io_specification: Some(io),
        }],
    );
}

/// tInputOutputSpecification: ports and sets are typed fields that survive a serde round trip.
#[test]
fn io_specification_is_typed_and_round_trips() {
    let io = full_io("t1", &["is"], &["out2"]);
    let (mut s, d) = session(|_| {});
    set_io(&mut s, d, io.clone());
    let got = activity_io(&s);
    assert_eq!(got, io);
    let back: IoSpecification =
        serde_json::from_str(&serde_json::to_string(&got).unwrap()).unwrap();
    assert_eq!(back, io);
    assert_eq!(back.inputs.len(), 1);
    assert_eq!(back.outputs.len(), 2);
    assert_eq!(back.input_sets[0].data_refs, sels(&["in1"]));
    assert_eq!(back.output_sets[0].while_executing_refs, sels(&["out2"]));
}

/// Tables 10.61-10.62 via `incompleteIoSpecification`, on a node and on the Process.
#[test]
fn incomplete_io_specification_is_diagnosed_on_node_and_process() {
    let ok = full_io("single", &["is"], &[]);
    let good = project(|p| {
        p.nodes[0].io_specification = Some(ok.clone());
        p.io_specification = Some(ok.clone());
    });
    assert_eq!(count(&good, "incompleteIoSpecification"), 0);

    let mut no_inputs = ok.clone();
    no_inputs.input_sets.clear();
    let mut no_outputs = ok.clone();
    no_outputs.output_sets.clear();
    let mut orphan = ok.clone();
    orphan.inputs.push(port("orphan", "single"));
    for bad_io in [no_inputs, no_outputs, orphan] {
        let node_bad = project(|p| p.nodes[0].io_specification = Some(bad_io.clone()));
        assert!(has(&node_bad, "incompleteIoSpecification", "activity"));
        assert_eq!(count(&node_bad, "incompleteIoSpecification"), 1);
        let proc_bad = project(|p| p.io_specification = Some(bad_io.clone()));
        assert!(has(&proc_bad, "incompleteIoSpecification", "process"));
    }
}

/// tDataInput itemSubjectRef: edit on an input port, then undo.
#[test]
fn data_input_item_subject_ref_edit_and_undo() {
    let (mut s, d) = session(|_| {});
    set_io(&mut s, d, full_io("t1", &[], &[]));
    let item = |s: &Session| {
        activity_io(s).inputs[0]
            .item_aware
            .as_ref()
            .unwrap()
            .item_subject_ref
            .clone()
    };
    assert_eq!(item(&s), Some(q("t1")));
    set_io(&mut s, d, full_io("t2", &[], &[]));
    assert_eq!(item(&s), Some(q("t2")));
    undo(&mut s);
    assert_eq!(item(&s), Some(q("t1")));
}

/// tOutputSet inputSetRefs / whileExecutingOutputRefs: value changes after the edit and undo restores.
#[test]
fn output_set_counterpart_and_while_executing_edit_and_undo() {
    let (mut s, d) = session(|_| {});
    set_io(&mut s, d, full_io("t1", &["is"], &["out1"]));
    let os = |s: &Session| activity_io(s).output_sets[0].clone();
    assert_eq!(os(&s).counterpart_refs, sels(&["is"]));
    assert_eq!(os(&s).while_executing_refs, sels(&["out1"]));
    set_io(&mut s, d, full_io("t1", &[], &["out2"]));
    assert!(os(&s).counterpart_refs.is_empty());
    assert_eq!(os(&s).while_executing_refs, sels(&["out2"]));
    set_io(&mut s, d, full_io("t1", &["is"], &[]));
    assert_eq!(os(&s).counterpart_refs, sels(&["is"]));
    assert!(os(&s).while_executing_refs.is_empty());
    undo(&mut s);
    undo(&mut s);
    assert_eq!(os(&s).counterpart_refs, sels(&["is"]));
    assert_eq!(os(&s).while_executing_refs, sels(&["out1"]));
}

/// tDataAssociation sourceRef: sources are edited and the previous list is restored by undo.
#[test]
fn data_association_source_edit_and_undo() {
    let (mut s, d) = session(|p| {
        p.data_objects = vec![object("o1"), object("o2")];
        p.nodes[0].io_specification = Some(full_io("single", &[], &[]));
    });
    let mk = |sources: &[&str]| assoc("a1", DataAssociationKind::Input, sources, "in1");
    apply(
        &mut s,
        vec![Mutation::CreateDataAssociation {
            document_id: d,
            scope_id: id("process"),
            association: mk(&["o1"]),
        }],
    );
    let sources = |s: &Session| doc(s).processes[0].data_associations[0].sources.clone();
    assert_eq!(sources(&s), sels(&["o1"]));
    apply(
        &mut s,
        vec![Mutation::UpdateDataAssociation {
            document_id: d,
            scope_id: id("process"),
            association: mk(&["o2"]),
        }],
    );
    assert_eq!(sources(&s), sels(&["o2"]));
    undo(&mut s);
    assert_eq!(sources(&s), sels(&["o1"]));
}

/// Tables 10.61/10.62 reference rules for the sets not covered by the sibling test.
#[test]
fn set_reference_diagnostics_cover_input_data_refs_and_output_set_members() {
    let bad = project(|p| {
        let input_set = set("is", &["ghostIn"], &[], &["os"]);
        let mut os = set("os", &["out1"], &[], &["ghostIn"]);
        os.while_executing_refs = sels(&["out2"]);
        p.nodes[0].io_specification = Some(IoSpecification {
            inputs: vec![port("in1", "single")],
            outputs: vec![port("out1", "single"), port("out2", "single")],
            input_sets: vec![input_set],
            output_sets: vec![os],
            ..Default::default()
        });
    });
    assert!(has(&bad, "ioSetPortRefUnresolved", "is"));
    assert!(has(&bad, "ioSetCounterpartUnresolved", "os"));
    assert!(has(&bad, "ioSetMemberNotInDataRefs", "os"));
    let good = project(|p| {
        let mut os = set("os", &["out1", "out2"], &[], &["is"]);
        os.while_executing_refs = sels(&["out2"]);
        p.nodes[0].io_specification = Some(IoSpecification {
            inputs: vec![port("in1", "single")],
            outputs: vec![port("out1", "single"), port("out2", "single")],
            input_sets: vec![set("is", &["in1"], &[], &["os"])],
            output_sets: vec![os],
            ..Default::default()
        });
    });
    for c in [
        "ioSetPortRefUnresolved",
        "ioSetCounterpartUnresolved",
        "ioSetMemberNotInDataRefs",
    ] {
        assert_eq!(count(&good, c), 0, "{c}");
    }
}

/// tDataOutput itemSubjectRef must resolve (Table 10.60).
#[test]
fn data_output_item_subject_ref_must_resolve() {
    let bad = project(|p| {
        let mut io = full_io("single", &[], &[]);
        io.outputs[1] = port("out2", "missing");
        p.nodes[0].io_specification = Some(io);
    });
    assert!(has(&bad, "dataItemSubjectUnresolved", "out2"));
    assert!(!has(&bad, "dataItemSubjectUnresolved", "out1"));
}

/// tDataOutputAssociation: more than one source needs a transformation; adding one clears it.
#[test]
fn data_output_association_source_count_is_diagnosed() {
    let build = |transform: bool| {
        project(|p| {
            p.data_objects = vec![object("o1")];
            p.nodes[0].io_specification = Some(full_io("single", &[], &[]));
            let mut a = assoc("aout", DataAssociationKind::Output, &["out1", "out2"], "o1");
            if transform {
                a.transformation = Some(Expression::Structured(ExpressionData {
                    foreign_attributes: vec![],
                    id: None,
                    kind: ExpressionKind::Formal {
                        explicit_type: false,
                        language: None,
                        evaluates_to_type_ref: None,
                    },
                    text: "merge".into(),
                    namespaces: vec![],
                }));
            }
            p.data_associations = vec![a];
        })
    };
    assert_eq!(count(&build(false), "dataAssociationSourceCount"), 1);
    assert_eq!(count(&build(true), "dataAssociationSourceCount"), 0);
}
