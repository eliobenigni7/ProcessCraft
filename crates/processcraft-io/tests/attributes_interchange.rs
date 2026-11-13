//! M5.1b: custom attribute values in BPMN interchange (`pc:attributes`).
use processcraft_core::{
    attributes::{
        AttributeAssignment, AttributeDefinition, AttributeTarget, AttributeType, AttributeValue,
        apply_create, apply_set_values,
    },
    commands::{Envelope, Mutation, Request, Session},
    ids::{AttributeDefinitionId, ElementKey, ElementSelector, XmlId},
    model::{DictionaryEntry, DictionaryKind, DictionaryTarget, ProjectData},
};
use processcraft_io::interchange::{
    PC_ATTRIBUTES_NS, export_bpmn, plan_attribute_import, prepare_import,
};
use std::sync::Arc;

const SOURCE: &str = r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" targetNamespace="urn:test" id="defs"><collaboration id="collab"><participant id="part" name="Supplier" processRef="proc"/></collaboration><process id="proc" isExecutable="false"><laneSet id="ls"><lane id="lane" name="Finance"><flowNodeRef>task</flowNodeRef></lane></laneSet><startEvent id="start"/><task id="task" name="Review"/><exclusiveGateway id="gw"/><dataObject id="dobj" name="Invoice"/><dataObjectReference id="dref" dataObjectRef="dobj"/><subProcess id="sub"><task id="inner"/></subProcess><endEvent id="end"/><sequenceFlow id="flow1" sourceRef="start" targetRef="task"/><sequenceFlow id="flow2" sourceRef="task" targetRef="end"/></process></definitions>"#;
const FINANCE_LEAD: &str = "99999999-9999-4999-8999-999999999999";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/bpmn/attributes")
            .join(name),
    )
    .unwrap()
}

fn xml_id(id: &str) -> ElementSelector {
    ElementSelector::Xml(id.parse::<XmlId>().unwrap())
}

fn definition(
    name: &str,
    value_type: AttributeType,
    unit: Option<&str>,
    options: &[&str],
    kind: Option<DictionaryKind>,
) -> AttributeDefinition {
    AttributeDefinition {
        id: AttributeDefinitionId::new(),
        name: name.into(),
        description: String::new(),
        value_type,
        unit: unit.map(str::to_owned),
        options: options.iter().map(|o| (*o).to_owned()).collect(),
        dictionary_kind: kind,
        applies_to: vec![
            AttributeTarget::Activity,
            AttributeTarget::Event,
            AttributeTarget::Gateway,
            AttributeTarget::SequenceFlow,
            AttributeTarget::Lane,
            AttributeTarget::Participant,
            AttributeTarget::Process,
            AttributeTarget::DataObject,
        ],
        required: false,
    }
}

/// A project holding the imported `SOURCE` document and one definition per type.
fn project_from(xml: &str) -> (ProjectData, processcraft_core::ids::DocumentId) {
    let prepared = prepare_import(xml.as_bytes(), "attributes").unwrap();
    assert!(
        prepared.report.editable,
        "{:?}",
        prepared.report.limitations
    );
    let id = prepared.document.id;
    let mut project = ProjectData::new("attributes".into());
    project.documents.push(Arc::new(prepared.document));
    project.resources = prepared.resources;
    (project, id)
}

fn set(
    project: &mut ProjectData,
    document_id: processcraft_core::ids::DocumentId,
    owner: &str,
    definition: &AttributeDefinition,
    value: AttributeValue,
) {
    apply_set_values(
        project,
        ElementKey {
            document_id,
            xml_id: xml_id(owner),
        },
        vec![processcraft_core::attributes::AttributeValueInput {
            definition_id: definition.id,
            value: Some(value),
        }],
    )
    .unwrap_or_else(|e| panic!("{owner}: {e:?}"));
}

struct Full {
    project: ProjectData,
    document: processcraft_core::ids::DocumentId,
    definitions: Vec<AttributeDefinition>,
}

