//! M5.1a custom attributes: validators, mutations, pruning, resolutions,
//! checkpoints, queries and limits.
use processcraft_core::{
    attributes::{
        self, AttributeAssignment, AttributeDefinition, AttributeDeleteResolution,
        AttributeOptionMapping, AttributeTarget, AttributeType, AttributeUpdateResolution,
        AttributeValue, AttributeValueInput,
    },
    commands::{CommandError, DictionaryDeleteResolution, Envelope, Mutation, Request, Session},
    ids::{
        AttributeDefinitionId, CheckpointId, DictionaryEntryId, DocumentId, ElementKey,
        ElementSelector, XmlId,
    },
    model::*,
    queries,
};
use std::sync::Arc;

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}
fn docid() -> DocumentId {
    id("11111111-1111-4111-8111-111111111111")
}
fn entry_id() -> DictionaryEntryId {
    id("33333333-3333-4333-8333-333333333333")
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
fn apply_one(s: &mut Session, m: Mutation) -> Result<(), CommandError> {
    apply(s, vec![m])
}

/// A process with one element of every attribute target except participant:
/// activity, event, gateway, sequence flow, lane, process, data object.
fn document() -> BpmnDocument {
    let mut d = BpmnDocument::new("Doc".into(), id("defs"));
    d.id = docid();
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        lane_set_name: None,
        groups: vec![],
        data_objects: vec![processcraft_core::bpmn_data::DataObject {
            flow_metadata: None,
            base: None,
            id: Some(id("dataobj")),
            name: Some("Invoice".into()),
            is_collection: None,
            item_aware: None,
        }],
        data_associations: vec![],
        id: id("process"),
        name: Some("Process".into()),
        documentation: String::new().into(),
        nodes: vec![
            node("activity", "Review request", NodeKind::Task),
            node("event", "Received", NodeKind::StartEvent),
            node("gateway", "Approved?", NodeKind::ExclusiveGateway),
            node("dataref", "Invoice", NodeKind::DataObjectReference),
        ],
        lanes: vec![Lane {
            base: None,
            child_lane_set_id: None,
            child_lane_set_base: None,
            child_lane_set_name: None,
            partition_element: None,
            partition_element_ref: None,
            id: id("lane"),
            name: Some("Clerk".into()),
            parent: None,
            members: vec![id("activity")],
        }],
        flows: vec![Flow {
            message_ref: None,
            flow_metadata: None,
            association_direction: None,
            id: id("flow"),
            kind: FlowKind::Sequence,
            source: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "event",
            ))),
            target: Some(processcraft_core::bpmn_model::EventReference::Local(id(
                "activity",
            ))),
            name: None,
            documentation: String::new().into(),
            base_extras: None,
            condition: None,
        }],
    });
    d
}
fn node(name: &str, label: &str, kind: NodeKind) -> Node {
    Node {
        flow_metadata: None,
        base_extras: None,
        item_aware: None,
        details: None,
        loop_characteristics: None,
        io_specification: None,
        activity: None,
        id: id(name),
        name: Some(label.into()),
        documentation: String::new().into(),
        kind,
        owner: String::new(),
        contact: String::new(),
    }
}
fn key(xml_id: &str) -> ElementKey {
    ElementKey {
        document_id: docid(),
        xml_id: id(xml_id),
    }
}
fn session() -> Session {
    let mut p = ProjectData::new("p".into());
    p.documents.push(Arc::new(document()));
    Session::new(p).unwrap()
}
fn definition(name: &str, value_type: AttributeType) -> AttributeDefinition {
    definition_with(name, value_type, &[AttributeTarget::Activity])
}
fn definition_with(
    name: &str,
    value_type: AttributeType,
    applies_to: &[AttributeTarget],
) -> AttributeDefinition {
    let mut options = Vec::new();
    if value_type == AttributeType::Enum {
        options = vec!["Low".into(), "Medium".into(), "High".into()];
    }
    AttributeDefinition {
        id: AttributeDefinitionId::new(),
        name: name.into(),
        description: String::new(),
        value_type,
        unit: matches!(value_type, AttributeType::Number | AttributeType::Currency)
            .then(|| "EUR".into()),
        options,
        dictionary_kind: (value_type == AttributeType::DictionaryRef)
            .then_some(DictionaryKind::Role),
        applies_to: applies_to.to_vec(),
        required: false,
    }
}
fn create(definition: AttributeDefinition) -> Mutation {
    Mutation::CreateAttributeDefinition { definition }
}
fn set(source: &str, definition_id: AttributeDefinitionId, value: AttributeValue) -> Mutation {
    Mutation::SetAttributeValues {
        source: key(source),
        values: vec![AttributeValueInput {
            definition_id,
            value: Some(value),
        }],
    }
}

#[test]
fn decimal_validator_accepts_rejects_and_canonicalizes() {
    for (input, canonical) in [
        ("1.50", "1.5"),
        ("+1", "1"),
        ("-0", "0"),
        ("-0.5", "-0.5"),
        (".5", "0.5"),
        ("1.", "1"),
        ("007", "7"),
        ("0", "0"),
        ("12", "12"),
    ] {
        assert_eq!(attributes::parse_decimal(input).unwrap(), canonical);
    }
    for input in ["", "abc", "1e3", "1 2", "1.2.3", "--1", "1,5", "٣"] {
        assert!(
            attributes::parse_decimal(input).is_err(),
            "accepted {input:?}"
        );
    }
}

