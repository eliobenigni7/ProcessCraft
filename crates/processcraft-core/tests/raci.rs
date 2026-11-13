//! M5.3 RACI matrix: derivation from lanes and attributes, literals versus
//! entries, rename, activities outside lanes, CSV quoting, deterministic order.
use processcraft_core::{
    attributes::{AttributeValue, AttributeValueInput, RaciAttribute},
    commands::{Envelope, Mutation, Request, Session},
    ids::{DictionaryEntryId, DocumentId, ElementKey, ElementSelector},
    model::*,
    queries::{self, RaciMatrix},
};
use serde_json::json;

const DOC: &str = "11111111-1111-4111-8111-111111111111";
const REQUESTER: &str = "22222222-2222-4222-8222-222222222222";
const PROCUREMENT: &str = "33333333-3333-4333-8333-333333333333";
const FINANCE: &str = "44444444-4444-4444-8444-444444444444";

fn apply(session: &mut Session, request: Request) {
    session
        .apply(
            Envelope {
                request_id: "x".into(),
                session_id: session.id(),
                expected_generation: None,
            },
            request,
        )
        .unwrap();
}
fn batch(session: &mut Session, mutations: Vec<Mutation>) {
    apply(session, Request::Batch { mutations });
}
fn session() -> Session {
    let request: Request =
        serde_json::from_str(include_str!("../../../tests/fixtures/commands/raci.json")).unwrap();
    let mut session = Session::new(ProjectData::new("p".into())).unwrap();
    apply(&mut session, request);
    session
}
fn doc() -> DocumentId {
    DOC.parse().unwrap()
}
fn process() -> ElementSelector {
    "procurement".parse().unwrap()
}
fn matrix(session: &Session) -> RaciMatrix {
    queries::raci_matrix(&session.snapshot().project, doc(), &process()).unwrap()
}
fn entry(id: &str) -> DictionaryTarget {
    DictionaryTarget::Entry(id.parse().unwrap())
}
fn key(xml_id: &str) -> ElementKey {
    ElementKey {
        document_id: doc(),
        xml_id: xml_id.parse().unwrap(),
    }
}
fn set(xml_id: &str, raci: RaciAttribute, refs: Vec<DictionaryTarget>) -> Mutation {
    Mutation::SetAttributeValues {
        source: key(xml_id),
        values: vec![AttributeValueInput {
            definition_id: raci.id(),
            value: Some(AttributeValue::DictionaryRefs(refs)),
        }],
    }
}
/// Column names and each row's letters per column name (`""` when empty).
fn grid(matrix: &RaciMatrix) -> Vec<(String, Vec<String>)> {
    matrix
        .rows
        .iter()
        .map(|row| {
            (
                row.name.clone().unwrap_or_default(),
                matrix
                    .columns
                    .iter()
                    .map(|column| {
                        row.cells
                            .iter()
                            .find(|cell| cell.target == column.target)
                            .map(|cell| cell.letters())
                            .unwrap_or_default()
                    })
                    .collect(),
            )
        })
        .collect()
}
fn row<'a>(matrix: &'a RaciMatrix, name: &str) -> &'a queries::RaciRow {
    matrix
        .rows
        .iter()
        .find(|row| row.name.as_deref() == Some(name))
        .unwrap()
}
fn new_task(id: &str, name: &str) -> Mutation {
    serde_json::from_value(json!({
        "type": "createElement",
        "document_id": DOC,
        "process_id": "procurement",
        "element": {"id": id, "name": name, "documentation": "", "kind": "Task", "owner": "", "contact": ""}
    }))
    .unwrap()
}

#[test]
fn responsible_follows_lanes_and_attributes_fill_a_c_i() {
    let session = session();
    let matrix = matrix(&session);
    let names: Vec<_> = matrix.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["Finance manager", "Procurement", "Requester", "Legal"]
    );
    assert_eq!(
        grid(&matrix),
        [
            (
                "Review request".to_owned(),
                ["C", "A", "R", "I"].map(String::from).to_vec()
            ),
            (
                "Place order".to_owned(),
                ["I", "R,A", "C", ""].map(String::from).to_vec()
            ),
        ]
    );
    // Only activities are rows: no events or gateways.
    assert_eq!(matrix.rows.len(), 2);
}