/// All eight types on the task, plus one text value on every other owner kind.
fn full_project() -> Full {
    let (mut project, document) = project_from(SOURCE);
    project.dictionary_entries.push(DictionaryEntry {
        id: FINANCE_LEAD.parse().unwrap(),
        kind: DictionaryKind::Role,
        name: "Finance lead".into(),
        description: String::new(),
    });
    let definitions = vec![
        definition("Notes", AttributeType::Text, None, &[], None),
        definition("Volume", AttributeType::Number, Some("pcs"), &[], None),
        definition("Effort", AttributeType::Duration, None, &[], None),
        definition("Cost", AttributeType::Currency, Some("EUR"), &[], None),
        definition("Risk", AttributeType::Enum, None, &["Low", "High"], None),
        definition("Due", AttributeType::Date, None, &[], None),
        definition(
            "Consulted",
            AttributeType::DictionaryRef,
            None,
            &[],
            Some(DictionaryKind::Role),
        ),
        definition("Spec", AttributeType::Url, None, &[], None),
    ];
    for d in &definitions {
        apply_create(&mut project, d.clone()).unwrap();
    }
    let values = [
        AttributeValue::Text("A & B <c> \"d\"\n  second line ".into()),
        AttributeValue::Number("-12.5".into()),
        AttributeValue::Duration("PT1H30M".into()),
        AttributeValue::Currency("1200.75".into()),
        AttributeValue::Enum("High".into()),
        AttributeValue::Date("2026-11-13".into()),
        AttributeValue::DictionaryRefs(vec![
            DictionaryTarget::Entry(FINANCE_LEAD.parse().unwrap()),
            DictionaryTarget::Literal("External auditor".into()),
        ]),
        AttributeValue::Url("https://example.org/spec?a=1&b=2".into()),
    ];
    for (definition, value) in definitions.iter().zip(values) {
        set(&mut project, document, "task", definition, value);
    }
    for owner in [
        "proc", "part", "lane", "start", "gw", "flow1", "dobj", "dref", "sub", "inner",
    ] {
        set(
            &mut project,
            document,
            owner,
            &definitions[0],
            AttributeValue::Text(format!("note on {owner}")),
        );
    }
    Full {
        project,
        document,
        definitions,
    }
}

fn exported(project: &ProjectData, document: processcraft_core::ids::DocumentId) -> Vec<u8> {
    export_bpmn(project, document, false)
        .unwrap_or_else(|failure| panic!("{:?}", failure.report))
        .bytes
}

/// Applies an import into `project` the way the desktop and CLI do.
fn import_into(
    project: &mut ProjectData,
    bytes: &[u8],
) -> (processcraft_core::ids::DocumentId, Vec<String>) {
    let mut prepared = prepare_import(bytes, "again").unwrap();
    assert!(
        prepared.report.editable,
        "{:?}",
        prepared.report.limitations
    );
    let plan = plan_attribute_import(project, &mut prepared.document, &prepared.attributes);
    let id = prepared.document.id;
    let mut mutations: Vec<_> = prepared
        .resources
        .into_iter()
        .map(|resource| Mutation::AddResource { resource })
        .collect();
    mutations.push(Mutation::CreateDocument {
        document: prepared.document,
    });
    mutations.extend(plan.mutations);
    let mut session = Session::new(project.clone()).unwrap();
    let envelope = Envelope {
        request_id: "import".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    session
        .apply(envelope, Request::Batch { mutations })
        .unwrap_or_else(|e| panic!("{e:?}"));
    *project = (*session.snapshot().project).clone();
    project.validate().unwrap();
    (id, plan.notes)
}

fn values_of(
    project: &ProjectData,
    document: processcraft_core::ids::DocumentId,
) -> Vec<(ElementSelector, String, AttributeValue)> {
    let mut values: Vec<_> = project
        .attribute_values
        .iter()
        .filter(|v| v.source.document_id == document)
        .map(|v| {
            let name = project
                .attribute_definitions
                .iter()
                .find(|d| d.id == v.definition)
                .unwrap()
                .name
                .clone();
            (v.source.xml_id.clone(), name, v.value.clone())
        })
        .collect();
    values.sort_by_key(|(owner, name, _)| (format!("{owner:?}"), name.clone()));
    values
}

#[test]
fn native_to_bpmn_to_native_keeps_every_value_and_all_eight_types() {
    let full = full_project();
    let bytes = exported(&full.project, full.document);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(PC_ATTRIBUTES_NS));
    assert!(text.contains("type=\"dictionaryRef\" dictionaryKind=\"role\""));

    // Re-import into a project that already holds the same definitions and entry:
    // definitions and the dictionary entry bind by ID.
    let mut same = full.project.clone();
    let (again, notes) = import_into(&mut same, &bytes);
    assert!(!notes.is_empty());
    assert_eq!(
        same.attribute_definitions.len(),
        8,
        "no definition is duplicated"
    );
    let expected = values_of(&full.project, full.document);
    assert_eq!(expected.len(), 8 + 10);
    assert_eq!(values_of(&same, again), expected);

    // Into an empty project: definitions are created under their file IDs, every
    // type survives, and the dictionary reference to an unknown entry is a literal.
    let mut empty = ProjectData::new("empty".into());
    let (imported, _) = import_into(&mut empty, &bytes);
    assert_eq!(empty.attribute_definitions.len(), 8);
    for original in &full.definitions {
        let created = empty
            .attribute_definitions
            .iter()
            .find(|d| d.id == original.id)
            .unwrap_or_else(|| panic!("{} missing", original.name));
        assert_eq!(created.value_type, original.value_type);
        assert_eq!(created.unit, original.unit);
        assert_eq!(created.dictionary_kind, original.dictionary_kind);
        assert_eq!(created.name, original.name);
    }
    let imported_values = values_of(&empty, imported);
    assert_eq!(imported_values.len(), expected.len());
    for (owner, name, value) in &imported_values {
        let (_, _, original) = expected
            .iter()
            .find(|(o, n, _)| o == owner && n == name)
            .unwrap();
        if name == "Consulted" {
            assert_eq!(
                value,
                &AttributeValue::DictionaryRefs(vec![
                    DictionaryTarget::Literal("Finance lead".into()),
                    DictionaryTarget::Literal("External auditor".into()),
                ])
            );
        } else {
            assert_eq!(value, original, "{name} on {owner:?}");
        }
    }
    // The imported enum definition carries the options seen in values.
    let risk = empty
        .attribute_definitions
        .iter()
        .find(|d| d.name == "Risk")
        .unwrap();
    assert_eq!(risk.options, vec!["High".to_owned()]);

    // A second export of the imported project writes the same values again.
    let second = String::from_utf8(exported(&empty, imported)).unwrap();
    assert_eq!(second.matches("<pc:attribute ").count(), 18);
}

