//! M5.2 modeling conventions: rules, profile bounds, supersede table, cap,
//! scopes and non-editable documents.
use processcraft_core::{
    DiagnosticSeverity,
    attributes::AttributeTarget,
    bpmn_model::{EventReference, NodeDetails},
    commands::{CommandError, Envelope, Mutation, Request, Session},
    conventions::{self, ConventionRule, ConventionsProfile, MAX_DIAGNOSTICS, SUPERSEDED_BUILTINS},
    ids::{ElementIdentity, ElementSelector, XmlId},
    model::*,
    validation::Diagnostic,
};
use std::sync::Arc;

const W: DiagnosticSeverity = DiagnosticSeverity::Warning;
const E: DiagnosticSeverity = DiagnosticSeverity::Error;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn apply(s: &mut Session, m: Vec<Mutation>) -> Result<(), CommandError> {
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
fn fixture(name: &str) -> Session {
    let path = format!(
        "{}/../../tests/fixtures/conventions/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let request: Request = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut session = Session::new(ProjectData::new("Conv".into())).unwrap();
    let envelope = Envelope {
        request_id: "fixture".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session.apply(envelope, request).unwrap();
    session
}
fn project(name: &str) -> ProjectData {
    (*fixture(name).snapshot().project).clone()
}
fn conventions_only(diagnostics: Vec<Diagnostic>) -> Vec<(String, String, String)> {
    let mut found: Vec<_> = diagnostics
        .into_iter()
        .filter(|d| d.code.starts_with("convention"))
        .map(|d| {
            (
                d.code.to_owned(),
                d.element_id.map(|e| e.to_string()).unwrap_or_default(),
                format!("{:?}", d.severity),
            )
        })
        .collect();
    found.sort();
    found
}
fn expect(items: &[(&str, &str, DiagnosticSeverity)]) -> Vec<(String, String, String)> {
    let mut v: Vec<_> = items
        .iter()
        .map(|(c, e, s)| ((*c).to_owned(), (*e).to_owned(), format!("{s:?}")))
        .collect();
    v.sort();
    v
}

/// Each failing fixture yields exactly its expected codes, elements and severities.
#[test]
fn every_rule_has_a_passing_and_a_failing_fixture() {
    let passing = project("pass");
    assert!(passing.conventions.is_some());
    assert_eq!(
        passing.diagnostics(),
        vec![],
        "pass.json has no diagnostics at all"
    );
    let all = passing.conventions.clone().unwrap();
    assert_eq!(all.rules.len(), 8);

    type Expected = Vec<(&'static str, &'static str, DiagnosticSeverity)>;
    let cases: [(&str, Expected); 8] = [
        (
            "fail-label-required",
            vec![("conventionLabelRequired", "notify", E)],
        ),
        (
            "fail-activity-verb-object",
            vec![("conventionActivityVerbObject", "notify", W)],
        ),
        (
            "fail-gateway-flow-label",
            vec![("conventionGatewayFlowLabel", "f_rejected", W)],
        ),
        (
            "fail-no-implicit-split-join",
            vec![
                ("conventionImplicitSplit", "review", W),
                ("conventionImplicitJoin", "end", W),
            ],
        ),
        (
            "fail-single-start-end",
            vec![
                ("conventionStartCount", "conv_process", E),
                ("conventionEndCount", "conv_process", E),
            ],
        ),
        (
            "fail-max-elements",
            vec![("conventionMaxElements", "conv_process", W)],
        ),
        (
            "fail-required-attributes",
            vec![("conventionRequiredAttribute", "notify", W)],
        ),
        (
            "fail-dictionary-terms",
            vec![
                ("conventionDictionaryTerm", "lane_buyer", W),
                ("conventionDictionaryTerm", "order", W),
            ],
        ),
    ];
    for (name, expected) in cases {
        let failing = project(name);
        assert_eq!(
            conventions_only(failing.diagnostics()),
            expect(&expected),
            "{name}"
        );
        // The same project without a profile reports no convention code.
        let mut off = failing.clone();
        off.conventions = None;
        assert!(conventions_only(off.diagnostics()).is_empty(), "{name}");
        // The full profile finds the same problems, except where the limit differs.
        if name != "fail-max-elements" {
            let with_all: Vec<_> = conventions_only(failing.diagnostics_with(&all))
                .into_iter()
                .map(|(code, element, _)| (code, element))
                .collect();
            for (code, element, _) in expect(&expected) {
                assert!(with_all.contains(&(code, element)), "{name}");
            }
        }
    }
}

fn pass_with(profile: ConventionsProfile) -> Vec<(String, String, String)> {
    conventions_only(project("pass").diagnostics_with(&profile))
}

#[test]
fn severity_and_parameters_are_honoured() {
    use ConventionRule::*;
    // Limit: 9 shapes pass at 9 and fail at 8, with the configured severity.
    let max = |limit| MaxElements { severity: E, limit };
    assert!(
        pass_with(ConventionsProfile {
            rules: vec![max(9)]
        })
        .is_empty()
    );
    assert_eq!(
        pass_with(ConventionsProfile {
            rules: vec![max(8)]
        }),
        expect(&[("conventionMaxElements", "conv_process", E)])
    );
    // Kinds: only lanes are required to be labelled; rename a lane to blank.
    let mut p = project("pass");
    let doc = Arc::make_mut(&mut p.documents[0]);
    doc.processes[0].lanes[1].name = Some("  ".into());
    let kinds = |kinds| ConventionsProfile {
        rules: vec![LabelRequired { severity: W, kinds }],
    };
    assert_eq!(
        conventions_only(p.diagnostics_with(&kinds(vec![AttributeTarget::Lane]))),
        expect(&[("conventionLabelRequired", "lane_buyer", W)])
    );
    assert!(conventions_only(p.diagnostics_with(&kinds(vec![AttributeTarget::Event]))).is_empty());
    // Verbs: a custom list replaces the built-in one, case-insensitively.
    let verbs = |verbs: Vec<&str>| ConventionsProfile {
        rules: vec![ActivityVerbObject {
            severity: W,
            verbs: verbs.into_iter().map(str::to_owned).collect(),
        }],
    };
    assert_eq!(
        pass_with(verbs(vec!["REVIEW", "Place"])),
        expect(&[("conventionActivityVerbObject", "notify", W)])
    );
    assert!(pass_with(verbs(vec!["review", "place", "notify"])).is_empty());
    // A one-word label is never verb + object.
    let mut one = project("pass");
    Arc::make_mut(&mut one.documents[0]).processes[0].nodes[1].name = Some("Review".into());
    assert_eq!(
        conventions_only(one.diagnostics_with(&verbs(vec![]))),
        expect(&[("conventionActivityVerbObject", "review", W)])
    );
    // A rule that is absent is off.
    assert!(pass_with(ConventionsProfile { rules: vec![] }).is_empty());
}

/// Rules never fire on conditions they do not cover.
#[test]
fn gateway_default_flow_and_single_flow_gateways_are_not_reported() {
    let mut p = project("pass");
    let doc = Arc::make_mut(&mut p.documents[0]);
    let process = &mut doc.processes[0];
    process
        .flows
        .iter_mut()
        .find(|f| f.id.selector() == id::<ElementSelector>("f_rejected"))
        .unwrap()
        .name = Some(String::new());
    let decision = process
        .nodes
        .iter_mut()
        .find(|n| n.id.selector() == id::<ElementSelector>("decision"))
        .unwrap();
    decision.details = Some(NodeDetails::Gateway {
        default_flow: Some(id("f_rejected")),
        direction: None,
        instantiate: None,
        event_gateway_type: None,
        activation_condition: None,
    });
    let profile = ConventionsProfile {
        rules: vec![ConventionRule::GatewayFlowsLabelled { severity: W }],
    };
    assert!(conventions_only(p.diagnostics_with(&profile)).is_empty());
}

fn task(name: &str, label: Option<&str>, kind: NodeKind) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        id: id::<ElementIdentity>(name),
        name: label.map(str::to_owned),
        documentation: String::new().into(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
fn sequence(name: &str, from: &str, to: &str) -> Flow {
    Flow {
        message_ref: None,
        flow_metadata: None,
        association_direction: None,
        id: id(name),
        kind: FlowKind::Sequence,
        source: Some(EventReference::Local(id(from))),
        target: Some(EventReference::Local(id(to))),
        name: Some(name.into()),
        documentation: String::new().into(),
        base_extras: None,
        condition: None,
    }
}
fn process(nodes: Vec<Node>, flows: Vec<Flow>) -> Process {
    let mut p = project("pass").documents[0].processes[0].clone();
    p.nodes = nodes;
    p.flows = flows;
    p.lanes = vec![];
    p.data_objects = vec![];
    p
}
fn document_with(processes: Vec<Process>) -> BpmnDocument {
    let mut d = BpmnDocument::new("Doc".into(), id("defs"));
    d.processes = processes;
    d
}

#[test]
fn nested_sub_process_scopes_are_checked_on_their_own() {
    let mut sub = task("sub", Some("Handle claim"), NodeKind::SubProcess);
    sub.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            nodes: vec![task("inner_task", Some("Check claim"), NodeKind::Task)],
            ..Default::default()
        }),
    });
    let mut ad_hoc = task("adhoc", Some("Collect notes"), NodeKind::AdHocSubProcess);
    ad_hoc.details = Some(NodeDetails::SubProcess {
        content: Box::new(FlowContainer {
            nodes: vec![task("note", Some("Write note"), NodeKind::Task)],
            ..Default::default()
        }),
    });
    let nodes = vec![
        task("start", Some("Claim received"), NodeKind::StartEvent),
        sub,
        ad_hoc,
        task("end", Some("Claim closed"), NodeKind::EndEvent),
    ];
    let flows = vec![
        sequence("f1", "start", "sub"),
        sequence("f2", "sub", "adhoc"),
        sequence("f3", "adhoc", "end"),
    ];
    let mut project = ProjectData::new("p".into());
    let document = document_with(vec![process(nodes, flows)]);
    let document_id = document.id;
    project.documents.push(Arc::new(document));
    let profile = ConventionsProfile {
        rules: vec![ConventionRule::SingleStartEnd { severity: W }],
    };
    let found = project.diagnostics_with(&profile);
    let found: Vec<_> = found
        .iter()
        .filter(|d| d.code.starts_with("convention"))
        .map(|d| {
            (
                d.code,
                d.element_id.clone().unwrap().to_string(),
                d.document_id,
            )
        })
        .collect();
    // The process scope is fine; the sub-process has neither; the ad-hoc one is skipped.
    assert_eq!(
        found,
        vec![
            ("conventionStartCount", "sub".to_owned(), document_id),
            ("conventionEndCount", "sub".to_owned(), document_id),
        ]
    );
}

