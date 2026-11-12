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

// ---- Activities, loops, events (BPMN 2.0.2 §§8.4.7, 10.3, 10.5) ----
use processcraft_core::{
    bpmn_activity::{ActivityProperties, FlowElementMetadata, MultiInstanceFlowCondition},
    bpmn_catalog::{Interface, Operation},
    bpmn_model::{
        EventDefinition, EventReference, Expression, ExpressionBody, ExpressionData,
        ExpressionKind, LoopCharacteristics,
    },
};

fn with_loop(l: LoopCharacteristics) -> ProjectData {
    project(|_, p| p.nodes[0].loop_characteristics = Some(l))
}
fn mi(f: impl FnOnce(&mut LoopCharacteristics)) -> ProjectData {
    let mut l = LoopCharacteristics::MultiInstance {
        id: None,
        base: None,
        is_sequential: None,
        behavior: None,
        loop_data_input_ref: None,
        loop_data_output_ref: None,
        input_data_item: None,
        output_data_item: None,
        complex_behavior_definitions: vec![],
        one_behavior_event_ref: None,
        none_behavior_event_ref: None,
        cardinality: Some("3".into()),
        completion_condition: None,
    };
    f(&mut l);
    with_loop(l)
}
fn msg(p: &ProjectData, code: &str) -> String {
    p.diagnostics()
        .into_iter()
        .find(|d| d.code == code)
        .unwrap()
        .message
}
fn local(s: &str) -> EventReference {
    EventReference::Local(id(s))
}
fn event(name: &str, kind: NodeKind, defs: Vec<EventDefinition>, cancel: Option<bool>) -> Node {
    node(
        name,
        kind,
        Some(NodeDetails::Event {
            definitions: defs,
            attached_to: Some(local("activity")),
            cancel_activity: cancel,
            definition_refs: vec![],
            parallel_multiple: None,
            is_interrupting: None,
            properties: vec![],
            io: None,
        }),
    )
}

/// Table 10.28: loopMaximum is an integer; testBefore/loopCondition add no further constraint.
#[test]
fn standard_loop_maximum_is_validated() {
    let std = |m: Option<&str>| LoopCharacteristics::Standard {
        id: None,
        base: None,
        test_before: Some(true),
        condition: None,
        maximum: m.map(Into::into),
    };
    for ok in [None, Some("0"), Some("12"), Some("+3")] {
        assert_eq!(
            count(&with_loop(std(ok)), "standardLoopMaximumNotInteger"),
            0
        );
    }
    let bad = with_loop(std(Some("many")));
    assert!(has(&bad, "standardLoopMaximumNotInteger", "activity"));
    assert!(msg(&bad, "standardLoopMaximumNotInteger").contains("Table 10.28"));
    assert_eq!(
        count(&with_loop(std(Some(""))), "standardLoopMaximumNotInteger"),
        1
    );
}

/// Table 10.29: either loopCardinality or loopDataInput MUST be specified; loop refs must resolve.
#[test]
fn multi_instance_initialisation_and_references_are_validated() {
    assert_eq!(count(&mi(|_| {}), "incompleteMultiInstanceInput"), 0);
    let none = mi(|l| {
        if let LoopCharacteristics::MultiInstance { cardinality, .. } = l {
            *cardinality = None
        }
    });
    assert!(has(&none, "incompleteMultiInstanceInput", "activity"));
    assert!(msg(&none, "incompleteMultiInstanceInput").contains("Table 10.29"));
    // Dangling loopDataOutputRef / behavior event refs are reported.
    let dangling = mi(|l| {
        if let LoopCharacteristics::MultiInstance {
            loop_data_output_ref,
            one_behavior_event_ref,
            none_behavior_event_ref,
            behavior,
            ..
        } = l
        {
            *loop_data_output_ref = Some(local("ghostOut"));
            *one_behavior_event_ref = Some(local("ghostOne"));
            *none_behavior_event_ref = Some(local("ghostNone"));
            *behavior = Some(MultiInstanceFlowCondition::All);
        }
    });
    assert_eq!(count(&dangling, "unresolvedActivityFamilyReference"), 3);
}