#[test]
fn duration_validator_accepts_the_iso_subset_and_canonicalizes() {
    for (input, canonical) in [
        ("P1D", "P1D"),
        ("p1d", "P1D"),
        ("PT2H30M", "PT2H30M"),
        ("pt2h30m", "PT2H30M"),
        ("P1DT2H", "P1DT2H"),
        ("P1DT0H", "P1D"),
        ("PT0S", "PT0S"),
        ("P0D", "PT0S"),
        ("PT90S", "PT1M30S"),
        ("PT90M", "PT1H30M"),
        ("PT24H", "P1D"),
        ("P1DT25H", "P2DT1H"),
        ("P1DT1H1M1S", "P1DT1H1M1S"),
    ] {
        assert_eq!(attributes::parse_duration(input).unwrap(), canonical);
    }
    for input in [
        "P",
        "PT",
        "P1Y",
        "P1M",
        "P1W",
        "T1H",
        "P1DT",
        "P-1D",
        "P1.5D",
        "P9999999999D",
    ] {
        assert!(
            attributes::parse_duration(input).is_err(),
            "accepted {input:?}"
        );
    }
}

#[test]
fn date_validator_accepts_real_calendar_dates_only() {
    for input in ["2026-11-13", "2024-02-29", "2000-02-29", "0000-01-01"] {
        assert_eq!(attributes::parse_date(input).unwrap(), input);
    }
    for input in [
        "2023-02-29",
        "2100-02-29",
        "2026-13-01",
        "2026-00-10",
        "2026-11-00",
        "2026-11-31",
        "2026-4-01",
        "26-01-01",
        "2026-11-13T00:00",
    ] {
        assert!(attributes::parse_date(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn target_of_maps_every_element_family() {
    let document = document();
    let expected = [
        ("activity", AttributeTarget::Activity),
        ("event", AttributeTarget::Event),
        ("gateway", AttributeTarget::Gateway),
        ("flow", AttributeTarget::SequenceFlow),
        ("lane", AttributeTarget::Lane),
        ("process", AttributeTarget::Process),
        ("dataobj", AttributeTarget::DataObject),
        ("dataref", AttributeTarget::DataObject),
    ];
    for (selector, target) in expected {
        let selector: ElementSelector = id(selector);
        assert_eq!(
            attributes::target_of(&document, &selector),
            Some(target),
            "selector {selector}"
        );
    }
    assert_eq!(
        attributes::target_of(&document, &id::<ElementSelector>("missing")),
        None
    );
}

#[test]
fn all_eight_types_set_clear_and_round_trip() {
    let mut s = session();
    let mut definitions = Vec::new();
    for (name, value_type) in [
        ("Note", AttributeType::Text),
        ("Amount", AttributeType::Number),
        ("Elapsed", AttributeType::Duration),
        ("Cost", AttributeType::Currency),
        ("Risk", AttributeType::Enum),
        ("Deadline", AttributeType::Date),
        ("Roles", AttributeType::DictionaryRef),
        ("Policy", AttributeType::Url),
    ] {
        let mut definition = definition(name, value_type);
        definition.applies_to = vec![
            AttributeTarget::Activity,
            AttributeTarget::Event,
            AttributeTarget::Gateway,
        ];
        apply_one(&mut s, create(definition.clone())).unwrap();
        definitions.push(definition);
    }
    let entry = DictionaryEntry {
        id: entry_id(),
        kind: DictionaryKind::Role,
        name: "Finance lead".into(),
        description: String::new(),
    };
    apply_one(&mut s, Mutation::CreateDictionaryEntry { entry }).unwrap();
    let values = [
        AttributeValue::Text("Pending audit".into()),
        AttributeValue::Number("0.5".into()),
        AttributeValue::Duration("PT2H30M".into()),
        AttributeValue::Currency("12.50".into()),
        AttributeValue::Enum("Medium".into()),
        AttributeValue::Date("2026-11-13".into()),
        AttributeValue::DictionaryRefs(vec![DictionaryTarget::Entry(entry_id())]),
        AttributeValue::Url("https://example.com/policy".into()),
    ];
    for (definition, value) in definitions.iter().zip(values) {
        apply_one(&mut s, set("activity", definition.id, value.clone())).unwrap();
    }
    // Canonical forms are stored, not the raw input.
    assert_eq!(
        s.snapshot().project.attribute_values[1].value,
        AttributeValue::Number("0.5".into())
    );
    s.snapshot().project.validate().unwrap();
    let before = s.snapshot().content_id;
    let roundtrip: ProjectData =
        serde_json::from_slice(&serde_json::to_vec(&s.snapshot().project).unwrap()).unwrap();
    assert_eq!(
        roundtrip.content_identity().unwrap(),
        s.snapshot().content_id
    );
    assert_eq!(roundtrip.content_identity().unwrap(), before);
    // Clearing one value removes exactly that assignment.
    apply_one(
        &mut s,
        Mutation::SetAttributeValues {
            source: key("activity"),
            values: vec![AttributeValueInput {
                definition_id: definitions[1].id,
                value: None,
            }],
        },
    )
    .unwrap();
    let values_now = &s.snapshot().project.attribute_values;
    assert_eq!(values_now.len(), 7);
    assert!(!values_now.iter().any(|v| v.definition == definitions[1].id));
    s.snapshot().project.validate().unwrap();
}

#[test]
fn applies_to_is_enforced_when_values_are_set_but_not_by_validate() {
    let mut s = session();
    let mut on_activities = definition("Note", AttributeType::Text);
    on_activities.applies_to = vec![AttributeTarget::Activity];
    let definition_id = on_activities.id;
    apply_one(&mut s, create(on_activities.clone())).unwrap();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    // Not an invariant of validate(): a value whose definition no longer
    // applies (here written directly, as an older or imported file can) stays
    // loadable, visible, renamable and clearable.
    let mut stale = s.snapshot().project.as_ref().clone();
    stale.attribute_definitions[0].applies_to = vec![AttributeTarget::Process];
    stale.validate().unwrap();
    assert_eq!(stale.attribute_values.len(), 1);
    let mut stale_session = Session::new(stale).unwrap();
    let mut renamed = on_activities.clone();
    renamed.name = "Renamed".into();
    renamed.applies_to = vec![AttributeTarget::Process];
    apply_one(
        &mut stale_session,
        Mutation::UpdateAttributeDefinition {
            definition: renamed,
            resolution: AttributeUpdateResolution::Reject,
        },
    )
    .unwrap();
    assert_eq!(
        queries::element_attributes(&stale_session.snapshot().project, docid())
            .unwrap()
            .len(),
        1
    );
    apply_one(
        &mut stale_session,
        Mutation::SetAttributeValues {
            source: key("activity"),
            values: vec![AttributeValueInput {
                definition_id,
                value: None,
            }],
        },
    )
    .unwrap();
    assert!(stale_session.snapshot().project.attribute_values.is_empty());
    // A narrowing update made now is refused, or cleared on request.
    let mut narrowed = on_activities.clone();
    narrowed.applies_to = vec![AttributeTarget::Process];
    assert!(
        apply_one(
            &mut s,
            Mutation::UpdateAttributeDefinition {
                definition: narrowed.clone(),
                resolution: AttributeUpdateResolution::Reject,
            },
        )
        .is_err()
    );
    apply_one(
        &mut s,
        Mutation::UpdateAttributeDefinition {
            definition: narrowed,
            resolution: AttributeUpdateResolution::Clear,
        },
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    // Setting a new value on a disallowed element is refused.
    let error = apply_one(
        &mut s,
        set("event", definition_id, AttributeValue::Text("a".into())),
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::InvalidParams(_)));
    // A wide definition keeps accepting values for every target it lists.
    let mut s2 = session();
    let wide = definition_with(
        "Note",
        AttributeType::Text,
        &[AttributeTarget::Activity, AttributeTarget::Event],
    );
    let wide_id = wide.id;
    apply_one(&mut s2, create(wide)).unwrap();
    apply_one(
        &mut s2,
        set("activity", wide_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    apply_one(
        &mut s2,
        set("event", wide_id, AttributeValue::Text("b".into())),
    )
    .unwrap();
    s2.snapshot().project.validate().unwrap();
}

#[test]
fn updates_allow_renames_and_resolve_restricted_changes() {
    let mut s = session();
    let mut definition = definition("Risk", AttributeType::Enum);
    definition.applies_to = vec![AttributeTarget::Activity];
    let definition_id = definition.id;
    apply_one(&mut s, create(definition.clone())).unwrap();
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::Enum("Low".into()),
        ),
    )
    .unwrap();
    // Rename, description, required and unit always succeed.
    let mut renamed = definition.clone();
    renamed.name = "Severity".into();
    renamed.description = "Renamed".into();
    renamed.required = true;
    apply_one(
        &mut s,
        Mutation::UpdateAttributeDefinition {
            definition: renamed,
            resolution: AttributeUpdateResolution::Reject,
        },
    )
    .unwrap();
    // value_type is immutable.
    let mut wrong_type = definition.clone();
    wrong_type.name = "Severity".into();
    wrong_type.value_type = AttributeType::Text;
    assert!(
        apply_one(
            &mut s,
            Mutation::UpdateAttributeDefinition {
                definition: wrong_type,
                resolution: AttributeUpdateResolution::Reject,
            }
        )
        .is_err()
    );
    // Removing an option that is in use is refused without a resolution.
    let mut dropped = definition.clone();
    dropped.name = "Severity".into();
    dropped.options.retain(|option| option != "Low");
    assert!(
        apply_one(
            &mut s,
            Mutation::UpdateAttributeDefinition {
                definition: dropped.clone(),
                resolution: AttributeUpdateResolution::Reject,
            }
        )
        .is_err()
    );
    // Clear drops the affected value.
    apply_one(
        &mut s,
        Mutation::UpdateAttributeDefinition {
            definition: dropped.clone(),
            resolution: AttributeUpdateResolution::Clear,
        },
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    // MapOptions retargets the value instead.
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::Enum("Medium".into()),
        ),
    )
    .unwrap();
    let mut mapped = definition.clone();
    mapped.name = "Severity".into();
    mapped.options = vec!["High".into()];
    apply_one(
        &mut s,
        Mutation::UpdateAttributeDefinition {
            definition: mapped.clone(),
            resolution: AttributeUpdateResolution::MapOptions(vec![AttributeOptionMapping {
                from: "Medium".into(),
                to: "High".into(),
            }]),
        },
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].value,
        AttributeValue::Enum("High".into())
    );
    // A mapping to an option that does not exist is refused.
    assert!(
        apply_one(
            &mut s,
            Mutation::UpdateAttributeDefinition {
                definition: mapped.clone(),
                resolution: AttributeUpdateResolution::MapOptions(vec![AttributeOptionMapping {
                    from: "High".into(),
                    to: "Extreme".into(),
                }]),
            }
        )
        .is_err()
    );
    // Duplicate names are refused.
    let duplicate = definition_with("Severity", AttributeType::Text, &[]);
    assert!(apply_one(&mut s, create(duplicate)).is_err());
}

#[test]
fn delete_resolutions_refuse_discard_and_move_values() {
    // Deleting a used definition fails without a resolution.
    let mut s = session();
    let used = definition_with("Note", AttributeType::Text, &[AttributeTarget::Activity]);
    let definition_id = used.id;
    apply_one(&mut s, create(used)).unwrap();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    let error = apply_one(
        &mut s,
        Mutation::DeleteAttributeDefinition {
            definition_id,
            resolution: AttributeDeleteResolution::Reject,
        },
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::InvalidParams(_)));
    assert!(
        s.snapshot()
            .project
            .attribute_definitions
            .iter()
            .any(|d| d.id == definition_id)
    );
    // Discard drops the values with the definition.
    apply_one(
        &mut s,
        Mutation::DeleteAttributeDefinition {
            definition_id,
            resolution: AttributeDeleteResolution::Discard,
        },
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    // MoveTo requires the same type.
    let source = definition_with(
        "Note",
        AttributeType::Text,
        &[AttributeTarget::Activity, AttributeTarget::Event],
    );
    let source_id = source.id;
    let target = definition_with(
        "Comment",
        AttributeType::Text,
        &[AttributeTarget::Activity, AttributeTarget::Event],
    );
    apply_one(&mut s, create(source)).unwrap();
    apply_one(&mut s, create(target.clone())).unwrap();
    apply_one(
        &mut s,
        set("activity", source_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    apply_one(
        &mut s,
        Mutation::DeleteAttributeDefinition {
            definition_id: source_id,
            resolution: AttributeDeleteResolution::MoveTo(target.id),
        },
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].definition,
        target.id
    );
    let wrong_target = definition("Risk", AttributeType::Enum);
    let _ = &wrong_target;
    apply_one(&mut s, create(wrong_target.clone())).unwrap();
    assert!(
        apply_one(
            &mut s,
            Mutation::DeleteAttributeDefinition {
                definition_id: target.id,
                resolution: AttributeDeleteResolution::MoveTo(wrong_target.id),
            }
        )
        .is_err()
    );
    // A source that already has a value on the target refuses the move
    // instead of silently dropping one of the two values.
    let other_source = definition_with(
        "Extra",
        AttributeType::Text,
        &[AttributeTarget::Activity, AttributeTarget::Event],
    );
    let other_source_id = other_source.id;
    apply_one(&mut s, create(other_source)).unwrap();
    apply_one(
        &mut s,
        set("event", other_source_id, AttributeValue::Text("b".into())),
    )
    .unwrap();
    apply_one(
        &mut s,
        set("event", target.id, AttributeValue::Text("t".into())),
    )
    .unwrap();
    assert!(
        apply_one(
            &mut s,
            Mutation::DeleteAttributeDefinition {
                definition_id: other_source_id,
                resolution: AttributeDeleteResolution::MoveTo(target.id),
            },
        )
        .is_err()
    );
    let on_event = s
        .snapshot()
        .project
        .attribute_values
        .iter()
        .filter(|value| value.source.xml_id == id::<XmlId>("event"))
        .count();
    assert_eq!(on_event, 2);
}

#[test]
fn failed_batch_leaves_state_history_and_generation_unchanged() {
    let mut s = session();
    let definition = definition_with("Note", AttributeType::Text, &[AttributeTarget::Activity]);
    let before = s.snapshot();
    let error = apply(
        &mut s,
        vec![
            create(definition.clone()),
            set("activity", definition.id, AttributeValue::Text("a".into())),
            set("gateway", definition.id, AttributeValue::Text("a".into())),
        ],
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::InvalidParams(_)));
    let after = s.snapshot();
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.content_id, after.content_id);
    assert_eq!(
        before.project.attribute_definitions.len(),
        after.project.attribute_definitions.len()
    );
    assert_eq!(
        before.project.attribute_values.len(),
        after.project.attribute_values.len()
    );
    // Nothing was pushed onto the history either.
    let undo = s.apply(
        Envelope {
            request_id: "u".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Undo,
    );
    assert!(matches!(undo, Err(CommandError::NothingToUndo)));
}

#[test]
fn attribute_changes_undo_and_redo() {
    let mut s = session();
    let definition = definition_with("Note", AttributeType::Text, &[AttributeTarget::Activity]);
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    assert_eq!(s.snapshot().project.attribute_values.len(), 1);
    s.apply(
        Envelope {
            request_id: "u".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Undo,
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    assert!(
        s.snapshot()
            .project
            .attribute_definitions
            .iter()
            .any(|d| d.id == definition_id)
    );
    s.apply(
        Envelope {
            request_id: "r".into(),
            session_id: s.id(),
            expected_generation: None,
        },
        Request::Redo,
    )
    .unwrap();
    assert_eq!(s.snapshot().project.attribute_values.len(), 1);
}

#[test]
fn values_are_pruned_with_elements_and_documents_and_remapped_by_xml_id() {
    let mut s = session();
    let definition = definition_with("Note", AttributeType::Text, &[AttributeTarget::Activity]);
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("a".into())),
    )
    .unwrap();
    // setElementXmlId remaps the value source, exactly like dictionary links.
    apply_one(
        &mut s,
        Mutation::SetElementXmlId {
            document_id: docid(),
            element_id: id("activity"),
            xml_id: Some(id::<XmlId>("activity2")),
        },
    )
    .unwrap();
    let remapped = s.snapshot().project.attribute_values[0]
        .source
        .xml_id
        .clone();
    assert!(remapped.is_native());
    s.snapshot().project.validate().unwrap();
    // Deleting the element prunes its value.
    apply_one(
        &mut s,
        Mutation::Delete {
            document_id: docid(),
            element_id: remapped,
        },
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    s.snapshot().project.validate().unwrap();
    // Values of a deleted document go with it.
    let definition2 = definition_with("Note2", AttributeType::Text, &[AttributeTarget::Event]);
    let definition2_id = definition2.id;
    apply_one(&mut s, create(definition2)).unwrap();
    apply_one(
        &mut s,
        set("event", definition2_id, AttributeValue::Text("b".into())),
    )
    .unwrap();
    assert_eq!(s.snapshot().project.attribute_values.len(), 1);
    apply_one(
        &mut s,
        Mutation::DeleteDocument {
            document_id: docid(),
        },
    )
    .unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    s.snapshot().project.validate().unwrap();
}

#[test]
fn dictionary_entry_delete_resolutions_cover_attribute_values() {
    let entry = DictionaryEntry {
        id: entry_id(),
        kind: DictionaryKind::Role,
        name: "Finance lead".into(),
        description: String::new(),
    };
    let replacement = DictionaryEntry {
        id: id("44444444-4444-4444-8444-444444444444"),
        kind: DictionaryKind::Role,
        name: "Controller".into(),
        description: String::new(),
    };
    // Reject counts attribute usages.
    let mut s = session();
    let mut definition = definition("Roles", AttributeType::DictionaryRef);
    definition.applies_to = vec![AttributeTarget::Activity];
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        Mutation::CreateDictionaryEntry {
            entry: entry.clone(),
        },
    )
    .unwrap();
    apply_one(
        &mut s,
        Mutation::CreateDictionaryEntry {
            entry: replacement.clone(),
        },
    )
    .unwrap();
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::DictionaryRefs(vec![DictionaryTarget::Entry(entry_id())]),
        ),
    )
    .unwrap();
    let error = apply_one(
        &mut s,
        Mutation::DeleteDictionaryEntry {
            entry_id: entry_id(),
            resolution: DictionaryDeleteResolution::Reject,
        },
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::InvalidParams(_)));
    assert!(
        s.snapshot()
            .project
            .dictionary_entries
            .iter()
            .any(|e| e.id == entry_id())
    );
    // dictionary_usages includes the attribute source.
    let usages = queries::dictionary_usages(&s.snapshot().project, entry_id());
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].xml_id, id::<XmlId>("activity"));
    // Replace retargets the value.
    apply_one(
        &mut s,
        Mutation::DeleteDictionaryEntry {
            entry_id: entry_id(),
            resolution: DictionaryDeleteResolution::Replace(id(
                "44444444-4444-4444-8444-444444444444",
            )),
        },
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].value,
        AttributeValue::DictionaryRefs(vec![DictionaryTarget::Entry(replacement.id)])
    );
    s.snapshot().project.validate().unwrap();
    // Unlink turns the reference into a literal with the entry name.
    apply_one(
        &mut s,
        Mutation::CreateDictionaryEntry {
            entry: entry.clone(),
        },
    )
    .unwrap();
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::DictionaryRefs(vec![DictionaryTarget::Entry(replacement.id)]),
        ),
    )
    .unwrap();
    apply_one(
        &mut s,
        Mutation::DeleteDictionaryEntry {
            entry_id: replacement.id,
            resolution: DictionaryDeleteResolution::Unlink,
        },
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].value,
        AttributeValue::DictionaryRefs(vec![DictionaryTarget::Literal("Controller".into())])
    );
    s.snapshot().project.validate().unwrap();
}

