use processcraft_core::{
    ids::{ElementIdentity, ElementSelector},
    model::{BpmnDocument, ProjectData},
};
use processcraft_io::interchange_compare::xml_model_value;
use serde_json::json;
use std::sync::Arc;

fn document(named: bool) -> BpmnDocument {
    let identity = |id: &str| {
        if named {
            id.parse::<ElementIdentity>().unwrap()
        } else {
            ElementIdentity::anonymous()
        }
    };
    let first = identity("First");
    let second = identity("Second");
    let mut document = BpmnDocument::new("Workspace name".into(), identity("Definitions"));
    document.processes.push(serde_json::from_value(json!({
        "id":"Process","name":null,"documentation":"",
        "nodes":[
            {"id":first,"name":"Same","documentation":"Left","kind":"Task","owner":"","contact":""},
            {"id":second,"name":"Same","documentation":"Right","kind":"Task","owner":"","contact":""}
        ],
        "lanes":[],
        "flows":[{"id":"Flow","kind":"Sequence","source":first.selector(),"target":second.selector(),
            "name":null,"documentation":"","condition":null}]
    })).unwrap());
    document
}

#[test]
fn reimport_local_handles_normalize_by_exact_owner_path_without_mutation() {
    let first = document(false);
    let second = document(false);
    let original = serde_json::to_value(&first).unwrap();
    assert_ne!(
        first.processes[0].nodes[0].id,
        second.processes[0].nodes[0].id
    );
    let projection = xml_model_value(&first).unwrap();
    assert_eq!(projection, xml_model_value(&second).unwrap());
    assert_eq!(
        projection["processes"][0]["flows"][0]["source"],
        json!({"nativeTargetAt":"/processes/0/nodes/0/id"})
    );
    assert_eq!(
        projection["processes"][0]["flows"][0]["target"],
        json!({"nativeTargetAt":"/processes/0/nodes/1/id"})
    );
    assert_eq!(serde_json::to_value(&first).unwrap(), original);
}

#[test]
fn sibling_owner_swaps_and_reference_retargeting_are_observable() {
    let original = document(false);
    let mut swapped = original.clone();
    swapped.processes[0].nodes.swap(0, 1);
    assert_ne!(
        xml_model_value(&original).unwrap(),
        xml_model_value(&swapped).unwrap()
    );
    let mut retargeted = original.clone();
    retargeted.processes[0].flows[0].source = retargeted.processes[0].nodes[1].id.selector();
    assert_ne!(
        xml_model_value(&original).unwrap(),
        xml_model_value(&retargeted).unwrap()
    );
}

#[test]
fn assigned_native_and_legacy_ids_compare_by_real_xml_id() {
    let legacy = document(true);
    let mut native = legacy.clone();
    let first = ElementIdentity::Native {
        native_id: processcraft_core::ids::NativeElementId::new(),
        xml_id: Some("First".parse().unwrap()),
    };
    native.processes[0].nodes[0].id = first.clone();
    native.processes[0].flows[0].source = first.selector();
    assert_eq!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&native).unwrap()
    );
    if let ElementIdentity::Native { xml_id, .. } = &mut native.processes[0].nodes[0].id {
        *xml_id = Some("Changed".parse().unwrap());
    }
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&native).unwrap()
    );
}

#[test]
fn optional_id_absence_normalizes_but_native_identity_and_payload_stay_exact() {
    let mut missing = document(true);
    missing.processes[0].nodes[0].documentation = serde_json::from_value(json!([
        {"id":null,"text_format":null,"content":"{\"native_id\":\"literal payload\",\"xml_id\":null}"}
    ])).unwrap();
    let mut addressed = missing.clone();
    let processcraft_core::bpmn_metadata::DocumentationSet::Structured(docs) =
        &mut addressed.processes[0].nodes[0].documentation
    else {
        unreachable!()
    };
    docs[0].id = Some(ElementIdentity::anonymous());
    assert_ne!(
        serde_json::to_value(&missing).unwrap(),
        serde_json::to_value(&addressed).unwrap()
    );
    assert_eq!(
        xml_model_value(&missing).unwrap(),
        xml_model_value(&addressed).unwrap()
    );
    let mut project = ProjectData::new("Native must remain exact".into());
    project.documents.push(Arc::new(addressed));
    project.validate().unwrap();
    let identity = project.content_identity().unwrap();
    let archive = processcraft_io::native::write_bytes(&project).unwrap();
    assert_eq!(
        processcraft_io::native::load_bytes(&archive)
            .unwrap()
            .project
            .content_identity()
            .unwrap(),
        identity
    );
}

