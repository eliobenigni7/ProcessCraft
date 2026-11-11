use processcraft_io::bpmn::{ImportErrorKind, ImportMode, MAX_RAW_BYTES, import_bpmn};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/bpmn/{name}.bpmn",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn preserved_source_is_byte_identical() {
    for name in [
        "minimal",
        "extensions",
        "multiple-diagrams",
        "unsupported-event",
        "namespace-rebinding",
    ] {
        let bytes = fixture(name);
        let imported = import_bpmn(&bytes).unwrap();
        assert_eq!(imported.export_source(), bytes);
        assert_eq!(imported.report().mode, ImportMode::PreservedSource);
        assert!(!imported.report().semantic_edits_allowed);
    }
}

#[test]
fn qnames_keep_namespace_identity() {
    let imported = import_bpmn(&fixture("namespace-rebinding")).unwrap();
    let names = &imported.report().qname_values;
    assert!(
        names
            .iter()
            .any(|n| n.namespace_uri.as_deref() == Some("urn:vendor:first")
                && n.local_name == "Kind")
    );
    assert!(
        names
            .iter()
            .any(|n| n.namespace_uri.as_deref() == Some("urn:vendor:second")
                && n.local_name == "Kind")
    );
    assert!(
        names
            .iter()
            .any(|n| n.namespace_uri.as_deref() == Some("urn:vendor:first")
                && n.local_name == "After")
    );
}

#[test]
fn unsupported_content_never_becomes_editable() {
    for name in ["minimal", "extensions", "unsupported-event"] {
        let imported = import_bpmn(&fixture(name)).unwrap();
        assert!(!imported.report().semantic_edits_allowed);
        assert_eq!(imported.report().mode, ImportMode::PreservedSource);
        assert!(!imported.report().limitations.is_empty());
    }
}

#[test]
fn xml_limits_stop_before_commit() {
    for (bytes, kind) in [
        (fixture("malformed"), ImportErrorKind::Xml),
        (fixture("dtd"), ImportErrorKind::DtdForbidden),
        (vec![b' '; MAX_RAW_BYTES + 1], ImportErrorKind::RawBytes),
        (
            wrap(&format!(
                "{}{}",
                "<b:extensionElements>".repeat(128),
                "</b:extensionElements>".repeat(128)
            )),
            ImportErrorKind::Depth,
        ),
        (
            wrap(&format!("<b:task name='{}'/>", "a".repeat(16 * 1024 + 1))),
            ImportErrorKind::Label,
        ),
        (
            wrap(&format!(
                "<b:documentation>{}</b:documentation>",
                "a".repeat(1024 * 1024 + 1)
            )),
            ImportErrorKind::Documentation,
        ),
        (wrap(&"<b:task/>".repeat(10_000)), ImportErrorKind::Elements),
    ] {
        let old = import_bpmn(&fixture("minimal")).unwrap();
        assert_eq!(import_bpmn(&bytes).unwrap_err().kind, kind);
        assert_eq!(old.export_source(), fixture("minimal"));
    }
}

fn wrap(body: &str) -> Vec<u8> {
    format!("<b:definitions xmlns:b='http://www.omg.org/spec/BPMN/20100524/MODEL' xmlns:di='http://www.omg.org/spec/DD/20100524/DC'>{body}</b:definitions>").into_bytes()
}

#[test]
fn invalid_standard_structure_is_rejected() {
    for body in [
        "<b:task id='1bad'/>",
        "<b:task id='a'/><b:task id='a'/>",
        "<b:sequenceFlow id='f' sourceRef='missing' targetRef='missing'/>",
        "<di:Bounds x='NaN' y='0' width='1' height='2'/>",
        "<di:Bounds x='0' y='0' width='-1' height='2'/>",
        "<di:Bounds x='1000000001' y='0' width='1' height='2'/>",
        "<di:Bounds x='0' y='0' width='1'/>",
    ] {
        assert!(import_bpmn(&wrap(body)).is_err(), "accepted {body}");
    }
    for body in [
        "<b:task>&undefined;</b:task>",
        "<bad:task/>",
        "<b:task a='1' a='2'/>",
        "<b:task name='&#0;'/>",
    ] {
        assert!(import_bpmn(&wrap(body)).is_err(), "accepted {body}");
    }
    assert!(import_bpmn(b"<a/><a/>").is_err());
}

#[test]
fn vendor_attributes_are_not_standard_references_or_ids() {
    let bytes = wrap(
        "<b:extensionElements xmlns:v='urn:vendor'><v:opaque id='not an XML id' sourceRef='no such id'/><v:opaque id='not an XML id'/></b:extensionElements><b:task id='Task' xmlns:v='urn:vendor' v:id='not an id' v:sourceRef='not a ref'/>",
    );
    assert_eq!(import_bpmn(&bytes).unwrap().export_source(), bytes);
}