/// Table 10.29: oneBehaviorEventRef / noneBehaviorEventRef apply only to behavior One / None (default All).
#[test]
fn multi_instance_behavior_event_refs_must_match_behavior() {
    use MultiInstanceFlowCondition::*;
    let refs = |behavior: Option<MultiInstanceFlowCondition>, one: bool, none: bool| {
        mi(|l| {
            if let LoopCharacteristics::MultiInstance {
                behavior: b,
                one_behavior_event_ref,
                none_behavior_event_ref,
                ..
            } = l
            {
                *b = behavior;
                let r = || {
                    Some(EventReference::Qualified(QNameRef {
                        namespace: "urn:ext".into(),
                        local: id("evt"),
                    }))
                };
                if one {
                    *one_behavior_event_ref = r()
                }
                if none {
                    *none_behavior_event_ref = r()
                }
            }
        })
    };
    let ok = refs(Some(One), true, false);
    assert_eq!(count(&ok, "multiInstanceOneEventBehaviorMismatch"), 0);
    assert_eq!(
        count(
            &refs(Some(None), false, true),
            "multiInstanceNoneEventBehaviorMismatch"
        ),
        0
    );
    let bad_one = refs(Some(All), true, false);
    assert!(has(
        &bad_one,
        "multiInstanceOneEventBehaviorMismatch",
        "activity"
    ));
    assert!(msg(&bad_one, "multiInstanceOneEventBehaviorMismatch").contains("Table 10.29"));
    // Omitted behavior defaults to All.
    assert_eq!(
        count(
            &refs(Option::None, false, true),
            "multiInstanceNoneEventBehaviorMismatch"
        ),
        1
    );
    assert_eq!(
        count(
            &refs(Some(Complex), true, true),
            "multiInstanceOneEventBehaviorMismatch"
        ),
        1
    );
}

/// Table 10.29: inputDataItem / outputDataItem MUST be the scalar of the loop collection.
#[test]
fn multi_instance_data_items_must_be_scalar() {
    let items = |collection: Option<bool>, item: &str| {
        mi(|l| {
            if let LoopCharacteristics::MultiInstance {
                input_data_item,
                output_data_item,
                ..
            } = l
            {
                let mut p = port("item", false, item);
                p.is_collection = collection;
                *input_data_item = Some(p.clone());
                p.id = Some(id("itemOut"));
                *output_data_item = Some(p);
            }
        })
    };
    let ok = items(Some(false), "single");
    assert_eq!(
        count(&ok, "multiInstanceInputItemNotScalar")
            + count(&ok, "multiInstanceOutputItemNotScalar"),
        0
    );
    let bad = items(Some(true), "single");
    assert!(has(&bad, "multiInstanceInputItemNotScalar", "item"));
    assert!(has(&bad, "multiInstanceOutputItemNotScalar", "itemOut"));
    // A scalar flag whose ItemDefinition is a collection is also not scalar.
    assert_eq!(
        count(
            &items(Some(false), "many"),
            "multiInstanceInputItemNotScalar"
        ),
        1
    );
}

/// Table 10.92: Error and Cancel boundary Events always cancel the Activity.
#[test]
fn error_and_cancel_boundary_events_must_interrupt() {
    let boundary = |defs: Vec<EventDefinition>, cancel: Option<bool>| {
        project(|_, p| {
            p.nodes
                .push(event("b", NodeKind::BoundaryEvent, defs, cancel))
        })
    };
    let error = || EventDefinition::Error {
        id: None,
        base: None,
        error_ref: None,
    };
    let cancel_def = || EventDefinition::Cancel {
        id: None,
        base: None,
    };
    let signal = || EventDefinition::Signal {
        id: None,
        base: None,
        signal_ref: None,
    };
    for def in [error(), cancel_def()] {
        assert_eq!(
            count(
                &boundary(vec![def.clone()], Some(true)),
                "boundaryErrorCancelNotInterrupting"
            ),
            0
        );
        let bad = boundary(vec![def], Some(false));
        assert!(has(&bad, "boundaryErrorCancelNotInterrupting", "b"));
        assert!(msg(&bad, "boundaryErrorCancelNotInterrupting").contains("Table"));
    }
    // Multiple triggers containing Error must interrupt; Signal may be non-interrupting.
    assert_eq!(
        count(
            &boundary(vec![signal(), error()], Some(false)),
            "boundaryErrorCancelNotInterrupting"
        ),
        1
    );
    assert_eq!(
        count(
            &boundary(vec![signal()], Some(false)),
            "boundaryErrorCancelNotInterrupting"
        ),
        0
    );
}