#[test]
fn pc_attributes_are_the_last_child_after_foreign_extensions_and_survive_in_order() {
    let source = fixture("foreign-and-processcraft.bpmn");
    let prepared = prepare_import(&source, "fixture").unwrap();
    assert!(prepared.report.editable);
    assert_eq!(
        prepared.report.limitations.len(),
        1,
        "{:?}",
        prepared.report.limitations
    );
    assert_eq!(prepared.attributes.definitions.len(), 4);
    assert_eq!(prepared.attributes.values.len(), 4);
    // Lifted values are canonical: 12.50 -> 12.5, PT90M -> PT1H30M.
    let value = |name: &str| {
        let id = prepared
            .attributes
            .definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap()
            .id;
        prepared
            .attributes
            .values
            .iter()
            .find(|v| v.definition == id)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(value("Cost"), AttributeValue::Currency("12.5".into()));
    assert_eq!(value("Effort"), AttributeValue::Duration("PT1H30M".into()));
    assert_eq!(value("Risk"), AttributeValue::Enum("High".into()));
    // Lifted out of the stored XML content, so they are not stored twice.
    let stored = serde_json::to_string(&prepared.document).unwrap();
    assert!(!stored.contains("\"local\":\"attributes\""));
    assert!(stored.contains("taskListener"));

    let mut project = ProjectData::new("fixture".into());
    let (document, notes) = import_into(&mut project, &source);
    assert!(
        notes[0].starts_with("Imported 4 attribute value(s)"),
        "{notes:?}"
    );
    let out = String::from_utf8(exported(&project, document)).unwrap();
    let properties = out.find("<camunda:properties").unwrap();
    let listener = out.find("<camunda:taskListener").unwrap();
    let attributes = out.find("<pc:attributes").unwrap();
    assert!(properties < listener && listener < attributes, "{out}");
    // The foreign nodes are unchanged in the model (same elements, order and attributes).
    let reread = prepare_import(out.as_bytes(), "out").unwrap();
    let foreign = |d: &processcraft_core::model::BpmnDocument| {
        serde_json::to_value(&d.processes[0].nodes[1].base_extras).unwrap()
    };
    assert_eq!(foreign(&reread.document), foreign(&prepared.document));
    assert_eq!(out.matches("<pc:attributes").count(), 1);
    // And a further import reads the same values back.
    let mut again = ProjectData::new("again".into());
    let (second, _) = import_into(&mut again, out.as_bytes());
    assert_eq!(values_of(&again, second).len(), 4);
}

#[test]
fn malformed_pc_attributes_stay_extension_content_and_are_named() {
    let source = fixture("foreign-and-processcraft-malformed.bpmn");
    let prepared = prepare_import(&source, "malformed").unwrap();
    assert!(prepared.report.editable);
    assert!(prepared.attributes.definitions.is_empty());
    assert!(prepared.attributes.values.is_empty());
    assert!(
        prepared
            .report
            .limitations
            .iter()
            .any(|l| l.contains("pc:attributes on <review>") && l.contains("unknown type bogus")),
        "{:?}",
        prepared.report.limitations
    );
    let stored = serde_json::to_string(&prepared.document).unwrap();
    assert!(stored.contains("\"local\":\"attributes\"") && stored.contains("bogus"));
    // Preserved on export, in order after the foreign content, and still not interpreted.
    let mut project = ProjectData::new("malformed".into());
    project.documents.push(Arc::new(prepared.document));
    project.resources = prepared.resources;
    let id = project.documents[0].id;
    let out = String::from_utf8(exported(&project, id)).unwrap();
    assert!(out.find("<camunda:taskListener").unwrap() < out.find("<pc:attributes").unwrap());
    assert!(out.contains("type=\"bogus\""));
    let again = prepare_import(out.as_bytes(), "again").unwrap();
    assert!(again.attributes.values.is_empty());
}

/// One lifted/imported variation of the fixture's task attribute.
fn with_attribute(attribute: &str) -> String {
    format!(
        r#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:pc="{PC_ATTRIBUTES_NS}" targetNamespace="urn:test" id="defs"><process id="p" isExecutable="false"><task id="t"><extensionElements>{attribute}</extensionElements></task></process></definitions>"#
    )
}

#[test]
fn invalid_attribute_elements_are_never_lifted() {
    let id = "11111111-1111-4111-8111-111111111111";
    for (label, attribute) in [
        (
            "decimal",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="number">1e3</pc:attribute></pc:attributes>"#),
        ),
        (
            "currency unit",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="currency" unit="eur">1</pc:attribute></pc:attributes>"#),
        ),
        (
            "unexpected child",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="text"><x/></pc:attribute></pc:attributes>"#),
        ),
        (
            "unexpected element",
            r#"<pc:attributes><pc:other/></pc:attributes>"#.to_owned(),
        ),
        (
            "bad definition id",
            r#"<pc:attributes><pc:attribute definition="nope" name="N" type="text">x</pc:attribute></pc:attributes>"#.to_owned(),
        ),
        (
            "bad date",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="date">2026-02-30</pc:attribute></pc:attributes>"#),
        ),
        (
            "unknown attribute",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="text" x="1">x</pc:attribute></pc:attributes>"#),
        ),
        (
            "same definition twice",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="text">a</pc:attribute><pc:attribute definition="{id}" name="N" type="text">b</pc:attribute></pc:attributes>"#),
        ),
        (
            "inconsistent redeclaration",
            format!(r#"<pc:attributes><pc:attribute definition="{id}" name="N" type="text">a</pc:attribute></pc:attributes><pc:attributes><pc:attribute definition="{id}" name="Other" type="text">b</pc:attribute></pc:attributes>"#),
        ),
    ] {
        let prepared = prepare_import(with_attribute(&attribute).as_bytes(), label).unwrap();
        assert!(prepared.report.editable, "{label}: {:?}", prepared.report.limitations);
        // `inconsistent redeclaration` lifts its first element and keeps the second.
        let lifted = prepared.attributes.values.len();
        assert_eq!(lifted, usize::from(label == "inconsistent redeclaration"), "{label}");
        assert!(
            prepared.report.limitations.iter().any(|l| l.contains("pc:attributes on <t>")),
            "{label}: {:?}",
            prepared.report.limitations
        );
        let stored = serde_json::to_string(&prepared.document).unwrap();
        assert!(stored.contains("\"local\":\"attributes\""), "{label}");
    }
}

#[test]
fn import_bounds_definitions_and_whitespace() {
    // Pretty-printed whitespace between elements is accepted; a value is exact text.
    let id = "11111111-1111-4111-8111-111111111111";
    let pretty = with_attribute(&format!(
        "\n<pc:attributes>\n  <pc:attribute definition=\"{id}\" name=\"N\" type=\"text\"> keep </pc:attribute>\n</pc:attributes>\n"
    ));
    let prepared = prepare_import(pretty.as_bytes(), "pretty").unwrap();
    assert_eq!(
        prepared.attributes.values[0].value,
        AttributeValue::Text(" keep ".into())
    );
    // More definitions than the limit: nothing past the limit is lifted.
    let mut many = String::from("<pc:attributes>");
    for n in 0..201 {
        many.push_str(&format!(
            "<pc:attribute definition=\"00000000-0000-4000-8000-{n:012}\" name=\"A{n}\" type=\"text\">x</pc:attribute>"
        ));
    }
    many.push_str("</pc:attributes>");
    let prepared = prepare_import(with_attribute(&many).as_bytes(), "many").unwrap();
    assert!(prepared.attributes.values.is_empty());
    assert!(
        prepared
            .report
            .limitations
            .iter()
            .any(|l| l.contains("too many"))
    );
}

#[test]
fn colliding_ids_and_names_and_inapplicable_targets_never_fail_the_import() {
    let id = "11111111-1111-4111-8111-111111111111";
    let source = with_attribute(&format!(
        r#"<pc:attributes><pc:attribute definition="{id}" name="Cost" type="currency" unit="EUR">5</pc:attribute></pc:attributes>"#
    ));
    // Same ID, other type: a fresh definition under a suffixed name.
    let mut project = ProjectData::new("p".into());
    let existing = AttributeDefinition {
        id: id.parse().unwrap(),
        ..definition("Cost", AttributeType::Text, None, &[], None)
    };
    apply_create(&mut project, existing.clone()).unwrap();
    let (document, notes) = import_into(&mut project, source.as_bytes());
    assert_eq!(project.attribute_definitions.len(), 2);
    let created = &project.attribute_definitions[1];
    assert_eq!(created.name, "Cost (imported)");
    assert_ne!(created.id, existing.id);
    assert_eq!(created.value_type, AttributeType::Currency);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("imported as \"Cost (imported)\""))
    );
    assert_eq!(values_of(&project, document).len(), 1);

    // Same name, other ID: also suffixed, so names stay unique.
    let mut project = ProjectData::new("p".into());
    apply_create(
        &mut project,
        definition("Cost", AttributeType::Currency, Some("EUR"), &[], None),
    )
    .unwrap();
    import_into(&mut project, source.as_bytes());
    assert_eq!(
        project
            .attribute_definitions
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["Cost", "Cost (imported)"]
    );

    // The same ID with a compatible shape binds to the existing definition;
    // when it does not apply to the element the value is skipped and reported,
    // and the definition is not widened.
    let mut project = ProjectData::new("p".into());
    let mut narrow = AttributeDefinition {
        id: id.parse().unwrap(),
        ..definition("Cost", AttributeType::Currency, Some("EUR"), &[], None)
    };
    narrow.applies_to = vec![AttributeTarget::Gateway];
    apply_create(&mut project, narrow).unwrap();
    let (document, notes) = import_into(&mut project, source.as_bytes());
    assert_eq!(project.attribute_definitions.len(), 1);
    assert_eq!(
        project.attribute_definitions[0].applies_to,
        vec![AttributeTarget::Gateway]
    );
    assert!(values_of(&project, document).is_empty());
    assert!(
        notes.iter().any(|n| n.contains("does not apply")),
        "{notes:?}"
    );

    // An enum value outside the existing options is skipped, not fatal.
    let enum_id = "22222222-2222-4222-8222-222222222222";
    let enum_source = with_attribute(&format!(
        r#"<pc:attributes><pc:attribute definition="{enum_id}" name="Risk" type="enum">Critical</pc:attribute></pc:attributes>"#
    ));
    let mut project = ProjectData::new("p".into());
    apply_create(
        &mut project,
        AttributeDefinition {
            id: enum_id.parse().unwrap(),
            ..definition("Risk", AttributeType::Enum, None, &["Low", "High"], None)
        },
    )
    .unwrap();
    let (document, notes) = import_into(&mut project, enum_source.as_bytes());
    assert!(values_of(&project, document).is_empty());
    assert!(
        notes.iter().any(|n| n.contains("does not fit")),
        "{notes:?}"
    );
}