#[test]
fn non_editable_documents_are_skipped_and_keep_their_builtin_diagnostics() {
    let mut project = ProjectData::new("p".into());
    let mut document = document_with(vec![process(
        vec![task("only", Some(""), NodeKind::Task)],
        vec![],
    )]);
    document.capability = Capability::PreservedSource {
        source_resource_id: id::<XmlId>("source"),
    };
    project.documents.push(Arc::new(document));
    let found = project.diagnostics_with(&ConventionsProfile::recommended());
    assert!(found.iter().all(|d| !d.code.starts_with("convention")));
    // The superseded built-ins stay because no rule evaluated this document.
    assert!(found.iter().any(|d| d.code == "missingStart"));
    assert!(found.iter().any(|d| d.code == "missingEnd"));
}

#[test]
fn output_is_capped_with_an_omitted_entry() {
    let nodes = (0..MAX_DIAGNOSTICS + 100)
        .map(|i| task(&format!("n{i}"), None, NodeKind::Task))
        .collect();
    let mut project = ProjectData::new("p".into());
    project
        .documents
        .push(Arc::new(document_with(vec![process(nodes, vec![])])));
    let profile = ConventionsProfile {
        rules: vec![ConventionRule::LabelRequired {
            severity: W,
            kinds: vec![AttributeTarget::Activity],
        }],
    };
    let found = conventions::diagnostics(&project, &profile);
    assert_eq!(found.len(), MAX_DIAGNOSTICS + 1);
    let last = found.last().unwrap();
    assert_eq!(last.code, "conventionOutputTruncated");
    assert_eq!(last.element_id, None);
    assert!(last.message.contains("more results omitted"));
    assert!(
        found[..MAX_DIAGNOSTICS]
            .iter()
            .all(|d| d.code == "conventionLabelRequired")
    );
}