#[test]
fn literals_follow_entries_and_are_keyed_by_their_text() {
    let mut session = session();
    batch(
        &mut session,
        vec![
            set(
                "review",
                RaciAttribute::Informed,
                vec![
                    DictionaryTarget::Literal("Auditors".into()),
                    DictionaryTarget::Literal("Legal".into()),
                ],
            ),
            set(
                "order",
                RaciAttribute::Informed,
                vec![DictionaryTarget::Literal("Legal".into())],
            ),
        ],
    );
    let matrix = matrix(&session);
    let names: Vec<_> = matrix.columns.iter().map(|c| c.name.as_str()).collect();
    // Entries by name first, then literals by text.
    assert_eq!(
        names,
        [
            "Finance manager",
            "Procurement",
            "Requester",
            "Auditors",
            "Legal"
        ]
    );
    assert_eq!(
        row(&matrix, "Place order").cells.last().unwrap().letters(),
        "I"
    );
}

#[test]
fn rename_keeps_one_column_and_same_name_entries_stay_two() {
    let mut session = session();
    let renamed = DictionaryEntry {
        id: PROCUREMENT.parse().unwrap(),
        kind: DictionaryKind::Role,
        name: "Purchasing".into(),
        description: String::new(),
    };
    let twin = DictionaryEntry {
        id: DictionaryEntryId::new(),
        kind: DictionaryKind::Role,
        name: "Requester".into(),
        description: String::new(),
    };
    batch(
        &mut session,
        vec![
            Mutation::UpdateDictionaryEntry { entry: renamed },
            Mutation::CreateDictionaryEntry {
                entry: twin.clone(),
            },
            // The twin is only Consulted on "Review request"; the lane role stays the original.
            set(
                "review",
                RaciAttribute::Consulted,
                vec![DictionaryTarget::Entry(twin.id)],
            ),
        ],
    );
    let matrix = matrix(&session);
    let names: Vec<_> = matrix.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Finance manager",
            "Purchasing",
            "Requester",
            "Requester",
            "Legal"
        ]
    );
    // The renamed entry is still one column holding both R and A on "Place order".
    assert_eq!(
        row(&matrix, "Place order")
            .cells
            .iter()
            .find(|c| c.target == entry(PROCUREMENT))
            .unwrap()
            .letters(),
        "R,A"
    );
}

#[test]
fn responsible_reads_the_link_target_not_the_label() {
    let mut session = session();
    let twin = DictionaryEntry {
        id: DictionaryEntryId::new(),
        kind: DictionaryKind::Role,
        name: "Requester".into(),
        description: String::new(),
    };
    batch(
        &mut session,
        vec![
            Mutation::CreateDictionaryEntry {
                entry: twin.clone(),
            },
            // "Review request" is Accountable for the same-named twin of its lane role.
            set(
                "review",
                RaciAttribute::Accountable,
                vec![DictionaryTarget::Entry(twin.id)],
            ),
        ],
    );
    let matrix = matrix(&session);
    let review = row(&matrix, "Review request");
    let original = review
        .cells
        .iter()
        .find(|c| c.target == entry(REQUESTER))
        .unwrap();
    let other = review
        .cells
        .iter()
        .find(|c| c.target == DictionaryTarget::Entry(twin.id))
        .unwrap();
    assert_eq!(original.letters(), "R");
    assert_eq!(other.letters(), "A");
    assert_ne!(original.target, other.target);
}

#[test]
fn role_rename_is_reflected_in_the_column_name_only() {
    let mut session = session();
    let before = matrix(&session);
    batch(
        &mut session,
        vec![Mutation::UpdateDictionaryEntry {
            entry: DictionaryEntry {
                id: REQUESTER.parse().unwrap(),
                kind: DictionaryKind::Role,
                name: "Applicant".into(),
                description: String::new(),
            },
        }],
    );
    let after = matrix(&session);
    let applicant = after
        .columns
        .iter()
        .find(|c| c.target == entry(REQUESTER))
        .unwrap();
    assert_eq!(applicant.name, "Applicant");
    // Same targets, same cells; R and A/C/I of one entry share a column.
    let targets = |m: &RaciMatrix| {
        let mut t: Vec<_> = m.columns.iter().map(|c| c.target.clone()).collect();
        t.sort_by_key(|t| format!("{t:?}"));
        t
    };
    assert_eq!(targets(&before), targets(&after));
    assert_eq!(
        row(&after, "Review request")
            .cells
            .iter()
            .find(|c| c.target == entry(REQUESTER))
            .unwrap()
            .letters(),
        "R"
    );
}

#[test]
fn activity_outside_any_lane_has_no_responsible_and_ghost_roles_are_skipped() {
    let mut session = session();
    batch(
        &mut session,
        vec![
            new_task("loose", "Loose task"),
            set("loose", RaciAttribute::Accountable, vec![entry(FINANCE)]),
        ],
    );
    let matrix = matrix(&session);
    let loose = row(&matrix, "Loose task");
    assert!(loose.lane.is_none());
    assert_eq!(loose.cells.len(), 1);
    assert_eq!(loose.cells[0].letters(), "A");
    assert!(!loose.cells[0].responsible);
    // Rows keep the activity order of the table query.
    let order: Vec<_> = matrix
        .rows
        .iter()
        .map(|r| r.name.clone().unwrap())
        .collect();
    assert_eq!(order, ["Review request", "Place order", "Loose task"]);
}