#[test]
fn dictionary_entry_of_another_kind_becomes_a_literal() {
    let id = "11111111-1111-4111-8111-111111111111";
    let source = with_attribute(&format!(
        r#"<pc:attributes><pc:attribute definition="{id}" name="Systems" type="dictionaryRef" dictionaryKind="system"><pc:ref entry="{FINANCE_LEAD}" name="ERP"/></pc:attribute></pc:attributes>"#
    ));
    let mut project = ProjectData::new("p".into());
    project.dictionary_entries.push(DictionaryEntry {
        id: FINANCE_LEAD.parse().unwrap(),
        kind: DictionaryKind::Role,
        name: "Finance lead".into(),
        description: String::new(),
    });
    let (document, _) = import_into(&mut project, source.as_bytes());
    assert_eq!(
        values_of(&project, document)[0].2,
        AttributeValue::DictionaryRefs(vec![DictionaryTarget::Literal("ERP".into())])
    );
}

#[test]
fn a_value_on_a_missing_owner_blocks_the_export_instead_of_being_dropped() {
    let mut full = full_project();
    full.project.attribute_values.push(AttributeAssignment {
        source: ElementKey {
            document_id: full.document,
            xml_id: xml_id("does-not-exist"),
        },
        definition: full.definitions[0].id,
        value: AttributeValue::Text("orphan".into()),
    });
    let failure = export_bpmn(&full.project, full.document, false).unwrap_err();
    assert!(!failure.report.blocked.is_empty());
}