#[test]
fn qname_namespace_and_uuid_looking_text_are_not_rewritten() {
    let mut first = document(false);
    let text = first.processes[0].nodes[0].id.to_string();
    first.processes[0].nodes[0].documentation = text.clone().into();
    first.catalog = serde_json::from_value(json!({"item_definitions":[{
        "id":null,"structure_ref":{"namespace":"urn:external","local":"First"},
        "item_kind":null,"is_collection":null
    }]}))
    .unwrap();
    let projection = xml_model_value(&first).unwrap();
    assert_eq!(
        projection["processes"][0]["nodes"][0]["documentation"],
        text
    );
    assert_eq!(
        projection["catalog"]["item_definitions"][0]["structure_ref"]["namespace"],
        "urn:external"
    );
    first.catalog.item_definitions[0]
        .structure_ref
        .as_mut()
        .unwrap()
        .namespace = "urn:other".into();
    assert_ne!(projection, xml_model_value(&first).unwrap());
}

#[test]
fn duplicate_owner_and_unknown_native_target_fail_closed() {
    let mut duplicate = document(false);
    duplicate.processes[0].nodes[1].id = duplicate.processes[0].nodes[0].id.clone();
    assert!(
        xml_model_value(&duplicate)
            .unwrap_err()
            .contains("multiple serialized owners")
    );
    let mut unknown = document(false);
    unknown.processes[0].flows[0].source = ElementSelector::Native {
        native_id: processcraft_core::ids::NativeElementId::new(),
    };
    assert!(
        xml_model_value(&unknown)
            .unwrap_err()
            .contains("no typed identity owner")
    );
}

#[test]
fn legacy_definitions_require_authoritative_context_and_preserve_explicit_values() {
    use processcraft_core::model::{DefinitionsMetadata, GENERATED_TARGET_NAMESPACE, Provenance};
    use processcraft_io::interchange_compare::xml_model_value_for_export;
    let new = document(true);
    assert!(xml_model_value_for_export(&new, "urn:invented").is_err());
    let mut legacy = new.clone();
    legacy.provenance = Some(Provenance {
        source_resource_id: "Source".parse().unwrap(),
        source_name: "Never opened here".into(),
    });
    assert!(
        xml_model_value(&legacy)
            .unwrap_err()
            .contains("explicitly recovered")
    );
    let mut explicit = legacy.clone();
    explicit.definitions = Some(DefinitionsMetadata {
        target_namespace: "urn:recovered".into(),
        name: None,
        expression_language: None,
        type_language: None,
        exporter: None,
        exporter_version: None,
        imports: vec![],
        namespaces: vec![],
        foreign_attributes: vec![],
    });
    assert_eq!(
        xml_model_value_for_export(&legacy, "urn:recovered").unwrap(),
        xml_model_value(&explicit).unwrap()
    );
    assert!(xml_model_value_for_export(&explicit, "urn:wrong").is_err());
    explicit.definitions.as_mut().unwrap().exporter = Some(String::new());
    assert_ne!(
        xml_model_value_for_export(&legacy, "urn:recovered").unwrap(),
        xml_model_value(&explicit).unwrap()
    );
    assert_eq!(
        xml_model_value(&new).unwrap()["definitions"]["target_namespace"],
        GENERATED_TARGET_NAMESPACE
    );
    assert!(legacy.definitions.is_none());
}

