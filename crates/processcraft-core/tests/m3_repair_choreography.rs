use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_collaboration::{
        Choreography, ChoreographyActivity, ChoreographyActivityDetails, ChoreographyFlowElement,
        ChoreographyGraph, ChoreographyRootKind,
    },
    model::*,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn local(s: &str) -> ModelReference {
    ModelReference::Local(id(s))
}
fn participant(name: &str) -> Participant {
    Participant {
        interface_refs: vec![],
        endpoint_refs: vec![],
        multiplicity: None,
        base: None,
        id: id(name),
        name: Some(name.into()),
        process: None,
    }
}
fn activity(name: &str, refs: &[&str]) -> ChoreographyActivity {
    ChoreographyActivity {
        loop_type: None,
        details: None,
        flow_metadata: None,
        correlation_keys: vec![],
        base: None,
        id: id(name),
        name: Some(name.into()),
        participant_refs: refs.iter().map(|r| local(r)).collect(),
        initiating_participant: Some(local(refs[0])),
        message_flow_refs: vec![],
    }
}
fn root(name: &str, participants: &[&str]) -> Choreography {
    Choreography {
        kind: None,
        graph: None,
        extras: None,
        conversation_nodes: vec![],
        conversation_links: vec![],
        correlation_keys: vec![],
        base: None,
        id: id(name),
        name: Some(name.into()),
        participants: participants.iter().map(|p| participant(p)).collect(),
        message_flows: vec![],
        activities: vec![],
        sequence_flows: vec![],
    }
}

#[test]
fn global_choreography_task_kind_and_initiator_are_typed() {
    let mut global = root("global", &["gb", "gs"]);
    assert!(global.kind.is_none());
    global.kind = Some(ChoreographyRootKind::Global {
        initiating_participant_ref: Some(local("gs")),
    });
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.choreographies = vec![global];
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    assert!(
        p.diagnostics()
            .iter()
            .all(|d| !d.code.starts_with("globalChoreography"))
    );
    let back: Choreography =
        serde_json::from_str(&serde_json::to_string(&p.documents[0].choreographies[0]).unwrap())
            .unwrap();
    assert_eq!(
        back.kind,
        Some(ChoreographyRootKind::Global {
            initiating_participant_ref: Some(local("gs"))
        })
    );
}

fn sub_project(nested_refs: &[&str]) -> ProjectData {
    let mut sub = activity("sub", &["a", "b"]);
    sub.details = Some(ChoreographyActivityDetails::Sub {
        graph: Box::new(ChoreographyGraph {
            data_associations: vec![],
            flow_elements: vec![ChoreographyFlowElement::Activity(Box::new(activity(
                "nested",
                nested_refs,
            )))],
        }),
        artifacts: vec![],
    });
    let mut main = root("main", &["a", "b", "c"]);
    main.activities = vec![sub];
    let mut d = BpmnDocument::new("doc".into(), id("defs"));
    d.choreographies = vec![main];
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(d));
    p
}

#[test]
fn nested_participant_outside_sub_choreography_is_diagnosed() {
    let found = sub_project(&["a", "c"]).diagnostics();
    let hit: Vec<_> = found
        .iter()
        .filter(|d| d.code == "subChoreographyParticipantSubset")
        .collect();
    assert_eq!(hit.len(), 1, "{found:?}");
    assert_eq!(hit[0].element_id.as_ref().unwrap().to_string(), "nested");
    assert!(hit[0].message.contains("§11.5.2"));
    assert!(
        sub_project(&["a", "b"])
            .diagnostics()
            .iter()
            .all(|d| d.code != "subChoreographyParticipantSubset")
    );
}
