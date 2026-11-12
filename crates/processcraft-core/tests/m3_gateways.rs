use processcraft_core::{
    DiagnosticSeverity,
    bpmn_model::{EventGatewayType, GatewayDirection, NodeDetails},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::DocumentId,
    model::*,
};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn apply(s: &mut Session, r: Request) -> Result<(), CommandError> {
    let env = Envelope {
        request_id: "test".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(env, r).map(|_| ())
}
fn batch(m: Vec<Mutation>) -> Request {
    Request::Batch { mutations: m }
}
fn mutation(json: serde_json::Value) -> Mutation {
    serde_json::from_value(json).unwrap()
}
fn create_element(name: &str, kind: &str) -> Mutation {
    mutation(serde_json::json!({
        "type": "createElement", "document_id": docid().to_string(), "process_id": "procurement",
        "element": {"id": name, "name": name, "documentation": "", "kind": kind, "owner": "", "contact": ""}
    }))
}
fn create_flow(name: &str, source: &str, target: &str) -> Mutation {
    mutation(serde_json::json!({
        "type": "createFlow", "document_id": docid().to_string(), "process_id": "procurement",
        "flow": {"id": name, "kind": "Sequence", "source": source, "target": target,
                 "name": "", "documentation": "", "condition": null}
    }))
}
fn session() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let r: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    apply(&mut s, r).unwrap();
    s
}
fn gateway_details(
    default_flow: Option<&str>,
    direction: Option<GatewayDirection>,
    instantiate: Option<bool>,
    event_gateway_type: Option<EventGatewayType>,
) -> NodeDetails {
    NodeDetails::Gateway {
        default_flow: default_flow.map(id),
        direction,
        instantiate,
        event_gateway_type,
        activation_condition: None,
    }
}
fn set_details(element: &str, details: Option<NodeDetails>) -> Mutation {
    Mutation::SetNodeDetails {
        document_id: docid(),
        element_id: id(element),
        details,
    }
}
fn node(s: &Session, name: &str) -> Option<Node> {
    s.snapshot().project.documents[0].processes[0]
        .nodes
        .iter()
        .find(|n| n.id.to_string() == name)
        .cloned()
}
fn codes_for(s: &Session, element: &str) -> Vec<&'static str> {
    s.snapshot()
        .project
        .diagnostics()
        .iter()
        .filter(|d| d.element_id == Some(id(element)))
        .map(|d| d.code)
        .collect()
}