#[test]
fn checkpoints_capture_restore_and_compare_attributes() {
    let mut s = session();
    let mut definition = definition_with(
        "Cost",
        AttributeType::Currency,
        &[AttributeTarget::Activity],
    );
    definition.unit = Some("EUR".into());
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    let checkpoint_id: CheckpointId = id("55555555-5555-4555-8555-555555555555");
    apply_one(
        &mut s,
        Mutation::CreateCheckpoint {
            id: checkpoint_id,
            name: "Baseline".into(),
            created_unix_ms: 0,
        },
    )
    .unwrap();
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::Currency("12.50".into()),
        ),
    )
    .unwrap();
    let comparison = queries::compare_checkpoint(&s.snapshot().project, checkpoint_id).unwrap();
    assert!(comparison.attributes_changed);
    let change = comparison
        .changed_properties
        .iter()
        .find(|change| change.key == key("activity"))
        .expect("changed attribute reported on the element");
    assert_eq!(change.properties[0].property, "attribute:Cost");
    assert_eq!(change.properties[0].before, serde_json::json!(null));
    assert_eq!(
        change.properties[0].after,
        serde_json::json!({"Currency": "12.5"})
    );
    // Restoring the checkpoint removes the values again.
    apply_one(&mut s, Mutation::RestoreCheckpoint { id: checkpoint_id }).unwrap();
    assert!(s.snapshot().project.attribute_values.is_empty());
    let comparison = queries::compare_checkpoint(&s.snapshot().project, checkpoint_id).unwrap();
    assert!(!comparison.attributes_changed);
    // A definition rename alone marks the checkpoint comparison.
    let mut renamed = s
        .snapshot()
        .project
        .attribute_definitions
        .iter()
        .find(|d| d.id == definition_id)
        .unwrap()
        .clone();
    renamed.name = "Price".into();
    apply_one(
        &mut s,
        Mutation::UpdateAttributeDefinition {
            definition: renamed,
            resolution: AttributeUpdateResolution::Reject,
        },
    )
    .unwrap();
    let comparison = queries::compare_checkpoint(&s.snapshot().project, checkpoint_id).unwrap();
    assert!(comparison.attributes_changed);
}