#[test]
fn references_use_namespace_identity_and_text_values() {
    let valid = wrap(
        "<b:process id='P'><b:task id='A'><b:outgoing>F</b:outgoing></b:task><b:task id='B'/><b:sequenceFlow id='F' sourceRef='A' targetRef='B'/></b:process>",
    );
    assert!(import_bpmn(&valid).is_ok());
    let wrong_namespace = wrap(
        "<b:task id='A'/><b:sequenceFlow id='F' xmlns:v='urn:unrelated' sourceRef='v:A' targetRef='A'/>",
    );
    assert_eq!(
        import_bpmn(&wrong_namespace).unwrap_err().kind,
        ImportErrorKind::Reference
    );
    let missing_text_ref = wrap("<b:task id='A'><b:incoming>Missing</b:incoming></b:task>");
    assert_eq!(
        import_bpmn(&missing_text_ref).unwrap_err().kind,
        ImportErrorKind::Reference
    );
}

#[test]
fn external_qname_targets_are_preserved_without_resolving_or_aliasing_local_ids() {
    let bytes = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL"
        xmlns:ext="urn:external" xmlns:local="urn:local" targetNamespace="urn:local" id="Defs">
        <import namespace="urn:external" location="file:///nonexistent/external.bpmn"
            importType="http://www.omg.org/spec/BPMN/20100524/MODEL"/>
        <process id="P"><startEvent id="Start"><messageEventDefinition messageRef="ext:Message"/></startEvent></process>
    </definitions>"#;
    let preserved = import_bpmn(bytes).unwrap();
    assert_eq!(preserved.export_source(), bytes);
    assert!(
        preserved
            .report()
            .limitations
            .iter()
            .any(|l| l.contains("External QName"))
    );
    let prepared = processcraft_io::interchange::prepare_import(bytes, "external").unwrap();
    assert!(!prepared.report.editable);
    let mut project = processcraft_core::model::ProjectData::new("External".into());
    let id = prepared.document.id;
    project
        .documents
        .push(std::sync::Arc::new(prepared.document));
    project.resources = prepared.resources;
    let output = processcraft_io::interchange::export_bpmn(&project, id, false).unwrap();
    assert_eq!(output.bytes, bytes);
    let local_missing = std::str::from_utf8(bytes)
        .unwrap()
        .replace("ext:Message", "local:Message");
    assert_eq!(
        import_bpmn(local_missing.as_bytes()).unwrap_err().kind,
        ImportErrorKind::Reference
    );
}

#[test]
fn link_definition_references_and_compensation_activity_use_expanded_qnames() {
    let bytes = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL"
        xmlns:local="urn:links" xmlns:external="urn:external" id="Defs" targetNamespace="urn:links">
        <process id="P"><task id="Work"/>
          <intermediateThrowEvent id="Throw"><linkEventDefinition id="From" name="Continue"><target>local:To</target></linkEventDefinition></intermediateThrowEvent>
          <intermediateCatchEvent id="Catch"><linkEventDefinition id="To" name="Continue"><source>local:From</source></linkEventDefinition></intermediateCatchEvent>
          <intermediateThrowEvent id="Compensate"><compensateEventDefinition activityRef="local:Work"/></intermediateThrowEvent>
        </process></definitions>"#;
    assert_eq!(import_bpmn(bytes).unwrap().export_source(), bytes);
    let source = std::str::from_utf8(bytes).unwrap();
    for changed in [
        source.replace("local:To", "local:Missing"),
        source.replace("local:Work", "local:Missing"),
    ] {
        assert_eq!(
            import_bpmn(changed.as_bytes()).unwrap_err().kind,
            ImportErrorKind::Reference
        );
    }
    let external = source.replace("local:To", "external:To");
    assert_eq!(
        import_bpmn(external.as_bytes()).unwrap().export_source(),
        external.as_bytes()
    );
}

