use processcraft_core::bpmn_collaboration::{
    ChoreographyActivityDetails as Details, ChoreographyFlowElement as Element, ChoreographyGraph,
};
use processcraft_core::ids::ElementIdentity;
use processcraft_core::model::ProjectData;
use processcraft_io::bpmn::{
    ImportErrorKind, MAX_ATTRIBUTES_PER_ELEMENT, MAX_NAMESPACE_DECLARATIONS, import_bpmn,
};
use processcraft_io::interchange::{export_bpmn, prepare_import};
use std::sync::Arc;

const BPMN: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";

fn refused(xml: String) -> processcraft_io::bpmn::ImportError {
    match import_bpmn(xml.as_bytes()) {
        Err(e) => e,
        Ok(_) => panic!("hostile input was accepted"),
    }
}

#[test]
fn namespace_declaration_count_is_capped() {
    // The XML resolver already bounds in-scope bindings (128), so spread
    // declarations over sibling elements to exceed the per-document cap.
    let per = 100;
    let siblings: String = (0..=MAX_NAMESPACE_DECLARATIONS / per)
        .map(|s| {
            let decls: String = (0..per)
                .map(|i| format!(" xmlns:n{s}x{i}=\"urn:n{s}x{i}\""))
                .collect();
            format!("<x:e xmlns:x=\"urn:x\"{decls}/>")
        })
        .collect();
    let e = refused(format!(
        "<definitions xmlns=\"{BPMN}\" id=\"d\" targetNamespace=\"urn:t\">{siblings}</definitions>"
    ));
    assert_eq!(e.kind, ImportErrorKind::Metadata);
    assert!(e.to_string().contains("namespace declarations"), "{e}");
}

#[test]
fn attributes_per_element_are_capped() {
    let attrs: String = (0..=MAX_ATTRIBUTES_PER_ELEMENT)
        .map(|i| format!(" a{i}=\"v\""))
        .collect();
    let e = refused(format!(
        "<definitions xmlns=\"{BPMN}\" id=\"d\" targetNamespace=\"urn:t\"{attrs}/>"
    ));
    assert_eq!(e.kind, ImportErrorKind::Metadata);
    assert!(e.to_string().contains("attributes"), "{e}");
}

fn nested_subs(levels: usize) -> String {
    let mut inner = String::new();
    for i in 0..levels {
        inner = format!(
            "<subChoreography id=\"s{i}\" initiatingParticipantRef=\"t:buyer\"><participantRef>t:buyer</participantRef><participantRef>t:seller</participantRef>{inner}</subChoreography>"
        );
    }
    format!(
        "<definitions xmlns=\"{BPMN}\" xmlns:t=\"urn:g\" id=\"defs\" targetNamespace=\"urn:g\"><choreography id=\"main\"><participant id=\"buyer\"/><participant id=\"seller\"/>{inner}</choreography></definitions>"
    )
}

#[test]
fn choreography_depth_is_consistent_and_export_names_depth() {
    // 17 nested subs (the deepest at depth 16) is the shared maximum: editable and exportable.
    let parsed = prepare_import(nested_subs(17).as_bytes(), "limit").unwrap();
    assert!(parsed.report.editable, "{:?}", parsed.report);
    let mut project = ProjectData::new("p".into());
    project.documents.push(Arc::new(parsed.document.clone()));
    project.resources = parsed.resources.clone();
    export_bpmn(&project, parsed.document.id, false).unwrap();
    // One more level is refused by the parser (preserved source)...
    assert!(
        !prepare_import(nested_subs(19).as_bytes(), "deep")
            .unwrap()
            .report
            .editable
    );

    // ...and by the writer, with a depth-specific message.
    let mut doc = parsed.document.clone();
    let mut graph: ChoreographyGraph = doc.choreographies[0].graph.clone().unwrap();
    for _ in 0..2 {
        let Element::Activity(inner) = graph.flow_elements[0].clone() else {
            panic!("expected activity");
        };
        let mut outer = inner.clone();
        outer.id = ElementIdentity::anonymous();
        outer.details = Some(Details::Sub {
            graph: Box::new(graph),
            artifacts: vec![],
        });
        graph = ChoreographyGraph {
            flow_elements: vec![Element::Activity(outer)],
            ..Default::default()
        };
    }
    doc.choreographies[0].graph = Some(graph);
    project.documents[0] = Arc::new(doc.clone());
    let failure = export_bpmn(&project, doc.id, false).unwrap_err();
    assert!(
        failure.report.blocked.iter().any(|s| s.contains("depth")),
        "{:?}",
        failure.report.blocked
    );
}