#[test]
fn attribute_queries_return_definitions_rows_and_table_cells() {
    let mut s = session();
    let mut definition = definition_with(
        "Cost",
        AttributeType::Currency,
        &[AttributeTarget::Activity],
    );
    definition.unit = Some("EUR".into());
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::Currency("12.50".into()),
        ),
    )
    .unwrap();
    let definitions = queries::attribute_definitions(&s.snapshot().project);
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].usage_count, 1);
    assert_eq!(definitions[0].definition.name, "Cost");
    let rows = queries::element_attributes(&s.snapshot().project, docid()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target, Some(AttributeTarget::Activity));
    assert_eq!(rows[0].name.as_deref(), Some("Review request"));
    assert_eq!(rows[0].values[0].name, "Cost");
    assert_eq!(
        rows[0].values[0].value,
        AttributeValue::Currency("12.5".into())
    );
    let table = queries::activity_table(&s.snapshot().project, docid(), &id("process")).unwrap();
    let row = table
        .iter()
        .find(|row| row.key.xml_id == id::<XmlId>("activity"))
        .unwrap();
    assert_eq!(row.attributes.len(), 1);
    assert_eq!(
        row.attributes[0].value,
        AttributeValue::Currency("12.5".into())
    );
    assert!(
        table
            .iter()
            .filter(|row| row.key.xml_id != id::<XmlId>("activity"))
            .all(|row| row.attributes.is_empty())
    );
}