#[test]
fn catalog_model_references_are_checked_separately_from_service_type_bindings() {
    let bytes = br#"<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL"
        xmlns:local="urn:catalog" xmlns:external="urn:external" id="Defs" targetNamespace="urn:catalog">
        <itemDefinition id="Item" structureRef="local:SchemaType"/>
        <message id="Input"/><message id="Output"/><error id="Fault"/>
        <interface name="Service" implementationRef="local:WsdlInterface"><operation name="Run" implementationRef="external:WsdlOperation">
            <inMessageRef>local:Input</inMessageRef><outMessageRef>local:Output</outMessageRef><errorRef>local:Fault</errorRef>
        </operation></interface>
    </definitions>"#;
    let parsed = import_bpmn(bytes).unwrap();
    assert!(
        parsed
            .report()
            .qname_values
            .iter()
            .any(|q| q.local_name == "SchemaType"
                && q.namespace_uri.as_deref() == Some("urn:catalog"))
    );
    assert!(
        parsed
            .report()
            .qname_values
            .iter()
            .any(|q| q.local_name == "WsdlInterface")
    );
    let input = std::str::from_utf8(bytes).unwrap();
    let missing = input.replace("local:Input", "local:Missing");
    assert_eq!(
        import_bpmn(missing.as_bytes()).unwrap_err().kind,
        ImportErrorKind::Reference
    );
    let external = input.replace("local:Input", "external:Input");
    assert_eq!(
        import_bpmn(external.as_bytes()).unwrap().export_source(),
        external.as_bytes()
    );
    let rebound = input.replace("local:SchemaType", "unbound:SchemaType");
    assert!(import_bpmn(rebound.as_bytes()).is_err());
    let no_namespace = br#"<b:definitions xmlns:b="http://www.omg.org/spec/BPMN/20100524/MODEL"
        id="D" targetNamespace="urn:catalog">
        <b:interface name="Service"><b:operation name="Run"><b:inMessageRef>ExternalMessage</b:inMessageRef></b:operation></b:interface>
    </b:definitions>"#;
    assert_eq!(
        import_bpmn(no_namespace).unwrap().export_source(),
        no_namespace
    );
    let typed = processcraft_io::interchange::prepare_import(no_namespace, "no namespace").unwrap();
    assert!(typed.report.editable);
    let operation = &typed.document.catalog.interfaces[0].operations[0];
    assert!(operation.in_message_ref.namespace.is_empty());
    assert_eq!(
        operation.in_message_ref.local.to_string(),
        "ExternalMessage"
    );
}

#[test]
fn malformed_namespaces_and_entities_are_refused() {
    for body in [
        "<b:task xmlns:v='urn:a' xmlns:w='urn:a' v:flag='1' w:flag='2'/>",
        "<b:task xmlns:xml='urn:wrong'/>",
        "<b:task xmlns:v='urn:a' v:bad:attribute='1'/>",
        "<b:task>&#xD800;</b:task>",
        "<b:task>bad ]]> text</b:task>",
        "<b:task><!-- invalid -- comment --></b:task>",
        "<b:task name='&unknown;'/>",
    ] {
        assert!(import_bpmn(&wrap(body)).is_err(), "accepted {body}");
    }
}

#[test]
fn strings_are_counted_across_event_boundaries() {
    let source = wrap(&format!(
        "<b:documentation>{}<![CDATA[a]]>&amp;</b:documentation>",
        "x".repeat(1024 * 1024 - 1)
    ));
    assert_eq!(
        import_bpmn(&source).unwrap_err().kind,
        ImportErrorKind::Documentation
    );
    let valid = wrap(&format!(
        "<b:documentation>{}</b:documentation>",
        "x".repeat(1024 * 1024)
    ));
    assert!(import_bpmn(&valid).is_ok());
}

#[test]
fn exact_limits_and_utf8_are_measured_in_bytes() {
    let nested = wrap(&format!(
        "{}{}",
        "<b:extensionElements>".repeat(127),
        "</b:extensionElements>".repeat(127)
    ));
    assert!(import_bpmn(&nested).is_ok());
    let exact_label = wrap(&format!(
        "<b:task id='éclair' name='{}'/>",
        "é".repeat(8 * 1024)
    ));
    assert!(import_bpmn(&exact_label).is_ok());
    let oversized_label = wrap(&format!("<b:task name='{}'/>", "é".repeat(8 * 1024 + 1)));
    assert_eq!(
        import_bpmn(&oversized_label).unwrap_err().kind,
        ImportErrorKind::Label
    );
    let exact_elements = wrap(&"<b:task/>".repeat(9_999));
    assert!(import_bpmn(&exact_elements).is_ok());
    let mut raw = fixture("minimal");
    raw.extend(b"<!--");
    raw.resize(MAX_RAW_BYTES - 3, b' ');
    raw.extend(b"-->");
    assert_eq!(raw.len(), MAX_RAW_BYTES);
    assert!(import_bpmn(&raw).is_ok());
}

#[test]
fn namespace_names_reject_whitespace_and_escaped_reserved_bindings() {
    for binding in [
        "xmlns:v='urn:vendor:with space'",
        "xmlns:v='urn:vendor:with&#x20;space'",
        "xmlns:v='urn:vendor:with\nnewline'",
        "xmlns:v='http://www.w3.org/XML/1998/nam&#101;space'",
        "xmlns='http://www.w3.org/XML/1998/nam&#101;space'",
        "xmlns:v='http://www.w3.org/2000/xmln&#115;/'",
        "xmlns='http://www.w3.org/2000/xmln&#115;/'",
    ] {
        let bytes = wrap(&format!("<b:extensionElements {binding}/>"));
        assert!(import_bpmn(&bytes).is_err(), "accepted {binding}");
    }
    let bytes = wrap(
        "<b:extensionElements xmlns:v='urn:vendor:fir&#115;t' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><v:item xsi:type='v:Kind'/></b:extensionElements>",
    );
    let imported = import_bpmn(&bytes).unwrap();
    assert!(
        imported
            .report()
            .qname_values
            .iter()
            .any(
                |name| name.namespace_uri.as_deref() == Some("urn:vendor:first")
                    && name.local_name == "Kind"
            )
    );
}
