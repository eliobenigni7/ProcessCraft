//! Round-2 integration validate evidence: lane partitions (BPMN 2.0.2 §10.8, Table 10.135),
//! BPMNLabel styles (§12.2.3.7) and TextAnnotation textFormat (Table 8.24).
use processcraft_core::{
    bpmn_activity::ModelReference, bpmn_model::NodeDetails, ids::ElementSelector, model::*,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
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
fn project(edit: impl FnOnce(&mut BpmnDocument, &mut Process)) -> ProjectData {
    let mut p = ProjectData::new("p".into());
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    let mut process = Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        lane_set_name: None,
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

fn lane(name: &str, parent: Option<&str>, partition_ref: Option<&str>, inline: bool) -> Lane {
    Lane {
        base: None,
        child_lane_set_id: None,
        child_lane_set_base: None,
        child_lane_set_name: None,
        partition_element: inline.then(|| LanePartitionElement {
            id: id(&format!("{name}_part")),
            value: Some("v".into()),
        }),
        partition_element_ref: partition_ref.map(|r| ModelReference::Local(id(r))),
        id: id(name),
        name: Some(name.into()),
        parent: parent.map(id),
        members: vec![],
    }
}

#[test]
fn lane_partition_reference_must_resolve() {
    let ok = project(|_, p| p.lanes = vec![lane("l1", None, Some("activity"), false)]);
    assert_eq!(count(&ok, "danglingPartitionElementRef"), 0);
    let bad = project(|_, p| p.lanes = vec![lane("l1", None, Some("missing"), false)]);
    assert!(has(&bad, "danglingPartitionElementRef", "l1"));
}

#[test]
fn lanes_of_one_lane_set_must_share_partition_type() {
    // inline partitionElement is a CategoryValue; a reference to a flow node is a different type.
    let mixed = project(|_, p| {
        p.lanes = vec![
            lane("l1", None, None, true),
            lane("l2", None, Some("activity"), false),
        ]
    });
    assert!(has(&mixed, "mixedLanePartitionTypes", "l1"));
    assert!(has(&mixed, "mixedLanePartitionTypes", "l2"));
    let same = project(|_, p| {
        p.lanes = vec![
            lane("l1", None, None, true),
            lane("l2", None, None, true),
            // a child lane set is a separate LaneSet and may use another type
            lane("l3", Some("l1"), Some("activity"), false),
        ]
    });
    assert_eq!(count(&same, "mixedLanePartitionTypes"), 0);
}

fn with_label_style(style_ref: &str) -> ProjectData {
    let mut p = project(|_, _| {});
    let mut d = (*p.documents[0]).clone();
    let label = Label {
        di_metadata: None,
        id: None,
        bounds: None,
        style: None,
        style_ref: Some(DiStyleRef::Local(id(style_ref))),
    };
    d.diagrams.push(Diagram {
        plane_metadata: None,
        plane_order: None,
        name: None,
        documentation: None,
        resolution: None,
        label_styles: vec![LabelStyle {
            id: Some(id("style")),
            font: None,
        }],
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes: vec![Shape {
            di_metadata: None,
            is_message_visible: None,
            participant_band_kind: None,
            choreography_activity_shape: None,
            id: id("shape"),
            element: id("activity"),
            is_expanded: None,
            is_horizontal: None,
            is_marker_visible: None,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            },
            label: Some(label),
            label_visibility: None,
            style: None,
        }],
        edges: vec![],
    });
    p.documents[0] = Arc::new(d);
    p
}

#[test]
fn bpmn_label_style_must_be_owned_by_the_diagram() {
    assert_eq!(count(&with_label_style("style"), "danglingLabelStyle"), 0);
    assert!(has(
        &with_label_style("other"),
        "danglingLabelStyle",
        "shape"
    ));
}

#[test]
fn text_annotation_text_format_must_be_a_mime_type() {
    let annotated = |format: &str| {
        project(|_, p| {
            p.nodes.push(node(
                "note",
                NodeKind::TextAnnotation,
                Some(NodeDetails::TextAnnotation {
                    text_format: Some(format.into()),
                    text: None,
                }),
            ))
        })
    };
    assert_eq!(count(&annotated("text/plain"), "invalidTextFormat"), 0);
    assert_eq!(
        count(&annotated("text/html; charset=utf-8"), "invalidTextFormat"),
        0
    );
    assert!(has(&annotated("textplain"), "invalidTextFormat", "note"));
    assert!(has(&annotated("a b/c"), "invalidTextFormat", "note"));
}

#[test]
fn public_process_must_not_be_executable() {
    use processcraft_core::bpmn_activity::{ProcessActivityMetadata, ProcessType};
    let process_with = |ty: ProcessType, executable: bool| {
        project(|_, p| {
            p.is_executable = Some(executable);
            p.activity_metadata = Some(ProcessActivityMetadata {
                process_type: Some(ty),
                ..Default::default()
            });
        })
    };
    assert!(has(
        &process_with(ProcessType::Public, true),
        "publicProcessExecutable",
        "process"
    ));
    assert_eq!(
        count(
            &process_with(ProcessType::Private, true),
            "publicProcessExecutable"
        ),
        0
    );
    assert_eq!(
        count(
            &process_with(ProcessType::Public, false),
            "publicProcessExecutable"
        ),
        0
    );
}

#[test]
fn collaboration_and_its_choreography_must_agree_on_is_closed() {
    use processcraft_core::bpmn_collaboration::{Choreography, CollaborationExtras};
    let build = |collab_closed: bool, chor_closed: bool| {
        project(|d, _| {
            let extras = |closed, refs| {
                Some(CollaborationExtras {
                    is_closed: Some(closed),
                    choreography_refs: refs,
                    ..Default::default()
                })
            };
            d.collaboration = Some(Collaboration {
                kind: None,
                extras: extras(collab_closed, vec![ModelReference::Local(id("chor"))]),
                correlation_keys: vec![],
                base: None,
                name: None,
                id: id("collab"),
                participants: vec![],
                flows: vec![],
                conversation_nodes: vec![],
                conversation_links: vec![],
            });
            d.choreographies.push(Choreography {
                kind: None,
                graph: None,
                extras: extras(chor_closed, vec![]),
                conversation_nodes: vec![],
                conversation_links: vec![],
                correlation_keys: vec![],
                base: None,
                id: id("chor"),
                name: None,
                participants: vec![],
                message_flows: vec![],
                activities: vec![],
                sequence_flows: vec![],
            });
        })
    };
    assert!(has(
        &build(true, false),
        "collaborationChoreographyClosedMismatch",
        "collab"
    ));
    assert_eq!(
        count(
            &build(true, true),
            "collaborationChoreographyClosedMismatch"
        ),
        0
    );
}

#[test]
fn message_item_ref_must_resolve_to_an_item_definition() {
    use processcraft_core::{
        bpmn_catalog::ItemDefinition,
        bpmn_model::{EventResource, QNameRef},
    };
    let build = |item: &str| {
        project(|d, _| {
            d.catalog.item_definitions.push(ItemDefinition {
                base: None,
                id: Some(id("payload")),
                structure_ref: None,
                is_collection: None,
                item_kind: None,
            });
            d.event_resources.push(EventResource::Message {
                id: id("msg"),
                base: None,
                name: None,
                item_ref: Some(QNameRef {
                    namespace: GENERATED_TARGET_NAMESPACE.into(),
                    local: id(item),
                }),
            });
        })
    };
    assert_eq!(count(&build("payload"), "messageItemRefUnresolved"), 0);
    assert!(has(&build("nope"), "messageItemRefUnresolved", "msg"));
}