#[test]
fn attribute_limits_are_enforced() {
    let mut s = session();
    // 200 definitions fit, the 201st is refused.
    for i in 0..attributes::MAX_DEFINITIONS {
        apply_one(
            &mut s,
            create(definition(&format!("D{i}"), AttributeType::Text)),
        )
        .unwrap();
    }
    assert!(apply_one(&mut s, create(definition("Over", AttributeType::Text))).is_err());
    let mut fresh = session();
    // 64 enum options fit, 65 do not.
    let mut options = definition("Risk", AttributeType::Enum);
    options.applies_to = vec![AttributeTarget::Activity];
    options.options = (0..attributes::MAX_ENUM_OPTIONS + 1)
        .map(|i| format!("O{i}"))
        .collect();
    assert!(apply_one(&mut fresh, create(options)).is_err());
    // 16 dictionary references fit, 17 do not.
    let mut entries = Vec::new();
    for i in 0..attributes::MAX_DICTIONARY_REFS + 1 {
        entries.push(DictionaryEntry {
            id: id(&format!("66666666-6666-4666-8666-{:012}", i)),
            kind: DictionaryKind::Role,
            name: format!("Role {i}"),
            description: String::new(),
        });
    }
    for entry in entries {
        apply_one(&mut fresh, Mutation::CreateDictionaryEntry { entry }).unwrap();
    }
    let mut refs = definition("Roles", AttributeType::DictionaryRef);
    refs.applies_to = vec![AttributeTarget::Activity];
    let refs_id = refs.id;
    apply_one(&mut fresh, create(refs)).unwrap();
    let many: Vec<DictionaryTarget> = (0..attributes::MAX_DICTIONARY_REFS + 1)
        .map(|i| DictionaryTarget::Entry(id(&format!("66666666-6666-4666-8666-{:012}", i))))
        .collect();
    assert!(
        apply_one(
            &mut fresh,
            set("activity", refs_id, AttributeValue::DictionaryRefs(many))
        )
        .is_err()
    );
    // Text beyond the short() limit is refused.
    let mut long = definition("Note", AttributeType::Text);
    long.applies_to = vec![AttributeTarget::Activity];
    let long_id = long.id;
    apply_one(&mut fresh, create(long)).unwrap();
    let text = "x".repeat(attributes::MAX_TEXT + 1);
    assert!(
        apply_one(
            &mut fresh,
            set("activity", long_id, AttributeValue::Text(text))
        )
        .is_err()
    );
    // At most one value per (source, definition).
    apply_one(
        &mut fresh,
        set("activity", long_id, AttributeValue::Text("first".into())),
    )
    .unwrap();
    apply_one(
        &mut fresh,
        set("activity", long_id, AttributeValue::Text("second".into())),
    )
    .unwrap();
    assert_eq!(fresh.snapshot().project.attribute_values.len(), 1);
    fresh.snapshot().project.validate().unwrap();
}