/// Fixture that triggers every built-in code next to every rule: no start, no end,
/// an unnamed exclusive gateway, an unconnected task and a pass-through gateway.
fn messy() -> ProjectData {
    let mut session = fixture("pass");
    let doc = session.snapshot().project.documents[0].id;
    let mut mutations = vec![
        Mutation::Delete {
            document_id: doc,
            element_id: id("start"),
        },
        Mutation::Delete {
            document_id: doc,
            element_id: id("end"),
        },
        Mutation::Rename {
            document_id: doc,
            element_id: id("decision"),
            name: Some(String::new()),
        },
        Mutation::CreateElement {
            document_id: doc,
            process_id: id("conv_process"),
            element: task("orphan", Some("Archive file"), NodeKind::Task),
        },
        Mutation::CreateElement {
            document_id: doc,
            process_id: id("conv_process"),
            element: task("relay", Some("Relay"), NodeKind::ParallelGateway),
        },
    ];
    mutations.push(Mutation::CreateFlow {
        document_id: doc,
        process_id: Some(id("conv_process")),
        flow: sequence("f_in", "review", "relay"),
    });
    mutations.push(Mutation::CreateFlow {
        document_id: doc,
        process_id: Some(id("conv_process")),
        flow: sequence("f_out", "relay", "notify"),
    });
    apply(&mut session, mutations).unwrap();
    (*session.snapshot().project).clone()
}