#[test]
fn export_report_says_what_is_written_and_what_is_not() {
    let full = full_project();
    let exported = export_bpmn(&full.project, full.document, false).unwrap();
    assert!(
        exported
            .report
            .transformed
            .iter()
            .any(|l| l == "Custom attribute values written to the ProcessCraft extension namespace")
    );
    assert!(
        exported
            .report
            .omitted
            .iter()
            .any(|l| l.contains("Attribute definition metadata"))
    );
    // Values no longer need the native-field acknowledgement.
    assert!(export_bpmn(&full.project, full.document, false).is_ok());
    // A project without definitions reports nothing about attributes.
    let (plain, id) = project_from(SOURCE);
    let report = export_bpmn(&plain, id, false).unwrap().report;
    assert!(report.omitted.iter().all(|l| !l.contains("Attribute")));
    assert!(report.transformed.iter().all(|l| !l.contains("attribute")));
}

#[test]
fn values_the_project_cannot_take_stay_in_the_document_and_are_written_back() {
    let id = "11111111-1111-4111-8111-111111111111";
    let source = with_attribute(&format!(
        r#"<pc:attributes><pc:attribute definition="{id}" name="Cost" type="currency" unit="EUR">5</pc:attribute></pc:attributes>"#
    ));
    let mut project = ProjectData::new("p".into());
    let mut narrow = AttributeDefinition {
        id: id.parse().unwrap(),
        ..definition("Cost", AttributeType::Currency, Some("EUR"), &[], None)
    };
    narrow.applies_to = vec![AttributeTarget::Gateway];
    apply_create(&mut project, narrow).unwrap();
    let (document, notes) = import_into(&mut project, source.as_bytes());
    assert!(values_of(&project, document).is_empty());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("\"Cost\" on t not imported")
                && n.contains("kept as extension content")),
        "{notes:?}"
    );
    // Not lost: the element is extension content again and export writes it back.
    let out = String::from_utf8(exported(&project, document)).unwrap();
    assert!(
        out.contains("<pc:attribute ") && out.contains(">5</pc:attribute>"),
        "{out}"
    );
    assert_eq!(out.matches("<pc:attributes").count(), 1);
}

#[test]
fn set_attribute_values_is_refused_on_preserved_source_documents() {
    // Unknown attribute forces preserved source (see `unknown_attributes_force_exact_preservation`).
    let xml = SOURCE.replace("id=\"task\"", "id=\"task\" arbitrary=\"x\"");
    let prepared = prepare_import(xml.as_bytes(), "preserved").unwrap();
    assert!(!prepared.report.editable);
    let document = prepared.document.id;
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(prepared.document));
    project.resources = prepared.resources;
    let def = definition("Notes", AttributeType::Text, None, &[], None);
    apply_create(&mut project, def.clone()).unwrap();
    let mut session = Session::new(project).unwrap();
    let envelope = Envelope {
        request_id: "set".into(),
        session_id: session.id(),
        expected_generation: None,
    };
    let result = session.apply(
        envelope,
        Request::Batch {
            mutations: vec![Mutation::SetAttributeValues {
                source: ElementKey {
                    document_id: document,
                    xml_id: xml_id("task"),
                },
                values: vec![processcraft_core::attributes::AttributeValueInput {
                    definition_id: def.id,
                    value: Some(AttributeValue::Text("x".into())),
                }],
            }],
        },
    );
    assert!(result.is_err());
}