#[test]
fn starter_definitions_are_valid_and_have_expected_shapes() {
    let mut s = session();
    for definition in attributes::starter_definitions() {
        apply_one(&mut s, create(definition.clone())).unwrap();
    }
    let snapshot = s.snapshot();
    let names: Vec<_> = snapshot
        .project
        .attribute_definitions
        .iter()
        .map(|d| (d.name.as_str(), d.value_type))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Duration", AttributeType::Duration),
            ("Cost", AttributeType::Currency),
            ("SLA", AttributeType::Duration),
            ("Risk", AttributeType::Enum),
            ("Control", AttributeType::Text),
            ("Process owner", AttributeType::DictionaryRef),
        ]
    );
    let owner = snapshot
        .project
        .attribute_definitions
        .iter()
        .find(|d| d.name == "Process owner")
        .unwrap();
    assert_eq!(owner.dictionary_kind, Some(DictionaryKind::Role));
    assert_eq!(owner.applies_to, vec![AttributeTarget::Process]);
    let first = attributes::starter_definitions();
    let second = attributes::starter_definitions();
    assert_ne!(first[0].id, second[0].id);
    assert_eq!(first.len(), second.len());
    s.snapshot().project.validate().unwrap();
}

#[test]
fn definition_shape_violations_are_refused() {
    let mut s = session();
    // unit on a non-numeric type
    let mut unit = definition("Note", AttributeType::Text);
    unit.unit = Some("EUR".into());
    assert!(apply_one(&mut s, create(unit)).is_err());
    // options on a non-enum type
    let mut options = definition("Note", AttributeType::Text);
    options.options = vec!["A".into()];
    assert!(apply_one(&mut s, create(options)).is_err());
    // enum without options
    let mut empty = definition("Note", AttributeType::Enum);
    empty.options = vec![];
    assert!(apply_one(&mut s, create(empty)).is_err());
    // dictionary kind on a non-DictionaryRef type
    let mut kind = definition("Note", AttributeType::Text);
    kind.dictionary_kind = Some(DictionaryKind::Role);
    assert!(apply_one(&mut s, create(kind)).is_err());
    // DictionaryRef without a kind
    let mut no_kind = definition("Note", AttributeType::DictionaryRef);
    no_kind.dictionary_kind = None;
    assert!(apply_one(&mut s, create(no_kind)).is_err());
    // duplicate applies_to entries
    let mut duplicate = definition("Note", AttributeType::Text);
    duplicate.applies_to = vec![AttributeTarget::Activity, AttributeTarget::Activity];
    assert!(apply_one(&mut s, create(duplicate)).is_err());
    // duplicate options
    let mut duplicate_options = definition("Note", AttributeType::Enum);
    duplicate_options.options = vec!["A".into(), "A".into()];
    assert!(apply_one(&mut s, create(duplicate_options)).is_err());
    // value of the wrong variant for the definition type
    let mut definition = definition_with("Note", AttributeType::Text, &[AttributeTarget::Activity]);
    definition.applies_to = vec![AttributeTarget::Activity];
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    assert!(
        apply_one(
            &mut s,
            set(
                "activity",
                definition_id,
                AttributeValue::Number("1".into())
            )
        )
        .is_err()
    );
    // enum value outside the options
    let mut risk = definition_with("Risk", AttributeType::Enum, &[AttributeTarget::Activity]);
    risk.applies_to = vec![AttributeTarget::Activity];
    let risk_id = risk.id;
    apply_one(&mut s, create(risk)).unwrap();
    assert!(
        apply_one(
            &mut s,
            set("activity", risk_id, AttributeValue::Enum("Extreme".into()))
        )
        .is_err()
    );
    // dictionary entry of the wrong kind
    let entry = DictionaryEntry {
        id: entry_id(),
        kind: DictionaryKind::System,
        name: "ERP".into(),
        description: String::new(),
    };
    apply_one(&mut s, Mutation::CreateDictionaryEntry { entry }).unwrap();
    let mut refs = definition_with(
        "Roles",
        AttributeType::DictionaryRef,
        &[AttributeTarget::Activity],
    );
    refs.applies_to = vec![AttributeTarget::Activity];
    let refs_id = refs.id;
    apply_one(&mut s, create(refs)).unwrap();
    assert!(
        apply_one(
            &mut s,
            set(
                "activity",
                refs_id,
                AttributeValue::DictionaryRefs(vec![DictionaryTarget::Entry(entry_id())])
            )
        )
        .is_err()
    );
}