#[test]
fn unknown_process_has_no_matrix() {
    let session = session();
    assert!(
        queries::raci_matrix(&session.snapshot().project, doc(), &"nope".parse().unwrap())
            .is_none()
    );
}

#[test]
fn csv_matches_the_pinned_file_and_is_deterministic() {
    let session = session();
    let csv = queries::raci_csv(&matrix(&session));
    let expected = include_bytes!("../../../tests/fixtures/commands/raci.expected.csv");
    assert_eq!(csv.as_bytes(), expected);
    assert!(csv.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(csv, queries::raci_csv(&matrix(&session)));
}

#[test]
fn csv_quotes_fields_and_neutralises_formulas() {
    let mut session = session();
    let hostile = DictionaryEntry {
        id: DictionaryEntryId::new(),
        kind: DictionaryKind::Role,
        name: "=SUM(A1)".into(),
        description: String::new(),
    };
    batch(
        &mut session,
        vec![
            Mutation::CreateDictionaryEntry {
                entry: hostile.clone(),
            },
            Mutation::Rename {
                document_id: doc(),
                element_id: "review".parse().unwrap(),
                name: Some("Check \"A\", then\nB".into()),
            },
            Mutation::Rename {
                document_id: doc(),
                element_id: "order".parse().unwrap(),
                name: Some("@mention".into()),
            },
            set(
                "order",
                RaciAttribute::Informed,
                vec![DictionaryTarget::Entry(hostile.id)],
            ),
            set(
                "review",
                RaciAttribute::Informed,
                vec![DictionaryTarget::Literal("-1".into())],
            ),
        ],
    );
    let csv = queries::raci_csv(&matrix(&session));
    let lines: Vec<_> = csv.split("\r\n").collect();
    // Header, two rows and the trailing CRLF; the embedded newline stays inside quotes.
    assert!(csv.ends_with("\r\n"));
    assert!(lines[0].contains("'=SUM(A1)"), "{csv}");
    assert!(lines[0].contains(",'-1"), "{csv}");
    assert!(csv.contains("\"Check \"\"A\"\", then\nB\""), "{csv}");
    assert!(csv.contains("\r\n'@mention,"), "{csv}");
}

#[test]
fn csv_neutralises_leading_tab_and_carriage_return() {
    let mut session = session();
    batch(
        &mut session,
        vec![
            set(
                "review",
                RaciAttribute::Informed,
                vec![
                    DictionaryTarget::Literal("\tcmd".into()),
                    DictionaryTarget::Literal("\rcmd".into()),
                ],
            ),
            Mutation::Rename {
                document_id: doc(),
                element_id: "order".parse().unwrap(),
                name: Some("\t=1+1".into()),
            },
        ],
    );
    let csv = queries::raci_csv(&matrix(&session));
    assert!(csv.contains("'\tcmd"), "{csv:?}");
    // A CR-led field is prefixed, then quoted because it contains CR.
    assert!(csv.contains("\"'\rcmd\""), "{csv:?}");
    assert!(csv.contains("\r\n'\t=1+1,"), "{csv:?}");
}

#[test]
fn raci_definitions_have_fixed_ids_and_are_valid_mutations() {
    let ids: Vec<_> = RaciAttribute::ALL.iter().map(|r| r.id()).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    // The fixture pins the same IDs as the code.
    let json = include_str!("../../../tests/fixtures/commands/raci.json");
    for raci in RaciAttribute::ALL {
        assert!(json.contains(&raci.id().to_string()));
        assert_eq!(RaciAttribute::from_id(raci.id()), Some(raci));
    }
    // A fresh project accepts all three through ordinary creation, once.
    let mut session = Session::new(ProjectData::new("p".into())).unwrap();
    batch(
        &mut session,
        RaciAttribute::ALL
            .iter()
            .map(|r| Mutation::CreateAttributeDefinition {
                definition: r.definition(),
            })
            .collect(),
    );
    let project = session.snapshot().project.clone();
    assert_eq!(project.attribute_definitions.len(), 3);
    assert!(
        session
            .apply(
                Envelope {
                    request_id: "y".into(),
                    session_id: session.id(),
                    expected_generation: None,
                },
                Request::Batch {
                    mutations: vec![Mutation::CreateAttributeDefinition {
                        definition: RaciAttribute::Informed.definition()
                    }]
                }
            )
            .is_err()
    );
}