#[test]
fn supersede_table_removes_each_duplicate_condition_exactly_once() {
    let project = messy();
    // Without a profile every built-in fires.
    let mut plain = project.clone();
    plain.conventions = None;
    let builtin: Vec<_> = plain.diagnostics().iter().map(|d| d.code).collect();
    for code in [
        "missingStart",
        "missingEnd",
        "unnamedExclusiveGateway",
        "unconnectedNode",
        "gatewayDoesNotSplitOrMerge",
    ] {
        assert!(
            builtin.contains(&code),
            "{code} is triggered by the fixture"
        );
    }
    // With the full profile the superseded ones disappear and the rule reports instead.
    let found = project.diagnostics();
    for (code, rule) in SUPERSEDED_BUILTINS {
        assert!(
            !found.iter().any(|d| d.code == *code),
            "{code} is superseded by {rule}"
        );
    }
    for code in [
        "conventionStartCount",
        "conventionEndCount",
        "conventionLabelRequired",
        "unconnectedNode",
        "gatewayDoesNotSplitOrMerge",
    ] {
        assert!(found.iter().any(|d| d.code == code), "{code}");
    }
    // No element receives two diagnostics for one condition.
    let mut seen = std::collections::HashSet::new();
    for d in &found {
        assert!(
            seen.insert((d.document_id, d.element_id.clone(), d.code)),
            "{} reported twice",
            d.code
        );
    }
    let pairs = [
        ("missingStart", "conventionStartCount"),
        ("missingEnd", "conventionEndCount"),
        ("unnamedExclusiveGateway", "conventionLabelRequired"),
    ];
    for (builtin, convention) in pairs {
        assert!(
            !(found.iter().any(|d| d.code == builtin)
                && found.iter().any(|d| d.code == convention))
        );
    }
    // labelRequired without gateways does not supersede the gateway warning.
    let mut no_gateway = project.clone();
    no_gateway.conventions = Some(ConventionsProfile {
        rules: vec![ConventionRule::LabelRequired {
            severity: W,
            kinds: vec![AttributeTarget::Activity],
        }],
    });
    assert!(
        no_gateway
            .diagnostics()
            .iter()
            .any(|d| d.code == "unnamedExclusiveGateway")
    );
}

#[test]
fn implicit_split_and_join_look_only_at_activities_and_events() {
    let project = messy();
    let profile = ConventionsProfile {
        rules: vec![ConventionRule::NoImplicitSplitJoin { severity: W }],
    };
    // The two gateways (decision splits, relay passes through) are never reported; the task
    // that gains a second outgoing flow and the one that gains a second incoming flow are.
    assert_eq!(
        conventions_only(project.diagnostics_with(&profile)),
        expect(&[
            ("conventionImplicitJoin", "notify", W),
            ("conventionImplicitSplit", "review", W)
        ])
    );
}

fn session_with_stored(profile: Option<ConventionsProfile>) -> (Session, Result<(), CommandError>) {
    let mut session = Session::new(ProjectData::new("p".into())).unwrap();
    let result = apply(
        &mut session,
        vec![Mutation::SetConventionsProfile { profile }],
    );
    (session, result)
}