#[test]
fn assignment_struct_serializes_stably() {
    let assignment = AttributeAssignment {
        source: key("activity"),
        definition: id("77777777-7777-4777-8777-777777777777"),
        value: AttributeValue::Number("1.5".into()),
    };
    let value = serde_json::to_value(&assignment).unwrap();
    assert_eq!(
        value["source"]["document_id"],
        serde_json::json!(docid().to_string())
    );
    assert_eq!(value["source"]["xml_id"], "activity");
    assert_eq!(value["definition"], "77777777-7777-4777-8777-777777777777");
    assert_eq!(value["value"], serde_json::json!({"Number": "1.5"}));
}

fn every_target(name: &str, value_type: AttributeType) -> AttributeDefinition {
    definition_with(
        name,
        value_type,
        &[
            AttributeTarget::Activity,
            AttributeTarget::Event,
            AttributeTarget::Gateway,
            AttributeTarget::SequenceFlow,
            AttributeTarget::Lane,
            AttributeTarget::Participant,
            AttributeTarget::Process,
            AttributeTarget::DataObject,
        ],
    )
}
fn document_with_subprocess() -> BpmnDocument {
    let mut d = document();
    let mut sub = node("sub", "Sub", NodeKind::SubProcess);
    let mut flow = d.processes[0].flows[0].clone();
    flow.id = id("inner_flow");
    flow.source = Some(processcraft_core::bpmn_model::EventReference::Local(id(
        "inner_a",
    )));
    flow.target = Some(processcraft_core::bpmn_model::EventReference::Local(id(
        "inner_b",
    )));
    let content = FlowContainer {
        nodes: vec![
            node("inner_a", "Inner A", NodeKind::Task),
            node("inner_b", "Inner B", NodeKind::Task),
        ],
        flows: vec![flow],
        ..FlowContainer::default()
    };
    sub.details = Some(processcraft_core::bpmn_model::NodeDetails::SubProcess {
        content: Box::new(content),
    });
    d.processes[0].nodes.push(sub);
    d
}

#[test]
fn every_element_class_deletes_with_its_values() {
    // `Delete` covers flow nodes, flows and lanes; data objects have their own
    // mutations. Each one prunes the value instead of failing validation.
    for element in ["activity", "event", "gateway", "flow", "lane", "dataref"] {
        let mut s = session();
        let definition = every_target("Note", AttributeType::Text);
        let definition_id = definition.id;
        apply_one(&mut s, create(definition)).unwrap();
        apply_one(
            &mut s,
            set(element, definition_id, AttributeValue::Text("x".into())),
        )
        .unwrap();
        apply_one(
            &mut s,
            Mutation::Delete {
                document_id: docid(),
                element_id: id(element),
            },
        )
        .unwrap_or_else(|error| panic!("delete {element}: {error:?}"));
        assert!(
            s.snapshot().project.attribute_values.is_empty(),
            "{element}"
        );
    }
    for mutation in [
        Mutation::DeleteDataObject {
            document_id: docid(),
            scope_id: id("process"),
            data_object_id: id("dataobj"),
        },
        Mutation::SetScopeDataObjects {
            document_id: docid(),
            scope_id: id("process"),
            data_objects: vec![],
        },
    ] {
        let mut s = session();
        let definition = every_target("Note", AttributeType::Text);
        let definition_id = definition.id;
        apply_one(&mut s, create(definition)).unwrap();
        apply_one(
            &mut s,
            set("dataobj", definition_id, AttributeValue::Text("x".into())),
        )
        .unwrap();
        apply_one(&mut s, mutation).unwrap();
        assert!(s.snapshot().project.attribute_values.is_empty());
        s.snapshot().project.validate().unwrap();
    }
}

#[test]
fn sequence_flows_inside_subprocesses_are_targets() {
    let document = document_with_subprocess();
    assert_eq!(
        attributes::target_of(&document, &id("inner_flow")),
        Some(AttributeTarget::SequenceFlow)
    );
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(document));
    let mut s = Session::new(project).unwrap();
    let definition = every_target("Note", AttributeType::Text);
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        set(
            "inner_flow",
            definition_id,
            AttributeValue::Text("x".into()),
        ),
    )
    .unwrap();
}