/// Table 10.95: a Conditional trigger in an executable Process needs a FormalExpression.
#[test]
fn conditional_event_needs_formal_expression_when_executable() {
    let expr = |formal: bool| {
        Expression::Structured(ExpressionData {
            id: None,
            kind: if formal {
                ExpressionKind::Formal {
                    explicit_type: true,
                    language: None,
                    evaluates_to_type_ref: None,
                }
            } else {
                ExpressionKind::Base {
                    explicit_type: false,
                }
            },
            text: ExpressionBody::Text("x > 1".into()),
            namespaces: vec![],
            foreign_attributes: vec![],
        })
    };
    let build = |executable: bool, formal: bool| {
        project(|_, p| {
            p.is_executable = Some(executable);
            p.nodes.push(event(
                "c",
                NodeKind::IntermediateCatchEvent,
                vec![EventDefinition::Conditional {
                    id: Some(id("cond")),
                    base: None,
                    expression: expr(formal),
                }],
                None,
            ));
        })
    };
    assert_eq!(
        count(&build(true, true), "conditionalEventNotFormalExpression"),
        0
    );
    assert_eq!(
        count(&build(false, false), "conditionalEventNotFormalExpression"),
        0
    );
    let bad = build(true, false);
    assert!(has(&bad, "conditionalEventNotFormalExpression", "cond"));
    assert!(msg(&bad, "conditionalEventNotFormalExpression").contains("Table 10.95"));
}

/// Table 10.8: implementation is ##unspecified, ##WebService or a URI; operationRef must resolve.
#[test]
fn service_task_implementation_and_operation_are_validated() {
    let svc = |implementation: Option<&str>, operation: Option<&str>| {
        project(|d, p| {
            d.catalog.interfaces.push(Interface {
                base: None,
                id: Some(id("iface")),
                name: "I".into(),
                implementation_ref: None,
                operations: vec![Operation {
                    base: None,
                    id: Some(id("op")),
                    name: "op".into(),
                    implementation_ref: None,
                    in_message_ref: q("m"),
                    out_message_ref: None,
                    error_refs: vec![],
                }],
            });
            let mut n = node("svc", NodeKind::ServiceTask, None);
            n.activity = Some(ActivityProperties {
                implementation: implementation.map(Into::into),
                operation_ref: operation.map(q),
                ..Default::default()
            });
            p.nodes.push(n);
        })
    };
    for ok in [
        None,
        Some("##unspecified"),
        Some("##WebService"),
        Some("http://example.com/rpc"),
    ] {
        assert_eq!(
            count(&svc(ok, Some("op")), "activityImplementationInvalid"),
            0
        );
    }
    assert_eq!(
        count(
            &svc(Some("##Carrier"), None),
            "activityImplementationInvalid"
        ),
        1
    );
    assert!(has(
        &svc(Some("not a uri"), None),
        "activityImplementationInvalid",
        "svc"
    ));
    assert_eq!(
        count(&svc(None, Some("op")), "missingLocalActivityOperation"),
        0
    );
    assert!(has(
        &svc(None, Some("ghost")),
        "missingLocalActivityOperation",
        "svc"
    ));
}

/// Table 10.3 and §10.7.1: a Compensation Activity lies outside the normal Sequence Flow.
#[test]
fn compensation_activity_is_not_connected_to_normal_flow() {
    let seq = |fid: &str, from: &str, to: &str| Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id(fid),
        kind: FlowKind::Sequence,
        source: Some(local(from)),
        target: Some(local(to)),
        name: None,
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    };
    let build = |handler_chain: bool| {
        project(|_, p| {
            let mut comp = node("undo", NodeKind::Task, None);
            comp.activity = Some(ActivityProperties {
                is_for_compensation: Some(true),
                ..Default::default()
            });
            let mut comp2 = comp.clone();
            comp2.id = id("undo2");
            p.nodes.push(comp);
            p.nodes.push(comp2);
            p.flows.push(if handler_chain {
                seq("f", "undo", "undo2")
            } else {
                seq("f", "activity", "undo")
            });
        })
    };
    assert_eq!(count(&build(true), "compensationActivityInNormalFlow"), 0);
    let bad = build(false);
    assert!(has(&bad, "compensationActivityInNormalFlow", "undo"));
    assert!(msg(&bad, "compensationActivityInNormalFlow").contains("Table 10.3"));
}

/// Table 8.44: auditing and monitoring can only be defined for a Process.
#[test]
fn flow_elements_cannot_carry_auditing_or_monitoring() {
    let ok = project(|_, _| {});
    assert_eq!(count(&ok, "flowElementAuditMonitorMisplaced"), 0);
    let bad = project(|_, p| {
        p.nodes[0].flow_metadata = Some(FlowElementMetadata {
            auditing: Some(Default::default()),
            ..Default::default()
        });
    });
    assert!(has(&bad, "flowElementAuditMonitorMisplaced", "activity"));
    assert!(msg(&bad, "flowElementAuditMonitorMisplaced").contains("Table 8.44"));
}