#[test]
fn annotation_fallback_does_not_conflate_explicit_text_and_unrepresented_name() {
    use processcraft_core::{bpmn_model::NodeDetails, bpmn_xml::MixedXml, model::NodeKind};
    let mut legacy = document(true);
    legacy.processes[0].flows.clear();
    let node = &mut legacy.processes[0].nodes[0];
    node.kind = NodeKind::TextAnnotation;
    node.name = Some("Legacy body".into());
    node.details = None;
    let original = serde_json::to_value(&legacy).unwrap();
    let mut dedicated = legacy.clone();
    dedicated.processes[0].nodes[0].name = None;
    dedicated.processes[0].nodes[0].details = Some(NodeDetails::TextAnnotation {
        text_format: None,
        text: Some(MixedXml::Text("Legacy body".into())),
    });
    assert_eq!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&dedicated).unwrap()
    );
    dedicated.processes[0].nodes[0].name = Some("Additional unrepresented name".into());
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&dedicated).unwrap()
    );
    dedicated.processes[0].nodes[0].name = None;
    let Some(NodeDetails::TextAnnotation { text_format, .. }) =
        &mut dedicated.processes[0].nodes[0].details
    else {
        unreachable!()
    };
    *text_format = Some(String::new());
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&dedicated).unwrap()
    );
    assert_eq!(serde_json::to_value(&legacy).unwrap(), original);
}

#[test]
fn documentation_representation_keeps_empty_entry_and_explicit_metadata_distinct() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_xml::MixedXml,
    };
    let mut legacy = document(true);
    legacy.processes[0].nodes[0].documentation = "Body".into();
    let mut structured = legacy.clone();
    structured.processes[0].nodes[0].documentation =
        DocumentationSet::Structured(vec![Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Text("Body".into()),
        }]);
    assert_eq!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&structured).unwrap()
    );
    let DocumentationSet::Structured(docs) = &mut structured.processes[0].nodes[0].documentation
    else {
        unreachable!()
    };
    docs[0].text_format = Some("text/plain".into());
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&structured).unwrap()
    );
    legacy.processes[0].nodes[0].documentation = "".into();
    structured.processes[0].nodes[0].documentation = DocumentationSet::Structured(vec![]);
    assert_eq!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&structured).unwrap()
    );
    structured.processes[0].nodes[0].documentation =
        DocumentationSet::Structured(vec![Documentation {
            id: None,
            text_format: None,
            content: MixedXml::Text(String::new()),
        }]);
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&structured).unwrap()
    );
}