#[test]
fn inclusive_and_parallel_gateways_are_created_edited_and_undone() {
    let mut s = session();
    let before = s.snapshot().content_id;
    let inc_details = gateway_details(Some("inc_a"), Some(GatewayDirection::Diverging), None, None);
    let par_details = gateway_details(None, Some(GatewayDirection::Converging), None, None);
    apply(
        &mut s,
        batch(vec![
            create_element("inc", "InclusiveGateway"),
            create_element("par", "ParallelGateway"),
            create_flow("inc_a", "inc", "order"),
            create_flow("inc_b", "inc", "end"),
            set_details("inc", Some(inc_details.clone())),
            set_details("par", Some(par_details.clone())),
        ]),
    )
    .unwrap();
    let after = s.snapshot().content_id;
    let inc = node(&s, "inc").unwrap();
    assert_eq!(inc.kind, NodeKind::InclusiveGateway);
    assert_eq!(inc.details, Some(inc_details));
    let par = node(&s, "par").unwrap();
    assert_eq!(par.kind, NodeKind::ParallelGateway);
    assert_eq!(par.details, Some(par_details));

    // Clear the inclusive default, then undo restores it.
    apply(
        &mut s,
        batch(vec![set_details(
            "inc",
            Some(gateway_details(
                None,
                Some(GatewayDirection::Diverging),
                None,
                None,
            )),
        )]),
    )
    .unwrap();
    assert!(matches!(
        node(&s, "inc").unwrap().details,
        Some(NodeDetails::Gateway {
            default_flow: None,
            ..
        })
    ));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
    assert!(matches!(
        node(&s, "inc").unwrap().details,
        Some(NodeDetails::Gateway {
            default_flow: Some(_),
            ..
        })
    ));
    apply(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
    assert!(node(&s, "inc").is_none() && node(&s, "par").is_none());
    apply(&mut s, Request::Redo).unwrap();
    assert_eq!(s.snapshot().content_id, after);
}

#[test]
fn exclusive_default_flow_is_set_cleared_and_undone() {
    let mut s = session();
    assert!(node(&s, "decision").unwrap().details.is_none());
    let with_default = gateway_details(Some("flow_approved"), None, None, None);
    apply(
        &mut s,
        batch(vec![set_details("decision", Some(with_default.clone()))]),
    )
    .unwrap();
    assert_eq!(node(&s, "decision").unwrap().details, Some(with_default));
    apply(&mut s, batch(vec![set_details("decision", None)])).unwrap();
    assert!(node(&s, "decision").unwrap().details.is_none());
    apply(&mut s, Request::Undo).unwrap();
    assert!(matches!(
        node(&s, "decision").unwrap().details,
        Some(NodeDetails::Gateway {
            default_flow: Some(_),
            ..
        })
    ));
    apply(&mut s, Request::Undo).unwrap();
    assert!(node(&s, "decision").unwrap().details.is_none());
}

#[test]
fn event_based_gateway_attributes_are_edited_and_undone() {
    let mut s = session();
    let parallel = gateway_details(None, None, Some(true), Some(EventGatewayType::Parallel));
    apply(
        &mut s,
        batch(vec![
            create_element("evg", "EventBasedGateway"),
            set_details("evg", Some(parallel.clone())),
        ]),
    )
    .unwrap();
    assert_eq!(node(&s, "evg").unwrap().details, Some(parallel));
    let exclusive = gateway_details(None, None, Some(false), Some(EventGatewayType::Exclusive));
    apply(
        &mut s,
        batch(vec![set_details("evg", Some(exclusive.clone()))]),
    )
    .unwrap();
    assert_eq!(node(&s, "evg").unwrap().details, Some(exclusive));
    apply(&mut s, Request::Undo).unwrap();
    assert!(matches!(
        node(&s, "evg").unwrap().details,
        Some(NodeDetails::Gateway {
            instantiate: Some(true),
            event_gateway_type: Some(EventGatewayType::Parallel),
            ..
        })
    ));
}

#[test]
fn inclusive_and_parallel_gateways_diagnose_misuse() {
    let mut s = session();
    apply(
        &mut s,
        batch(vec![
            create_element("inc", "InclusiveGateway"),
            create_element("par", "ParallelGateway"),
            // one in, one out: neither splits nor merges (clause 10.6.1)
            create_flow("in_inc", "start", "inc"),
            create_flow("out_inc", "inc", "order"),
            create_flow("in_par", "review", "par"),
            create_flow("out_par", "par", "end"),
            set_details(
                "inc",
                Some(gateway_details(
                    Some("out_inc"),
                    Some(GatewayDirection::Diverging),
                    None,
                    None,
                )),
            ),
            set_details(
                "par",
                Some(gateway_details(
                    None,
                    Some(GatewayDirection::Mixed),
                    None,
                    None,
                )),
            ),
            mutation(serde_json::json!({
                "type": "setFlowCondition", "document_id": docid().to_string(),
                "flow_id": "out_inc", "condition": "x"
            })),
        ]),
    )
    .unwrap();
    for gateway in ["inc", "par"] {
        let codes = codes_for(&s, gateway);
        assert!(codes.contains(&"gatewayDoesNotSplitOrMerge"), "{codes:?}");
        assert!(codes.contains(&"gatewayDirectionMismatch"), "{codes:?}");
    }
    let diagnostics = s.snapshot().project.diagnostics();
    let default_condition = diagnostics
        .iter()
        .find(|d| d.code == "defaultFlowCondition" && d.element_id == Some(id("out_inc")))
        .expect("inclusive default flow with condition");
    assert_eq!(default_condition.severity, DiagnosticSeverity::Warning);
}

#[test]
fn event_based_gateway_attribute_rules_are_diagnosed() {
    let mut s = session();
    apply(
        &mut s,
        batch(vec![
            create_element("evg", "EventBasedGateway"),
            create_flow("in_evg", "review", "evg"),
            create_flow("out_evg1", "evg", "order"),
            create_flow("out_evg2", "evg", "end"),
            set_details(
                "evg",
                Some(gateway_details(
                    None,
                    None,
                    Some(true),
                    Some(EventGatewayType::Exclusive),
                )),
            ),
        ]),
    )
    .unwrap();
    assert!(codes_for(&s, "evg").contains(&"instantiatingGatewayIncoming"));
    assert!(!codes_for(&s, "evg").contains(&"parallelEventGatewayNotInstantiating"));
    apply(
        &mut s,
        batch(vec![set_details(
            "evg",
            Some(gateway_details(
                None,
                None,
                Some(false),
                Some(EventGatewayType::Parallel),
            )),
        )]),
    )
    .unwrap();
    let codes = codes_for(&s, "evg");
    assert!(codes.contains(&"parallelEventGatewayNotInstantiating"));
    assert!(!codes.contains(&"instantiatingGatewayIncoming"));
}