#[test]
fn set_profile_validates_bounds_atomically_and_undoes() {
    use ConventionRule::*;
    let bad = |rules| {
        let (session, result) = session_with_stored(Some(ConventionsProfile { rules }));
        assert!(matches!(result, Err(CommandError::InvalidParams(_))));
        assert_eq!(session.snapshot().generation, 0);
        assert!(session.snapshot().project.conventions.is_none());
    };
    bad(vec![MaxElements {
        severity: W,
        limit: 0,
    }]);
    bad(vec![MaxElements {
        severity: W,
        limit: 10_001,
    }]);
    bad(vec![LabelRequired {
        severity: W,
        kinds: vec![],
    }]);
    bad(vec![
        DictionaryTerms { severity: W },
        DictionaryTerms { severity: E },
    ]);
    bad(vec![ActivityVerbObject {
        severity: W,
        verbs: vec!["two words".into()],
    }]);
    bad(vec![ActivityVerbObject {
        severity: W,
        verbs: vec![String::new()],
    }]);
    bad(vec![ActivityVerbObject {
        severity: W,
        verbs: vec!["x".repeat(65)],
    }]);
    bad(vec![ActivityVerbObject {
        severity: W,
        verbs: (0..2_001).map(|i| format!("v{i}")).collect(),
    }]);
    let good = ConventionsProfile {
        rules: vec![
            ActivityVerbObject {
                severity: W,
                verbs: (0..2_000).map(|i| format!("v{i}")).collect(),
            },
            MaxElements {
                severity: E,
                limit: 10_000,
            },
        ],
    };
    let (mut session, result) = session_with_stored(Some(good.clone()));
    result.unwrap();
    assert_eq!(session.snapshot().project.conventions, Some(good));
    // A failing mutation after a good one rolls the whole batch back.
    let failing = apply(
        &mut session,
        vec![
            Mutation::SetConventionsProfile { profile: None },
            Mutation::SetConventionsProfile {
                profile: Some(ConventionsProfile {
                    rules: vec![MaxElements {
                        severity: W,
                        limit: 0,
                    }],
                }),
            },
        ],
    );
    assert!(failing.is_err());
    assert!(session.snapshot().project.conventions.is_some());
    // None switches every rule off, as one undo step.
    apply(
        &mut session,
        vec![Mutation::SetConventionsProfile { profile: None }],
    )
    .unwrap();
    assert!(session.snapshot().project.conventions.is_none());
    session
        .apply(
            Envelope {
                request_id: "u".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            Request::Undo,
        )
        .unwrap();
    assert!(session.snapshot().project.conventions.is_some());
}

#[test]
fn profile_json_is_strict_and_part_of_content_identity() {
    let recommended = ConventionsProfile::recommended();
    let json = serde_json::to_value(&recommended).unwrap();
    assert_eq!(json["rules"][0]["type"], "labelRequired");
    assert_eq!(json["rules"][0]["severity"], "warning");
    assert_eq!(json["rules"][5]["limit"], 50);
    assert_eq!(
        serde_json::from_value::<ConventionsProfile>(json.clone()).unwrap(),
        recommended
    );
    let mut unknown = json.clone();
    unknown["rules"][2]["extra"] = true.into();
    assert!(serde_json::from_value::<ConventionsProfile>(unknown).is_err());
    assert!(
        serde_json::from_str::<ConventionsProfile>(
            r#"{"rules":[{"type":"nonsense","severity":"warning"}]}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<ConventionsProfile>(
            r#"{"rules":[{"type":"singleStartEnd","severity":"fatal"}]}"#
        )
        .is_err()
    );
    // Identity: a profile changes content identity, removing it restores the old one.
    let mut p = ProjectData::new("p".into());
    let before = p.content_identity().unwrap();
    p.conventions = Some(recommended);
    assert_ne!(p.content_identity().unwrap(), before);
    p.conventions = None;
    assert_eq!(p.content_identity().unwrap(), before);
    // A stored profile outside the bounds makes the project invalid.
    p.conventions = Some(ConventionsProfile {
        rules: vec![ConventionRule::MaxElements {
            severity: W,
            limit: 0,
        }],
    });
    assert!(p.validate().is_err());
}

#[test]
fn checkpoint_comparison_reports_a_changed_profile() {
    let mut session = fixture("pass");
    let id: processcraft_core::ids::CheckpointId = id("66666666-6666-4666-8666-666666666666");
    apply(
        &mut session,
        vec![Mutation::CreateCheckpoint {
            id,
            name: "Baseline".into(),
            created_unix_ms: 0,
        }],
    )
    .unwrap();
    let compare = |s: &Session| {
        processcraft_core::queries::compare_checkpoint(&s.snapshot().project, id).unwrap()
    };
    assert!(!compare(&session).conventions_changed);
    apply(
        &mut session,
        vec![Mutation::SetConventionsProfile { profile: None }],
    )
    .unwrap();
    assert!(compare(&session).conventions_changed);
    apply(&mut session, vec![Mutation::RestoreCheckpoint { id }]).unwrap();
    assert!(!compare(&session).conventions_changed);
}