#[test]
fn resetting_a_value_keeps_identity_history_and_order() {
    let mut s = session();
    let definition = every_target("Note", AttributeType::Text);
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("x".into())),
    )
    .unwrap();
    apply_one(
        &mut s,
        set("event", definition_id, AttributeValue::Text("y".into())),
    )
    .unwrap();
    let checkpoint: CheckpointId = id("55555555-5555-4555-8555-555555555555");
    apply_one(
        &mut s,
        Mutation::CreateCheckpoint {
            id: checkpoint,
            name: "c".into(),
            created_unix_ms: 0,
        },
    )
    .unwrap();
    let before = s.snapshot();
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("x".into())),
    )
    .unwrap();
    let after = s.snapshot();
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.content_id, after.content_id);
    let comparison = queries::compare_checkpoint(&after.project, checkpoint).unwrap();
    assert!(!comparison.attributes_changed);
    // A changed value stays in place.
    apply_one(
        &mut s,
        set("activity", definition_id, AttributeValue::Text("z".into())),
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].source,
        key("activity")
    );
}

#[test]
fn dictionary_references_stay_unique() {
    let mut s = session();
    let ids: Vec<DictionaryEntryId> = (0..3)
        .map(|i| id(&format!("77777777-7777-4777-8777-{i:012}")))
        .collect();
    for (i, entry) in ids.iter().enumerate() {
        apply_one(
            &mut s,
            Mutation::CreateDictionaryEntry {
                entry: DictionaryEntry {
                    id: *entry,
                    kind: DictionaryKind::Role,
                    name: format!("R{i}"),
                    description: String::new(),
                },
            },
        )
        .unwrap();
    }
    let definition = definition("Roles", AttributeType::DictionaryRef);
    let definition_id = definition.id;
    apply_one(&mut s, create(definition)).unwrap();
    let entry = |i: usize| DictionaryTarget::Entry(ids[i]);
    assert!(
        apply_one(
            &mut s,
            set(
                "activity",
                definition_id,
                AttributeValue::DictionaryRefs(vec![entry(0), entry(0)])
            )
        )
        .is_err()
    );
    apply_one(
        &mut s,
        set(
            "activity",
            definition_id,
            AttributeValue::DictionaryRefs(vec![entry(0), entry(1), entry(2)]),
        ),
    )
    .unwrap();
    // Retargeting A onto C merges the non-adjacent duplicate.
    apply_one(
        &mut s,
        Mutation::DeleteDictionaryEntry {
            entry_id: ids[0],
            resolution: DictionaryDeleteResolution::Replace(ids[2]),
        },
    )
    .unwrap();
    assert_eq!(
        s.snapshot().project.attribute_values[0].value,
        AttributeValue::DictionaryRefs(vec![entry(2), entry(1)])
    );
}

#[test]
fn move_to_keeps_applies_to_units_and_existing_values() {
    let mut s = session();
    let source = definition("Source", AttributeType::Text);
    let events_only = definition_with("Events", AttributeType::Text, &[AttributeTarget::Event]);
    let (source_id, events_id) = (source.id, events_only.id);
    apply_one(&mut s, create(source)).unwrap();
    apply_one(&mut s, create(events_only)).unwrap();
    apply_one(
        &mut s,
        set("activity", source_id, AttributeValue::Text("x".into())),
    )
    .unwrap();
    assert!(
        apply_one(
            &mut s,
            Mutation::DeleteAttributeDefinition {
                definition_id: source_id,
                resolution: AttributeDeleteResolution::MoveTo(events_id),
            },
        )
        .is_err()
    );
    let euro = definition("Cost", AttributeType::Currency);
    let mut dollar = definition("Cost USD", AttributeType::Currency);
    dollar.unit = Some("USD".into());
    let (euro_id, dollar_id) = (euro.id, dollar.id);
    apply_one(&mut s, create(euro)).unwrap();
    apply_one(&mut s, create(dollar)).unwrap();
    apply_one(
        &mut s,
        set("activity", euro_id, AttributeValue::Currency("5".into())),
    )
    .unwrap();
    assert!(
        apply_one(
            &mut s,
            Mutation::DeleteAttributeDefinition {
                definition_id: euro_id,
                resolution: AttributeDeleteResolution::MoveTo(dollar_id),
            },
        )
        .is_err()
    );
}

#[test]
fn definition_text_fields_are_bounded_and_trimmed() {
    let mut s = session();
    let mut long_unit = definition("Amount", AttributeType::Number);
    long_unit.unit = Some("u".repeat(attributes::MAX_TEXT + 1));
    assert!(apply_one(&mut s, create(long_unit)).is_err());
    for unit in [None, Some("eur"), Some("EURO"), Some("")] {
        let mut cost = definition("Cost", AttributeType::Currency);
        cost.unit = unit.map(str::to_owned);
        assert!(apply_one(&mut s, create(cost)).is_err(), "{unit:?}");
    }
    for name in ["   ", " Cost", "Cost "] {
        assert!(
            apply_one(&mut s, create(definition(name, AttributeType::Text))).is_err(),
            "{name:?}"
        );
    }
    let mut nowhere = definition("Nowhere", AttributeType::Text);
    nowhere.applies_to = vec![];
    assert!(apply_one(&mut s, create(nowhere)).is_err());
    // An option mapping must start from a current option.
    let risk = definition("Risk", AttributeType::Enum);
    apply_one(&mut s, create(risk.clone())).unwrap();
    apply_one(
        &mut s,
        set("activity", risk.id, AttributeValue::Enum("Low".into())),
    )
    .unwrap();
    let mut renamed = risk.clone();
    renamed.options = vec!["Lo".into(), "Medium".into(), "High".into()];
    assert!(
        apply_one(
            &mut s,
            Mutation::UpdateAttributeDefinition {
                definition: renamed,
                resolution: AttributeUpdateResolution::MapOptions(vec![
                    AttributeOptionMapping {
                        from: "Low".into(),
                        to: "Lo".into(),
                    },
                    AttributeOptionMapping {
                        from: "Unknown".into(),
                        to: "High".into(),
                    },
                ]),
            },
        )
        .is_err()
    );
}