#[test]
fn native_authored_documentation_bodies_and_bare_expression_export_without_loss() {
    use processcraft_core::{
        bpmn_metadata::{Documentation, DocumentationSet},
        bpmn_model::{Expression, ExpressionData, ExpressionKind},
        bpmn_xml::{MixedXml, NamespaceBinding, XmlContent, XmlNode},
    };
    use processcraft_io::interchange::{export_bpmn, prepare_import};
    let doc = |text: &str| Documentation {
        id: None,
        text_format: None,
        content: text.into(),
    };
    let cases = [
        (DocumentationSet::Structured(vec![]), 0),
        (DocumentationSet::Structured(vec![doc("Hello & world")]), 1),
        (DocumentationSet::Structured(vec![doc("")]), 1),
        (
            DocumentationSet::Structured(vec![Documentation {
                id: None,
                text_format: None,
                content: MixedXml::Structured(XmlContent {
                    namespaces: vec![NamespaceBinding {
                        prefix: "".into(),
                        uri: "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
                    }],
                    nodes: vec![XmlNode::ProcessingInstruction {
                        target: "inspect".into(),
                        data: Some(String::new()),
                    }],
                }),
            }]),
            1,
        ),
        (
            DocumentationSet::Structured(vec![
                Documentation {
                    id: Some("Doc".parse().unwrap()),
                    text_format: Some("text/plain".into()),
                    content: MixedXml::Structured(XmlContent {
                        namespaces: vec![NamespaceBinding {
                            prefix: "".into(),
                            uri: "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
                        }],
                        nodes: vec![
                            XmlNode::Text {
                                text: "One & ".into(),
                            },
                            XmlNode::Text { text: "two".into() },
                            XmlNode::Comment {
                                text: "kept".into(),
                            },
                        ],
                    }),
                },
                doc(""),
            ]),
            2,
        ),
    ];
    for (documentation, expected_docs) in cases {
        let mut document = document(true);
        document.processes[0].nodes[0].documentation = documentation;
        document.processes[0].nodes[1].documentation = "".into();
        document.processes[0].flows[0].condition = Some(Expression::Structured(ExpressionData {
            id: None,
            kind: ExpressionKind::Base {
                explicit_type: false,
            },
            text: "amount > 0".into(),
            namespaces: vec![(
                "".into(),
                "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
            )],
            foreign_attributes: vec![],
        }));
        let id = document.id;
        let mut project = ProjectData::new("Native authoring".into());
        project.documents.push(Arc::new(document));
        project.validate().unwrap();
        let before = project.content_identity().unwrap();
        let exported = export_bpmn(&project, id, false).unwrap();
        let imported = prepare_import(&exported.bytes, "Again").unwrap();
        assert!(imported.report.editable);
        assert_eq!(
            xml_model_value(&project.documents[0]).unwrap(),
            xml_model_value(&imported.document).unwrap()
        );
        let mut reader = quick_xml::Reader::from_reader(exported.bytes.as_slice());
        let mut count = 0;
        loop {
            match reader.read_event().unwrap() {
                quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)
                    if e.local_name().as_ref() == "documentation" =>
                {
                    count += 1
                }
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(count, expected_docs);
        assert_eq!(project.content_identity().unwrap(), before);
    }
}

#[test]
fn anonymous_lane_wrapper_identity_keeps_presence_after_xml_id_normalization() {
    use processcraft_core::model::Lane;
    use processcraft_io::interchange::{export_bpmn, prepare_import};
    for child in [false, true] {
        let mut document = document(true);
        if child {
            document.processes[0].lanes.push(Lane {
                id: ElementIdentity::anonymous(),
                name: None,
                base: None,
                parent: None,
                members: vec![],
                child_lane_set_id: Some(ElementIdentity::anonymous()),
                child_lane_set_base: None,
            });
        } else {
            document.processes[0].lane_set_id = Some(ElementIdentity::anonymous());
        }
        let id = document.id;
        let mut project = ProjectData::new("Anonymous wrapper".into());
        project.documents.push(Arc::new(document));
        project.validate().unwrap();
        let expected = xml_model_value(&project.documents[0]).unwrap();
        let output = export_bpmn(&project, id, false).unwrap();
        let actual = prepare_import(&output.bytes, "Again").unwrap();
        assert!(actual.report.editable);
        assert_eq!(expected, xml_model_value(&actual.document).unwrap());
        let mut absent = (*project.documents[0]).clone();
        absent.processes[0].lanes.clear();
        absent.processes[0].lane_set_id = None;
        assert_ne!(expected, xml_model_value(&absent).unwrap());
    }
}

#[test]
fn legacy_di_visible_labels_never_compare_equal_to_imported_explicit_absence() {
    use processcraft_core::model::{DiLabelVisibility, Label};
    let input = include_bytes!("../../../tests/fixtures/bpmn/interop-process.bpmn");
    let imported = processcraft_io::interchange::prepare_import(input, "Labels").unwrap();
    let hidden = imported.document;
    let mut legacy = hidden.clone();
    legacy.diagrams[0].shapes[0].label_visibility = None;
    legacy.diagrams[0].shapes[0].label = None;
    let mut explicit_visible = hidden.clone();
    explicit_visible.diagrams[0].shapes[0].label_visibility = Some(DiLabelVisibility::Explicit);
    explicit_visible.diagrams[0].shapes[0].label = Some(Label::default());
    let before = serde_json::to_value(&legacy).unwrap();
    assert_eq!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&explicit_visible).unwrap()
    );
    assert_ne!(
        xml_model_value(&legacy).unwrap(),
        xml_model_value(&hidden).unwrap()
    );
    assert_eq!(serde_json::to_value(&legacy).unwrap(), before);
}
